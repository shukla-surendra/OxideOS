//! Compile-time configuration: window geometry, grid metrics, sidebar layout, and colors.
//!
//! All other modules import from here. Nothing should be hardcoded elsewhere.

// ── Window ─────────────────────────────────────────────────────────────────────
pub const WIN_W_INIT: u32 = 820;
pub const WIN_H_INIT: u32 = 540;

// ── Typography & grid ─────────────────────────────────────────────────────────
pub const CHAR_W:     u32 = 9;   // monospace glyph width in pixels
pub const PAD:        u32 = 8;   // general padding
pub const ROW_H:      u32 = 20;  // file-list row height
pub const HEADER_H:   u32 = 20;  // column-header bar height
pub const TOOLBAR_H:  u32 = 32;  // top toolbar height
pub const STATUS_H:   u32 = 22;  // bottom status bar height
pub const ACTION_BAR_H: u32 = 28; // new/rename/delete input bar height
pub const SIDEBAR_W:  u32 = 200; // sidebar width in pixels
pub const DIV_W:      u32 = 1;   // vertical divider line width
pub const SCROLL_W:   u32 = 10;  // scrollbar width
pub const COL_TYPE_W: u32 = 50;  // TYPE column width (measured from right edge)
pub const COL_SIZE_W: u32 = 68;  // SIZE column width (measured from right edge)

// ── Interaction ─────────────────────────────────────────────────────────────
/// Max gap between two clicks on the same row to count as a double-click,
/// in system-timer ticks. The timer runs at 100 Hz, so 40 ticks ≈ 400 ms.
pub const DOUBLE_CLICK_TICKS: u64 = 40;

// ── Sidebar section heights ────────────────────────────────────────────────────
pub const SIDEBAR_MAIN_H: u32 = 24; // "EXPLORER" master header
pub const SIDEBAR_SEC_H:  u32 = 18; // "PLACES" / "PATH" section headers
pub const SIDEBAR_ITEM_H: u32 = 22; // per-item row height
pub const SIDEBAR_INDENT: u32 = 10; // px per depth level in the PATH tree
pub const N_PLACES:       u32 = 5;  // must equal `SIDEBAR_ITEMS.len()`

// ── Precomputed sidebar Y positions (measured from window top) ─────────────────
pub const PLACES_SEC_Y:   u32 = TOOLBAR_H + SIDEBAR_MAIN_H;                 // 56
pub const PLACES_ITEMS_Y: u32 = PLACES_SEC_Y + SIDEBAR_SEC_H;               // 74
pub const PATH_SEC_Y:     u32 = PLACES_ITEMS_Y + N_PLACES * SIDEBAR_ITEM_H; // 184
pub const PATH_ITEMS_Y:   u32 = PATH_SEC_Y + SIDEBAR_SEC_H;                 // 202

// ── Colors (ARGB 0xFF_RR_GG_BB, VS Code dark theme inspired) ─────────────────
pub const COL_BG:           u32 = 0xFF1E1E1E;
pub const COL_SIDEBAR_BG:   u32 = 0xFF252526;
pub const COL_TOOLBAR_BG:   u32 = 0xFF3C3C3C;
pub const COL_HEADER_BG:    u32 = 0xFF2D2D30;
pub const COL_STATUS_BG:    u32 = 0xFF007ACC;
pub const COL_ROW_ODD:      u32 = 0xFF252526;
pub const COL_SELECTED:     u32 = 0xFF094771;
pub const COL_HOVER:        u32 = 0xFF2A2D2E;
pub const COL_SIDEBAR_CUR:  u32 = 0xFF37373D;
pub const COL_SIDEBAR_HOV:  u32 = 0xFF2A2D2E;
pub const COL_SIDEBAR_SEC:  u32 = 0xFF1C1C1C;
pub const COL_DIVIDER:      u32 = 0xFF3F3F46;
pub const COL_TEXT:         u32 = 0xFFD4D4D4;
pub const COL_TEXT_DIM:     u32 = 0xFF858585;
pub const COL_DIR:          u32 = 0xFF4EC9B0;
pub const COL_FILE:         u32 = 0xFFCCCCCC;
pub const COL_ACCENT:       u32 = 0xFF4EC9B0;
pub const COL_STATUS_TXT:   u32 = 0xFFFFFFFF;
pub const COL_SCROLL_TRACK: u32 = 0xFF3C3C3C;
pub const COL_SCROLL_THUMB: u32 = 0xFF6D6D6D;
pub const COL_SIZE_FG:      u32 = 0xFF808080;
pub const COL_BTN_BG:       u32 = 0xFF4D4D4D;
pub const COL_PATH_LEAF:    u32 = 0xFF4EC9B0; // current dir in PATH tree
pub const COL_PATH_ANC:     u32 = 0xFF9CDCFE; // ancestor dirs in PATH tree
pub const COL_SEC_TXT:      u32 = 0xFF6A6A6A; // section header label text
pub const COL_BAR_BG:       u32 = 0xFF2D2D30; // new/rename/delete action bar background
pub const COL_BAR_INPUT_BG: u32 = 0xFF1E1E1E; // action bar text-input background
pub const COL_DANGER:       u32 = 0xFFF48771; // delete-confirm / error text
pub const COL_OK:           u32 = 0xFF73C991; // positive / confirmation status text
pub const COL_EMPTY_TXT:    u32 = 0xFF6A6A6A; // "this folder is empty" placeholder
