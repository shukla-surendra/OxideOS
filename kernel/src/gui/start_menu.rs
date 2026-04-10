//! Start Menu for OxideOS.
//!
//! Draws a "⊞ Start" button in the taskbar (x=0..90, y=0..40).
//! Clicking it toggles a popup panel that lists every registered program,
//! grouped by category.  Clicking a program tile spawns it.

use crate::gui::fonts;
use crate::gui::graphics::Graphics;

// ── Button geometry ────────────────────────────────────────────────────────
const BTN_X: u64 = 0;
const BTN_W: u64 = 90;
const BTN_H: u64 = 40; // full taskbar height

// ── Menu geometry ──────────────────────────────────────────────────────────
const MENU_X:    u64 = 0;
const MENU_Y:    u64 = 40; // just below the taskbar
const MENU_W:    u64 = 240;
const HDR_H:     u64 = 36; // OS name header
const SEC_H:     u64 = 18; // section label row
const ITEM_H:    u64 = 30; // per-program row
const FOOTER_H:  u64 = 28; // bottom shutdown row

// ── Colour palette ─────────────────────────────────────────────────────────
const COL_BTN_IDLE:   u32 = 0xFF1C2236;
const COL_BTN_HOVER:  u32 = 0xFF223050;
const COL_BTN_ACTIVE: u32 = 0xFF0D4070;
const COL_BTN_BORDER: u32 = 0xFF007ACC;
const COL_BTN_TEXT:   u32 = 0xFFDDEEFF;

const COL_MENU_BG:    u32 = 0xFF10151F;
const COL_MENU_BDR:   u32 = 0xFF1A5F9A;
const COL_HDR_TOP:    u32 = 0xFF0D4070;
const COL_HDR_BOT:    u32 = 0xFF071828;
const COL_HDR_TEXT:   u32 = 0xFFDDEEFF;
const COL_SEC_BG:     u32 = 0xFF0A0F18;
const COL_SEC_TEXT:   u32 = 0xFF3A6090;
const COL_ITEM_HOVER: u32 = 0xFF162840;
const COL_ITEM_TEXT:  u32 = 0xFFBBCCDD;
const COL_ITEM_DESC:  u32 = 0xFF506070;
const COL_FOOT_BG:    u32 = 0xFF0A0D14;
const COL_FOOT_TEXT:  u32 = 0xFF8090A0;

// ── Program registry ────────────────────────────────────────────────────────

struct ProgramEntry {
    name:    &'static str,
    desc:    &'static str,
    accent:  u32,
    section: u8, // 0 = Shell, 1 = Programs, 2 = Interactive
}

const ENTRIES: &[ProgramEntry] = &[
    ProgramEntry { name: "terminal",  desc: "GUI Terminal",        accent: 0xFF007ACC, section: 0 },
    ProgramEntry { name: "sh",        desc: "Shell",               accent: 0xFF1A9A50, section: 0 },
    ProgramEntry { name: "hello_rust",desc: "Hello Rust",          accent: 0xFFCC6633, section: 1 },
    ProgramEntry { name: "sysinfo",   desc: "System Info",         accent: 0xFF3399AA, section: 1 },
    ProgramEntry { name: "fib",       desc: "Fibonacci",           accent: 0xFF2277CC, section: 1 },
    ProgramEntry { name: "primes",    desc: "Primes up to 100",    accent: 0xFF227788, section: 1 },
    ProgramEntry { name: "hello",     desc: "Hello (asm)",         accent: 0xFF886644, section: 1 },
    ProgramEntry { name: "counter",   desc: "Count 1–9",           accent: 0xFF446688, section: 1 },
    ProgramEntry { name: "countdown", desc: "Countdown 10→1",      accent: 0xFF996633, section: 1 },
    ProgramEntry { name: "spinner",   desc: "Spinner",             accent: 0xFF886699, section: 1 },
    ProgramEntry { name: "filetest",  desc: "File I/O Demo",       accent: 0xFF885533, section: 1 },
    ProgramEntry { name: "input",     desc: "Stdin Echo (Ctrl-C)", accent: 0xFF558844, section: 2 },
];

const SEC_NAMES: &[&str] = &["Shell / Terminal", "Programs", "Interactive"];

// ── Helper: compute menu height ─────────────────────────────────────────────

fn menu_height() -> u64 {
    // Count unique section headers
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

// ── StartMenu ───────────────────────────────────────────────────────────────

pub struct StartMenu {
    pub open:       bool,
    pub btn_hover:  bool,
    hovered_item:   Option<usize>, // index into ENTRIES
}

impl StartMenu {
    pub const fn new() -> Self {
        Self { open: false, btn_hover: false, hovered_item: None }
    }

    // ── Public API ────────────────────────────────────────────────────────

    pub fn toggle(&mut self) { self.open = !self.open; }
    pub fn close(&mut self)  { self.open = false; self.hovered_item = None; }

    /// Call on every mouse-move event.  Returns `true` if a redraw is needed.
    pub fn handle_mouse_move(&mut self, mx: u64, my: u64) -> bool {
        let mut dirty = false;

        // Button hover
        let over_btn = my < BTN_H && mx >= BTN_X && mx < BTN_X + BTN_W;
        if over_btn != self.btn_hover { self.btn_hover = over_btn; dirty = true; }

        // Item hover (only when open)
        if self.open {
            let prev = self.hovered_item;
            self.hovered_item = self.item_at(mx, my);
            if self.hovered_item != prev { dirty = true; }
        }

        dirty
    }

    /// Call on a left-click.
    /// Returns `Some(name)` when a program should be spawned,
    ///         `None` otherwise (may still toggle or close the menu).
    /// `consumed` is set true when the click was handled by the start menu at all.
    pub fn handle_click(&mut self, mx: u64, my: u64) -> (Option<&'static str>, bool) {
        // Click on the start button?
        if my < BTN_H && mx >= BTN_X && mx < BTN_X + BTN_W {
            self.toggle();
            return (None, true);
        }

        if !self.open { return (None, false); }

        // Click inside the menu panel?
        let mh = menu_height();
        if mx >= MENU_X && mx < MENU_X + MENU_W && my >= MENU_Y && my < MENU_Y + mh {
            let prog = self.item_at(mx, my).map(|i| ENTRIES[i].name);
            self.close();
            return (prog, true);
        }

        // Click outside while open → close
        self.close();
        (None, false)
    }

    // ── Geometry helpers ──────────────────────────────────────────────────

    /// (y_offset_within_menu, item_index) for each entry.
    fn item_positions() -> [(u64, usize); 13] {
        let mut result = [(0u64, 0usize); 13];
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

    // ── Drawing ───────────────────────────────────────────────────────────

    /// Draw the Start button over the left portion of the taskbar.
    /// Call this AFTER `wm.draw_taskbar()` so it paints on top.
    pub fn draw_button(&self, graphics: &Graphics) {
        let bg = if self.open         { COL_BTN_ACTIVE }
                 else if self.btn_hover { COL_BTN_HOVER  }
                 else                   { COL_BTN_IDLE   };

        graphics.fill_rect(BTN_X, 0, BTN_W, BTN_H, bg);

        // Bottom accent bar (matches taskbar accent line style)
        let accent = if self.open { 0xFF00D4FF } else { COL_BTN_BORDER };
        graphics.fill_rect(BTN_X, BTN_H - 2, BTN_W, 2, accent);

        // Border on right side only (separates from window tabs)
        graphics.fill_rect(BTN_X + BTN_W - 1, 0, 1, BTN_H - 2, COL_BTN_BORDER);

        // 4-square Windows-style icon (2×2 grid)
        let ix = BTN_X + 10;
        let iy = 12u64;
        let sq = 6u64;
        let gap = 2u64;
        graphics.fill_rect(ix,           iy,           sq, sq, 0xFF00AAFF);
        graphics.fill_rect(ix + sq + gap, iy,           sq, sq, 0xFF007ACC);
        graphics.fill_rect(ix,           iy + sq + gap, sq, sq, 0xFF005A99);
        graphics.fill_rect(ix + sq + gap, iy + sq + gap, sq, sq, 0xFF004477);

        // "Start" text
        fonts::draw_string(graphics, BTN_X + 30, 14, "Start", COL_BTN_TEXT);
    }

    /// Draw the popup menu panel.  Only draws when `self.open`.
    pub fn draw_menu(&self, graphics: &Graphics) {
        if !self.open { return; }

        let mh = menu_height();

        // Drop shadow
        graphics.fill_rect(MENU_X + 4, MENU_Y + 4, MENU_W, mh, 0xFF04060A);

        // Menu background
        graphics.fill_rect(MENU_X, MENU_Y, MENU_W, mh, COL_MENU_BG);

        // Header gradient
        graphics.fill_rect_gradient_v(MENU_X, MENU_Y, MENU_W, HDR_H, COL_HDR_TOP, COL_HDR_BOT);
        graphics.fill_rect(MENU_X, MENU_Y + HDR_H - 1, MENU_W, 1, COL_BTN_BORDER);

        // OS name + small icon in header
        let hix = MENU_X + 10;
        let hiy = MENU_Y + 8;
        let sq = 5u64; let gap = 2u64;
        graphics.fill_rect(hix,           hiy,           sq, sq, 0xFF00AAFF);
        graphics.fill_rect(hix + sq + gap, hiy,           sq, sq, 0xFF007ACC);
        graphics.fill_rect(hix,           hiy + sq + gap, sq, sq, 0xFF005A99);
        graphics.fill_rect(hix + sq + gap, hiy + sq + gap, sq, sq, 0xFF004477);
        fonts::draw_string(graphics, MENU_X + 28, MENU_Y + 11, "OxideOS", COL_HDR_TEXT);
        fonts::draw_string(graphics, MENU_X + 28, MENU_Y + 22, "v0.1.0-dev", COL_ITEM_DESC);

        // Programs list
        let mut y = MENU_Y + HDR_H;
        let mut last_sec: Option<u8> = None;
        for (i, entry) in ENTRIES.iter().enumerate() {
            // Section header
            if last_sec != Some(entry.section) {
                last_sec = Some(entry.section);
                graphics.fill_rect(MENU_X, y, MENU_W, SEC_H, COL_SEC_BG);
                graphics.fill_rect(MENU_X, y, MENU_W, 1, 0xFF1A2030);
                graphics.fill_rect(MENU_X, y + SEC_H - 1, MENU_W, 1, 0xFF1A2030);
                fonts::draw_string(graphics, MENU_X + 10, y + 5,
                    SEC_NAMES[entry.section as usize], COL_SEC_TEXT);
                y += SEC_H;
            }

            let hovered = self.hovered_item == Some(i);
            if hovered {
                graphics.fill_rect(MENU_X + 1, y, MENU_W - 2, ITEM_H, COL_ITEM_HOVER);
            } else if i % 2 == 1 {
                graphics.fill_rect(MENU_X + 1, y, MENU_W - 2, ITEM_H, 0xFF0C1018);
            }

            // Accent dot
            let dot_col = if hovered { entry.accent | 0xFF000000 } else {
                // dim: keep hue but reduce brightness
                let r = ((entry.accent >> 16) & 0xFF) / 2;
                let g = ((entry.accent >>  8) & 0xFF) / 2;
                let b = ( entry.accent        & 0xFF) / 2;
                0xFF000000 | (r << 16) | (g << 8) | b
            };
            graphics.fill_rect(MENU_X + 8, y + ITEM_H / 2 - 3, 6, 6, dot_col);

            // Name + description
            let text_col = if hovered { 0xFFDDEEFF } else { COL_ITEM_TEXT };
            fonts::draw_string(graphics, MENU_X + 20, y + 6,  entry.name, text_col);
            fonts::draw_string(graphics, MENU_X + 20, y + 17, entry.desc, COL_ITEM_DESC);

            // Separator
            graphics.fill_rect(MENU_X + 20, y + ITEM_H - 1, MENU_W - 28, 1, 0xFF141C28);

            y += ITEM_H;
        }

        // Footer (shutdown / info row)
        graphics.fill_rect(MENU_X, y, MENU_W, FOOTER_H, COL_FOOT_BG);
        graphics.fill_rect(MENU_X, y, MENU_W, 1, COL_MENU_BDR);
        fonts::draw_string(graphics, MENU_X + 10, y + 9,
            "Click a program to launch it", COL_FOOT_TEXT);

        // Border around the whole menu
        graphics.draw_rect(MENU_X, MENU_Y, MENU_W, mh, COL_MENU_BDR, 1);
    }
}
