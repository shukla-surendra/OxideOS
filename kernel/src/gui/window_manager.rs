// src/gui/window_manager.rs - Enhanced with minimize/maximize and taskbar
use super::widgets::Window;
use super::graphics::Graphics;
use super::colors;
use super::fonts;
use crate::kernel::serial::SERIAL_PORT;

const MAX_WINDOWS: usize = 16;
const TASKBAR_HEIGHT: u64 = 40;
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

    fn draw_window_with_controls(&self, graphics: &Graphics, window: &Window, is_focused: bool, is_maximized: bool) {
        // Shadow
        graphics.fill_rect(window.x + 3, window.y + 3, window.width, window.height, 0x30000000);

        // Window background
        graphics.fill_rect(window.x, window.y, window.width, window.height, window.bg_color);

        // Title bar
        let titlebar_color = if is_focused {
            colors::ui::TITLEBAR_ACTIVE
        } else {
            colors::ui::TITLEBAR
        };
        graphics.fill_rect(window.x, window.y, window.width, 30, titlebar_color);

        // Border
        graphics.draw_rect(window.x, window.y, window.width, window.height, colors::dark_theme::BORDER, 1);

        // Title text
        let title_color = if is_focused {
            colors::dark_theme::TEXT_PRIMARY
        } else {
            colors::dark_theme::TEXT_SECONDARY
        };
        fonts::draw_string(graphics, window.x + 10, window.y + 11, window.title, title_color);

        // Control buttons (right to left: close, maximize, minimize)
        self.draw_close_button(graphics, window);
        self.draw_maximize_button(graphics, window, is_maximized);
        self.draw_minimize_button(graphics, window);
    }

    fn draw_close_button(&self, graphics: &Graphics, window: &Window) {
        let button_x = window.x + window.width - 25;
        let button_y = window.y + 5;
        let button_size = 20;

        graphics.fill_rect(button_x, button_y, button_size, button_size, colors::dark_theme::ERROR);
        
        let center_x = button_x + button_size / 2;
        let center_y = button_y + button_size / 2;
        let offset = 5;

        graphics.draw_line(
            (center_x - offset) as i64, (center_y - offset) as i64,
            (center_x + offset) as i64, (center_y + offset) as i64,
            colors::WHITE,
        );
        graphics.draw_line(
            (center_x + offset) as i64, (center_y - offset) as i64,
            (center_x - offset) as i64, (center_y + offset) as i64,
            colors::WHITE,
        );
    }

    fn draw_maximize_button(&self, graphics: &Graphics, window: &Window, is_maximized: bool) {
        let button_x = window.x + window.width - 50;
        let button_y = window.y + 5;
        let button_size = 20;

        graphics.fill_rect(button_x, button_y, button_size, button_size, colors::dark_theme::BUTTON_SECONDARY);
        
        if is_maximized {
            // Draw "restore" icon (two overlapping squares)
            graphics.draw_rect(button_x + 6, button_y + 6, 8, 8, colors::WHITE, 1);
            graphics.draw_rect(button_x + 9, button_y + 9, 8, 8, colors::WHITE, 1);
        } else {
            // Draw maximize icon (single square)
            graphics.draw_rect(button_x + 5, button_y + 5, 10, 10, colors::WHITE, 2);
        }
    }

    fn draw_minimize_button(&self, graphics: &Graphics, window: &Window) {
        let button_x = window.x + window.width - 75;
        let button_y = window.y + 5;
        let button_size = 20;

        graphics.fill_rect(button_x, button_y, button_size, button_size, colors::dark_theme::BUTTON_SECONDARY);
        
        // Draw horizontal line
        graphics.fill_rect(button_x + 5, button_y + button_size / 2, 10, 2, colors::WHITE);
    }

    pub fn draw_taskbar(&self, graphics: &Graphics) {
        // Taskbar background
        graphics.fill_rect(0, 0, self.screen_width, TASKBAR_HEIGHT, colors::dark_theme::SURFACE_VARIANT);
        graphics.draw_rect(0, TASKBAR_HEIGHT - 1, self.screen_width, 1, colors::dark_theme::BORDER, 1);

        // OS name
        fonts::draw_string(graphics, 15, 16, "OxideOS", colors::dark_theme::ACCENT_PRIMARY);

        // Draw taskbar items for each window
        let start_x = 100u64;
        
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            let item_x = start_x + (i as u64) * (TASKBAR_ITEM_WIDTH + TASKBAR_ITEM_SPACING);
            
            if let Some(ref window) = self.windows[window_id] {
                self.draw_taskbar_item(graphics, window, window_id, item_x);
            }
        }
    }

    fn draw_taskbar_item(&self, graphics: &Graphics, window: &Window, window_id: usize, x: u64) {
        let is_focused = self.focused_window == Some(window_id);
        let is_minimized = self.window_states[window_id] == WindowState::Minimized;
        
        let bg_color = if is_focused && !is_minimized {
            colors::dark_theme::ACCENT_PRIMARY
        } else if is_minimized {
            colors::dark_theme::BUTTON_SECONDARY
        } else {
            colors::dark_theme::SURFACE
        };

        // Item background
        graphics.fill_rect(x, 5, TASKBAR_ITEM_WIDTH, 30, bg_color);
        graphics.draw_rect(x, 5, TASKBAR_ITEM_WIDTH, 30, colors::dark_theme::BORDER, 1);

        // Window title (truncated if needed)
        let text_color = if is_focused && !is_minimized {
            colors::WHITE
        } else {
            colors::dark_theme::TEXT_PRIMARY
        };
        
        fonts::draw_string(graphics, x + 8, 16, window.title, text_color);
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

    pub fn is_dragging(&self) -> bool {
        self.dragging_window.is_some()
    }
}