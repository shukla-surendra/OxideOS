// src/gui/widgets.rs
// Button has been removed — it was unused externally and its drawing logic
// (gradients, rounded rects with configurable radius) used Graphics primitives
// that are not duplicated in oxide-gui-core.  Canvas::button() in oxide-gui-core
// covers the portable widget use-case.
//
// Window is kept as the data model consumed by WindowManager; its draw methods
// use super::fonts (now backed by oxide-gui-core) and super::colors (now
// re-exporting from oxide-gui-core), so no call-site changes are needed.

use super::graphics::Graphics;
use super::colors;

#[derive(Clone, Copy)]
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
            bg_color: colors::dark_theme::SURFACE,
            visible: true,
            has_close_button: true,
        }
    }

    pub fn draw(&self, graphics: &Graphics) {
        if !self.visible { return; }

        graphics.draw_soft_shadow(self.x, self.y, self.width, self.height, 10, 0x40);
        graphics.fill_rounded_rect(self.x, self.y, self.width, self.height, 8, self.bg_color);
        graphics.fill_rounded_rect(self.x, self.y, self.width, 30, 8, colors::ui::TITLEBAR_ACTIVE);
        graphics.fill_rect(self.x, self.y + 20, self.width, 10, colors::ui::TITLEBAR_ACTIVE);
        graphics.draw_rounded_rect(self.x, self.y, self.width, self.height, 8, colors::dark_theme::BORDER, 1);

        super::fonts::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::dark_theme::TEXT_PRIMARY);

        if self.has_close_button { self.draw_close_button(graphics); }
    }

    pub fn draw_unfocused(&self, graphics: &Graphics) {
        if !self.visible { return; }

        graphics.draw_soft_shadow(self.x, self.y, self.width, self.height, 6, 0x30);
        graphics.fill_rounded_rect(self.x, self.y, self.width, self.height, 8, self.bg_color);
        graphics.fill_rounded_rect(self.x, self.y, self.width, 30, 8, colors::ui::TITLEBAR);
        graphics.fill_rect(self.x, self.y + 20, self.width, 10, colors::ui::TITLEBAR);
        graphics.draw_rounded_rect(self.x, self.y, self.width, self.height, 8, colors::dark_theme::BORDER, 1);

        super::fonts::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::dark_theme::TEXT_SECONDARY);

        if self.has_close_button { self.draw_close_button(graphics); }
    }

    fn draw_close_button(&self, graphics: &Graphics) {
        let bx   = self.x + self.width - 25;
        let by   = self.y + 5;
        let bsz  = 20u64;
        let cx   = bx + bsz / 2;
        let cy   = by + bsz / 2;
        let off  = 4i64;

        graphics.fill_rounded_rect(bx, by, bsz, bsz, 4, colors::dark_theme::ERROR);
        for i in -1i64..=1 {
            graphics.draw_line((cx as i64 - off) + i, cy as i64 - off, (cx as i64 + off) + i, cy as i64 + off, colors::WHITE);
            graphics.draw_line((cx as i64 + off) + i, cy as i64 - off, (cx as i64 - off) + i, cy as i64 + off, colors::WHITE);
        }
    }

    pub fn is_close_button_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        if !self.has_close_button || !self.visible { return false; }
        let bx = self.x + self.width - 25;
        let by = self.y + 5;
        let bsz = 20u64;
        mouse_x >= bx && mouse_x < bx + bsz && mouse_y >= by && mouse_y < by + bsz
    }

    pub fn is_titlebar_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        if !self.visible { return false; }
        mouse_x >= self.x && mouse_x < self.x + self.width && mouse_y >= self.y && mouse_y < self.y + 30
    }

    pub fn close(&mut self) { self.visible = false; }
}
