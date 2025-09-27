use super::graphics::{Graphics};

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

pub fn draw_char(graphics: &Graphics, x: u64, y: u64, ch: char, color: u32) {
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

pub fn draw_string(graphics: &Graphics, x: u64, y: u64, text: &str, color: u32) {
    let mut current_x = x;
    for ch in text.chars() {
        draw_char(graphics, current_x, y, ch, color);
        current_x += 9; // 8 pixels + 1 pixel spacing
    }
}

