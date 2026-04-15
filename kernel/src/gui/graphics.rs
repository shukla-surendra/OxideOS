// src/gui/graphics.rs
extern crate alloc;

use limine::framebuffer::Framebuffer;
use crate::kernel::serial::SERIAL_PORT;

// ── Panic framebuffer ─────────────────────────────────────────────────────────
//
// Stored once at Graphics::new() time so the panic handler can draw a BSoD
// without needing access to the Graphics object.
#[derive(Clone, Copy)]
pub struct PanicFb {
    pub addr:   *mut u32,
    pub width:  u64,
    pub height: u64,
    pub pitch:  u64, // in bytes
}
unsafe impl Send for PanicFb {}
unsafe impl Sync for PanicFb {}

pub static mut PANIC_FB: Option<PanicFb> = None;

/// Selects which procedural wallpaper to render on the desktop.
#[derive(Clone, Copy, PartialEq)]
pub enum BackgroundStyle {
    Default,
    Sunset,
    Space,
    Aurora,
    Geometric,
    /// Uses the PNG embedded from `assets/wallpaper.png` at build time.
    /// Falls back to Default if the file was absent when building.
    Image,
}

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

        let fb_addr = framebuffer.addr();
        let fb_pitch = framebuffer.pitch();

        // Publish for the panic handler.
        unsafe {
            PANIC_FB = Some(PanicFb {
                addr:   fb_addr as *mut u32,
                width,
                height,
                pitch:  fb_pitch,
            });
        }

        Self {
            framebuffer_addr: fb_addr,
            back_buffer: back_ptr,
            width,
            height,
            pitch: fb_pitch,
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

    /// Dispatch to the selected background style.
    pub fn draw_background(&self, style: BackgroundStyle) {
        match style {
            BackgroundStyle::Default   => self.draw_desktop_background(),
            BackgroundStyle::Sunset    => self.draw_background_sunset(),
            BackgroundStyle::Space     => self.draw_background_space(),
            BackgroundStyle::Aurora    => self.draw_background_aurora(),
            BackgroundStyle::Geometric => self.draw_background_geometric(),
            BackgroundStyle::Image     => self.draw_background_image(),
        }
    }

    /// Warm sunset: deep purple at top fading to burnt orange at bottom.
    fn draw_background_sunset(&self) {
        let h = self.height;
        let w = self.width;
        // Sky: purple → magenta-red
        self.fill_rect_gradient_v(0, 0, w, h * 55 / 100, 0xFF180A38, 0xFF8B1A50);
        // Horizon band: red → orange
        self.fill_rect_gradient_v(0, h * 55 / 100, w, h * 20 / 100, 0xFF8B1A50, 0xFFE05820);
        // Ground: dark orange → near black
        self.fill_rect_gradient_v(0, h * 75 / 100, w, h * 25 / 100, 0xFF501000, 0xFF140400);
        // Horizon glow line
        self.fill_rect(0, h * 55 / 100 - 1, w, 3, 0xFFFF8830);
        // A few faint star dots in the upper sky
        let mut seed: u64 = 0xCAFEBABE;
        for _ in 0..60u32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sx = (seed >> 18) % w;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sy = (seed >> 18) % (h * 45 / 100);
            self.put_pixel(sx, sy, 0xFFFFCCAA);
        }
    }

    /// Deep space: near-black with procedural stars.
    fn draw_background_space(&self) {
        let h = self.height;
        let w = self.width;
        // Base: very dark blue-black gradient
        self.fill_rect_gradient_v(0, 0, w, h, 0xFF04060E, 0xFF020408);
        // Subtle nebula haze in the middle band
        self.fill_rect_gradient_v(0, h / 4, w, h / 2, 0xFF04060E, 0xFF060A18);
        self.fill_rect_gradient_v(0, h * 3 / 4, w, h / 4, 0xFF060A18, 0xFF020408);
        // Stars: use LCG to scatter bright pixels
        let mut seed: u64 = 0xDEADBEEF1234ABCD;
        for _ in 0..400u32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sx = (seed >> 16) % w;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sy = (seed >> 16) % h;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let bright = 0xA0u32 + ((seed >> 24) & 0x5F) as u32;
            let color = 0xFF000000 | (bright << 16) | (bright << 8) | bright;
            self.put_pixel(sx, sy, color);
            // 1-in-16 chance of a brighter 2×2 star
            if seed & 0xF == 0 {
                self.put_pixel_safe(sx as i64 + 1, sy as i64,     color);
                self.put_pixel_safe(sx as i64,     sy as i64 + 1, color);
                self.put_pixel_safe(sx as i64 + 1, sy as i64 + 1, color);
            }
        }
    }

    /// Aurora borealis: dark teal base with green and blue-violet glow bands.
    fn draw_background_aurora(&self) {
        let h = self.height;
        let w = self.width;
        // Dark teal base
        self.fill_rect_gradient_v(0, 0, w, h, 0xFF030E12, 0xFF020810);
        // Green aurora band (upper third)
        let g1_top = h / 5;
        for row in 0..100u64 {
            if g1_top + row >= h { break; }
            let t = if row < 50 { row * 2 } else { (100 - row) * 2 };
            let g = (t * 45 / 100) as u32;
            let b = (t * 20 / 100) as u32;
            self.fill_rect(0, g1_top + row, w, 1, 0xFF000000 | (g << 8) | b);
        }
        // Blue-violet aurora band (middle)
        let g2_top = h * 2 / 5;
        for row in 0..80u64 {
            if g2_top + row >= h { break; }
            let t = if row < 40 { row * 2 } else { (80 - row) * 2 };
            let r = (t * 18 / 100) as u32;
            let b = (t * 50 / 100) as u32;
            let g = (t * 10 / 100) as u32;
            self.fill_rect(0, g2_top + row, w, 1, 0xFF000000 | (r << 16) | (g << 8) | b);
        }
        // Faint star field
        let mut seed: u64 = 0x1234567890ABCDEF;
        for _ in 0..120u32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sx = (seed >> 16) % w;
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let sy = (seed >> 16) % h;
            self.put_pixel(sx, sy, 0xFF6090A0);
        }
    }

    /// Geometric: dark grid with subtle diagonal accent lines.
    fn draw_background_geometric(&self) {
        let h = self.height;
        let w = self.width;
        // Dark gradient base
        self.fill_rect_gradient_v(0, 0, w, h, 0xFF06080F, 0xFF030408);
        // Vertical grid lines
        let grid_col = 0xFF0C1220;
        let mut x = 0u64;
        while x < w { self.fill_rect(x, 0, 1, h, grid_col); x += 80; }
        // Horizontal grid lines
        let mut y = 0u64;
        while y < h { self.fill_rect(0, y, w, 1, grid_col); y += 80; }
        // Diagonal accent lines (top-left to bottom-right)
        let accent = 0xFF0A1428;
        let step = 160i64;
        let mut s: i64 = -(h as i64);
        while s < w as i64 {
            self.draw_line(s, 0, s + h as i64, h as i64, accent);
            s += step;
        }
        // Bright dot at each grid intersection
        let dot_col = 0xFF141E30;
        let mut gy = 0u64;
        while gy < h {
            let mut gx = 0u64;
            while gx < w {
                self.put_pixel(gx, gy, dot_col);
                gx += 80;
            }
            gy += 80;
        }
    }

    /// Blit the PNG embedded from `assets/wallpaper.png` (decoded to RGBA at
    /// build time) onto the whole screen using nearest-neighbour scaling.
    /// Falls back to the Default gradient when no image was embedded.
    fn draw_background_image(&self) {
        let w_img = crate::wallpaper::WALLPAPER_W as u64;
        let h_img = crate::wallpaper::WALLPAPER_H as u64;
        if w_img == 0 || h_img == 0 {
            self.draw_desktop_background();
            return;
        }

        let pixels  = crate::wallpaper::PIXELS;
        let sw      = self.width;
        let sh      = self.height;
        let w_total = self.width as usize;

        unsafe {
            for sy in 0..sh {
                // Map screen row → source row
                let py = (sy * h_img / sh) as usize;
                for sx in 0..sw {
                    // Map screen col → source col
                    let px  = (sx * w_img / sw) as usize;
                    let idx = (py * w_img as usize + px) * 4;
                    // Bounds check — pixels.len() is known at compile time so
                    // this branch is almost always eliminated by the optimizer.
                    if idx + 2 < pixels.len() {
                        let r = pixels[idx    ] as u32;
                        let g = pixels[idx + 1] as u32;
                        let b = pixels[idx + 2] as u32;
                        let offset = sy as usize * w_total + sx as usize;
                        *self.back_buffer.add(offset) = 0xFF00_0000 | (r << 16) | (g << 8) | b;
                    }
                }
            }
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
