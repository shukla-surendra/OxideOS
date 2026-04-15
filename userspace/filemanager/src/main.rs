//! OxideOS File Manager
//!
//! A simple GUI file manager that uses the per-process GUI syscalls.
//! Fully responsive: layout recomputes from actual window content dimensions.
#![no_std]
#![no_main]

use oxide_rt::{
    exit, sleep_ms, readdir, chdir, getcwd,
    gui_create, gui_destroy, gui_fill_rect, gui_draw_text, gui_present,
    gui_poll_event, gui_get_size,
    GuiEvent, GuiWindow,
};

// ── Fixed constants (not window-size dependent) ───────────────────────────────

const WIN_W_INIT: u32 = 680;
const WIN_H_INIT: u32 = 440;

const CHAR_W:    u32 = 9;
const PAD:       u32 = 8;
const ROW_H:     u32 = 18;
const STATUS_H:  u32 = 20;
const TOOLBAR_H: u32 = 30;
const LEFT_W:    u32 = 200;
const DIV_W:     u32 = 2;

// ── Responsive layout ─────────────────────────────────────────────────────────

/// All layout values derived from the current window content size.
#[derive(Clone, Copy)]
struct Layout {
    w:        u32,
    h:        u32,
    right_x:  u32,  // LEFT_W + DIV_W
    right_y:  u32,  // TOOLBAR_H
    right_w:  u32,  // w - right_x
    status_y: u32,  // h - STATUS_H
    left_h:   u32,  // h - TOOLBAR_H - STATUS_H
}

impl Layout {
    fn from_win(win: GuiWindow) -> Self {
        let w = win.width.max(LEFT_W + DIV_W + 80);
        let h = win.height.max(TOOLBAR_H + STATUS_H + ROW_H);
        let right_x  = LEFT_W + DIV_W;
        let right_y  = TOOLBAR_H;
        let right_w  = w.saturating_sub(right_x);
        let status_y = h.saturating_sub(STATUS_H);
        let left_h   = h.saturating_sub(TOOLBAR_H + STATUS_H);
        Self { w, h, right_x, right_y, right_w, status_y, left_h }
    }

    fn visible_rows(&self) -> usize {
        let list_h = self.status_y.saturating_sub(self.right_y + 20);
        (list_h / ROW_H) as usize
    }
}

// ── Colour palette ────────────────────────────────────────────────────────────

const COL_BG:         u32 = 0xFF0D1117;
const COL_PANEL:      u32 = 0xFF161B22;
const COL_TOOLBAR_BG: u32 = 0xFF1C2128;
const COL_STATUS_BG:  u32 = 0xFF0D1117;
const COL_SELECTED:   u32 = 0xFF1A3A5C;
const COL_HOVER:      u32 = 0xFF1A2433;
const COL_BORDER:     u32 = 0xFF30363D;
const COL_TEXT_DIM:   u32 = 0xFF636E7B;
const COL_DIR:        u32 = 0xFF79C0FF;
const COL_FILE:       u32 = 0xFFCDD9E5;
const COL_ACCENT:     u32 = 0xFF2F81F7;
const COL_STATUS_TXT: u32 = 0xFF6E7681;

// ── Tiny heap-free fixed string ───────────────────────────────────────────────

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
}

// ── Directory entry ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct DirEntry {
    name:   FixStr<128>,
    is_dir: bool,
}

impl DirEntry {
    const fn empty() -> Self {
        Self { name: FixStr::new(), is_dir: false }
    }
}

// ── Application state ─────────────────────────────────────────────────────────

const MAX_ENTRIES: usize = 64;

struct App {
    win:         GuiWindow,
    cwd:         FixStr<256>,
    entries:     [DirEntry; MAX_ENTRIES],
    entry_count: usize,
    selected:    usize,
    scroll:      usize,
    hover:       Option<usize>,
    dirty:       bool,
}

impl App {
    fn new(win: GuiWindow) -> Self {
        let mut a = Self {
            win,
            cwd:         FixStr::new(),
            entries:     [DirEntry::empty(); MAX_ENTRIES],
            entry_count: 0,
            selected:    0,
            scroll:      0,
            hover:       None,
            dirty:       true,
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
            let s = core::str::from_utf8(&buf[..n as usize]).unwrap_or("/");
            self.cwd.push_str(s);
        } else {
            self.cwd.push(b'/');
        }
    }

    fn load_entries(&mut self) {
        self.entries     = [DirEntry::empty(); MAX_ENTRIES];
        self.entry_count = 0;
        self.selected    = 0;
        self.scroll      = 0;

        let mut buf = [0u8; 4096];
        let n = readdir(self.cwd.as_str(), &mut buf);
        if n <= 0 { return; }

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
        self.dirty = true;
    }

    fn navigate_to(&mut self, path: &str) {
        let r = chdir(path);
        if r >= 0 {
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

    /// Re-query window size. Returns true if size changed (needs redraw).
    fn sync_size(&mut self) -> bool {
        let (nw, nh) = gui_get_size(self.win);
        if nw != self.win.width || nh != self.win.height {
            self.win.width  = nw;
            self.win.height = nh;
            self.dirty = true;
            return true;
        }
        false
    }

    fn handle_event(&mut self, ev: GuiEvent) -> bool {
        if ev.is_close() { exit(0); }

        let lay = Layout::from_win(self.win);

        if let Some(key) = ev.as_key() {
            match key {
                0x1B => {}
                b'q' | 3 => exit(0),
                b'\n' | b'\r' => { self.enter_selected(); return true; }
                8 | 127 => { self.navigate_to(".."); return true; }
                _ => {}
            }
        }

        if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
            let bx = x as u32; let by = y as u32;
            if pressed && bx >= lay.right_x && bx < lay.w
                && by >= lay.right_y && by < lay.status_y
            {
                let row_y = by.saturating_sub(lay.right_y + 20);
                let idx   = self.scroll + (row_y / ROW_H) as usize;
                if idx < self.entry_count {
                    if self.selected == idx {
                        self.enter_selected();
                    } else {
                        self.selected = idx;
                        self.dirty = true;
                    }
                    return true;
                }
            }
            // Left pane click: navigate up
            if pressed && bx < LEFT_W
                && by >= TOOLBAR_H && by < TOOLBAR_H + lay.left_h
            {
                self.navigate_to("..");
                return true;
            }
            // Toolbar back button (x: PAD..PAD+60, y: 5..25)
            if pressed && bx >= PAD && bx < PAD + 60 && by >= 5 && by < 25 {
                self.navigate_to("..");
                return true;
            }
        }

        if let Some((x, y)) = ev.as_mouse_move() {
            let mx = x as u32; let my = y as u32;
            if mx >= lay.right_x && mx < lay.w
                && my >= lay.right_y + 20 && my < lay.status_y
            {
                let row_y = my.saturating_sub(lay.right_y + 20);
                let idx   = self.scroll + (row_y / ROW_H) as usize;
                let new_hover = if idx < self.entry_count { Some(idx) } else { None };
                if new_hover != self.hover {
                    self.hover = new_hover;
                    self.dirty = true;
                    return true;
                }
            } else if self.hover.is_some() {
                self.hover = None;
                self.dirty = true;
            }
        }

        false
    }

    fn draw(&self) {
        let win = self.win;
        let lay = Layout::from_win(win);

        // ── Background ────────────────────────────────────────────────────
        gui_fill_rect(win, 0, 0, lay.w, lay.h, COL_BG);

        // ── Toolbar ───────────────────────────────────────────────────────
        gui_fill_rect(win, 0, 0, lay.w, TOOLBAR_H, COL_TOOLBAR_BG);
        gui_fill_rect(win, 0, TOOLBAR_H - 1, lay.w, 1, COL_BORDER);

        // "<- Back" button
        gui_fill_rect(win, PAD, 5, 60, 20, 0xFF21262D);
        gui_draw_text(win, PAD + 6, 9, COL_TEXT_DIM, "<- Back");

        // Current path in toolbar
        let path_x = PAD + 70;
        let path_w = lay.w.saturating_sub(path_x + PAD);
        gui_fill_rect(win, path_x, 5, path_w, 20, 0xFF0D1117);
        gui_draw_text(win, path_x + 6, 9, COL_DIR, self.cwd.as_str());

        // ── Left pane ─────────────────────────────────────────────────────
        gui_fill_rect(win, 0, TOOLBAR_H, LEFT_W, lay.left_h, COL_PANEL);
        gui_fill_rect(win, 0, TOOLBAR_H, LEFT_W, 18, 0xFF161B22);
        gui_draw_text(win, PAD, TOOLBAR_H + 3, COL_TEXT_DIM, "DIRECTORIES");
        gui_fill_rect(win, 0, TOOLBAR_H + 18, LEFT_W, 1, COL_BORDER);

        let left_items: &[&str] = &["..", "/", "/bin", "/dev", "/disk"];
        for (i, &item) in left_items.iter().enumerate() {
            let iy = TOOLBAR_H + 20 + i as u32 * ROW_H;
            if iy + ROW_H > TOOLBAR_H + lay.left_h { break; }
            let txt_col = if item == ".." { COL_TEXT_DIM } else { COL_DIR };
            gui_draw_text(win, PAD, iy + 2, txt_col, item);
        }

        // ── Divider ───────────────────────────────────────────────────────
        gui_fill_rect(win, LEFT_W, TOOLBAR_H, DIV_W, lay.left_h, COL_BORDER);

        // ── Right pane header ─────────────────────────────────────────────
        gui_fill_rect(win, lay.right_x, lay.right_y, lay.right_w, 18, 0xFF1C2128);
        gui_draw_text(win, lay.right_x + PAD, lay.right_y + 3, COL_TEXT_DIM, "NAME");
        let type_col_x = lay.w.saturating_sub(60);
        gui_draw_text(win, type_col_x, lay.right_y + 3, COL_TEXT_DIM, "TYPE");
        gui_fill_rect(win, lay.right_x, lay.right_y + 18, lay.right_w, 1, COL_BORDER);

        // ── File list ─────────────────────────────────────────────────────
        let list_y0  = lay.right_y + 20;
        let vis      = lay.visible_rows();
        for row in 0..vis {
            let idx = self.scroll + row;
            let ry  = list_y0 + row as u32 * ROW_H;
            if ry + ROW_H > lay.status_y { break; }

            if idx >= self.entry_count {
                gui_fill_rect(win, lay.right_x, ry, lay.right_w, ROW_H, COL_PANEL);
                continue;
            }

            let e        = self.entries[idx];
            let is_sel   = idx == self.selected;
            let is_hover = self.hover == Some(idx);
            let row_bg   = if is_sel { COL_SELECTED }
                           else if is_hover { COL_HOVER }
                           else if row % 2 == 0 { COL_PANEL } else { 0xFF111720 };

            gui_fill_rect(win, lay.right_x, ry, lay.right_w, ROW_H, row_bg);

            if is_sel {
                gui_fill_rect(win, lay.right_x, ry, 3, ROW_H, COL_ACCENT);
            }

            let (icon, name_col, type_str) = if e.is_dir {
                ("[D]", COL_DIR, "DIR ")
            } else {
                ("[F]", COL_FILE, "FILE")
            };

            gui_draw_text(win, lay.right_x + PAD, ry + 2, COL_TEXT_DIM, icon);

            let name_str  = e.name.as_str();
            let max_chars = (lay.right_w.saturating_sub(80 + 28) / CHAR_W) as usize;
            let display   = if name_str.len() > max_chars {
                &name_str[..max_chars.saturating_sub(2)]
            } else {
                name_str
            };
            gui_draw_text(win, lay.right_x + PAD + 28, ry + 2, name_col, display);
            gui_draw_text(win, type_col_x, ry + 2, COL_TEXT_DIM, type_str);
        }

        // ── Status bar ────────────────────────────────────────────────────
        gui_fill_rect(win, 0, lay.status_y, lay.w, STATUS_H, COL_STATUS_BG);
        gui_fill_rect(win, 0, lay.status_y, lay.w, 1, COL_BORDER);

        let mut status_str = FixStr::<64>::new();
        fn push_num(s: &mut FixStr<64>, v: usize) {
            let mut tmp = [0u8; 20]; let mut i = tmp.len();
            let mut vv = v;
            if vv == 0 { i -= 1; tmp[i] = b'0'; }
            while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
            for &b in &tmp[i..] { s.push(b); }
        }
        push_num(&mut status_str, self.entry_count);
        for b in b" items | ".iter() { status_str.push(*b); }
        if self.selected < self.entry_count {
            status_str.push_str(self.entries[self.selected].name.as_str());
        }
        gui_draw_text(win, PAD, lay.status_y + 3, COL_STATUS_TXT, status_str.as_str());

        let hint = "Enter=open  Bksp=up  Q=quit";
        let hint_x = lay.w.saturating_sub((hint.len() as u32) * CHAR_W + PAD);
        gui_draw_text(win, hint_x, lay.status_y + 3, COL_TEXT_DIM, hint);

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

    let mut app = App::new(win);

    let mut esc_pending  = false;
    let mut size_tick: u32 = 0;

    loop {
        // Poll window size every ~30 frames (~0.5 s) to catch resizes
        size_tick += 1;
        if size_tick >= 30 {
            size_tick = 0;
            app.sync_size();
        }

        // Process all pending events
        let mut updated = false;
        loop {
            let Some(ev) = gui_poll_event(app.win) else { break; };
            if let Some(key) = ev.as_key() {
                if esc_pending {
                    esc_pending = false;
                    continue;
                }
                if key == 0x1B {
                    esc_pending = true;
                    continue;
                }
            } else {
                esc_pending = false;
            }
            if app.handle_event(ev) { updated = true; }
        }

        if app.dirty || updated {
            app.draw();
            app.dirty = false;
        }

        sleep_ms(16); // ~60 fps
    }
}
