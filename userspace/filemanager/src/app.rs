//! Application state — `App` struct, navigation, input handling, and scroll logic.
//!
//! Drawing is in `render.rs` (a separate `impl App` block).
//! Each method group has a section comment so the file stays easy to scan.

use oxide_rt::{exit, chdir, getcwd, readdir, stat, FileStat, gui_get_size, GuiEvent, GuiWindow};
use crate::constants::*;
use crate::fixstr::FixStr;
use crate::types::{EscState, Layout, DirEntry, SidebarHit, SIDEBAR_ITEMS};

pub const MAX_ENTRIES: usize = 128;
pub const MAX_SEGS:    usize = 14;

// ── App struct ────────────────────────────────────────────────────────────────

pub struct App {
    pub(crate) win:            GuiWindow,
    pub(crate) cwd:            FixStr<256>,
    /// PATH tree: element 0 is always "/", subsequent elements are path components.
    pub(crate) path_segs:      [FixStr<64>; MAX_SEGS],
    pub(crate) path_seg_count: usize,
    pub(crate) entries:        [DirEntry; MAX_ENTRIES],
    pub(crate) entry_count:    usize,
    pub(crate) selected:       usize,
    pub(crate) scroll:         usize,
    pub(crate) hover:          Option<usize>,
    pub(crate) sidebar_hover:  Option<SidebarHit>,
    pub(crate) dirty:          bool,
    pub(crate) esc:            EscState,
}

impl App {
    pub fn new(win: GuiWindow) -> Self {
        let mut a = Self {
            win,
            cwd:            FixStr::new(),
            path_segs:      [FixStr::new(); MAX_SEGS],
            path_seg_count: 0,
            entries:        [DirEntry::empty(); MAX_ENTRIES],
            entry_count:    0,
            selected:       0,
            scroll:         0,
            hover:          None,
            sidebar_hover:  None,
            dirty:          true,
            esc:            EscState::None,
        };
        a.refresh_cwd();
        a.load_entries();
        a
    }
}

// ── Navigation ────────────────────────────────────────────────────────────────

impl App {
    pub(crate) fn refresh_cwd(&mut self) {
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
        self.build_path_segs();
    }

    /// Split `self.cwd` into named segments for the PATH sidebar tree.
    /// Segment 0 is always `"/"` (root); subsequent segments are directory names.
    pub(crate) fn build_path_segs(&mut self) {
        for s in self.path_segs.iter_mut() { s.clear(); }
        self.path_seg_count = 0;

        self.path_segs[0].push(b'/');
        self.path_seg_count = 1;

        let cwd   = self.cwd.as_str();
        if cwd == "/" { return; }

        let bytes = cwd.as_bytes();
        let mut seg_start = 1usize; // skip leading '/'
        let mut i = 1usize;
        while i <= bytes.len() {
            let at_sep = i == bytes.len() || bytes[i] == b'/';
            if at_sep && i > seg_start {
                if self.path_seg_count < MAX_SEGS {
                    if let Ok(s) = core::str::from_utf8(&bytes[seg_start..i]) {
                        self.path_segs[self.path_seg_count].push_str(s);
                        self.path_seg_count += 1;
                    }
                }
                seg_start = i + 1;
            }
            i += 1;
        }
    }

    /// Build the full absolute path to path segment `idx`
    /// (0 = root `/`, 1 = `/first`, 2 = `/first/second`, …).
    pub(crate) fn seg_path(&self, idx: usize) -> FixStr<256> {
        let mut p = FixStr::<256>::new();
        if idx == 0 {
            p.push(b'/');
        } else {
            for i in 1..=idx {
                p.push(b'/');
                p.push_str(self.path_segs[i].as_str());
            }
        }
        p
    }

    /// Read the current directory and populate `self.entries`.
    pub(crate) fn load_entries(&mut self) {
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

        // Stat regular files to get their sizes.
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

    /// `chdir` to `path` and reload the entry list.
    pub fn navigate_to(&mut self, path: &str) {
        if chdir(path) >= 0 {
            self.refresh_cwd();
            self.load_entries();
        }
    }

    /// Enter the currently selected directory (no-op for files).
    pub(crate) fn enter_selected(&mut self) {
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
}

// ── Scroll & selection ────────────────────────────────────────────────────────

impl App {
    pub(crate) fn clamp_scroll(&mut self) {
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

    pub(crate) fn sel_up(&mut self) {
        if self.selected > 0 { self.selected -= 1; }
        self.clamp_scroll(); self.dirty = true;
    }

    pub(crate) fn sel_down(&mut self) {
        if self.entry_count > 0 && self.selected + 1 < self.entry_count {
            self.selected += 1;
        }
        self.clamp_scroll(); self.dirty = true;
    }
}

// ── Window resize ─────────────────────────────────────────────────────────────

impl App {
    /// Check if the window has been resized and mark the frame dirty if so.
    pub fn sync_size(&mut self) {
        let (nw, nh) = gui_get_size(self.win);
        if nw != self.win.width || nh != self.win.height {
            self.win.width  = nw;
            self.win.height = nh;
            self.dirty = true;
        }
    }
}

// ── Event handling ────────────────────────────────────────────────────────────

impl App {
    /// Dispatch a single GUI event (keyboard, mouse, or close).
    pub fn handle_event(&mut self, ev: GuiEvent) {
        if ev.is_close() { exit(0); }
        let lay = Layout::from_win(self.win);

        // ── Keyboard ──────────────────────────────────────────────────────
        if let Some(key) = ev.as_key() {
            match self.esc {
                EscState::GotBracket => {
                    self.esc = EscState::None;
                    match key {
                        b'A' => self.sel_up(),
                        b'B' => self.sel_down(),
                        b'H' | b'1' => {
                            self.selected = 0; self.scroll = 0; self.dirty = true;
                        }
                        b'F' | b'4' => {
                            if self.entry_count > 0 {
                                self.selected = self.entry_count - 1;
                                self.clamp_scroll(); self.dirty = true;
                            }
                        }
                        b'5' => { // Page Up
                            let vis = lay.visible_rows();
                            self.selected = self.selected.saturating_sub(vis);
                            self.clamp_scroll(); self.dirty = true;
                        }
                        b'6' => { // Page Down
                            let vis = lay.visible_rows();
                            if self.entry_count > 0 {
                                self.selected = (self.selected + vis).min(self.entry_count - 1);
                                self.clamp_scroll(); self.dirty = true;
                            }
                        }
                        _ => {}
                    }
                }
                EscState::GotEsc => {
                    self.esc = if key == b'[' { EscState::GotBracket } else { EscState::None };
                }
                EscState::None => {
                    match key {
                        0x1B        => self.esc = EscState::GotEsc,
                        b'q' | 3    => exit(0),
                        b'\n'|b'\r' => self.enter_selected(),
                        8 | 127     => self.navigate_to(".."),
                        b'j'        => self.sel_down(),
                        b'k'        => self.sel_up(),
                        b'r'|b'R'   => self.load_entries(),
                        b'g'        => { self.selected = 0; self.scroll = 0; self.dirty = true; }
                        b'G'        => {
                            if self.entry_count > 0 {
                                self.selected = self.entry_count - 1;
                                self.clamp_scroll(); self.dirty = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            return;
        }

        // ── Mouse click ───────────────────────────────────────────────────
        if let Some((x, y, _btn, pressed)) = ev.as_mouse_btn() {
            if !pressed { return; }
            let bx = x as u32; let by = y as u32;

            // File list
            if bx >= lay.right_x && bx < lay.scroll_x
                && by >= lay.list_y0 && by < lay.status_y
            {
                let idx = self.scroll + ((by - lay.list_y0) / ROW_H) as usize;
                if idx < self.entry_count {
                    if self.selected == idx { self.enter_selected(); }
                    else { self.selected = idx; self.dirty = true; }
                }
                return;
            }

            // Sidebar: PLACES shortcuts
            if bx < SIDEBAR_W && by >= PLACES_ITEMS_Y && by < PATH_SEC_Y {
                let i = ((by - PLACES_ITEMS_Y) / SIDEBAR_ITEM_H) as usize;
                if i < SIDEBAR_ITEMS.len() { self.navigate_to(SIDEBAR_ITEMS[i].path); }
                return;
            }

            // Sidebar: PATH tree segments (click navigates to that ancestor)
            if bx < SIDEBAR_W && by >= PATH_ITEMS_Y {
                let i = ((by - PATH_ITEMS_Y) / SIDEBAR_ITEM_H) as usize;
                if i < self.path_seg_count {
                    let p = self.seg_path(i);
                    self.navigate_to(p.as_str());
                }
                return;
            }

            // Back button in toolbar
            if bx >= PAD && bx < PAD + 56 && by >= 6 && by < 26 {
                self.navigate_to("..");
            }
        }

        // ── Mouse move — hover highlights ─────────────────────────────────
        if let Some((x, y)) = ev.as_mouse_move() {
            let mx = x as u32; let my = y as u32;
            let mut changed = false;

            let new_hover = if mx >= lay.right_x && mx < lay.scroll_x
                && my >= lay.list_y0 && my < lay.status_y
            {
                let idx = self.scroll + ((my - lay.list_y0) / ROW_H) as usize;
                if idx < self.entry_count { Some(idx) } else { None }
            } else { None };
            if new_hover != self.hover { self.hover = new_hover; changed = true; }

            let new_sh = if mx < SIDEBAR_W {
                if my >= PLACES_ITEMS_Y && my < PATH_SEC_Y {
                    let i = ((my - PLACES_ITEMS_Y) / SIDEBAR_ITEM_H) as usize;
                    if i < SIDEBAR_ITEMS.len() { Some(SidebarHit::Place(i)) } else { None }
                } else if my >= PATH_ITEMS_Y {
                    let i = ((my - PATH_ITEMS_Y) / SIDEBAR_ITEM_H) as usize;
                    if i < self.path_seg_count { Some(SidebarHit::Seg(i)) } else { None }
                } else { None }
            } else { None };
            if new_sh != self.sidebar_hover { self.sidebar_hover = new_sh; changed = true; }

            if changed { self.dirty = true; }
        }
    }
}
