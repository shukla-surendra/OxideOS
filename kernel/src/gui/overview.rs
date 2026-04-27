//! GNOME-style Activities overview for OxideOS.
//!
//! Triggered by the Activities button.  Shows miniature window thumbnails in a
//! grid.  Clicking a thumbnail brings it to front; clicking its × closes it.

extern crate alloc;
use alloc::vec::Vec;

use crate::gui::graphics::Graphics;
use crate::gui::fonts;
use crate::gui::window_manager::WindowManager;

// ── Thumbnail geometry ────────────────────────────────────────────────────────
const THUMB_W:       u64 = 220;
const THUMB_H:       u64 = 150;
const THUMB_GAP:     u64 = 24;
const THUMB_TOP:     u64 = 120; // vertical start (below search bar)
const THUMB_TITLE_H: u64 = 30;  // title-bar height inside thumbnail

// ── Layout constants ──────────────────────────────────────────────────────────
const TOPBAR_H:  u64 = 50;
const SEARCH_Y:  u64 = 60;
const SEARCH_H:  u64 = 32;

// ── Colours ───────────────────────────────────────────────────────────────────
const OVERLAY_BG: u32 = 0xD4000000;

pub struct Overview {
    pub visible:    bool,
    last_closed:    Option<usize>,
}

impl Overview {
    pub const fn new() -> Self {
        Self { visible: false, last_closed: None }
    }

    pub fn toggle(&mut self) { self.visible = !self.visible; }
    pub fn close(&mut self)  { self.visible = false; }
    pub fn is_visible(&self) -> bool { self.visible }

    /// Returns the window_id of any window the user closed from within the
    /// overview this frame (so the caller can clean up terminals / notepads).
    pub fn take_last_closed(&mut self) -> Option<usize> {
        self.last_closed.take()
    }

    // ── Input ─────────────────────────────────────────────────────────────────

    /// Handle a click while the overview is open.
    /// Returns `(consumed, focused_wid)`:
    /// - `consumed` is always true when the overview is visible.
    /// - `focused_wid` is `Some(id)` when a thumbnail was activated.
    pub fn handle_click(
        &mut self,
        mx: u64, my: u64,
        wm: &mut WindowManager,
        screen_w: u64,
        screen_h: u64,
    ) -> (bool, Option<usize>) {
        if !self.visible { return (false, None); }

        let ids = visible_windows(wm);
        let n   = ids.len();

        if n > 0 {
            let cols = thumb_columns(n);
            for (i, &wid) in ids.iter().enumerate() {
                let (tx, ty) = thumb_pos(i, cols, n, screen_w);

                // Close button: top-right corner of thumbnail
                let cbx = tx + THUMB_W.saturating_sub(16);
                let cby = ty + 8;
                if mx >= cbx && mx < cbx + 16 && my >= cby && my < cby + 16 {
                    wm.remove_window(wid);
                    self.last_closed = Some(wid);
                    // Close overview if no windows remain
                    if visible_windows(wm).is_empty() { self.visible = false; }
                    return (true, None);
                }

                // Thumbnail body click → focus & exit overview
                if mx >= tx && mx < tx + THUMB_W && my >= ty && my < ty + THUMB_H {
                    wm.restore_window(wid);
                    wm.bring_to_front(wid);
                    self.visible = false;
                    return (true, Some(wid));
                }
            }
        }

        // Click outside top-bar or thumbnails → close overview
        if my > TOPBAR_H {
            self.visible = false;
        }
        (true, None)
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager, screen_w: u64, screen_h: u64) {
        if !self.visible { return; }

        // Full-screen dark overlay
        graphics.fill_rect(0, 0, screen_w, screen_h, OVERLAY_BG);

        // ── Top bar ───────────────────────────────────────────────────────────
        graphics.fill_rect(0, 0, screen_w, TOPBAR_H, 0xFF222222);
        graphics.fill_rect(0, TOPBAR_H, screen_w, 1, 0xFF3A3A3A);

        // "Activities" label centred
        let label = "Activities";
        let lx = (screen_w.saturating_sub(label.len() as u64 * 9)) / 2;
        fonts::draw_string(graphics, lx, 17, label, 0xFFEEEEEE);

        // Close / ESC button (top-right)
        let cx = screen_w.saturating_sub(44);
        graphics.fill_rounded_rect(cx, 13, 24, 24, 12, 0xFF484848);
        graphics.draw_rounded_rect(cx, 13, 24, 24, 12, 0xFF606060, 1);
        let ccx = cx as i64 + 12; let ccy = 25i64;
        for d in [-3i64, -2, -1, 1, 2, 3] {
            graphics.put_pixel_safe(ccx + d, ccy + d, 0xFFCCCCCC);
            graphics.put_pixel_safe(ccx + d, ccy - d, 0xFFCCCCCC);
        }
        fonts::draw_string(graphics, cx.saturating_sub(54), 17, "Esc to close", 0xFF666666);

        // ── Search bar ────────────────────────────────────────────────────────
        let sb_w = (screen_w / 3).max(200).min(480);
        let sb_x = (screen_w.saturating_sub(sb_w)) / 2;
        graphics.fill_rounded_rect(sb_x, SEARCH_Y, sb_w, SEARCH_H, 8, 0xFF303030);
        graphics.draw_rounded_rect(sb_x, SEARCH_Y, sb_w, SEARCH_H, 8, 0xFF5294E2, 1);
        // Search icon (simple magnifier)
        graphics.fill_rounded_rect(sb_x + 10, SEARCH_Y + 9, 12, 12, 6, 0xFF505050);
        graphics.draw_rounded_rect(sb_x + 10, SEARCH_Y + 9, 12, 12, 6, 0xFF888888, 1);
        graphics.fill_rect(sb_x + 20, SEARCH_Y + 19, 2, 6, 0xFF888888);
        fonts::draw_string(graphics, sb_x + 30, SEARCH_Y + 12, "Type to search", 0xFF555555);

        // ── Window thumbnails ─────────────────────────────────────────────────
        let ids = visible_windows(wm);
        let n   = ids.len();

        if n == 0 {
            let msg = "No open windows";
            let mx2  = (screen_w.saturating_sub(msg.len() as u64 * 9)) / 2;
            fonts::draw_string(graphics, mx2, screen_h / 2, msg, 0xFF555555);
        } else {
            let cols = thumb_columns(n);
            for (i, &wid) in ids.iter().enumerate() {
                let (tx, ty) = thumb_pos(i, cols, n, screen_w);
                draw_thumbnail(graphics, tx, ty, wm, wid);
            }
        }

        // ── Workspace indicator dots (bottom) ─────────────────────────────────
        let dot_y  = screen_h.saturating_sub(28);
        let dot_x0 = screen_w / 2 - 24;
        // Active workspace (blue, larger)
        graphics.fill_rounded_rect(dot_x0, dot_y, 14, 14, 7, 0xFF5294E2);
        // Inactive workspaces (dim)
        graphics.fill_rounded_rect(dot_x0 + 22, dot_y + 2, 10, 10, 5, 0xFF404040);
        graphics.fill_rounded_rect(dot_x0 + 40, dot_y + 2, 10, 10, 5, 0xFF404040);
    }
}

// ── Thumbnail drawing ─────────────────────────────────────────────────────────

fn draw_thumbnail(graphics: &Graphics, tx: u64, ty: u64, wm: &WindowManager, wid: usize) {
    let Some(win)  = wm.get_window(wid) else { return; };
    let is_focused = wm.get_focused() == Some(wid);

    // Shadow
    graphics.draw_soft_shadow(tx, ty, THUMB_W, THUMB_H, 10, 0x60);

    // Window body
    graphics.fill_rounded_rect(tx, ty, THUMB_W, THUMB_H, 8, 0xFF2D2D2D);

    // Title bar
    let tb_col = if is_focused { 0xFF3A3A3A } else { 0xFF282828 };
    graphics.fill_rounded_rect(tx, ty, THUMB_W, THUMB_TITLE_H, 8, tb_col);
    graphics.fill_rect(tx, ty + THUMB_TITLE_H / 2, THUMB_W, THUMB_TITLE_H / 2, tb_col);
    let accent = if is_focused { 0xFF5294E2 } else { 0xFF3A3A3A };
    graphics.fill_rect(tx, ty + THUMB_TITLE_H - 1, THUMB_W, 1, accent);

    // Mini GNOME control buttons (left, matching real window decorations)
    let bby = ty + (THUMB_TITLE_H - 8) / 2;
    graphics.fill_rounded_rect(tx + 6,  bby, 8, 8, 4, 0xFFED333B);
    graphics.fill_rounded_rect(tx + 18, bby, 8, 8, 4, 0xFFE5A50A);
    graphics.fill_rounded_rect(tx + 30, bby, 8, 8, 4, 0xFF26A269);

    // Title text centered
    let tpx = win.title.len() as u64 * 9;
    let title_x = tx + (THUMB_W.saturating_sub(tpx)) / 2;
    fonts::draw_string(graphics, title_x, ty + THUMB_TITLE_H / 2 - 4, win.title, 0xFFDDDDDD);

    // Content preview — stylised lines representing text
    let content_top = ty + THUMB_TITLE_H;
    graphics.fill_rect(tx, content_top, THUMB_W, THUMB_H - THUMB_TITLE_H, 0xFF1A1A1A);
    // Vary the line color slightly per window for visual distinction
    let line_col = 0xFF222222u32.wrapping_add((wid as u32).wrapping_mul(0x0A0507));
    for row in 0..4u64 {
        let lw = THUMB_W - 16 - row * 20;
        graphics.fill_rounded_rect(tx + 8, content_top + 12 + row * 22, lw.min(THUMB_W - 16), 6, 3, line_col);
    }

    // Outer border
    let border_col = if is_focused { 0xFF5294E2 } else { 0xFF454545 };
    graphics.draw_rounded_rect(tx, ty, THUMB_W, THUMB_H, 8, border_col, 1);

    // Close overlay button (top-right corner of thumbnail)
    let cbx = tx + THUMB_W.saturating_sub(16);
    let cby = ty + 8;
    graphics.fill_rounded_rect(cbx, cby, 16, 16, 8, 0x80000000);
    graphics.draw_rounded_rect(cbx, cby, 16, 16, 8, 0xFF555555, 1);
    let ocx = cbx as i64 + 8; let ocy = cby as i64 + 8;
    for d in [-2i64, -1, 1, 2] {
        graphics.put_pixel_safe(ocx + d, ocy + d, 0xFFAAAAAA);
        graphics.put_pixel_safe(ocx + d, ocy - d, 0xFFAAAAAA);
    }
    graphics.put_pixel_safe(ocx, ocy, 0xFFAAAAAA);
}

// ── Layout helpers ────────────────────────────────────────────────────────────

fn visible_windows(wm: &WindowManager) -> Vec<usize> {
    wm.z_order_slice()
        .iter()
        .copied()
        .filter(|&id| wm.is_window_visible(id))
        .collect()
}

fn thumb_columns(n: usize) -> usize {
    if n <= 2 { 2 } else if n <= 6 { 3 } else { 4 }
}

fn thumb_pos(i: usize, cols: usize, total: usize, screen_w: u64) -> (u64, u64) {
    let _ = total;
    let col = (i % cols) as u64;
    let row = (i / cols) as u64;
    let total_w = cols as u64 * THUMB_W + cols.saturating_sub(1) as u64 * THUMB_GAP;
    let start_x = screen_w.saturating_sub(total_w) / 2;
    let x = start_x + col * (THUMB_W + THUMB_GAP);
    let y = THUMB_TOP + row * (THUMB_H + THUMB_GAP);
    (x, y)
}
