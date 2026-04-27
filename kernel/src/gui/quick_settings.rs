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

// Slider hit-box height (generous for easy clicking)
const SLIDER_H:   u64 = 20;
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

// ── Tabs ──────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
pub enum QsTab { Display, Sound, Network, About }

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
    pub fn close(&mut self)  { self.visible = false; self.dragging_brightness = false; self.dragging_volume = false; }

    // ── Geometry helpers ──────────────────────────────────────────────────────
    fn panel_x(screen_w: u64) -> u64 { screen_w.saturating_sub(PANEL_W + PANEL_PAD) }
    fn panel_h() -> u64 { TAB_H + 240 + FOOTER_H } // tab bar + content + footer

    // Slider geometry within content area
    fn slider_rect(px: u64, cy: u64) -> (u64, u64, u64) {
        // returns (x, y, width)
        (px + CONTENT_PAD, cy + 4, PANEL_W - CONTENT_PAD * 2)
    }

    fn value_from_click(slider_x: u64, slider_w: u64, mx: u64) -> u8 {
        let rel = mx.saturating_sub(slider_x).min(slider_w);
        (rel * 100 / slider_w) as u8
    }

    fn tab_rect(px: u64, tab: QsTab) -> (u64, u64, u64, u64) {
        let tw = PANEL_W / 4;
        let tx = px + match tab {
            QsTab::Display => 0,
            QsTab::Sound   => tw,
            QsTab::Network => tw * 2,
            QsTab::About   => tw * 3,
        };
        (tx, PANEL_TOP, tw, TAB_H)
    }

    // ── Toggle area (right side of taskbar) ───────────────────────────────────
    pub fn is_toggle_area(mx: u64, my: u64, screen_w: u64) -> bool {
        my < 48 && mx + 200 > screen_w
    }

    // ── Input handling ────────────────────────────────────────────────────────

    /// Call every mouse-move frame, passing whether the left button is held.
    pub fn handle_mouse_move(&mut self, mx: u64, my: u64, screen_w: u64, left_held: bool) {
        if !self.visible { return; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        // Hover detection for footer buttons
        let footer_y = PANEL_TOP + ph - FOOTER_H;
        let btn_y = footer_y + 12;
        let btn_h = 34u64;
        let half_w = PANEL_W / 2 - 18;
        self.hovered_shutdown = mx >= px + 12 && mx < px + 12 + half_w
                             && my >= btn_y && my < btn_y + btn_h;
        let rb_x = px + PANEL_W / 2 + 6;
        self.hovered_reboot   = mx >= rb_x && mx < rb_x + half_w
                             && my >= btn_y && my < btn_y + btn_h;

        // Slider drag — update value if dragging while left is held
        if left_held && self.tab == QsTab::Display {
            let content_y = PANEL_TOP + TAB_H + 12;
            let bcy = content_y + 44; // brightness slider y (see draw_display_tab)
            let (sx, sy, sw) = Self::slider_rect(px, bcy);
            if my >= sy && my < sy + SLIDER_H {
                if mx >= sx && mx <= sx + sw {
                    self.brightness = Self::value_from_click(sx, sw, mx);
                    self.dragging_brightness = true;
                }
            }
        }
        if left_held && self.tab == QsTab::Sound {
            let content_y = PANEL_TOP + TAB_H + 12;
            let vcy = content_y + 44; // volume slider y
            let (sx, sy, sw) = Self::slider_rect(px, vcy);
            if my >= sy && my < sy + SLIDER_H {
                if mx >= sx && mx <= sx + sw {
                    self.volume = Self::value_from_click(sx, sw, mx);
                    self.dragging_volume = true;
                }
            }
        }
        if !left_held {
            self.dragging_brightness = false;
            self.dragging_volume     = false;
        }
    }

    /// Call on left mouse-button press.
    pub fn handle_click(&mut self, mx: u64, my: u64, screen_w: u64) -> QsAction {
        if !self.visible { return QsAction::None; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        // Click outside panel → dismiss
        if mx < px || mx >= px + PANEL_W || my < PANEL_TOP || my >= PANEL_TOP + ph {
            self.visible = false;
            return QsAction::Dismiss;
        }

        // Tab bar clicks
        for &t in &[QsTab::Display, QsTab::Sound, QsTab::Network, QsTab::About] {
            let (tx, ty, tw, th) = Self::tab_rect(px, t);
            if mx >= tx && mx < tx + tw && my >= ty && my < ty + th {
                self.tab = t;
                return QsAction::None;
            }
        }

        // Footer buttons
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

        // Slider clicks (immediate set on click too, not just drag)
        let content_y = PANEL_TOP + TAB_H + 12;
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

        QsAction::None
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, screen_w: u64) {
        if !self.visible { return; }
        let px = Self::panel_x(screen_w);
        let ph = Self::panel_h();

        // ── Panel chrome ──────────────────────────────────────────────────────
        graphics.draw_soft_shadow(px, PANEL_TOP, PANEL_W, ph, 20, 0x80);
        graphics.fill_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, C_BG);
        graphics.draw_rounded_rect(px, PANEL_TOP, PANEL_W, ph, 14, C_BORDER, 1);

        // ── Tab bar ───────────────────────────────────────────────────────────
        graphics.fill_rect(px, PANEL_TOP, PANEL_W, TAB_H, C_SURFACE);
        // Round the top corners only by overdrawing sharp corners
        graphics.fill_rounded_rect(px, PANEL_TOP, PANEL_W, TAB_H, 14, C_SURFACE);
        graphics.fill_rect(px, PANEL_TOP + TAB_H / 2, PANEL_W, TAB_H / 2, C_SURFACE);

        let tab_labels = [("Display", QsTab::Display), ("Sound", QsTab::Sound),
                          ("Network", QsTab::Network), ("About", QsTab::About)];
        let tw = PANEL_W / 4;
        for (label, t) in &tab_labels {
            let (tx, ty, _tw, _th) = Self::tab_rect(px, *t);
            let is_active = self.tab == *t;
            if is_active {
                graphics.fill_rounded_rect(tx + 4, ty + 4, tw - 8, TAB_H - 8, 8, C_BG);
                // Accent underline
                graphics.fill_rounded_rect(tx + 8, ty + TAB_H - 4, tw - 16, 3, 1, C_ACCENT);
            }
            let lx = tx + (tw.saturating_sub(label.len() as u64 * 9)) / 2;
            let ly = ty + (TAB_H - 8) / 2;
            let col = if is_active { C_TEXT } else { C_DIM };
            fonts::draw_string(graphics, lx, ly, label, col);
            // Divider between tabs (except last)
            if *t != QsTab::About {
                graphics.fill_rect(tx + tw - 1, ty + 6, 1, TAB_H - 12, C_BORDER);
            }
        }

        // Tab-content area separator
        graphics.fill_rect(px, PANEL_TOP + TAB_H, PANEL_W, 1, C_BORDER);

        // ── Tab content ───────────────────────────────────────────────────────
        let content_y = PANEL_TOP + TAB_H + 12;
        match self.tab {
            QsTab::Display => self.draw_display_tab(graphics, px, content_y),
            QsTab::Sound   => self.draw_sound_tab  (graphics, px, content_y),
            QsTab::Network => self.draw_network_tab (graphics, px, content_y),
            QsTab::About   => self.draw_about_tab   (graphics, px, content_y),
        }

        // ── Footer ────────────────────────────────────────────────────────────
        let footer_y = PANEL_TOP + ph - FOOTER_H;
        graphics.fill_rect(px, footer_y, PANEL_W, 1, C_BORDER);
        self.draw_footer(graphics, px, footer_y);
    }

    fn draw_display_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        // Section label
        section_label(graphics, px, cy, "Brightness", C_ACCENT);

        // Brightness slider
        let scy = cy + 26;
        let (sx, _, sw) = Self::slider_rect(px, scy);
        draw_slider(graphics, sx, scy, sw, self.brightness as u64, C_ACCENT, self.dragging_brightness);

        // Value pill
        draw_value_pill(graphics, px + PANEL_W - CONTENT_PAD - 36, scy - 2, self.brightness);

        let cy2 = scy + 36;
        section_label(graphics, px, cy2, "Appearance", C_ACCENT);
        let cy3 = cy2 + 26;
        // Dark mode toggle (visual only for now)
        toggle_row(graphics, px, cy3, "Dark mode", true);
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

        // Status row
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

    fn draw_about_tab(&self, graphics: &Graphics, px: u64, cy: u64) {
        section_label(graphics, px, cy, crate::version::NAME, C_ACCENT);
        let cy2 = cy + 26;
        info_row(graphics, px, cy2,      "Version",      crate::version::V_VERSION);
        info_row(graphics, px, cy2 + 22, "Codename",     crate::version::CODENAME);
        info_row(graphics, px, cy2 + 44, "Architecture", crate::version::ARCH);
        info_row(graphics, px, cy2 + 66, "Bootloader",   crate::version::BOOTLOADER);
        info_row(graphics, px, cy2 + 88, "Kernel",       crate::version::KERNEL_LANG);
        info_row(graphics, px, cy2 +110, "Built",        crate::version::BUILD_DATE);

        let cy3 = cy2 + 96;
        graphics.fill_rect(px + CONTENT_PAD, cy3 - 8, PANEL_W - CONTENT_PAD * 2, 1, C_BORDER);
        section_label(graphics, px, cy3, "Runtime", C_ACCENT);
        let cy4 = cy3 + 26;
        // Uptime
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
        info_row(graphics, px, cy4 + 22, "System time", core::str::from_utf8(&tbuf).unwrap_or("?"));
    }

    fn draw_footer(&self, graphics: &Graphics, px: u64, footer_y: u64) {
        let btn_y   = footer_y + 12;
        let btn_h   = 34u64;
        let half_w  = PANEL_W / 2 - 18;

        // Shutdown
        let sd_bg = if self.hovered_shutdown { 0xFF6A2020u32 } else { 0xFF3A1010u32 };
        let sd_br = if self.hovered_shutdown { 0xFF8A3030u32 } else { 0xFF5A1818u32 };
        graphics.fill_rounded_rect(px + 12, btn_y, half_w, btn_h, 8, sd_bg);
        graphics.draw_rounded_rect(px + 12, btn_y, half_w, btn_h, 8, sd_br, 1);
        let sd_col = if self.hovered_shutdown { 0xFFFF7777u32 } else { 0xFFCC4444u32 };
        let lbl = "Shut Down";
        let lpx = lbl.len() as u64 * 9;
        fonts::draw_string(graphics, px + 12 + (half_w.saturating_sub(lpx))/2, btn_y + 13, lbl, sd_col);

        // Reboot
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
    // Left accent stripe
    graphics.fill_rounded_rect(px + CONTENT_PAD, cy + 2, 3, 12, 1, accent);
    fonts::draw_string(graphics, px + CONTENT_PAD + 8, cy + 2, label, C_TEXT);
}

/// Draws a horizontal slider with thumb. `value` is 0–100.
fn draw_slider(graphics: &Graphics, x: u64, y: u64, w: u64, value: u64, accent: u32, active: bool) {
    let fill_w = (w * value / 100).min(w);

    // Track background
    graphics.fill_rounded_rect(x, y + (SLIDER_H - SLIDER_TRACK_H) / 2, w, SLIDER_TRACK_H, 3, C_TRACK);

    // Filled portion
    if fill_w > 0 {
        graphics.fill_rounded_rect(x, y + (SLIDER_H - SLIDER_TRACK_H) / 2, fill_w, SLIDER_TRACK_H, 3, accent);
    }

    // Thumb — larger when actively dragging for better feedback
    let thumb_r: u64 = if active { 9 } else { 7 };
    let thumb_x = x + fill_w;
    let thumb_y = y + SLIDER_H / 2;
    let thumb_col = if active { 0xFFFFFFFFu32 } else { 0xFFDDDDDDu32 };
    graphics.fill_rounded_rect(
        thumb_x.saturating_sub(thumb_r),
        thumb_y.saturating_sub(thumb_r),
        thumb_r * 2, thumb_r * 2, thumb_r, thumb_col,
    );
    graphics.draw_rounded_rect(
        thumb_x.saturating_sub(thumb_r),
        thumb_y.saturating_sub(thumb_r),
        thumb_r * 2, thumb_r * 2, thumb_r, accent, 1,
    );
}

/// Small pill showing the current value ("80%").
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
    let s = core::str::from_utf8(&buf).unwrap_or(" 0%");
    // right-align: find actual start
    let start = buf.iter().position(|&b| b != b' ').unwrap_or(0);
    if let Ok(s) = core::str::from_utf8(&buf[start..]) {
        fonts::draw_string(graphics, x + 4, y + 5, s, C_TEXT);
    }
    let _ = s;
}

/// A label + "on/off" indicator row (visual only).
fn toggle_row(graphics: &Graphics, px: u64, cy: u64, label: &str, on: bool) {
    fonts::draw_string(graphics, px + CONTENT_PAD, cy, label, C_TEXT);
    // Pill toggle at right
    let pw = 36u64; let ph = 18u64;
    let pill_x = px + PANEL_W - CONTENT_PAD - pw;
    let bg = if on { C_ACCENT } else { C_TRACK };
    graphics.fill_rounded_rect(pill_x, cy - 1, pw, ph, ph / 2, bg);
    let thumb_x = if on { pill_x + pw - ph + 2 } else { pill_x + 2 };
    graphics.fill_rounded_rect(thumb_x, cy + 1, ph - 4, ph - 4, (ph-4)/2, 0xFFEEEEEE);
}

/// One info row: dim label on left, value on right.
fn info_row(graphics: &Graphics, px: u64, cy: u64, label: &str, value: &str) {
    fonts::draw_string(graphics, px + CONTENT_PAD, cy, label, C_DIM);
    let val_px = value.len() as u64 * 9;
    let val_x  = px + PANEL_W - CONTENT_PAD - val_px;
    fonts::draw_string(graphics, val_x, cy, value, C_TEXT);
}
