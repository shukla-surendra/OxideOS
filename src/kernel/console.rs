use super::vga::{VGA_H, SCREEN_WIDTH,  vga_put_at};
use super::vga;

pub struct Console {
    pub color: u8,
    cur_x: usize,
    cur_y: usize,
}

impl Console {
    pub const fn new(color: u8) -> Self {
        Self { color, cur_x: 0, cur_y: 0 }
    }

    /// Call once after clear to make cursor visible.
    pub fn enable_cursor(&self) {
        vga::enable_cursor(13, 15); // block cursor
    }
    pub fn sync_cursor(&self) {
        // cur_x/cur_y are likely usize/u8 in your code; cast to u16 to avoid warnings
        let pos: u16 = (self.cur_y as u16) * SCREEN_WIDTH + (self.cur_x as u16);
        vga::set_cursor_pos(pos);
    }

    pub fn clear(&mut self) {
        for row in 0..vga::VGA_H {
            vga::clear_row(row, self.color);
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
        self.sync_cursor();
    }

    pub fn backspace(&mut self) {
        if self.cur_x > 0 {
            self.cur_x -= 1;
            vga::vga_put_at(b' ', self.color, self.cur_x, self.cur_y);
        }
        self.sync_cursor();
    }

    pub fn putc(&mut self, ch: u8) {
        if ch == b'\n' {
            self.newline();
            return;
        }
        vga_put_at(ch, self.color, self.cur_x, self.cur_y);
        self.cur_x += 1;
        if self.cur_x >= vga::VGA_W {
            self.cur_x = 0;
            if self.cur_y + 1 < VGA_H {
                self.cur_y += 1;
            } else {
                self.scroll();
            }
        }
        self.sync_cursor();
    }

    pub fn write_str(&mut self, s: &str) {
        for &b in s.as_bytes() {
            self.putc(b);
        }
        self.sync_cursor();
    }

    fn scroll(&mut self) {
        // Move rows 1..end up by one
        for row in 1..vga::VGA_H {
            for col in 0..vga::VGA_W {
                let from = ((row * vga::VGA_W + col) * 2) as isize;
                let to   = (((row - 1) * vga::VGA_W + col) * 2) as isize;
                unsafe {
                    let ptr = 0xb8000 as *mut u8;
                    *ptr.offset(to)     = *ptr.offset(from);
                    *ptr.offset(to + 1) = *ptr.offset(from + 1);
                }
            }
        }
        // Clear last row
        vga::clear_row(vga::VGA_H - 1, self.color);
    }
}
