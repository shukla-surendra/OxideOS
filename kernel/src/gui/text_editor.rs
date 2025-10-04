// src/gui/text_editor.rs - Complete corrected version

use super::graphics::Graphics;
use super::colors;
use super::fonts;

const MAX_LINES: usize = 30;
const MAX_LINE_LENGTH: usize = 80;

pub struct TextEditor {
    pub x: u64,
    pub y: u64,
    pub width: u64,
    pub height: u64,
    pub title: &'static str,
    pub visible: bool,
    
    // Text buffer
    lines: [[u8; MAX_LINE_LENGTH]; MAX_LINES],
    line_lengths: [usize; MAX_LINES],
    total_lines: usize,
    
    // Cursor position
    cursor_line: usize,
    cursor_col: usize,
    
    // View position (for scrolling)
    scroll_offset: usize,
    
    // Colors
    bg_color: u32,
    text_color: u32,
    cursor_color: u32,
}

impl TextEditor {
    pub fn new(x: u64, y: u64, width: u64, height: u64, title: &'static str) -> Self {
        Self {
            x,
            y,
            width,
            height,
            title,
            visible: true,
            lines: [[b' '; MAX_LINE_LENGTH]; MAX_LINES],
            line_lengths: [0; MAX_LINES],
            total_lines: 1, // Start with one empty line
            cursor_line: 0,
            cursor_col: 0,
            scroll_offset: 0,
            bg_color: colors::dark_theme::SURFACE,
            text_color: colors::dark_theme::TEXT_PRIMARY,
            cursor_color: colors::dark_theme::ACCENT_PRIMARY,
        }
    }

    pub fn draw(&self, graphics: &Graphics) {
        if !self.visible {
            return;
        }

        // Shadow
        graphics.fill_rect(self.x + 3, self.y + 3, self.width, self.height, 0x30000000);

        // Window background
        graphics.fill_rect(self.x, self.y, self.width, self.height, self.bg_color);

        // Title bar
        graphics.fill_rect(self.x, self.y, self.width, 30, colors::ui::TITLEBAR_ACTIVE);
        graphics.draw_rect(self.x, self.y, self.width, self.height, colors::dark_theme::BORDER, 1);

        // Title text
        fonts::draw_string(graphics, self.x + 10, self.y + 11, self.title, colors::WHITE);

        // Text area background (slightly darker)
        let text_area_y = self.y + 30;
        let text_area_height = self.height - 30;
        graphics.fill_rect(self.x, text_area_y, self.width, text_area_height, colors::dark_theme::BACKGROUND);

        // Draw text content
        self.draw_text(graphics);

        // Draw cursor
        self.draw_cursor(graphics);

        // Draw status bar at bottom
        self.draw_status_bar(graphics);
    }

    fn draw_text(&self, graphics: &Graphics) {
        let char_width = 9;
        let char_height = 16;
        let start_x = self.x + 10;
        let start_y = self.y + 40;

        let visible_lines = ((self.height - 70) / char_height).min(MAX_LINES as u64) as usize;

        for line_idx in 0..visible_lines {
            let actual_line = self.scroll_offset + line_idx;
            
            if actual_line >= self.total_lines {
                break;
            }

            let line = &self.lines[actual_line][..self.line_lengths[actual_line]];
            let y = start_y + (line_idx as u64 * char_height);

            // Draw line number
            let line_num = actual_line + 1;
            draw_line_number(graphics, self.x + 5, y, line_num, colors::dark_theme::TEXT_DISABLED);

            // Draw text
            for (col, &ch) in line.iter().enumerate() {
                let x = start_x + (col as u64 * char_width);
                if ch != b' ' && ch >= 32 && ch < 127 {
                    fonts::draw_char(graphics, x, y, ch as char, self.text_color);
                }
            }
        }
    }

    fn draw_cursor(&self, graphics: &Graphics) {
        // Only draw cursor if it's in visible area
        if self.cursor_line < self.scroll_offset {
            return;
        }

        let visible_line = self.cursor_line - self.scroll_offset;
        let visible_lines = ((self.height - 70) / 16) as usize;
        
        if visible_line >= visible_lines {
            return;
        }

        let char_width = 9;
        let char_height = 16;
        let cursor_x = self.x + 10 + (self.cursor_col as u64 * char_width);
        let cursor_y = self.y + 40 + (visible_line as u64 * char_height);

        // Draw blinking cursor (simple block for now)
        graphics.fill_rect(cursor_x, cursor_y, 2, char_height, self.cursor_color);
    }

    fn draw_status_bar(&self, graphics: &Graphics) {
        let status_y = self.y + self.height - 20;
        
        graphics.fill_rect(self.x, status_y, self.width, 20, colors::dark_theme::SURFACE_VARIANT);
        
        // Status text: Line, Column, Total Lines
        draw_status(graphics, self.x + 10, status_y + 6, 
                   self.cursor_line + 1, self.cursor_col + 1, self.total_lines,
                   colors::dark_theme::TEXT_SECONDARY);
    }

    /// Handle keyboard character input
    pub fn input_char(&mut self, ch: u8) {
        match ch {
            b'\n' | b'\r' => self.insert_newline(),
            8 | 127 => self.backspace(), // Backspace or DEL
            b'\t' => {
                // Insert 4 spaces for tab
                for _ in 0..4 {
                    self.input_char(b' ');
                }
            }
            32..=126 => { // Printable ASCII
                if self.cursor_col < MAX_LINE_LENGTH {
                    let line = &mut self.lines[self.cursor_line];
                    
                    // Insert character
                    if self.cursor_col < self.line_lengths[self.cursor_line] {
                        // Insert in middle - shift characters right
                        for i in (self.cursor_col..self.line_lengths[self.cursor_line]).rev() {
                            if i + 1 < MAX_LINE_LENGTH {
                                line[i + 1] = line[i];
                            }
                        }
                    }
                    
                    line[self.cursor_col] = ch;
                    
                    if self.cursor_col >= self.line_lengths[self.cursor_line] {
                        self.line_lengths[self.cursor_line] = self.cursor_col + 1;
                    } else {
                        self.line_lengths[self.cursor_line] += 1;
                    }
                    
                    self.cursor_col += 1;
                }
            }
            _ => {} // Ignore control characters
        }
    }

    fn insert_newline(&mut self) {
        if self.total_lines >= MAX_LINES {
            return; // Buffer full
        }

        // Split current line at cursor
        let current_len = self.line_lengths[self.cursor_line];
        
        // Shift all lines below down by one
        for i in (self.cursor_line + 1..self.total_lines).rev() {
            if i + 1 < MAX_LINES {
                self.lines[i + 1] = self.lines[i];
                self.line_lengths[i + 1] = self.line_lengths[i];
            }
        }
        
        // Create new line with text after cursor
        if self.cursor_line + 1 < MAX_LINES {
            let new_line_idx = self.cursor_line + 1;
            
            // Copy text after cursor to new line
            let chars_to_copy = current_len.saturating_sub(self.cursor_col);
            for i in 0..chars_to_copy {
                self.lines[new_line_idx][i] = self.lines[self.cursor_line][self.cursor_col + i];
            }
            self.line_lengths[new_line_idx] = chars_to_copy;
            
            // Truncate current line at cursor
            self.line_lengths[self.cursor_line] = self.cursor_col;
            
            // Move cursor to start of new line
            self.cursor_line += 1;
            self.cursor_col = 0;
            self.total_lines += 1;
            
            // Auto-scroll if needed
            let visible_lines = ((self.height - 70) / 16) as usize;
            if self.cursor_line >= self.scroll_offset + visible_lines {
                self.scroll_offset += 1;
            }
        }
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            // Delete character before cursor
            self.cursor_col -= 1;
            
            let line = &mut self.lines[self.cursor_line];
            let len = self.line_lengths[self.cursor_line];
            
            // Shift characters left
            for i in self.cursor_col..len.saturating_sub(1) {
                line[i] = line[i + 1];
            }
            
            if len > 0 {
                line[len - 1] = b' ';
                self.line_lengths[self.cursor_line] = len - 1;
            }
        } else if self.cursor_line > 0 {
            // Join with previous line
            let prev_line = self.cursor_line - 1;
            let prev_len = self.line_lengths[prev_line];
            
            // Move cursor to end of previous line
            self.cursor_col = prev_len;
            
            // Copy current line to previous line
            let current_len = self.line_lengths[self.cursor_line];
            for i in 0..current_len {
                if prev_len + i < MAX_LINE_LENGTH {
                    self.lines[prev_line][prev_len + i] = self.lines[self.cursor_line][i];
                }
            }
            
            self.line_lengths[prev_line] = (prev_len + current_len).min(MAX_LINE_LENGTH);
            
            // Shift all lines up
            for i in self.cursor_line..self.total_lines - 1 {
                self.lines[i] = self.lines[i + 1];
                self.line_lengths[i] = self.line_lengths[i + 1];
            }
            
            self.cursor_line = prev_line;
            self.total_lines -= 1;
            
            // Clear last line
            self.lines[self.total_lines] = [b' '; MAX_LINE_LENGTH];
            self.line_lengths[self.total_lines] = 0;
        }
    }

    /// Handle arrow keys
    pub fn move_cursor_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.cursor_col.min(self.line_lengths[self.cursor_line]);
            
            // Scroll up if needed
            if self.cursor_line < self.scroll_offset {
                self.scroll_offset = self.cursor_line;
            }
        }
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor_line + 1 < self.total_lines {
            self.cursor_line += 1;
            self.cursor_col = self.cursor_col.min(self.line_lengths[self.cursor_line]);
            
            // Scroll down if needed
            let visible_lines = ((self.height - 70) / 16) as usize;
            if self.cursor_line >= self.scroll_offset + visible_lines {
                self.scroll_offset += 1;
            }
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.line_lengths[self.cursor_line];
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_col < self.line_lengths[self.cursor_line] {
            self.cursor_col += 1;
        } else if self.cursor_line + 1 < self.total_lines {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }

    pub fn is_titlebar_clicked(&self, mouse_x: u64, mouse_y: u64) -> bool {
        mouse_x >= self.x && mouse_x < self.x + self.width &&
        mouse_y >= self.y && mouse_y < self.y + 30
    }

    pub fn get_text(&self) -> [u8; MAX_LINES * MAX_LINE_LENGTH] {
        let mut result = [0u8; MAX_LINES * MAX_LINE_LENGTH];
        let mut pos = 0;
        
        for line_idx in 0..self.total_lines {
            let line_len = self.line_lengths[line_idx];
            for i in 0..line_len {
                if pos < result.len() {
                    result[pos] = self.lines[line_idx][i];
                    pos += 1;
                }
            }
            if pos < result.len() && line_idx + 1 < self.total_lines {
                result[pos] = b'\n';
                pos += 1;
            }
        }
        
        result
    }
}

// Helper functions (no std, so we draw directly)
fn draw_line_number(graphics: &Graphics, x: u64, y: u64, num: usize, color: u32) {
    let mut temp = num;
    let mut digits = [0u8; 4];
    let mut count = 0;
    
    if temp == 0 {
        fonts::draw_char(graphics, x, y, '0', color);
        return;
    }
    
    while temp > 0 && count < 4 {
        digits[count] = (temp % 10) as u8;
        temp /= 10;
        count += 1;
    }
    
    let mut current_x = x;
    for i in (0..count).rev() {
        let ch = (b'0' + digits[i]) as char;
        fonts::draw_char(graphics, current_x, y, ch, color);
        current_x += 9;
    }
}

fn draw_status(graphics: &Graphics, x: u64, y: u64, line: usize, col: usize, _total: usize, color: u32) {
    let mut current_x = x;
    
    // "Ln "
    fonts::draw_char(graphics, current_x, y, 'L', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, 'n', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, ' ', color);
    current_x += 9;
    
    // Line number
    current_x = draw_number(graphics, current_x, y, line, color);
    
    fonts::draw_char(graphics, current_x, y, ',', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, ' ', color);
    current_x += 9;
    
    // "Col "
    fonts::draw_char(graphics, current_x, y, 'C', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, 'o', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, 'l', color);
    current_x += 9;
    fonts::draw_char(graphics, current_x, y, ' ', color);
    current_x += 9;
    
    // Column number
    draw_number(graphics, current_x, y, col, color);
}

fn draw_number(graphics: &Graphics, x: u64, y: u64, mut num: usize, color: u32) -> u64 {
    if num == 0 {
        fonts::draw_char(graphics, x, y, '0', color);
        return x + 9;
    }
    
    let mut digits = [0u8; 10];
    let mut count = 0;
    
    while num > 0 {
        digits[count] = (num % 10) as u8;
        num /= 10;
        count += 1;
    }
    
    let mut current_x = x;
    for i in (0..count).rev() {
        let ch = (b'0' + digits[i]) as char;
        fonts::draw_char(graphics, current_x, y, ch, color);
        current_x += 9;
    }
    
    current_x
}