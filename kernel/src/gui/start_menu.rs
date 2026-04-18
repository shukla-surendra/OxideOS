//! Start Menu for OxideOS.
//!
//! Draws a "Start" button in the taskbar (x=0..100, y=0..40).
//! Clicking it toggles a popup panel that lists every registered program.

use crate::gui::fonts;
use crate::gui::graphics::Graphics;

// ── Button geometry ────────────────────────────────────────────────────────
const BTN_X: u64 = 0;
const BTN_W: u64 = 100;
const BTN_H: u64 = 40;

// ── Menu geometry ──────────────────────────────────────────────────────────
const MENU_X:    u64 = 6;
const MENU_Y:    u64 = 46;
const MENU_W:    u64 = 260;
const HDR_H:     u64 = 42;
const SEC_H:     u64 = 20;
const ITEM_H:    u64 = 34;
const FOOTER_H:  u64 = 40;

// ── Colour palette (Professional Slate Theme) ──────────────────────────────
const COL_BTN_IDLE:   u32 = 0x00000000;
const COL_BTN_HOVER:  u32 = 0x20FFFFFF;
const COL_BTN_ACTIVE: u32 = 0x30FFFFFF;
const COL_BTN_TEXT:   u32 = 0xFFE0E0E0;

const COL_MENU_BG:    u32 = 0xF51A1C22; // Very dark slate-gray
const COL_MENU_BDR:   u32 = 0xFF3A3F4B;
const COL_HDR_TOP:    u32 = 0xFF252A33;
const COL_HDR_BOT:    u32 = 0xFF1A1C22;
const COL_HDR_TEXT:   u32 = 0xFFFFFFFF;
const COL_SEC_BG:     u32 = 0xFF14161B;
const COL_SEC_TEXT:   u32 = 0xFF5C6370;
const COL_ITEM_HOVER: u32 = 0xFF2C313A;
const COL_ITEM_TEXT:  u32 = 0xFFD1D5DA;
const COL_ITEM_DESC:  u32 = 0xFF6A737D;
const COL_FOOT_BG:    u32 = 0xFF21252B;
const COL_ACCENT:     u32 = 0xFF4EC9B0; // Professional Teal

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
    pub open:       bool,
    pub btn_hover:  bool,
    hovered_item:   Option<usize>,
}

impl StartMenu {
    pub const fn new() -> Self {
        Self { open: false, btn_hover: false, hovered_item: None }
    }

    pub fn toggle(&mut self) { self.open = !self.open; }
    pub fn close(&mut self)  { self.open = false; self.hovered_item = None; }

    pub fn handle_mouse_move(&mut self, mx: u64, my: u64) -> bool {
        let mut dirty = false;
        let over_btn = my < BTN_H && mx < BTN_W;
        if over_btn != self.btn_hover { self.btn_hover = over_btn; dirty = true; }
        if self.open {
            let prev = self.hovered_item;
            self.hovered_item = self.item_at(mx, my);
            if self.hovered_item != prev { dirty = true; }
        }
        dirty
    }

    pub fn handle_click(&mut self, mx: u64, my: u64) -> (Option<&'static str>, u8, bool) {
        if my < BTN_H && mx < BTN_W {
            self.toggle();
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
        let bg = if self.open         { COL_BTN_ACTIVE }
                 else if self.btn_hover { COL_BTN_HOVER  }
                 else                   { COL_BTN_IDLE   };

        if bg != 0 {
            graphics.fill_rect(BTN_X, 0, BTN_W, BTN_H, bg);
        }

        if self.open {
            graphics.fill_rect(BTN_X, BTN_H - 3, BTN_W, 3, COL_ACCENT);
        }

        // Sleek Icon: 4 circles
        let ix = BTN_X + 14;
        let iy = 15u64;
        let r  = 4u64;
        let gap = 3u64;
        graphics.fill_rounded_rect(ix,           iy,           r, r, 2, COL_ACCENT);
        graphics.fill_rounded_rect(ix + r + gap, iy,           r, r, 2, COL_ACCENT);
        graphics.fill_rounded_rect(ix,           iy + r + gap, r, r, 2, COL_ACCENT);
        graphics.fill_rounded_rect(ix + r + gap, iy + r + gap, r, r, 2, COL_ACCENT);

        fonts::draw_string(graphics, BTN_X + 38, 14, "Start", COL_BTN_TEXT);
    }

    pub fn draw_menu(&self, graphics: &Graphics) {
        if !self.open { return; }

        let mh = menu_height();
        graphics.draw_soft_shadow(MENU_X, MENU_Y, MENU_W, mh, 16, 0x70);
        graphics.fill_rounded_rect(MENU_X, MENU_Y, MENU_W, mh, 10, COL_MENU_BG);

        // Header
        graphics.fill_rounded_rect(MENU_X, MENU_Y, MENU_W, HDR_H, 10, COL_HDR_TOP);
        graphics.fill_rect(MENU_X, MENU_Y + HDR_H / 2, MENU_W, HDR_H / 2, COL_HDR_TOP);
        graphics.fill_rect_gradient_v(MENU_X, MENU_Y, MENU_W, HDR_H, COL_HDR_TOP, COL_HDR_BOT);
        
        fonts::draw_string(graphics, MENU_X + 14, MENU_Y + 14, "OxideOS", COL_HDR_TEXT);
        fonts::draw_string(graphics, MENU_X + 180, MENU_Y + 16, "v0.1.0", COL_ITEM_DESC);

        let mut y = MENU_Y + HDR_H;
        let mut last_sec: Option<u8> = None;
        for (i, entry) in ENTRIES.iter().enumerate() {
            if last_sec != Some(entry.section) {
                last_sec = Some(entry.section);
                graphics.fill_rect(MENU_X + 1, y, MENU_W - 2, SEC_H, COL_SEC_BG);
                fonts::draw_string(graphics, MENU_X + 14, y + 4,
                    SEC_NAMES[entry.section as usize], COL_SEC_TEXT);
                y += SEC_H;
            }

            let hovered = self.hovered_item == Some(i);
            if hovered {
                graphics.fill_rect(MENU_X + 4, y, MENU_W - 8, ITEM_H, COL_ITEM_HOVER);
                graphics.fill_rect(MENU_X + 1, y + 4, 3, ITEM_H - 8, COL_ACCENT);
            }

            let text_col = if hovered { 0xFFFFFFFF } else { COL_ITEM_TEXT };
            fonts::draw_string(graphics, MENU_X + 14, y + 6,  entry.name, text_col);
            fonts::draw_string(graphics, MENU_X + 14, y + 20, entry.desc, COL_ITEM_DESC);

            y += ITEM_H;
        }

        // Footer
        let footer_y = MENU_Y + mh - FOOTER_H;
        graphics.fill_rounded_rect(MENU_X, footer_y, MENU_W, FOOTER_H, 10, COL_FOOT_BG);
        graphics.fill_rect(MENU_X, footer_y, MENU_W, FOOTER_H / 2, COL_FOOT_BG);
        graphics.fill_rect(MENU_X, footer_y, MENU_W, 1, COL_MENU_BDR);

        let half = MENU_W / 2;
        // Hit area feedback (if mouse in footer)
        if self.hovered_item.is_none() {
            // we use a custom check for footer hover if we wanted feedback, 
            // but let's keep it simple for now.
        }

        fonts::draw_string(graphics, MENU_X + 20, footer_y + 14, "Shut Down", 0xFFF14C4C);
        fonts::draw_string(graphics, MENU_X + half + 25, footer_y + 14, "Reboot", 0xFF569CD6);

        graphics.draw_rounded_rect(MENU_X, MENU_Y, MENU_W, mh, 10, COL_MENU_BDR, 1);
    }
}
