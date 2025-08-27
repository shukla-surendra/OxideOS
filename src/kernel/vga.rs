pub const VGA_PTR: *mut u8 = 0xb8000 as *mut u8;
pub const VGA_W: usize = 80;
pub const VGA_H: usize = 25;

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
