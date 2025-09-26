// src/gui/mod.rs - Basic GUI system for OxideOS
#![no_std]

use limine::framebuffer::Framebuffer;
use crate::kernel::serial::SERIAL_PORT;


// Basic color definitions
pub mod colors {
    pub const BLACK: u32       = 0xFF000000;
    pub const WHITE: u32       = 0xFFFFFFFF;
    pub const RED: u32         = 0xFFFF0000;
    pub const GREEN: u32       = 0xFF00FF00;
    pub const BLUE: u32        = 0xFF0000FF;
    pub const YELLOW: u32      = 0xFFFFFF00;
    pub const CYAN: u32        = 0xFF00FFFF;
    pub const MAGENTA: u32     = 0xFFFF00FF;
    pub const GRAY: u32        = 0xFF808080;
    pub const DARK_GRAY: u32   = 0xFF404040;
    pub const LIGHT_GRAY: u32  = 0xFFC0C0C0;
    pub const ORANGE: u32      = 0xFFFFA500;
    pub const PURPLE: u32      = 0xFF800080;
}

// Basic graphics operations
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
    fn put_pixel_safe(&self, x: i64, y: i64, color: u32) {
        if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
            self.put_pixel(x as u64, y as u64, color);
        }
    }

    // Get screen dimensions
    pub fn get_dimensions(&self) -> (u64, u64) {
        (self.width, self.height)
    }
}

// Simple bitmap font (8x8 pixels) - just a few characters for demo
pub mod font {
    // Simple 8x8 bitmap for letter 'O'
    pub const CHAR_O: [u8; 8] = [
        0b00111100,
        0b01100110,
        0b11000011,
        0b11000011,
        0b11000011,
        0b11000011,
        0b01100110,
        0b00111100,
    ];

    // Simple 8x8 bitmap for letter 'S'  
    pub const CHAR_S: [u8; 8] = [
        0b00111100,
        0b01100110,
        0b01100000,
        0b00111100,
        0b00000110,
        0b00000110,
        0b01100110,
        0b00111100,
    ];

    // You can add more characters here...
    
    pub fn draw_char(graphics: &super::Graphics, x: u64, y: u64, ch: char, color: u32) {
        let bitmap = match ch {
            'O' | 'o' => &CHAR_O,
            'S' | 's' => &CHAR_S,
            _ => return, // Unknown character, skip
        };

        for row in 0..8 {
            let byte = bitmap[row];
            for col in 0..8 {
                if (byte & (0b10000000 >> col)) != 0 {
                    graphics.put_pixel(x + col as u64, y + row as u64, color);
                }
            }
        }
    }

    pub fn draw_string(graphics: &super::Graphics, x: u64, y: u64, text: &str, color: u32) {
        let mut current_x = x;
        for ch in text.chars() {
            draw_char(graphics, current_x, y, ch, color);
            current_x += 9; // 8 pixels + 1 pixel spacing
        }
    }
}

// Basic UI widgets
pub mod widgets {
    use super::{Graphics, colors};

    pub struct Button {
        pub x: u64,
        pub y: u64,
        pub width: u64,
        pub height: u64,
        pub text: &'static str,
        pub bg_color: u32,
        pub fg_color: u32,
        pub pressed: bool,
    }

    impl Button {
        pub fn new(x: u64, y: u64, width: u64, height: u64, text: &'static str) -> Self {
            Self {
                x, y, width, height, text,
                bg_color: colors::LIGHT_GRAY,
                fg_color: colors::BLACK,
                pressed: false,
            }
        }

        pub fn draw(&self, graphics: &Graphics) {
            let bg = if self.pressed { colors::GRAY } else { self.bg_color };
            
            // Draw button background
            graphics.fill_rect(self.x, self.y, self.width, self.height, bg);
            
            // Draw border
            graphics.draw_rect(self.x, self.y, self.width, self.height, colors::BLACK, 1);
            
            // Draw text (centered roughly)
            let text_x = self.x + (self.width / 2).saturating_sub((self.text.len() as u64 * 9) / 2);
            let text_y = self.y + (self.height / 2).saturating_sub(4);
            super::font::draw_string(graphics, text_x, text_y, self.text, self.fg_color);
        }

        pub fn is_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
            mouse_x >= self.x && mouse_x < self.x + self.width &&
            mouse_y >= self.y && mouse_y < self.y + self.height
        }
    }

    pub struct Window {
        pub x: u64,
        pub y: u64,
        pub width: u64,
        pub height: u64,
        pub title: &'static str,
        pub bg_color: u32,
    }

    impl Window {
        pub fn new(x: u64, y: u64, width: u64, height: u64, title: &'static str) -> Self {
            Self {
                x, y, width, height, title,
                bg_color: colors::WHITE,
            }
        }

        pub fn draw(&self, graphics: &Graphics) {
            // Draw window background
            graphics.fill_rect(self.x, self.y, self.width, self.height, self.bg_color);
            
            // Draw title bar
            graphics.fill_rect(self.x, self.y, self.width, 30, colors::BLUE);
            
            // Draw window border
            graphics.draw_rect(self.x, self.y, self.width, self.height, colors::BLACK, 2);
            
            // Draw title text
            super::font::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::WHITE);
        }
    }
}