// src/gui/graphics.rs
extern crate alloc;

use limine::framebuffer::Framebuffer;
use crate::kernel::serial::SERIAL_PORT;

pub struct Graphics {
    framebuffer_addr: *mut u8,
    /// Back buffer in plain RAM — all drawing targets this.
    /// Layout: row-major, stride = width (no padding), size = width * height u32s.
    back_buffer: *mut u32,
    width: u64,
    height: u64,
    /// Framebuffer pitch in bytes (may include row padding).
    pitch: u64,
}

// Safety: single-threaded kernel; no concurrent access.
unsafe impl Send for Graphics {}
unsafe impl Sync for Graphics {}

impl Graphics {
    #[inline(always)]
    fn pitch_pixels(&self) -> usize {
        (self.pitch / 4) as usize
    }

    /// Offset into the **back buffer** for pixel (x, y).
    #[inline(always)]
    fn pixel_offset(&self, x: u64, y: u64) -> usize {
        y as usize * self.width as usize + x as usize
    }

    pub fn new(framebuffer: Framebuffer) -> Self {
        unsafe {
            SERIAL_PORT.write_str("GUI: Initializing graphics system\n");
            SERIAL_PORT.write_str("  Resolution: ");
            SERIAL_PORT.write_decimal(framebuffer.width() as u32);
            SERIAL_PORT.write_str("x");
            SERIAL_PORT.write_decimal(framebuffer.height() as u32);
            SERIAL_PORT.write_str(" BPP: ");
            SERIAL_PORT.write_decimal(framebuffer.bpp() as u32);
            SERIAL_PORT.write_str("\n");
        }

        let width  = framebuffer.width();
        let height = framebuffer.height();
        let buf_size = (width * height) as usize;

        // Allocate back buffer in heap RAM and leak it (kernel lives forever).
        let mut back_vec: alloc::vec::Vec<u32> = alloc::vec![0u32; buf_size];
        let back_ptr = back_vec.as_mut_ptr();
        core::mem::forget(back_vec);

        Self {
            framebuffer_addr: framebuffer.addr(),
            back_buffer: back_ptr,
            width,
            height,
            pitch: framebuffer.pitch(),
        }
    }

    // ── Present ────────────────────────────────────────────────────────────────

    /// Blit the back buffer to the real framebuffer.
    /// Called once per frame after all drawing is done.
    pub fn present(&self) {
        let fb_ptr       = self.framebuffer_addr as *mut u32;
        let pitch_pixels = self.pitch_pixels();
        let w            = self.width  as usize;
        let h            = self.height as usize;

        unsafe {
            for y in 0..h {
                let src = self.back_buffer.add(y * w);
                let dst = fb_ptr.add(y * pitch_pixels);
                core::ptr::copy_nonoverlapping(src, dst, w);
            }
        }
    }

    // ── Primitives (all write to back buffer) ──────────────────────────────────

    /// Clear entire screen with a solid color.
    pub fn clear_screen(&self, color: u32) {
        let total = (self.width * self.height) as usize;
        unsafe {
            let buf = core::slice::from_raw_parts_mut(self.back_buffer, total);
            buf.fill(color);
        }
    }

    /// Draw a single pixel.
    pub fn put_pixel(&self, x: u64, y: u64, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = self.pixel_offset(x, y);
        unsafe {
            *self.back_buffer.add(offset) = color;
        }
    }

    /// Draw a filled rectangle.
    pub fn fill_rect(&self, x: u64, y: u64, width: u64, height: u64, color: u32) {
        if width == 0 || height == 0 || x >= self.width || y >= self.height {
            return;
        }

        let clipped_width  = width.min(self.width - x)   as usize;
        let clipped_height = height.min(self.height - y) as usize;
        let start_x        = x as usize;
        let start_y        = y as usize;
        let w              = self.width as usize;
        let total          = (self.width * self.height) as usize;

        unsafe {
            for dy in 0..clipped_height {
                let row_start = (start_y + dy) * w + start_x;
                if row_start + clipped_width > total { break; }
                let row = core::slice::from_raw_parts_mut(
                    self.back_buffer.add(row_start), clipped_width);
                row.fill(color);
            }
        }
    }

    /// Draw a rectangle outline.
    pub fn draw_rect(&self, x: u64, y: u64, width: u64, height: u64, color: u32, thickness: u64) {
        for i in 0..width {
            for t in 0..thickness {
                if y + t < self.height {
                    self.put_pixel(x + i, y + t, color);
                }
                if y + height > t && y + height - t - 1 < self.height {
                    self.put_pixel(x + i, y + height - t - 1, color);
                }
            }
        }
        for i in 0..height {
            for t in 0..thickness {
                if x + t < self.width {
                    self.put_pixel(x + t, y + i, color);
                }
                if x + width > t && x + width - t - 1 < self.width {
                    self.put_pixel(x + width - t - 1, y + i, color);
                }
            }
        }
    }

    /// Draw a line using Bresenham's algorithm.
    pub fn draw_line(&self, x0: i64, y0: i64, x1: i64, y1: i64, color: u32) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
                self.put_pixel(x as u64, y as u64, color);
            }
            if x == x1 && y == y1 { break; }
            let e2 = 2 * err;
            if e2 > -dy { err -= dy; x += sx; }
            if e2 < dx  { err += dx; y += sy; }
        }
    }

    /// Draw a circle using the midpoint algorithm.
    pub fn draw_circle(&self, center_x: i64, center_y: i64, radius: i64, color: u32) {
        let mut x = 0;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            self.put_pixel_safe(center_x + x, center_y + y, color);
            self.put_pixel_safe(center_x - x, center_y + y, color);
            self.put_pixel_safe(center_x + x, center_y - y, color);
            self.put_pixel_safe(center_x - x, center_y - y, color);
            self.put_pixel_safe(center_x + y, center_y + x, color);
            self.put_pixel_safe(center_x - y, center_y + x, color);
            self.put_pixel_safe(center_x + y, center_y - x, color);
            self.put_pixel_safe(center_x - y, center_y - x, color);

            if d < 0 { d += 2 * x + 3; }
            else      { d += 2 * (x - y) + 5; y -= 1; }
            x += 1;
        }
    }

    /// Put a pixel with bounds checking (accepts signed coords).
    pub fn put_pixel_safe(&self, x: i64, y: i64, color: u32) {
        if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
            self.put_pixel(x as u64, y as u64, color);
        }
    }

    // ── Dimensions ─────────────────────────────────────────────────────────────

    pub fn get_dimensions(&self) -> (u64, u64) {
        (self.width, self.height)
    }

    // ── Cursor helpers ─────────────────────────────────────────────────────────

    /// Draw an arrow cursor at (x, y) into the back buffer.
    pub fn draw_cursor(&self, x: i64, y: i64, color: u32) {
        let cursor_data = [
            "X          ",
            "XX         ",
            "X.X        ",
            "X..X       ",
            "X...X      ",
            "X....X     ",
            "X.....X    ",
            "X......X   ",
            "X.......X  ",
            "X........X ",
            "X.........X",
            "X......XXXX",
            "X...X..X   ",
            "X..X X..X  ",
            "X.X  X..X  ",
            "XX   X..X  ",
            "X     X..X ",
            "      X..X ",
            "       XX  ",
        ];

        for (row, line) in cursor_data.iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let px = x + col as i64;
                let py = y + row as i64;
                match ch {
                    'X' => self.put_pixel_safe(px, py, color),
                    '.' => self.put_pixel_safe(px, py, 0xFF000000),
                    _   => {}
                }
            }
        }
    }

    /// Save the 11×19 pixel area under the cursor from the back buffer.
    pub fn save_cursor_area(&self, x: i64, y: i64) -> [[u32; 11]; 19] {
        let mut saved = [[0u32; 11]; 19];
        for dy in 0..19i64 {
            for dx in 0..11i64 {
                let px = x + dx;
                let py = y + dy;
                if px >= 0 && py >= 0 && px < self.width as i64 && py < self.height as i64 {
                    let offset = self.pixel_offset(px as u64, py as u64);
                    unsafe {
                        saved[dy as usize][dx as usize] = *self.back_buffer.add(offset);
                    }
                }
            }
        }
        saved
    }

    /// Restore a previously saved 11×19 pixel area into the back buffer.
    pub fn restore_cursor_area(&self, x: i64, y: i64, saved: &[[u32; 11]; 19]) {
        for dy in 0..19i64 {
            for dx in 0..11i64 {
                let px = x + dx;
                let py = y + dy;
                if px >= 0 && py >= 0 && px < self.width as i64 && py < self.height as i64 {
                    self.put_pixel(px as u64, py as u64, saved[dy as usize][dx as usize]);
                }
            }
        }
    }

    /// Clear cursor area with a background color (back buffer).
    pub fn clear_cursor(&self, x: i64, y: i64, bg_color: u32) {
        for dy in 0..19 {
            for dx in 0..11 {
                self.put_pixel_safe(x + dx, y + dy, bg_color);
            }
        }
    }

    // ── Gradient & background helpers ──────────────────────────────────────────

    /// Linearly interpolate between two opaque 0xFFRRGGBB colours.
    /// t=0 → colour a, t=255 → colour b.
    #[inline]
    pub fn lerp_color(a: u32, b: u32, t: u8) -> u32 {
        let inv = 255 - t as u32;
        let t   = t as u32;
        let r = (((a >> 16) & 0xFF) * inv + ((b >> 16) & 0xFF) * t) / 255;
        let g = (((a >>  8) & 0xFF) * inv + ((b >>  8) & 0xFF) * t) / 255;
        let bl = ((a & 0xFF) * inv + (b & 0xFF) * t) / 255;
        0xFF000000 | (r << 16) | (g << 8) | bl
    }

    /// Fill a rectangle with a vertical colour gradient (top → bottom).
    pub fn fill_rect_gradient_v(&self, x: u64, y: u64, w: u64, h: u64, top: u32, bot: u32) {
        if w == 0 || h == 0 { return; }
        for row in 0..h {
            let t = if h <= 1 { 0u8 } else { ((row * 255) / (h - 1)) as u8 };
            self.fill_rect(x, y + row, w, 1, Self::lerp_color(top, bot, t));
        }
    }

    /// Fill a rectangle with a horizontal colour gradient (left → right).
    pub fn fill_rect_gradient_h(&self, x: u64, y: u64, w: u64, h: u64, lft: u32, rgt: u32) {
        if w == 0 || h == 0 { return; }
        for col in 0..w {
            let t = if w <= 1 { 0u8 } else { ((col * 255) / (w - 1)) as u8 };
            self.fill_rect(x + col, y, 1, h, Self::lerp_color(lft, rgt, t));
        }
    }

    /// Draw the desktop wallpaper: deep-navy-to-black gradient + subtle dot grid.
    pub fn draw_desktop_background(&self) {
        // Vertical gradient: deep navy → near-black
        self.fill_rect_gradient_v(0, 0, self.width, self.height, 0xFF0D1B2A, 0xFF08101C);

        // Subtle 40×40 dot grid — slightly lighter than background
        let grid_col = 0xFF142030;
        let mut gy = 0u64;
        while gy < self.height {
            let mut gx = 0u64;
            while gx < self.width {
                self.put_pixel(gx, gy, grid_col);
                gx += 40;
            }
            gy += 40;
        }
    }

    /// Draw a horizontal progress bar (filled fraction 0–100).
    pub fn draw_progress_bar(&self, x: u64, y: u64, w: u64, h: u64, pct: u8,
                              bg: u32, fill: u32, border: u32) {
        self.fill_rect(x, y, w, h, bg);
        self.draw_rect(x, y, w, h, border, 1);
        let filled = ((w.saturating_sub(2)) * pct.min(100) as u64) / 100;
        if filled > 0 {
            self.fill_rect_gradient_h(x + 1, y + 1, filled, h.saturating_sub(2),
                                      fill, Self::lerp_color(fill, 0xFF00D4FF, 200));
        }
    }
}
