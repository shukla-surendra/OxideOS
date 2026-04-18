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
    pub hovered: bool,
}

impl Button {
    pub fn new(x: u64, y: u64, width: u64, height: u64, text: &'static str) -> Self {
        Self {
            x, y, width, height, text,
            bg_color: colors::dark_theme::BUTTON_PRIMARY,
            fg_color: colors::dark_theme::TEXT_PRIMARY,
            pressed: false,
            hovered: false,
        }
    }

    pub fn draw(&self, graphics: &Graphics) {
        let mut bg = if self.pressed {
            colors::dark_theme::BUTTON_PRESSED
        } else if self.hovered {
            colors::dark_theme::BUTTON_HOVER
        } else {
            self.bg_color
        };

        // Button background with rounded corners
        graphics.fill_rounded_rect(self.x, self.y, self.width, self.height, 6, bg);

        // Subtle gradient for depth
        graphics.fill_rect_gradient_v(self.x + 2, self.y + 1, self.width - 4, 2, 0x40FFFFFF, 0x00FFFFFF);

        // Border
        let border_col = if self.hovered { colors::dark_theme::BORDER_FOCUS } else { colors::dark_theme::BORDER };
        graphics.draw_rounded_rect(self.x, self.y, self.width, self.height, 6, border_col, 1);

        // Centered text
        let text_x = self.x + (self.width / 2).saturating_sub((self.text.len() as u64 * 9) / 2);
        let text_y = self.y + (self.height / 2).saturating_sub(4);
        super::fonts::draw_string(graphics, text_x, text_y, self.text, self.fg_color);
    }

    pub fn is_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        mouse_x >= self.x && mouse_x < self.x + self.width &&
        mouse_y >= self.y && mouse_y < self.y + self.height
    }

    pub fn update_hover(&mut self, mouse_x: u64, mouse_y: u64) {
        self.hovered = mouse_x >= self.x && mouse_x < self.x + self.width &&
                       mouse_y >= self.y && mouse_y < self.y + self.height;
    }
}

#[derive(Clone)]
#[derive(Copy)]
pub struct Window {
    pub x: u64,
    pub y: u64,
    pub width: u64,
    pub height: u64,
    pub title: &'static str,
    pub bg_color: u32,
    pub visible: bool,
    pub has_close_button: bool,
}

impl Window {
    pub fn new(x: u64, y: u64, width: u64, height: u64, title: &'static str) -> Self {
        Self {
            x, y, width, height, title,
            bg_color: colors::dark_theme::SURFACE,  // Professional dark background
            visible: true,
            has_close_button: true,
        }
    }

    pub fn draw(&self, graphics: &Graphics) {
        if !self.visible {
            return;
        }

        // Soft shadow
        graphics.draw_soft_shadow(self.x, self.y, self.width, self.height, 10, 0x40);

        // Window background
        graphics.fill_rounded_rect(self.x, self.y, self.width, self.height, 8, self.bg_color);

        // Modern title bar with rounded top corners
        graphics.fill_rounded_rect(self.x, self.y, self.width, 30, 8, colors::ui::TITLEBAR_ACTIVE);
        // Cover bottom rounded corners of title bar
        graphics.fill_rect(self.x, self.y + 20, self.width, 10, colors::ui::TITLEBAR_ACTIVE);

        // Subtle border
        graphics.draw_rounded_rect(self.x, self.y, self.width, self.height, 8, colors::dark_theme::BORDER, 1);

        // Title text
        super::fonts::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::dark_theme::TEXT_PRIMARY);

        // Close button
        if self.has_close_button {
            self.draw_close_button(graphics);
        }
    }

    /// Draw window in unfocused state (dimmed)
    pub fn draw_unfocused(&self, graphics: &Graphics) {
        if !self.visible {
            return;
        }

        // Shadow
        graphics.draw_soft_shadow(self.x, self.y, self.width, self.height, 6, 0x30);

        // Window background
        graphics.fill_rounded_rect(self.x, self.y, self.width, self.height, 8, self.bg_color);

        // Dimmed title bar
        graphics.fill_rounded_rect(self.x, self.y, self.width, 30, 8, colors::ui::TITLEBAR);
        graphics.fill_rect(self.x, self.y + 20, self.width, 10, colors::ui::TITLEBAR);

        // Border
        graphics.draw_rounded_rect(self.x, self.y, self.width, self.height, 8, colors::dark_theme::BORDER, 1);

        // Dimmed title text
        super::fonts::draw_string(
            graphics, 
            self.x + 10, 
            self.y + 11, 
            self.title, 
            colors::dark_theme::TEXT_SECONDARY
        );

        // Close button (still visible)
        if self.has_close_button {
            self.draw_close_button(graphics);
        }
    }

    fn draw_close_button(&self, graphics: &Graphics) {
        let button_x = self.x + self.width - 25;
        let button_y = self.y + 5;
        let button_size = 20;

        // Subtle red background
        graphics.fill_rounded_rect(button_x, button_y, button_size, button_size, 4, colors::dark_theme::ERROR);
        
        // Draw X with proper thickness
        let center_x = button_x + button_size / 2;
        let center_y = button_y + button_size / 2;
        let offset = 4;

        for i in -1..=1 {
            graphics.draw_line(
                (center_x - offset) as i64 + i,
                (center_y - offset) as i64,
                (center_x + offset) as i64 + i,
                (center_y + offset) as i64,
                colors::WHITE,
            );
            graphics.draw_line(
                (center_x + offset) as i64 + i,
                (center_y - offset) as i64,
                (center_x - offset) as i64 + i,
                (center_y + offset) as i64,
                colors::WHITE,
            );
        }
    }

    /// Check if close button was clicked
    pub fn is_close_button_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        if !self.has_close_button || !self.visible {
            return false;
        }

        let button_x = self.x + self.width - 25;
        let button_y = self.y + 5;
        let button_size = 20;

        mouse_x >= button_x && mouse_x < button_x + button_size &&
        mouse_y >= button_y && mouse_y < button_y + button_size
    }

    /// Check if title bar was clicked (for dragging)
    pub fn is_titlebar_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        if !self.visible {
            return false;
        }

        mouse_x >= self.x && mouse_x < self.x + self.width &&
        mouse_y >= self.y && mouse_y < self.y + 30
    }

    /// Close the window
    pub fn close(&mut self) {
        self.visible = false;
    }
}