//! Activities button and application menu for OxideOS.
//!
//! The Activities button (x=0..140) opens the Activities Overview when clicked.
//! A secondary popup lists every registered program (power options included).

use crate::gui::fonts;
use crate::gui::graphics::Graphics;

// ── Button geometry ────────────────────────────────────────────────────────
const BTN_X: u64 = 0;
const BTN_W: u64 = 140; // wide enough for "Activities"
const BTN_H: u64 = 48;  // matches TASKBAR_HEIGHT

// ── Menu geometry ──────────────────────────────────────────────────────────
const MENU_X:    u64 = 6;
const MENU_Y:    u64 = 54;  // sits just below 48px taskbar + 6px gap
const MENU_W:    u64 = 290;
const HDR_H:     u64 = 52;
const SEC_H:     u64 = 20;
const ITEM_H:    u64 = 36;
const FOOTER_H:  u64 = 46;

// ── Colour palette (Professional Slate Theme) ──────────────────────────────
const COL_BTN_IDLE:   u32 = 0x00000000;
const COL_BTN_HOVER:  u32 = 0x20FFFFFF;
const COL_BTN_ACTIVE: u32 = 0x30FFFFFF;
const COL_BTN_TEXT:   u32 = 0xFFE0E0E0;

const COL_MENU_BG:    u32 = 0xF51A1C24; // Very dark slate-gray
const COL_MENU_BDR:   u32 = 0xFF2E3344;
const COL_HDR_TOP:    u32 = 0xFF1E2538;
const COL_HDR_BOT:    u32 = 0xFF141824;
const COL_HDR_TEXT:   u32 = 0xFFFFFFFF;
const COL_SEC_BG:     u32 = 0xFF10121A;
const COL_SEC_TEXT:   u32 = 0xFF4A5268;
const COL_ITEM_HOVER: u32 = 0xFF222840;
const COL_ITEM_TEXT:  u32 = 0xFFCDD5E0;
const COL_ITEM_DESC:  u32 = 0xFF5A6475;
const COL_FOOT_BG:    u32 = 0xFF181C28;
const COL_ACCENT:     u32 = 0xFF3A8FE0; // Bright blue accent

// ── Program registry ────────────────────────────────────────────────────────

struct ProgramEntry {
    name:    &'static str,
    desc:    &'static str,
    accent:  u32,
    section: u8,
}

const ENTRIES: &[ProgramEntry] = &[
    ProgramEntry { name: "terminal",  desc: "Command Console",     accent: 0xFF4EC9B0, section: 0 },
    ProgramEntry { name: "sh",        desc: "Bourne Shell",        accent: 0xFF569CD6, section: 0 },
    ProgramEntry { name: "filemanager", desc: "Browse Files",      accent: 0xFFC586C0, section: 0 },
    ProgramEntry { name: "edit",      desc: "Text Editor",         accent: 0xFFCE9178, section: 0 },
    ProgramEntry { name: "hello_rust",desc: "Rust Demo",          accent: 0xFFD7BA7D, section: 1 },
    ProgramEntry { name: "sysinfo",   desc: "Kernel Monitor",      accent: 0xFF4EC9B0, section: 1 },
    ProgramEntry { name: "fib",       desc: "Math Test",           accent: 0xFF569CD6, section: 1 },
    ProgramEntry { name: "primes",    desc: "Sieve Test",          accent: 0xFF569CD6, section: 1 },
    ProgramEntry { name: "spinner",   desc: "UI Animation",        accent: 0xFFC586C0, section: 1 },
    ProgramEntry { name: "filetest",  desc: "VFS Stress Test",     accent: 0xFFCE9178, section: 1 },
    ProgramEntry { name: "input",     desc: "Keyboard Echo",       accent: 0xFF4EC9B0, section: 2 },
];

const SEC_NAMES: &[&str] = &["SYSTEM APPS", "UTILITIES", "TOOLS"];

fn menu_height() -> u64 {
    let mut sections = 0u64;
    let mut last_sec: Option<u8> = None;
    for e in ENTRIES {
        if last_sec != Some(e.section) {
            sections += 1;
            last_sec = Some(e.section);
        }
    }
    HDR_H + sections * SEC_H + ENTRIES.len() as u64 * ITEM_H + FOOTER_H
}

pub struct StartMenu {
    pub open:            bool,
    pub btn_hover:       bool,
    hovered_item:        Option<usize>,
    hovered_shutdown:    bool,
    hovered_reboot:      bool,
    activities_request:  bool,
}

impl StartMenu {
    pub const fn new() -> Self {
        Self {
            open: false, btn_hover: false, hovered_item: None,
            hovered_shutdown: false, hovered_reboot: false,
            activities_request: false,
        }
    }

    pub fn toggle(&mut self) { self.open = !self.open; }
    pub fn close(&mut self) {
        self.open = false;
        self.hovered_item = None;
        self.hovered_shutdown = false;
        self.hovered_reboot = false;
    }

    /// Returns `true` once (then resets) when the Activities button was clicked.
    pub fn take_activities_request(&mut self) -> bool {
        let r = self.activities_request;
        self.activities_request = false;
        r
    }

    /// Returns (shutdown_x, reboot_x, btn_y, btn_w, btn_h) for the footer power buttons.
    fn footer_rects() -> (u64, u64, u64, u64, u64) {
        let mh = menu_height();
        let footer_y = MENU_Y + mh - FOOTER_H;
        let btn_y    = footer_y + (FOOTER_H - 26) / 2;
        let half     = MENU_W / 2;
        (MENU_X + 12, MENU_X + half + 8, btn_y, half - 20, 26)
    }

    pub fn handle_mouse_move(&mut self, mx: u64, my: u64) -> bool {
        let mut dirty = false;
        let over_btn = my < BTN_H && mx < BTN_W;
        if over_btn != self.btn_hover { self.btn_hover = over_btn; dirty = true; }
        if self.open {
            let prev = self.hovered_item;
            self.hovered_item = self.item_at(mx, my);
            if self.hovered_item != prev { dirty = true; }

            let (sd_x, rb_x, btn_y, btn_w, btn_h) = Self::footer_rects();
            let prev_sd = self.hovered_shutdown;
            let prev_rb = self.hovered_reboot;
            self.hovered_shutdown = mx >= sd_x && mx < sd_x + btn_w
                                 && my >= btn_y && my < btn_y + btn_h;
            self.hovered_reboot   = mx >= rb_x && mx < rb_x + btn_w
                                 && my >= btn_y && my < btn_y + btn_h;
            if self.hovered_shutdown != prev_sd || self.hovered_reboot != prev_rb {
                dirty = true;
            }
        }
        dirty
    }

    pub fn handle_click(&mut self, mx: u64, my: u64) -> (Option<&'static str>, u8, bool) {
        // Activities button: first click opens the start menu; clicking again
        // closes it and opens the Activities Overview instead.
        if my < BTN_H && mx < BTN_W {
            if self.open {
                self.close();
                self.activities_request = true;
            } else {
                self.open = true;
            }
            return (None, 0, true);
        }

        if !self.open { return (None, 0, false); }

        let mh = menu_height();
        if mx >= MENU_X && mx < MENU_X + MENU_W && my >= MENU_Y && my < MENU_Y + mh {
            let footer_y = MENU_Y + mh - FOOTER_H;
            if my >= footer_y {
                let half = MENU_X + MENU_W / 2;
                let action = if mx < half { 1u8 } else { 2u8 };
                self.close();
                return (None, action, true);
            }
            let prog = self.item_at(mx, my).map(|i| ENTRIES[i].name);
            if prog.is_some() { self.close(); }
            return (prog, 0, true);
        }

        self.close();
        (None, 0, false)
    }

    fn item_positions() -> [(u64, usize); 11] {
        let mut result = [(0u64, 0usize); 11];
        let mut y = HDR_H;
        let mut last_sec: Option<u8> = None;
        for (i, e) in ENTRIES.iter().enumerate() {
            if last_sec != Some(e.section) {
                y += SEC_H;
                last_sec = Some(e.section);
            }
            result[i] = (y, i);
            y += ITEM_H;
        }
        result
    }

    fn item_at(&self, mx: u64, my: u64) -> Option<usize> {
        if !self.open { return None; }
        if mx < MENU_X || mx >= MENU_X + MENU_W { return None; }
        if my < MENU_Y { return None; }
        let rel_y = my - MENU_Y;
        let positions = Self::item_positions();
        for (item_y, idx) in &positions {
            if rel_y >= *item_y && rel_y < *item_y + ITEM_H {
                return Some(*idx);
            }
        }
        None
    }

    pub fn draw_button(&self, graphics: &Graphics) {
        let bg = if self.btn_hover { COL_BTN_HOVER } else { COL_BTN_IDLE };
        if bg != 0 {
            graphics.fill_rounded_rect(BTN_X + 6, 6, BTN_W - 12, BTN_H - 12, 8, bg);
        }

        // GNOME Activities icon: outer ring + inner dot
        let ix = BTN_X + 16;
        let iy = (BTN_H - 18) / 2;
        graphics.fill_rounded_rect(ix, iy, 18, 18, 9, 0x40FFFFFF); // outer ring bg
        graphics.draw_rounded_rect(ix, iy, 18, 18, 9, COL_ACCENT, 1); // ring border
        graphics.fill_rounded_rect(ix + 5, iy + 5, 8, 8, 4, COL_ACCENT); // inner dot

        // "Activities" label
        fonts::draw_string(graphics, BTN_X + 42, (BTN_H - 8) / 2, "Activities", COL_BTN_TEXT);

        // Active indicator — blue underline bar
        if self.open {
            let ind_w = 60u64;
            let ind_x = BTN_X + (BTN_W - ind_w) / 2;
            graphics.fill_rounded_rect(ind_x, BTN_H - 4, ind_w, 3, 1, COL_ACCENT);
        }
    }

    pub fn draw_menu(&self, graphics: &Graphics) {
        if !self.open { return; }

        let mh = menu_height();

        // Large shadow for elevated feel
        graphics.draw_soft_shadow(MENU_X, MENU_Y, MENU_W, mh, 20, 0x80);
        graphics.fill_rounded_rect(MENU_X, MENU_Y, MENU_W, mh, 10, COL_MENU_BG);

        // ── Header ─────────────────────────────────────────────────────────────
        graphics.fill_rounded_rect(MENU_X, MENU_Y, MENU_W, HDR_H, 10, COL_HDR_TOP);
        graphics.fill_rect(MENU_X, MENU_Y + HDR_H / 2, MENU_W, HDR_H / 2, COL_HDR_TOP);
        graphics.fill_rect_gradient_v(MENU_X, MENU_Y, MENU_W, HDR_H, COL_HDR_TOP, COL_HDR_BOT);

        // Blue left accent bar in header
        graphics.fill_rounded_rect(MENU_X + 1, MENU_Y + 10, 3, HDR_H - 20, 1, COL_ACCENT);

        // OS name + version
        fonts::draw_string(graphics, MENU_X + 16, MENU_Y + 12, "OxideOS", 0xFFFFFFFF);
        fonts::draw_string(graphics, MENU_X + 16, MENU_Y + 28, "v0.1.0-dev", COL_ITEM_DESC);

        // Divider line below header
        graphics.fill_rect(MENU_X + 1, MENU_Y + HDR_H - 1, MENU_W - 2, 1, 0xFF202840);

        // ── Items ──────────────────────────────────────────────────────────────
        let mut y = MENU_Y + HDR_H;
        let mut last_sec: Option<u8> = None;
        for (i, entry) in ENTRIES.iter().enumerate() {
            if last_sec != Some(entry.section) {
                last_sec = Some(entry.section);
                graphics.fill_rect(MENU_X + 1, y, MENU_W - 2, SEC_H, COL_SEC_BG);
                // Section label with left indent dot
                graphics.fill_rounded_rect(MENU_X + 8, y + 7, 4, 4, 2, 0xFF2A3A5A);
                fonts::draw_string(graphics, MENU_X + 18, y + 5,
                    SEC_NAMES[entry.section as usize], COL_SEC_TEXT);
                y += SEC_H;
            }

            let hovered = self.hovered_item == Some(i);
            if hovered {
                graphics.fill_rounded_rect(MENU_X + 4, y + 2, MENU_W - 8, ITEM_H - 4, 5, COL_ITEM_HOVER);
                // Left accent bar on hover
                graphics.fill_rounded_rect(MENU_X + 1, y + 6, 3, ITEM_H - 12, 1, COL_ACCENT);
            }

            let text_col  = if hovered { 0xFFFFFFFF } else { COL_ITEM_TEXT };
            let desc_col  = if hovered { 0xFF8090A8 } else { COL_ITEM_DESC };
            // Accent dot beside name
            graphics.fill_rounded_rect(MENU_X + 10, y + 14, 4, 4, 2, entry.accent);
            fonts::draw_string(graphics, MENU_X + 20, y + 9,  entry.name, text_col);
            fonts::draw_string(graphics, MENU_X + 20, y + 23, entry.desc, desc_col);

            y += ITEM_H;
        }

        // ── Footer ─────────────────────────────────────────────────────────────
        let footer_y = MENU_Y + mh - FOOTER_H;
        graphics.fill_rounded_rect(MENU_X, footer_y, MENU_W, FOOTER_H, 10, COL_FOOT_BG);
        graphics.fill_rect(MENU_X, footer_y, MENU_W, FOOTER_H / 2, COL_FOOT_BG);
        graphics.fill_rect(MENU_X, footer_y, MENU_W, 1, 0xFF1E2438);

        let half = MENU_W / 2;
        let btn_y = footer_y + (FOOTER_H - 26) / 2;

        // Shut Down button
        let sd_bg  = if self.hovered_shutdown { 0xFF7A2020 } else { 0xFF3A1414 };
        let sd_bdr = if self.hovered_shutdown { 0xFFAA3030 } else { 0xFF6A2020 };
        let sd_txt = if self.hovered_shutdown { 0xFFFF9090 } else { 0xFFE05050 };
        graphics.fill_rounded_rect(MENU_X + 12, btn_y, half - 20, 26, 5, sd_bg);
        graphics.draw_rounded_rect(MENU_X + 12, btn_y, half - 20, 26, 5, sd_bdr, 1);
        fonts::draw_string(graphics, MENU_X + 22, btn_y + 9, "Shut Down", sd_txt);

        // Reboot button
        let rb_bg  = if self.hovered_reboot { 0xFF1E4A8A } else { 0xFF101828 };
        let rb_bdr = if self.hovered_reboot { 0xFF2E6AAA } else { 0xFF1E3A6A };
        let rb_txt = if self.hovered_reboot { 0xFF99DDFF } else { 0xFF569CD6 };
        graphics.fill_rounded_rect(MENU_X + half + 8, btn_y, half - 20, 26, 5, rb_bg);
        graphics.draw_rounded_rect(MENU_X + half + 8, btn_y, half - 20, 26, 5, rb_bdr, 1);
        fonts::draw_string(graphics, MENU_X + half + 22, btn_y + 9, "Reboot", rb_txt);

        // Outer border
        graphics.draw_rounded_rect(MENU_X, MENU_Y, MENU_W, mh, 10, COL_MENU_BDR, 1);
    }
}
