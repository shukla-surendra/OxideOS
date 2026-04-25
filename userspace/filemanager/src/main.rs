//! OxideOS File Manager — v2
//!
//! Responsive GUI file manager with keyboard navigation, file sizes, and a
//! clickable sidebar.  All layout recomputes from the live window dimensions.
#![no_std]
#![no_main]

use oxide_rt::{
    exit, sleep_ms, readdir, chdir, getcwd, stat, FileStat,
    gui_create, gui_fill_rect, gui_draw_text, gui_present,
    gui_poll_event, gui_get_size,
    GuiEvent, GuiWindow,
};

// ── Window & grid constants ────────────────────────────────────────────────────

const WIN_W_INIT: u32 = 760;
const WIN_H_INIT: u32 = 500;

const CHAR_W:     u32 = 9;
const PAD:        u32 = 8;
const ROW_H:      u32 = 20;
const HEADER_H:   u32 = 20;
const TOOLBAR_H:  u32 = 32;
const STATUS_H:   u32 = 22;
const SIDEBAR_W:  u32 = 180;
const DIV_W:      u32 = 1;
const SCROLL_W:   u32 = 10;

// Right-pane columns (measured from the right edge)
const COL_TYPE_W: u32 = 50;
const COL_SIZE_W: u32 = 68;

// ── Color palette (VS Code dark theme inspired) ────────────────────────────────

const COL_BG:           u32 = 0xFF1E1E1E;
const COL_SIDEBAR_BG:   u32 = 0xFF252526;
const COL_TOOLBAR_BG:   u32 = 0xFF3C3C3C;
const COL_HEADER_BG:    u32 = 0xFF2D2D30;
const COL_STATUS_BG:    u32 = 0xFF007ACC;
const COL_ROW_ODD:      u32 = 0xFF252526;
const COL_SELECTED:     u32 = 0xFF094771;
const COL_HOVER:        u32 = 0xFF2A2D2E;
const COL_SIDEBAR_CUR:  u32 = 0xFF37373D;
const COL_SIDEBAR_HOV:  u32 = 0xFF2A2D2E;
const COL_DIVIDER:      u32 = 0xFF3F3F46;
const COL_TEXT:         u32 = 0xFFD4D4D4;
const COL_TEXT_DIM:     u32 = 0xFF858585;
const COL_DIR:          u32 = 0xFF4EC9B0;
const COL_FILE:         u32 = 0xFFCCCCCC;
const COL_ACCENT:       u32 = 0xFF4EC9B0;
const COL_STATUS_TXT:   u32 = 0xFFFFFFFF;
const COL_SCROLL_TRACK: u32 = 0xFF3C3C3C;
const COL_SCROLL_THUMB: u32 = 0xFF6D6D6D;
const COL_SIZE_FG:      u32 = 0xFF808080;
const COL_BTN_BG:       u32 = 0xFF4D4D4D;

// ── Escape-sequence state machine ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum EscState { None, GotEsc, GotBracket }

// ── Responsive layout ─────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Layout {
    w:         u32,
    h:         u32,
    right_x:   u32,  // sidebar + divider
    list_y0:   u32,  // top of file rows (below toolbar + header)
    list_h:    u32,  // pixel height of the file list area
    status_y:  u32,  // top of status bar
    sidebar_h: u32,  // height of sidebar area
    scroll_x:  u32,  // left edge of scrollbar
    col_size_x: u32, // left edge of SIZE column
    col_type_x: u32, // left edge of TYPE column
}

impl Layout {
    fn from_win(win: GuiWindow) -> Self {
        let w = win.width.max(SIDEBAR_W + DIV_W + 220);
        let h = win.height.max(TOOLBAR_H + HEADER_H + STATUS_H + ROW_H * 2);
        let right_x   = SIDEBAR_W + DIV_W;
        let status_y  = h.saturating_sub(STATUS_H);
        let sidebar_h = h.saturating_sub(TOOLBAR_H + STATUS_H);
        let scroll_x  = w.saturating_sub(SCROLL_W);
        let list_y0   = TOOLBAR_H + HEADER_H;
        let list_h    = status_y.saturating_sub(list_y0);
        let col_type_x = scroll_x.saturating_sub(COL_TYPE_W + PAD);
        let col_size_x = col_type_x.saturating_sub(COL_SIZE_W + PAD);
        Self { w, h, right_x, list_y0, list_h, status_y, sidebar_h,
               scroll_x, col_size_x, col_type_x }
    }

    fn visible_rows(&self) -> usize { (self.list_h / ROW_H) as usize }
}

// ── Heap-free fixed-capacity string ───────────────────────────────────────────

#[derive(Clone, Copy)]
struct FixStr<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FixStr<N> {
    const fn new() -> Self { Self { buf: [0; N], len: 0 } }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    fn push(&mut self, b: u8) {
        if self.len < N { self.buf[self.len] = b; self.len += 1; }
    }

    fn push_str(&mut self, s: &str) { for b in s.bytes() { self.push(b); } }

    fn clear(&mut self) { self.len = 0; }

    fn push_u64(&mut self, mut v: u64) {
        let mut tmp = [0u8; 20];
        let mut i = tmp.len();
        if v == 0 { i -= 1; tmp[i] = b'0'; }
        while v > 0 { i -= 1; tmp[i] = b'0' + (v % 10) as u8; v /= 10; }
        self.push_str(core::str::from_utf8(&tmp[i..]).unwrap_or(""));
    }

    fn push_usize(&mut self, v: usize) { self.push_u64(v as u64); }

    fn push_size(&mut self, bytes: u64) {
        const MB: u64 = 1024 * 1024;
        const KB: u64 = 1024;
        if bytes >= MB {
            self.push_u64(bytes / MB);
            self.push(b'.');
            self.push_u64((bytes % MB) * 10 / MB);
            self.push_str("M");
        } else if bytes >= KB {
            self.push_u64(bytes / KB);
            self.push(b'K');
        } else {
            self.push_u64(bytes);
            self.push(b'B');
        }
    }
}

// ── Directory entry ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct DirEntry {
    name:      FixStr<128>,
    is_dir:    bool,
    size:      u64,
    have_size: bool,
}

impl DirEntry {
    const fn empty() -> Self {
        Self { name: FixStr::new(), is_dir: false, size: 0, have_size: false }
    }
}

// ── Sidebar items ─────────────────────────────────────────────────────────────

struct SidebarItem {
    label: &'static str,
    path:  &'static str,
}

const SIDEBAR_ITEMS: &[SidebarItem] = &[
    SidebarItem { label: "/ Root",      path: "/" },
    SidebarItem { label: "/bin",        path: "/bin" },
    SidebarItem { label: "/dev",        path: "/dev" },
    SidebarItem { label: "/disk",       path: "/disk" },
    SidebarItem { label: "/tmp",        path: "/tmp" },
];

const SIDEBAR_HDR_H:  u32 = 24;
const SIDEBAR_ITEM_H: u32 = 22;

// ── Application state ─────────────────────────────────────────────────────────

const MAX_ENTRIES: usize = 128;

struct App {
    win:          GuiWindow,
    cwd:          FixStr<256>,
    entries:      [DirEntry; MAX_ENTRIES],
    entry_count:  usize,
    selected:     usize,
    scroll:       usize,
    hover:        Option<usize>,
    sidebar_hover: Option<usize>,
    dirty:        bool,
    esc:          EscState,
}

impl App {
    fn new(win: GuiWindow) -> Self {
        let mut a = Self {
            win,
            cwd:           FixStr::new(),
            entries:       [DirEntry::empty(); MAX_ENTRIES],
            entry_count:   0,
            selected:      0,
            scroll:        0,
            hover:         None,
            sidebar_hover: None,
            dirty:         true,
            esc:           EscState::None,
        };
        a.refresh_cwd();
        a.load_entries();
        a
    }

    fn refresh_cwd(&mut self) {
        let mut buf = [0u8; 256];
        let n = getcwd(&mut buf);
        self.cwd.clear();
        if n > 0 {
            self.cwd.push_str(
                core::str::from_utf8(&buf[..n as usize]).unwrap_or("/")
            );
        } else {
            self.cwd.push(b'/');
        }
    }

    fn load_entries(&mut self) {
        self.entries     = [DirEntry::empty(); MAX_ENTRIES];
        self.entry_count = 0;
        self.selected    = 0;
        self.scroll      = 0;

        let mut buf = [0u8; 8192];
        let n = readdir(self.cwd.as_str(), &mut buf);
        if n <= 0 { self.dirty = true; return; }

        let raw = core::str::from_utf8(&buf[..n as usize]).unwrap_or("");
        for line in raw.lines() {
            if self.entry_count >= MAX_ENTRIES { break; }
            if line.is_empty() { continue; }
            let (name, is_dir) = if line.ends_with('/') {
                (&line[..line.len() - 1], true)
            } else {
                (line, false)
            };
            if name == "." { continue; }

            let mut e = DirEntry::empty();
            e.name.push_str(name);
            e.is_dir = is_dir;
            self.entries[self.entry_count] = e;
            self.entry_count += 1;
        }

        // Fetch sizes for regular files
        for i in 0..self.entry_count {
            if self.entries[i].is_dir { continue; }
            if self.entries[i].name.as_str() == ".." { continue; }

            let mut path = FixStr::<384>::new();
            path.push_str(self.cwd.as_str());
            if !self.cwd.as_str().ends_with('/') { path.push(b'/'); }
            path.push_str(self.entries[i].name.as_str());

            let mut st = FileStat::zeroed();
            if stat(path.as_str(), &mut st) == 0 {
                self.entries[i].size      = st.size;
                self.entries[i].have_size = true;
            }
        }

        self.dirty = true;
    }

    fn navigate_to(&mut self, path: &str) {
        if chdir(path) >= 0 {
            self.refresh_cwd();
            self.load_entries();
        }
    }

    fn enter_selected(&mut self) {
        if self.selected >= self.entry_count { return; }
        let e = self.entries[self.selected];
        if !e.is_dir { return; }
        if e.name.as_str() == ".." {
            self.navigate_to("..");
        } else {
            let mut path = FixStr::<256>::new();
            path.push_str(self.cwd.as_str());
            if !self.cwd.as_str().ends_with('/') { path.push(b'/'); }
            path.push_str(e.name.as_str());
            self.navigate_to(path.as_str());
        }
    }

    fn clamp_scroll(&mut self) {
        let vis = Layout::from_win(self.win).visible_rows();
        if self.entry_count == 0 { self.scroll = 0; return; }
        let max_scroll = self.entry_count.saturating_sub(vis);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + vis {
            self.scroll = self.selected + 1 - vis;
        }
        if self.scroll > max_scroll { self.scroll = max_scroll; }
    }

    fn sel_up(&mut self) {
        if self.selected > 0 { self.selected -= 1; }
        self.clamp_scroll();
        self.dirty = true;
    }

    fn sel_down(&mut self) {
        if self.entry_count > 0 && self.selected + 1 < self.entry_count {
            self.selected += 1;
        }
        self.clamp_scroll();
        self.dirty = true;
    }

    fn sync_size(&mut self) -> bool {
        let (nw, nh) = gui_get_size(self.win);
        if nw != self.win.width || nh != self.win.height {
            self.win.width  = nw;
            self.win.height = nh;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    fn handle_event(&mut self, ev: GuiEvent) -> bool {
        if ev.is_close() { exit(0); }

        let lay = Layout::from_win(self.win);

        // ── Keyboard ──────────────────────────────────────────────────────
        if let Some(key) = ev.as_key() {
            match self.esc {
                EscState::GotBracket => {
                    self.esc = EscState::None;
                    match key {
                        b'A' => { self.sel_up();   return true; }
                        b'B' => { self.sel_down(); return true; }
                        b'H' | b'1' => {
                            self.selected = 0; self.scroll = 0;
                            self.dirty = true; return true;
                        }
                        b'F' | b'4' => {
                            if self.entry_count > 0 {
                                self.selected = self.entry_count - 1;
                                self.clamp_scroll();
                                self.dirty = true;
                            }
                            return true;
                        }
                        b'5' => {
                            let vis = lay.visible_rows();
                            self.selected = self.selected.saturating_sub(vis);
                            self.clamp_scroll(); self.dirty = true; return true;
                        }
                        b'6' => {
                            let vis = lay.visible_rows();
                            if self.entry_count > 0 {
                                self.selected =
                                    (self.selected + vis).min(self.entry_count - 1);
                                self.clamp_scroll(); self.dirty = true;
                            }
                            return true;
                        }
                        _ => {}
                    }
                    return false;
                }
                EscState::GotEsc => {
                    self.esc = if key == b'[' { EscState::GotBracket } else { EscState::None };
                    return false;
                }
                EscState::None => {
                    match key {
                        0x1B      => { self.esc = EscState::GotEsc; return false; }
                        b'q' | 3  => exit(0),
                        b'\n' | b'\r' => { self.enter_selected(); return true; }
                        8 | 127   => { self.navigate_to(".."); return true; }
                        b'j'      => { self.sel_down(); return true; }
                        b'k'      => { self.sel_up();   return true; }
                        b'r' | b'R' => { self.load_entries(); return true; }
                        b'g'      => {
                            self.selected = 0; self.scroll = 0;
                            self.dirty = true; return true;
                        }
                        b'G'      => {
                            if self.entry_count > 0 {
                                self.selected = self.entry_count - 1;
                                self.clamp_scroll(); self.dirty = true;
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
            }
        }

        // ── Mouse click ───────────────────────────────────────────────────
        if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
            let bx = x as u32; let by = y as u32;

            if pressed {
                // File list area
                if bx >= lay.right_x && bx < lay.scroll_x
                    && by >= lay.list_y0 && by < lay.status_y
                {
                    let row_y = by.saturating_sub(lay.list_y0);
                    let idx   = self.scroll + (row_y / ROW_H) as usize;
                    if idx < self.entry_count {
                        if self.selected == idx {
                            self.enter_selected();
                        } else {
                            self.selected = idx;
                            self.dirty    = true;
                        }
                        return true;
                    }
                }

                // Sidebar items (below header)
                if bx < SIDEBAR_W && by >= TOOLBAR_H + SIDEBAR_HDR_H {
                    let rel_y = by.saturating_sub(TOOLBAR_H + SIDEBAR_HDR_H);
                    let i     = (rel_y / SIDEBAR_ITEM_H) as usize;
                    if i < SIDEBAR_ITEMS.len() {
                        self.navigate_to(SIDEBAR_ITEMS[i].path);
                        return true;
                    }
                }

                // Back button in toolbar
                if bx >= PAD && bx < PAD + 56 && by >= 6 && by < 26 {
                    self.navigate_to("..");
                    return true;
                }
            }
        }

        // ── Mouse move (hover) ────────────────────────────────────────────
        if let Some((x, y)) = ev.as_mouse_move() {
            let mx = x as u32; let my = y as u32;
            let mut changed = false;

            if mx >= lay.right_x && mx < lay.scroll_x
                && my >= lay.list_y0 && my < lay.status_y
            {
                let row_y = my.saturating_sub(lay.list_y0);
                let idx   = self.scroll + (row_y / ROW_H) as usize;
                let new_h = if idx < self.entry_count { Some(idx) } else { None };
                if new_h != self.hover { self.hover = new_h; changed = true; }
            } else if self.hover.is_some() {
                self.hover = None; changed = true;
            }

            if mx < SIDEBAR_W && my >= TOOLBAR_H + SIDEBAR_HDR_H {
                let rel_y = my.saturating_sub(TOOLBAR_H + SIDEBAR_HDR_H);
                let i     = (rel_y / SIDEBAR_ITEM_H) as usize;
                let new_h = if i < SIDEBAR_ITEMS.len() { Some(i) } else { None };
                if new_h != self.sidebar_hover { self.sidebar_hover = new_h; changed = true; }
            } else if self.sidebar_hover.is_some() {
                self.sidebar_hover = None; changed = true;
            }

            if changed { self.dirty = true; return true; }
        }

        false
    }

    fn draw(&self) {
        let win = self.win;
        let lay = Layout::from_win(win);

        // Fill overall background
        gui_fill_rect(win, 0, 0, lay.w, lay.h, COL_BG);

        // ── Toolbar ───────────────────────────────────────────────────────
        gui_fill_rect(win, 0, 0, lay.w, TOOLBAR_H, COL_TOOLBAR_BG);
        gui_fill_rect(win, 0, TOOLBAR_H - 1, lay.w, 1, COL_DIVIDER);

        // Back button
        gui_fill_rect(win, PAD, 6, 56, 20, COL_BTN_BG);
        gui_fill_rect(win, PAD, 6, 56, 1, 0xFF5A5A5A);
        gui_fill_rect(win, PAD, 25, 56, 1, 0xFF333333);
        gui_draw_text(win, PAD + 7, 10, COL_TEXT, "<- Back");

        // Path display
        let path_x = PAD + 64;
        let path_w = lay.w.saturating_sub(path_x + PAD);
        gui_fill_rect(win, path_x, 6, path_w, 20, 0xFF2D2D30);
        gui_fill_rect(win, path_x, 6, 1, 20, COL_DIVIDER);
        gui_fill_rect(win, path_x + path_w - 1, 6, 1, 20, COL_DIVIDER);
        gui_draw_text(win, path_x + 8, 10, COL_DIR, self.cwd.as_str());

        // ── Sidebar ───────────────────────────────────────────────────────
        gui_fill_rect(win, 0, TOOLBAR_H, SIDEBAR_W, lay.sidebar_h, COL_SIDEBAR_BG);

        // Sidebar header row
        gui_fill_rect(win, 0, TOOLBAR_H, SIDEBAR_W, SIDEBAR_HDR_H, 0xFF1E1E1E);
        gui_draw_text(win, PAD, TOOLBAR_H + 5, COL_TEXT_DIM, "EXPLORER");
        gui_fill_rect(win, 0, TOOLBAR_H + SIDEBAR_HDR_H - 1, SIDEBAR_W, 1, COL_DIVIDER);

        for (i, item) in SIDEBAR_ITEMS.iter().enumerate() {
            let iy = TOOLBAR_H + SIDEBAR_HDR_H + i as u32 * SIDEBAR_ITEM_H;
            if iy + SIDEBAR_ITEM_H > lay.status_y { break; }

            let is_cur = self.cwd.as_str() == item.path;
            let is_hov = self.sidebar_hover == Some(i);
            let bg = if is_cur { COL_SIDEBAR_CUR }
                     else if is_hov { COL_SIDEBAR_HOV }
                     else { COL_SIDEBAR_BG };

            gui_fill_rect(win, 0, iy, SIDEBAR_W, SIDEBAR_ITEM_H, bg);
            if is_cur {
                gui_fill_rect(win, 0, iy, 2, SIDEBAR_ITEM_H, COL_ACCENT);
            }
            let col = if is_cur { COL_DIR } else { COL_TEXT };
            gui_draw_text(win, PAD + 8, iy + 4, col, item.label);
        }

        // ── Sidebar / list divider ────────────────────────────────────────
        gui_fill_rect(win, SIDEBAR_W, TOOLBAR_H, DIV_W, lay.sidebar_h, COL_DIVIDER);

        // ── Column header row ─────────────────────────────────────────────
        let hdr_y = TOOLBAR_H;
        gui_fill_rect(win, lay.right_x, hdr_y, lay.w - lay.right_x, HEADER_H, COL_HEADER_BG);
        gui_fill_rect(win, lay.right_x, hdr_y + HEADER_H - 1, lay.w - lay.right_x, 1, COL_DIVIDER);

        gui_draw_text(win, lay.right_x + PAD + 28, hdr_y + 3, COL_TEXT_DIM, "NAME");
        gui_draw_text(win, lay.col_size_x, hdr_y + 3, COL_TEXT_DIM, "SIZE");
        gui_draw_text(win, lay.col_type_x, hdr_y + 3, COL_TEXT_DIM, "TYPE");

        // Column separator lines in header
        gui_fill_rect(win, lay.col_size_x - 4, hdr_y, 1, HEADER_H, COL_DIVIDER);
        gui_fill_rect(win, lay.col_type_x - 4, hdr_y, 1, HEADER_H, COL_DIVIDER);

        // ── File list ─────────────────────────────────────────────────────
        let vis = lay.visible_rows();
        let name_w = lay.col_size_x.saturating_sub(lay.right_x + PAD + 28 + 4);

        for row in 0..vis {
            let idx = self.scroll + row;
            let ry  = lay.list_y0 + row as u32 * ROW_H;
            if ry + ROW_H > lay.status_y { break; }

            // Alternating background for empty rows
            if idx >= self.entry_count {
                if row % 2 != 0 {
                    gui_fill_rect(win, lay.right_x, ry, lay.scroll_x - lay.right_x, ROW_H, COL_ROW_ODD);
                }
                continue;
            }

            let e       = self.entries[idx];
            let is_sel  = idx == self.selected;
            let is_hov  = self.hover == Some(idx);
            let row_bg  = if is_sel { COL_SELECTED }
                          else if is_hov { COL_HOVER }
                          else if row % 2 != 0 { COL_ROW_ODD }
                          else { COL_BG };

            gui_fill_rect(win, lay.right_x, ry, lay.scroll_x - lay.right_x, ROW_H, row_bg);

            // Left accent bar on selected row
            if is_sel {
                gui_fill_rect(win, lay.right_x, ry, 2, ROW_H, COL_ACCENT);
            }

            // Icon
            let (icon, icon_col) = if e.is_dir {
                if e.name.as_str() == ".." { ("..", COL_TEXT_DIM) }
                else { ("dir", COL_DIR) }
            } else {
                ("   ", COL_TEXT_DIM)
            };
            gui_draw_text(win, lay.right_x + PAD, ry + 2, icon_col, icon);

            // Name (truncated)
            let name_str  = e.name.as_str();
            let max_chars = (name_w / CHAR_W) as usize;
            let (display, truncated) = if name_str.len() > max_chars && max_chars > 3 {
                (&name_str[..max_chars.saturating_sub(2)], true)
            } else {
                (name_str, false)
            };
            let name_col = if e.is_dir { COL_DIR } else { COL_FILE };
            gui_draw_text(win, lay.right_x + PAD + 28, ry + 2, name_col, display);
            if truncated {
                let tx = lay.right_x + PAD + 28 + display.len() as u32 * CHAR_W;
                gui_draw_text(win, tx, ry + 2, COL_TEXT_DIM, "..");
            }

            // Size column (files only)
            if !e.is_dir && e.have_size {
                let mut sz = FixStr::<16>::new();
                sz.push_size(e.size);
                gui_draw_text(win, lay.col_size_x, ry + 2, COL_SIZE_FG, sz.as_str());
            }

            // Type column
            let (type_str, type_col) = if e.is_dir { ("DIR",  COL_DIR) }
                                       else          { ("FILE", COL_TEXT_DIM) };
            gui_draw_text(win, lay.col_type_x, ry + 2, type_col, type_str);
        }

        // ── Scrollbar ─────────────────────────────────────────────────────
        gui_fill_rect(win, lay.scroll_x, lay.list_y0, SCROLL_W, lay.list_h, COL_SCROLL_TRACK);
        if vis > 0 && self.entry_count > vis {
            let total   = self.entry_count as u32;
            let vis_u   = vis as u32;
            let thumb_h = (lay.list_h * vis_u / total).max(16).min(lay.list_h);
            let max_sc  = total.saturating_sub(vis_u);
            let thumb_y = if max_sc > 0 {
                (lay.list_h - thumb_h) * self.scroll as u32 / max_sc
            } else { 0 };
            gui_fill_rect(win, lay.scroll_x + 2, lay.list_y0 + thumb_y,
                          SCROLL_W - 4, thumb_h, COL_SCROLL_THUMB);
        }

        // ── Status bar ────────────────────────────────────────────────────
        gui_fill_rect(win, 0, lay.status_y, lay.w, STATUS_H, COL_STATUS_BG);

        let mut st = FixStr::<128>::new();
        st.push_str("  ");
        st.push_usize(self.entry_count);
        st.push_str(" items");
        if self.selected < self.entry_count {
            let e = &self.entries[self.selected];
            st.push_str("  |  ");
            st.push_str(e.name.as_str());
            if !e.is_dir && e.have_size {
                st.push_str("  (");
                st.push_size(e.size);
                st.push(b')');
            }
        }
        gui_draw_text(win, 0, lay.status_y + 3, COL_STATUS_TXT, st.as_str());

        // Key hint (right-aligned)
        let hint    = "  arrows/jk  enter  bksp=up  r=refresh  q=quit  ";
        let hint_x  = lay.w.saturating_sub(hint.len() as u32 * CHAR_W);
        gui_draw_text(win, hint_x, lay.status_y + 3, 0xFFD0E8F8, hint);

        gui_present(win);
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let win = match gui_create("File Manager", WIN_W_INIT, WIN_H_INIT) {
        Some(w) => w,
        None    => exit(1),
    };

    let mut app       = App::new(win);
    let mut size_tick = 0u32;

    loop {
        // Check for window resize every ~0.5 s
        size_tick += 1;
        if size_tick >= 30 { size_tick = 0; app.sync_size(); }

        let mut updated = false;
        loop {
            let Some(ev) = gui_poll_event(app.win) else { break };
            if app.handle_event(ev) { updated = true; }
        }

        if app.dirty || updated {
            app.draw();
            app.dirty = false;
        }

        sleep_ms(16); // ~60 fps
    }
}
