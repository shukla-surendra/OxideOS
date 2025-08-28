use super::io::{out8, in8};

pub const VGA_PTR: *mut u8 = 0xb8000 as *mut u8;
pub const VGA_W: usize = 80;
pub const VGA_H: usize = 25;
const CRTC_INDEX: u16 = 0x3D4; // color text mode
const CRTC_DATA:  u16 = 0x3D5;
pub const SCREEN_WIDTH: u16 = 80;
// pub const SCREEN_HEIGHT: u16 = 25;


pub fn enable_cursor(start: u8, end: u8) {
    // Cursor Start (0x0A)
    out8(CRTC_INDEX, 0x0A);
    let prev = in8(CRTC_DATA);
    out8(CRTC_DATA, (prev & 0xC0) | (start & 0x1F));

    // Cursor End (0x0B)
    out8(CRTC_INDEX, 0x0B);
    let prev = in8(CRTC_DATA);
    out8(CRTC_DATA, (prev & 0xE0) | (end & 0x1F));
}

/// Put a character at (x, y) with a color attribute.
pub fn vga_put_at(ch: u8, color: u8, x: usize, y: usize) {
    let idx = (y * VGA_W + x) * 2;
    unsafe {
        *VGA_PTR.add(idx) = ch;
        *VGA_PTR.add(idx + 1) = color;
    }
}

/// Clear a whole row to spaces with the given color.
pub fn clear_row(row: usize, color: u8) {
    for col in 0..VGA_W {
        vga_put_at(b' ', color, col, row);
    }
}

/// Set the hardware cursor position to a linear cell offset (row*80 + col).
pub fn set_cursor_pos(pos: u16) {
    // low byte
    out8(CRTC_INDEX, 0x0F);
    out8(CRTC_DATA, (pos & 0x00FF) as u8);
    // high byte
    out8(CRTC_INDEX, 0x0E);
    out8(CRTC_DATA, (pos >> 8) as u8);
}
