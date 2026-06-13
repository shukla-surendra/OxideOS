// gui/fonts.rs — text rendering via oxide-gui-core Backend.
// The bitmap font data and per-pixel rendering logic have been removed; all
// drawing now goes through OxideBackend which delegates to oxide-gui-core's
// embedded 8×16 font via the Backend trait's default draw_char / draw_text.

use super::graphics::Graphics;
use super::oxide_backend::OxideBackend;
use oxide_gui_core::Backend;
use oxide_gui_core::font;

pub fn draw_char(graphics: &Graphics, x: u64, y: u64, ch: char, color: u32) {
    let mut backend = OxideBackend::new(graphics);
    backend.draw_char(x as u32, y as u32, ch, color);
}

pub fn draw_string(graphics: &Graphics, x: u64, y: u64, text: &str, color: u32) {
    let mut backend = OxideBackend::new(graphics);
    backend.draw_text(x as u32, y as u32, text, color);
}

/// Returns the pixel width of `text` using the oxide-gui-core font metrics.
pub fn get_text_width(text: &str) -> u64 {
    font::text_width(text) as u64
}

pub fn draw_multiline_string(
    graphics: &Graphics,
    x: u64, y: u64,
    text: &str,
    color: u32,
    line_height: u64,
) {
    let mut backend = OxideBackend::new(graphics);
    let mut cy = y as u32;
    for line in text.lines() {
        backend.draw_text(x as u32, cy, line, color);
        cy = cy.saturating_add(line_height as u32);
    }
}
