// src/gui/mouse.rs - Simple mouse cursor system
#![no_std]

use super::{Graphics, colors};
use crate::kernel::serial::SERIAL_PORT;

pub struct MouseCursor {
    pub x: i64,
    pub y: i64,
    pub visible: bool,
    pub color: u32,
}

impl MouseCursor {
    pub fn new() -> Self {
        Self {
            x: 400, // Start in center-ish
            y: 300,
            visible: true,
            color: colors::WHITE,
        }
    }

    // Update cursor position from PS/2 mouse data
    pub fn update(&mut self, dx: i8, dy: i8, screen_width: u64, screen_height: u64) {
        self.x += dx as i64;
        self.y -= dy as i64; // Y is usually inverted
        
        // Clamp to screen bounds
        self.x = self.x.max(0).min(screen_width as i64 - 1);
        self.y = self.y.max(0).min(screen_height as i64 - 1);
    }

    // Draw a simple arrow cursor
    pub fn draw(&self, graphics: &Graphics) {
        if !self.visible {
            return;
        }

        let x = self.x;
        let y = self.y;

        // Draw arrow cursor (simple version)
        // Vertical line of the arrow
        for i in 0..12 {
            graphics.put_pixel_safe(x, y + i, self.color);
            if i < 6 {
                graphics.put_pixel_safe(x + 1, y + i, self.color);
            }
        }
        
        // Horizontal line of the arrow
        for i in 0..8 {
            graphics.put_pixel_safe(x + i, y, self.color);
            if i < 4 {
                graphics.put_pixel_safe(x + i, y + 1, self.color);
            }
        }
        
        // Arrow point
        graphics.put_pixel_safe(x + 3, y + 3, self.color);
        graphics.put_pixel_safe(x + 4, y + 4, self.color);
        graphics.put_pixel_safe(x + 5, y + 5, self.color);
    }

    // Clear cursor area (call before redrawing)
    pub fn clear(&self, graphics: &Graphics, bg_color: u32) {
        if !self.visible {
            return;
        }

        // Clear a 12x12 area around cursor
        for dy in 0..12 {
            for dx in 0..12 {
                graphics.put_pixel_safe(self.x + dx, self.y + dy, bg_color);
            }
        }
    }

    pub fn get_position(&self) -> (i64, i64) {
        (self.x, self.y)
    }
}

// PS/2 Mouse handler (integrate with your interrupt system)
pub struct PS2Mouse {
    packet_buffer: [u8; 3],
    packet_index: usize,
    left_button: bool,
    right_button: bool,
    middle_button: bool,
}

impl PS2Mouse {
    pub fn new() -> Self {
        Self {
            packet_buffer: [0; 3],
            packet_index: 0,
            left_button: false,
            right_button: false,
            middle_button: false,
        }
    }

    // Initialize PS/2 mouse (call during system init)
    pub unsafe fn init(&mut self) {
        SERIAL_PORT.write_str("Initializing PS/2 mouse...\n");
        
        // Send initialize sequence to mouse
        self.send_mouse_command(0xF6); // Set defaults
        self.send_mouse_command(0xF4); // Enable data reporting
        
        SERIAL_PORT.write_str("PS/2 mouse initialized\n");
    }

    // Handle mouse interrupt (IRQ12) - call this from your interrupt handler
    pub unsafe fn handle_interrupt(&mut self, cursor: &mut MouseCursor, screen_width: u64, screen_height: u64) {
        // Read data from mouse port
        let data: u8;
        core::arch::asm!("in al, 0x60", out("al") data);
        
        self.packet_buffer[self.packet_index] = data;
        self.packet_index += 1;
        
        // Process complete packet (3 bytes)
        if self.packet_index >= 3 {
            self.process_packet(cursor, screen_width, screen_height);
            self.packet_index = 0;
        }
    }

    fn process_packet(&mut self, cursor: &mut MouseCursor, screen_width: u64, screen_height: u64) {
        let flags = self.packet_buffer[0];
        let dx = self.packet_buffer[1] as i8;
        let dy = self.packet_buffer[2] as i8;
        
        // Update button states
        self.left_button = (flags & 0x01) != 0;
        self.right_button = (flags & 0x02) != 0;
        self.middle_button = (flags & 0x04) != 0;
        
        // Update cursor position
        cursor.update(dx, dy, screen_width, screen_height);
        
        unsafe {
            if dx != 0 || dy != 0 {
                SERIAL_PORT.write_str("Mouse: dx=");
                SERIAL_PORT.write_decimal(dx as u32);
                SERIAL_PORT.write_str(" dy=");
                SERIAL_PORT.write_decimal(dy as u32);
                SERIAL_PORT.write_str(" buttons=");
                if self.left_button { SERIAL_PORT.write_str("L"); }
                if self.right_button { SERIAL_PORT.write_str("R"); }
                if self.middle_button { SERIAL_PORT.write_str("M"); }
                SERIAL_PORT.write_str("\n");
            }
        }
    }

    unsafe fn send_mouse_command(&self, command: u8) {
        // Wait for input buffer to be empty
        loop {
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);
            if (status & 0x02) == 0 {
                break;
            }
        }
        
        // Tell controller we want to send to mouse
        core::arch::asm!("out 0x64, al", in("al") 0xD4u8);
        
        // Wait for input buffer to be empty again
        loop {
            let status: u8;
            core::arch::asm!("in al, 0x64", out("al") status);
            if (status & 0x02) == 0 {
                break;
            }
        }
        
        // Send the command
        core::arch::asm!("out 0x60, al", in("al") command);
    }

    pub fn is_left_clicked(&self) -> bool {
        self.left_button
    }

    pub fn is_right_clicked(&self) -> bool {
        self.right_button
    }
}

// Add this method to Graphics struct
impl Graphics {
    // Helper function to safely put pixel with bounds checking (for mouse cursor)
    pub fn put_pixel_safe(&self, x: i64, y: i64, color: u32) {
        if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
            self.put_pixel(x as u64, y as u64, color);
        }
    }
}