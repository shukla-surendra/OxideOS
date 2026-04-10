// src/gui/window_manager.rs - Enhanced with minimize/maximize and taskbar
use super::widgets::Window;
use super::graphics::{Graphics, BackgroundStyle};
use super::colors;
use super::fonts;
use crate::kernel::serial::SERIAL_PORT;

const MAX_WINDOWS: usize = 16;
const TASKBAR_HEIGHT: u64 = 40;

/// Format timer ticks as "HH:MM:SS" into `buf` (exactly 8 bytes).
/// The timer runs at 100 Hz, so ticks / 100 = seconds since boot.
fn format_uptime(ticks: u64, buf: &mut [u8; 8]) {
    let total_secs = ticks / 100;
    let h = (total_secs / 3600) % 100; // cap hours at 99 to stay 2 digits
    let m = (total_secs / 60) % 60;
    let s = total_secs % 60;
    buf[0] = b'0' + (h / 10) as u8;
    buf[1] = b'0' + (h % 10) as u8;
    buf[2] = b':';
    buf[3] = b'0' + (m / 10) as u8;
    buf[4] = b'0' + (m % 10) as u8;
    buf[5] = b':';
    buf[6] = b'0' + (s / 10) as u8;
    buf[7] = b'0' + (s % 10) as u8;
}
const TASKBAR_ITEM_WIDTH: u64 = 150;
const TASKBAR_ITEM_SPACING: u64 = 5;

#[derive(Clone, Copy, PartialEq)]
pub enum WindowState {
    Normal,
    Minimized,
    Maximized,
}

pub struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    window_states: [WindowState; MAX_WINDOWS],
    saved_positions: [(u64, u64, u64, u64); MAX_WINDOWS], // x, y, width, height before maximize
    z_order: [usize; MAX_WINDOWS],
    window_count: usize,
    focused_window: Option<usize>,
    dragging_window: Option<usize>,
    drag_offset_x: i64,
    drag_offset_y: i64,
    screen_width: u64,
    screen_height: u64,
    // Desktop context menu
    context_menu_visible: bool,
    context_menu_x: u64,
    context_menu_y: u64,
    background_style: BackgroundStyle,
}

impl WindowManager {
    pub const fn new() -> Self {
        Self {
            windows: [None; MAX_WINDOWS],
            window_states: [WindowState::Normal; MAX_WINDOWS],
            saved_positions: [(0, 0, 0, 0); MAX_WINDOWS],
            z_order: [0; MAX_WINDOWS],
            window_count: 0,
            focused_window: None,
            dragging_window: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
            screen_width: 1280,
            screen_height: 800,
            context_menu_visible: false,
            context_menu_x: 0,
            context_menu_y: 0,
            background_style: BackgroundStyle::Default,
        }
    }

    pub fn set_screen_dimensions(&mut self, width: u64, height: u64) {
        self.screen_width = width;
        self.screen_height = height;
    }

    pub fn add_window(&mut self, window: Window) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            unsafe {
                SERIAL_PORT.write_str("WindowManager: Maximum windows reached\n");
            }
            return None;
        }

        for i in 0..MAX_WINDOWS {
            if self.windows[i].is_none() {
                self.windows[i] = Some(window);
                self.window_states[i] = WindowState::Normal;
                self.z_order[self.window_count] = i;
                self.window_count += 1;
                self.focused_window = Some(i);
                
                unsafe {
                    SERIAL_PORT.write_str("WindowManager: Added window ID ");
                    SERIAL_PORT.write_decimal(i as u32);
                    SERIAL_PORT.write_str("\n");
                }
                
                return Some(i);
            }
        }
        
        None
    }

    pub fn remove_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.windows[window_id].is_none() {
            return;
        }

        self.windows[window_id] = None;
        
        let mut found_idx = None;
        for i in 0..self.window_count {
            if self.z_order[i] == window_id {
                found_idx = Some(i);
                break;
            }
        }
        
        if let Some(idx) = found_idx {
            for i in idx..self.window_count - 1 {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.window_count -= 1;
        }
        
        if self.focused_window == Some(window_id) {
            self.focused_window = if self.window_count > 0 {
                Some(self.z_order[self.window_count - 1])
            } else {
                None
            };
        }
        
        if self.dragging_window == Some(window_id) {
            self.dragging_window = None;
        }
    }

    pub fn minimize_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.windows[window_id].is_none() {
            return;
        }

        self.window_states[window_id] = WindowState::Minimized;
        
        // Unfocus this window
        if self.focused_window == Some(window_id) {
            self.focused_window = None;
            // Focus next visible window
            for i in (0..self.window_count).rev() {
                let id = self.z_order[i];
                if id != window_id && self.window_states[id] != WindowState::Minimized {
                    self.focused_window = Some(id);
                    break;
                }
            }
        }

        unsafe {
            SERIAL_PORT.write_str("WindowManager: Minimized window ");
            SERIAL_PORT.write_decimal(window_id as u32);
            SERIAL_PORT.write_str("\n");
        }
    }

    pub fn maximize_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS {
            return;
        }

        if let Some(ref mut window) = self.windows[window_id] {
            match self.window_states[window_id] {
                WindowState::Maximized => {
                    // Restore to normal
                    let (x, y, w, h) = self.saved_positions[window_id];
                    window.x = x;
                    window.y = y;
                    window.width = w;
                    window.height = h;
                    self.window_states[window_id] = WindowState::Normal;
                    
                    unsafe {
                        SERIAL_PORT.write_str("WindowManager: Restored window ");
                        SERIAL_PORT.write_decimal(window_id as u32);
                        SERIAL_PORT.write_str("\n");
                    }
                },
                _ => {
                    // Save current position
                    self.saved_positions[window_id] = (window.x, window.y, window.width, window.height);
                    
                    // Maximize (leave space for taskbar)
                    window.x = 0;
                    window.y = TASKBAR_HEIGHT;
                    window.width = self.screen_width;
                    window.height = self.screen_height - TASKBAR_HEIGHT;
                    self.window_states[window_id] = WindowState::Maximized;
                    
                    unsafe {
                        SERIAL_PORT.write_str("WindowManager: Maximized window ");
                        SERIAL_PORT.write_decimal(window_id as u32);
                        SERIAL_PORT.write_str("\n");
                    }
                }
            }
        }
    }

    pub fn restore_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS {
            return;
        }

        match self.window_states[window_id] {
            WindowState::Minimized => {
                self.window_states[window_id] = WindowState::Normal;
                self.bring_to_front(window_id);
            },
            WindowState::Maximized => {
                self.maximize_window(window_id); // Toggle back to normal
            },
            WindowState::Normal => {}
        }
    }

    pub fn bring_to_front(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.windows[window_id].is_none() {
            return;
        }

        let mut current_pos = None;
        for i in 0..self.window_count {
            if self.z_order[i] == window_id {
                current_pos = Some(i);
                break;
            }
        }

        if let Some(pos) = current_pos {
            if pos == self.window_count - 1 {
                return;
            }

            for i in pos..self.window_count - 1 {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.z_order[self.window_count - 1] = window_id;
            
            self.focused_window = Some(window_id);
        }
    }

    pub fn handle_click(&mut self, mouse_x: u64, mouse_y: u64) -> bool {
        // Check taskbar first
        if mouse_y < TASKBAR_HEIGHT {
            return self.handle_taskbar_click(mouse_x);
        }

        // Check windows from top to bottom
        let mut clicked_window: Option<usize> = None;
        let mut clicked_close_button = false;
        let mut clicked_minimize_button = false;
        let mut clicked_maximize_button = false;
        let mut clicked_titlebar = false;
        let mut drag_offset_x = 0i64;
        let mut drag_offset_y = 0i64;

        for i in (0..self.window_count).rev() {
            let window_id = self.z_order[i];
            
            if self.window_states[window_id] == WindowState::Minimized {
                continue;
            }
            
            if let Some(ref window) = self.windows[window_id] {
                if !window.visible {
                    continue;
                }

                // Check window control buttons
                if self.is_close_button_clicked(window, mouse_x, mouse_y) {
                    clicked_window = Some(window_id);
                    clicked_close_button = true;
                    break;
                }

                if self.is_maximize_button_clicked(window, mouse_x, mouse_y) {
                    clicked_window = Some(window_id);
                    clicked_maximize_button = true;
                    break;
                }

                if self.is_minimize_button_clicked(window, mouse_x, mouse_y) {
                    clicked_window = Some(window_id);
                    clicked_minimize_button = true;
                    break;
                }

                if window.is_titlebar_clicked(mouse_x, mouse_y) {
                    clicked_window = Some(window_id);
                    clicked_titlebar = true;
                    drag_offset_x = mouse_x as i64 - window.x as i64;
                    drag_offset_y = mouse_y as i64 - window.y as i64;
                    break;
                }

                if mouse_x >= window.x && mouse_x < window.x + window.width &&
                   mouse_y >= window.y && mouse_y < window.y + window.height {
                    clicked_window = Some(window_id);
                    break;
                }
            }
        }

        if let Some(window_id) = clicked_window {
            if clicked_close_button {
                self.remove_window(window_id);
            } else if clicked_maximize_button {
                self.maximize_window(window_id);
            } else if clicked_minimize_button {
                self.minimize_window(window_id);
            } else if clicked_titlebar {
                self.bring_to_front(window_id);
                // Don't allow dragging maximized windows
                if self.window_states[window_id] != WindowState::Maximized {
                    self.dragging_window = Some(window_id);
                    self.drag_offset_x = drag_offset_x;
                    self.drag_offset_y = drag_offset_y;
                }
            } else {
                self.bring_to_front(window_id);
            }
            
            return true;
        }
        
        false
    }

    fn handle_taskbar_click(&mut self, mouse_x: u64) -> bool {
        // Calculate which taskbar item was clicked
        let start_x = 100u64; // Leave space for OS name
        
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            let item_x = start_x + (i as u64) * (TASKBAR_ITEM_WIDTH + TASKBAR_ITEM_SPACING);
            
            if mouse_x >= item_x && mouse_x < item_x + TASKBAR_ITEM_WIDTH {
                // Clicked this window's taskbar item
                match self.window_states[window_id] {
                    WindowState::Minimized => {
                        self.restore_window(window_id);
                    },
                    _ => {
                        if self.focused_window == Some(window_id) {
                            // Already focused, minimize it
                            self.minimize_window(window_id);
                        } else {
                            // Bring to front
                            self.bring_to_front(window_id);
                        }
                    }
                }
                return true;
            }
        }
        
        false
    }

    fn is_close_button_clicked(&self, window: &Window, mouse_x: u64, mouse_y: u64) -> bool {
        let button_x = window.x + window.width - 25;
        let button_y = window.y + 5;
        let button_size = 20;

        mouse_x >= button_x && mouse_x < button_x + button_size &&
        mouse_y >= button_y && mouse_y < button_y + button_size
    }

    fn is_maximize_button_clicked(&self, window: &Window, mouse_x: u64, mouse_y: u64) -> bool {
        let button_x = window.x + window.width - 50;
        let button_y = window.y + 5;
        let button_size = 20;

        mouse_x >= button_x && mouse_x < button_x + button_size &&
        mouse_y >= button_y && mouse_y < button_y + button_size
    }

    fn is_minimize_button_clicked(&self, window: &Window, mouse_x: u64, mouse_y: u64) -> bool {
        let button_x = window.x + window.width - 75;
        let button_y = window.y + 5;
        let button_size = 20;

        mouse_x >= button_x && mouse_x < button_x + button_size &&
        mouse_y >= button_y && mouse_y < button_y + button_size
    }

    pub fn handle_drag(&mut self, mouse_x: u64, mouse_y: u64) {
        if let Some(window_id) = self.dragging_window {
            if let Some(ref mut window) = self.windows[window_id] {
                window.x = (mouse_x as i64 - self.drag_offset_x).max(0) as u64;
                window.y = (mouse_y as i64 - self.drag_offset_y).max(TASKBAR_HEIGHT as i64) as u64;
            }
        }
    }

    pub fn release_drag(&mut self) {
        if self.dragging_window.is_some() {
            self.dragging_window = None;
        }
    }

    pub fn draw_all(&self, graphics: &Graphics) {
        // Draw windows in z-order (bottom to top)
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            
            if self.window_states[window_id] == WindowState::Minimized {
                continue;
            }
            
            if let Some(ref window) = self.windows[window_id] {
                if window.visible {
                    let is_focused = self.focused_window == Some(window_id);
                    let is_maximized = self.window_states[window_id] == WindowState::Maximized;
                    
                    self.draw_window_with_controls(graphics, window, is_focused, is_maximized);
                }
            }
        }
    }

    pub fn draw_window(&self, graphics: &Graphics, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.window_states[window_id] == WindowState::Minimized {
            return;
        }

        if let Some(ref window) = self.windows[window_id] {
            if window.visible {
                let is_focused = self.focused_window == Some(window_id);
                let is_maximized = self.window_states[window_id] == WindowState::Maximized;
                self.draw_window_with_controls(graphics, window, is_focused, is_maximized);
            }
        }
    }

    fn draw_window_with_controls(&self, graphics: &Graphics, window: &Window, is_focused: bool, is_maximized: bool) {
        // Layered drop shadow — 4 progressively darker rectangles
        let shadow_cols = [0xFF0C1826u32, 0xFF0A1420u32, 0xFF08101Au32, 0xFF060C14u32];
        for (i, &sc) in shadow_cols.iter().enumerate() {
            let o = (i as u64 + 1) * 2;
            graphics.fill_rect(window.x + o, window.y + o, window.width, window.height, sc);
        }

        // Window body
        graphics.fill_rect(window.x, window.y, window.width, window.height, window.bg_color);

        // Title bar — horizontal gradient
        let (tb_left, tb_right) = if is_focused {
            (0xFF0D5FA0, 0xFF072C50) // vivid blue → deep blue
        } else {
            (0xFF2A2D38, 0xFF1C1F28) // dark slate → darker slate
        };
        graphics.fill_rect_gradient_h(window.x, window.y, window.width, 30, tb_left, tb_right);

        // Thin accent line at bottom of titlebar
        let accent_line = if is_focused { 0xFF00AAFF } else { 0xFF3A3F50 };
        graphics.fill_rect(window.x, window.y + 29, window.width, 1, accent_line);

        // Outer border
        let border_col = if is_focused { 0xFF1A5F9A } else { 0xFF2A2F3E };
        graphics.draw_rect(window.x, window.y, window.width, window.height, border_col, 1);

        // Title text with subtle text-shadow offset (+1,+1)
        let shadow_txt = 0xFF000000;
        let title_color = if is_focused { 0xFFE8F0FE } else { colors::dark_theme::TEXT_SECONDARY };
        fonts::draw_string(graphics, window.x + 11, window.y + 12, window.title, shadow_txt);
        fonts::draw_string(graphics, window.x + 10, window.y + 11, window.title, title_color);

        // Control buttons
        self.draw_close_button(graphics, window);
        self.draw_maximize_button(graphics, window, is_maximized);
        self.draw_minimize_button(graphics, window);
    }

    fn draw_close_button(&self, graphics: &Graphics, window: &Window) {
        let bx = window.x + window.width - 26;
        let by = window.y + 5;
        // Red gradient button
        graphics.fill_rect_gradient_v(bx, by, 20, 20, 0xFFE05050, 0xFFA02020);
        graphics.draw_rect(bx, by, 20, 20, 0xFFFF7070, 1);
        let cx = bx + 10; let cy = by + 10;
        for d in -4i64..=4 {
            graphics.put_pixel_safe(cx as i64 + d, cy as i64 + d, 0xFFFFFFFF);
            graphics.put_pixel_safe(cx as i64 + d, cy as i64 - d, 0xFFFFFFFF);
            // 2px thick
            graphics.put_pixel_safe(cx as i64 + d + 1, cy as i64 + d, 0xFFFFFFFF);
            graphics.put_pixel_safe(cx as i64 + d + 1, cy as i64 - d, 0xFFFFFFFF);
        }
    }

    fn draw_maximize_button(&self, graphics: &Graphics, window: &Window, is_maximized: bool) {
        let bx = window.x + window.width - 50;
        let by = window.y + 5;
        // Green gradient button
        graphics.fill_rect_gradient_v(bx, by, 20, 20, 0xFF38A838, 0xFF1E6E1E);
        graphics.draw_rect(bx, by, 20, 20, 0xFF60E060, 1);
        if is_maximized {
            graphics.draw_rect(bx + 5, by + 7, 7, 7, 0xFFFFFFFF, 1);
            graphics.draw_rect(bx + 8, by + 5, 7, 7, 0xFFFFFFFF, 1);
        } else {
            graphics.draw_rect(bx + 5, by + 5, 10, 10, 0xFFFFFFFF, 2);
        }
    }

    fn draw_minimize_button(&self, graphics: &Graphics, window: &Window) {
        let bx = window.x + window.width - 74;
        let by = window.y + 5;
        // Yellow/amber gradient button
        graphics.fill_rect_gradient_v(bx, by, 20, 20, 0xFFC8A020, 0xFF8A6A10);
        graphics.draw_rect(bx, by, 20, 20, 0xFFFFCC40, 1);
        // Minus icon
        graphics.fill_rect(bx + 5, by + 9, 10, 2, 0xFFFFFFFF);
    }

    pub fn draw_taskbar(&self, graphics: &Graphics) {
        // Gradient taskbar: slightly lighter at top, darker at bottom
        graphics.fill_rect_gradient_v(0, 0, self.screen_width, TASKBAR_HEIGHT,
                                      0xFF252A38, 0xFF161B28);
        // Bright blue accent line at the very bottom
        graphics.fill_rect(0, TASKBAR_HEIGHT - 2, self.screen_width, 2, 0xFF007ACC);

        // OS logo / name — bold with accent colour + small icon dot
        graphics.fill_rect(8, 12, 16, 16, 0xFF007ACC);       // coloured square icon
        graphics.fill_rect(10, 14, 12, 12, 0xFF00AAFF);      // inner lighter square
        graphics.fill_rect(12, 16, 8, 8, 0xFFE8F0FE);        // white core
        fonts::draw_string(graphics, 30, 13, "OxideOS", 0xFF007ACC);
        // thin vertical separator after the name
        graphics.fill_rect(95, 8, 1, 24, 0xFF3A4060);

        // Taskbar window items
        let start_x = 100u64;
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            let item_x = start_x + (i as u64) * (TASKBAR_ITEM_WIDTH + TASKBAR_ITEM_SPACING);
            if let Some(ref window) = self.windows[window_id] {
                self.draw_taskbar_item(graphics, window, window_id, item_x);
            }
        }

        // ── Clock box ──────────────────────────────────────────────────────────
        const CLOCK_CHARS: u64 = 8;
        const CHAR_W:  u64 = 9;
        const CLOCK_W: u64 = CLOCK_CHARS * CHAR_W; // 72
        const BOX_PAD: u64 = 8;
        const BOX_W:   u64 = CLOCK_W + BOX_PAD * 2;  // 88
        let box_x = self.screen_width.saturating_sub(BOX_W + 8);

        // Clock background pill
        graphics.fill_rect_gradient_v(box_x, 5, BOX_W, 30, 0xFF1E2840, 0xFF141C30);
        graphics.draw_rect(box_x, 5, BOX_W, 30, 0xFF007ACC, 1);

        // Clock text
        let mut time_buf = [0u8; 8];
        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        format_uptime(ticks, &mut time_buf);
        let time_str = core::str::from_utf8(&time_buf).unwrap_or("00:00:00");
        fonts::draw_string(graphics, box_x + BOX_PAD, 14, time_str, 0xFF7FC8FF);
    }

    fn draw_taskbar_item(&self, graphics: &Graphics, window: &Window, window_id: usize, x: u64) {
        let is_focused   = self.focused_window == Some(window_id);
        let is_minimized = self.window_states[window_id] == WindowState::Minimized;

        let (top_c, bot_c, border_c) = if is_focused && !is_minimized {
            (0xFF1A6FB0, 0xFF0D4070, 0xFF00AAFF) // bright blue gradient
        } else if is_minimized {
            (0xFF2A2E40, 0xFF1C2030, 0xFF4A5070) // muted dark
        } else {
            (0xFF242840, 0xFF181C2E, 0xFF3A4060) // resting state
        };

        graphics.fill_rect_gradient_v(x, 5, TASKBAR_ITEM_WIDTH, 30, top_c, bot_c);
        graphics.draw_rect(x, 5, TASKBAR_ITEM_WIDTH, 30, border_c, 1);

        // Active indicator: 2-px bottom bar
        if is_focused && !is_minimized {
            graphics.fill_rect(x + 2, 33, TASKBAR_ITEM_WIDTH - 4, 2, 0xFF00D4FF);
        }

        let text_color = if is_focused && !is_minimized { 0xFFE8F4FF }
                         else if is_minimized           { 0xFF6080A0 }
                         else                           { 0xFFB0C0D8 };

        fonts::draw_string(graphics, x + 8, 16, window.title, text_color);
    }

    // ── Context menu ───────────────────────────────────────────────────────────

    /// Show the desktop context menu at (mouse_x, mouse_y).
    /// If the click lands on any visible window, the menu is dismissed instead.
    pub fn handle_right_click(&mut self, mouse_x: u64, mouse_y: u64) {
        // Clicked on a window? Dismiss any open menu and bail.
        for i in (0..self.window_count).rev() {
            let wid = self.z_order[i];
            if self.window_states[wid] == WindowState::Minimized { continue; }
            if let Some(ref w) = self.windows[wid] {
                if w.visible
                    && mouse_x >= w.x && mouse_x < w.x + w.width
                    && mouse_y >= w.y && mouse_y < w.y + w.height
                {
                    self.context_menu_visible = false;
                    return;
                }
            }
        }

        const MENU_W: u64 = 170;
        const MENU_H: u64 = 22 + 6 * 22;
        let mx = if mouse_x + MENU_W > self.screen_width {
            self.screen_width.saturating_sub(MENU_W)
        } else {
            mouse_x
        };
        let my = if mouse_y + MENU_H > self.screen_height {
            self.screen_height.saturating_sub(MENU_H)
        } else {
            mouse_y
        };
        self.context_menu_x       = mx;
        self.context_menu_y       = my;
        self.context_menu_visible = true;
    }

    /// Handle a left-click, checking the context menu first.
    /// Returns `true` if the click was consumed by the context menu.
    pub fn handle_context_menu_click(&mut self, mouse_x: u64, mouse_y: u64) -> bool {
        if !self.context_menu_visible { return false; }

        const MENU_W:   u64   = 170;
        const ITEM_H:   u64   = 22;
        const HEADER_H: u64   = 22;
        const N:        usize = 6;
        let menu_h = HEADER_H + N as u64 * ITEM_H;
        let mx = self.context_menu_x;
        let my = self.context_menu_y;

        // Click outside → dismiss
        if mouse_x < mx || mouse_x >= mx + MENU_W
            || mouse_y < my || mouse_y >= my + menu_h
        {
            self.context_menu_visible = false;
            return false;
        }

        self.context_menu_visible = false; // always close on any click inside

        if mouse_y >= my + HEADER_H {
            let idx = ((mouse_y - my - HEADER_H) / ITEM_H) as usize;
            let styles = [
                BackgroundStyle::Default,
                BackgroundStyle::Sunset,
                BackgroundStyle::Space,
                BackgroundStyle::Aurora,
                BackgroundStyle::Geometric,
                BackgroundStyle::Image,
            ];
            if idx < styles.len() {
                self.background_style = styles[idx];
            }
        }
        true
    }

    /// Draw the context menu on top of everything else.
    pub fn draw_context_menu(&self, graphics: &Graphics) {
        if !self.context_menu_visible { return; }

        const MENU_W:   u64   = 170;
        const ITEM_H:   u64   = 22;
        const HEADER_H: u64   = 22;
        const N:        usize = 6;
        let menu_h = HEADER_H + N as u64 * ITEM_H;
        let mx = self.context_menu_x;
        let my = self.context_menu_y;

        // Drop shadow
        graphics.fill_rect(mx + 3, my + 3, MENU_W, menu_h, 0xFF060810);

        // Background
        graphics.fill_rect(mx, my, MENU_W, menu_h, 0xFF14192A);

        // Header gradient + accent line
        graphics.fill_rect_gradient_h(mx, my, MENU_W, HEADER_H, 0xFF0D5FA0, 0xFF072C50);
        graphics.fill_rect(mx, my + HEADER_H - 1, MENU_W, 1, 0xFF00AAFF);
        fonts::draw_string(graphics, mx + 8, my + 7, "Wallpaper", 0xFFE8F0FE);

        // Border
        graphics.draw_rect(mx, my, MENU_W, menu_h, 0xFF1A5F9A, 1);

        let names  = ["Default", "Sunset", "Space", "Aurora", "Geometric", "Image"];
        let styles = [
            BackgroundStyle::Default,
            BackgroundStyle::Sunset,
            BackgroundStyle::Space,
            BackgroundStyle::Aurora,
            BackgroundStyle::Geometric,
            BackgroundStyle::Image,
        ];

        for i in 0..N {
            let iy = my + HEADER_H + i as u64 * ITEM_H;
            let selected = self.background_style == styles[i];

            // Row background
            if selected {
                graphics.fill_rect(mx + 1, iy, MENU_W - 2, ITEM_H, 0xFF0D3A5A);
            } else if i % 2 == 1 {
                graphics.fill_rect(mx + 1, iy, MENU_W - 2, ITEM_H, 0xFF0E1220);
            }

            // Thin separator (skip first)
            if i > 0 {
                graphics.fill_rect(mx + 8, iy, MENU_W - 16, 1, 0xFF202840);
            }

            let text_col = if selected { 0xFF00D4FF } else { 0xFFB0C8E8 };
            let marker   = if selected { "*" } else { " " };
            fonts::draw_string(graphics, mx + 8,  iy + 7, marker,   text_col);
            fonts::draw_string(graphics, mx + 22, iy + 7, names[i], text_col);
        }
    }

    /// Return the currently selected background style.
    pub fn get_background_style(&self) -> BackgroundStyle {
        self.background_style
    }

    pub fn get_window(&self, window_id: usize) -> Option<&Window> {
        if window_id < MAX_WINDOWS {
            self.windows[window_id].as_ref()
        } else {
            None
        }
    }

    pub fn get_window_mut(&mut self, window_id: usize) -> Option<&mut Window> {
        if window_id < MAX_WINDOWS {
            self.windows[window_id].as_mut()
        } else {
            None
        }
    }

    pub fn get_focused(&self) -> Option<usize> {
        self.focused_window
    }

    pub fn set_focused(&mut self, id: Option<usize>) {
        self.focused_window = id;
    }

    pub fn is_window_visible(&self, window_id: usize) -> bool {
        if window_id >= MAX_WINDOWS {
            return false;
        }

        self.windows[window_id]
            .as_ref()
            .map(|window| window.visible && self.window_states[window_id] != WindowState::Minimized)
            .unwrap_or(false)
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging_window.is_some()
    }
}
