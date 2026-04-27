// src/gui/window_manager.rs - Enhanced with minimize/maximize and taskbar
use super::widgets::Window;
use super::graphics::{Graphics, BackgroundStyle};
use super::colors;
use super::fonts;
use crate::kernel::serial::SERIAL_PORT;

const MAX_WINDOWS: usize = 16;
const TASKBAR_HEIGHT: u64 = 48;
const TITLEBAR_H:    u64 = 34;

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
    SnappedLeft,   // drag-to-left-edge: fills left half of screen
    SnappedRight,  // drag-to-right-edge: fills right half of screen
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
    // ID of the last window closed via the × button (cleared by take_closed_window).
    last_closed_window: Option<usize>,
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
            last_closed_window: None,
        }
    }

    /// Returns the WM window id of the most recently × -closed window, then
    /// clears it.  Called from the main loop to propagate close to gui_proc.
    pub fn take_closed_window(&mut self) -> Option<usize> {
        self.last_closed_window.take()
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
        if window_id >= MAX_WINDOWS { return; }

        if let Some(ref mut window) = self.windows[window_id] {
            match self.window_states[window_id] {
                WindowState::Maximized
                | WindowState::SnappedLeft
                | WindowState::SnappedRight => {
                    // Restore to saved pre-snap/pre-maximize position
                    let (x, y, w, h) = self.saved_positions[window_id];
                    window.x = x; window.y = y; window.width = w; window.height = h;
                    self.window_states[window_id] = WindowState::Normal;
                },
                _ => {
                    self.saved_positions[window_id] = (window.x, window.y, window.width, window.height);
                    window.x = 0;
                    window.y = TASKBAR_HEIGHT;
                    window.width  = self.screen_width;
                    window.height = self.screen_height.saturating_sub(TASKBAR_HEIGHT);
                    self.window_states[window_id] = WindowState::Maximized;
                }
            }
        }
    }

    pub fn restore_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS { return; }
        match self.window_states[window_id] {
            WindowState::Minimized => {
                self.window_states[window_id] = WindowState::Normal;
                self.bring_to_front(window_id);
            },
            WindowState::Maximized
            | WindowState::SnappedLeft
            | WindowState::SnappedRight => {
                self.maximize_window(window_id); // toggles back to Normal
            },
            WindowState::Normal => {}
        }
    }

    /// Snap `window_id` to the left half of the screen (saves current position).
    pub fn snap_left(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS { return; }
        if let Some(ref mut win) = self.windows[window_id] {
            self.saved_positions[window_id] = (win.x, win.y, win.width, win.height);
            win.x = 0;
            win.y = TASKBAR_HEIGHT;
            win.width  = self.screen_width / 2;
            win.height = self.screen_height.saturating_sub(TASKBAR_HEIGHT);
            self.window_states[window_id] = WindowState::SnappedLeft;
        }
    }

    /// Snap `window_id` to the right half of the screen (saves current position).
    pub fn snap_right(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS { return; }
        if let Some(ref mut win) = self.windows[window_id] {
            self.saved_positions[window_id] = (win.x, win.y, win.width, win.height);
            win.x = self.screen_width / 2;
            win.y = TASKBAR_HEIGHT;
            win.width  = self.screen_width / 2;
            win.height = self.screen_height.saturating_sub(TASKBAR_HEIGHT);
            self.window_states[window_id] = WindowState::SnappedRight;
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
                self.last_closed_window = Some(window_id);
                self.remove_window(window_id);
            } else if clicked_maximize_button {
                self.maximize_window(window_id);
            } else if clicked_minimize_button {
                self.minimize_window(window_id);
            } else if clicked_titlebar {
                self.bring_to_front(window_id);
                let state = self.window_states[window_id];
                if state != WindowState::Maximized {
                    // Dragging a snapped window restores it to its saved size first
                    if matches!(state, WindowState::SnappedLeft | WindowState::SnappedRight) {
                        self.restore_window(window_id);
                    }
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
        let bx = window.x + Self::BTN_CLOSE_OX;
        let by = Self::btn_by(window);
        mouse_x >= bx && mouse_x < bx + Self::BTN_SIZE &&
        mouse_y >= by && mouse_y < by + Self::BTN_SIZE
    }

    fn is_minimize_button_clicked(&self, window: &Window, mouse_x: u64, mouse_y: u64) -> bool {
        let bx = window.x + Self::BTN_MIN_OX;
        let by = Self::btn_by(window);
        mouse_x >= bx && mouse_x < bx + Self::BTN_SIZE &&
        mouse_y >= by && mouse_y < by + Self::BTN_SIZE
    }

    fn is_maximize_button_clicked(&self, window: &Window, mouse_x: u64, mouse_y: u64) -> bool {
        let bx = window.x + Self::BTN_MAX_OX;
        let by = Self::btn_by(window);
        mouse_x >= bx && mouse_x < bx + Self::BTN_SIZE &&
        mouse_y >= by && mouse_y < by + Self::BTN_SIZE
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
        if let Some(window_id) = self.dragging_window {
            // Edge snapping (only for Normal windows; already excluded Maximized from dragging)
            if self.window_states[window_id] == WindowState::Normal {
                const SNAP_EDGE: u64 = 20;
                let snap = if let Some(ref w) = self.windows[window_id] {
                    if w.x <= SNAP_EDGE { 1i8 }
                    else if w.x + w.width + SNAP_EDGE >= self.screen_width { 2i8 }
                    else { 0 }
                } else { 0 };
                match snap {
                    1 => self.snap_left(window_id),
                    2 => self.snap_right(window_id),
                    _ => {}
                }
            }
            self.dragging_window = None;
        }
    }

    pub fn z_order_slice(&self) -> &[usize] {
        &self.z_order[..self.window_count]
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
        // Deeper drop shadow for depth
        graphics.draw_soft_shadow(window.x, window.y, window.width, window.height, 18, 0x60);

        // Window body with GNOME-style 12px corner radius
        graphics.fill_rounded_rect(window.x, window.y, window.width, window.height, 12, window.bg_color);

        // Title bar — subtle horizontal gradient, rounded top corners
        let (tb_left, tb_right, accent) = if is_focused {
            (colors::ui::TITLEBAR_FOCUSED_LEFT, colors::ui::TITLEBAR_FOCUSED_RIGHT, colors::ui::TITLEBAR_ACCENT_FOCUSED)
        } else {
            (colors::ui::TITLEBAR_UNFOCUSED_LEFT, colors::ui::TITLEBAR_UNFOCUSED_RIGHT, colors::ui::TITLEBAR_ACCENT_UNFOCUSED)
        };

        graphics.fill_rounded_rect(window.x, window.y, window.width, TITLEBAR_H, 12, tb_left);
        // Flatten bottom portion so titlebar joins content area cleanly
        graphics.fill_rect(window.x, window.y + TITLEBAR_H / 2, window.width, TITLEBAR_H / 2, tb_left);
        graphics.fill_rect_gradient_h(window.x, window.y, window.width, TITLEBAR_H, tb_left, tb_right);

        // Thin accent line at bottom of titlebar (GNOME blue when focused)
        graphics.fill_rect(window.x, window.y + TITLEBAR_H - 1, window.width, 1, accent);

        // Outer border — blue highlight when focused, subtle gray when unfocused
        let border_col = if is_focused { colors::ui::WINDOW_BORDER_FOCUSED } else { colors::ui::WINDOW_BORDER_UNFOCUSED };
        graphics.draw_rounded_rect(window.x, window.y, window.width, window.height, 12, border_col, 1);

        // Title text — centered horizontally, clear of the left-side buttons
        let title_y = window.y + TITLEBAR_H / 2 - 4;
        let title_len_px = window.title.len() as u64 * 9;
        // Center, but never overlap the three left buttons (3×18px + 10 margin = 64px)
        let title_x = {
            let centered = window.x + (window.width.saturating_sub(title_len_px)) / 2;
            centered.max(window.x + 68)
        };
        let title_color = if is_focused { 0xFFEEEEEE } else { 0xFF888888 };
        fonts::draw_string(graphics, title_x + 1, title_y + 1, window.title, 0x80000000);
        fonts::draw_string(graphics, title_x, title_y, window.title, title_color);

        // Control buttons — LEFT side (GNOME style): [×] [-] [+]
        self.draw_close_button(graphics, window);
        self.draw_minimize_button(graphics, window);
        self.draw_maximize_button(graphics, window, is_maximized);
    }

    // ── Window control button helpers ──────────────────────────────────────────
    // Layout: LEFT-aligned (GNOME style), 3 buttons of 14×14, 4px gaps, 10px from left.
    //   Close    at window.x + 10
    //   Minimize at window.x + 28  (10 + 14 + 4)
    //   Maximize at window.x + 46  (28 + 14 + 4)
    // Vertical: centered in TITLEBAR_H=34 → by = window.y + 10
    const BTN_SIZE:   u64 = 14;
    const BTN_RADIUS: u64 = 7;
    const BTN_CLOSE_OX:  u64 = 10; // offset from left edge
    const BTN_MIN_OX:    u64 = 28; // close + size + gap
    const BTN_MAX_OX:    u64 = 46; // min + size + gap

    #[inline(always)]
    fn btn_by(window: &Window) -> u64 { window.y + (TITLEBAR_H - Self::BTN_SIZE) / 2 }

    fn draw_close_button(&self, graphics: &Graphics, window: &Window) {
        let bx = window.x + Self::BTN_CLOSE_OX;
        let by = Self::btn_by(window);
        // GNOME-style red close button
        graphics.fill_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFFED333B);
        graphics.draw_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFFA8222A, 1);
        let cx = bx as i64 + 7; let cy = by as i64 + 7;
        for d in [-2i64, -1, 1, 2] {
            graphics.put_pixel_safe(cx + d, cy + d, 0xFF7A1015);
            graphics.put_pixel_safe(cx + d, cy - d, 0xFF7A1015);
        }
        graphics.put_pixel_safe(cx, cy, 0xFF7A1015);
    }

    fn draw_minimize_button(&self, graphics: &Graphics, window: &Window) {
        let bx = window.x + Self::BTN_MIN_OX;
        let by = Self::btn_by(window);
        // GNOME-style amber minimize button
        graphics.fill_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFFE5A50A);
        graphics.draw_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFFA07008, 1);
        graphics.fill_rect(bx + 4, by + 6, 6, 2, 0xFF6A4A04);
    }

    fn draw_maximize_button(&self, graphics: &Graphics, window: &Window, is_maximized: bool) {
        let bx = window.x + Self::BTN_MAX_OX;
        let by = Self::btn_by(window);
        // GNOME-style green maximize button
        graphics.fill_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFF26A269);
        graphics.draw_rounded_rect(bx, by, Self::BTN_SIZE, Self::BTN_SIZE, Self::BTN_RADIUS, 0xFF186A44, 1);
        let icon_col = 0xFF0A4A28u32;
        if is_maximized {
            graphics.draw_rect(bx + 3, by + 6, 4, 4, icon_col, 1);
            graphics.draw_rect(bx + 6, by + 4, 4, 4, icon_col, 1);
        } else {
            graphics.draw_rect(bx + 4, by + 4, 6, 6, icon_col, 1);
        }
    }

    pub fn draw_taskbar(&self, graphics: &Graphics) {
        // GNOME-style flat dark top panel
        graphics.fill_rect(0, 0, self.screen_width, TASKBAR_HEIGHT, 0xFF2E2E2E);

        // Bottom border line (subtle separator from desktop)
        graphics.fill_rect(0, TASKBAR_HEIGHT - 1, self.screen_width, 1, 0xFF1A1A1A);

        // Separator after Activities button area
        graphics.fill_rect(146, 10, 1, 28, 0x28FFFFFF);

        // Centered clock
        self.draw_center_clock(graphics);

        // Taskbar window items — start after the wider Activities button
        let start_x = 158u64;
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            let item_x = start_x + (i as u64) * (TASKBAR_ITEM_WIDTH + TASKBAR_ITEM_SPACING);
            if let Some(ref window) = self.windows[window_id] {
                self.draw_taskbar_item(graphics, window, window_id, item_x);
            }
        }

        // System tray (network indicator only — clock is now centered)
        self.draw_system_tray(graphics);
    }

    /// Draw the clock in the centre of the taskbar (GNOME top-bar style).
    fn draw_center_clock(&self, graphics: &Graphics) {
        let mut time_buf = [0u8; 8];
        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        format_uptime(ticks, &mut time_buf);
        let time_str = core::str::from_utf8(&time_buf).unwrap_or("00:00:00");
        // "HH:MM:SS" = 8 chars × 9px = 72px wide
        let clock_px: u64 = 72;
        let clock_x = (self.screen_width.saturating_sub(clock_px)) / 2;
        let clock_y = (TASKBAR_HEIGHT - 8) / 2;
        // Subtle pill background
        graphics.fill_rounded_rect(clock_x - 10, clock_y - 5, clock_px + 20, 18, 6, 0xFF383838);
        fonts::draw_string(graphics, clock_x, clock_y, time_str, 0xFFEEEEEE);
    }

    fn draw_system_tray(&self, graphics: &Graphics) {
        // ── Network indicator pill (right edge; clock is now centred) ─────────
        const NET_W:   u64 = 130;
        const NET_H:   u64 = 34;
        const NET_PAD: u64 = 8;
        let net_y     = (TASKBAR_HEIGHT - NET_H) / 2;
        let net_box_x = self.screen_width.saturating_sub(NET_W + 10);

        let net_present = crate::kernel::net::is_present();
        let (dot_col, label, label_col) = if net_present {
            (0xFF26A269u32, "10.0.2.15", 0xFF80D8A0u32)
        } else {
            (0xFFED333Bu32, "No NIC   ", 0xFFAA6060u32)
        };

        graphics.fill_rounded_rect(net_box_x, net_y, NET_W, NET_H, 6, 0xFF383838);
        graphics.draw_rounded_rect(net_box_x, net_y, NET_W, NET_H, 6, 0xFF4A4A4A, 1);

        let dot_x = net_box_x + NET_PAD;
        let dot_y2 = net_y + (NET_H - 8) / 2;
        graphics.fill_rounded_rect(dot_x, dot_y2, 8, 8, 4, dot_col);

        fonts::draw_string(graphics, dot_x + 12, net_y + 7,  label,     label_col);
        fonts::draw_string(graphics, dot_x + 12, net_y + 20, "Network", 0xFF888888);
    }

    fn draw_taskbar_item(&self, graphics: &Graphics, window: &Window, window_id: usize, x: u64) {
        let is_focused   = self.focused_window == Some(window_id);
        let is_minimized = self.window_states[window_id] == WindowState::Minimized;

        const ITEM_Y: u64 = 6;
        const ITEM_H: u64 = 36;

        let (bg_c, border_c, text_c) = if is_focused && !is_minimized {
            (0xFF3A3A3A, 0xFF5294E2, 0xFFEEEEEE) // Focused: GNOME blue border
        } else if is_minimized {
            (0x00000000, 0xFF4A4A4A, 0xFF777777)  // Minimized: dim text
        } else {
            (0x00000000, 0x00000000, 0xFFCCCCCC)  // Normal: transparent bg
        };

        if bg_c != 0 {
            graphics.fill_rounded_rect(x, ITEM_Y, TASKBAR_ITEM_WIDTH, ITEM_H, 6, bg_c);
        }
        if border_c != 0 {
            graphics.draw_rounded_rect(x, ITEM_Y, TASKBAR_ITEM_WIDTH, ITEM_H, 6, border_c, 1);
        }

        // Bottom indicator — GNOME blue pill for active, small dot for open
        if is_focused && !is_minimized {
            let ind_w = 28u64;
            let ind_x = x + (TASKBAR_ITEM_WIDTH - ind_w) / 2;
            graphics.fill_rounded_rect(ind_x, TASKBAR_HEIGHT - 4, ind_w, 3, 1, 0xFF5294E2);
        } else if !is_minimized {
            let ind_x = x + TASKBAR_ITEM_WIDTH / 2 - 2;
            graphics.fill_rounded_rect(ind_x, TASKBAR_HEIGHT - 4, 4, 3, 1, 0xFF5294E2);
        }

        fonts::draw_string(graphics, x + 12, ITEM_Y + 14, window.title, text_c);
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
        if window_id >= MAX_WINDOWS { return false; }
        self.windows[window_id]
            .as_ref()
            .map(|window| window.visible && self.window_states[window_id] != WindowState::Minimized)
            .unwrap_or(false)
    }

    pub fn is_dragging(&self) -> bool { self.dragging_window.is_some() }

    pub fn get_screen_dimensions(&self) -> (u64, u64) {
        (self.screen_width, self.screen_height)
    }
}
