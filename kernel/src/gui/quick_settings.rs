//! Settings panel for OxideOS — tabbed, with functional sliders.
//! Opened by clicking the right-side system tray area of the taskbar.

use crate::gui::graphics::Graphics;
use crate::gui::fonts;

// ── Panel geometry ────────────────────────────────────────────────────────────
const PANEL_W:    u64 = 380;
const PANEL_TOP:  u64 = 52;
const PANEL_PAD:  u64 = 10;   // right margin from screen edge
const TAB_H:      u64 = 38;
const CONTENT_PAD:u64 = 16;
const FOOTER_H:   u64 = 58;

const SLIDER_H:       u64 = 20;
const SLIDER_TRACK_H: u64 = 6;

// ── Colors ────────────────────────────────────────────────────────────────────
const C_BG:      u32 = 0xFF252525;
const C_SURFACE: u32 = 0xFF303030;
const C_BORDER:  u32 = 0xFF424242;
const C_ACCENT:  u32 = 0xFF5294E2;
const C_ACCENT2: u32 = 0xFF26A269;
const C_TEXT:    u32 = 0xFFEEEEEE;
const C_DIM:     u32 = 0xFF888888;
const C_TRACK:   u32 = 0xFF3A3A3A;

// ── Timezone table ────────────────────────────────────────────────────────────
// (display label, city, offset_in_minutes)
const TIMEZONES: &[(&str, &str, i32)] = &[
    ("UTC-8",    "Los Angeles", -480),
    ("UTC-7",    "Denver",      -420),
    ("UTC-6",    "Chicago",     -360),
    ("UTC-5",    "New York",    -300),
    ("UTC+0",    "London",         0),
    ("UTC+1",    "Paris",         60),
    ("UTC+2",    "Athens",       120),
    ("UTC+3",    "Moscow",       180),
    ("UTC+5:30", "Mumbai",       330),
    ("UTC+8",    "Beijing",      480),
    ("UTC+9",    "Tokyo",        540),
    ("UTC+10",   "Sydney",       600),
];

// ── Tabs ──────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
pub enum QsTab { Display, Sound, Network, Time, About }

/// Returned by handle_click / handle_mouse_move.
#[derive(Clone, Copy, PartialEq)]
pub enum QsAction { None, Shutdown, Reboot, Dismiss }

pub struct QuickSettings {
    pub visible: bool,
    pub tab: QsTab,
    pub brightness: u8,   // 0–100
    pub volume:     u8,   // 0–100
    dragging_brightness: bool,
    dragging_volume:     bool,
    hovered_shutdown: bool,
    hovered_reboot:   bool,
}

impl QuickSettings {
    pub const fn new() -> Self {
        Self {
            visible: false,
            tab: QsTab::Display,
            brightness: 80,
            volume: 65,
            dragging_brightness: false,
            dragging_volume:     false,
            hovered_shutdown: false,
            hovered_reboot:   false,
        }
    }

    pub fn toggle(&mut self) { self.visible = !self.visible; }
    pub fn close(&mut self)  {
        self.visible = false;
        self.dragging_brightness = false;
        self.dragging_volume = false;
    }

    /// Open directly on the Time tab (e.g. when the clock is clicked).
    pub fn open_time_tab(&mut self) {
        self.visible = true;
        self.tab     = QsTab::Time;
    }

    // ── Geometry helpers ──────────────────────────────────────────────────────
    fn panel_x(screen_w: u64) -> u64 { screen_w.saturating_sub(PANEL_W + PANEL_PAD) }
    fn panel_h() -> u64 { TAB_H + 240 + FOOTER_H }

    fn slider_rect(px: u64, cy: u64) -> (u64, u64, u64) {
        (px + CONTENT_PAD, cy + 4, PANEL_W - CONTENT_PAD * 2)
    }

    fn value_from_click(slider_x: u64, slider_w: u64, mx: u64) -> u8 {
        let rel = mx.saturating_sub(slider_x).min(slider_w);
        (rel * 100 / slider_w) as u8
    }

    fn tab_rect(px: u64, tab: QsTab) -> (u64, u64, u64, u64) {
        let tw = PANEL_W / 5;
        let tx = px + tw * match tab {
            QsTab::Display => 0,
            QsTab::Sound   => 1,
            QsTab::Network => 2,
            QsTab::Time    => 3,
            QsTab::About   => 4,
        };
        (tx, PANEL_TOP, tw, TAB_H)
    }

    // ── Toggle area (right side of taskbar) ───────────────────────────────────
    pub fn is_toggle_area(mx: u64, my: u64, screen_w: u64) -> bool {
        my < 48 && mx + 200 > screen_w
    }

    // ── TZ button layout helpers ──────────────────────────────────────────────
    // Timezone grid starts at content_y + TZ_GRID_OFFSET_Y.
    // 2 columns × 6 rows, each cell BTN_ROW_H tall, BTN_W wide.
    const TZ_NOTICE_H:    u64 = 44;
    const TZ_SECTION_H:   u64 = 26;
    const TZ_GRID_OFF_Y:  u64 = Self::TZ_NOTICE_H + Self::TZ_SECTION_H + 4;
    const TZ_BTN_ROW_H:   u64 = 28;
    const TZ_BTN_H:       u64 = 24;
    const TZ_BTN_W:       u64 = (PANEL_W - CONTENT_PAD * 2 - 8) / 2; // ~166px

    fn tz_button_rect(px: u64, content_y: u64, idx: usize) -> (u64, u64, u64, u64) {
        let row = (idx / 2) as u64;
        let col = (idx % 2) as u64;
        let gap = 8u64;
        let bx = px + CONTENT_PAD + col * (Self::TZ_BTN_W + gap);
        let by = content_y + Self::TZ_GRID_OFF_Y + row * Self::TZ_BTN_ROW_H;
        (bx, by, Self::TZ_BTN_W, Self::TZ_BTN_H)
    }

    fn current_tz_index() -> usize {
        let offset = crate::kernel::rtc::get_tz_offset();
        for (i, &(_, _, ofs)) in TIMEZONES.iter().enumerate() {
            if ofs == offset { return i; }
        }
        usize::MAX
    }

    // ── Input handling ────────────────────────────────────────────────────────

    pub fn handle_mouse_move(&mut self, mx: u64, my: u64, screen_w: u64, left_held: bool) {
        if !self.visible { return; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        let footer_y = PANEL_TOP + ph - FOOTER_H;
        let btn_y = footer_y + 12;
        let btn_h = 34u64;
        let half_w = PANEL_W / 2 - 18;
        self.hovered_shutdown = mx >= px + 12 && mx < px + 12 + half_w
                             && my >= btn_y && my < btn_y + btn_h;
        let rb_x = px + PANEL_W / 2 + 6;
        self.hovered_reboot   = mx >= rb_x && mx < rb_x + half_w
                             && my >= btn_y && my < btn_y + btn_h;

        if left_held && self.tab == QsTab::Display {
            let content_y = PANEL_TOP + TAB_H + 12;
            let bcy = content_y + 44;
            let (sx, sy, sw) = Self::slider_rect(px, bcy);
            if my >= sy && my < sy + SLIDER_H && mx >= sx && mx <= sx + sw {
                self.brightness = Self::value_from_click(sx, sw, mx);
                self.dragging_brightness = true;
            }
        }
        if left_held && self.tab == QsTab::Sound {
            let content_y = PANEL_TOP + TAB_H + 12;
            let vcy = content_y + 44;
            let (sx, sy, sw) = Self::slider_rect(px, vcy);
            if my >= sy && my < sy + SLIDER_H && mx >= sx && mx <= sx + sw {
                self.volume = Self::value_from_click(sx, sw, mx);
                self.dragging_volume = true;
            }
        }
        if !left_held {
            self.dragging_brightness = false;
            self.dragging_volume     = false;
        }
    }

    pub fn handle_click(&mut self, mx: u64, my: u64, screen_w: u64) -> QsAction {
        if !self.visible { return QsAction::None; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        if mx < px || mx >= px + PANEL_W || my < PANEL_TOP || my >= PANEL_TOP + ph {
            self.visible = false;
            return QsAction::Dismiss;
        }

        // Tab bar
        for &t in &[QsTab::Display, QsTab::Sound, QsTab::Network, QsTab::Time, QsTab::About] {
            let (tx, ty, tw, th) = Self::tab_rect(px, t);
            if mx >= tx && mx < tx + tw && my >= ty && my < ty + th {
                self.tab = t;
                return QsAction::None;
            }
        }

        // Footer
        let footer_y = PANEL_TOP + ph - FOOTER_H;
        let btn_y = footer_y + 12;
        let btn_h = 34u64;
        let half_w = PANEL_W / 2 - 18;
        if my >= btn_y && my < btn_y + btn_h {
            if mx >= px + 12 && mx < px + 12 + half_w {
                self.visible = false;
                return QsAction::Shutdown;
            }
            let rb_x = px + PANEL_W / 2 + 6;
            if mx >= rb_x && mx < rb_x + half_w {
                self.visible = false;
                return QsAction::Reboot;
            }
        }

        let content_y = PANEL_TOP + TAB_H + 12;

        // Sliders
        if self.tab == QsTab::Display {
            let bcy = content_y + 44;
            let (sx, sy, sw) = Self::slider_rect(px, bcy);
            if my >= sy && my < sy + SLIDER_H && mx >= sx && mx <= sx + sw {
                self.brightness = Self::value_from_click(sx, sw, mx);
                self.dragging_brightness = true;
            }
        }
        if self.tab == QsTab::Sound {
            let vcy = content_y + 44;
            let (sx, sy, sw) = Self::slider_rect(px, vcy);
            if my >= sy && my < sy + SLIDER_H && mx >= sx && mx <= sx + sw {
                self.volume = Self::value_from_click(sx, sw, mx);
                self.dragging_volume = true;
            }
        }

        // Timezone buttons
        if self.tab == QsTab::Time {
            for (i, &(_, _, ofs)) in TIMEZONES.iter().enumerate() {
                let (bx, by, bw, bh) = Self::tz_button_rect(px, content_y, i);
                if mx >= bx && mx < bx + bw && my >= by && my < by + bh {
                    crate::kernel::rtc::set_tz_offset(ofs);
                    break;
                }
            }
        }

        QsAction::None
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, screen_w: u64) {
        if !self.visible { return; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        graphics.draw_soft_shadow(px, PANEL_TOP, PANEL_W, ph, 20, 0x80);
        graphics.fill_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, C_BG);
        graphics.draw_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, C_BORDER, 1);

        // ── Tab bar (5 tabs) ───────────────────────────────────────────────────
        graphics.fill_rounded_rect(px, PANEL_TOP, PANEL_W, TAB_H, 14, C_SURFACE);
        graphics.fill_rect(px, PANEL_TOP + TAB_H / 2, PANEL_W, TAB_H / 2, C_SURFACE);

        let tab_labels = [
            ("Display", QsTab::Display),
            ("Sound",   QsTab::Sound),
            ("Network", QsTab::Network),
            ("Time",    QsTab::Time),
            ("About",   QsTab::About),
        ];
        let tw = PANEL_W / 5;
        for (label, t) in &tab_labels {
            let (tx, ty, _tw, _th) = Self::tab_rect(px, *t);
            let is_active = self.tab == *t;
            if is_active {
                graphics.fill_rounded_rect(tx + 3, ty + 4, tw - 6, TAB_H - 8, 7, C_BG);
                graphics.fill_rounded_rect(tx + 6, ty + TAB_H - 4, tw - 12, 3, 1, C_ACCENT);
            }
            let lx = tx + (tw.saturating_sub(label.len() as u64 * 9)) / 2;
            let ly = ty + (TAB_H - 8) / 2;
            let col = if is_active { C_TEXT } else { C_DIM };
            fonts::draw_string(graphics, lx, ly, label, col);
            if *t != QsTab::About {
                graphics.fill_rect(tx + tw - 1, ty + 6, 1, TAB_H - 12, C_BORDER);
            }
        }

        graphics.fill_rect(px, PANEL_TOP + TAB_H, PANEL_W, 1, C_BORDER);

        let content_y = PANEL_TOP + TAB_H + 12;
        match self.tab {
            QsTab::Display => self.draw_display_tab(graphics, px, content_y),
            QsTab::Sound   => self.draw_sound_tab  (graphics, px, content_y),
            QsTab::Network => self.draw_network_tab (graphics, px, content_y),
            QsTab::Time    => self.draw_time_tab    (graphics, px, content_y),
            QsTab::About   => self.draw_about_tab   (graphics, px, content_y),
        }

        let footer_y = PANEL_TOP + ph - FOOTER_H;
        graphics.fill_rect(px, footer_y, PANEL_W, 1, C_BORDER);
        self.draw_footer(graphics, px, footer_y);
    }

    fn draw_display_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        section_label(graphics, px, cy, "Brightness", C_ACCENT);
        let scy = cy + 26;
        let (sx, _, sw) = Self::slider_rect(px, scy);
        draw_slider(graphics, sx, scy, sw, self.brightness as u64, C_ACCENT, self.dragging_brightness);
        draw_value_pill(graphics, px + PANEL_W - CONTENT_PAD - 36, scy - 2, self.brightness);

        let cy2 = scy + 36;
        section_label(graphics, px, cy2, "Appearance", C_ACCENT);
        let cy3 = cy2 + 26;
        toggle_row(graphics, px, cy3,      "Dark mode",  true);
        toggle_row(graphics, px, cy3 + 28, "Animations", true);
    }

    fn draw_sound_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        section_label(graphics, px, cy, "Master Volume", C_ACCENT);
        let scy = cy + 26;
        let (sx, _, sw) = Self::slider_rect(px, scy);
        draw_slider(graphics, sx, scy, sw, self.volume as u64, C_ACCENT, self.dragging_volume);
        draw_value_pill(graphics, px + PANEL_W - CONTENT_PAD - 36, scy - 2, self.volume);

        let cy2 = scy + 36;
        section_label(graphics, px, cy2, "Output", C_ACCENT);
        let cy3 = cy2 + 26;
        toggle_row(graphics, px, cy3,      "System sounds", true);
        toggle_row(graphics, px, cy3 + 28, "Event sounds",  false);
    }

    fn draw_network_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        let net_up = crate::kernel::net::is_present();
        let dot_col = if net_up { C_ACCENT2 } else { 0xFFED333Bu32 };
        graphics.fill_rounded_rect(px + CONTENT_PAD, cy + 4, 10, 10, 5, dot_col);
        let status = if net_up { "Connected" } else { "No interface" };
        fonts::draw_string(graphics, px + CONTENT_PAD + 16, cy + 3, status, C_TEXT);
        let nic = crate::kernel::net::nic_name();
        fonts::draw_string(graphics, px + CONTENT_PAD + 16, cy + 16, nic, C_DIM);

        let cy2 = cy + 40;
        graphics.fill_rect(px + CONTENT_PAD, cy2, PANEL_W - CONTENT_PAD * 2, 1, C_BORDER);
        let cy3 = cy2 + 10;

        if net_up {
            let ip = crate::kernel::net::get_ip();
            let mut ip_buf = [0u8; 20];
            let mut pos = 0usize;
            for (i, &oct) in ip.iter().enumerate() {
                if i > 0 { ip_buf[pos] = b'.'; pos += 1; }
                let mut n = oct as u32;
                if n >= 100 { ip_buf[pos] = b'0'+(n/100) as u8; pos+=1; n%=100; }
                if n >= 10  { ip_buf[pos] = b'0'+(n/10) as u8;  pos+=1; n%=10;  }
                ip_buf[pos] = b'0'+n as u8; pos += 1;
            }
            info_row(graphics, px, cy3,      "IPv4 Address",  core::str::from_utf8(&ip_buf[..pos]).unwrap_or("?"));
            info_row(graphics, px, cy3 + 22, "Gateway",       "10.0.2.2");
            info_row(graphics, px, cy3 + 44, "DNS",           "10.0.2.3");
            info_row(graphics, px, cy3 + 66, "Mode",          "QEMU NAT (slirp)");
        } else {
            fonts::draw_string(graphics, px + CONTENT_PAD, cy3, "No network adapter detected.", C_DIM);
        }
    }

    fn draw_time_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        // ── UTC notice box ────────────────────────────────────────────────────
        let nb_x = px + CONTENT_PAD;
        let nb_w = PANEL_W - CONTENT_PAD * 2;
        graphics.fill_rounded_rect(nb_x, cy, nb_w, Self::TZ_NOTICE_H, 6, 0xFF1A2A44);
        graphics.draw_rounded_rect(nb_x, cy, nb_w, Self::TZ_NOTICE_H, 6, 0xFF2A4A7A, 1);
        // Info icon
        graphics.fill_rounded_rect(nb_x + 8, cy + 14, 10, 10, 5, C_ACCENT);
        fonts::draw_string(graphics, nb_x + 11, cy + 15, "i", 0xFF000000);
        // Notice text (two lines)
        fonts::draw_string(graphics, nb_x + 24, cy + 8,
            "Hardware RTC stores UTC time.", C_TEXT);
        fonts::draw_string(graphics, nb_x + 24, cy + 22,
            "Select a timezone to show local time.", C_DIM);

        // ── Section header ────────────────────────────────────────────────────
        let sec_y = cy + Self::TZ_NOTICE_H + 6;
        section_label(graphics, px, sec_y, "Timezone", C_ACCENT);

        // Current selection info (right-aligned)
        let cur_idx = Self::current_tz_index();
        let cur_label = if cur_idx < TIMEZONES.len() { TIMEZONES[cur_idx].0 } else { "Custom" };
        let cur_city  = if cur_idx < TIMEZONES.len() { TIMEZONES[cur_idx].1 } else { "" };
        let info_px = cur_label.len() as u64 * 9 + cur_city.len() as u64 * 9 + 12;
        let info_x  = px + PANEL_W - CONTENT_PAD - info_px;
        fonts::draw_string(graphics, info_x, sec_y + 2, cur_label, C_ACCENT);
        fonts::draw_string(graphics, info_x + cur_label.len() as u64 * 9 + 6, sec_y + 2, cur_city, C_DIM);

        // ── Timezone button grid (2 columns × 6 rows) ─────────────────────────
        for (i, &(label, city, _)) in TIMEZONES.iter().enumerate() {
            let (bx, by, bw, bh) = Self::tz_button_rect(px, cy, i);
            let selected = cur_idx == i;

            if selected {
                graphics.fill_rounded_rect(bx, by, bw, bh, 5, 0xFF1A3A5E);
                graphics.draw_rounded_rect(bx, by, bw, bh, 5, C_ACCENT, 1);
                // Left accent bar
                graphics.fill_rounded_rect(bx, by + 3, 3, bh - 6, 1, C_ACCENT);
            } else {
                graphics.fill_rounded_rect(bx, by, bw, bh, 5, C_SURFACE);
                graphics.draw_rounded_rect(bx, by, bw, bh, 5, C_BORDER, 1);
            }

            let label_col = if selected { 0xFF88C8FFu32 } else { C_TEXT };
            let city_col  = if selected { 0xFF6698BBu32 } else { C_DIM };
            fonts::draw_string(graphics, bx + 8,  by + 8, label, label_col);
            // City name — right-aligned inside button
            let city_px = city.len() as u64 * 9;
            let city_x  = bx + bw - city_px - 6;
            fonts::draw_string(graphics, city_x, by + 8, city, city_col);
        }
    }

    fn draw_about_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        section_label(graphics, px, cy, crate::version::NAME, C_ACCENT);
        let cy2 = cy + 26;
        info_row(graphics, px, cy2,      "Version",      crate::version::V_VERSION);
        info_row(graphics, px, cy2 + 22, "Codename",     crate::version::CODENAME);
        info_row(graphics, px, cy2 + 44, "Architecture", crate::version::ARCH);
        info_row(graphics, px, cy2 + 66, "Bootloader",   crate::version::BOOTLOADER);
        info_row(graphics, px, cy2 + 88, "Kernel",       crate::version::KERNEL_LANG);
        info_row(graphics, px, cy2 +110, "Built",        crate::version::BUILD_DATE);

        let cy3 = cy2 + 130;
        graphics.fill_rect(px + CONTENT_PAD, cy3 - 8, PANEL_W - CONTENT_PAD * 2, 1, C_BORDER);
        section_label(graphics, px, cy3, "Runtime", C_ACCENT);
        let cy4 = cy3 + 26;

        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        let total = ticks / 100;
        let h = (total / 3600) % 100;
        let m = (total / 60) % 60;
        let s = total % 60;
        let mut ubuf = [0u8; 8];
        ubuf[0]=b'0'+(h/10) as u8; ubuf[1]=b'0'+(h%10) as u8; ubuf[2]=b':';
        ubuf[3]=b'0'+(m/10) as u8; ubuf[4]=b'0'+(m%10) as u8; ubuf[5]=b':';
        ubuf[6]=b'0'+(s/10) as u8; ubuf[7]=b'0'+(s%10) as u8;
        info_row(graphics, px, cy4, "Uptime", core::str::from_utf8(&ubuf).unwrap_or("?"));

        let mut tbuf = [0u8; 11];
        crate::kernel::rtc::format_time_ampm(&mut tbuf);
        info_row(graphics, px, cy4 + 22, "Local time", core::str::from_utf8(&tbuf).unwrap_or("?"));

        // Timezone label
        let cur_idx = Self::current_tz_index();
        let tz_label = if cur_idx < TIMEZONES.len() {
            TIMEZONES[cur_idx].0
        } else {
            "UTC (custom)"
        };
        info_row(graphics, px, cy4 + 44, "Timezone", tz_label);
    }

    fn draw_footer(&self, graphics: &Graphics, px: u64, footer_y: u64) {
        let btn_y  = footer_y + 12;
        let btn_h  = 34u64;
        let half_w = PANEL_W / 2 - 18;

        let sd_bg = if self.hovered_shutdown { 0xFF6A2020u32 } else { 0xFF3A1010u32 };
        let sd_br = if self.hovered_shutdown { 0xFF8A3030u32 } else { 0xFF5A1818u32 };
        graphics.fill_rounded_rect(px + 12, btn_y, half_w, btn_h, 8, sd_bg);
        graphics.draw_rounded_rect(px + 12, btn_y, half_w, btn_h, 8, sd_br, 1);
        let sd_col = if self.hovered_shutdown { 0xFFFF7777u32 } else { 0xFFCC4444u32 };
        let lbl = "Shut Down";
        let lpx = lbl.len() as u64 * 9;
        fonts::draw_string(graphics, px + 12 + (half_w.saturating_sub(lpx))/2, btn_y + 13, lbl, sd_col);

        let rb_x  = px + PANEL_W / 2 + 6;
        let rb_bg = if self.hovered_reboot { 0xFF1E4A8Au32 } else { 0xFF101828u32 };
        let rb_br = if self.hovered_reboot { 0xFF2E5AAAu32 } else { 0xFF1E3A6Au32 };
        graphics.fill_rounded_rect(rb_x, btn_y, half_w, btn_h, 8, rb_bg);
        graphics.draw_rounded_rect(rb_x, btn_y, half_w, btn_h, 8, rb_br, 1);
        let rb_col = if self.hovered_reboot { 0xFF88CCFFu32 } else { 0xFF5588CCu32 };
        let lbl2 = "Reboot";
        let lpx2 = lbl2.len() as u64 * 9;
        fonts::draw_string(graphics, rb_x + (half_w.saturating_sub(lpx2))/2, btn_y + 13, lbl2, rb_col);
    }
}

// ── Standalone drawing helpers ────────────────────────────────────────────────

fn section_label(graphics: &Graphics, px: u64, cy: u64, label: &str, accent: u32) {
    graphics.fill_rounded_rect(px + CONTENT_PAD, cy + 2, 3, 12, 1, accent);
    fonts::draw_string(graphics, px + CONTENT_PAD + 8, cy + 2, label, C_TEXT);
}

fn draw_slider(graphics: &Graphics, x: u64, y: u64, w: u64, value: u64, accent: u32, active: bool) {
    let fill_w = (w * value / 100).min(w);
    graphics.fill_rounded_rect(x, y + (SLIDER_H - SLIDER_TRACK_H) / 2, w, SLIDER_TRACK_H, 3, C_TRACK);
    if fill_w > 0 {
        graphics.fill_rounded_rect(x, y + (SLIDER_H - SLIDER_TRACK_H) / 2, fill_w, SLIDER_TRACK_H, 3, accent);
    }
    let thumb_r: u64 = if active { 9 } else { 7 };
    let thumb_x = x + fill_w;
    let thumb_y = y + SLIDER_H / 2;
    let thumb_col = if active { 0xFFFFFFFFu32 } else { 0xFFDDDDDDu32 };
    graphics.fill_rounded_rect(
        thumb_x.saturating_sub(thumb_r), thumb_y.saturating_sub(thumb_r),
        thumb_r * 2, thumb_r * 2, thumb_r, thumb_col,
    );
    graphics.draw_rounded_rect(
        thumb_x.saturating_sub(thumb_r), thumb_y.saturating_sub(thumb_r),
        thumb_r * 2, thumb_r * 2, thumb_r, accent, 1,
    );
}

fn draw_value_pill(graphics: &Graphics, x: u64, y: u64, value: u8) {
    graphics.fill_rounded_rect(x, y, 34, 18, 4, C_SURFACE);
    graphics.draw_rounded_rect(x, y, 34, 18, 4, C_BORDER, 1);
    let mut buf = [b' '; 4];
    let mut v = value as u32;
    let mut i = 2usize;
    if v == 0 { buf[0] = b'0'; } else {
        while v > 0 && i < 3 { buf[i] = b'0' + (v%10) as u8; v /= 10; i = i.wrapping_sub(1); }
    }
    buf[3] = b'%';
    let start = buf.iter().position(|&b| b != b' ').unwrap_or(0);
    if let Ok(s) = core::str::from_utf8(&buf[start..]) {
        fonts::draw_string(graphics, x + 4, y + 5, s, C_TEXT);
    }
}

fn toggle_row(graphics: &Graphics, px: u64, cy: u64, label: &str, on: bool) {
    fonts::draw_string(graphics, px + CONTENT_PAD, cy, label, C_TEXT);
    let pw = 36u64; let ph = 18u64;
    let pill_x = px + PANEL_W - CONTENT_PAD - pw;
    let bg = if on { C_ACCENT } else { C_TRACK };
    graphics.fill_rounded_rect(pill_x, cy - 1, pw, ph, ph / 2, bg);
    let thumb_x = if on { pill_x + pw - ph + 2 } else { pill_x + 2 };
    graphics.fill_rounded_rect(thumb_x, cy + 1, ph - 4, ph - 4, (ph-4)/2, 0xFFEEEEEE);
}

fn info_row(graphics: &Graphics, px: u64, cy: u64, label: &str, value: &str) {
    fonts::draw_string(graphics, px + CONTENT_PAD, cy, label, C_DIM);
    let val_px = value.len() as u64 * 9;
    let val_x  = px + PANEL_W - CONTENT_PAD - val_px;
    fonts::draw_string(graphics, val_x, cy, value, C_TEXT);
}
