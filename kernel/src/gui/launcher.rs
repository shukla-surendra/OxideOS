//! Program launcher panel for OxideOS.
//!
//! Rendered as a left-anchored overlay panel (not a window).
//! Opened by clicking the Activities button; dismissed by clicking outside.
//! Supports scrolling when the program list is taller than the panel.

use crate::gui::fonts;
use crate::gui::graphics::Graphics;

// ── Panel geometry ────────────────────────────────────────────────────────────
const TASKBAR_H:  u64 = 48;
const PANEL_W:    u64 = 260;
const PANEL_PAD:  u64 = 10;  // left margin from screen edge
const ROW_H:      u64 = 56;  // height of one program row
const SEC_LABEL_H:u64 = 22;  // height of a section divider row
const SCROLL_BTN: u64 = 28;  // height of the scroll arrow buttons at bottom

// ── Colors ────────────────────────────────────────────────────────────────────
const C_BG:       u32 = 0xFF1E1E2E;
const C_SURFACE:  u32 = 0xFF252535;
const C_BORDER:   u32 = 0xFF383858;
const C_HOVER:    u32 = 0xFF2A2A44;
const C_LAUNCH:   u32 = 0xFF1A3A5A;
const C_SEC:      u32 = 0xFF5060A0;
const C_SEC_LINE: u32 = 0xFF2A2A44;
const C_TEXT:     u32 = 0xFFDDEEFF;
const C_DESC:     u32 = 0xFF7080A0;
const C_SCROLL:   u32 = 0xFF303050;
const C_SCROLL_A: u32 = 0xFF5060A0;

// ── Program entries ───────────────────────────────────────────────────────────
struct Entry {
    name:    &'static str,
    desc:    &'static str,
    accent:  u32,
    section: u8,  // 0=Apps, 1=Programs, 2=Interactive
}

const ENTRIES: &[Entry] = &[
    // Apps
    Entry { name: "terminal",    desc: "GUI Terminal",          accent: 0xFF007ACC, section: 0 },
    Entry { name: "notepad",     desc: "GUI Text Editor",       accent: 0xFFE5A50A, section: 0 },
    Entry { name: "filemanager", desc: "GUI File Manager",      accent: 0xFF8844CC, section: 0 },
    Entry { name: "sh",          desc: "Minimal Shell",         accent: 0xFF1A9A50, section: 0 },
    // Programs
    Entry { name: "hello_rust",  desc: "Hello from Rust",       accent: 0xFFCC6633, section: 1 },
    Entry { name: "sysinfo",     desc: "System Information",    accent: 0xFF3399AA, section: 1 },
    Entry { name: "fib",         desc: "Fibonacci Sequence",    accent: 0xFF2277CC, section: 1 },
    Entry { name: "primes",      desc: "Primes up to 100",      accent: 0xFF227788, section: 1 },
    Entry { name: "hello",       desc: "Hello World (asm)",     accent: 0xFF886644, section: 1 },
    Entry { name: "counter",     desc: "Count 1–9",             accent: 0xFF446688, section: 1 },
    Entry { name: "countdown",   desc: "Countdown 10→1",        accent: 0xFF996633, section: 1 },
    Entry { name: "spinner",     desc: "Spinner Animation",     accent: 0xFF886699, section: 1 },
    Entry { name: "filetest",    desc: "File I/O Demo",         accent: 0xFF885533, section: 1 },
    // Interactive
    Entry { name: "input",       desc: "Stdin Echo (Ctrl-C)",   accent: 0xFF558844, section: 2 },
];

const SECTION_NAMES: &[&str] = &["Applications", "Programs", "Interactive"];

// ── Total content height ──────────────────────────────────────────────────────
fn total_content_h() -> u64 {
    let mut h = 0u64;
    let mut prev_sec: Option<u8> = None;
    for e in ENTRIES {
        if prev_sec != Some(e.section) {
            h += SEC_LABEL_H;
            prev_sec = Some(e.section);
        }
        h += ROW_H;
    }
    h
}

// ── Launcher panel ────────────────────────────────────────────────────────────
pub struct LauncherApp {
    pub visible:      bool,
    scroll_offset:    u64,  // pixels scrolled down
    hovered:          Option<usize>,
    launched_idx:     Option<usize>,
    launched_frames:  u8,
}

impl LauncherApp {
    pub const fn new() -> Self {
        Self {
            visible:         false,
            scroll_offset:   0,
            hovered:         None,
            launched_idx:    None,
            launched_frames: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        if !self.visible { self.scroll_offset = 0; }
    }
    pub fn close(&mut self) {
        self.visible      = false;
        self.scroll_offset = 0;
        self.hovered      = None;
    }

    // ── Geometry ──────────────────────────────────────────────────────────────

    fn panel_x() -> u64 { PANEL_PAD }
    fn panel_y() -> u64 { TASKBAR_H }

    /// Visible content height = panel height minus the header and scroll buttons.
    fn visible_h(screen_h: u64) -> u64 {
        screen_h.saturating_sub(TASKBAR_H + SCROLL_BTN + 2)
    }

    /// y-coordinate of a row inside the content (before clipping/scroll).
    /// Returns (row_y, is_section_label).
    fn row_positions() -> [(u64, bool, usize); /* ENTRIES.len() + sections */ 32] {
        let mut out = [(0u64, false, 0usize); 32];
        let mut idx = 0usize;
        let mut y   = 0u64;
        let mut prev_sec: Option<u8> = None;
        for (ei, e) in ENTRIES.iter().enumerate() {
            if prev_sec != Some(e.section) {
                out[idx] = (y, true, e.section as usize); // section label
                idx += 1;
                y += SEC_LABEL_H;
                prev_sec = Some(e.section);
            }
            out[idx] = (y, false, ei); // program row
            idx += 1;
            y += ROW_H;
        }
        out
    }

    // ── Scroll helpers ────────────────────────────────────────────────────────

    fn max_scroll(screen_h: u64) -> u64 {
        total_content_h().saturating_sub(Self::visible_h(screen_h))
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(ROW_H);
    }

    pub fn scroll_down(&mut self, screen_h: u64) {
        let max = Self::max_scroll(screen_h);
        self.scroll_offset = (self.scroll_offset + ROW_H).min(max);
    }

    // ── Scroll-button hit areas ───────────────────────────────────────────────
    fn scroll_up_rect(screen_h: u64) -> (u64, u64, u64, u64) {
        let py = Self::panel_y() + Self::visible_h(screen_h) + 1;
        (Self::panel_x(), py, PANEL_W / 2 - 1, SCROLL_BTN)
    }
    fn scroll_down_rect(screen_h: u64) -> (u64, u64, u64, u64) {
        let py = Self::panel_y() + Self::visible_h(screen_h) + 1;
        (Self::panel_x() + PANEL_W / 2 + 1, py, PANEL_W / 2 - 1, SCROLL_BTN)
    }

    // ── Mouse events ──────────────────────────────────────────────────────────

    /// Returns `true` if the launcher consumed the event (click or dismiss).
    pub fn is_toggle_area(mx: u64, my: u64) -> bool {
        // Left strip of the taskbar can re-open the launcher (handled elsewhere)
        my < TASKBAR_H && mx < 150
    }

    /// Update hover. Call every mouse-move frame.
    pub fn handle_mouse_move(&mut self, mx: u64, my: u64, screen_h: u64) -> bool {
        if !self.visible { return false; }
        let prev = self.hovered;
        self.hovered = self.entry_at(mx, my, screen_h);
        self.hovered != prev
    }

    /// Handle a left-click. Returns the program name to launch, if any.
    pub fn handle_click(&mut self, mx: u64, my: u64, screen_h: u64) -> Option<&'static str> {
        if !self.visible { return None; }

        let px = Self::panel_x();

        // Click outside panel → dismiss
        let panel_bottom = Self::panel_y() + Self::visible_h(screen_h) + SCROLL_BTN + 2;
        if mx < px || mx >= px + PANEL_W || my < Self::panel_y() || my >= panel_bottom {
            self.close();
            return None;
        }

        // Scroll buttons
        let (ux, uy, uw, uh) = Self::scroll_up_rect(screen_h);
        if mx >= ux && mx < ux+uw && my >= uy && my < uy+uh {
            self.scroll_up();
            return None;
        }
        let (dx, dy, dw, dh) = Self::scroll_down_rect(screen_h);
        if mx >= dx && mx < dx+dw && my >= dy && my < dy+dh {
            self.scroll_down(screen_h);
            return None;
        }

        // Program entry
        if let Some(idx) = self.entry_at(mx, my, screen_h) {
            self.launched_idx    = Some(idx);
            self.launched_frames = 0;
            let name = ENTRIES[idx].name;
            self.close();
            return Some(name);
        }
        None
    }

    fn entry_at(&self, mx: u64, my: u64, screen_h: u64) -> Option<usize> {
        if !self.visible { return None; }
        let px = Self::panel_x();
        let py = Self::panel_y();
        let vh = Self::visible_h(screen_h);
        if mx < px || mx >= px + PANEL_W { return None; }
        if my < py || my >= py + vh       { return None; }

        let rel_y = my - py + self.scroll_offset;
        let rows = Self::row_positions();
        for &(ry, is_sec, idx) in rows.iter() {
            if ry == 0 && is_sec == false && idx == 0 && ry + ROW_H == 0 { break; }
            if is_sec { continue; }
            if rel_y >= ry && rel_y < ry + ROW_H {
                if idx < ENTRIES.len() { return Some(idx); }
            }
        }
        None
    }

    /// Advance animation state. Returns `true` when a redraw is needed.
    pub fn tick(&mut self) -> bool {
        if self.launched_idx.is_some() {
            self.launched_frames += 1;
            if self.launched_frames > 10 {
                self.launched_idx    = None;
                self.launched_frames = 0;
                return true;
            }
        }
        false
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, screen_h: u64) {
        if !self.visible { return; }

        let px = Self::panel_x();
        let py = Self::panel_y();
        let vh = Self::visible_h(screen_h);

        // ── Panel background ──────────────────────────────────────────────────
        graphics.draw_soft_shadow(px, py, PANEL_W, vh + SCROLL_BTN + 2, 16, 0x70);
        graphics.fill_rounded_rect(px, py, PANEL_W, vh, 10, C_BG);
        graphics.draw_rounded_rect(px, py, PANEL_W, vh, 10, C_BORDER, 1);

        // ── Clip region: only draw rows visible in [scroll_offset, scroll_offset+vh) ──
        let rows = Self::row_positions();
        let clip_top    = py;
        let clip_bottom = py + vh;

        let mut prev_sec: Option<u8> = None;
        for &(ry, is_sec, idx) in rows.iter() {
            // Stop iterating at the end of the array sentinel
            if ry == 0 && !is_sec && idx == 0 && prev_sec.is_some() { break; }

            let abs_y = py + ry - self.scroll_offset;

            if is_sec {
                // Section label row
                let sec_idx = idx;
                if prev_sec == Some(sec_idx as u8) { continue; }
                prev_sec = Some(sec_idx as u8);
                let row_bottom = abs_y + SEC_LABEL_H;
                if row_bottom <= clip_top || abs_y >= clip_bottom { continue; }
                // Section divider line
                graphics.fill_rect(px + 10, abs_y + SEC_LABEL_H - 1, PANEL_W - 20, 1, C_SEC_LINE);
                // Label
                fonts::draw_string(graphics, px + 12, abs_y + 6,
                    SECTION_NAMES[sec_idx], C_SEC);
            } else {
                // Program row
                if idx >= ENTRIES.len() { break; }
                let row_bottom = abs_y + ROW_H;
                if row_bottom <= clip_top || abs_y >= clip_bottom { continue; }
                self.draw_row(graphics, px, abs_y, idx, clip_top, clip_bottom);
            }
        }

        // ── Scroll bar indicator ──────────────────────────────────────────────
        let total_h = total_content_h();
        if total_h > vh {
            let bar_h = (vh * vh / total_h).max(20);
            let bar_y = py + (vh.saturating_sub(bar_h)) * self.scroll_offset
                        / Self::max_scroll(screen_h + TASKBAR_H + SCROLL_BTN + 2).max(1);
            graphics.fill_rounded_rect(px + PANEL_W - 5, bar_y, 4, bar_h, 2, C_SCROLL_A);
        }

        // ── Scroll buttons at bottom ──────────────────────────────────────────
        let btn_y = py + vh + 1;
        let hw = PANEL_W / 2 - 1;
        // Up arrow
        let can_up = self.scroll_offset > 0;
        let up_bg  = if can_up { C_SCROLL_A } else { C_SCROLL };
        graphics.fill_rounded_rect(px, btn_y, hw, SCROLL_BTN, 6, up_bg);
        graphics.fill_rect(px + hw/2 - 4, btn_y + 10, 8, 2, C_TEXT);
        graphics.fill_rect(px + hw/2 - 2, btn_y + 7,  4, 3, C_TEXT); // top spike
        graphics.fill_rect(px + hw/2 - 0, btn_y + 5,  1, 2, C_TEXT);
        // Down arrow
        let can_dn = self.scroll_offset < Self::max_scroll(screen_h + TASKBAR_H + SCROLL_BTN + 2);
        let dn_bg  = if can_dn { C_SCROLL_A } else { C_SCROLL };
        graphics.fill_rounded_rect(px + hw + 2, btn_y, hw, SCROLL_BTN, 6, dn_bg);
        graphics.fill_rect(px + hw + 2 + hw/2 - 4, btn_y + 8,  8, 2, C_TEXT);
        graphics.fill_rect(px + hw + 2 + hw/2 - 2, btn_y + 10, 4, 3, C_TEXT); // bottom spike
        graphics.fill_rect(px + hw + 2 + hw/2,     btn_y + 13, 1, 2, C_TEXT);
    }

    fn draw_row(&self, graphics: &Graphics, px: u64, ay: u64, idx: usize,
                clip_top: u64, clip_bottom: u64) {
        let entry = &ENTRIES[idx];
        let is_hover    = self.hovered == Some(idx);
        let is_launched = self.launched_idx == Some(idx);

        // Visible slice of this row (for clipping)
        let row_top    = ay.max(clip_top);
        let row_bottom = (ay + ROW_H).min(clip_bottom);
        if row_bottom <= row_top { return; }

        let bg = if is_launched { C_LAUNCH }
                 else if is_hover { C_HOVER }
                 else { C_BG };

        // Row background (only the visible slice)
        graphics.fill_rect(px + 1, row_top, PANEL_W - 2, row_bottom - row_top, bg);

        // Only draw the row interior if the full row is visible (avoids clipping artifacts)
        if ay >= clip_top && ay + ROW_H <= clip_bottom {
            // Left accent stripe
            graphics.fill_rounded_rect(px + 8, ay + 10, 4, ROW_H - 20, 2, entry.accent);

            // Separator line above row
            graphics.fill_rect(px + 16, ay, PANEL_W - 24, 1, C_SEC_LINE);

            // Program name
            fonts::draw_string(graphics, px + 20, ay + 10, entry.name, C_TEXT);
            // Description
            fonts::draw_string(graphics, px + 20, ay + 26, entry.desc, C_DESC);

            // Right arrow indicator on hover
            if is_hover || is_launched {
                let arrow_x = px + PANEL_W - 18;
                let arrow_y = ay + ROW_H / 2;
                graphics.fill_rect(arrow_x,     arrow_y - 1, 8, 2, entry.accent);
                graphics.fill_rect(arrow_x + 5, arrow_y - 4, 2, 4, entry.accent);
                graphics.fill_rect(arrow_x + 5, arrow_y + 1, 2, 4, entry.accent);
            }
        }
    }
}
