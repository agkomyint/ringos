//! The arcade: the gallery menu, and the `Host` that lets Ring games draw and read keys.
//!
//! Games think in a fixed 320x200 "virtual screen". We scale that by a whole number to fit the
//! real screen and centre it (black bars around). So a game written once looks right on any
//! monitor, and 1 virtual pixel is always a crisp square of real pixels.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering::Relaxed;
use core::time::Duration;
use ringvm::pak::{Entry, Pak};
use ringvm::{Host, Program, Vm};

use crate::gfx::{Screen, VH as SVH, VW as SVW};
use crate::disk::DiskReader;
use crate::input::{key_down, now_ms, poll, speaker_off, speaker_tone, take_pressed, PRESSED};
use crate::store;

const VW: i64 = SVW as i64;
const VH: i64 = SVH as i64;

/// Colour numbers wrap at 256 (see `gfx::Color`).
fn color(c: i64) -> u8 {
    c as u8
}

/// PS/2 scancodes per game button. Each button accepts several keys: left, right, up, down,
/// A (A, Z, Space), B (S, X, Shift), start (Enter).
const KEYMAP: [&[u8]; 7] = [&[0x4B], &[0x4D], &[0x48], &[0x50], &[0x1E, 0x2C, 0x39], &[0x1F, 0x2D, 0x2A], &[0x1C]];
const ESC: u8 = 0x01;

struct Arcade<'a> {
    s: &'a mut Screen,
    next: u64,
    start: u64,
    in_game: bool,
    esc_at: u64,
    tone_off: u64, // tick when the current beep ends (0 = none)
    game: String,  // name of the running game (keys its saved numbers)
    slots: [i64; 8],
    writes_left: u32, // NVRAM writes this game may still make (flash wears out)
    dirty: bool,      // a saved value is waiting to be written
}

impl<'a> Arcade<'a> {
    fn new(s: &'a mut Screen) -> Arcade<'a> {
        Arcade { s, next: 0, start: now_ms(), esc_at: 0, in_game: false, tone_off: 0, game: String::new(), slots: [0; 8], writes_left: 0, dirty: false }
    }

    /// Draw text with the font enlarged by `mult` (menu titles).
    fn text_big(&mut self, x: i64, y: i64, s: &str, c: i64, mult: usize) {
        if x < 0 || y < 0 || x >= VW || y >= VH {
            return;
        }
        self.s.text(x as usize, y as usize, s, color(c), mult);
    }
}

impl<'a> Arcade<'a> {
    /// Esc during a game: freeze it behind a small box. ENTER resumes, Esc leaves the game.
    /// Returns false when the player leaves (the VM then stops).
    fn pause(&mut self) -> bool {
        speaker_off();
        self.tone_off = 0;
        let t0 = now_ms();
        self.rect(60, 70, 200, 60, 1);
        self.rect(60, 70, 200, 1, 15);
        self.rect(60, 129, 200, 1, 15);
        self.text_big(124, 80, "PAUSED", 14, 2);
        self.text_big(76, 104, "ENTER resume", 15, 1);
        self.text_big(76, 114, "ESC   quit to menu", 7, 1);
        self.s.present();
        let resume = loop {
            poll();
            if take_pressed(0x1C) || take_pressed(0x39) {
                break true;
            }
            if take_pressed(ESC) {
                break false;
            }
            uefi::boot::stall(Duration::from_millis(2));
        };
        self.start += now_ms() - t0; // time() does not count the pause
        self.next = now_ms();
        for k in PRESSED.iter() {
            k.store(false, Relaxed);
        }
        resume
    }
}

impl<'a> Host for Arcade<'a> {
    fn clear(&mut self, c: i64) {
        self.s.clear(color(c));
    }
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: i64) {
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = (x.saturating_add(w).min(VW), y.saturating_add(h).min(VH));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        self.s.rect(x0 as usize, y0 as usize, (x1 - x0) as usize, (y1 - y0) as usize, color(c));
    }
    fn pixel(&mut self, x: i64, y: i64, c: i64) {
        self.rect(x, y, 1, 1, c);
    }
    fn text(&mut self, x: i64, y: i64, s: &[u8], c: i64) {
        if x < 0 || y < 0 || x >= VW || y >= VH {
            return;
        }
        self.s.text_chars(x as usize, y as usize, s.iter().map(|&b| b as char), color(c), 1);
    }
    fn num(&mut self, x: i64, y: i64, n: i64, c: i64) {
        // format into a small stack buffer: this runs many times per frame
        let mut buf = [0u8; 20];
        let mut i = buf.len();
        let mut v = n.unsigned_abs();
        loop {
            i -= 1;
            buf[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        if n < 0 {
            i -= 1;
            buf[i] = b'-';
        }
        self.text(x, y, &buf[i..], c);
    }
    fn key(&mut self, k: i64) -> bool {
        KEYMAP.get(k as usize).map_or(false, |keys| keys.iter().any(|&sc| key_down(sc) || PRESSED[sc as usize].load(Relaxed)))
    }
    fn frame(&mut self) -> bool {
        self.s.present();
        if self.in_game {
            // the game has had its look at this frame's input: forget the latched taps
            for &sc in KEYMAP.iter().flat_map(|k| k.iter()) {
                PRESSED[sc as usize].store(false, Relaxed);
            }
        }
        // wait for the 60 Hz frame boundary, reading the keyboard as we go (a tap is never missed)
        poll();
        while now_ms() < self.next {
            uefi::boot::stall(Duration::from_micros(300));
            poll();
        }
        self.next = now_ms() + 16;
        if self.tone_off != 0 && now_ms() >= self.tone_off {
            speaker_off();
            self.tone_off = 0;
        }
        if self.in_game && take_pressed(ESC) {
            return self.pause();
        }
        true
    }
    fn time(&mut self) -> i64 {
        now_ms().saturating_sub(self.start) as i64
    }
    fn width(&mut self) -> i64 {
        VW
    }
    fn height(&mut self) -> i64 {
        VH
    }
    fn should_quit(&mut self) -> bool {
        // a runaway game that never calls frame(): Esc held for a second stops it
        if !key_down(ESC) {
            self.esc_at = 0;
            return false;
        }
        if self.esc_at == 0 {
            self.esc_at = now_ms();
        }
        now_ms().saturating_sub(self.esc_at) > 1000
    }
    fn print(&mut self, _s: &str) {}
    fn beep(&mut self, freq: i64, ms: i64) {
        if freq <= 0 || ms <= 0 {
            speaker_off();
            self.tone_off = 0;
        } else {
            speaker_tone(freq.min(20_000) as u32);
            self.tone_off = now_ms() + ms.min(5000) as u64;
        }
    }
    fn sprite(&mut self, x: i64, y: i64, data: &[u8], w: i64, scale: i64) {
        self.s.sprite(x, y, data, w, scale);
    }
    fn tiles(&mut self, map: &[u8], map_w: i64, sheet: &[u8], tile: i64, sx: i64, sy: i64) {
        self.s.tiles(map, map_w, sheet, tile, sx, sy);
    }
    fn palette(&mut self, colour: i64, r: i64, g: i64, b: i64) {
        self.s.set_color(colour as usize, r, g, b);
    }
    fn shake(&mut self, n: i64) {
        let n = n.clamp(0, 16);
        let r = now_ms().wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let span = 2 * n as u64 + 1;
        self.s.shake(((r >> 20) % span) as i64 - n, ((r >> 40) % span) as i64 - n);
    }
    fn load(&mut self, slot: i64) -> i64 {
        self.slots[(slot & 7) as usize]
    }
    fn save(&mut self, slot: i64, value: i64) {
        let i = (slot & 7) as usize;
        if self.slots[i] == value {
            return; // nothing changed: do not touch the flash
        }
        self.slots[i] = value;
        if self.writes_left > 0 {
            self.writes_left -= 1;
            self.dirty = false;
            store::save(&self.game, &self.slots);
        } else {
            self.dirty = true; // over the cap: remember it, write once when the game ends
        }
    }
}

fn edge(_prev: &mut [bool; 128], sc: u8) -> bool {
    take_pressed(sc)
}

/// PS/2 scancode -> character, for typing a game name to search for.
const LETTERS: [(u8, char); 36] = [
    (0x1E, 'a'), (0x30, 'b'), (0x2E, 'c'), (0x20, 'd'), (0x12, 'e'), (0x21, 'f'), (0x22, 'g'), (0x23, 'h'), (0x17, 'i'),
    (0x24, 'j'), (0x25, 'k'), (0x26, 'l'), (0x32, 'm'), (0x31, 'n'), (0x18, 'o'), (0x19, 'p'), (0x10, 'q'), (0x13, 'r'),
    (0x1F, 's'), (0x14, 't'), (0x16, 'u'), (0x2F, 'v'), (0x11, 'w'), (0x2D, 'x'), (0x15, 'y'), (0x2C, 'z'), (0x02, '1'),
    (0x03, '2'), (0x04, '3'), (0x05, '4'), (0x06, '5'), (0x07, '6'), (0x08, '7'), (0x09, '8'), (0x0A, '9'), (0x0B, '0'),
];
const ROWS: u32 = 8;

/// Read the page of menu rows starting at `top` (at most ROWS entries, cached by the disk reader).
fn load_page(pak: &mut Pak<DiskReader>, top: u32) -> Vec<Entry> {
    (top..top.saturating_add(ROWS)).map_while(|i| pak.entry(i)).collect()
}

/// The gallery. Never returns: an arcade machine has no "off" except the power button.
///
/// The menu holds only `ROWS` entries in memory; the rest of the (possibly huge) game list stays
/// in the pak on disk. Keys: UP/DOWN move 1, PGUP/PGDN 8, LEFT/RIGHT 100, TAB random game,
/// letters/digits search by name, BACKSPACE edits the search, ENTER plays.
pub fn run(s: &mut Screen, mut pak: Option<Pak<DiskReader>>) -> ! {
    let mut a = Arcade::new(s);
    let mut prev = [false; 128];
    let total = pak.as_ref().map_or(0, |p| p.len());
    let last = total.saturating_sub(1) as i64;
    let mut sel = 0u32;
    let mut query = String::new();
    let mut page: Vec<Entry> = Vec::new();
    let mut top = 0u32;
    let mut page_top = u32::MAX;
    loop {
        poll();
        let mut d = 0i64;
        if edge(&mut prev, 0x48) { d -= 1; }
        if edge(&mut prev, 0x50) { d += 1; }
        if edge(&mut prev, 0x49) { d -= ROWS as i64; }
        if edge(&mut prev, 0x51) { d += ROWS as i64; }
        if edge(&mut prev, 0x4B) { d -= 100; }
        if edge(&mut prev, 0x4D) { d += 100; }
        sel = (sel as i64 + d).clamp(0, last) as u32;
        let go = edge(&mut prev, 0x1C) | edge(&mut prev, 0x39);
        if edge(&mut prev, 0x0F) && total > 0 {
            sel = ((now_ms().wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 20) % total as u64) as u32;
        }
        let mut typed = false;
        if edge(&mut prev, 0x0E) && query.pop().is_some() {
            typed = true;
        }
        if edge(&mut prev, 0x01) && !query.is_empty() {
            query.clear();
            typed = true;
        }
        for &(sc, ch) in LETTERS.iter() {
            if take_pressed(sc) && query.len() < ringvm::pak::NAME_MAX {
                query.push(ch);
                typed = true;
            }
        }
        if typed {
            if let Some(p) = pak.as_mut() {
                sel = p.lower_bound(&query).min(total.saturating_sub(1));
            }
        }
        // keep the selection inside the visible window of ROWS entries
        if sel < top {
            top = sel;
        } else if sel >= top + ROWS {
            top = sel + 1 - ROWS;
        }
        if let Some(p) = pak.as_mut() {
            if page_top != top {
                page = load_page(p, top);
                page_top = top;
            }
        }

        if go && total > 0 {
            let chosen = page.get((sel - page_top) as usize).cloned();
            if let (Some(p), Some(e)) = (pak.as_mut(), chosen) {
                // read just this game from disk, verify it, run it, then drop it again
                let loaded = p.read_game(&e).ok_or("could not read game").and_then(|b| Program::from_bytes(&b).map_err(|_| "bad program"));
                let outcome = match loaded {
                    Ok(program) => {
                        let seed = now_ms().wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
                        let mut vm = Vm::new(&program, seed);
                        a.next = 0;
                        for k in PRESSED.iter() {
                            k.store(false, Relaxed); // taps made in the menu must not leak into the game
                        }
                        a.game = String::from(e.name());
                        a.slots = store::load(&a.game);
                        a.writes_left = 16;
                        a.dirty = false;
                        a.start = now_ms();
                        a.in_game = true;
                        let result = vm.run(&mut a);
                        a.in_game = false;
                        if a.dirty {
                            store::save(&a.game, &a.slots);
                            a.dirty = false;
                        }
                        speaker_off();
                        a.s.reset_palette();
                        a.tone_off = 0;
                        result.map_err(|e| e.message())
                    }
                    Err(m) => Err(m),
                };
                if let Err(m) = outcome {
                    crash_screen(&mut a, e.name(), m);
                }
                // wait until Esc / Enter are released so the menu doesn't react to the same press
                while key_down(ESC) || key_down(0x1C) {
                    a.frame();
                }
                for k in PRESSED.iter() {
                    k.store(false, Relaxed);
                }
            }
            continue;
        }

        a.clear(1);
        a.text_big(56, 10, "RingOS ARCADE", 14, 2);
        a.text_big(268, 10, &if crate::sound::ready() { String::from("HDA") } else { format!("BEEP{}", crate::sound::STAGE.load(Relaxed)) }, 8, 1);
        a.text_big(268, 20, concat!("v", env!("CARGO_PKG_VERSION")), 8, 1);
        a.rect(8, 34, 304, 1, 9);
        if pak.is_none() {
            a.text_big(16, 60, "No games.pak on the disk", 12, 1);
        } else if total == 0 {
            a.text_big(16, 60, "games.pak is empty", 12, 1);
        }
        for (row, g) in page.iter().enumerate() {
            let y = 44 + row as i64 * 14;
            let size = g.size as usize;
            let here = page_top + row as u32 == sel;
            if here {
                a.rect(8, y - 3, 304, 13, 9);
                a.text_big(16, y, &format!("> {}", g.name()), 15, 1);
            } else {
                a.text_big(16, y, &format!("  {}", g.name()), 7, 1);
            }
            a.text_big(248, y, &format!("{}.{}K", size / 1024, size % 1024 * 10 / 1024), if here { 15 } else { 8 }, 1);
        }
        // read the number and release the lock BEFORE formatting: format! allocates, and the
        // heap lock is not re-entrant
        let used = crate::HEAP.lock().used();
        a.text_big(8, 160, &format!("find: {}_", query), 14, 1);
        a.text_big(8, 170, &format!("{} of {} games   RAM {}K", if total == 0 { 0 } else { sel + 1 }, total, used / 1024), 8, 1);
        a.rect(8, 180, 304, 1, 9);
        a.text_big(8, 187, "ENTER play  TAB random  type to find", 7, 1);
        a.frame();
    }
}

fn crash_screen(a: &mut Arcade, name: &str, msg: &str) {
    let mut prev = [false; 128];
    loop {
        a.clear(4);
        a.text_big(16, 20, "GAME STOPPED", 15, 2);
        a.text_big(16, 60, name, 14, 1);
        a.text_big(16, 80, msg, 15, 1);
        a.text_big(16, 170, "Press ENTER", 7, 1);
        a.frame();
        if edge(&mut prev, 0x1C) {
            return;
        }
    }
}
