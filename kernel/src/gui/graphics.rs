// src/gui/fonts.rs
use limine::framebuffer::Framebuffer;
use crate::kernel::serial::SERIAL_PORT;

pub struct Graphics {
    framebuffer_addr: *mut u8,
    width: u64,
    height: u64,
    pitch: u64,
}

impl Graphics {
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

        Self {
            framebuffer_addr: framebuffer.addr(),
            width: framebuffer.width(),
            height: framebuffer.height(),
            pitch: framebuffer.pitch(),
        }
    }

    // Clear entire screen with color
    pub fn clear_screen(&self, color: u32) {
        let fb_ptr = self.framebuffer_addr as *mut u32;
        let pixel_count = (self.width * self.height) as usize;

        unsafe {
            for i in 0..pixel_count {
                *fb_ptr.add(i) = color;
            }
        }
    }

    // Draw a single pixel
    pub fn put_pixel(&self, x: u64, y: u64, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }

        let offset = (y * self.width + x) as usize;
        let fb_ptr = self.framebuffer_addr as *mut u32;

        unsafe {
            *fb_ptr.add(offset) = color;
        }
    }

    // Draw a filled rectangle
    pub fn fill_rect(&self, x: u64, y: u64, width: u64, height: u64, color: u32) {
        for dy in 0..height {
            for dx in 0..width {
                self.put_pixel(x + dx, y + dy, color);
            }
        }
    }

    // Draw rectangle outline
    pub fn draw_rect(&self, x: u64, y: u64, width: u64, height: u64, color: u32, thickness: u64) {
        // Top and bottom borders
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

        // Left and right borders
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

    // Draw a line using Bresenham's algorithm
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

            if x == x1 && y == y1 {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    // Draw a circle using midpoint algorithm
    pub fn draw_circle(&self, center_x: i64, center_y: i64, radius: i64, color: u32) {
        let mut x = 0;
        let mut y = radius;
        let mut d = 1 - radius;

        while x <= y {
            // Draw 8 octants
            self.put_pixel_safe(center_x + x, center_y + y, color);
            self.put_pixel_safe(center_x - x, center_y + y, color);
            self.put_pixel_safe(center_x + x, center_y - y, color);
            self.put_pixel_safe(center_x - x, center_y - y, color);
            self.put_pixel_safe(center_x + y, center_y + x, color);
            self.put_pixel_safe(center_x - y, center_y + x, color);
            self.put_pixel_safe(center_x + y, center_y - x, color);
            self.put_pixel_safe(center_x - y, center_y - x, color);

            if d < 0 {
                d += 2 * x + 3;
            } else {
                d += 2 * (x - y) + 5;
                y -= 1;
            }
            x += 1;
        }
    }

    // Helper function to safely put pixel with bounds checking
    pub fn put_pixel_safe(&self, x: i64, y: i64, color: u32) {
        if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
            self.put_pixel(x as u64, y as u64, color);
        }
    }

    // Get screen dimensions
    pub fn get_dimensions(&self) -> (u64, u64) {
        (self.width, self.height)
    }

    /// Draw a simple arrow cursor at the specified position
    pub fn draw_cursor(&self, x: i64, y: i64, color: u32) {
        // Simple arrow cursor (11x19 pixels)
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
                    '.' => self.put_pixel_safe(px, py, 0xFF000000), // Black outline
                    _ => {} // Transparent
                }
            }
        }
    }

    /// Clear cursor area (call before redrawing)
    pub fn clear_cursor(&self, x: i64, y: i64, bg_color: u32) {
        // Clear a 11x19 area around cursor
        for dy in 0..19 {
            for dx in 0..11 {
                self.put_pixel_safe(x + dx, y + dy, bg_color);
            }
        }
    }
        /// Save pixels under cursor area and return them
    pub fn save_cursor_area(&self, x: i64, y: i64) -> [[u32; 11]; 19] {
        let mut saved = [[0u32; 11]; 19];
        
        for dy in 0..19 {
            for dx in 0..11 {
                let px = x + dx;
                let py = y + dy;
                
                if px >= 0 && py >= 0 && px < self.width as i64 && py < self.height as i64 {
                    let offset = (py as u64 * self.width + px as u64) as usize;
                    let fb_ptr = self.framebuffer_addr as *mut u32;
                    unsafe {
                        saved[dy as usize][dx as usize] = *fb_ptr.add(offset);
                    }
                }
            }
        }
        saved
    }
    
    /// Restore saved pixels
    pub fn restore_cursor_area(&self, x: i64, y: i64, saved: &[[u32; 11]; 19]) {
        for dy in 0..19 {
            for dx in 0..11 {
                let px = x + dx;
                let py = y + dy;
                
                if px >= 0 && py >= 0 && px < self.width as i64 && py < self.height as i64 {
                    self.put_pixel(px as u64, py as u64, saved[dy as usize][dx as usize]);
                }
            }
        }
    }
}

