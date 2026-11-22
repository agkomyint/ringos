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

use crate::gfx::Screen;
use crate::interrupts::{key_down, take_pressed, PRESSED, TICKS};

pub struct Game {
    pub name: String,
    pub program: Program,
}

const VW: i64 = 320;
const VH: i64 = 200;

/// The classic 16-colour PC palette. Numbers above 15 are used as raw 0xRRGGBB.
const PALETTE: [u32; 16] = [
    0x000000, 0x0000aa, 0x00aa00, 0x00aaaa, 0xaa0000, 0xaa00aa, 0xaa5500, 0xaaaaaa, 0x555555, 0x5555ff, 0x55ff55,
    0x55ffff, 0xff5555, 0xff55ff, 0xffff55, 0xffffff,
];

fn color(c: i64) -> u32 {
    if (0..16).contains(&c) {
        PALETTE[c as usize]
    } else {
        (c as u32) & 0xffffff
    }
}

/// PS/2 scancodes per game button. Each button accepts several keys: left, right, up, down,
/// A (A, Z, Space), B (S, X, Shift), start (Enter).
const KEYMAP: [&[u8]; 7] = [&[0x4B], &[0x4D], &[0x48], &[0x50], &[0x1E, 0x2C, 0x39], &[0x1F, 0x2D, 0x2A], &[0x1C]];
const ESC: u8 = 0x01;

struct Arcade<'a> {
    s: &'a mut Screen,
    scale: usize,
    ox: usize,
    oy: usize,
    next: u64,
    start: u64,
    in_game: bool,
}

impl<'a> Arcade<'a> {
    fn new(s: &'a mut Screen) -> Arcade<'a> {
        let scale = (s.width / VW as usize).min(s.height / VH as usize).max(1);
        let ox = s.width.saturating_sub(VW as usize * scale) / 2;
        let oy = s.height.saturating_sub(VH as usize * scale) / 2;
        Arcade { s, scale, ox, oy, next: 0, start: TICKS.load(Relaxed), in_game: false }
    }

    /// Draw text with the font enlarged by `mult` (menu titles).
    fn text_big(&mut self, x: i64, y: i64, s: &str, c: i64, mult: usize) {
        if x < 0 || y < 0 || x >= VW || y >= VH {
            return;
        }
        let sc = self.scale * mult;
        self.s.text(self.ox + x as usize * self.scale, self.oy + y as usize * self.scale, s, color(c), sc);
    }
}

impl<'a> Host for Arcade<'a> {
    fn clear(&mut self, c: i64) {
        self.s.clear(0); // black bars
        self.s.rect(self.ox, self.oy, VW as usize * self.scale, VH as usize * self.scale, color(c));
    }
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: i64) {
        let (x0, y0) = (x.max(0), y.max(0));
        let (x1, y1) = (x.saturating_add(w).min(VW), y.saturating_add(h).min(VH));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let sc = self.scale;
        self.s.rect(self.ox + x0 as usize * sc, self.oy + y0 as usize * sc, (x1 - x0) as usize * sc, (y1 - y0) as usize * sc, color(c));
    }
    fn pixel(&mut self, x: i64, y: i64, c: i64) {
        self.rect(x, y, 1, 1, c);
    }
    fn text(&mut self, x: i64, y: i64, s: &[u8], c: i64) {
        let st: String = s.iter().map(|&b| b as char).collect();
        self.text_big(x, y, &st, c, 1);
    }
    fn num(&mut self, x: i64, y: i64, n: i64, c: i64) {
        self.text_big(x, y, &format!("{}", n), c, 1);
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
            a.in_game = true;
            let result = vm.run(&mut a);
            a.in_game = false;
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
        }
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
