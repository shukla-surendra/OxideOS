use core::ptr::{write_volatile};
use crate::multiboot_parser::Framebuffer;

/// Pack a 0xAARRGGBB color into the framebuffer format and write at (x,y).
/// Supports 32, 24, and 16 (RGB565) bpps. Assumes physical==virtual (identity mapping).
impl Framebuffer {
    /// Put a pixel at (x,y). Safe wrapper is `unsafe` because it dereferences raw addresses.
    /// color is 0xAARRGGBB, alpha ignored for many framebuffers.
    pub unsafe fn put_pixel(&self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height { return; }

        let base = self.phys_addr as *mut u8;
        let bytes_per_pixel = (self.bpp / 8) as usize;
        let offset = y * self.pitch + x * bytes_per_pixel;
        let p = base.add(offset);

        match self.bpp {
            32 => {
                let ptr = p as *mut u32;
                write_volatile(ptr, color);
            }
            24 => {
                // little-endian layout: memory order is B, G, R
                let b = (color & 0xFF) as u8;
                let g = ((color >> 8) & 0xFF) as u8;
                let r = ((color >> 16) & 0xFF) as u8;
                write_volatile(p, b);
                write_volatile(p.add(1), g);
                write_volatile(p.add(2), r);
            }
            16 => {
                // RGB565
                let r8 = ((color >> 16) & 0xFF) as u16;
                let g8 = ((color >> 8) & 0xFF) as u16;
                let b8 = (color & 0xFF) as u16;
                let r5 = (r8 >> 3) & 0x1F;
                let g6 = (g8 >> 2) & 0x3F;
                let b5 = (b8 >> 3) & 0x1F;
                let pixel16: u16 = (r5 << 11) | (g6 << 5) | b5;
                let ptr16 = p as *mut u16;
                write_volatile(ptr16, pixel16);
            }
            _ => {
                // unsupported bpp: do nothing
            }
        }
    }

    /// Fill one row quickly by repeating the `pixel_bytes`. Works for small bpp values.
    /// `pixel_bytes` should contain the in-memory pixel representation (len 4 for 32bpp, 3 for 24bpp, 2 for 16bpp).
    pub unsafe fn fill_row_bytes(&self, y: usize, x0: usize, x1: usize, pixel_bytes: &[u8]) {
        if y >= self.height || x0 >= x1 { return; }
        let base = self.phys_addr as *mut u8;
        let stride = self.pitch;
        let start = base.add(y * stride + x0 * pixel_bytes.len());
        let mut dst = start;
        let count = x1 - x0;
        for _ in 0..count {
            for i in 0..pixel_bytes.len() {
                write_volatile(dst.add(i), pixel_bytes[i]);
            }
            dst = dst.add(pixel_bytes.len());
        }
    }

    /// Draw a filled rectangle. For 32bpp we do fast u32 writes, otherwise we fallback to put_pixel.
    pub unsafe fn fill_rect(&self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        if x >= x1 || y >= y1 { return; }

        if self.bpp == 32 {
            for yy in y..y1 {
                let base = self.phys_addr as *mut u8;
                let mut ptr = base.add(yy * self.pitch + x * 4) as *mut u32;
                for _col in x..x1 {
                    write_volatile(ptr, color);
                    ptr = ptr.add(1);
                }
            }
        } else {
            for yy in y..y1 {
                for xx in x..x1 {
                    self.put_pixel(xx, yy, color);
                }
            }
        }
    }

    /// Bresenham line drawing (integer)
    pub unsafe fn draw_line(&self, x0: isize, y0: isize, x1: isize, y1: isize, color: u32) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            if x >= 0 && (x as usize) < self.width && y >= 0 && (y as usize) < self.height {
                self.put_pixel(x as usize, y as usize, color);
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Draw a horizontal gradient for a quick visual test.
    pub unsafe fn draw_gradient(&self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let t = if self.width > 1 { (x * 255) / (self.width - 1) } else { 0 };
                let r = t as u32;
                let g = ((y * 128) / (self.height.saturating_sub(1))) as u32;
                let b = (255 - t) as u32;
                let color = (0xFF << 24) | (r << 16) | (g << 8) | b;
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Put pixel specialized for 32bpp (faster wrapper).
    pub unsafe fn put_pixel_32(&self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height { return; }
        let base = self.phys_addr as *mut u8;
        let offset = y * self.pitch + x * 4;
        let ptr = base.add(offset) as *mut u32;
        write_volatile(ptr, color);
    }

    /// Clear entire screen (32bpp)
    pub unsafe fn clear_32(&self, color: u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel_32(x, y, color);
            }
        }
    }
}
