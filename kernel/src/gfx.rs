//! Graphics. Games draw on a fixed 320x200 canvas (64 KB: one byte per pixel, a colour number
//! 0-255 looked up in a palette when presenting), and `present()` scales it by a whole
//! number straight into video memory, centred. There is no full-resolution back buffer: that
//! used to cost 3 MB at 1024x768 and 33 MB at 4K, for no benefit.
extern crate alloc;
use alloc::vec::Vec;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

pub const VW: usize = 320;
pub const VH: usize = 200;

pub struct Screen {
    front: *mut u32, // video memory (what the monitor reads)
    canvas: Vec<u8>,  // VW*VH colour numbers
    pal: [u32; 256],  // colour number -> pixel value in the monitor's byte order
    row: Vec<u32>,   // one scaled row, reused by present()
    width: usize,
    height: usize,
    stride: usize, // pixels per row in VIDEO memory (may exceed width)
    scale: usize,
    ox: usize,
    oy: usize,
    bgr: bool,
    shake: (i64, i64), // one-shot jolt applied by the next present()
}

/// A colour number 0-255: 0-15 the classic PC colours, 16-231 a 6x6x6 colour cube, 232-255 greys.
pub type Color = u8;

fn rgb(c: usize) -> u32 {
    const EGA: [u32; 16] = [
        0x000000, 0x0000aa, 0x00aa00, 0x00aaaa, 0xaa0000, 0xaa00aa, 0xaa5500, 0xaaaaaa, 0x555555, 0x5555ff, 0x55ff55, 0x55ffff, 0xff5555, 0xff55ff, 0xffff55,
        0xffffff,
    ];
    if c < 16 {
        EGA[c]
    } else if c < 232 {
        let i = c - 16;
        let lv = |v: usize| (v * 51) as u32;
        (lv(i / 36) << 16) | (lv(i / 6 % 6) << 8) | lv(i % 6)
    } else {
        let g = (8 + (c - 232) * 10) as u32;
        (g << 16) | (g << 8) | g
    }
}

impl Screen {
    /// Must be called while firmware is still alive (needs the GOP protocol).
    pub fn new(gop: &mut GraphicsOutput) -> Screen {
        let info = gop.current_mode_info();
        let (width, height) = info.resolution();
        let bgr = match info.pixel_format() {
            PixelFormat::Rgb => false,
            PixelFormat::Bgr => true,
            _ => panic!("unsupported pixel format"),
        };
        let front = gop.frame_buffer().as_mut_ptr() as *mut u32;
        let stride = info.stride();
        let scale = (width / VW).min(height / VH).max(1);
        let ox = width.saturating_sub(VW * scale) / 2;
        let oy = height.saturating_sub(VH * scale) / 2;
        // paint the black bars once; present() never touches them again
        for y in 0..height {
            for x in 0..width {
                unsafe { front.add(y * stride + x).write_volatile(0) };
            }
        }
        let mut pal = [0u32; 256];
        for (i, p) in pal.iter_mut().enumerate() {
            let c = rgb(i);
            *p = if bgr { c } else { ((c & 0xff) << 16) | (c & 0xff00) | (c >> 16) };
        }
        Screen { front, canvas: alloc::vec![0; VW * VH], pal, row: alloc::vec![0; VW * scale], width, height, stride, scale, ox, oy, bgr, shake: (0, 0) }
    }

    pub fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: Color) {
        let e = c;
        for yy in y..y.saturating_add(h).min(VH) {
            let row = yy * VW;
            for xx in x..x.saturating_add(w).min(VW) {
                self.canvas[row + xx] = e;
            }
        }
    }

    fn monitor(&self, c: u32) -> u32 {
        if self.bgr { c } else { ((c & 0xff) << 16) | (c & 0xff00) | (c >> 16) }
    }

    /// Redefine one colour for the running game.
    pub fn set_color(&mut self, i: usize, r: i64, g: i64, b: i64) {
        let c = (r.clamp(0, 255) as u32) << 16 | (g.clamp(0, 255) as u32) << 8 | b.clamp(0, 255) as u32;
        self.pal[i & 255] = self.monitor(c);
    }

    /// Back to the standard colours (the menu and the next game expect them).
    pub fn reset_palette(&mut self) {
        for i in 0..256 {
            self.pal[i] = self.monitor(rgb(i));
        }
    }

    pub fn shake(&mut self, dx: i64, dy: i64) {
        self.shake = (dx, dy);
    }

    /// Picture characters: `0-9a-f` are colours 0-15, anything else is see-through.
    fn glyph_color(ch: u8) -> Option<u8> {
        match ch {
            b'0'..=b'9' => Some(ch - b'0'),
            b'a'..=b'f' => Some(ch - b'a' + 10),
            b'A'..=b'F' => Some(ch - b'A' + 10),
            _ => None,
        }
    }

    /// One character per pixel, `w` per row, each pixel a `scale` x `scale` block; clipped.
    pub fn sprite(&mut self, x: i64, y: i64, data: &[u8], w: i64, scale: i64) {
        let (w, s) = (w.clamp(1, 4096), scale.clamp(1, 64));
        for (i, &ch) in data.iter().enumerate() {
            let Some(c) = Self::glyph_color(ch) else { continue };
            let px = x.saturating_add((i as i64 % w) * s);
            let py = y.saturating_add((i as i64 / w) * s);
            let (x0, x1) = (px.max(0), px.saturating_add(s).min(VW as i64));
            let (y0, y1) = (py.max(0), py.saturating_add(s).min(VH as i64));
            if x0 >= x1 || y0 >= y1 {
                continue;
            }
            for yy in y0..y1 {
                let row = yy as usize * VW;
                self.canvas[row + x0 as usize..row + x1 as usize].fill(c);
            }
        }
    }

    /// A scrolling tile map. `map`: tile numbers, `map_w` per row. `sheet`: the tiles one after
    /// another, each `tile` x `tile` characters. (sx, sy) = world pixel at the screen's top-left.
    pub fn tiles(&mut self, map: &[u8], map_w: i64, sheet: &[u8], tile: i64, sx: i64, sy: i64) {
        let (t, mw) = (tile.clamp(1, 64), map_w.clamp(1, 4096));
        let area = (t * t) as usize;
        let ntiles = sheet.len() / area;
        let rows = (map.len() as i64 + mw - 1) / mw;
        for ty in sy.div_euclid(t)..=(sy.saturating_add(VH as i64 - 1)).div_euclid(t) {
            if ty < 0 || ty >= rows {
                continue;
            }
            for tx in sx.div_euclid(t)..=(sx.saturating_add(VW as i64 - 1)).div_euclid(t) {
                if tx < 0 || tx >= mw {
                    continue;
                }
                let Some(&n) = map.get((ty * mw + tx) as usize) else { continue };
                if n as usize >= ntiles {
                    continue;
                }
                let base = n as usize * area;
                for py in 0..t {
                    let yy = ty * t + py - sy;
                    if yy < 0 || yy >= VH as i64 {
                        continue;
                    }
                    for px in 0..t {
                        let xx = tx * t + px - sx;
                        if xx < 0 || xx >= VW as i64 {
                            continue;
                        }
                        if let Some(c) = Self::glyph_color(sheet[base + (py * t + px) as usize]) {
                            self.canvas[yy as usize * VW + xx as usize] = c;
                        }
                    }
                }
            }
        }
    }

    pub fn clear(&mut self, c: Color) {
        self.canvas.fill(c);
    }

    /// Scale the canvas into video memory. Each canvas row is expanded once into `row`, then
    /// copied `scale` times (video memory is slow to read, so we never read it).
    pub fn present(&mut self) {
        let sc = self.scale;
        let (dx, dy) = core::mem::take(&mut self.shake);
        for vy in 0..VH {
            let srow = vy as i64 - dy;
            for vx in 0..VW {
                let scol = vx as i64 - dx;
                let c = if srow < 0 || srow >= VH as i64 || scol < 0 || scol >= VW as i64 { 0 } else { self.canvas[srow as usize * VW + scol as usize] };
                let p = self.pal[c as usize];
                self.row[vx * sc..vx * sc + sc].fill(p);
            }
            for k in 0..sc {
                let y = self.oy + vy * sc + k;
                if y >= self.height {
                    return;
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(self.row.as_ptr(), self.front.add(y * self.stride + self.ox), (VW * sc).min(self.width - self.ox));
                }
            }
        }
    }

    /// Draw ASCII text with the built-in 8x8 font, each font pixel becoming a `mult` x `mult` block.
    pub fn text(&mut self, x: usize, y: usize, s: &str, c: Color, mult: usize) {
        self.text_chars(x, y, s.chars(), c, mult);
    }

    /// Draw characters straight from an iterator: no temporary String needed.
    pub fn text_chars(&mut self, x: usize, y: usize, chars: impl Iterator<Item = char>, c: Color, mult: usize) {
        let mut cx = x;
        for ch in chars {
            let glyph = font8x8::legacy::BASIC_LEGACY.get(ch as usize).copied().unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        self.rect(cx + col * mult, y + row * mult, mult, mult, c);
                    }
                }
            }
            cx += 8 * mult;
        }
    }
}
