//! Kernel-native Notepad GUI window for OxideOS.
//!
//! A focused, editable text window integrated into the WindowManager z-order.
//! Keyboard input is consumed from the shared terminal key ring when focused.
//!
//! Keybindings:
//!   Arrow keys     — move cursor
//!   Ctrl+S (0x13)  — save to RamFS as /notes/<filename>
//!   Ctrl+N (0x0E)  — clear buffer (new file)
//!   Backspace/DEL  — delete character before cursor
//!   Enter          — insert newline
//!   Tab            — insert 4 spaces

extern crate alloc;
use alloc::string::String;
use alloc::format;

use crate::gui::fonts;
use crate::gui::graphics::Graphics;
use crate::gui::window_manager::WindowManager;
use crate::kernel::fs::ramfs::RAMFS;

// ── Layout ────────────────────────────────────────────────────────────────────
const CHAR_W:    u64 = 9;
const LINE_H:    u64 = 16;
const GUTTER_W:  u64 = 38;   // line-number gutter width
const PAD_X:     u64 = 8;
const TOOLBAR_H: u64 = 26;
const STATUS_H:  u64 = 20;

// ── Arrow-key event codes (matches terminal.rs encoding) ──────────────────────
const EV_UP:    u16 = 0x100;
const EV_DOWN:  u16 = 0x101;
const EV_LEFT:  u16 = 0x102;
const EV_RIGHT: u16 = 0x103;

// ── Buffer limits ─────────────────────────────────────────────────────────────
const MAX_LINES:   usize = 1024;
const MAX_LINE_LEN: usize = 256;

// ── Colour palette ────────────────────────────────────────────────────────────
const BG:          u32 = 0xFF1E1E1E;
const GUTTER_BG:   u32 = 0xFF252526;
const GUTTER_LINE: u32 = 0xFF333337;
const GUTTER_FG:   u32 = 0xFF858585;
const GUTTER_CUR:  u32 = 0xFFCCCCCC;
const CUR_LINE_BG: u32 = 0xFF282828;
const TEXT_COL:    u32 = 0xFFD4D4D4;
const CURSOR_COL:  u32 = 0xFFFFCC00;
const TOOLBAR_BG:  u32 = 0xFF2D2D30;
const TOOLBAR_SEP: u32 = 0xFF3F3F46;
const STATUS_BG:   u32 = 0xFF007ACC;
const STATUS_FG:   u32 = 0xFFFFFFFF;
const DIRTY_COL:   u32 = 0xFFFFCC00;
const BTN_COL:     u32 = 0xFFAAAAAA;
const BTN_HOT:     u32 = 0xFFFFFFFF;

pub struct NotepadApp {
    pub window_id: usize,
    lines:         [[u8; MAX_LINE_LEN]; MAX_LINES],
    line_lens:     [usize; MAX_LINES],
    num_lines:     usize,
    cursor_row:    usize,
    cursor_col:    usize,
    scroll_top:    usize,
    filename:      [u8; 64],
    filename_len:  usize,
    dirty:         bool,
}

impl NotepadApp {
    pub fn new(window_id: usize) -> Self {
        Self {
            window_id,
            lines:        [[0u8; MAX_LINE_LEN]; MAX_LINES],
            line_lens:    [0usize; MAX_LINES],
            num_lines:    1,
            cursor_row:   0,
            cursor_col:   0,
            scroll_top:   0,
            filename:     [0u8; 64],
            filename_len: 0,
            dirty:        false,
        }
    }

    pub fn window_id(&self) -> usize { self.window_id }

    // ── Input processing ──────────────────────────────────────────────────────

    pub fn process_input(&mut self, focused: bool) -> bool {
        if !focused { return false; }
        let mut changed = false;
        while let Some(ev) = crate::gui::terminal::pop_key_event() {
            changed = true;
            match ev {
                EV_UP    => self.move_up(),
                EV_DOWN  => self.move_down(),
                EV_LEFT  => self.move_left(),
                EV_RIGHT => self.move_right(),
                0x0E     => self.new_file(),   // Ctrl+N
                0x13     => self.save(),       // Ctrl+S
                ev if ev < 0x100 => {
                    let ch = ev as u8;
                    match ch {
                        b'\n' | b'\r' => self.insert_newline(),
                        8 | 127       => self.backspace(),
                        b'\t'         => { for _ in 0..4 { self.insert_char(b' '); } }
                        32..=126      => self.insert_char(ch),
                        _             => { changed = false; }
                    }
                }
                _ => { changed = false; }
            }
        }
        changed
    }

    // ── File operations ───────────────────────────────────────────────────────

    fn new_file(&mut self) {
        for i in 0..self.num_lines { self.line_lens[i] = 0; }
        self.num_lines    = 1;
        self.cursor_row   = 0;
        self.cursor_col   = 0;
        self.scroll_top   = 0;
        self.filename_len = 0;
        self.dirty        = false;
    }

    fn save(&mut self) {
        if self.filename_len == 0 {
            let default = b"/note.txt";
            self.filename[..default.len()].copy_from_slice(default);
            self.filename_len = default.len();
        }
        let path_bytes = &self.filename[..self.filename_len];
        let path_str = match core::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Serialise lines → flat byte buffer
        let mut buf = [0u8; MAX_LINES * (MAX_LINE_LEN + 1)];
        let mut pos = 0usize;
        for i in 0..self.num_lines {
            let len = self.line_lens[i];
            for j in 0..len {
                if pos < buf.len() { buf[pos] = self.lines[i][j]; pos += 1; }
            }
            if i + 1 < self.num_lines && pos < buf.len() {
                buf[pos] = b'\n'; pos += 1;
            }
        }

        unsafe {
            if let Some(fs) = RAMFS.get() {
                // create_file returns EEXIST if it already exists — that's fine
                let _ = fs.create_file(path_str);
                if let Some(idx) = fs.resolve(path_str) {
                    fs.inodes[idx].data.clear();
                    fs.inodes[idx].data.extend_from_slice(&buf[..pos]);
                }
            }
        }
        self.dirty = false;
    }

    // ── Edit operations ───────────────────────────────────────────────────────

    fn insert_char(&mut self, ch: u8) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        if row >= MAX_LINES { return; }
        let len = self.line_lens[row];
        if len >= MAX_LINE_LEN { return; }
        let line = &mut self.lines[row];
        for i in (col..len).rev() { line[i + 1] = line[i]; }
        line[col] = ch;
        self.line_lens[row] += 1;
        self.cursor_col += 1;
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        if self.num_lines >= MAX_LINES { return; }
        let row = self.cursor_row;
        let col = self.cursor_col;
        let old_len  = self.line_lens[row];
        let tail_len = old_len.saturating_sub(col);
        // Shift lines below down
        for i in (row + 1..self.num_lines).rev() {
            if i + 1 < MAX_LINES {
                self.lines[i + 1]    = self.lines[i];
                self.line_lens[i + 1] = self.line_lens[i];
            }
        }
        // New line = tail of current line
        let mut new_line = [0u8; MAX_LINE_LEN];
        new_line[..tail_len].copy_from_slice(&self.lines[row][col..col + tail_len]);
        self.line_lens[row]     = col;
        self.lines[row + 1]     = new_line;
        self.line_lens[row + 1] = tail_len;
        self.num_lines  += 1;
        self.cursor_row += 1;
        self.cursor_col  = 0;
        self.ensure_visible(20);
        self.dirty = true;
    }

    fn backspace(&mut self) {
        let row = self.cursor_col;  // re-assigned below
        let _ = row;
        let row = self.cursor_row;
        let col = self.cursor_col;
        if col > 0 {
            let len  = self.line_lens[row];
            let line = &mut self.lines[row];
            for i in (col - 1)..(len.saturating_sub(1)) { line[i] = line[i + 1]; }
            if len > 0 { line[len - 1] = 0; }
            self.line_lens[row] = len.saturating_sub(1);
            self.cursor_col -= 1;
            self.dirty = true;
        } else if row > 0 {
            let prev_len = self.line_lens[row - 1];
            let cur_len  = self.line_lens[row];
            let copy_len = cur_len.min(MAX_LINE_LEN.saturating_sub(prev_len));
            for i in 0..copy_len {
                self.lines[row - 1][prev_len + i] = self.lines[row][i];
            }
            self.line_lens[row - 1] = prev_len + copy_len;
            for i in row..(self.num_lines.saturating_sub(1)) {
                self.lines[i]    = self.lines[i + 1];
                self.line_lens[i] = self.line_lens[i + 1];
            }
            self.num_lines  = self.num_lines.saturating_sub(1);
            self.cursor_row -= 1;
            self.cursor_col  = prev_len;
            self.ensure_visible(20);
            self.dirty = true;
        }
    }

    // ── Cursor movement ───────────────────────────────────────────────────────

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.line_lens[self.cursor_row]);
            self.ensure_visible(20);
        }
    }
    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.num_lines {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.line_lens[self.cursor_row]);
            self.ensure_visible(20);
        }
    }
    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.line_lens[self.cursor_row];
            self.ensure_visible(20);
        }
    }
    fn move_right(&mut self) {
        if self.cursor_col < self.line_lens[self.cursor_row] {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.num_lines {
            self.cursor_row += 1;
            self.cursor_col = 0;
            self.ensure_visible(20);
        }
    }

    fn ensure_visible(&mut self, approx_rows: usize) {
        if self.cursor_row < self.scroll_top {
            self.scroll_top = self.cursor_row;
        } else if self.cursor_row >= self.scroll_top + approx_rows && approx_rows > 0 {
            self.scroll_top = self.cursor_row + 1 - approx_rows;
        }
    }

    // ── Drawing ───────────────────────────────────────────────────────────────

    pub fn draw(&self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(win) = wm.get_window(self.window_id) else { return; };
        let is_focused = wm.get_focused() == Some(self.window_id);

        let cx = win.x + 1;
        let cy = win.y + 31;          // content starts below WM title bar
        let cw = win.width.saturating_sub(2);
        let ch = win.height.saturating_sub(32);

        // ── Toolbar ───────────────────────────────────────────────────────────
        graphics.fill_rect(cx, cy, cw, TOOLBAR_H, TOOLBAR_BG);
        graphics.fill_rect(cx, cy + TOOLBAR_H, cw, 1, TOOLBAR_SEP);

        let mut tx = cx + 8;
        let new_col = if !self.dirty { BTN_HOT } else { BTN_COL };
        fonts::draw_string(graphics, tx, cy + 5, "[Ctrl+N]", new_col);
        tx += 9 * CHAR_W;

        let save_col = if self.dirty { DIRTY_COL } else { BTN_COL };
        fonts::draw_string(graphics, tx, cy + 5, "[Ctrl+S]", save_col);
        tx += 9 * CHAR_W + 12;

        // Dot indicator + filename
        let dot_col  = if self.dirty { DIRTY_COL } else { 0xFF555555 };
        fonts::draw_string(graphics, tx, cy + 5, "\u{25CF}", dot_col);
        tx += 2 * CHAR_W;
        let fname = if self.filename_len > 0 {
            core::str::from_utf8(&self.filename[..self.filename_len]).unwrap_or("untitled")
        } else {
            "untitled"
        };
        fonts::draw_string(graphics, tx, cy + 5, fname, 0xFFAAAAAA);

        // ── Text area ─────────────────────────────────────────────────────────
        let text_top = cy + TOOLBAR_H + 1;
        let text_h   = ch.saturating_sub(TOOLBAR_H + 1 + STATUS_H);

        graphics.fill_rect(cx, text_top, cw, text_h, BG);

        // Gutter
        graphics.fill_rect(cx, text_top, GUTTER_W, text_h, GUTTER_BG);
        graphics.fill_rect(cx + GUTTER_W, text_top, 1, text_h, GUTTER_LINE);

        let visible_rows = ((text_h / LINE_H) as usize).max(1);
        let text_x       = cx + GUTTER_W + PAD_X;
        let usable_w     = cw.saturating_sub(GUTTER_W + PAD_X + 4);
        let max_cols     = (usable_w / CHAR_W) as usize;

        // Clamp scroll so cursor stays visible
        let scroll_top = {
            let mut st = self.scroll_top;
            if self.cursor_row < st {
                st = self.cursor_row;
            } else if self.cursor_row >= st + visible_rows {
                st = self.cursor_row + 1 - visible_rows;
            }
            st
        };

        for i in 0..visible_rows {
            let row = scroll_top + i;
            if row >= self.num_lines { break; }
            let y = text_top + i as u64 * LINE_H;

            let is_cur = row == self.cursor_row;

            // Current-line highlight
            if is_cur && is_focused {
                graphics.fill_rect(cx + GUTTER_W + 1, y, cw.saturating_sub(GUTTER_W + 1), LINE_H, CUR_LINE_BG);
            }

            // Line number (right-aligned in gutter)
            let gnum_col = if is_cur { GUTTER_CUR } else { GUTTER_FG };
            draw_linenum(graphics, cx + 2, y + 1, row + 1, gnum_col);

            // Text content
            let len = self.line_lens[row].min(max_cols);
            if let Ok(s) = core::str::from_utf8(&self.lines[row][..len]) {
                fonts::draw_string(graphics, text_x, y + 1, s, TEXT_COL);
            }

            // Cursor bar
            if is_cur && is_focused {
                let col_clamped = self.cursor_col.min(max_cols);
                let cx_cur = text_x + col_clamped as u64 * CHAR_W;
                graphics.fill_rect(cx_cur, y + 1, 2, LINE_H - 2, CURSOR_COL);
            }
        }

        // ── Status bar ────────────────────────────────────────────────────────
        let sy = cy + ch.saturating_sub(STATUS_H);
        graphics.fill_rect(cx, sy, cw, STATUS_H, STATUS_BG);

        let status = format!(
            "  Ln {}, Col {}  |  {} lines  |  {}  ",
            self.cursor_row + 1,
            self.cursor_col + 1,
            self.num_lines,
            if self.dirty { "Modified" } else { "Saved" },
        );
        let s = if status.len() > 55 { &status[..55] } else { status.as_str() };
        fonts::draw_string(graphics, cx + 4, sy + 3, s, STATUS_FG);

        // Hint on the right
        let hint = "Ctrl+S: save  Ctrl+N: new";
        let hint_x = cx + cw.saturating_sub(hint.len() as u64 * CHAR_W + 6);
        fonts::draw_string(graphics, hint_x, sy + 3, hint, 0xFFCCDDFF);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn draw_linenum(graphics: &Graphics, x: u64, y: u64, num: usize, color: u32) {
    // Render up to 4 decimal digits right-aligned inside the gutter.
    let mut digits = [0u8; 4];
    let mut n      = num;
    let mut count  = 0usize;
    while n > 0 && count < 4 { digits[count] = (n % 10) as u8; n /= 10; count += 1; }
    if count == 0 { digits[0] = 0; count = 1; }
    // Right-justify: available width = GUTTER_W - 4 (2px left pad, 2px margin)
    let right_edge_x = x + GUTTER_W - 6;
    let start_x      = right_edge_x.saturating_sub((count as u64).saturating_sub(1) * CHAR_W);
    for i in (0..count).rev() {
        let cx = start_x + (count - 1 - i) as u64 * CHAR_W;
        fonts::draw_char(graphics, cx, y, (b'0' + digits[i]) as char, color);
    }
}
