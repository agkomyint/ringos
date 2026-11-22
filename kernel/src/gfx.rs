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
        Screen { front, canvas: alloc::vec![0; VW * VH], pal, row: alloc::vec![0; VW * scale], width, height, stride, scale, ox, oy, bgr }
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

    pub fn clear(&mut self, c: Color) {
        self.canvas.fill(c);
    }

    /// Scale the canvas into video memory. Each canvas row is expanded once into `row`, then
    /// copied `scale` times (video memory is slow to read, so we never read it).
    pub fn present(&mut self) {
        let sc = self.scale;
        for vy in 0..VH {
            for vx in 0..VW {
                let p = self.pal[self.canvas[vy * VW + vx] as usize];
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
