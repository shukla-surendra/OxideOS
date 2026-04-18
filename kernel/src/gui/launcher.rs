//! Program launcher for OxideOS.
//!
//! Displays all registered programs as clickable tiles.
//! Clicking a tile immediately spawns the program as a new task.

use crate::gui::colors;
use crate::gui::fonts;
use crate::gui::graphics::Graphics;
use crate::gui::window_manager::WindowManager;

// ── Layout constants ──────────────────────────────────────────────────────────

const TITLE_BAR_H: u64 = 31;
const COLS:        usize = 3;
const TILE_W:      u64 = 160;
const TILE_H:      u64 = 72;
const GAP_X:       u64 = 10;
const GAP_Y:       u64 = 10;
const PAD_X:       u64 = 12;
const PAD_Y:       u64 = 10;

// ── Colour palette ────────────────────────────────────────────────────────────

const COL_WIN_BG:       u32 = 0xFF08111E;
const COL_TILE_BG:      u32 = 0xFF0E1A2E;
const COL_TILE_HOVER:   u32 = 0xFF142030;
const COL_TILE_BORDER:  u32 = 0xFF1E3050;
const COL_TILE_ACTIVE:  u32 = 0xFF1A5F9A;
const COL_NAME:         u32 = 0xFFDDEEFF;
const COL_DESC:         u32 = 0xFF5070A0;
const COL_HINT:         u32 = 0xFF283848;
const COL_SECTION_LABEL:u32 = 0xFF3A6090;
const COL_SECTION_LINE: u32 = 0xFF162030;

// ── Program entries ───────────────────────────────────────────────────────────

struct Entry {
    name:    &'static str,
    desc:    &'static str,
    accent:  u32,   // top-strip accent colour
    section: u8,    // 0=Shell, 1=Demo, 2=Interactive
}

const ENTRIES: &[Entry] = &[
    // ── Shell / terminal ──────────────────────────────────────
    Entry { name: "terminal",    desc: "GUI Terminal (IPC)",   accent: 0xFF007ACC, section: 0 },
    Entry { name: "sh",          desc: "Minimal Shell",        accent: 0xFF1A9A50, section: 0 },
    Entry { name: "filemanager", desc: "GUI File Manager",     accent: 0xFF8844CC, section: 0 },
    // ── Rust programs ─────────────────────────────────────────
    Entry { name: "hello_rust",  desc: "Hello from Rust",      accent: 0xFFCC6633, section: 1 },
    Entry { name: "sysinfo",     desc: "System Information",   accent: 0xFF3399AA, section: 1 },
    Entry { name: "fib",         desc: "Fibonacci Sequence",   accent: 0xFF2277CC, section: 1 },
    Entry { name: "primes",      desc: "Primes up to 100",     accent: 0xFF227788, section: 1 },
    // ── ASM demos ─────────────────────────────────────────────
    Entry { name: "hello",       desc: "Hello World (asm)",    accent: 0xFF886644, section: 1 },
    Entry { name: "counter",     desc: "Count 1 – 9",          accent: 0xFF446688, section: 1 },
    Entry { name: "countdown",   desc: "Countdown 10 → 1",     accent: 0xFF996633, section: 1 },
    Entry { name: "spinner",     desc: "Spinner Animation",    accent: 0xFF886699, section: 1 },
    Entry { name: "filetest",    desc: "File I/O Demo",        accent: 0xFF885533, section: 1 },
    // ── Interactive ───────────────────────────────────────────
    Entry { name: "input",       desc: "Stdin Echo (Ctrl-C)",  accent: 0xFF558844, section: 2 },
];

const SECTION_LABELS: &[&str] = &["Shell / Terminal / GUI", "Programs", "Interactive"];

// ── Launcher application ──────────────────────────────────────────────────────

pub struct LauncherApp {
    pub window_id:    usize,
    /// Index of the tile currently under the mouse (for hover highlight).
    hovered:          Option<usize>,
    /// Tile launched on last click — shown briefly in a different colour.
    last_launched:    Option<usize>,
    /// Ticks since the last launch highlight started (cleared after a few frames).
    launched_frames:  u8,
    /// Status message shown in the header.
    status:           [u8; 48],
    status_len:       usize,
    status_color:     u32,
}

impl LauncherApp {
    pub fn new(window_id: usize) -> Self {
        let mut app = Self {
            window_id,
            hovered:         None,
            last_launched:   None,
            launched_frames: 0,
            status:          [0; 48],
            status_len:      0,
            status_color:    COL_DESC,
        };
        app.set_status("Click a tile to launch a program.", COL_DESC);
        app
    }

    fn set_status(&mut self, msg: &str, color: u32) {
        let bytes = msg.as_bytes();
        let len   = bytes.len().min(self.status.len());
        self.status[..len].copy_from_slice(&bytes[..len]);
        self.status_len  = len;
        self.status_color = color;
    }

    fn status_str(&self) -> &str {
        core::str::from_utf8(&self.status[..self.status_len]).unwrap_or("")
    }

    // ── Geometry ──────────────────────────────────────────────────────────────

    /// Tile content-area position for tile `i`.
    fn tile_rect(i: usize) -> (u64, u64) {
        let col = (i % COLS) as u64;
        let row = (i / COLS) as u64;
        // Account for section labels — count how many section headers appear before row `row`.
        // Simple: each section adds 18px of label height before its first row.
        let logical_row = i / COLS;
        let mut section_offset = 0u64;
        let mut prev_section: Option<u8> = None;
        for j in 0..i {
            if j % COLS == 0 {
                let sec = ENTRIES[j].section;
                if prev_section != Some(sec) {
                    section_offset += 18; // section label height
                    prev_section = Some(sec);
                }
            }
        }
        // Section label for THIS tile
        let this_sec = ENTRIES[i].section;
        if prev_section != Some(this_sec) {
            section_offset += 18;
        }

        let x = PAD_X + col * (TILE_W + GAP_X);
        let y = PAD_Y + section_offset + logical_row as u64 * (TILE_H + GAP_Y);
        (x, y)
    }

    // ── Mouse events ──────────────────────────────────────────────────────────

    /// Update hover state given absolute mouse position.
    /// Returns true if the hover state changed (needs redraw).
    pub fn handle_mouse_move(&mut self, wm: &WindowManager, abs_x: u64, abs_y: u64) -> bool {
        let prev = self.hovered;
        self.hovered = self.tile_at(wm, abs_x, abs_y);
        self.hovered != prev
    }

    /// Handle a mouse click.  Returns the program name if a tile was hit.
    pub fn handle_click(&mut self, wm: &WindowManager, abs_x: u64, abs_y: u64) -> Option<&'static str> {
        let idx = self.tile_at(wm, abs_x, abs_y)?;
        let entry = &ENTRIES[idx];
        self.last_launched   = Some(idx);
        self.launched_frames = 0;
        let mut msg = [0u8; 48];
        let prefix = b"Launching: ";
        msg[..prefix.len()].copy_from_slice(prefix);
        let n = entry.name.len().min(48 - prefix.len());
        msg[prefix.len()..prefix.len() + n].copy_from_slice(&entry.name.as_bytes()[..n]);
        let msg_str = core::str::from_utf8(&msg[..prefix.len() + n]).unwrap_or("");
        self.set_status(msg_str, 0xFF40C870);
        Some(entry.name)
    }

    /// Called each frame; clears the "launched" highlight after a few frames.
    pub fn tick(&mut self) -> bool {
        if self.last_launched.is_some() {
            self.launched_frames += 1;
            if self.launched_frames > 12 {
                self.last_launched   = None;
                self.launched_frames = 0;
                self.set_status("Click a tile to launch a program.", COL_DESC);
                return true;
            }
        }
        false
    }

    fn tile_at(&self, wm: &WindowManager, abs_x: u64, abs_y: u64) -> Option<usize> {
        let win = wm.get_window(self.window_id)?;
        if abs_y < win.y + TITLE_BAR_H { return None; }
        let rx = abs_x.wrapping_sub(win.x);
        let ry = abs_y.wrapping_sub(win.y + TITLE_BAR_H);
        for i in 0..ENTRIES.len() {
            let (tx, ty) = Self::tile_rect(i);
            if rx >= tx && rx < tx + TILE_W && ry >= ty && ry < ty + TILE_H {
                return Some(i);
            }
        }
        None
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) { return; }
        let win = match wm.get_window(self.window_id) {
            Some(w) => w,
            None    => return,
        };

        let ox = win.x;
        let oy = win.y + TITLE_BAR_H;
        let content_w = win.width;
        let content_h = win.height.saturating_sub(TITLE_BAR_H);

        // Window background
        graphics.fill_rect(ox, oy, content_w, content_h, COL_WIN_BG);

        // Status bar at the top of content area
        graphics.fill_rect(ox, oy, content_w, 20, 0xFF0A1828);
        graphics.fill_rect(ox, oy + 20, content_w, 1, 0xFF1A3050);
        fonts::draw_string(graphics, ox + 8, oy + 5, self.status_str(), self.status_color);

        // Content starts below status bar
        let oy = oy + 22;

        // Draw tiles grouped by section
        let mut prev_section: Option<u8> = None;
        for (i, entry) in ENTRIES.iter().enumerate() {
            // Section label
            if prev_section != Some(entry.section) {
                prev_section = Some(entry.section);
                let (tx, ty) = Self::tile_rect(i);
                let label_y = oy + ty - 16;
                if label_y > oy.saturating_sub(4) {
                    fonts::draw_string(graphics, ox + tx, label_y,
                        SECTION_LABELS[entry.section as usize], COL_SECTION_LABEL);
                    graphics.fill_rect(ox + tx, label_y + 13,
                        content_w.saturating_sub(tx + 4), 1, COL_SECTION_LINE);
                }
            }

            let (tx, ty) = Self::tile_rect(i);
            self.draw_tile(graphics, ox + tx, oy + ty, i, entry);
        }
    }

    fn draw_tile(&self, graphics: &Graphics, ax: u64, ay: u64, idx: usize, entry: &Entry) {
        let is_launched = self.last_launched == Some(idx);
        let is_hovered  = self.hovered == Some(idx);

        let bg     = if is_launched { 0xFF0A2040 }
                     else if is_hovered { COL_TILE_HOVER }
                     else { COL_TILE_BG };
        let border = if is_launched { 0xFF00AAFF }
                     else if is_hovered { COL_TILE_ACTIVE }
                     else { COL_TILE_BORDER };

        // Soft shadow for hovered tile
        if is_hovered {
            graphics.draw_soft_shadow(ax, ay, TILE_W, TILE_H, 6, 0x30);
        }

        // Background & border
        graphics.fill_rounded_rect(ax, ay, TILE_W, TILE_H, 6, bg);
        graphics.draw_rounded_rect(ax, ay, TILE_W, TILE_H, 6, border, 1);

        // Accent strip (rounded at top)
        graphics.fill_rounded_rect(ax + 1, ay + 1, TILE_W - 2, 8, 4, entry.accent);
        // Cover bottom part of accent strip to make it flat
        graphics.fill_rect(ax + 1, ay + 5, TILE_W - 2, 4, entry.accent);
        
        // Subtle gradient fade below accent
        let dimmed = (entry.accent & 0xFFFFFF) | 0x44000000;
        graphics.fill_rect(ax + 1, ay + 9, TILE_W - 2, 2, dimmed);

        // Program name (bold feel: draw twice offset by 1)
        fonts::draw_string(graphics, ax + 8, ay + 17, entry.name, 0xFF000000); // shadow
        fonts::draw_string(graphics, ax + 7, ay + 16, entry.name, COL_NAME);

        // Description
        fonts::draw_string(graphics, ax + 8, ay + 34, entry.desc, COL_DESC);

        // Bottom hint row
        let hint_bg = if is_hovered || is_launched { 0xFF0A1C34 } else { 0xFF090F1A };
        graphics.fill_rounded_rect(ax + 1, ay + TILE_H - 18, TILE_W - 2, 17, 4, hint_bg);
        // Cover top part of hint bg to make it flat
        graphics.fill_rect(ax + 1, ay + TILE_H - 18, TILE_W - 2, 8, hint_bg);
        
        graphics.fill_rect(ax + 1, ay + TILE_H - 19, TILE_W - 2, 1, border);

        let hint_txt = if is_launched { "  spawning..." }
                       else if is_hovered { "  click to run" }
                       else { "  click to run" };
        let hint_col = if is_launched { 0xFF40C870 }
                       else if is_hovered { 0xFF60AADD }
                       else { COL_HINT };
        fonts::draw_string(graphics, ax + 2, ay + TILE_H - 14, hint_txt, hint_col);
    }
}
