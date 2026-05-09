//! Rendering — the single `draw` method for `App`.
//!
//! Draws the entire file manager UI in one pass: toolbar → sidebar → file list
//! → scrollbar → status bar, then calls `gui_present` to flip the frame.
//!
//! Kept in a separate file from `app.rs` so navigation/event logic and
//! presentation code never mix.  Rust allows splitting `impl App` across files
//! within the same crate.

use oxide_rt::{gui_fill_rect, gui_draw_text, gui_present};
use crate::app::App;
use crate::constants::*;
use crate::fixstr::FixStr;
use crate::types::{Layout, SidebarHit, SIDEBAR_ITEMS};

impl App {
    pub fn draw(&self) {
        let win = self.win;
        let lay = Layout::from_win(win);

        // Clear background
        gui_fill_rect(win, 0, 0, lay.w, lay.h, COL_BG);

        draw_toolbar(self, win, &lay);
        draw_sidebar(self, win, &lay);
        draw_column_headers(win, &lay);
        draw_file_list(self, win, &lay);
        draw_scrollbar(self, win, &lay);
        draw_status_bar(self, win, &lay);

        gui_present(win);
    }
}

// ── Toolbar ───────────────────────────────────────────────────────────────────

fn draw_toolbar(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    gui_fill_rect(win, 0, 0, lay.w, TOOLBAR_H, COL_TOOLBAR_BG);
    gui_fill_rect(win, 0, TOOLBAR_H - 1, lay.w, 1, COL_DIVIDER);

    // Back button
    gui_fill_rect(win, PAD, 6, 56, 20, COL_BTN_BG);
    gui_fill_rect(win, PAD, 6, 56, 1, 0xFF5A5A5A);
    gui_fill_rect(win, PAD, 25, 56, 1, 0xFF333333);
    gui_draw_text(win, PAD + 7, 10, COL_TEXT, "<- Back");

    // Path bar
    let path_x = PAD + 64;
    let path_w = lay.w.saturating_sub(path_x + PAD);
    gui_fill_rect(win, path_x, 6, path_w, 20, 0xFF2D2D30);
    gui_fill_rect(win, path_x, 6, 1, 20, COL_DIVIDER);
    gui_fill_rect(win, path_x + path_w - 1, 6, 1, 20, COL_DIVIDER);
    gui_draw_text(win, path_x + 8, 10, COL_DIR, app.cwd.as_str());
}

// ── Sidebar ───────────────────────────────────────────────────────────────────

fn draw_sidebar(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    // Background
    gui_fill_rect(win, 0, TOOLBAR_H, SIDEBAR_W, lay.sidebar_h, COL_SIDEBAR_BG);

    // "EXPLORER" master header
    gui_fill_rect(win, 0, TOOLBAR_H, SIDEBAR_W, SIDEBAR_MAIN_H, 0xFF1E1E1E);
    gui_draw_text(win, PAD, TOOLBAR_H + 5, COL_TEXT_DIM, "EXPLORER");
    gui_fill_rect(win, 0, TOOLBAR_H + SIDEBAR_MAIN_H - 1, SIDEBAR_W, 1, COL_DIVIDER);

    draw_places_section(app, win, lay);
    draw_path_section(app, win, lay);

    // Sidebar / file-pane vertical divider
    gui_fill_rect(win, SIDEBAR_W, TOOLBAR_H, DIV_W, lay.sidebar_h, COL_DIVIDER);
}

fn draw_places_section(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    // Section header
    gui_fill_rect(win, 0, PLACES_SEC_Y, SIDEBAR_W, SIDEBAR_SEC_H, COL_SIDEBAR_SEC);
    gui_draw_text(win, PAD, PLACES_SEC_Y + 3, COL_SEC_TXT, "PLACES");
    gui_fill_rect(win, 0, PLACES_SEC_Y + SIDEBAR_SEC_H - 1, SIDEBAR_W, 1, COL_DIVIDER);

    for (i, item) in SIDEBAR_ITEMS.iter().enumerate() {
        let iy = PLACES_ITEMS_Y + i as u32 * SIDEBAR_ITEM_H;
        if iy + SIDEBAR_ITEM_H > lay.status_y { break; }

        let is_cur = app.cwd.as_str() == item.path;
        let is_hov = app.sidebar_hover == Some(SidebarHit::Place(i));
        let bg = if is_cur { COL_SIDEBAR_CUR } else if is_hov { COL_SIDEBAR_HOV } else { COL_SIDEBAR_BG };

        gui_fill_rect(win, 0, iy, SIDEBAR_W, SIDEBAR_ITEM_H, bg);
        if is_cur { gui_fill_rect(win, 0, iy, 2, SIDEBAR_ITEM_H, COL_ACCENT); }
        gui_draw_text(win, PAD + 8, iy + 4, if is_cur { COL_DIR } else { COL_TEXT }, item.label);
    }
}

fn draw_path_section(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    if PATH_SEC_Y + SIDEBAR_SEC_H > lay.status_y { return; }

    // Section header
    gui_fill_rect(win, 0, PATH_SEC_Y, SIDEBAR_W, SIDEBAR_SEC_H, COL_SIDEBAR_SEC);
    gui_draw_text(win, PAD, PATH_SEC_Y + 3, COL_SEC_TXT, "PATH");
    gui_fill_rect(win, 0, PATH_SEC_Y + SIDEBAR_SEC_H - 1, SIDEBAR_W, 1, COL_DIVIDER);

    let leaf = app.path_seg_count.saturating_sub(1);
    for i in 0..app.path_seg_count {
        let iy = PATH_ITEMS_Y + i as u32 * SIDEBAR_ITEM_H;
        if iy + SIDEBAR_ITEM_H > lay.status_y { break; }

        let is_leaf = i == leaf;
        let is_hov  = app.sidebar_hover == Some(SidebarHit::Seg(i));

        gui_fill_rect(win, 0, iy, SIDEBAR_W, SIDEBAR_ITEM_H,
                      if is_hov { COL_SIDEBAR_HOV } else { COL_SIDEBAR_BG });

        // Left accent bar marks the current (leaf) directory
        if is_leaf { gui_fill_rect(win, 0, iy, 2, SIDEBAR_ITEM_H, COL_ACCENT); }

        // Connector lines: vertical arm from parent row, horizontal arm to label
        if i > 0 {
            let lx = PAD + (i as u32 - 1) * SIDEBAR_INDENT + 4;
            gui_fill_rect(win, lx, iy, 1, SIDEBAR_ITEM_H / 2 + 1, COL_DIVIDER);
            gui_fill_rect(win, lx, iy + SIDEBAR_ITEM_H / 2, SIDEBAR_INDENT - 2, 1, COL_DIVIDER);
        }

        let indent  = PAD + i as u32 * SIDEBAR_INDENT;
        let chev    = if is_leaf { ">" } else { "v" };
        let chev_col = if is_leaf { COL_ACCENT } else { COL_TEXT_DIM };
        gui_draw_text(win, indent, iy + 4, chev_col, chev);
        gui_draw_text(win, indent + CHAR_W + 2, iy + 4,
                      if is_leaf { COL_PATH_LEAF } else { COL_PATH_ANC },
                      app.path_segs[i].as_str());
    }
}

// ── Column headers ────────────────────────────────────────────────────────────

fn draw_column_headers(win: oxide_rt::GuiWindow, lay: &Layout) {
    gui_fill_rect(win, lay.right_x, TOOLBAR_H, lay.w - lay.right_x, HEADER_H, COL_HEADER_BG);
    gui_fill_rect(win, lay.right_x, TOOLBAR_H + HEADER_H - 1, lay.w - lay.right_x, 1, COL_DIVIDER);
    gui_draw_text(win, lay.right_x + PAD + 28, TOOLBAR_H + 3, COL_TEXT_DIM, "NAME");
    gui_draw_text(win, lay.col_size_x, TOOLBAR_H + 3, COL_TEXT_DIM, "SIZE");
    gui_draw_text(win, lay.col_type_x, TOOLBAR_H + 3, COL_TEXT_DIM, "TYPE");
    gui_fill_rect(win, lay.col_size_x - 4, TOOLBAR_H, 1, HEADER_H, COL_DIVIDER);
    gui_fill_rect(win, lay.col_type_x - 4, TOOLBAR_H, 1, HEADER_H, COL_DIVIDER);
}

// ── File list ─────────────────────────────────────────────────────────────────

fn draw_file_list(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    let vis    = lay.visible_rows();
    let name_w = lay.col_size_x.saturating_sub(lay.right_x + PAD + 28 + 4);

    for row in 0..vis {
        let idx = app.scroll + row;
        let ry  = lay.list_y0 + row as u32 * ROW_H;
        if ry + ROW_H > lay.status_y { break; }

        // Empty rows get alternating background
        if idx >= app.entry_count {
            if row % 2 != 0 {
                gui_fill_rect(win, lay.right_x, ry, lay.scroll_x - lay.right_x, ROW_H, COL_ROW_ODD);
            }
            continue;
        }

        let e      = app.entries[idx];
        let is_sel = idx == app.selected;
        let is_hov = app.hover == Some(idx);
        let row_bg = if is_sel { COL_SELECTED }
                     else if is_hov { COL_HOVER }
                     else if row % 2 != 0 { COL_ROW_ODD }
                     else { COL_BG };

        gui_fill_rect(win, lay.right_x, ry, lay.scroll_x - lay.right_x, ROW_H, row_bg);
        if is_sel { gui_fill_rect(win, lay.right_x, ry, 2, ROW_H, COL_ACCENT); }

        // Icon
        let (icon, icon_col) = if e.is_dir {
            if e.name.as_str() == ".." { ("..", COL_TEXT_DIM) } else { ("dir", COL_DIR) }
        } else { ("   ", COL_TEXT_DIM) };
        gui_draw_text(win, lay.right_x + PAD, ry + 2, icon_col, icon);

        // Name (truncated with ".." if too long)
        let name_str  = e.name.as_str();
        let max_chars = (name_w / CHAR_W) as usize;
        let (display, truncated) = if name_str.len() > max_chars && max_chars > 3 {
            (&name_str[..max_chars.saturating_sub(2)], true)
        } else { (name_str, false) };
        gui_draw_text(win, lay.right_x + PAD + 28, ry + 2,
                      if e.is_dir { COL_DIR } else { COL_FILE }, display);
        if truncated {
            let tx = lay.right_x + PAD + 28 + display.len() as u32 * CHAR_W;
            gui_draw_text(win, tx, ry + 2, COL_TEXT_DIM, "..");
        }

        // Size (files only)
        if !e.is_dir && e.have_size {
            let mut sz = FixStr::<16>::new();
            sz.push_size(e.size);
            gui_draw_text(win, lay.col_size_x, ry + 2, COL_SIZE_FG, sz.as_str());
        }

        // Type badge
        let (type_str, type_col) = if e.is_dir { ("DIR", COL_DIR) } else { ("FILE", COL_TEXT_DIM) };
        gui_draw_text(win, lay.col_type_x, ry + 2, type_col, type_str);
    }
}

// ── Scrollbar ─────────────────────────────────────────────────────────────────

fn draw_scrollbar(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    gui_fill_rect(win, lay.scroll_x, lay.list_y0, SCROLL_W, lay.list_h, COL_SCROLL_TRACK);
    let vis = lay.visible_rows();
    if vis > 0 && app.entry_count > vis {
        let total   = app.entry_count as u32;
        let vis_u   = vis as u32;
        let thumb_h = (lay.list_h * vis_u / total).max(16).min(lay.list_h);
        let max_sc  = total.saturating_sub(vis_u);
        let thumb_y = if max_sc > 0 {
            (lay.list_h - thumb_h) * app.scroll as u32 / max_sc
        } else { 0 };
        gui_fill_rect(win, lay.scroll_x + 2, lay.list_y0 + thumb_y, SCROLL_W - 4, thumb_h, COL_SCROLL_THUMB);
    }
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status_bar(app: &App, win: oxide_rt::GuiWindow, lay: &Layout) {
    gui_fill_rect(win, 0, lay.status_y, lay.w, STATUS_H, COL_STATUS_BG);

    let mut st = FixStr::<128>::new();
    st.push_str("  ");
    st.push_usize(app.entry_count);
    st.push_str(" items");
    if app.selected < app.entry_count {
        let e = &app.entries[app.selected];
        st.push_str("  |  ");
        st.push_str(e.name.as_str());
        if !e.is_dir && e.have_size {
            st.push_str("  ("); st.push_size(e.size); st.push(b')');
        }
    }
    gui_draw_text(win, 0, lay.status_y + 3, COL_STATUS_TXT, st.as_str());

    let hint   = "  jk/arrows  enter  bksp=up  r=refresh  q=quit  ";
    let hint_x = lay.w.saturating_sub(hint.len() as u32 * CHAR_W);
    gui_draw_text(win, hint_x, lay.status_y + 3, 0xFFD0E8F8, hint);
}
