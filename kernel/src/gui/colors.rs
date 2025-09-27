// Complete color palette for OxideOS
// Format: 0xAARRGGBB (Alpha, Red, Green, Blue)

// ============================================================================
// BASIC COLORS
// ============================================================================
pub const BLACK: u32       = 0xFF000000;
pub const WHITE: u32       = 0xFFFFFFFF;
pub const RED: u32         = 0xFFFF0000;
pub const GREEN: u32       = 0xFF00FF00;
pub const BLUE: u32        = 0xFF0000FF;
pub const YELLOW: u32      = 0xFFFFFF00;
pub const CYAN: u32        = 0xFF00FFFF;
pub const MAGENTA: u32     = 0xFFFF00FF;

// ============================================================================
// GRAYSCALE SPECTRUM
// ============================================================================
pub const GRAY: u32        = 0xFF808080;
pub const DARK_GRAY: u32   = 0xFF404040;
pub const LIGHT_GRAY: u32  = 0xFFC0C0C0;
pub const VERY_DARK_GRAY: u32 = 0xFF202020;
pub const VERY_LIGHT_GRAY: u32 = 0xFFE0E0E0;
pub const SILVER: u32      = 0xFFC4C4C4;
pub const CHARCOAL: u32    = 0xFF36454F;

// ============================================================================
// EXTENDED COLOR PALETTE
// ============================================================================
pub const ORANGE: u32      = 0xFFFFA500;
pub const PURPLE: u32      = 0xFF800080;
pub const BROWN: u32       = 0xFFA52A2A;
pub const PINK: u32        = 0xFFFFC0CB;
pub const LIME: u32        = 0xFF32CD32;
pub const MAROON: u32      = 0xFF800000;
pub const NAVY: u32        = 0xFF000080;
pub const OLIVE: u32       = 0xFF808000;
pub const TEAL: u32        = 0xFF008080;
pub const AQUA: u32        = 0xFF00FFFF;
pub const FUCHSIA: u32     = 0xFFFF00FF;
pub const INDIGO: u32      = 0xFF4B0082;
pub const VIOLET: u32      = 0xFFEE82EE;
pub const GOLD: u32        = 0xFFFFD700;
pub const CORAL: u32       = 0xFFFF7F50;
pub const SALMON: u32      = 0xFFFA8072;
pub const KHAKI: u32       = 0xFFF0E68C;
pub const TURQUOISE: u32   = 0xFF40E0D0;

// ============================================================================
// OS THEME COLORS - PROFESSIONAL DARK THEME
// ============================================================================
pub mod dark_theme {
    // Background colors
    pub const BACKGROUND: u32        = 0xFF1E1E1E;  // Dark background
    pub const SURFACE: u32           = 0xFF2D2D2D;  // Card/panel background
    pub const SURFACE_VARIANT: u32   = 0xFF3C3C3C;  // Elevated surfaces

    // Border and outline colors
    pub const BORDER: u32            = 0xFF484848;  // Default borders
    pub const BORDER_FOCUS: u32      = 0xFF007ACC;  // Focused element border
    pub const DIVIDER: u32           = 0xFF3C3C3C;  // Divider lines

    // Text colors
    pub const TEXT_PRIMARY: u32      = 0xFFE1E1E1;  // Main text
    pub const TEXT_SECONDARY: u32    = 0xFFB3B3B3;  // Secondary text
    pub const TEXT_DISABLED: u32     = 0xFF6B6B6B;  // Disabled text
    pub const TEXT_INVERSE: u32      = 0xFF1E1E1E;  // Text on light backgrounds

    // Accent colors
    pub const ACCENT_PRIMARY: u32    = 0xFF007ACC;  // Primary brand color
    pub const ACCENT_SECONDARY: u32  = 0xFF00A8CC;  // Secondary accent
    pub const ACCENT_TERTIARY: u32   = 0xFF9B59B6;  // Tertiary accent

    // State colors
    pub const SUCCESS: u32           = 0xFF4CAF50;  // Success/confirmation
    pub const WARNING: u32           = 0xFFFF9800;  // Warning/caution
    pub const ERROR: u32             = 0xFFF44336;  // Error/danger
    pub const INFO: u32              = 0xFF2196F3;  // Information

    // Interactive element colors
    pub const BUTTON_PRIMARY: u32    = 0xFF007ACC;  // Primary button
    pub const BUTTON_SECONDARY: u32  = 0xFF484848;  // Secondary button
    pub const BUTTON_HOVER: u32      = 0xFF005A9E;  // Button hover state
    pub const BUTTON_PRESSED: u32    = 0xFF003D6B;  // Button pressed state
    pub const BUTTON_DISABLED: u32   = 0xFF2D2D2D;  // Disabled button
}

// ============================================================================
// OS THEME COLORS - LIGHT THEME
// ============================================================================
pub mod light_theme {
    // Background colors
    pub const BACKGROUND: u32        = 0xFFFAFAFA;  // Light background
    pub const SURFACE: u32           = 0xFFFFFFFF;  // Card/panel background
    pub const SURFACE_VARIANT: u32   = 0xFFF5F5F5;  // Elevated surfaces

    // Border and outline colors
    pub const BORDER: u32            = 0xFFE0E0E0;  // Default borders
    pub const BORDER_FOCUS: u32      = 0xFF007ACC;  // Focused element border
    pub const DIVIDER: u32           = 0xFFE8E8E8;  // Divider lines

    // Text colors
    pub const TEXT_PRIMARY: u32      = 0xFF212121;  // Main text
    pub const TEXT_SECONDARY: u32    = 0xFF757575;  // Secondary text
    pub const TEXT_DISABLED: u32     = 0xFFBDBDBD;  // Disabled text
    pub const TEXT_INVERSE: u32      = 0xFFFFFFFF;  // Text on dark backgrounds

    // Accent colors (same as dark theme for consistency)
    pub const ACCENT_PRIMARY: u32    = 0xFF007ACC;  // Primary brand color
    pub const ACCENT_SECONDARY: u32  = 0xFF00A8CC;  // Secondary accent
    pub const ACCENT_TERTIARY: u32   = 0xFF9B59B6;  // Tertiary accent

    // State colors (same as dark theme)
    pub const SUCCESS: u32           = 0xFF4CAF50;  // Success/confirmation
    pub const WARNING: u32           = 0xFFFF9800;  // Warning/caution
    pub const ERROR: u32             = 0xFFF44336;  // Error/danger
    pub const INFO: u32              = 0xFF2196F3;  // Information

    // Interactive element colors
    pub const BUTTON_PRIMARY: u32    = 0xFF007ACC;  // Primary button
    pub const BUTTON_SECONDARY: u32  = 0xFFE0E0E0;  // Secondary button
    pub const BUTTON_HOVER: u32      = 0xFF005A9E;  // Button hover state
    pub const BUTTON_PRESSED: u32    = 0xFF003D6B;  // Button pressed state
    pub const BUTTON_DISABLED: u32   = 0xFFF5F5F5;  // Disabled button
}

// ============================================================================
// RETRO/TERMINAL THEME COLORS
// ============================================================================
pub mod retro_theme {
    // Classic terminal colors
    pub const BACKGROUND: u32        = 0xFF000000;  // Black background
    pub const TEXT: u32              = 0xFF00FF00;  // Green text
    pub const CURSOR: u32            = 0xFF00FF00;  // Green cursor
    pub const SELECTION: u32         = 0xFF004000;  // Dark green selection

    // Alternative retro schemes
    pub const AMBER_TEXT: u32        = 0xFFFFB000;  // Amber terminal
    pub const AMBER_BACKGROUND: u32  = 0xFF000000;  // Black background

    pub const BLUE_TEXT: u32         = 0xFF00AAFF;  // Blue terminal
    pub const BLUE_BACKGROUND: u32   = 0xFF000022;  // Dark blue background
}

// ============================================================================
// SEMANTIC COLORS FOR UI COMPONENTS
// ============================================================================
pub mod ui {
    // Window components
    pub const TITLEBAR: u32          = 0xFF2D2D2D;  // Window title bar
    pub const TITLEBAR_ACTIVE: u32   = 0xFF007ACC;  // Active window title bar
    pub const TITLEBAR_TEXT: u32     = 0xFFE1E1E1;  // Title bar text

    // Menu and toolbar
    pub const MENU_BACKGROUND: u32   = 0xFF2D2D2D;  // Menu background
    pub const MENU_HOVER: u32        = 0xFF484848;  // Menu item hover
    pub const MENU_SELECTED: u32     = 0xFF007ACC;  // Menu item selected
    pub const MENU_TEXT: u32         = 0xFFE1E1E1;  // Menu text

    // Scrollbars
    pub const SCROLLBAR_TRACK: u32   = 0xFF1E1E1E;  // Scrollbar track
    pub const SCROLLBAR_THUMB: u32   = 0xFF484848;  // Scrollbar thumb
    pub const SCROLLBAR_HOVER: u32   = 0xFF6B6B6B;  // Scrollbar thumb hover

    // Input fields
    pub const INPUT_BACKGROUND: u32  = 0xFF1E1E1E;  // Input field background
    pub const INPUT_BORDER: u32      = 0xFF484848;  // Input field border
    pub const INPUT_FOCUS: u32       = 0xFF007ACC;  // Input field focus border
    pub const INPUT_TEXT: u32        = 0xFFE1E1E1;  // Input field text
    pub const INPUT_PLACEHOLDER: u32 = 0xFF888888;  // Placeholder text

    // Progress bars
    pub const PROGRESS_BACKGROUND: u32 = 0xFF2D2D2D;  // Progress bar background
    pub const PROGRESS_FILL: u32      = 0xFF007ACC;   // Progress bar fill

    // Tooltips
    pub const TOOLTIP_BACKGROUND: u32 = 0xFF383838;   // Tooltip background
    pub const TOOLTIP_BORDER: u32     = 0xFF484848;   // Tooltip border
    pub const TOOLTIP_TEXT: u32       = 0xFFE1E1E1;   // Tooltip text
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Create a color with alpha transparency
pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Create an opaque color from RGB values
pub const fn rgb(r: u8, g: u8, b: u8) -> u32 {
    rgba(r, g, b, 255)
}

/// Extract red component from color
pub const fn get_red(color: u32) -> u8 {
    ((color >> 16) & 0xFF) as u8
}

/// Extract green component from color
pub const fn get_green(color: u32) -> u8 {
    ((color >> 8) & 0xFF) as u8
}

/// Extract blue component from color
pub const fn get_blue(color: u32) -> u8 {
    (color & 0xFF) as u8
}

/// Extract alpha component from color
pub const fn get_alpha(color: u32) -> u8 {
    ((color >> 24) & 0xFF) as u8
}

/// Blend two colors with alpha
pub fn blend_colors(foreground: u32, background: u32) -> u32 {
    let fg_alpha = get_alpha(foreground) as u32;
    let bg_alpha = 255 - fg_alpha;

    let r = (get_red(foreground) as u32 * fg_alpha + get_red(background) as u32 * bg_alpha) / 255;
    let g = (get_green(foreground) as u32 * fg_alpha + get_green(background) as u32 * bg_alpha) / 255;
    let b = (get_blue(foreground) as u32 * fg_alpha + get_blue(background) as u32 * bg_alpha) / 255;

    rgb(r as u8, g as u8, b as u8)
}

/// Darken a color by a percentage (0-100)
pub fn darken(color: u32, percent: u8) -> u32 {
    let factor = (100 - percent.min(100)) as u32;
    let r = (get_red(color) as u32 * factor / 100) as u8;
    let g = (get_green(color) as u32 * factor / 100) as u8;
    let b = (get_blue(color) as u32 * factor / 100) as u8;

    rgba(r, g, b, get_alpha(color))
}

/// Lighten a color by a percentage (0-100)
pub fn lighten(color: u32, percent: u8) -> u32 {
    let factor = percent.min(100) as u32;
    let r = (get_red(color) as u32 + (255 - get_red(color) as u32) * factor / 100) as u8;
    let g = (get_green(color) as u32 + (255 - get_green(color) as u32) * factor / 100) as u8;
    let b = (get_blue(color) as u32 + (255 - get_blue(color) as u32) * factor / 100) as u8;

    rgba(r, g, b, get_alpha(color))
}