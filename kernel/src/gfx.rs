//! Framebuffer graphics with a back buffer.
//! We draw into `back` (ordinary RAM, invisible), then `present()` copies the finished
//! frame to video memory in one go -> no flicker, no half-drawn frames.
extern crate alloc;
use alloc::vec::Vec;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};

pub struct Screen {
    front: *mut u32, // video memory (what the monitor reads)
    back: Vec<u32>,  // our private canvas, width*height pixels, no padding
    pub width: usize,
    pub height: usize,
    stride: usize, // pixels per row in VIDEO memory (may exceed width)
    bgr: bool,
}

/// A colour as 0xRRGGBB, independent of the hardware byte order.
pub type Color = u32;

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
        Screen { front, back: alloc::vec![0; width * height], width, height, stride: info.stride(), bgr }
    }

    #[inline]
    fn encode(&self, c: Color) -> u32 {
        if self.bgr {
            c
        } else {
            let (r, g, b) = ((c >> 16) & 0xff, (c >> 8) & 0xff, c & 0xff);
            (b << 16) | (g << 8) | r
        }
    }

    pub fn pixel(&mut self, x: usize, y: usize, c: Color) {
        if x < self.width && y < self.height {
            let e = self.encode(c);
            self.back[y * self.width + x] = e;
        }
    }

    pub fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: Color) {
        let e = self.encode(c);
        for yy in y..(y + h).min(self.height) {
            let row = yy * self.width;
            for xx in x..(x + w).min(self.width) {
                self.back[row + xx] = e;
            }
        }
    }

    pub fn clear(&mut self, c: Color) {
        let e = self.encode(c);
        self.back.fill(e);
    }

    /// Copy the finished frame to the screen, one row at a time (rows are `stride` apart there).
    pub fn present(&self) {
        for y in 0..self.height {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.back.as_ptr().add(y * self.width),
                    self.front.add(y * self.stride),
                    self.width,
                );
            }
        }
    }
}

impl Screen {
    /// Draw ASCII text with the built-in 8x8 font, each font pixel becoming a `scale`x`scale` block.
    pub fn text(&mut self, x: usize, y: usize, s: &str, c: Color, scale: usize) {
        let mut cx = x;
        for ch in s.chars() {
            let glyph = font8x8::legacy::BASIC_LEGACY.get(ch as usize).copied().unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits >> col & 1 == 1 {
                        self.rect(cx + col * scale, y + row * scale, scale, scale, c);
                    }
                }
            }
            cx += 8 * scale;
        }
    }
}
