//! GNOME-style Quick Settings panel for OxideOS.
//!
//! Anchored to the top-right corner, just below the taskbar.
//! Opened by clicking the right portion of the taskbar (system-tray area).

use crate::gui::graphics::Graphics;
use crate::gui::fonts;

const PANEL_W:    u64 = 360;
const PANEL_TOP:  u64 = 52;  // gap below 48px taskbar
const PANEL_PAD:  u64 = 12;  // right margin from screen edge
const SECTION_H:  u64 = 74;  // height of one card row
const FOOTER_H:   u64 = 60;  // power buttons footer
const CARD_RADIUS: u64 = 10;
const CARD_BG:    u32 = 0xFF383838;
const CARD_BDR:   u32 = 0xFF4A4A4A;

/// Returned by [`QuickSettings::handle_click`].
#[derive(Clone, Copy, PartialEq)]
pub enum QsAction {
    None,
    Shutdown,
    Reboot,
    Dismiss,
}

pub struct QuickSettings {
    pub visible: bool,
}

impl QuickSettings {
    pub const fn new() -> Self { Self { visible: false } }
    pub fn toggle(&mut self) { self.visible = !self.visible; }
    pub fn close(&mut self) { self.visible = false; }

    /// Returns `true` if `(mx, my)` is inside the right-side "system tray" zone of
    /// the taskbar — used by the main loop to decide whether to open the panel.
    pub fn is_toggle_area(mx: u64, my: u64, screen_w: u64) -> bool {
        my < 48 && mx + 220 > screen_w
    }

    /// Handle a left-click when the panel is open.
    pub fn handle_click(&mut self, mx: u64, my: u64, screen_w: u64) -> QsAction {
        if !self.visible { return QsAction::None; }

        let px = panel_x(screen_w);
        let ph = panel_h();

        // Outside the panel → dismiss
        if mx < px || mx >= px + PANEL_W || my < PANEL_TOP || my >= PANEL_TOP + ph {
            self.visible = false;
            return QsAction::Dismiss;
        }

        // Power footer buttons
        let footer_y = PANEL_TOP + ph - FOOTER_H;
        if my >= footer_y + 14 && my < footer_y + FOOTER_H - 14 {
            let mid = px + PANEL_W / 2;
            if mx >= px + 12 && mx < mid - 4 {
                self.visible = false;
                return QsAction::Shutdown;
            }
            if mx >= mid + 4 && mx < px + PANEL_W - 12 {
                self.visible = false;
                return QsAction::Reboot;
            }
        }

        QsAction::None
    }

    pub fn draw(&self, graphics: &Graphics, screen_w: u64) {
        if !self.visible { return; }

        let px = panel_x(screen_w);
        let ph = panel_h();

        // ── Panel chrome ─────────────────────────────────────────────────────
        graphics.draw_soft_shadow(px, PANEL_TOP, PANEL_W, ph, 16, 0x70);
        graphics.fill_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, 0xFF2E2E2E);
        graphics.draw_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, 0xFF4A4A4A, 1);

        let mut cy = PANEL_TOP + 14;

        // ── Network card ──────────────────────────────────────────────────────
        cy = draw_card_header(graphics, px, cy, "Network", 0xFF5294E2);
        let net_up = crate::kernel::net::is_present();
        let (dot_col, net_str) = if net_up {
            (0xFF26A269, "Connected   10.0.2.15")
        } else {
            (0xFFED333B, "No network interface")
        };
        graphics.fill_rounded_rect(px + 16, cy + 5, 12, 12, 6, dot_col);
        fonts::draw_string(graphics, px + 36, cy + 5, net_str, 0xFFAAAAAA);
        cy += 26 + 14;

        // ── Brightness card ───────────────────────────────────────────────────
        cy = draw_card_header(graphics, px, cy, "Brightness", 0xFFE5A50A);
        draw_slider(graphics, px + 14, cy + 4, PANEL_W - 28, 80);
        cy += 22 + 14;

        // ── Volume card ───────────────────────────────────────────────────────
        cy = draw_card_header(graphics, px, cy, "Volume", 0xFF5294E2);
        draw_slider(graphics, px + 14, cy + 4, PANEL_W - 28, 65);
        cy += 22 + 14;

        // ── Power footer ──────────────────────────────────────────────────────
        let footer_y = PANEL_TOP + ph - FOOTER_H;
        graphics.fill_rect(px + 1, footer_y, PANEL_W - 2, 1, 0xFF3A3A3A);

        let btn_y = footer_y + 14;
        let btn_h = 30u64;
        let half_w = PANEL_W / 2 - 18;

        // Shut Down
        graphics.fill_rounded_rect(px + 12, btn_y, half_w, btn_h, 6, 0xFF3A1414);
        graphics.draw_rounded_rect(px + 12, btn_y, half_w, btn_h, 6, 0xFF6A2020, 1);
        let sd_lbl = "Shut Down";
        let sd_px = sd_lbl.len() as u64 * 9;
        fonts::draw_string(graphics, px + 12 + (half_w.saturating_sub(sd_px)) / 2, btn_y + 11, sd_lbl, 0xFFE05050);

        // Reboot
        let rb_x = px + PANEL_W / 2 + 6;
        graphics.fill_rounded_rect(rb_x, btn_y, half_w, btn_h, 6, 0xFF101828);
        graphics.draw_rounded_rect(rb_x, btn_y, half_w, btn_h, 6, 0xFF1E3A6A, 1);
        let rb_lbl = "Reboot";
        let rb_px = rb_lbl.len() as u64 * 9;
        fonts::draw_string(graphics, rb_x + (half_w.saturating_sub(rb_px)) / 2, btn_y + 11, rb_lbl, 0xFF569CD6);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn panel_x(screen_w: u64) -> u64 { screen_w.saturating_sub(PANEL_W + PANEL_PAD) }
fn panel_h() -> u64 { SECTION_H * 3 + FOOTER_H + 28 }

/// Draws a section label row, returns the new `cy` pointing just after the label.
fn draw_card_header(graphics: &Graphics, px: u64, cy: u64, label: &str, accent: u32) -> u64 {
    graphics.fill_rounded_rect(px + 8, cy, 4, 16, 2, accent);
    fonts::draw_string(graphics, px + 20, cy + 4, label, 0xFFEEEEEE);
    cy + 24
}

/// Horizontal progress bar + draggable thumb (visual only).
fn draw_slider(graphics: &Graphics, x: u64, y: u64, w: u64, pct: u64) {
    let fill_w = w * pct / 100;
    graphics.fill_rounded_rect(x, y + 3, w, 6, 3, 0xFF3A3A3A);
    if fill_w > 0 {
        graphics.fill_rounded_rect(x, y + 3, fill_w, 6, 3, 0xFF5294E2);
    }
    // Thumb
    if fill_w >= 6 {
        graphics.fill_rounded_rect(x + fill_w - 7, y - 1, 14, 14, 7, 0xFFEEEEEE);
        graphics.draw_rounded_rect(x + fill_w - 7, y - 1, 14, 14, 7, 0xFFAAAAAA, 1);
    }
    // Value label
    let pct_str: &str = match pct {
        0..=9  => "0%",
        10..=19 => "10%",
        20..=29 => "20%",
        30..=39 => "30%",
        40..=49 => "40%",
        50..=59 => "50%",
        60..=69 => "65%",
        70..=79 => "70%",
        80..=89 => "80%",
        90..=99 => "90%",
        _       => "100%",
    };
    fonts::draw_string(graphics, x + w + 6, y + 2, pct_str, 0xFF888888);
}
