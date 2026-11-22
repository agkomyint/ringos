//! The arcade: the gallery menu, and the `Host` that lets Ring games draw and read keys.
//!
//! Games think in a fixed 320x200 "virtual screen". We scale that by a whole number to fit the
//! real screen and centre it (black bars around). So a game written once looks right on any
//! monitor, and 1 virtual pixel is always a crisp square of real pixels.
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering::Relaxed;
use ringvm::{Host, Program, Vm};

use crate::gfx::{Screen, VH as SVH, VW as SVW};
use crate::interrupts::{key_down, speaker_off, speaker_tone, take_pressed, PRESSED, TICKS};
use crate::store;

pub struct Game {
    pub name: String,
    pub program: Program,
    pub size: usize, // compiled size in bytes
}

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
    tone_off: u64, // tick when the current beep ends (0 = none)
    game: String,  // name of the running game (keys its saved numbers)
    slots: [i64; 8],
}

impl<'a> Arcade<'a> {
    fn new(s: &'a mut Screen) -> Arcade<'a> {
        Arcade { s, next: 0, start: TICKS.load(Relaxed), in_game: false, tone_off: 0, game: String::new(), slots: [0; 8] }
    }

    /// Draw text with the font enlarged by `mult` (menu titles).
    fn text_big(&mut self, x: i64, y: i64, s: &str, c: i64, mult: usize) {
        if x < 0 || y < 0 || x >= VW || y >= VH {
            return;
        }
        self.s.text(x as usize, y as usize, s, color(c), mult);
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
        // sleep until the 60 Hz frame boundary; the CPU idles in `hlt` instead of spinning
        while TICKS.load(Relaxed) < self.next {
            x86_64::instructions::hlt();
        }
        self.next = TICKS.load(Relaxed) + 16;
        if self.tone_off != 0 && TICKS.load(Relaxed) >= self.tone_off {
            speaker_off();
            self.tone_off = 0;
        }
        !key_down(ESC)
    }
    fn time(&mut self) -> i64 {
        (TICKS.load(Relaxed) - self.start) as i64
    }
    fn width(&mut self) -> i64 {
        VW
    }
    fn height(&mut self) -> i64 {
        VH
    }
    fn should_quit(&mut self) -> bool {
        key_down(ESC)
    }
    fn print(&mut self, _s: &str) {}
    fn beep(&mut self, freq: i64, ms: i64) {
        if freq <= 0 || ms <= 0 {
            speaker_off();
            self.tone_off = 0;
        } else {
            speaker_tone(freq.min(20_000) as u32);
            self.tone_off = TICKS.load(Relaxed) + ms.min(5000) as u64;
        }
    }
    fn load(&mut self, slot: i64) -> i64 {
        self.slots[(slot & 7) as usize]
    }
    fn save(&mut self, slot: i64, value: i64) {
        self.slots[(slot & 7) as usize] = value;
        store::save(&self.game, &self.slots);
    }
}

fn edge(_prev: &mut [bool; 128], sc: u8) -> bool {
    take_pressed(sc)
}

/// The gallery. Never returns: an arcade machine has no "off" except the power button.
pub fn run(s: &mut Screen, games: Vec<Game>) -> ! {
    let mut a = Arcade::new(s);
    let mut prev = [false; 128];
    let mut sel = 0usize;
    const ROWS: usize = 8;
    loop {
        let up = edge(&mut prev, 0x48);
        let down = edge(&mut prev, 0x50);
        let go = edge(&mut prev, 0x1C) | edge(&mut prev, 0x1E) | edge(&mut prev, 0x2C) | edge(&mut prev, 0x39);
        if up && sel > 0 {
            sel -= 1;
        }
        if down && sel + 1 < games.len() {
            sel += 1;
        }

        if go && !games.is_empty() {
            let seed = TICKS.load(Relaxed).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut vm = Vm::new(&games[sel].program, seed);
            a.next = 0;
            for k in PRESSED.iter() {
                k.store(false, Relaxed); // taps made in the menu must not leak into the game
            }
            a.game = games[sel].name.clone();
            a.slots = store::load(&a.game);
            a.in_game = true;
            let result = vm.run(&mut a);
            a.in_game = false;
            speaker_off();
            a.tone_off = 0;
            match result {
                Ok(_) => {}
                Err(e) => crash_screen(&mut a, &games[sel].name, e.message()),
            }
            // wait until Esc / Enter are released so the menu doesn't react to the same press
            while key_down(ESC) || key_down(0x1C) {
                a.frame();
            }
            continue;
        }

        a.clear(1);
        a.text_big(56, 10, "RingOS ARCADE", 14, 2);
        a.rect(8, 34, 304, 1, 9);
        if games.is_empty() {
            a.text_big(16, 60, "No games found in \\games", 12, 1);
        }
        let first = sel.saturating_sub(ROWS - 1);
        for (row, g) in games.iter().enumerate().skip(first).take(ROWS) {
            let y = 44 + (row - first) as i64 * 16;
            if row == sel {
                a.rect(8, y - 3, 304, 14, 9);
                a.text_big(16, y, &format!("> {}", g.name), 15, 1);
            } else {
                a.text_big(16, y, &format!("  {}", g.name), 7, 1);
            }
            a.text_big(248, y, &format!("{}.{}K", g.size / 1024, g.size % 1024 * 10 / 1024), if row == sel { 15 } else { 8 }, 1);
        }
        // read the numbers and release the lock BEFORE formatting: format! allocates, and the
        // heap lock is not re-entrant
        let (used, total) = {
            let heap = crate::HEAP.lock();
            (heap.used(), heap.size())
        };
        a.text_big(8, 166, &format!("RAM {}K of {}K used  {} games", used / 1024, total / 1024, games.len()), 8, 1);
        a.rect(8, 178, 304, 1, 9);
        a.text_big(8, 184, "UP/DOWN pick  ENTER play  ESC=exit", 7, 1);
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
