// src/gui/widgets.rs

use super::graphics::{Graphics};
use super::colors;

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
        super::fonts::draw_string(graphics, text_x, text_y, self.text, self.fg_color);
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
        super::fonts::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::WHITE);
    }
}