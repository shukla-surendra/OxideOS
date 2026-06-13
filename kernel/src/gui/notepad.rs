//! Kernel-native Notepad GUI — full-featured text editor.
//!
//! Menu bar:
//!   File   — Open (^O), New (^N), Save (^S), Save As, Exit
//!   Edit   — Undo (^Z), Redo (^Y), Cut (^X), Copy (^C), Paste (^V), Select All (^A), Find (^F)
//!   Format — Word Wrap (toggle)
//!   View   — Status Bar (toggle)
//!   Help   — About
//!
//! Keyboard shortcuts:
//!   Ctrl+O      — open file (inline path bar)
//!   Ctrl+N      — new file
//!   Ctrl+S      — save
//!   Ctrl+Z      — undo
//!   Ctrl+Y      — redo
//!   Ctrl+A      — select all
//!   Ctrl+C      — copy selection
//!   Ctrl+X      — cut selection
//!   Ctrl+V      — paste
//!   Ctrl+F      — find (inline search bar)
//!   Shift+Arrow — extend selection
//!   Arrow keys  — cursor movement (clears selection)
//!   Backspace   — delete / delete selection
//!   Enter / Tab — insert

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

use crate::gui::fonts;
use crate::gui::graphics::Graphics;
use crate::gui::menu::{MenuBar, MenuAction, Menu, MenuItem, MENUBAR_H};
use crate::gui::window_manager::WindowManager;
use crate::gui::terminal::{EVENT_SHIFT_UP, EVENT_SHIFT_DOWN, EVENT_SHIFT_LEFT, EVENT_SHIFT_RIGHT};
use crate::kernel::fs::ramfs::RAMFS;

// ── Persistent disk helpers ──────────────────────────────────────────────────

/// True for paths under the persistent FAT16 mount (`/disk` or `/disk/...`).
fn is_disk_path(path: &str) -> bool {
    path == "/disk" || path.starts_with("/disk/")
}

/// Write `data` to `path` on the FAT16 disk, creating/truncating as needed.
/// Returns `true` on success.
unsafe fn save_to_disk(path: &[u8], data: &[u8]) -> bool {
    if !crate::kernel::ata::is_present() { return false; }
    use crate::kernel::fs::{O_WRONLY, O_CREAT, O_TRUNC};
    let fd = unsafe { crate::kernel::fat::open(path, O_WRONLY | O_CREAT | O_TRUNC) };
    if fd < 0 { return false; }
    let mut off = 0usize;
    while off < data.len() {
        let n = unsafe { crate::kernel::fat::write_fd(fd as i32, &data[off..]) };
        if n <= 0 { break; }
        off += n as usize;
    }
    unsafe { crate::kernel::fat::close(fd as i32); }
    off == data.len()
}

/// Read the full contents of `path` from the FAT16 disk.
/// Returns `None` if the disk is absent or the file cannot be opened.
unsafe fn load_from_disk(path: &[u8]) -> Option<Vec<u8>> {
    if !crate::kernel::ata::is_present() { return None; }
    use crate::kernel::fs::O_RDONLY;
    let fd = unsafe { crate::kernel::fat::open(path, O_RDONLY) };
    if fd < 0 { return None; }
    let mut data = Vec::new();
    let mut chunk = [0u8; 512];
    loop {
        let n = unsafe { crate::kernel::fat::read_fd(fd as i32, &mut chunk) };
        if n <= 0 { break; }
        data.extend_from_slice(&chunk[..n as usize]);
    }
    unsafe { crate::kernel::fat::close(fd as i32); }
    Some(data)
}

// ── Layout ─────────────────────────────────────────────────────────────────────
const CHAR_W:      u64 = 9;
const LINE_H:      u64 = 16;
const GUTTER_W:    u64 = 38;
const PAD_X:       u64 = 8;
const STATUS_H:    u64 = 20;
const FIND_BAR_H:  u64 = 26;

// ── Key event codes ────────────────────────────────────────────────────────────
const EV_UP:    u16 = 0x100;
const EV_DOWN:  u16 = 0x101;
const EV_LEFT:  u16 = 0x102;
const EV_RIGHT: u16 = 0x103;

// ── Buffer limits ──────────────────────────────────────────────────────────────
const MAX_LINES:    usize = 1024;
const MAX_LINE_LEN: usize = 256;

// ── Undo/Redo ─────────────────────────────────────────────────────────────────
const UNDO_CAP: usize = 256;

#[derive(Copy, Clone)]
enum UndoOp {
    /// A character was inserted (insert=true) or deleted (insert=false) at (row, col).
    Char { row: u16, col: u16, ch: u8, insert: bool },
    /// A newline was inserted (insert=true) or deleted (insert=false).
    /// For insert: at position (row, col) in the original line.
    /// For delete (backspace at col=0): join point was end of row `row` at col `col`.
    Newline { row: u16, col: u16, insert: bool },
}

const UNDO_ZERO: UndoOp = UndoOp::Char { row: 0, col: 0, ch: 0, insert: false };

// ── Input bar mode ─────────────────────────────────────────────────────────────
#[derive(Copy, Clone, PartialEq)]
enum BarMode { None, Find, Open }

// ── Clipboard (module-level global) ───────────────────────────────────────────
const CLIPBOARD_MAX: usize = 4096;
static mut CLIPBOARD: [u8; CLIPBOARD_MAX] = [0u8; CLIPBOARD_MAX];
static mut CLIPBOARD_LEN: usize = 0;

// ── Colours ────────────────────────────────────────────────────────────────────
const BG:           u32 = 0xFF1E1E1E;
const GUTTER_BG:    u32 = 0xFF252526;
const GUTTER_LINE:  u32 = 0xFF333337;
const GUTTER_FG:    u32 = 0xFF858585;
const GUTTER_CUR:   u32 = 0xFFCCCCCC;
const CUR_LINE_BG:  u32 = 0xFF282828;
const TEXT_COL:     u32 = 0xFFD4D4D4;
const CURSOR_COL:   u32 = 0xFFFFCC00;
const STATUS_BG:    u32 = 0xFF007ACC;
const STATUS_FG:    u32 = 0xFFFFFFFF;
const SEL_BG:       u32 = 0xFF264F78;
const FIND_MATCH_BG:u32 = 0xFF6B4F1A;
const FIND_BAR_BG:  u32 = 0xFF1A1A2E;
const FIND_BAR_BORDER: u32 = 0xFF3F3F6E;
const FIND_INPUT_BG:u32 = 0xFF2D2D42;

// ── Menu indices ───────────────────────────────────────────────────────────────
const MENU_FORMAT:     usize = 2;
const MENU_VIEW:       usize = 3;
const FORMAT_WORDWRAP: usize = 0;
const VIEW_STATUSBAR:  usize = 0;

// ── About dialog ──────────────────────────────────────────────────────────────
const ABOUT_BG:     u32 = 0xFF252526;
const ABOUT_BORDER: u32 = 0xFF3F3F46;
const ABOUT_H:      u64 = 110;
const ABOUT_W:      u64 = 260;

// ── NotepadApp ─────────────────────────────────────────────────────────────────

pub struct NotepadApp {
    pub window_id:   usize,
    // Buffer
    lines:           [[u8; MAX_LINE_LEN]; MAX_LINES],
    line_lens:       [usize; MAX_LINES],
    num_lines:       usize,
    cursor_row:      usize,
    cursor_col:      usize,
    scroll_top:      usize,
    filename:        [u8; 64],
    filename_len:    usize,
    dirty:           bool,
    // Undo/Redo ring buffers
    undo_buf:        [UndoOp; UNDO_CAP],
    undo_head:       usize,
    undo_len:        usize,
    redo_buf:        [UndoOp; UNDO_CAP],
    redo_len:        usize,
    // Selection
    sel_active:      bool,
    sel_anchor_row:  usize,
    sel_anchor_col:  usize,
    // Inline input bar (find / open)
    bar_mode:        BarMode,
    bar_text:        [u8; 64],
    bar_len:         usize,
    find_match_row:  usize,
    find_match_col:  usize,
    find_has_match:  bool,
    // Menu bar
    menu:            MenuBar,
    // View toggles
    word_wrap:       bool,
    show_status_bar: bool,
    // About dialog
    show_about:      bool,
    // Layout cache
    last_bar_x:      u64,
    last_bar_y:      u64,
    last_bar_w:      u64,
}

impl NotepadApp {
    pub fn new(window_id: usize) -> Self {
        let mut app = Self {
            window_id,
            lines:           [[0u8; MAX_LINE_LEN]; MAX_LINES],
            line_lens:       [0usize; MAX_LINES],
            num_lines:       1,
            cursor_row:      0,
            cursor_col:      0,
            scroll_top:      0,
            filename:        [0u8; 64],
            filename_len:    0,
            dirty:           false,
            undo_buf:        [UNDO_ZERO; UNDO_CAP],
            undo_head:       0,
            undo_len:        0,
            redo_buf:        [UNDO_ZERO; UNDO_CAP],
            redo_len:        0,
            sel_active:      false,
            sel_anchor_row:  0,
            sel_anchor_col:  0,
            bar_mode:        BarMode::None,
            bar_text:        [0u8; 64],
            bar_len:         0,
            find_match_row:  0,
            find_match_col:  0,
            find_has_match:  false,
            menu:            MenuBar::new(),
            word_wrap:       false,
            show_status_bar: true,
            show_about:      false,
            last_bar_x:      0,
            last_bar_y:      0,
            last_bar_w:      0,
        };
        app.build_menu();
        app
    }

    pub fn window_id(&self) -> usize { self.window_id }

    // ── Menu construction ──────────────────────────────────────────────────────

    fn build_menu(&mut self) {
        let mut file = Menu::new("File");
        file.add(MenuItem::item("Open...",  "^O", MenuAction::FileOpen));
        file.add(MenuItem::item("New",      "^N", MenuAction::FileNew));
        file.add(MenuItem::sep());
        file.add(MenuItem::item("Save",     "^S", MenuAction::FileSave));
        file.add(MenuItem::item("Save As",  "",   MenuAction::FileSaveAs));
        file.add(MenuItem::sep());
        file.add(MenuItem::item("Exit",     "",   MenuAction::FileExit));
        self.menu.add_menu(file);

        let mut edit = Menu::new("Edit");
        edit.add(MenuItem::item("Undo",       "^Z", MenuAction::EditUndo));
        edit.add(MenuItem::item("Redo",       "^Y", MenuAction::EditRedo));
        edit.add(MenuItem::sep());
        edit.add(MenuItem::item("Cut",        "^X", MenuAction::EditCut));
        edit.add(MenuItem::item("Copy",       "^C", MenuAction::EditCopy));
        edit.add(MenuItem::item("Paste",      "^V", MenuAction::EditPaste));
        edit.add(MenuItem::sep());
        edit.add(MenuItem::item("Select All", "^A", MenuAction::EditSelectAll));
        edit.add(MenuItem::sep());
        edit.add(MenuItem::item("Find",       "^F", MenuAction::EditFind));
        self.menu.add_menu(edit);

        let mut fmt = Menu::new("Format");
        fmt.add(MenuItem::checked_item("Word Wrap", "", MenuAction::FormatWordWrap, false));
        self.menu.add_menu(fmt);

        let mut view = Menu::new("View");
        view.add(MenuItem::checked_item("Status Bar", "", MenuAction::ViewStatusBar, true));
        self.menu.add_menu(view);

        let mut help = Menu::new("Help");
        help.add(MenuItem::item("About Notepad", "", MenuAction::HelpAbout));
        self.menu.add_menu(help);
    }

    // ── Keyboard input ─────────────────────────────────────────────────────────

    pub fn process_input(&mut self, focused: bool) -> bool {
        if !focused { return false; }
        let mut changed = false;
        while let Some(ev) = crate::gui::terminal::pop_key_event() {
            changed = true;

            // ── Input bar (find / open) captures all input ─────────────────────
            if self.bar_mode != BarMode::None {
                match ev {
                    ev if ev == 0x1B => {              // Escape
                        self.bar_mode = BarMode::None;
                        self.bar_len  = 0;
                        self.find_has_match = false;
                    }
                    ev if ev == b'\n' as u16 || ev == b'\r' as u16 => {
                        match self.bar_mode {
                            BarMode::Find => self.find_next(),
                            BarMode::Open => self.open_file(),
                            BarMode::None => {}
                        }
                    }
                    ev if ev == 8 || ev == 127 => {   // Backspace in bar
                        if self.bar_len > 0 {
                            self.bar_len -= 1;
                            self.bar_text[self.bar_len] = 0;
                            if self.bar_mode == BarMode::Find {
                                self.find_has_match = false;
                                if self.bar_len > 0 { self.find_next_from_start(); }
                            }
                        }
                    }
                    ev if ev >= 0x20 && ev < 0x7F => { // Printable
                        if self.bar_len < 63 {
                            self.bar_text[self.bar_len] = ev as u8;
                            self.bar_len += 1;
                            if self.bar_mode == BarMode::Find {
                                self.find_has_match = false;
                                self.find_next_from_start();
                            }
                        }
                    }
                    _ => { changed = false; }
                }
                continue;
            }

            // ── Normal editor input ────────────────────────────────────────────
            match ev {
                // Arrow keys — clear selection, move cursor
                EV_UP    => { self.sel_active = false; self.move_up(); }
                EV_DOWN  => { self.sel_active = false; self.move_down(); }
                EV_LEFT  => { self.sel_active = false; self.move_left(); }
                EV_RIGHT => { self.sel_active = false; self.move_right(); }
                // Shift+Arrows — extend selection
                EVENT_SHIFT_UP    => { self.start_sel(); self.move_up(); }
                EVENT_SHIFT_DOWN  => { self.start_sel(); self.move_down(); }
                EVENT_SHIFT_LEFT  => { self.start_sel(); self.move_left(); }
                EVENT_SHIFT_RIGHT => { self.start_sel(); self.move_right(); }
                // Ctrl shortcuts
                0x0F => self.open_bar(),       // Ctrl+O
                0x0E => self.new_file(),       // Ctrl+N
                0x13 => self.save(),           // Ctrl+S
                0x1A => self.undo(),           // Ctrl+Z
                0x19 => self.redo(),           // Ctrl+Y
                0x01 => self.select_all(),     // Ctrl+A
                0x03 => self.copy_selection(), // Ctrl+C
                0x18 => { self.copy_selection(); self.delete_selection(); } // Ctrl+X
                0x16 => self.paste_clipboard(),// Ctrl+V
                0x06 => self.find_bar(),       // Ctrl+F
                // Printable + special
                ev if ev < 0x100 => {
                    let ch = ev as u8;
                    match ch {
                        b'\n' | b'\r' => {
                            if self.sel_active { self.delete_selection(); }
                            self.insert_newline();
                        }
                        8 | 127 => {
                            if self.sel_active { self.delete_selection(); }
                            else { self.backspace(); }
                        }
                        b'\t' => {
                            if self.sel_active { self.delete_selection(); }
                            for _ in 0..4 { self.insert_char(b' '); }
                        }
                        32..=126 => {
                            if self.sel_active { self.delete_selection(); }
                            self.insert_char(ch);
                        }
                        _ => { changed = false; }
                    }
                }
                _ => { changed = false; }
            }
        }
        changed
    }

    // ── Menu mouse handling ────────────────────────────────────────────────────

    pub fn handle_mouse_move(&mut self, mx: u64, my: u64, wm: &WindowManager) -> bool {
        let Some((bx, by, bw)) = self.menu_bar_coords(wm) else { return false; };
        self.menu.handle_mouse_move(mx, my, bx, by, bw)
    }

    pub fn handle_click(&mut self, mx: u64, my: u64, wm: &WindowManager) -> MenuAction {
        let Some((bx, by, bw)) = self.menu_bar_coords(wm) else { return MenuAction::None; };

        let was_open = self.menu.is_open();
        if !self.menu.hit_test(mx, my, bx, by, bw) && !was_open {
            return MenuAction::None;
        }

        let action = self.menu.handle_click(mx, my, bx, by, bw);
        if action != MenuAction::FileExit {
            self.apply_action(action);
        }
        if action == MenuAction::None && was_open {
            return MenuAction::Consumed;
        }
        action
    }

    pub fn draw_dropdown_overlay(&self, graphics: &Graphics, wm: &WindowManager) {
        if !self.menu.is_open() { return; }
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(win) = wm.get_window(self.window_id) else { return; };
        let cy = win.y + 31;
        self.menu.draw_overlay(graphics, cy);
    }

    fn apply_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::None | MenuAction::Consumed => {}
            MenuAction::FileOpen      => self.open_bar(),
            MenuAction::FileNew       => self.new_file(),
            MenuAction::FileSave      => self.save(),
            MenuAction::FileSaveAs    => self.save(),
            MenuAction::FileExit      => {}
            MenuAction::EditUndo      => self.undo(),
            MenuAction::EditRedo      => self.redo(),
            MenuAction::EditCut       => { self.copy_selection(); self.delete_selection(); }
            MenuAction::EditCopy      => self.copy_selection(),
            MenuAction::EditPaste     => self.paste_clipboard(),
            MenuAction::EditSelectAll => self.select_all(),
            MenuAction::EditFind      => self.find_bar(),
            MenuAction::FormatWordWrap => {
                self.word_wrap = !self.word_wrap;
                self.menu.set_checked(MENU_FORMAT, FORMAT_WORDWRAP, self.word_wrap);
            }
            MenuAction::ViewStatusBar => {
                self.show_status_bar = !self.show_status_bar;
                self.menu.set_checked(MENU_VIEW, VIEW_STATUSBAR, self.show_status_bar);
            }
            MenuAction::HelpAbout => {
                self.show_about = !self.show_about;
            }
        }
    }

    // ── Coordinate helper ──────────────────────────────────────────────────────

    fn menu_bar_coords(&self, wm: &WindowManager) -> Option<(u64, u64, u64)> {
        let win = wm.get_window(self.window_id)?;
        let bx = win.x + 1;
        let by = win.y + 31;
        let bw = win.width.saturating_sub(2);
        Some((bx, by, bw))
    }

    // ── File operations ────────────────────────────────────────────────────────

    fn new_file(&mut self) {
        for i in 0..self.num_lines { self.line_lens[i] = 0; }
        self.num_lines    = 1;
        self.cursor_row   = 0;
        self.cursor_col   = 0;
        self.scroll_top   = 0;
        self.filename_len = 0;
        self.dirty        = false;
        self.sel_active   = false;
        self.undo_len     = 0;
        self.undo_head    = 0;
        self.redo_len     = 0;
        self.bar_mode     = BarMode::None;
    }

    fn save(&mut self) {
        if self.filename_len == 0 {
            let default = b"/note.txt";
            self.filename[..default.len()].copy_from_slice(default);
            self.filename_len = default.len();
        }
        let path_bytes = &self.filename[..self.filename_len];
        let path_str = match core::str::from_utf8(path_bytes) { Ok(s) => s, Err(_) => return };

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

        if is_disk_path(path_str) {
            if unsafe { save_to_disk(path_bytes, &buf[..pos]) } {
                self.dirty = false;
            }
            return;
        }

        unsafe {
            if let Some(fs) = RAMFS.get() {
                let _ = fs.create_file(path_str);
                if let Some(idx) = fs.resolve(path_str) {
                    fs.inodes[idx].data.clear();
                    fs.inodes[idx].data.extend_from_slice(&buf[..pos]);
                }
            }
        }
        self.dirty = false;
    }

    fn open_bar(&mut self) {
        self.bar_mode = BarMode::Open;
        self.bar_len  = 0;
        for b in &mut self.bar_text { *b = 0; }
    }

    fn open_file(&mut self) {
        if self.bar_len == 0 { self.bar_mode = BarMode::None; return; }
        let path_str = match core::str::from_utf8(&self.bar_text[..self.bar_len]) {
            Ok(s) => s,
            Err(_) => { self.bar_mode = BarMode::None; return; }
        };

        let data: Option<Vec<u8>> = if is_disk_path(path_str) {
            unsafe { load_from_disk(path_str.as_bytes()) }
        } else {
            unsafe { RAMFS.get() }
                .and_then(|fs| fs.resolve(path_str))
                .map(|idx| unsafe { RAMFS.get().unwrap().inodes[idx].data.clone() })
        };

        if let Some(data) = data {
            // Clear current buffer
            for i in 0..MAX_LINES { self.line_lens[i] = 0; }
            self.num_lines  = 1;
            self.cursor_row = 0;
            self.cursor_col = 0;
            self.scroll_top = 0;
            self.sel_active = false;
            self.undo_len   = 0;
            self.undo_head  = 0;
            self.redo_len   = 0;

            // Parse file data into lines
            let mut row = 0usize;
            let mut col = 0usize;
            for &byte in data.iter() {
                if row >= MAX_LINES { break; }
                if byte == b'\n' {
                    row += 1;
                    col  = 0;
                    if row < MAX_LINES { self.line_lens[row] = 0; }
                } else if col < MAX_LINE_LEN {
                    self.lines[row][col] = byte;
                    col += 1;
                    self.line_lens[row] = col;
                }
            }
            self.num_lines = (row + 1).max(1);

            // Store filename
            let flen = self.bar_len.min(63);
            self.filename[..flen].copy_from_slice(&self.bar_text[..flen]);
            self.filename_len = flen;
            self.dirty = false;
        }

        self.bar_mode = BarMode::None;
        self.bar_len  = 0;
    }

    // ── Find bar ───────────────────────────────────────────────────────────────

    fn find_bar(&mut self) {
        self.bar_mode = BarMode::Find;
        self.bar_len  = 0;
        for b in &mut self.bar_text { *b = 0; }
        self.find_has_match = false;
    }

    fn find_next_from_start(&mut self) {
        self.find_match_row = 0;
        self.find_match_col = 0;
        self.find_has_match = false;
        self.find_next();
    }

    fn find_next(&mut self) {
        if self.bar_len == 0 { self.find_has_match = false; return; }
        let qlen = self.bar_len;

        // Start searching after the current match position
        let (start_row, start_col) = if self.find_has_match {
            let nc = self.find_match_col + 1;
            let nr = self.find_match_row;
            if nc + qlen > self.line_lens[nr] { (nr + 1, 0) } else { (nr, nc) }
        } else {
            (0, 0)
        };

        // Two-pass: first from current position to end, then wrap from beginning
        for pass in 0..2usize {
            let (row_start, col_start_at_row) = if pass == 0 {
                (start_row, start_col)
            } else {
                (0, 0)
            };
            // On the second pass, don't re-search past the original start
            for row in row_start..self.num_lines {
                let line_len = self.line_lens[row];
                let col_from = if row == row_start && pass == 0 { col_start_at_row } else { 0 };
                if line_len >= qlen {
                    let limit = line_len - qlen;
                    for col in col_from..=limit {
                        if self.lines[row][col..col + qlen] == self.bar_text[..qlen] {
                            self.find_match_row = row;
                            self.find_match_col = col;
                            self.find_has_match = true;
                            self.cursor_row     = row;
                            self.cursor_col     = col;
                            self.ensure_visible(20);
                            return;
                        }
                    }
                }
            }
            // Only do a second pass if we didn't start from the beginning
            if pass == 0 && start_row == 0 && start_col == 0 { break; }
        }
        self.find_has_match = false;
    }

    // ── Undo / Redo ────────────────────────────────────────────────────────────

    fn undo_push(&mut self, op: UndoOp) {
        self.redo_len = 0; // new edit clears redo history
        self.undo_buf[self.undo_head] = op;
        self.undo_head = (self.undo_head + 1) % UNDO_CAP;
        self.undo_len  = (self.undo_len + 1).min(UNDO_CAP);
    }

    fn undo_push_raw(&mut self, op: UndoOp) {
        // Push without clearing redo (used internally during redo)
        self.undo_buf[self.undo_head] = op;
        self.undo_head = (self.undo_head + 1) % UNDO_CAP;
        self.undo_len  = (self.undo_len + 1).min(UNDO_CAP);
    }

    fn undo_pop(&mut self) -> Option<UndoOp> {
        if self.undo_len == 0 { return None; }
        self.undo_head = (self.undo_head + UNDO_CAP - 1) % UNDO_CAP;
        self.undo_len -= 1;
        Some(self.undo_buf[self.undo_head])
    }

    fn redo_push(&mut self, op: UndoOp) {
        if self.redo_len < UNDO_CAP {
            self.redo_buf[self.redo_len] = op;
            self.redo_len += 1;
        }
    }

    fn redo_pop(&mut self) -> Option<UndoOp> {
        if self.redo_len == 0 { return None; }
        self.redo_len -= 1;
        Some(self.redo_buf[self.redo_len])
    }

    /// Apply an undo op, returning the reverse op (which can be pushed to the other stack).
    fn apply_op(&mut self, op: UndoOp) -> UndoOp {
        match op {
            UndoOp::Char { row, col, ch, insert } => {
                let r = row as usize;
                let c = col as usize;
                if insert {
                    // Undo of insert: delete the char we inserted
                    self.raw_delete_char_at(r, c);
                    self.cursor_row = r;
                    self.cursor_col = c;
                    UndoOp::Char { row, col, ch, insert: false }
                } else {
                    // Undo of delete: re-insert the char
                    self.raw_insert_char_at(r, c, ch);
                    self.cursor_row = r;
                    self.cursor_col = c + 1;
                    UndoOp::Char { row, col, ch, insert: true }
                }
            }
            UndoOp::Newline { row, col, insert } => {
                let r = row as usize;
                let c = col as usize;
                if insert {
                    // Undo of insert-newline: join lines r and r+1
                    self.raw_join_lines(r);
                    self.cursor_row = r;
                    self.cursor_col = c;
                    UndoOp::Newline { row, col, insert: false }
                } else {
                    // Undo of delete-newline (backspace at col=0): re-split at (r, c)
                    self.raw_split_line(r, c);
                    self.cursor_row = r + 1;
                    self.cursor_col = 0;
                    UndoOp::Newline { row, col, insert: true }
                }
            }
        }
    }

    fn undo(&mut self) {
        let Some(op) = self.undo_pop() else { return; };
        let rev = self.apply_op(op);
        self.redo_push(rev);
        self.sel_active = false;
        self.dirty = true;
        self.ensure_visible(20);
    }

    fn redo(&mut self) {
        let Some(op) = self.redo_pop() else { return; };
        let rev = self.apply_op(op);
        self.undo_push_raw(rev);
        self.sel_active = false;
        self.dirty = true;
        self.ensure_visible(20);
    }

    // ── Raw editing primitives (no cursor/dirty side effects) ─────────────────

    fn raw_insert_char_at(&mut self, row: usize, col: usize, ch: u8) {
        if row >= MAX_LINES { return; }
        let len = self.line_lens[row];
        if len >= MAX_LINE_LEN { return; }
        let line = &mut self.lines[row];
        for i in (col..len).rev() { line[i + 1] = line[i]; }
        line[col] = ch;
        self.line_lens[row] += 1;
    }

    fn raw_delete_char_at(&mut self, row: usize, col: usize) {
        if row >= MAX_LINES { return; }
        let len = self.line_lens[row];
        if col >= len { return; }
        let line = &mut self.lines[row];
        for i in col..(len.saturating_sub(1)) { line[i] = line[i + 1]; }
        if len > 0 { line[len - 1] = 0; }
        self.line_lens[row] = len.saturating_sub(1);
    }

    /// Split line `row` at `col`: tail becomes new line `row+1`.
    fn raw_split_line(&mut self, row: usize, col: usize) {
        if self.num_lines >= MAX_LINES { return; }
        let old_len  = self.line_lens[row];
        let col      = col.min(old_len);
        let tail_len = old_len - col;
        // Shift lines down
        for i in (row + 1..self.num_lines).rev() {
            if i + 1 < MAX_LINES {
                self.lines[i + 1]     = self.lines[i];
                self.line_lens[i + 1] = self.line_lens[i];
            }
        }
        // Copy tail to new line, clear from current
        let mut new_line = [0u8; MAX_LINE_LEN];
        for i in 0..tail_len { new_line[i] = self.lines[row][col + i]; }
        for i in col..old_len { self.lines[row][i] = 0; }
        self.line_lens[row]     = col;
        self.lines[row + 1]     = new_line;
        self.line_lens[row + 1] = tail_len;
        self.num_lines += 1;
    }

    /// Join line `row+1` into end of line `row`.
    fn raw_join_lines(&mut self, row: usize) {
        if row + 1 >= self.num_lines { return; }
        let prev_len = self.line_lens[row];
        let cur_len  = self.line_lens[row + 1];
        let copy_len = cur_len.min(MAX_LINE_LEN.saturating_sub(prev_len));
        for i in 0..copy_len {
            self.lines[row][prev_len + i] = self.lines[row + 1][i];
        }
        self.line_lens[row] = prev_len + copy_len;
        // Shift remaining lines up
        for i in (row + 1)..(self.num_lines.saturating_sub(1)) {
            self.lines[i]     = self.lines[i + 1];
            self.line_lens[i] = self.line_lens[i + 1];
        }
        self.num_lines = self.num_lines.saturating_sub(1);
        if self.num_lines == 0 { self.num_lines = 1; }
    }

    // ── High-level editing (cursor + undo) ─────────────────────────────────────

    fn insert_char(&mut self, ch: u8) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        if row >= MAX_LINES || self.line_lens[row] >= MAX_LINE_LEN { return; }
        self.raw_insert_char_at(row, col, ch);
        self.cursor_col += 1;
        self.undo_push(UndoOp::Char { row: row as u16, col: col as u16, ch, insert: true });
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        if self.num_lines >= MAX_LINES { return; }
        self.raw_split_line(row, col);
        self.cursor_row += 1;
        self.cursor_col  = 0;
        self.undo_push(UndoOp::Newline { row: row as u16, col: col as u16, insert: true });
        self.ensure_visible(20);
        self.dirty = true;
    }

    fn backspace(&mut self) {
        let row = self.cursor_row;
        let col = self.cursor_col;
        if col > 0 {
            let old_ch = self.lines[row][col - 1];
            self.raw_delete_char_at(row, col - 1);
            self.cursor_col -= 1;
            self.undo_push(UndoOp::Char { row: row as u16, col: (col - 1) as u16, ch: old_ch, insert: false });
            self.dirty = true;
        } else if row > 0 {
            let prev_len = self.line_lens[row - 1];
            self.raw_join_lines(row - 1);
            self.cursor_row -= 1;
            self.cursor_col  = prev_len;
            self.undo_push(UndoOp::Newline { row: (row - 1) as u16, col: prev_len as u16, insert: false });
            self.ensure_visible(20);
            self.dirty = true;
        }
    }

    // ── Selection ──────────────────────────────────────────────────────────────

    fn start_sel(&mut self) {
        if !self.sel_active {
            self.sel_anchor_row = self.cursor_row;
            self.sel_anchor_col = self.cursor_col;
            self.sel_active     = true;
        }
    }

    fn select_all(&mut self) {
        self.sel_anchor_row = 0;
        self.sel_anchor_col = 0;
        self.sel_active     = true;
        if self.num_lines > 0 {
            self.cursor_row = self.num_lines - 1;
            self.cursor_col = self.line_lens[self.cursor_row];
            self.ensure_visible(20);
        }
    }

    /// Returns (start_row, start_col, end_row, end_col) in document order, or None.
    fn sel_range(&self) -> Option<(usize, usize, usize, usize)> {
        if !self.sel_active { return None; }
        let (ar, ac) = (self.sel_anchor_row, self.sel_anchor_col);
        let (cr, cc) = (self.cursor_row,     self.cursor_col);
        if ar < cr || (ar == cr && ac < cc) {
            Some((ar, ac, cr, cc))
        } else if cr < ar || (cr == ar && cc < ac) {
            Some((cr, cc, ar, ac))
        } else {
            None // zero-length selection
        }
    }

    fn copy_selection(&mut self) {
        let Some((sr, sc, er, ec)) = self.sel_range() else { return; };
        let mut pos = 0usize;
        unsafe {
            for row in sr..=er {
                if row >= self.num_lines { break; }
                let start = if row == sr { sc } else { 0 };
                let end   = if row == er { ec } else { self.line_lens[row] };
                for col in start..end.min(self.line_lens[row]) {
                    if pos < CLIPBOARD_MAX { CLIPBOARD[pos] = self.lines[row][col]; pos += 1; }
                }
                if row < er && pos < CLIPBOARD_MAX { CLIPBOARD[pos] = b'\n'; pos += 1; }
            }
            CLIPBOARD_LEN = pos;
        }
    }

    fn delete_selection(&mut self) {
        let Some((sr, sc, er, ec)) = self.sel_range() else {
            self.sel_active = false;
            return;
        };
        self.cursor_row = sr;
        self.cursor_col = sc;
        self.sel_active = false;
        // Clear redo since this is a new (batch) edit
        self.redo_len = 0;

        if sr == er {
            // Single-line selection
            let len = self.line_lens[sr];
            let end = ec.min(len);
            let del = end.saturating_sub(sc);
            for i in sc..(len - del) { self.lines[sr][i] = self.lines[sr][i + del]; }
            for i in (len - del)..len { self.lines[sr][i] = 0; }
            self.line_lens[sr] = len - del;
        } else {
            // Multi-line: merge start of sr with tail of er
            let tail_start = ec.min(self.line_lens[er]);
            let tail_len   = self.line_lens[er].saturating_sub(tail_start);
            let avail      = MAX_LINE_LEN.saturating_sub(sc);
            let copy       = tail_len.min(avail);
            for i in 0..copy { self.lines[sr][sc + i] = self.lines[er][tail_start + i]; }
            self.line_lens[sr] = sc + copy;
            // Remove lines sr+1 through er
            let remove = er - sr;
            for i in (sr + 1)..(self.num_lines.saturating_sub(remove)) {
                self.lines[i]     = self.lines[i + remove];
                self.line_lens[i] = self.line_lens[i + remove];
            }
            self.num_lines = self.num_lines.saturating_sub(remove);
            if self.num_lines == 0 { self.num_lines = 1; }
        }
        self.dirty = true;
        self.ensure_visible(20);
    }

    fn paste_clipboard(&mut self) {
        if self.sel_active { self.delete_selection(); }
        self.redo_len = 0; // new edit
        unsafe {
            for &byte in CLIPBOARD[..CLIPBOARD_LEN].iter() {
                if byte == b'\n' {
                    if self.num_lines < MAX_LINES {
                        let r = self.cursor_row; let c = self.cursor_col;
                        self.raw_split_line(r, c);
                        self.cursor_row += 1;
                        self.cursor_col  = 0;
                    }
                } else if byte >= 0x20 || byte == b'\t' {
                    let r = self.cursor_row;
                    if self.line_lens[r] < MAX_LINE_LEN {
                        let c = self.cursor_col;
                        self.raw_insert_char_at(r, c, byte);
                        self.cursor_col += 1;
                    }
                }
            }
        }
        self.dirty = true;
        self.ensure_visible(20);
    }

    // ── Cursor movement ────────────────────────────────────────────────────────

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

    // ── Drawing ────────────────────────────────────────────────────────────────

    pub fn draw(&mut self, graphics: &Graphics, wm: &WindowManager) {
        if !wm.is_window_visible(self.window_id) { return; }
        let Some(win) = wm.get_window(self.window_id) else { return; };
        let is_focused = wm.get_focused() == Some(self.window_id);

        let cx = win.x + 1;
        let cy = win.y + 31;
        let cw = win.width.saturating_sub(2);
        let ch = win.height.saturating_sub(32);

        // ── Menu bar ───────────────────────────────────────────────────────────
        if cx != self.last_bar_x || cy != self.last_bar_y || cw != self.last_bar_w {
            self.last_bar_x = cx;
            self.last_bar_y = cy;
            self.last_bar_w = cw;
            self.menu.layout(cx);
        }
        self.menu.draw(graphics, cx, cy, cw);

        // ── Chrome heights ─────────────────────────────────────────────────────
        let find_bar_h  = if self.bar_mode != BarMode::None { FIND_BAR_H } else { 0 };
        let status_h    = if self.show_status_bar { STATUS_H } else { 0 };
        let bottom_chrome = status_h + find_bar_h;
        let text_top    = cy + MENUBAR_H;
        let text_h      = ch.saturating_sub(MENUBAR_H + bottom_chrome);

        // ── Text area background + gutter ──────────────────────────────────────
        graphics.fill_rect(cx, text_top, cw, text_h, BG);
        graphics.fill_rect(cx, text_top, GUTTER_W, text_h, GUTTER_BG);
        graphics.fill_rect(cx + GUTTER_W, text_top, 1, text_h, GUTTER_LINE);

        let visible_rows = ((text_h / LINE_H) as usize).max(1);
        let text_x       = cx + GUTTER_W + PAD_X;
        let usable_w     = cw.saturating_sub(GUTTER_W + PAD_X + 4);
        let max_cols     = (usable_w / CHAR_W) as usize;

        // Compute scroll_top (clamp to keep cursor visible)
        let scroll_top = {
            let mut st = self.scroll_top;
            if self.cursor_row < st {
                st = self.cursor_row;
            } else if self.cursor_row >= st + visible_rows {
                st = self.cursor_row + 1 - visible_rows;
            }
            st
        };

        let sel_range = self.sel_range();

        if self.word_wrap && max_cols > 0 {
            self.draw_wrapped(graphics, cx, text_top, cw, text_x,
                              visible_rows, max_cols, scroll_top,
                              is_focused, sel_range);
        } else {
            self.draw_nowrap(graphics, cx, text_top, cw, text_x,
                             visible_rows, max_cols, scroll_top,
                             is_focused, sel_range);
        }

        // ── Find / Open bar ────────────────────────────────────────────────────
        if self.bar_mode != BarMode::None {
            let bar_y = cy + ch.saturating_sub(bottom_chrome);
            graphics.fill_rect(cx, bar_y, cw, FIND_BAR_H, FIND_BAR_BG);
            graphics.fill_rect(cx, bar_y, cw, 1, FIND_BAR_BORDER);

            let label = match self.bar_mode { BarMode::Find => "Find: ", BarMode::Open => "Open: ", _ => "" };
            let label_w = label.len() as u64 * CHAR_W;
            fonts::draw_string(graphics, cx + 6, bar_y + 9, label, 0xFFAAAAAA);

            let input_x = cx + 6 + label_w;
            let input_w = cw.saturating_sub(label_w + 16);
            graphics.fill_rect(input_x - 2, bar_y + 4, input_w, FIND_BAR_H - 8, FIND_INPUT_BG);

            if let Ok(s) = core::str::from_utf8(&self.bar_text[..self.bar_len]) {
                fonts::draw_string(graphics, input_x, bar_y + 9, s, 0xFFE0E0E0);
            }

            // Bar cursor
            let bcx = input_x + self.bar_len as u64 * CHAR_W;
            graphics.fill_rect(bcx, bar_y + 6, 2, FIND_BAR_H - 10, CURSOR_COL);

            if self.bar_mode == BarMode::Find && self.bar_len > 0 && !self.find_has_match {
                fonts::draw_string(graphics, bcx + 8, bar_y + 9, "Not found", 0xFFAA4444);
            }
        }

        // ── Status bar ─────────────────────────────────────────────────────────
        if self.show_status_bar {
            let sy = cy + ch.saturating_sub(STATUS_H);
            graphics.fill_rect(cx, sy, cw, STATUS_H, STATUS_BG);

            let sel_info = if self.sel_active {
                if let Some((sr, sc, er, ec)) = sel_range {
                    let chars: usize = if sr == er {
                        ec.saturating_sub(sc)
                    } else {
                        let mut n = self.line_lens[sr].saturating_sub(sc) + 1;
                        for r in (sr + 1)..er { n += self.line_lens[r] + 1; }
                        n += ec;
                        n
                    };
                    format!("  Sel {} chars  |", chars)
                } else { String::new() }
            } else { String::new() };

            let status = format!(
                "  Ln {}, Col {}{}  {} lines  |  {}  ",
                self.cursor_row + 1,
                self.cursor_col + 1,
                sel_info,
                self.num_lines,
                if self.dirty { "Modified" } else { "Saved" },
            );
            let s = if status.len() > 80 { &status[..80] } else { status.as_str() };
            fonts::draw_string(graphics, cx + 4, sy + 3, s, STATUS_FG);

            let wrap_hint = if self.word_wrap { "Wrap" } else { "" };
            if !wrap_hint.is_empty() {
                let hx = cx + cw.saturating_sub(wrap_hint.len() as u64 * CHAR_W + 6);
                fonts::draw_string(graphics, hx, sy + 3, wrap_hint, 0xFFCCDDFF);
            }
        }

        // ── About dialog ───────────────────────────────────────────────────────
        if self.show_about {
            self.draw_about(graphics, cx, cy, cw, ch);
        }
    }

    fn draw_nowrap(
        &self,
        graphics: &Graphics,
        cx: u64, text_top: u64, cw: u64, text_x: u64,
        visible_rows: usize, max_cols: usize, scroll_top: usize,
        is_focused: bool,
        sel_range: Option<(usize, usize, usize, usize)>,
    ) {
        for i in 0..visible_rows {
            let row = scroll_top + i;
            if row >= self.num_lines { break; }
            let y = text_top + i as u64 * LINE_H;
            let is_cur = row == self.cursor_row;

            // Current-line highlight
            if is_cur && is_focused {
                graphics.fill_rect(cx + GUTTER_W + 1, y, cw.saturating_sub(GUTTER_W + 1), LINE_H, CUR_LINE_BG);
            }

            // Gutter line number
            let gnum_col = if is_cur { GUTTER_CUR } else { GUTTER_FG };
            draw_linenum(graphics, cx + 2, y + 1, row + 1, gnum_col);

            // Selection highlight
            if let Some((sr, sc, er, ec)) = sel_range {
                if row >= sr && row <= er {
                    let hstart = if row == sr { sc } else { 0 };
                    let hend   = if row == er { ec } else { self.line_lens[row] };
                    if hstart < hend {
                        let hx = text_x + hstart as u64 * CHAR_W;
                        let hw = (hend - hstart) as u64 * CHAR_W;
                        graphics.fill_rect(hx, y + 1, hw.max(2), LINE_H - 2, SEL_BG);
                    }
                    // Extend to end-of-line marker for non-last rows
                    if row < er {
                        let eol_x = text_x + hend as u64 * CHAR_W;
                        graphics.fill_rect(eol_x, y + 1, CHAR_W, LINE_H - 2, SEL_BG);
                    }
                }
            }

            // Find match highlight
            if self.bar_mode == BarMode::Find && self.find_has_match && row == self.find_match_row {
                let mx = text_x + self.find_match_col as u64 * CHAR_W;
                let mw = (self.bar_len as u64 * CHAR_W).max(2);
                graphics.fill_rect(mx, y + 1, mw, LINE_H - 2, FIND_MATCH_BG);
            }

            // Text content
            let len = self.line_lens[row].min(max_cols);
            if let Ok(s) = core::str::from_utf8(&self.lines[row][..len]) {
                fonts::draw_string(graphics, text_x, y + 1, s, TEXT_COL);
            }

            // Cursor bar
            if is_cur && is_focused {
                let col_c = self.cursor_col.min(max_cols);
                let cx_c  = text_x + col_c as u64 * CHAR_W;
                graphics.fill_rect(cx_c, y + 1, 2, LINE_H - 2, CURSOR_COL);
            }
        }
    }

    fn draw_wrapped(
        &self,
        graphics: &Graphics,
        cx: u64, text_top: u64, cw: u64, text_x: u64,
        visible_rows: usize, max_cols: usize, scroll_top: usize,
        is_focused: bool,
        sel_range: Option<(usize, usize, usize, usize)>,
    ) {
        let mut visual = 0usize;

        'outer: for logical_row in scroll_top..self.num_lines {
            let line_len   = self.line_lens[logical_row];
            let num_chunks = if line_len == 0 || max_cols == 0 { 1 }
                             else { (line_len + max_cols - 1) / max_cols };
            let is_cur_row = logical_row == self.cursor_row;

            for chunk in 0..num_chunks {
                if visual >= visible_rows { break 'outer; }
                let y           = text_top + visual as u64 * LINE_H;
                let chunk_start = chunk * max_cols;
                let chunk_end   = (chunk_start + max_cols).min(line_len);

                // Current-line highlight (entire logical row)
                if is_cur_row && is_focused {
                    graphics.fill_rect(cx + GUTTER_W + 1, y, cw.saturating_sub(GUTTER_W + 1), LINE_H, CUR_LINE_BG);
                }

                // Gutter
                let gnum_col = if is_cur_row { GUTTER_CUR } else { GUTTER_FG };
                if chunk == 0 {
                    draw_linenum(graphics, cx + 2, y + 1, logical_row + 1, gnum_col);
                } else {
                    // Continuation marker
                    fonts::draw_string(graphics, cx + GUTTER_W.saturating_sub(12), y + 1, ">>", GUTTER_FG);
                }

                // Selection highlight for this chunk
                if let Some((sr, sc, er, ec)) = sel_range {
                    if logical_row >= sr && logical_row <= er {
                        let line_sel_start = if logical_row == sr { sc } else { 0 };
                        let line_sel_end   = if logical_row == er { ec } else { line_len };
                        let cstart = line_sel_start.max(chunk_start).min(chunk_end);
                        let cend   = line_sel_end.min(chunk_end);
                        if cstart < cend {
                            let hx = text_x + (cstart - chunk_start) as u64 * CHAR_W;
                            let hw = (cend - cstart) as u64 * CHAR_W;
                            graphics.fill_rect(hx, y + 1, hw.max(2), LINE_H - 2, SEL_BG);
                        }
                        // EOL marker on non-last selected rows
                        if logical_row < er && line_sel_end >= chunk_end {
                            let ex = text_x + (chunk_end - chunk_start) as u64 * CHAR_W;
                            graphics.fill_rect(ex, y + 1, CHAR_W, LINE_H - 2, SEL_BG);
                        }
                    }
                }

                // Find-match highlight
                if self.bar_mode == BarMode::Find && self.find_has_match && logical_row == self.find_match_row {
                    let mc = self.find_match_col;
                    let me = mc + self.bar_len;
                    if mc < chunk_end && me > chunk_start {
                        let cstart = mc.max(chunk_start);
                        let cend   = me.min(chunk_end);
                        let fx = text_x + (cstart - chunk_start) as u64 * CHAR_W;
                        let fw = (cend - cstart) as u64 * CHAR_W;
                        graphics.fill_rect(fx, y + 1, fw.max(2), LINE_H - 2, FIND_MATCH_BG);
                    }
                }

                // Text
                if chunk_start < line_len {
                    if let Ok(s) = core::str::from_utf8(&self.lines[logical_row][chunk_start..chunk_end]) {
                        fonts::draw_string(graphics, text_x, y + 1, s, TEXT_COL);
                    }
                }

                // Cursor
                if is_cur_row && is_focused {
                    let cursor_chunk = if max_cols > 0 { self.cursor_col / max_cols } else { 0 };
                    if cursor_chunk == chunk {
                        let vcol  = if max_cols > 0 { self.cursor_col % max_cols } else { self.cursor_col };
                        let cx_c  = text_x + vcol as u64 * CHAR_W;
                        graphics.fill_rect(cx_c, y + 1, 2, LINE_H - 2, CURSOR_COL);
                    }
                }

                visual += 1;
            }
        }
    }

    fn draw_about(&self, graphics: &Graphics, cx: u64, cy: u64, cw: u64, ch: u64) {
        let ax = cx + (cw.saturating_sub(ABOUT_W)) / 2;
        let ay = cy + (ch.saturating_sub(ABOUT_H)) / 2;

        graphics.fill_rect(ax + 4, ay + 4, ABOUT_W, ABOUT_H, 0x88000000);
        graphics.fill_rect(ax, ay, ABOUT_W, ABOUT_H, ABOUT_BG);
        graphics.draw_rect(ax, ay, ABOUT_W, ABOUT_H, ABOUT_BORDER, 1);

        graphics.fill_rect(ax, ay, ABOUT_W, 24, 0xFF007ACC);
        fonts::draw_string(graphics, ax + 8, ay + 7, "About Notepad", 0xFFFFFFFF);

        let line_y = |n: u64| ay + 32 + n * 16;
        fonts::draw_string(graphics, ax + 16, line_y(0), "OxideOS Notepad", 0xFFE1E1E1);
        fonts::draw_string(graphics, ax + 16, line_y(1), "Undo/Redo  Ctrl+Z / Ctrl+Y", 0xFF888888);
        fonts::draw_string(graphics, ax + 16, line_y(2), "Selection  Shift+Arrows", 0xFF888888);
        fonts::draw_string(graphics, ax + 16, line_y(3), "Find       Ctrl+F", 0xFF888888);
        fonts::draw_string(graphics, ax + 16, line_y(4), "Click Help > About to close", 0xFF555555);
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn draw_linenum(graphics: &Graphics, x: u64, y: u64, num: usize, color: u32) {
    let mut digits = [0u8; 4];
    let mut n      = num;
    let mut count  = 0usize;
    while n > 0 && count < 4 { digits[count] = (n % 10) as u8; n /= 10; count += 1; }
    if count == 0 { digits[0] = 0; count = 1; }
    let right_edge_x = x + GUTTER_W - 6;
    let start_x      = right_edge_x.saturating_sub((count as u64).saturating_sub(1) * CHAR_W);
    for i in (0..count).rev() {
        let cx = start_x + (count - 1 - i) as u64 * CHAR_W;
        fonts::draw_char(graphics, cx, y, (b'0' + digits[i]) as char, color);
    }
}
