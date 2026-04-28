// src/gui/menu.rs — MenuBar and dropdown widget for OxideOS kernel GUI.
//
// Layout (absolute screen coordinates):
//   MenuBar is drawn at (bar_x, bar_y) with width bar_w and height MENUBAR_H.
//   Dropdowns appear immediately below the bar, anchored at the left of the
//   clicked tab.
//
// Usage:
//   1. Build menus with Menu::new() + .add(MenuItem::item(...) / MenuItem::sep())
//   2. Call menubar.add_menu(menu) for each top-level menu
//   3. Call menubar.layout(bar_x) once (or whenever bar_x changes)
//   4. In the draw loop: menubar.draw(graphics, bar_x, bar_y, bar_w)
//   5. On mouse move: menubar.handle_mouse_move(mx, my, bar_x, bar_y, bar_w)
//   6. On mouse click: let action = menubar.handle_click(mx, my, bar_x, bar_y, bar_w)

use crate::gui::fonts;
use crate::gui::graphics::Graphics;

// ── Constants ──────────────────────────────────────────────────────────────────

pub const MENUBAR_H: u64 = 22;

const MAX_MENUS: usize = 8;
const MAX_ITEMS: usize = 12;

const ITEM_H:        u64 = 20;
const SEP_H:         u64 = 9;
const DROPDOWN_W:    u64 = 195;
const CHAR_W:        u64 = 9;
const TAB_PAD_X:     u64 = 12;
const DROPDOWN_PAD_Y: u64 = 4;
const ITEM_PAD_X:    u64 = 20;

// Color palette
const BAR_BG:      u32 = 0xFF2D2D30;
const BAR_BORDER:  u32 = 0xFF3F3F46;
const TAB_HOT_BG:  u32 = 0xFF3E3E42;
const TAB_OPEN_BG: u32 = 0xFF1C1C1E;
const TAB_TEXT:    u32 = 0xFFCCCCCC;
const DROP_BG:     u32 = 0xFF1E1E1E;
const DROP_BORDER: u32 = 0xFF555555;
const DROP_SHADOW: u32 = 0x88000000;
const ITEM_HOT_BG: u32 = 0xFF0E639C;
const ITEM_TEXT:   u32 = 0xFFD4D4D4;
const ITEM_DIM:    u32 = 0xFF777777;
const SEP_COL:     u32 = 0xFF3F3F46;
const CHECK_COL:   u32 = 0xFF89D185;

// ── MenuAction ─────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, PartialEq)]
pub enum MenuAction {
    None,
    // File
    FileNew,
    FileSave,
    FileSaveAs,
    FileExit,
    // Edit
    EditUndo,
    EditRedo,
    EditSelectAll,
    // Format
    FormatWordWrap,
    // View
    ViewStatusBar,
    // Help
    HelpAbout,
}

// ── MenuItem ───────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct MenuItem {
    pub label:    &'static str,
    pub shortcut: &'static str,
    pub action:   MenuAction,
    pub is_sep:   bool,
    pub checked:  bool,
    pub enabled:  bool,
}

impl MenuItem {
    pub const fn item(label: &'static str, shortcut: &'static str, action: MenuAction) -> Self {
        Self { label, shortcut, action, is_sep: false, checked: false, enabled: true }
    }
    pub const fn sep() -> Self {
        Self { label: "", shortcut: "", action: MenuAction::None,
               is_sep: true, checked: false, enabled: false }
    }
    pub const fn disabled(label: &'static str, shortcut: &'static str, action: MenuAction) -> Self {
        Self { label, shortcut, action, is_sep: false, checked: false, enabled: false }
    }
    pub const fn checked_item(label: &'static str, shortcut: &'static str, action: MenuAction, checked: bool) -> Self {
        Self { label, shortcut, action, is_sep: false, checked, enabled: true }
    }
}

// ── Menu ───────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct Menu {
    pub label:      &'static str,
    items:          [MenuItem; MAX_ITEMS],
    pub item_count: usize,
    // Computed by MenuBar::layout()
    pub tab_x:      u64,
    pub tab_w:      u64,
}

impl Menu {
    pub const fn new(label: &'static str) -> Self {
        const BLANK_ITEM: MenuItem = MenuItem::sep();
        Self {
            label,
            items: [BLANK_ITEM; MAX_ITEMS],
            item_count: 0,
            tab_x: 0,
            tab_w: 0,
        }
    }

    pub fn add(&mut self, item: MenuItem) {
        if self.item_count < MAX_ITEMS {
            self.items[self.item_count] = item;
            self.item_count += 1;
        }
    }

    pub fn item_at(&self, idx: usize) -> Option<&MenuItem> {
        if idx < self.item_count { Some(&self.items[idx]) } else { None }
    }

    pub fn item_at_mut(&mut self, idx: usize) -> Option<&mut MenuItem> {
        if idx < self.item_count { Some(&mut self.items[idx]) } else { None }
    }

    /// Total height of the dropdown including padding.
    pub fn dropdown_height(&self) -> u64 {
        let mut h = DROPDOWN_PAD_Y * 2;
        for i in 0..self.item_count {
            h += if self.items[i].is_sep { SEP_H } else { ITEM_H };
        }
        h
    }

    /// Item index at relative-y inside the dropdown (from dropdown top, not bar).
    pub fn item_at_rel_y(&self, y: u64) -> Option<usize> {
        let mut cur = DROPDOWN_PAD_Y;
        for i in 0..self.item_count {
            let h = if self.items[i].is_sep { SEP_H } else { ITEM_H };
            if !self.items[i].is_sep && y >= cur && y < cur + h {
                return Some(i);
            }
            cur += h;
        }
        None
    }
}

// ── MenuBar ────────────────────────────────────────────────────────────────────

pub struct MenuBar {
    pub menus:      [Menu; MAX_MENUS],
    pub menu_count: usize,
    pub open_menu:  Option<usize>,
    pub hov_menu:   Option<usize>,
    pub hov_item:   Option<usize>,
}

impl MenuBar {
    pub const fn new() -> Self {
        const BLANK_MENU: Menu = Menu::new("");
        Self {
            menus:      [BLANK_MENU; MAX_MENUS],
            menu_count: 0,
            open_menu:  None,
            hov_menu:   None,
            hov_item:   None,
        }
    }

    /// Append a top-level menu to the bar.
    pub fn add_menu(&mut self, menu: Menu) {
        if self.menu_count < MAX_MENUS {
            self.menus[self.menu_count] = menu;
            self.menu_count += 1;
        }
    }

    /// Update checked state for a specific item.
    pub fn set_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if menu_idx < self.menu_count {
            if let Some(item) = self.menus[menu_idx].item_at_mut(item_idx) {
                item.checked = checked;
            }
        }
    }

    /// Compute and cache tab x/w positions.  Call whenever bar_x changes.
    pub fn layout(&mut self, bar_x: u64) {
        let mut x = bar_x + 4;
        for i in 0..self.menu_count {
            let w = self.menus[i].label.len() as u64 * CHAR_W + TAB_PAD_X * 2;
            self.menus[i].tab_x = x;
            self.menus[i].tab_w = w;
            x += w;
        }
    }

    /// Draw the bar and, if a menu is open, its dropdown.
    pub fn draw(&self, graphics: &Graphics, bar_x: u64, bar_y: u64, bar_w: u64) {
        // ── Bar background ────────────────────────────────────────────────────
        graphics.fill_rect(bar_x, bar_y, bar_w, MENUBAR_H, BAR_BG);
        graphics.fill_rect(bar_x, bar_y + MENUBAR_H - 1, bar_w, 1, BAR_BORDER);

        // ── Tabs ──────────────────────────────────────────────────────────────
        for i in 0..self.menu_count {
            let menu = &self.menus[i];
            let is_open = self.open_menu == Some(i);
            let is_hot  = self.hov_menu  == Some(i);

            let bg = if is_open { TAB_OPEN_BG } else if is_hot { TAB_HOT_BG } else { BAR_BG };
            if bg != BAR_BG {
                graphics.fill_rect(menu.tab_x, bar_y, menu.tab_w, MENUBAR_H - 1, bg);
            }

            let text_x = menu.tab_x + TAB_PAD_X;
            let text_y = bar_y + (MENUBAR_H - 8) / 2;
            fonts::draw_string(graphics, text_x, text_y, menu.label, TAB_TEXT);

            if is_open {
                self.draw_dropdown(graphics, i, menu.tab_x, bar_y + MENUBAR_H);
            }
        }
    }

    fn draw_dropdown(&self, graphics: &Graphics, menu_idx: usize, drop_x: u64, drop_y: u64) {
        let menu = &self.menus[menu_idx];
        let h = menu.dropdown_height();
        let w = DROPDOWN_W;

        // Shadow
        graphics.fill_rect(drop_x + 3, drop_y + 3, w, h, DROP_SHADOW);

        // Panel
        graphics.fill_rect(drop_x, drop_y, w, h, DROP_BG);
        graphics.draw_rect(drop_x, drop_y, w, h, DROP_BORDER, 1);

        let mut cur_y = drop_y + DROPDOWN_PAD_Y;
        for i in 0..menu.item_count {
            let item = &menu.items[i];

            if item.is_sep {
                let sy = cur_y + SEP_H / 2;
                graphics.fill_rect(drop_x + 8, sy, w - 16, 1, SEP_COL);
                cur_y += SEP_H;
                continue;
            }

            // Hover highlight
            if self.hov_item == Some(i) {
                graphics.fill_rect(drop_x + 1, cur_y, w - 2, ITEM_H, ITEM_HOT_BG);
            }

            // Checkmark column
            if item.checked {
                // Draw a simple check mark: "v" styled
                fonts::draw_string(graphics, drop_x + 4, cur_y + (ITEM_H - 8) / 2, "*", CHECK_COL);
            }

            let text_col = if item.enabled { ITEM_TEXT } else { ITEM_DIM };
            fonts::draw_string(graphics, drop_x + ITEM_PAD_X, cur_y + (ITEM_H - 8) / 2, item.label, text_col);

            // Shortcut hint (right-aligned)
            if !item.shortcut.is_empty() {
                let sc_w   = item.shortcut.len() as u64 * CHAR_W;
                let sc_x   = drop_x + w.saturating_sub(sc_w + 10);
                fonts::draw_string(graphics, sc_x, cur_y + (ITEM_H - 8) / 2, item.shortcut, ITEM_DIM);
            }

            cur_y += ITEM_H;
        }
    }

    // ── Mouse handling ─────────────────────────────────────────────────────────

    /// Update hover state.  Returns true if the bar/dropdown consumed the move
    /// (caller should trigger a redraw).
    pub fn handle_mouse_move(&mut self, mx: u64, my: u64, bar_x: u64, bar_y: u64, bar_w: u64) -> bool {
        // Inside the bar itself
        if my >= bar_y && my < bar_y + MENUBAR_H && mx >= bar_x && mx < bar_x + bar_w {
            self.hov_item = None;
            for i in 0..self.menu_count {
                let m = &self.menus[i];
                if mx >= m.tab_x && mx < m.tab_x + m.tab_w {
                    let new_hov = Some(i);
                    let changed = self.hov_menu != new_hov;
                    self.hov_menu = new_hov;
                    // If a different menu is already open, auto-switch to this one
                    if self.open_menu.is_some() && self.open_menu != new_hov {
                        self.open_menu = new_hov;
                    }
                    return changed || self.open_menu.is_some();
                }
            }
            let changed = self.hov_menu.is_some();
            self.hov_menu = None;
            return changed;
        }

        // Inside an open dropdown
        if let Some(mi) = self.open_menu {
            let menu = &self.menus[mi];
            let drop_x = menu.tab_x;
            let drop_y = bar_y + MENUBAR_H;
            let drop_h = menu.dropdown_height();

            if mx >= drop_x && mx < drop_x + DROPDOWN_W
                && my >= drop_y && my < drop_y + drop_h
            {
                let rel_y = my - drop_y;
                let new_hov = menu.item_at_rel_y(rel_y);
                let changed = self.hov_item != new_hov;
                self.hov_item = new_hov;
                return changed;
            }
        }

        false
    }

    /// Handle a left-click.  Returns the triggered `MenuAction` (may be `None`).
    /// Also closes the open menu when clicking outside.
    pub fn handle_click(&mut self, mx: u64, my: u64, bar_x: u64, bar_y: u64, bar_w: u64) -> MenuAction {
        // Click on a tab?
        if my >= bar_y && my < bar_y + MENUBAR_H && mx >= bar_x && mx < bar_x + bar_w {
            for i in 0..self.menu_count {
                let m = &self.menus[i];
                if mx >= m.tab_x && mx < m.tab_x + m.tab_w {
                    // Toggle: close if already open, else open
                    self.open_menu = if self.open_menu == Some(i) { None } else { Some(i) };
                    self.hov_item  = None;
                    return MenuAction::None;
                }
            }
            // Clicked bar but not a tab — close menu
            self.open_menu = None;
            return MenuAction::None;
        }

        // Click in the open dropdown?
        if let Some(mi) = self.open_menu {
            let menu = &self.menus[mi];
            let drop_x = menu.tab_x;
            let drop_y = bar_y + MENUBAR_H;
            let drop_h = menu.dropdown_height();

            if mx >= drop_x && mx < drop_x + DROPDOWN_W
                && my >= drop_y && my < drop_y + drop_h
            {
                let rel_y = my - drop_y;
                if let Some(item_idx) = menu.item_at_rel_y(rel_y) {
                    let item = menu.items[item_idx];
                    if item.enabled && !item.is_sep {
                        self.open_menu = None;
                        self.hov_item  = None;
                        return item.action;
                    }
                }
                return MenuAction::None; // Consumed but no action (e.g. disabled item)
            }
        }

        // Click outside → close
        self.close();
        MenuAction::None
    }

    /// Close any open dropdown.
    pub fn close(&mut self) {
        self.open_menu = None;
        self.hov_menu  = None;
        self.hov_item  = None;
    }

    /// True if any dropdown is currently open.
    pub fn is_open(&self) -> bool { self.open_menu.is_some() }

    /// True if (mx, my) lies within the bar or open dropdown.
    pub fn hit_test(&self, mx: u64, my: u64, bar_x: u64, bar_y: u64, bar_w: u64) -> bool {
        if my >= bar_y && my < bar_y + MENUBAR_H && mx >= bar_x && mx < bar_x + bar_w {
            return true;
        }
        if let Some(mi) = self.open_menu {
            let menu = &self.menus[mi];
            let drop_y = bar_y + MENUBAR_H;
            let drop_h = menu.dropdown_height();
            if mx >= menu.tab_x && mx < menu.tab_x + DROPDOWN_W
                && my >= drop_y && my < drop_y + drop_h
            {
                return true;
            }
        }
        false
    }
}
