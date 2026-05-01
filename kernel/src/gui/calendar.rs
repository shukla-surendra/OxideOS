//! Drop-down calendar panel for OxideOS.
//!
//! Opens when the user clicks the center clock pill in the taskbar.
//! Shows the current month with today highlighted; prev/next arrows navigate months.
//! Dismissed by clicking outside the panel or clicking the clock again.

use crate::gui::graphics::Graphics;
use crate::gui::fonts;

// ── Panel geometry ────────────────────────────────────────────────────────────
const PANEL_W:    u64 = 280;
const HEADER_H:   u64 = 44;   // month/year row + nav arrows
const DAY_HDR_H:  u64 = 22;   // "Su Mo Tu We Th Fr Sa" row
const CELL_W:     u64 = 34;   // 7 × 34 = 238 px grid width
const CELL_H:     u64 = 28;
const MAX_ROWS:   u64 = 6;
const FOOTER_H:   u64 = 28;   // date summary row at bottom
const GRID_PAD_X: u64 = (PANEL_W - 7 * CELL_W) / 2; // left/right padding so grid is centred

const PANEL_H: u64 = HEADER_H + DAY_HDR_H + MAX_ROWS * CELL_H + FOOTER_H;

// ── Colors ────────────────────────────────────────────────────────────────────
const C_BG:        u32 = 0xFF1A1D2A;
const C_HEADER:    u32 = 0xFF222638;
const C_BORDER:    u32 = 0xFF303550;
const C_ACCENT:    u32 = 0xFF5294E2;
const C_TODAY_BG:  u32 = 0xFF1E4A8A;
const C_TODAY_FG:  u32 = 0xFFFFFFFF;
const C_WEEKEND:   u32 = 0xFF8899CC;
const C_WEEKDAY:   u32 = 0xFFCCDDEE;
const C_OTHER:     u32 = 0xFF505870;  // days outside view month (prev/next spill)
const C_NAV:       u32 = 0xFF8090C0;
const C_NAV_HOV:   u32 = 0xFFAABBDD;
const C_HDR_TEXT:  u32 = 0xFF7080A8;
const C_FOOTER:    u32 = 0xFF151828;
const C_FOOTER_TXT:u32 = 0xFF6070A0;

const MONTH_NAMES: [&str; 12] = [
    "January","February","March","April","May","June",
    "July","August","September","October","November","December",
];

// ── Calendar maths ────────────────────────────────────────────────────────────

/// Day of week for (year, month, day) — 0=Sun … 6=Sat.
/// Tomohiko Sakamoto's algorithm.
fn dow(y: u32, m: u8, d: u8) -> u8 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = y;
    if m < 3 { y = y.wrapping_sub(1); }
    ((y + y/4 - y/100 + y/400 + T[(m - 1) as usize] + d as u32) % 7) as u8
}

fn days_in_month(year: u32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11               => 30,
        2 => if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 29 } else { 28 },
        _ => 30,
    }
}

// ── Public struct ─────────────────────────────────────────────────────────────

pub struct CalendarPanel {
    pub visible: bool,
    /// Month currently shown (1-12).
    view_month: u8,
    /// Year currently shown.
    view_year: u32,
    /// Today's date from the RTC (year, month, day).
    today: (u32, u8, u8),
}

impl CalendarPanel {
    pub const fn new() -> Self {
        Self {
            visible:     false,
            view_month:  1,
            view_year:   2026,
            today:       (2026, 1, 1),
        }
    }

    /// Open the calendar, snapping the view to the current date.
    pub fn open(&mut self) {
        let year          = crate::kernel::rtc::read_year();
        let (_, day, mon) = crate::kernel::rtc::read_date();
        // Apply TZ day delta so "today" matches the displayed date
        let (h24, min, _) = crate::kernel::rtc::read_time();
        let tz            = crate::kernel::rtc::get_tz_offset();
        let total         = h24 as i32 * 60 + min as i32 + tz;
        let day_delta: i32 = if total < 0 { -1 } else if total >= 1440 { 1 } else { 0 };
        let local_day = (day as i32 + day_delta).clamp(1, 31) as u8;

        self.today      = (year, mon, local_day);
        self.view_year  = year;
        self.view_month = mon;
        self.visible    = true;
    }

    pub fn close(&mut self) { self.visible = false; }

    pub fn toggle(&mut self) {
        if self.visible { self.close(); } else { self.open(); }
    }

    // ── Panel position ────────────────────────────────────────────────────────
    fn panel_x(screen_w: u64) -> u64 {
        screen_w / 2 - PANEL_W / 2
    }
    const fn panel_y() -> u64 { 52 }   // just below 48px taskbar + 4px gap

    // ── Navigation button rects ───────────────────────────────────────────────
    fn prev_rect(px: u64) -> (u64, u64, u64, u64) {
        (px + 10, Self::panel_y() + 10, 24, 24)
    }
    fn next_rect(px: u64) -> (u64, u64, u64, u64) {
        (px + PANEL_W - 34, Self::panel_y() + 10, 24, 24)
    }

    // ── Month navigation ──────────────────────────────────────────────────────
    fn prev_month(&mut self) {
        if self.view_month == 1 {
            self.view_month = 12;
            self.view_year  = self.view_year.saturating_sub(1);
        } else {
            self.view_month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.view_month == 12 {
            self.view_month = 1;
            self.view_year += 1;
        } else {
            self.view_month += 1;
        }
    }

    // ── Input ─────────────────────────────────────────────────────────────────

    /// Returns `true` if the click was consumed by the calendar.
    pub fn handle_click(&mut self, mx: u64, my: u64, screen_w: u64) -> bool {
        if !self.visible { return false; }

        let px = Self::panel_x(screen_w);
        let py = Self::panel_y();

        // Click outside → dismiss
        if mx < px || mx >= px + PANEL_W || my < py || my >= py + PANEL_H {
            self.close();
            return false; // don't consume — let the outside click propagate
        }

        // Prev arrow
        let (ax, ay, aw, ah) = Self::prev_rect(px);
        if mx >= ax && mx < ax + aw && my >= ay && my < ay + ah {
            self.prev_month();
            return true;
        }

        // Next arrow
        let (ax, ay, aw, ah) = Self::next_rect(px);
        if mx >= ax && mx < ax + aw && my >= ay && my < ay + ah {
            self.next_month();
            return true;
        }

        // "Today" button in footer — snaps back to current month
        let footer_y = py + HEADER_H + DAY_HDR_H + MAX_ROWS * CELL_H;
        if my >= footer_y && my < footer_y + FOOTER_H {
            self.view_year  = self.today.0;
            self.view_month = self.today.1;
            return true;
        }

        // Day cell click — currently just consumed (no further action)
        true
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, screen_w: u64) {
        if !self.visible { return; }

        let px = Self::panel_x(screen_w);
        let py = Self::panel_y();

        // ── Shadow + panel chrome ─────────────────────────────────────────────
        graphics.draw_soft_shadow(px, py, PANEL_W, PANEL_H, 16, 0x70);
        graphics.fill_rounded_rect(px, py, PANEL_W, PANEL_H, 12, C_BG);
        graphics.draw_rounded_rect(px, py, PANEL_W, PANEL_H, 12, C_BORDER, 1);

        // ── Header ────────────────────────────────────────────────────────────
        graphics.fill_rounded_rect(px, py, PANEL_W, HEADER_H, 12, C_HEADER);
        graphics.fill_rect(px, py + HEADER_H / 2, PANEL_W, HEADER_H / 2, C_HEADER);
        graphics.fill_rect(px, py + HEADER_H - 1, PANEL_W, 1, C_BORDER);

        // Prev "<" arrow button
        let (ax, ay, aw, ah) = Self::prev_rect(px);
        graphics.fill_rounded_rect(ax, ay, aw, ah, 6, 0xFF2A3050);
        graphics.draw_rounded_rect(ax, ay, aw, ah, 6, C_BORDER, 1);
        fonts::draw_string(graphics, ax + 7, ay + 8, "<", C_NAV);

        // Next ">" arrow button
        let (ax, ay, aw, ah) = Self::next_rect(px);
        graphics.fill_rounded_rect(ax, ay, aw, ah, 6, 0xFF2A3050);
        graphics.draw_rounded_rect(ax, ay, aw, ah, 6, C_BORDER, 1);
        fonts::draw_string(graphics, ax + 7, ay + 8, ">", C_NAV);

        // Month + year centred in header
        let mname = MONTH_NAMES[(self.view_month - 1) as usize];
        let mut yr_buf = [0u8; 4];
        let y = self.view_year;
        yr_buf[0] = b'0' + ((y / 1000) % 10) as u8;
        yr_buf[1] = b'0' + ((y / 100)  % 10) as u8;
        yr_buf[2] = b'0' + ((y / 10)   % 10) as u8;
        yr_buf[3] = b'0' + (y           % 10) as u8;
        let yr_str = core::str::from_utf8(&yr_buf).unwrap_or("????");

        // "May 2026" — month name + space + year
        let label_len = mname.len() as u64 * 9 + 9 + 4 * 9; // chars × font width
        let label_x   = px + (PANEL_W - label_len) / 2;
        let label_y   = py + (HEADER_H - 8) / 2;
        fonts::draw_string(graphics, label_x, label_y, mname, 0xFFDDEEFF);
        fonts::draw_string(graphics, label_x + mname.len() as u64 * 9 + 9, label_y, yr_str, C_ACCENT);

        // ── Day-of-week headers ───────────────────────────────────────────────
        let hdr_y = py + HEADER_H;
        graphics.fill_rect(px, hdr_y, PANEL_W, DAY_HDR_H, 0xFF171A28);
        graphics.fill_rect(px, hdr_y + DAY_HDR_H - 1, PANEL_W, 1, C_BORDER);

        const DAY_NAMES: [&str; 7] = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
        for (col, &name) in DAY_NAMES.iter().enumerate() {
            let cx = px + GRID_PAD_X + col as u64 * CELL_W + (CELL_W - 18) / 2;
            let cy = hdr_y + (DAY_HDR_H - 8) / 2;
            let col_c = if col == 0 || col == 6 { C_WEEKEND } else { C_HDR_TEXT };
            fonts::draw_string(graphics, cx, cy, name, col_c);
        }

        // ── Day grid ──────────────────────────────────────────────────────────
        let grid_top = py + HEADER_H + DAY_HDR_H;

        let dim   = days_in_month(self.view_year, self.view_month);
        let first = dow(self.view_year, self.view_month, 1) as u64; // 0=Sun
        let (ty, tm, td) = self.today;

        // Days from previous month (greyed out)
        if first > 0 {
            let prev_month = if self.view_month == 1 { 12 } else { self.view_month - 1 };
            let prev_year  = if self.view_month == 1 { self.view_year - 1 } else { self.view_year };
            let prev_dim   = days_in_month(prev_year, prev_month) as u64;
            for slot in 0..first {
                let day_num = prev_dim - (first - 1 - slot);
                draw_day_cell(graphics, px, grid_top, slot, day_num as u8, C_OTHER, false, false);
            }
        }

        // Current month days
        for d in 1u8..=dim {
            let slot = first + (d as u64 - 1);
            let col = slot % 7;
            let is_today   = self.view_year == ty && self.view_month == tm && d == td;
            let is_weekend = col == 0 || col == 6;
            let fg = if is_today { C_TODAY_FG }
                     else if is_weekend { C_WEEKEND }
                     else { C_WEEKDAY };
            draw_day_cell(graphics, px, grid_top, slot, d, fg, is_today, false);
        }

        // Days from next month (greyed out)
        let used  = first + dim as u64;
        let total = ((used + 6) / 7) * 7; // round up to complete row
        // Always fill to 6 rows for consistent height
        let show_to = (first + MAX_ROWS * 7).max(total);
        let mut next_day = 1u8;
        for slot in used..show_to {
            draw_day_cell(graphics, px, grid_top, slot, next_day, C_OTHER, false, true);
            next_day += 1;
        }

        // Row separators (thin lines between week rows)
        for row in 1..MAX_ROWS {
            let ry = grid_top + row * CELL_H;
            graphics.fill_rect(px + GRID_PAD_X, ry, PANEL_W - GRID_PAD_X * 2, 1, 0xFF202438);
        }

        // ── Footer ────────────────────────────────────────────────────────────
        let footer_y = grid_top + MAX_ROWS * CELL_H;
        graphics.fill_rect(px, footer_y, PANEL_W, 1, C_BORDER);
        graphics.fill_rounded_rect(px, footer_y, PANEL_W, FOOTER_H, 12, C_FOOTER);
        graphics.fill_rect(px, footer_y, PANEL_W, FOOTER_H / 2, C_FOOTER);

        // Show today's date in footer; "Today" button on right if not in current month
        let mut dbuf = [0u8; 10];
        crate::kernel::rtc::format_date(&mut dbuf);
        let date_str = core::str::from_utf8(&dbuf).unwrap_or("--- -- ---");
        fonts::draw_string(graphics, px + 14, footer_y + 10, date_str, C_FOOTER_TXT);

        // "Today" button — only shown when viewing a different month
        if self.view_year != ty || self.view_month != tm {
            let btn_label = "Today";
            let btn_px    = btn_label.len() as u64 * 9;
            let btn_x     = px + PANEL_W - btn_px - 18;
            let btn_y     = footer_y + 5;
            graphics.fill_rounded_rect(btn_x - 4, btn_y, btn_px + 12, 18, 5, 0xFF1E3A60);
            graphics.draw_rounded_rect(btn_x - 4, btn_y, btn_px + 12, 18, 5, C_ACCENT, 1);
            fonts::draw_string(graphics, btn_x, btn_y + 5, btn_label, C_ACCENT);
        }
    }
}

// ── Cell drawing helper ───────────────────────────────────────────────────────

fn draw_day_cell(
    graphics: &Graphics,
    panel_x: u64,
    grid_top: u64,
    slot: u64,
    day_num: u8,
    fg: u32,
    is_today: bool,
    _is_overflow: bool,
) {
    let row = slot / 7;
    let col = slot % 7;
    let cx  = panel_x + GRID_PAD_X + col * CELL_W;
    let cy  = grid_top + row * CELL_H;

    // Today gets a filled accent pill as background
    if is_today {
        let pill_w = 26u64;
        let pill_h = 22u64;
        let pill_x = cx + (CELL_W - pill_w) / 2;
        let pill_y = cy + (CELL_H - pill_h) / 2;
        graphics.fill_rounded_rect(pill_x, pill_y, pill_w, pill_h, 6, C_TODAY_BG);
        graphics.draw_rounded_rect(pill_x, pill_y, pill_w, pill_h, 6, C_ACCENT, 1);
    }

    // Number — center inside cell
    let num_w = if day_num >= 10 { 18u64 } else { 9u64 };
    let num_x = cx + (CELL_W - num_w) / 2;
    let num_y = cy + (CELL_H - 8) / 2;

    // Draw the digit(s)
    let mut buf = [0u8; 2];
    let mut len = 0usize;
    if day_num >= 10 { buf[len] = b'0' + day_num / 10; len += 1; }
    buf[len] = b'0' + day_num % 10; len += 1;
    if let Ok(s) = core::str::from_utf8(&buf[..len]) {
        fonts::draw_string(graphics, num_x, num_y, s, fg);
    }
}
