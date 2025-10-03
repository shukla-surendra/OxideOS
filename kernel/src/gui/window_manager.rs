// src/gui/window_manager.rs
use super::widgets::Window;
use super::graphics::Graphics;
use super::colors;
use crate::kernel::serial::SERIAL_PORT;

const MAX_WINDOWS: usize = 16;

pub struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    z_order: [usize; MAX_WINDOWS],  // Indices into windows array, sorted by z-order
    window_count: usize,
    focused_window: Option<usize>,  // Index of focused window
    dragging_window: Option<usize>, // Index of window being dragged
    drag_offset_x: i64,
    drag_offset_y: i64,
}

impl WindowManager {
    pub const fn new() -> Self {
        Self {
            windows: [None; MAX_WINDOWS],
            z_order: [0; MAX_WINDOWS],
            window_count: 0,
            focused_window: None,
            dragging_window: None,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Add a new window and return its ID
    pub fn add_window(&mut self, window: Window) -> Option<usize> {
        if self.window_count >= MAX_WINDOWS {
            unsafe {
                SERIAL_PORT.write_str("WindowManager: Maximum windows reached\n");
            }
            return None;
        }

        // Find first empty slot
        for i in 0..MAX_WINDOWS {
            if self.windows[i].is_none() {
                self.windows[i] = Some(window);
                
                // Add to z-order at the top (end of array)
                self.z_order[self.window_count] = i;
                self.window_count += 1;
                
                // Set as focused window
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

    /// Remove a window by ID
    pub fn remove_window(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.windows[window_id].is_none() {
            return;
        }

        self.windows[window_id] = None;
        
        // Remove from z-order
        let mut found_idx = None;
        for i in 0..self.window_count {
            if self.z_order[i] == window_id {
                found_idx = Some(i);
                break;
            }
        }
        
        if let Some(idx) = found_idx {
            // Shift remaining windows down
            for i in idx..self.window_count - 1 {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.window_count -= 1;
        }
        
        // Clear focus if this was focused
        if self.focused_window == Some(window_id) {
            self.focused_window = if self.window_count > 0 {
                Some(self.z_order[self.window_count - 1])
            } else {
                None
            };
        }
        
        // Clear dragging state
        if self.dragging_window == Some(window_id) {
            self.dragging_window = None;
        }
    }

    /// Bring window to front (highest z-index)
    pub fn bring_to_front(&mut self, window_id: usize) {
        if window_id >= MAX_WINDOWS || self.windows[window_id].is_none() {
            return;
        }

        // Find window in z-order
        let mut current_pos = None;
        for i in 0..self.window_count {
            if self.z_order[i] == window_id {
                current_pos = Some(i);
                break;
            }
        }

        if let Some(pos) = current_pos {
            // Already at front
            if pos == self.window_count - 1 {
                return;
            }

            // Shift windows down and move this one to end
            for i in pos..self.window_count - 1 {
                self.z_order[i] = self.z_order[i + 1];
            }
            self.z_order[self.window_count - 1] = window_id;
            
            self.focused_window = Some(window_id);
        }
    }

    /// Handle mouse click - returns true if any window was clicked
/// Handle mouse click - returns true if any window was clicked
pub fn handle_click(&mut self, mouse_x: u64, mouse_y: u64) -> bool {
    // First pass: check what was clicked (read-only)
    let mut clicked_window: Option<usize> = None;
    let mut clicked_close_button = false;
    let mut clicked_titlebar = false;
    let mut drag_offset_x = 0i64;
    let mut drag_offset_y = 0i64;

    // Check from top to bottom (reverse z-order)
    for i in (0..self.window_count).rev() {
        let window_id = self.z_order[i];
        
        if let Some(ref window) = self.windows[window_id] {
            if !window.visible {
                continue;
            }

            // Check close button first
            if window.is_close_button_clicked(mouse_x, mouse_y) {
                clicked_window = Some(window_id);
                clicked_close_button = true;
                break;
            }

            // Check title bar for dragging
            if window.is_titlebar_clicked(mouse_x, mouse_y) {
                clicked_window = Some(window_id);
                clicked_titlebar = true;
                drag_offset_x = mouse_x as i64 - window.x as i64;
                drag_offset_y = mouse_y as i64 - window.y as i64;
                break;
            }

            // Check if clicked anywhere in window
            if mouse_x >= window.x && mouse_x < window.x + window.width &&
               mouse_y >= window.y && mouse_y < window.y + window.height {
                clicked_window = Some(window_id);
                break;
            }
        }
    }

    // Second pass: perform actions based on what was clicked
    if let Some(window_id) = clicked_window {
        if clicked_close_button {
            unsafe {
                SERIAL_PORT.write_str("WindowManager: Closing window ");
                SERIAL_PORT.write_decimal(window_id as u32);
                SERIAL_PORT.write_str("\n");
            }
            self.remove_window(window_id);
        } else if clicked_titlebar {
            self.bring_to_front(window_id);
            self.dragging_window = Some(window_id);
            self.drag_offset_x = drag_offset_x;
            self.drag_offset_y = drag_offset_y;
            
            unsafe {
                SERIAL_PORT.write_str("WindowManager: Started dragging window ");
                SERIAL_PORT.write_decimal(window_id as u32);
                SERIAL_PORT.write_str("\n");
            }
        } else {
            self.bring_to_front(window_id);
        }
        
        return true;
    }
    
    false
}

    /// Handle mouse drag
    pub fn handle_drag(&mut self, mouse_x: u64, mouse_y: u64) {
        if let Some(window_id) = self.dragging_window {
            if let Some(ref mut window) = self.windows[window_id] {
                window.x = (mouse_x as i64 - self.drag_offset_x).max(0) as u64;
                window.y = (mouse_y as i64 - self.drag_offset_y).max(0) as u64;
            }
        }
    }

    /// Stop dragging
    pub fn release_drag(&mut self) {
        if self.dragging_window.is_some() {
            unsafe {
                SERIAL_PORT.write_str("WindowManager: Stopped dragging\n");
            }
            self.dragging_window = None;
        }
    }

    /// Draw all windows in z-order (bottom to top)
    pub fn draw_all(&self, graphics: &Graphics) {
        for i in 0..self.window_count {
            let window_id = self.z_order[i];
            
            if let Some(ref window) = self.windows[window_id] {
                if window.visible {
                    // Highlight focused window
                    let is_focused = self.focused_window == Some(window_id);
                    
                    if is_focused {
                        window.draw(graphics);
                    } else {
                        // Draw unfocused window with dimmed title bar
                        window.draw_unfocused(graphics);
                    }
                }
            }
        }
    }

    /// Get window by ID
    pub fn get_window(&self, window_id: usize) -> Option<&Window> {
        if window_id < MAX_WINDOWS {
            self.windows[window_id].as_ref()
        } else {
            None
        }
    }

    /// Get mutable window by ID
    pub fn get_window_mut(&mut self, window_id: usize) -> Option<&mut Window> {
        if window_id < MAX_WINDOWS {
            self.windows[window_id].as_mut()
        } else {
            None
        }
    }

    /// Get focused window ID
    pub fn get_focused(&self) -> Option<usize> {
        self.focused_window
    }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.dragging_window.is_some()
    }
}