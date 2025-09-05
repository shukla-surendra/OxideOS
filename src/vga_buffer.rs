#![no_std]

// VGA buffer driver for text mode (80x25).
// - Uses spin::Mutex for safe global access in kernel contexts.
// - Writes to VGA memory as volatile u16 words (color << 8 | ascii).
// - Exposes safe `write_vga_fmt` plus `print!`/`println!` macros.

use core::fmt::{self, Write};
use core::ptr;
use spin::Mutex;

// Constants for VGA buffer
const VGA_BUFFER_ADDRESS: *mut u16 = 0xb8000 as *mut u16;
const VGA_WIDTH: usize = 80;
const VGA_HEIGHT: usize = 25;

// Color codes
#[allow(dead_code)]
#[repr(u8)]
enum Color {
    Black = 0x0,
    White = 0xF,
    LightGreen = 0xA,
}

// VGA buffer state
pub struct VgaBuffer {
    row: usize,
    col: usize,
    color: u8,
}

impl VgaBuffer {
    // Create a new VGA buffer with default settings
    pub const fn new() -> Self {
        VgaBuffer {
            row: 0,
            col: 0,
            color: Color::White as u8,
        }
    }

    // Clear the screen
    pub fn clear(&mut self) {
        for y in 0..VGA_HEIGHT {
            for x in 0..VGA_WIDTH {
                self.write_cell(x, y, b' ', self.color);
            }
        }
        self.row = 0;
        self.col = 0;
    }

    // Low-level: write a cell (character + color) using a volatile u16 write
    fn write_cell(&mut self, x: usize, y: usize, character: u8, color: u8) {
        let index = y * VGA_WIDTH + x;
        let word: u16 = ((color as u16) << 8) | (character as u16);
        unsafe {
            ptr::write_volatile(VGA_BUFFER_ADDRESS.add(index), word);
        }
    }

    // Low-level: read a cell (u16)
    fn read_cell_word(&self, x: usize, y: usize) -> u16 {
        let index = y * VGA_WIDTH + x;
        unsafe { ptr::read_volatile(VGA_BUFFER_ADDRESS.add(index)) }
    }

    // Write a string at the current cursor position
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
    }

    // Write a single byte, handling newlines and wrapping
    fn write_byte(&mut self, byte: u8) {
        if byte == b'\n' {
            self.new_line();
        } else {
            if self.col >= VGA_WIDTH {
                self.new_line();
            }
            self.write_cell(self.col, self.row, byte, self.color);
            self.col += 1;
        }
    }

    // Move to the next line, scrolling if needed
    fn new_line(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= VGA_HEIGHT {
            self.scroll();
            self.row = VGA_HEIGHT - 1;
        }
    }

    // Scroll the screen up by one line (move lines 1.. to 0..)
    fn scroll(&mut self) {
        // copy each row+1 to row
        for y in 0..(VGA_HEIGHT - 1) {
            for x in 0..VGA_WIDTH {
                let src_index = (y + 1) * VGA_WIDTH + x;
                let dst_index = y * VGA_WIDTH + x;
                unsafe {
                    let word = ptr::read_volatile(VGA_BUFFER_ADDRESS.add(src_index));
                    ptr::write_volatile(VGA_BUFFER_ADDRESS.add(dst_index), word);
                }
            }
        }

        // clear last line
        for x in 0..VGA_WIDTH {
            self.write_cell(x, VGA_HEIGHT - 1, b' ', self.color);
        }
    }

    // Write an integer as hex (0xXXXXXXXX)
    pub fn write_hex(&mut self, value: u32) {
        let hex_chars = b"0123456789ABCDEF";
        self.write_string("0x");
        for i in 0..8 {
            let shift = 28 - (i * 4);
            let nibble = ((value >> shift) & 0xF) as usize;
            self.write_byte(hex_chars[nibble]);
        }
    }

    // Public helper to write a single char (ASCII or '?')
    pub fn write_char_public(&mut self, c: char) {
        if c.is_ascii() {
            self.write_byte(c as u8);
        } else {
            self.write_byte(b'?');
        }
    }
}

impl fmt::Write for VgaBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

// Global VGA instance protected by a spin::Mutex for safe concurrent access
pub static VGA: Mutex<VgaBuffer> = Mutex::new(VgaBuffer::new());

// Safe wrapper to write formatted args to VGA
pub fn write_vga_fmt(args: fmt::Arguments) -> fmt::Result {
    // Access is protected by the mutex; volatile memory ops are inside VgaBuffer methods.
    let mut vga = VGA.lock();
    vga.write_fmt(args)
}

// Macros for printing — they use the crate path `vga_buffer`.
// If this file is `src/vga_buffer.rs`, the path `$crate::vga_buffer::write_vga_fmt` will work.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ({
        $crate::vga_buffer::write_vga_fmt(format_args!($($arg)*)).unwrap();
    });
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

// Initialize the VGA (clear screen)
pub fn init() {
    VGA.lock().clear();
}
