// OLD: use crate::vga::{VGA_W, VGA_H, vga_put_at, clear_row};
use super::vga::{VGA_W, VGA_H, vga_put_at, clear_row};

pub struct Console {
    pub color: u8,
    cur_x: usize,
    cur_y: usize,
}

impl Console {
    pub const fn new(color: u8) -> Self {
        Self { color, cur_x: 0, cur_y: 0 }
    }

    pub fn clear(&mut self) {
        for row in 0..VGA_H {
            clear_row(row, self.color);
        }
        self.cur_x = 0;
        self.cur_y = 0;
    }

    pub fn newline(&mut self) {
        self.cur_x = 0;
        if self.cur_y + 1 < VGA_H {
            self.cur_y += 1;
        } else {
            self.scroll();
        }
    }

    pub fn backspace(&mut self) {
        if self.cur_x > 0 {
            self.cur_x -= 1;
            vga_put_at(b' ', self.color, self.cur_x, self.cur_y);
        }
    }

    pub fn putc(&mut self, ch: u8) {
        if ch == b'\n' {
            self.newline();
            return;
        }
        vga_put_at(ch, self.color, self.cur_x, self.cur_y);
        self.cur_x += 1;
        if self.cur_x >= VGA_W {
            self.cur_x = 0;
            if self.cur_y + 1 < VGA_H {
                self.cur_y += 1;
            } else {
                self.scroll();
            }
        }
    }

    pub fn write_str(&mut self, s: &str) {
        for &b in s.as_bytes() {
            self.putc(b);
        }
    }

    fn scroll(&mut self) {
        // Move rows 1..end up by one
        for row in 1..VGA_H {
            for col in 0..VGA_W {
                let from = ((row * VGA_W + col) * 2) as isize;
                let to   = (((row - 1) * VGA_W + col) * 2) as isize;
                unsafe {
                    let ptr = 0xb8000 as *mut u8;
                    *ptr.offset(to)     = *ptr.offset(from);
                    *ptr.offset(to + 1) = *ptr.offset(from + 1);
                }
            }
        }
        // Clear last row
        clear_row(VGA_H - 1, self.color);
    }
}
