//! Keyboard, clock and speaker, with NO interrupt handlers of our own.
//!
//! The kernel stays inside UEFI boot services so it can read the disk while running (the
//! firmware's drivers do that). Those drivers rely on the firmware's own timer and interrupt
//! setup, so we must not replace the IDT or reprogram the PIC/PIT. Instead:
//!   * the clock is the CPU's cycle counter, calibrated once against the firmware's `stall`;
//!   * the keyboard is polled: we read the PS/2 controller each frame (and while waiting).
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::time::Duration;
use x86_64::instructions::port::Port;

/// Which keys are held down, indexed by PS/2 scancode.
pub static KEYS: [AtomicBool; 128] = [const { AtomicBool::new(false) }; 128];
/// Latched presses: set on key-down, cleared when a reader consumes it.
pub static PRESSED: [AtomicBool; 128] = [const { AtomicBool::new(false) }; 128];

static T0: AtomicU64 = AtomicU64::new(0);
static CYCLES_PER_MS: AtomicU64 = AtomicU64::new(1_000_000);

fn rdtsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Measure the cycle counter against 50 ms of firmware stall time.
pub fn init_clock() {
    let a = rdtsc();
    uefi::boot::stall(Duration::from_millis(50));
    let b = rdtsc();
    CYCLES_PER_MS.store(((b - a) / 50).max(1000), Ordering::Relaxed);
    T0.store(rdtsc(), Ordering::Relaxed);
}

/// Milliseconds since `init_clock`.
pub fn now_ms() -> u64 {
    rdtsc().wrapping_sub(T0.load(Ordering::Relaxed)) / CYCLES_PER_MS.load(Ordering::Relaxed)
}

/// Read every byte waiting in the PS/2 controller.
pub fn poll() {
    crate::sound::pump();
    unsafe {
        let mut status: Port<u8> = Port::new(0x64);
        let mut data: Port<u8> = Port::new(0x60);
        for _ in 0..32 {
            let s = status.read();
            if s & 1 == 0 {
                break; // nothing waiting
            }
            let code = data.read();
            if s & 0x20 != 0 || code == 0xE0 {
                continue; // a mouse byte, or the "extended key" prefix (the real code follows)
            }
            let pressed = code & 0x80 == 0;
            KEYS[(code & 0x7f) as usize].store(pressed, Ordering::Relaxed);
            if pressed {
                PRESSED[(code & 0x7f) as usize].store(true, Ordering::Relaxed);
            }
        }
    }
}

pub fn take_pressed(scancode: u8) -> bool {
    PRESSED[(scancode & 0x7f) as usize].swap(false, Ordering::Relaxed)
}

pub fn key_down(scancode: u8) -> bool {
    KEYS[(scancode & 0x7f) as usize].load(Ordering::Relaxed)
}

// PC speaker: PIT channel 2 makes the square wave, bits 0-1 of port 0x61 connect it to the speaker.
pub fn speaker_tone(freq: u32) {
    if crate::sound::ready() {
        crate::sound::tone(freq);
        return;
    }
    if freq < 20 || freq > 20_000 {
        speaker_off();
        return;
    }
    let div = (1_193_182 / freq).clamp(1, 65535) as u16;
    unsafe {
        let mut cmd: Port<u8> = Port::new(0x43);
        let mut ch2: Port<u8> = Port::new(0x42);
        cmd.write(0xB6);
        ch2.write((div & 0xff) as u8);
        ch2.write((div >> 8) as u8);
        let mut gate: Port<u8> = Port::new(0x61);
        let v = gate.read();
        gate.write(v | 0b11);
    }
}

pub fn speaker_off() {
    if crate::sound::ready() {
        crate::sound::off();
        return;
    }
    unsafe {
        let mut gate: Port<u8> = Port::new(0x61);
        let v = gate.read();
        gate.write(v & !0b11);
    }
}

/// Throw away anything the firmware left in the keyboard controller during boot, so the menu
/// does not start with phantom key presses.
pub fn flush() {
    for _ in 0..4 {
        poll();
    }
    for k in KEYS.iter().chain(PRESSED.iter()) {
        k.store(false, Ordering::Relaxed);
    }
}
