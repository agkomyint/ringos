//! Sound through an Intel High Definition Audio controller (laptops, desktops, VirtualBox's
//! default), with no interrupts: a small ring buffer is kept topped up by `pump()`, which the
//! keyboard poll calls many times per frame. If no controller or codec output path is found,
//! `ready()` stays false and the caller falls back to the PC speaker.
//!
//! UEFI maps all memory 1:1, so a buffer's address is also its physical (DMA) address.
use core::ptr::{read_volatile, write_volatile};
use core::time::Duration;
use uefi::boot::{self, AllocateType, MemoryType};
use x86_64::instructions::port::Port;

const RATE: u64 = 48_000;
const RING: usize = 16384; // bytes: 4096 stereo 16-bit frames, about 85 ms
const LEAD: usize = 8192; // keep this much audio queued ahead of the hardware (about 43 ms)
const BEHIND: usize = 14336; // if our write position is further "ahead" than this, we are really behind
const AMPLITUDE: i16 = 5000;

struct Hda {
    mmio: usize,
    sd: usize, // offset of the output stream descriptor
    ring: *mut u8,
    w: usize,
    phase: u32,
    step: u32, // 0 = silence
    corb: *mut u32,
    rirb: *mut u32,
    wp: core::cell::Cell<usize>,
    rp: core::cell::Cell<usize>,
    use_rings: bool,
}

static mut HDA: Option<Hda> = None;
/// Why the last bring-up attempt gave up (0 = none tried), shown in the gallery for diagnosis.
pub static STAGE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(1);
fn fail<T>(n: u8) -> Option<T> {
    STAGE.store(n, core::sync::atomic::Ordering::Relaxed);
    None
}

#[allow(static_mut_refs)]
fn state() -> Option<&'static mut Hda> {
    unsafe { HDA.as_mut() }
}

pub fn ready() -> bool {
    state().is_some()
}

fn us(n: u64) {
    boot::stall(Duration::from_micros(n));
}

// ---- PCI (legacy config ports) ----
fn pci_read(bus: u32, dev: u32, func: u32, off: u32) -> u32 {
    unsafe {
        let mut a: Port<u32> = Port::new(0xCF8);
        let mut d: Port<u32> = Port::new(0xCFC);
        a.write(0x8000_0000 | bus << 16 | dev << 11 | func << 8 | (off & 0xFC));
        d.read()
    }
}
fn pci_write(bus: u32, dev: u32, func: u32, off: u32, v: u32) {
    unsafe {
        let mut a: Port<u32> = Port::new(0xCF8);
        let mut d: Port<u32> = Port::new(0xCFC);
        a.write(0x8000_0000 | bus << 16 | dev << 11 | func << 8 | (off & 0xFC));
        d.write(v);
    }
}

/// Every PCI function whose class is "multimedia / HD audio" (0x0403xx).
fn find_controllers() -> [Option<(u32, u32, u32)>; 4] {
    let mut out = [None; 4];
    let mut n = 0;
    for bus in 0..=255u32 {
        for dev in 0..32u32 {
            if pci_read(bus, dev, 0, 0) & 0xFFFF == 0xFFFF {
                continue;
            }
            let multi = pci_read(bus, dev, 0, 0x0C) >> 16 & 0x80 != 0;
            for func in 0..if multi { 8 } else { 1 } {
                if pci_read(bus, dev, func, 0) & 0xFFFF == 0xFFFF {
                    continue;
                }
                if pci_read(bus, dev, func, 0x08) >> 16 == 0x0403 && n < out.len() {
                    out[n] = Some((bus, dev, func));
                    n += 1;
                }
            }
        }
    }
    out
}

// ---- controller registers ----
impl Hda {
    fn r8(&self, o: usize) -> u8 {
        unsafe { read_volatile((self.mmio + o) as *const u8) }
    }
    fn r16(&self, o: usize) -> u16 {
        unsafe { read_volatile((self.mmio + o) as *const u16) }
    }
    fn r32(&self, o: usize) -> u32 {
        unsafe { read_volatile((self.mmio + o) as *const u32) }
    }
    fn w8(&self, o: usize, v: u8) {
        unsafe { write_volatile((self.mmio + o) as *mut u8, v) }
    }
    fn w16(&self, o: usize, v: u16) {
        unsafe { write_volatile((self.mmio + o) as *mut u16, v) }
    }
    fn w32(&self, o: usize, v: u32) {
        unsafe { write_volatile((self.mmio + o) as *mut u32, v) }
    }

    /// Send one codec command and wait for the answer: through the CORB/RIRB rings when they are
    /// running, else through the Immediate Command registers.
    fn cmd(&self, codec: u32, nid: u32, verb: u32) -> Option<u32> {
        let word = codec << 28 | nid << 20 | verb;
        if self.use_rings {
            let wp = (self.wp.get() + 1) & 0xFF;
            unsafe { write_volatile(self.corb.add(wp), word) };
            self.wp.set(wp);
            self.w16(0x48, wp as u16);
            let want = (self.rp.get() + 1) & 0xFF;
            for _ in 0..500 {
                if self.r16(0x58) & 0xFF == want as u16 {
                    self.rp.set(want);
                    return Some(unsafe { read_volatile(self.rirb.add(want * 2)) });
                }
                us(10);
            }
            return None;
        }
        for _ in 0..200 {
            if self.r16(0x68) & 1 == 0 {
                break;
            }
            us(10);
        }
        self.w16(0x68, 0x2); // clear "result valid"
        self.w32(0x60, word);
        self.w16(0x68, 0x1); // go
        for _ in 0..400 {
            let s = self.r16(0x68);
            if s & 1 == 0 && s & 2 != 0 {
                return Some(self.r32(0x64));
            }
            us(10);
        }
        None
    }
    /// A "get parameter" style command: 12-bit verb, 8-bit payload.
    fn get(&self, c: u32, nid: u32, verb: u32, payload: u32) -> u32 {
        self.cmd(c, nid, verb << 8 | payload).unwrap_or(0)
    }
    fn set(&self, c: u32, nid: u32, verb: u32, payload: u32) {
        let _ = self.cmd(c, nid, verb << 8 | payload);
    }
    /// 4-bit verb, 16-bit payload (amplifier gain, stream format).
    fn set16(&self, c: u32, nid: u32, verb: u32, payload: u32) {
        let _ = self.cmd(c, nid, verb << 16 | payload);
    }
}

fn node_type(h: &Hda, c: u32, nid: u32) -> u32 {
    h.get(c, nid, 0xF00, 0x09) >> 20 & 0xF
}

/// Connection list of a widget (short and long forms), at most 16 entries.
fn connections(h: &Hda, c: u32, nid: u32, out: &mut [u32; 16]) -> usize {
    let len = h.get(c, nid, 0xF00, 0x0E);
    let long = len & 0x80 != 0;
    let n = ((len & 0x7F) as usize).min(16);
    let per = if long { 2 } else { 4 };
    let mut i = 0;
    while i < n {
        let r = h.get(c, nid, 0xF02, i as u32);
        for k in 0..per {
            if i + k >= n {
                break;
            }
            out[i + k] = if long { r >> (16 * k) & 0x7FFF } else { r >> (8 * k) & 0x7F };
        }
        i += per;
    }
    n
}

/// Depth-first search from `nid` towards a DAC. Fills `path` with (node, input index) pairs
/// from the pin side to the DAC side; returns the DAC's node number.
fn find_dac(h: &Hda, c: u32, nid: u32, depth: usize, path: &mut [(u32, u32); 6], used: &mut usize) -> Option<u32> {
    let mut list = [0u32; 16];
    let n = connections(h, c, nid, &mut list);
    for i in 0..n {
        let next = list[i];
        match node_type(h, c, next) {
            0 => {
                path[*used] = (nid, i as u32);
                *used += 1;
                return Some(next);
            }
            2 | 3 if depth < 4 => {
                let before = *used;
                path[*used] = (nid, i as u32);
                *used += 1;
                if let Some(d) = find_dac(h, c, next, depth + 1, path, used) {
                    return Some(d);
                }
                *used = before;
            }
            _ => {}
        }
    }
    None
}

/// Unmute and set unity gain on the output (and input) amplifiers of a widget.
fn unmute(h: &Hda, c: u32, afg: u32, nid: u32, input: Option<u32>) {
    let caps = h.get(c, nid, 0xF00, 0x09);
    let amp = |param: u32| -> u32 {
        let own = if caps & 8 != 0 { h.get(c, nid, 0xF00, param) } else { 0 };
        let r = if own != 0 { own } else { h.get(c, afg, 0xF00, param) };
        r & 0x7F // the 0 dB step
    };
    if caps & 4 != 0 {
        h.set16(c, nid, 0x3, 0xB000 | amp(0x12));
    }
    if caps & 2 != 0 {
        if let Some(i) = input {
            h.set16(c, nid, 0x3, 0x7000 | (i & 0xF) << 8 | amp(0x0D));
        }
    }
}

/// Find the first working output route on one controller and start streaming silence.
fn bring_up(bus: u32, dev: u32, func: u32) -> Option<Hda> {
    let bar0 = pci_read(bus, dev, func, 0x10);
    if bar0 & 1 != 0 {
        return fail(10); // an I/O BAR: not an HD Audio memory window
    }
    let mut base = (bar0 & !0xF) as u64;
    if bar0 >> 1 & 3 == 2 {
        base |= (pci_read(bus, dev, func, 0x14) as u64) << 32;
    }
    if base == 0 || base >= 1 << 40 {
        return fail(11);
    }
    let cmd = pci_read(bus, dev, func, 0x04);
    pci_write(bus, dev, func, 0x04, cmd | 0x6); // memory space + bus master

    // buffer descriptor list (1 page) + the ring, below 4 GB so any controller can reach them
    let pages = 2 + RING / 4096;
    let mem = boot::allocate_pages(AllocateType::MaxAddress(0xFFFF_F000), MemoryType::LOADER_DATA, pages).ok().or_else(|| fail(30))?;
    let bdl = mem.as_ptr();
    unsafe { core::ptr::write_bytes(bdl, 0, pages * 4096) };
    let ring = unsafe { bdl.add(8192) };
    let (corb, rirb) = unsafe { (bdl.add(1024) as *mut u32, bdl.add(4096) as *mut u32) };

    let mut h = Hda { mmio: base as usize, sd: 0, ring, w: 0, phase: 0, step: 0, corb, rirb, wp: core::cell::Cell::new(0), rp: core::cell::Cell::new(0), use_rings: false };

    // reset the controller and wait for codecs to announce themselves
    h.w32(0x08, 0);
    for _ in 0..100 {
        if h.r32(0x08) & 1 == 0 {
            break;
        }
        us(20);
    }
    us(200);
    h.w32(0x08, 1);
    let mut up = false;
    for _ in 0..100 {
        if h.r32(0x08) & 1 != 0 {
            up = true;
            break;
        }
        us(20);
    }
    if !up {
        return fail(12);
    }
    boot::stall(Duration::from_millis(3));
    let present = h.r16(0x0E) & 0x7FFF;
    if present == 0 {
        return fail(13);
    }
    let c = present.trailing_zeros();

    // command rings: 256 entries each (CORB 1 KB, RIRB 2 KB)
    h.w8(0x4C, 0);
    h.w8(0x5C, 0);
    h.w8(0x4E, 2);
    h.w8(0x5E, 2);
    h.w32(0x40, corb as u32);
    h.w32(0x44, 0);
    h.w32(0x50, rirb as u32);
    h.w32(0x54, 0);
    h.w16(0x4A, 0x8000); // reset the CORB read pointer
    for _ in 0..100 {
        if h.r16(0x4A) & 0x8000 != 0 {
            break;
        }
        us(20);
    }
    h.w16(0x4A, 0);
    for _ in 0..100 {
        if h.r16(0x4A) & 0x8000 == 0 {
            break;
        }
        us(20);
    }
    h.w16(0x48, 0);
    h.w16(0x58, 0x8000); // reset the RIRB write pointer
    h.w16(0x5A, 1);
    h.w8(0x4C, 2);
    h.w8(0x5C, 2);
    h.use_rings = true;
    if h.cmd(c, 0, 0xF00 << 8 | 0x04).is_none() {
        h.use_rings = false; // the rings did not answer: fall back to Immediate Command
        h.w8(0x4C, 0);
        h.w8(0x5C, 0);
    }

    // the audio function group
    let root = h.get(c, 0, 0xF00, 0x04);
    let (fstart, fcount) = (root >> 16 & 0xFF, root & 0xFF);
    let mut afg = None;
    for f in fstart..fstart + fcount {
        if h.get(c, f, 0xF00, 0x05) & 0xFF == 1 {
            afg = Some(f);
            break;
        }
    }
    let Some(afg) = afg else { return fail(31) };
    h.set(c, afg, 0x705, 0); // power up

    // output stream descriptors come after the input ones
    let gcap = h.r16(0x00) as usize;
    let iss = gcap >> 8 & 0xF;
    let oss = gcap >> 12 & 0xF;
    if oss == 0 {
        return fail(14);
    }
    h.sd = 0x80 + 0x20 * iss;

    // route every real output pin (speaker, line out, headphones) to a DAC, all on stream 1
    let widgets = h.get(c, afg, 0xF00, 0x04);
    STAGE.store(40, core::sync::atomic::Ordering::Relaxed);
    let (wstart, wcount) = (widgets >> 16 & 0xFF, widgets & 0xFF);
    let mut routed = 0;
    let mut dacs = [0u32; 8];
    let mut ndacs = 0;
    for pin in wstart..wstart + wcount {
        if node_type(&h, c, pin) != 4 {
            continue;
        }
        let pincaps = h.get(c, pin, 0xF00, 0x0C);
        let config = h.get(c, pin, 0xF1C, 0);
        let device = config >> 20 & 0xF;
        let unplugged = config >> 30 == 1;
        if pincaps & 0x10 == 0 || unplugged || device > 2 {
            continue; // not an output, not wired, or not speaker/line/headphone
        }
        let mut path = [(0u32, 0u32); 6];
        let mut used = 0;
        let Some(dac) = find_dac(&h, c, pin, 0, &mut path, &mut used) else { continue };
        for k in 0..used {
            let (node, sel) = path[k];
            h.set(c, node, 0x701, sel); // connection select
            h.set(c, node, 0x705, 0); // power up
            unmute(&h, c, afg, node, Some(sel));
            if k > 0 {
                continue;
            }
            h.set(c, node, 0x707, 0x40); // pin: output enabled
            h.set(c, node, 0x70C, 0x02); // external amplifier on
        }
        h.set(c, dac, 0x705, 0);
        unmute(&h, c, afg, dac, None);
        if !dacs[..ndacs].contains(&dac) && ndacs < dacs.len() {
            dacs[ndacs] = dac;
            ndacs += 1;
        }
        routed += 1;
    }
    if routed == 0 {
        return fail(15);
    }
    for &dac in &dacs[..ndacs] {
        h.set16(c, dac, 0x2, 0x0011); // 48 kHz, 16 bit, 2 channels
        h.set(c, dac, 0x706, 0x10); // stream 1, channel 0
    }

    // stream descriptor: reset, buffer list, format, then run
    let sd = h.sd;
    h.w8(sd, 1);
    for _ in 0..100 {
        if h.r8(sd) & 1 != 0 {
            break;
        }
        us(20);
    }
    h.w8(sd, 0);
    for _ in 0..100 {
        if h.r8(sd) & 1 == 0 {
            break;
        }
        us(20);
    }
    unsafe {
        let e = bdl as *mut u32;
        for i in 0..2usize {
            let addr = ring as u64 + (i * RING / 2) as u64;
            write_volatile(e.add(i * 4), addr as u32);
            write_volatile(e.add(i * 4 + 1), (addr >> 32) as u32);
            write_volatile(e.add(i * 4 + 2), (RING / 2) as u32);
            write_volatile(e.add(i * 4 + 3), 0);
        }
    }
    h.w32(sd + 0x08, RING as u32);
    h.w16(sd + 0x0C, 1);
    h.w16(sd + 0x12, 0x0011);
    h.w32(sd + 0x18, bdl as u32);
    h.w32(sd + 0x1C, 0);
    h.w32(sd, 1 << 20 | 0x02); // stream tag 1, RUN
    h.w = LEAD;
    Some(h)
}

/// Look for an HD Audio controller with a usable output; leaves `ready()` false if none.
pub fn init() {
    for slot in find_controllers().into_iter().flatten() {
        if let Some(h) = bring_up(slot.0, slot.1, slot.2) {
            STAGE.store(0, core::sync::atomic::Ordering::Relaxed);
            unsafe { HDA = Some(h) };
            return;
        }
    }
}

/// Play a square wave of `freq` Hz until `off()`.
pub fn tone(freq: u32) {
    if let Some(h) = state() {
        h.step = if (20..=20_000).contains(&freq) { ((freq as u64) << 32) / RATE } else { 0 } as u32;
        pump();
    }
}

pub fn off() {
    if let Some(h) = state() {
        h.step = 0;
    }
}

/// Top the ring buffer up to `LEAD` bytes ahead of the hardware's read position.
pub fn pump() {
    let Some(h) = state() else { return };
    let lpib = h.r32(h.sd + 0x04) as usize % RING;
    let mut ahead = (h.w + RING - lpib) % RING;
    if ahead > BEHIND {
        h.w = (lpib + LEAD) % RING; // we fell behind: skip forward rather than replay old audio
        ahead = LEAD;
    }
    while ahead < LEAD {
        let s: i16 = if h.step == 0 {
            0
        } else {
            h.phase = h.phase.wrapping_add(h.step);
            if h.phase >> 31 == 0 { AMPLITUDE } else { -AMPLITUDE }
        };
        if h.step == 0 {
            h.phase = 0;
        }
        unsafe {
            let p = h.ring.add(h.w) as *mut i16;
            write_volatile(p, s);
            write_volatile(p.add(1), s);
        }
        h.w = (h.w + 4) % RING;
        ahead += 4;
    }
}
