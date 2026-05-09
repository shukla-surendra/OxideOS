//! Domain types used across the file manager.
//!
//! | Type          | Purpose                                              |
//! |---------------|------------------------------------------------------|
//! | `EscState`    | Parser state for ANSI/VT100 escape sequences         |
//! | `Layout`      | Pixel positions derived from the live window size    |
//! | `DirEntry`    | One entry returned by `readdir`                      |
//! | `SidebarItem` | A named quick-access path in the PLACES section      |
//! | `SidebarHit`  | Which sidebar element the pointer is currently over  |

use oxide_rt::GuiWindow;
use crate::constants::*;
use crate::fixstr::FixStr;

// ── Escape-sequence state machine ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum EscState { None, GotEsc, GotBracket }

// ── Responsive layout ─────────────────────────────────────────────────────────

/// Pixel positions for the current window size.
/// Call `from_win` every frame; use the result for both drawing and hit-testing.
#[derive(Clone, Copy)]
pub struct Layout {
    pub w:          u32,
    pub h:          u32,
    pub right_x:    u32, // x where the file pane starts (sidebar + divider)
    pub list_y0:    u32, // y of the first file-list row
    pub list_h:     u32, // pixel height of the file-row area
    pub status_y:   u32, // y of the status bar
    pub sidebar_h:  u32, // height of the sidebar background rect
    pub scroll_x:   u32, // x of the scrollbar's left edge
    pub col_size_x: u32,
    pub col_type_x: u32,
}

impl Layout {
    pub fn from_win(win: GuiWindow) -> Self {
        let w = win.width.max(SIDEBAR_W + DIV_W + 220);
        let h = win.height.max(TOOLBAR_H + HEADER_H + STATUS_H + ROW_H * 2);
        let right_x    = SIDEBAR_W + DIV_W;
        let status_y   = h.saturating_sub(STATUS_H);
        let sidebar_h  = status_y.saturating_sub(TOOLBAR_H);
        let scroll_x   = w.saturating_sub(SCROLL_W);
        let list_y0    = TOOLBAR_H + HEADER_H;
        let list_h     = status_y.saturating_sub(list_y0);
        let col_type_x = scroll_x.saturating_sub(COL_TYPE_W + PAD);
        let col_size_x = col_type_x.saturating_sub(COL_SIZE_W + PAD);
        Self { w, h, right_x, list_y0, list_h, status_y, sidebar_h,
               scroll_x, col_size_x, col_type_x }
    }

    pub fn visible_rows(&self) -> usize { (self.list_h / ROW_H) as usize }
}

// ── Directory entry ────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct DirEntry {
    pub name:      FixStr<128>,
    pub is_dir:    bool,
    pub size:      u64,
    pub have_size: bool,
}

impl DirEntry {
    pub const fn empty() -> Self {
        Self { name: FixStr::new(), is_dir: false, size: 0, have_size: false }
    }
}

// ── Sidebar items ─────────────────────────────────────────────────────────────

pub struct SidebarItem {
    pub label: &'static str,
    pub path:  &'static str,
}

pub const SIDEBAR_ITEMS: &[SidebarItem] = &[
    SidebarItem { label: "/ Root", path: "/" },
    SidebarItem { label: "/bin",   path: "/bin" },
    SidebarItem { label: "/dev",   path: "/dev" },
    SidebarItem { label: "/disk",  path: "/disk" },
    SidebarItem { label: "/tmp",   path: "/tmp" },
];

/// Which sidebar element the pointer is currently over.
#[derive(Clone, Copy, PartialEq)]
pub enum SidebarHit {
    /// A PLACES shortcut — index into `SIDEBAR_ITEMS`.
    Place(usize),
    /// A PATH tree segment — index into `App::path_segs`.
    Seg(usize),
}
