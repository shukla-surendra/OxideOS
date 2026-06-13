// gui/colors.rs — re-exports oxide-gui-core color system + kernel-specific extensions.
// The base Color type, rgb/argb constructors, lerp_color, and named palette constants
// all come from oxide-gui-core::color.  Kernel-specific submodules (dark_theme, ui,
// light_theme, retro_theme) are defined here using the same rgb() primitive.

pub use oxide_gui_core::color::{Color, rgb, argb, lerp_color};
pub use oxide_gui_core::color::palette::{
    BLACK, WHITE, DARK_GRAY, GRAY, LIGHT_GRAY,
    RED, GREEN, BLUE, YELLOW, CYAN, ACCENT,
    PURPLE, DEEP_PURPLE, PINK, ROSE, ORANGE, AMBER, TEAL, INDIGO,
    NEON_CYAN, ELECTRIC_BLUE, GNOME_BLUE,
    SURFACE, SURFACE2, CARD_BG, CARD_BORDER,
    DARK_BG, PANEL_BG, TOOLBAR_BG, STATUS_BG, TEXT, TEXT_DIM, DIVIDER,
};

/// rgba(r, g, b, a) — kernel convention; delegates to oxide-gui-core argb(a, r, g, b).
#[inline]
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color { argb(a, r, g, b) }

/// Alpha-blend `foreground` over `background`.  Used by graphics.rs.
pub fn blend_colors(foreground: Color, background: Color) -> Color {
    let fa = ((foreground >> 24) & 0xFF) as u32;
    let ba = 255 - fa;
    let r = (((foreground >> 16) & 0xFF) * fa + ((background >> 16) & 0xFF) * ba) / 255;
    let g = (((foreground >>  8) & 0xFF) * fa + ((background >>  8) & 0xFF) * ba) / 255;
    let b = ((foreground        & 0xFF) * fa + (background        & 0xFF) * ba) / 255;
    rgb(r as u8, g as u8, b as u8)
}

// ── Kernel-specific color submodules ──────────────────────────────────────────

pub mod dark_theme {
    use super::rgb;
    pub const BACKGROUND:       u32 = rgb(0x1E, 0x1E, 0x1E);
    pub const SURFACE:          u32 = rgb(0x2D, 0x2D, 0x2D);
    pub const SURFACE_VARIANT:  u32 = rgb(0x3C, 0x3C, 0x3C);
    pub const BORDER:           u32 = rgb(0x48, 0x48, 0x48);
    pub const BORDER_FOCUS:     u32 = rgb(0x00, 0x7A, 0xCC);
    pub const DIVIDER:          u32 = rgb(0x3C, 0x3C, 0x3C);
    pub const TEXT_PRIMARY:     u32 = rgb(0xE1, 0xE1, 0xE1);
    pub const TEXT_SECONDARY:   u32 = rgb(0xB3, 0xB3, 0xB3);
    pub const TEXT_DISABLED:    u32 = rgb(0x6B, 0x6B, 0x6B);
    pub const TEXT_INVERSE:     u32 = rgb(0x1E, 0x1E, 0x1E);
    pub const ACCENT_PRIMARY:   u32 = rgb(0x00, 0x7A, 0xCC);
    pub const ACCENT_SECONDARY: u32 = rgb(0x00, 0xA8, 0xCC);
    pub const ACCENT_TERTIARY:  u32 = rgb(0x9B, 0x59, 0xB6);
    pub const SUCCESS:          u32 = rgb(0x4C, 0xAF, 0x50);
    pub const WARNING:          u32 = rgb(0xFF, 0x98, 0x00);
    pub const ERROR:            u32 = rgb(0xF4, 0x43, 0x36);
    pub const INFO:             u32 = rgb(0x21, 0x96, 0xF3);
    pub const BUTTON_PRIMARY:   u32 = rgb(0x00, 0x7A, 0xCC);
    pub const BUTTON_SECONDARY: u32 = rgb(0x48, 0x48, 0x48);
    pub const BUTTON_HOVER:     u32 = rgb(0x00, 0x5A, 0x9E);
    pub const BUTTON_PRESSED:   u32 = rgb(0x00, 0x3D, 0x6B);
    pub const BUTTON_DISABLED:  u32 = rgb(0x2D, 0x2D, 0x2D);
}

pub mod light_theme {
    use super::rgb;
    pub const BACKGROUND:       u32 = rgb(0xFA, 0xFA, 0xFA);
    pub const SURFACE:          u32 = rgb(0xFF, 0xFF, 0xFF);
    pub const SURFACE_VARIANT:  u32 = rgb(0xF5, 0xF5, 0xF5);
    pub const BORDER:           u32 = rgb(0xE0, 0xE0, 0xE0);
    pub const BORDER_FOCUS:     u32 = rgb(0x00, 0x7A, 0xCC);
    pub const DIVIDER:          u32 = rgb(0xE8, 0xE8, 0xE8);
    pub const TEXT_PRIMARY:     u32 = rgb(0x21, 0x21, 0x21);
    pub const TEXT_SECONDARY:   u32 = rgb(0x75, 0x75, 0x75);
    pub const TEXT_DISABLED:    u32 = rgb(0xBD, 0xBD, 0xBD);
    pub const TEXT_INVERSE:     u32 = rgb(0xFF, 0xFF, 0xFF);
    pub const ACCENT_PRIMARY:   u32 = rgb(0x00, 0x7A, 0xCC);
    pub const ACCENT_SECONDARY: u32 = rgb(0x00, 0xA8, 0xCC);
    pub const ACCENT_TERTIARY:  u32 = rgb(0x9B, 0x59, 0xB6);
    pub const SUCCESS:          u32 = rgb(0x4C, 0xAF, 0x50);
    pub const WARNING:          u32 = rgb(0xFF, 0x98, 0x00);
    pub const ERROR:            u32 = rgb(0xF4, 0x43, 0x36);
    pub const INFO:             u32 = rgb(0x21, 0x96, 0xF3);
    pub const BUTTON_PRIMARY:   u32 = rgb(0x00, 0x7A, 0xCC);
    pub const BUTTON_SECONDARY: u32 = rgb(0xE0, 0xE0, 0xE0);
    pub const BUTTON_HOVER:     u32 = rgb(0x00, 0x5A, 0x9E);
    pub const BUTTON_PRESSED:   u32 = rgb(0x00, 0x3D, 0x6B);
    pub const BUTTON_DISABLED:  u32 = rgb(0xF5, 0xF5, 0xF5);
}

pub mod retro_theme {
    use super::rgb;
    pub const BACKGROUND:       u32 = rgb(0x00, 0x00, 0x00);
    pub const TEXT:             u32 = rgb(0x00, 0xFF, 0x00);
    pub const CURSOR:           u32 = rgb(0x00, 0xFF, 0x00);
    pub const SELECTION:        u32 = rgb(0x00, 0x40, 0x00);
    pub const AMBER_TEXT:       u32 = rgb(0xFF, 0xB0, 0x00);
    pub const AMBER_BACKGROUND: u32 = rgb(0x00, 0x00, 0x00);
    pub const BLUE_TEXT:        u32 = rgb(0x00, 0xAA, 0xFF);
    pub const BLUE_BACKGROUND:  u32 = rgb(0x00, 0x00, 0x22);
}

pub mod ui {
    use super::rgb;
    pub const TITLEBAR:                  u32 = rgb(0x30, 0x30, 0x30);
    pub const TITLEBAR_ACTIVE:           u32 = rgb(0x38, 0x38, 0x38);
    pub const TITLEBAR_TEXT:             u32 = rgb(0xEE, 0xEE, 0xEE);
    pub const TITLEBAR_FOCUSED_LEFT:     u32 = rgb(0x3A, 0x3A, 0x3A);
    pub const TITLEBAR_FOCUSED_RIGHT:    u32 = rgb(0x42, 0x42, 0x42);
    pub const TITLEBAR_UNFOCUSED_LEFT:   u32 = rgb(0x28, 0x28, 0x28);
    pub const TITLEBAR_UNFOCUSED_RIGHT:  u32 = rgb(0x2E, 0x2E, 0x2E);
    pub const TITLEBAR_ACCENT_FOCUSED:   u32 = rgb(0x52, 0x94, 0xE2);
    pub const TITLEBAR_ACCENT_UNFOCUSED: u32 = rgb(0x3A, 0x3A, 0x3A);
    pub const WINDOW_SHADOW:             u32 = 0x60000000;
    pub const WINDOW_BORDER_FOCUSED:     u32 = rgb(0x52, 0x94, 0xE2);
    pub const WINDOW_BORDER_UNFOCUSED:   u32 = rgb(0x46, 0x46, 0x46);
    pub const TASKBAR_BG:                u32 = rgb(0x2E, 0x2E, 0x2E);
    pub const TASKBAR_ACCENT:            u32 = rgb(0x52, 0x94, 0xE2);
    pub const TASKBAR_TEXT:              u32 = rgb(0xEE, 0xEE, 0xEE);
    pub const MENU_BACKGROUND:           u32 = rgb(0x2D, 0x2D, 0x2D);
    pub const MENU_HOVER:                u32 = rgb(0x48, 0x48, 0x48);
    pub const MENU_SELECTED:             u32 = rgb(0x00, 0x7A, 0xCC);
    pub const MENU_TEXT:                 u32 = rgb(0xE1, 0xE1, 0xE1);
    pub const SCROLLBAR_TRACK:           u32 = rgb(0x1E, 0x1E, 0x1E);
    pub const SCROLLBAR_THUMB:           u32 = rgb(0x48, 0x48, 0x48);
    pub const SCROLLBAR_HOVER:           u32 = rgb(0x6B, 0x6B, 0x6B);
    pub const INPUT_BACKGROUND:          u32 = rgb(0x1E, 0x1E, 0x1E);
    pub const INPUT_BORDER:              u32 = rgb(0x48, 0x48, 0x48);
    pub const INPUT_FOCUS:               u32 = rgb(0x00, 0x7A, 0xCC);
    pub const INPUT_TEXT:                u32 = rgb(0xE1, 0xE1, 0xE1);
    pub const INPUT_PLACEHOLDER:         u32 = rgb(0x88, 0x88, 0x88);
    pub const PROGRESS_BACKGROUND:       u32 = rgb(0x2D, 0x2D, 0x2D);
    pub const PROGRESS_FILL:             u32 = rgb(0x00, 0x7A, 0xCC);
    pub const TOOLTIP_BACKGROUND:        u32 = rgb(0x38, 0x38, 0x38);
    pub const TOOLTIP_BORDER:            u32 = rgb(0x48, 0x48, 0x48);
    pub const TOOLTIP_TEXT:              u32 = rgb(0xE1, 0xE1, 0xE1);
}
