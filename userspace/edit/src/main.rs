//! OxideOS Text Editor — edit [filename]
//!
//! Ported to the gui_create/gui_poll_event window API so the menu bar
//! receives mouse click events alongside keyboard input.
//!
//! Keys (normal mode):
//!   Arrows / Home / End / PgUp / PgDn — navigation
//!   Enter / Backspace / Del / Tab      — editing
//!   Ctrl+N  new file      Ctrl+S  save      Ctrl+W  save-as
//!   Ctrl+A  select all    Ctrl+Q  quit       Ctrl+X  force quit
//!
//! Menu bar — click or press Alt+F / Alt+E / Alt+V / Alt+H to open.
//!   Arrow Up/Down  navigate items   Arrow Left/Right  switch menus
//!   Enter          run item         Escape            close menu
#![no_std]
#![no_main]

use oxide_rt::{
    exit, sleep_ms, get_time,
    gui_create, gui_fill_rect, gui_draw_text, gui_present, gui_poll_event,
    GuiWindow,
    open, close, write, read,
};

// ── Layout ────────────────────────────────────────────────────────────────────

const WIN_W:  u32 = 580;
const WIN_H:  u32 = 400;
const CHAR_W: u32 = 9;
const LINE_H: u32 = 16;

const TITLE_H:    u32 = 18;
const MENUBAR_H:  u32 = 18;
const STATUS_H:   u32 = 18;
const CONTENT_TOP: u32 = TITLE_H + MENUBAR_H;

const TEXT_X: u32 = 4;
const TEXT_Y: u32 = CONTENT_TOP + 2;
const TEXT_W: u32 = WIN_W - TEXT_X - 4;
const TEXT_H: u32 = WIN_H - CONTENT_TOP - STATUS_H - 4;

const VISIBLE_ROWS: usize = (TEXT_H / LINE_H) as usize;

const GUTTER_W:  u32 = 5 * CHAR_W;
const EDIT_X:    u32 = TEXT_X + GUTTER_W;
const EDIT_COLS: usize = ((TEXT_W - GUTTER_W) / CHAR_W) as usize;

// ── Buffer limits ─────────────────────────────────────────────────────────────

const MAX_LINES:    usize = 500;
const MAX_LINE_LEN: usize = 256;

// ── Colour palette ─────────────────────────────────────────────────────────────

const COL_BG:           u32 = 0xFF1E1E2E;
const COL_TITLE_BG:     u32 = 0xFF181825;
const COL_MENUBAR_BG:   u32 = 0xFF242434;
const COL_MENU_HOT:     u32 = 0xFF313244;
const COL_STATUS_BG:    u32 = 0xFF181825;
const COL_TEXT:         u32 = 0xFFCDD6F4;
const COL_DIM:          u32 = 0xFF585B70;
const COL_ACCENT:       u32 = 0xFF89B4FA;
const COL_CURSOR_BG:    u32 = 0xFF89B4FA;
const COL_CURSOR_FG:    u32 = 0xFF1E1E2E;
const COL_LINE_NUM:     u32 = 0xFF45475A;
const COL_MODIFIED:     u32 = 0xFFF38BA8;
const COL_SAVED:        u32 = 0xFFA6E3A1;
const COL_HINT:         u32 = 0xFF585B70;
const COL_SELECTION:    u32 = 0xFF313244;
const COL_WARN:         u32 = 0xFFFAB387;
const COL_DROP_BG:      u32 = 0xFF1E1E2E;
const COL_DROP_HOT:     u32 = 0xFF313244;
const COL_DROP_BORDER:  u32 = 0xFF45475A;
const COL_SAVE_PROMPT:  u32 = 0xFF313244;

// ── Menu definitions ──────────────────────────────────────────────────────────

const MENU_COUNT: usize = 4;
const MENU_LABELS: [&str; MENU_COUNT] = ["File", "Edit", "View", "Help"];
const MENU_TAB_W: u32 = 54; // each tab is the same width

// Each menu's items.  "|" = separator.
const FILE_ITEMS:  [&str; 5] = ["New        Ctrl+N", "Save       Ctrl+S", "Save As    Ctrl+W", "|", "Quit       Ctrl+Q"];
const EDIT_ITEMS:  [&str; 3] = ["Select All Ctrl+A", "|", "Word Wrap"];
const VIEW_ITEMS:  [&str; 2] = ["Status Bar", "Line Numbers"];
const HELP_ITEMS:  [&str; 1] = ["About edit"];
const MENU_COUNTS: [usize; MENU_COUNT] = [5, 3, 2, 1];

fn item_label(menu: usize, item: usize) -> &'static str {
    match menu {
        0 => FILE_ITEMS.get(item).copied().unwrap_or(""),
        1 => EDIT_ITEMS.get(item).copied().unwrap_or(""),
        2 => VIEW_ITEMS.get(item).copied().unwrap_or(""),
        3 => HELP_ITEMS.get(item).copied().unwrap_or(""),
        _ => "",
    }
}

const DROP_W:      u32 = 200;
const DROP_ITEM_H: u32 = LINE_H + 2;
const DROP_SEP_H:  u32 = 8;

fn dropdown_height(menu: usize) -> u32 {
    let mut h = 4u32;
    for i in 0..MENU_COUNTS[menu] {
        h += if item_label(menu, i) == "|" { DROP_SEP_H } else { DROP_ITEM_H };
    }
    h + 4
}

fn tab_x(idx: usize) -> u32 { 4 + idx as u32 * MENU_TAB_W }

/// Given a y within the dropdown (0 = top of dropdown), return the item index.
fn item_at_y(menu: usize, rel_y: u32) -> Option<usize> {
    let mut cur = 2u32;
    for i in 0..MENU_COUNTS[menu] {
        let h = if item_label(menu, i) == "|" { DROP_SEP_H } else { DROP_ITEM_H };
        if item_label(menu, i) != "|" && rel_y >= cur && rel_y < cur + h {
            return Some(i);
        }
        cur += h;
    }
    None
}

// ── Line ──────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Line { buf: [u8; MAX_LINE_LEN], len: usize }
impl Line {
    const fn new() -> Self { Self { buf: [0; MAX_LINE_LEN], len: 0 } }
    fn as_str(&self) -> &str { core::str::from_utf8(&self.buf[..self.len]).unwrap_or("") }
    fn insert(&mut self, pos: usize, b: u8) {
        if self.len >= MAX_LINE_LEN || pos > self.len { return; }
        let mut i = self.len;
        while i > pos { self.buf[i] = self.buf[i-1]; i -= 1; }
        self.buf[pos] = b; self.len += 1;
    }
    fn remove(&mut self, pos: usize) -> u8 {
        if pos >= self.len { return 0; }
        let b = self.buf[pos];
        let mut i = pos;
        while i + 1 < self.len { self.buf[i] = self.buf[i+1]; i += 1; }
        self.len -= 1; b
    }
    fn append(&mut self, o: &Line) {
        let n = o.len.min(MAX_LINE_LEN - self.len);
        self.buf[self.len..self.len+n].copy_from_slice(&o.buf[..n]);
        self.len += n;
    }
    fn split_at(&mut self, col: usize) -> Line {
        let mut nl = Line::new();
        let n = self.len.saturating_sub(col);
        nl.buf[..n].copy_from_slice(&self.buf[col..col+n]);
        nl.len = n; self.len = col.min(self.len); nl
    }
}

// ── Doc ───────────────────────────────────────────────────────────────────────

struct Doc {
    lines:    [Line; MAX_LINES],
    count:    usize,
    cur_row:  usize,
    cur_col:  usize,
    scroll:   usize,
    modified: bool,
    filename: [u8; 128],
    fname_len: usize,
    msg: [u8; 80], msg_len: usize, msg_ticks: u64, msg_ok: bool,
    // menu state
    menu_open: Option<usize>,
    menu_item: usize,
    // save-as prompt
    saveas_mode: bool,
    saveas_buf: [u8; 128],
    saveas_len: usize,
    // view flags
    show_linenum: bool,
    show_status:  bool,
    word_wrap:    bool,
    show_about:   bool,
}

impl Doc {
    fn new() -> Self {
        Self {
            lines: [Line::new(); MAX_LINES],
            count: 1, cur_row: 0, cur_col: 0, scroll: 0,
            modified: false,
            filename: [0; 128], fname_len: 0,
            msg: [0; 80], msg_len: 0, msg_ticks: 0, msg_ok: true,
            menu_open: None, menu_item: 0,
            saveas_mode: false, saveas_buf: [0; 128], saveas_len: 0,
            show_linenum: true, show_status: true, word_wrap: false,
            show_about: false,
        }
    }

    fn fname_str(&self) -> &str {
        core::str::from_utf8(&self.filename[..self.fname_len]).unwrap_or("[no name]")
    }

    fn set_msg(&mut self, s: &str, ok: bool) {
        let n = s.len().min(79);
        self.msg[..n].copy_from_slice(&s.as_bytes()[..n]);
        self.msg_len = n; self.msg_ticks = get_time(); self.msg_ok = ok;
    }

    fn msg_str(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.msg_len]).unwrap_or("")
    }

    fn clamp_col(&mut self) { let m = self.lines[self.cur_row].len; if self.cur_col > m { self.cur_col = m; } }

    fn ensure_visible(&mut self) {
        if self.cur_row < self.scroll { self.scroll = self.cur_row; }
        else if self.cur_row >= self.scroll + VISIBLE_ROWS { self.scroll = self.cur_row + 1 - VISIBLE_ROWS; }
    }

    fn move_up(&mut self)    { if self.cur_row > 0 { self.cur_row -= 1; self.clamp_col(); self.ensure_visible(); } }
    fn move_down(&mut self)  { if self.cur_row + 1 < self.count { self.cur_row += 1; self.clamp_col(); self.ensure_visible(); } }
    fn move_left(&mut self)  { if self.cur_col > 0 { self.cur_col -= 1; } else if self.cur_row > 0 { self.cur_row -= 1; self.cur_col = self.lines[self.cur_row].len; self.ensure_visible(); } }
    fn move_right(&mut self) { let l = self.lines[self.cur_row].len; if self.cur_col < l { self.cur_col += 1; } else if self.cur_row + 1 < self.count { self.cur_row += 1; self.cur_col = 0; self.ensure_visible(); } }
    fn home(&mut self) { self.cur_col = 0; }
    fn end(&mut self)  { self.cur_col = self.lines[self.cur_row].len; }
    fn select_all(&mut self) { self.cur_row = self.count.saturating_sub(1); self.cur_col = self.lines[self.cur_row].len; self.ensure_visible(); }
    fn page_up(&mut self)   { let s = VISIBLE_ROWS.saturating_sub(1).max(1); self.cur_row = self.cur_row.saturating_sub(s); self.scroll = self.scroll.saturating_sub(s); self.clamp_col(); self.ensure_visible(); }
    fn page_down(&mut self) { let s = VISIBLE_ROWS.saturating_sub(1).max(1); self.cur_row = (self.cur_row + s).min(self.count.saturating_sub(1)); self.clamp_col(); self.ensure_visible(); }

    fn insert_char(&mut self, b: u8) { self.lines[self.cur_row].insert(self.cur_col, b); self.cur_col += 1; self.modified = true; }

    fn insert_newline(&mut self) {
        if self.count >= MAX_LINES { return; }
        let nl = self.lines[self.cur_row].split_at(self.cur_col);
        let mut i = self.count;
        while i > self.cur_row + 1 { self.lines[i] = self.lines[i-1]; i -= 1; }
        self.lines[self.cur_row + 1] = nl; self.count += 1;
        self.cur_row += 1; self.cur_col = 0; self.modified = true; self.ensure_visible();
    }

    fn backspace(&mut self) {
        if self.cur_col > 0 { self.cur_col -= 1; self.lines[self.cur_row].remove(self.cur_col); self.modified = true; }
        else if self.cur_row > 0 {
            let pl = self.lines[self.cur_row - 1].len;
            let cl = self.lines[self.cur_row];
            self.lines[self.cur_row - 1].append(&cl);
            let mut i = self.cur_row;
            while i + 1 < self.count { self.lines[i] = self.lines[i+1]; i += 1; }
            self.count -= 1; self.cur_row -= 1; self.cur_col = pl; self.modified = true; self.ensure_visible();
        }
    }

    fn delete_char(&mut self) {
        let l = self.lines[self.cur_row].len;
        if self.cur_col < l { self.lines[self.cur_row].remove(self.cur_col); self.modified = true; }
        else if self.cur_row + 1 < self.count {
            let nx = self.lines[self.cur_row + 1];
            self.lines[self.cur_row].append(&nx);
            let mut i = self.cur_row + 1;
            while i + 1 < self.count { self.lines[i] = self.lines[i+1]; i += 1; }
            self.count -= 1; self.modified = true;
        }
    }

    fn new_file(&mut self) {
        for i in 0..self.count { self.lines[i] = Line::new(); }
        self.count = 1; self.cur_row = 0; self.cur_col = 0; self.scroll = 0;
        self.fname_len = 0; self.modified = false; self.set_msg("New file", true);
    }

    fn load_file(&mut self) -> bool {
        if self.fname_len == 0 { return false; }
        let fd = open(self.fname_str(), 1);
        if fd < 0 { return false; }
        self.count = 1; self.lines[0] = Line::new();
        self.cur_row = 0; self.cur_col = 0; self.scroll = 0;
        let mut row = 0usize;
        let mut buf = [0u8; 512];
        loop {
            let n = read(fd, &mut buf);
            if n <= 0 { break; }
            for i in 0..n as usize {
                let b = buf[i];
                if b == b'\n' { if row + 1 < MAX_LINES { row += 1; self.lines[row] = Line::new(); self.count = row + 1; } }
                else if b != b'\r' { self.lines[row].insert(self.lines[row].len, b); }
            }
        }
        close(fd); self.modified = false; true
    }

    fn save_to(&mut self, path: &str) -> bool {
        let fd = open(path, 0x241);
        if fd < 0 { self.set_msg("Save failed: cannot create file", false); return false; }
        for r in 0..self.count {
            let l = &self.lines[r];
            if l.len > 0 { write(fd, &l.buf[..l.len]); }
            write(fd, b"\n");
        }
        close(fd); self.modified = false; true
    }

    fn save_file(&mut self) {
        if self.fname_len == 0 { self.begin_saveas(); return; }
        let pb = self.filename; let pl = self.fname_len;
        let path = core::str::from_utf8(&pb[..pl]).unwrap_or("");
        if self.save_to(path) { self.set_msg("Saved.", true); }
    }

    fn begin_saveas(&mut self) {
        self.saveas_mode = true;
        self.saveas_len = self.fname_len.min(127);
        self.saveas_buf[..self.saveas_len].copy_from_slice(&self.filename[..self.saveas_len]);
        self.set_msg("Save as — type path then Enter, Esc to cancel", true);
    }

    fn commit_saveas(&mut self) {
        if self.saveas_len == 0 { self.saveas_mode = false; self.set_msg("Cancelled", false); return; }
        let pb = self.saveas_buf; let pl = self.saveas_len;
        let path = match core::str::from_utf8(&pb[..pl]) { Ok(s) => s, Err(_) => { self.saveas_mode = false; return; } };
        self.filename[..pl].copy_from_slice(&pb[..pl]); self.fname_len = pl;
        if self.save_to(path) { self.set_msg("Saved.", true); }
        self.saveas_mode = false;
    }

    // ── Menu helpers ──────────────────────────────────────────────────────────

    fn open_menu(&mut self, idx: usize) {
        self.menu_open = Some(idx); self.menu_item = self.first_sel(idx);
    }
    fn close_menu(&mut self) { self.menu_open = None; self.menu_item = 0; }

    fn first_sel(&self, m: usize) -> usize {
        for i in 0..MENU_COUNTS[m] { if item_label(m, i) != "|" { return i; } }
        0
    }
    fn menu_next(&mut self) {
        let m = match self.menu_open { Some(m) => m, None => return };
        let c = MENU_COUNTS[m]; let mut n = (self.menu_item + 1) % c;
        for _ in 0..c { if item_label(m, n) != "|" { break; } n = (n + 1) % c; }
        self.menu_item = n;
    }
    fn menu_prev(&mut self) {
        let m = match self.menu_open { Some(m) => m, None => return };
        let c = MENU_COUNTS[m]; let mut p = if self.menu_item == 0 { c - 1 } else { self.menu_item - 1 };
        for _ in 0..c { if item_label(m, p) != "|" { break; } p = if p == 0 { c - 1 } else { p - 1 }; }
        self.menu_item = p;
    }
    fn menu_left(&mut self)  { let m = match self.menu_open { Some(m) => m, None => return }; let n = if m == 0 { MENU_COUNT - 1 } else { m - 1 }; self.open_menu(n); }
    fn menu_right(&mut self) { let m = match self.menu_open { Some(m) => m, None => return }; self.open_menu((m + 1) % MENU_COUNT); }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

fn w(win: GuiWindow, x: u32, y: u32, ww: u32, h: u32, col: u32) {
    gui_fill_rect(win, x, y, ww, h, col);
}
fn t(win: GuiWindow, x: u32, y: u32, col: u32, s: &str) {
    gui_draw_text(win, x, y, col, s);
}

fn fmt_u32(buf: &mut [u8; 8], v: u32) -> &str {
    let mut i = buf.len(); let mut vv = v;
    if vv == 0 { i -= 1; buf[i] = b'0'; }
    while vv > 0 { i -= 1; buf[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

fn draw_title(win: GuiWindow, doc: &Doc) {
    w(win, 0, 0, WIN_W, TITLE_H, COL_TITLE_BG);
    w(win, 0, TITLE_H - 1, WIN_W, 1, 0xFF313244);
    t(win, 4, 3, COL_ACCENT, "edit");
    let fname = doc.fname_str();
    let fx = 4 + 5 * CHAR_W;
    t(win, fx, 3, COL_TEXT, fname);
    let mx = fx + fname.len() as u32 * CHAR_W + 4;
    if doc.modified        { t(win, mx, 3, COL_MODIFIED, "[+]"); }
    else if doc.fname_len > 0 { t(win, mx, 3, COL_SAVED, "[saved]"); }
    // row:col
    let mut rb = [0u8; 32]; let mut rl = 0usize;
    fn pn(buf: &mut [u8; 32], p: &mut usize, v: usize) {
        let mut tmp = [0u8; 8]; let mut i = 8usize; let mut vv = v;
        if vv == 0 { i -= 1; tmp[i] = b'0'; }
        while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
        for &b in &tmp[i..] { if *p < buf.len() { buf[*p] = b; *p += 1; } }
    }
    pn(&mut rb, &mut rl, doc.cur_row + 1);
    if rl < rb.len() { rb[rl] = b':'; rl += 1; }
    pn(&mut rb, &mut rl, doc.cur_col + 1);
    let rstr = core::str::from_utf8(&rb[..rl]).unwrap_or("");
    let rx = WIN_W.saturating_sub(rstr.len() as u32 * CHAR_W + 4);
    t(win, rx, 3, COL_DIM, rstr);
}

fn draw_menubar(win: GuiWindow, doc: &Doc) {
    let y = TITLE_H;
    w(win, 0, y, WIN_W, MENUBAR_H, COL_MENUBAR_BG);
    w(win, 0, y + MENUBAR_H - 1, WIN_W, 1, 0xFF313244);
    for i in 0..MENU_COUNT {
        let tx = tab_x(i);
        let is_open = doc.menu_open == Some(i);
        if is_open { w(win, tx, y, MENU_TAB_W, MENUBAR_H - 1, COL_MENU_HOT); }
        let col = if is_open { COL_ACCENT } else { COL_TEXT };
        t(win, tx + 8, y + 3, col, MENU_LABELS[i]);
    }
    if let Some(mi) = doc.menu_open { draw_dropdown(win, doc, mi); }
}

fn draw_dropdown(win: GuiWindow, doc: &Doc, menu_idx: usize) {
    let dx = tab_x(menu_idx);
    let dy = TITLE_H + MENUBAR_H;
    let dh = dropdown_height(menu_idx);
    // Shadow
    w(win, dx + 3, dy + 3, DROP_W, dh, 0x88000000);
    // Panel + border
    w(win, dx, dy, DROP_W, dh, COL_DROP_BG);
    w(win, dx, dy, DROP_W, 1, COL_DROP_BORDER);
    w(win, dx, dy + dh - 1, DROP_W, 1, COL_DROP_BORDER);
    w(win, dx, dy, 1, dh, COL_DROP_BORDER);
    w(win, dx + DROP_W - 1, dy, 1, dh, COL_DROP_BORDER);

    let mut iy = dy + 2;
    for i in 0..MENU_COUNTS[menu_idx] {
        let label = item_label(menu_idx, i);
        if label == "|" {
            w(win, dx + 8, iy + DROP_SEP_H / 2, DROP_W - 16, 1, COL_DROP_BORDER);
            iy += DROP_SEP_H;
            continue;
        }
        if doc.menu_item == i {
            w(win, dx + 1, iy, DROP_W - 2, DROP_ITEM_H, COL_DROP_HOT);
        }
        let col = if doc.menu_item == i { COL_ACCENT } else { COL_TEXT };
        t(win, dx + 14, iy + 2, col, label);
        iy += DROP_ITEM_H;
    }
}

fn draw_text_area(win: GuiWindow, doc: &Doc) {
    w(win, 0, TEXT_Y, WIN_W, TEXT_H, COL_BG);
    for r in 0..VISIBLE_ROWS {
        let row = doc.scroll + r;
        if row >= doc.count { break; }
        let y = TEXT_Y + r as u32 * LINE_H;

        if doc.show_linenum {
            let mut nb = [0u8; 8];
            let ns = fmt_u32(&mut nb, (row + 1) as u32);
            let npad = 4usize.saturating_sub(ns.len());
            t(win, TEXT_X + npad as u32 * CHAR_W, y, COL_LINE_NUM, ns);
            w(win, TEXT_X + 4 * CHAR_W, y, 1, LINE_H, 0xFF313244);
        }
        if row == doc.cur_row {
            w(win, EDIT_X, y, TEXT_W - GUTTER_W, LINE_H, COL_SELECTION);
        }
        let line = &doc.lines[row];
        let hs = if row == doc.cur_row && doc.cur_col >= EDIT_COLS { doc.cur_col.saturating_sub(EDIT_COLS - 1) } else { 0 };
        let bytes = line.as_str().as_bytes();
        if hs < bytes.len() {
            let vis = &bytes[hs..]; let vl = vis.len().min(EDIT_COLS + 4);
            if let Ok(s) = core::str::from_utf8(&vis[..vl]) { t(win, EDIT_X, y, COL_TEXT, s); }
        }
        if row == doc.cur_row {
            let cv = doc.cur_col.saturating_sub(hs);
            let cx = EDIT_X + cv as u32 * CHAR_W;
            if cx + CHAR_W <= TEXT_X + TEXT_W {
                w(win, cx, y, CHAR_W, LINE_H, COL_CURSOR_BG);
                if doc.cur_col < line.len {
                    if let Ok(cs) = core::str::from_utf8(core::slice::from_ref(&line.buf[doc.cur_col])) {
                        t(win, cx, y, COL_CURSOR_FG, cs);
                    }
                }
            }
        }
    }
}

fn draw_status(win: GuiWindow, doc: &Doc, ticks: u64) {
    if !doc.show_status { return; }
    let sy = WIN_H - STATUS_H;
    w(win, 0, sy, WIN_W, STATUS_H, COL_STATUS_BG);
    w(win, 0, sy, WIN_W, 1, 0xFF313244);
    if doc.saveas_mode {
        w(win, 0, sy, WIN_W, STATUS_H, COL_SAVE_PROMPT);
        t(win, 4, sy + 3, COL_ACCENT, "Save as: ");
        let px = 4 + 9 * CHAR_W;
        let name = core::str::from_utf8(&doc.saveas_buf[..doc.saveas_len]).unwrap_or("");
        t(win, px, sy + 3, COL_TEXT, name);
        w(win, px + doc.saveas_len as u32 * CHAR_W, sy + 3, 2, STATUS_H - 6, COL_ACCENT);
        return;
    }
    let expired = ticks.saturating_sub(doc.msg_ticks) > 300;
    if doc.msg_len > 0 && !expired {
        t(win, 4, sy + 3, if doc.msg_ok { COL_SAVED } else { COL_WARN }, doc.msg_str());
    } else {
        t(win, 4, sy + 3, COL_HINT, "^S Save  ^W SaveAs  ^Q Quit  Alt+F Menu");
        let mut b = [0u8; 8];
        let lc = fmt_u32(&mut b, doc.count as u32);
        let mut info = [0u8; 24]; let mut il = 0usize;
        for &c in b"Ln:" { if il < info.len() { info[il] = c; il += 1; } }
        for &c in lc.as_bytes() { if il < info.len() { info[il] = c; il += 1; } }
        let istr = core::str::from_utf8(&info[..il]).unwrap_or("");
        let ix = WIN_W.saturating_sub(istr.len() as u32 * CHAR_W + 4);
        t(win, ix, sy + 3, COL_DIM, istr);
    }
}

fn draw_about(win: GuiWindow, doc: &Doc) {
    if !doc.show_about { return; }
    let aw = 240u32; let ah = 90u32;
    let ax = (WIN_W - aw) / 2; let ay = (WIN_H - ah) / 2;
    w(win, ax + 3, ay + 3, aw, ah, 0x88000000);
    w(win, ax, ay, aw, ah, 0xFF242434);
    w(win, ax, ay, aw, 22, 0xFF89B4FA);
    t(win, ax + 8, ay + 5, 0xFF1E1E2E, "About edit");
    t(win, ax + 12, ay + 28, COL_TEXT, "OxideOS Text Editor");
    t(win, ax + 12, ay + 44, COL_DIM,  "File-based  |  Alt+F opens menu");
    t(win, ax + 12, ay + 60, COL_DIM,  "Click Help > About to close");
}

fn redraw(win: GuiWindow, doc: &Doc, ticks: u64) {
    draw_title(win, doc);
    draw_menubar(win, doc);
    draw_text_area(win, doc);
    draw_status(win, doc, ticks);
    draw_about(win, doc);
    gui_present(win);
}

// ── Mouse click handling ──────────────────────────────────────────────────────

/// Returns true if the editor should quit.
fn handle_click(doc: &mut Doc, x: u32, y: u32) -> bool {
    // Close about overlay on any click
    if doc.show_about { doc.show_about = false; return false; }

    // Click on menu bar (but NOT on an open dropdown)
    if y >= TITLE_H && y < TITLE_H + MENUBAR_H {
        for i in 0..MENU_COUNT {
            let tx = tab_x(i);
            if x >= tx && x < tx + MENU_TAB_W {
                if doc.menu_open == Some(i) { doc.close_menu(); }
                else { doc.open_menu(i); }
                return false;
            }
        }
        doc.close_menu();
        return false;
    }

    // Click inside an open dropdown
    if let Some(mi) = doc.menu_open {
        let dx = tab_x(mi);
        let dy = TITLE_H + MENUBAR_H;
        let dh = dropdown_height(mi);
        if x >= dx && x < dx + DROP_W && y >= dy && y < dy + dh {
            let rel_y = y - dy;
            if let Some(item) = item_at_y(mi, rel_y) {
                doc.close_menu();
                return run_item(doc, mi, item);
            }
            return false;
        }
    }

    // Click anywhere else — close menu
    doc.close_menu();
    false
}

/// Run the item at `(menu, item)`. Returns true to quit.
fn run_item(doc: &mut Doc, menu: usize, item: usize) -> bool {
    match (menu, item) {
        (0, 0) => { doc.new_file(); }
        (0, 1) => { doc.save_file(); }
        (0, 2) => { doc.begin_saveas(); }
        (0, 4) => {
            if doc.modified { doc.set_msg("Unsaved changes — save first or use Ctrl+X", false); }
            else { return true; }
        }
        (1, 0) => { doc.select_all(); }
        (1, 2) => { doc.word_wrap = !doc.word_wrap; doc.set_msg(if doc.word_wrap { "Word wrap on" } else { "Word wrap off" }, true); }
        (2, 0) => { doc.show_status = !doc.show_status; }
        (2, 1) => { doc.show_linenum = !doc.show_linenum; }
        (3, 0) => { doc.show_about = !doc.show_about; }
        _ => {}
    }
    false
}

// ── Keyboard: escape-sequence reader (non-blocking with timeout) ──────────────

fn next_key_timeout(win: GuiWindow, timeout_ticks: u64) -> Option<u8> {
    let deadline = get_time() + timeout_ticks;
    loop {
        while let Some(ev) = gui_poll_event(win) {
            if let Some(ch) = ev.as_key() { return Some(ch); }
            if ev.is_close() { exit(0); }
        }
        if get_time() >= deadline { return None; }
        sleep_ms(5);
    }
}

/// Process an ESC that was already received.  Returns true to quit.
fn handle_esc(doc: &mut Doc, win: GuiWindow) -> bool {
    let Some(b) = next_key_timeout(win, 5) else {
        // Bare ESC — close menu
        doc.close_menu(); return false;
    };
    match b {
        0x1B => { doc.close_menu(); return false; } // ESC ESC
        b'f' | b'F' => { doc.open_menu(0); return false; }
        b'e' | b'E' => { doc.open_menu(1); return false; }
        b'v' | b'V' => { doc.open_menu(2); return false; }
        b'h' | b'H' => { doc.open_menu(3); return false; }
        b'[' => {}    // ANSI CSI — fall through
        _ => return false,
    }
    let Some(code) = next_key_timeout(win, 5) else { return false; };
    match code {
        b'A' => { if doc.menu_open.is_some() { doc.menu_prev(); } else { doc.move_up(); } }
        b'B' => { if doc.menu_open.is_some() { doc.menu_next(); } else { doc.move_down(); } }
        b'C' => { if doc.menu_open.is_some() { doc.menu_right(); } else { doc.move_right(); } }
        b'D' => { if doc.menu_open.is_some() { doc.menu_left(); } else { doc.move_left(); } }
        b'H' => doc.home(),
        b'F' => doc.end(),
        b'3' => { let _ = next_key_timeout(win, 5); doc.delete_char(); } // Del
        b'5' => { let _ = next_key_timeout(win, 5); doc.page_up(); }
        b'6' => { let _ = next_key_timeout(win, 5); doc.page_down(); }
        _ => {}
    }
    false
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let Some(win) = gui_create("edit", WIN_W, WIN_H) else { exit(1); };
    let mut doc = Doc::new();

    // Load filename from /tmp/edit_arg if available
    {
        let fd = open("/tmp/edit_arg", 1);
        if fd >= 0 {
            let mut ab = [0u8; 128];
            let n = read(fd, &mut ab);
            close(fd);
            if n > 0 {
                let len = (n as usize).min(127);
                let len = if len > 0 && ab[len-1] == b'\n' { len-1 } else { len };
                doc.filename[..len].copy_from_slice(&ab[..len]);
                doc.fname_len = len;
                doc.load_file();
            }
        }
    }

    redraw(win, &doc, get_time());

    loop {
        let mut dirty = false;

        // Drain all pending events
        while let Some(ev) = gui_poll_event(win) {
            if ev.is_close() { exit(0); }

            // Mouse click
            if let Some((mx, my, btn, pressed)) = ev.as_mouse_btn() {
                if btn == 0 && pressed {
                    if doc.saveas_mode {
                        // Clicks outside save-as cancel it
                        let sy = WIN_H - STATUS_H;
                        if !((my as u32) >= sy && (my as u32) < sy + STATUS_H) {
                            doc.saveas_mode = false;
                        }
                    } else if handle_click(&mut doc, mx as u32, my as u32) {
                        exit(0);
                    }
                    dirty = true;
                }
            }

            // Keyboard
            if let Some(c) = ev.as_key() {
                dirty = true;

                // Save-as prompt absorbs all keystrokes
                if doc.saveas_mode {
                    match c {
                        b'\n' | b'\r' => doc.commit_saveas(),
                        0x1B           => { doc.saveas_mode = false; doc.set_msg("Cancelled", false); }
                        8 | 127        => { if doc.saveas_len > 0 { doc.saveas_len -= 1; } }
                        32..=126       => { if doc.saveas_len < 127 { doc.saveas_buf[doc.saveas_len] = c; doc.saveas_len += 1; } }
                        _ => {}
                    }
                    continue;
                }

                // Menu navigation absorbs all keystrokes when a menu is open
                if doc.menu_open.is_some() {
                    match c {
                        0x1B => { if handle_esc(&mut doc, win) { exit(0); } }
                        b'\n' | b'\r' => {
                            let (m, i) = (doc.menu_open.unwrap(), doc.menu_item);
                            doc.close_menu();
                            if run_item(&mut doc, m, i) { exit(0); }
                        }
                        _ => { doc.close_menu(); }
                    }
                    continue;
                }

                // Normal editing
                match c {
                    0x13 => doc.save_file(),           // Ctrl+S
                    0x17 => doc.begin_saveas(),        // Ctrl+W  Save As
                    0x0E => doc.new_file(),            // Ctrl+N
                    0x01 => doc.select_all(),          // Ctrl+A
                    0x11 => {                          // Ctrl+Q
                        if doc.modified {
                            doc.set_msg("Unsaved! Ctrl+Q again or use File > Quit", false);
                            redraw(win, &doc, get_time());
                            // Wait for confirmation
                            loop {
                                if let Some(ev2) = gui_poll_event(win) {
                                    if ev2.is_close() { exit(0); }
                                    if let Some(c2) = ev2.as_key() {
                                        if c2 == 0x11 { exit(0); }
                                        doc.set_msg("", true);
                                        break;
                                    }
                                }
                                sleep_ms(10);
                            }
                        } else { exit(0); }
                    }
                    0x18 => exit(0),                   // Ctrl+X
                    0x1B => { if handle_esc(&mut doc, win) { exit(0); } }
                    b'\n' | b'\r' => doc.insert_newline(),
                    8 | 127       => doc.backspace(),
                    b'\t'         => { for _ in 0..4 { doc.insert_char(b' '); } }
                    b if b >= 32 && b < 127 => doc.insert_char(b),
                    _ => { dirty = false; }
                }
            }
        }

        if dirty { redraw(win, &doc, get_time()); }
        else {
            // Blink cursor: redraw once per second for the cursor flash
            let t = get_time();
            if t % 100 < 5 { redraw(win, &doc, t); }
        }
        sleep_ms(16);
    }
}
