//! Output history ring-buffer.
//!
//! `History` holds up to `HISTORY_CAP` lines.  When full, the oldest line is
//! dropped (shift-left).  `scroll` lets the user page back through output
//! without losing the most recent lines.

use crate::constants::*;
use crate::fixstr::FixStr;

/// A single line of terminal output.
#[derive(Clone, Copy)]
pub struct HistLine {
    pub text:  FixStr<LINE_CAP>,
    pub color: u32,
}

impl HistLine {
    pub const fn empty() -> Self {
        Self { text: FixStr::new(), color: COL_DEFAULT }
    }
}

/// Scrollable ring-buffer of output lines.
pub struct History {
    pub lines:  [HistLine; HISTORY_CAP],
    pub count:  usize,
    pub scroll: usize, // 0 = bottom (most recent), positive = scrolled up
}

impl History {
    pub const fn new() -> Self {
        Self { lines: [HistLine::empty(); HISTORY_CAP], count: 0, scroll: 0 }
    }

    /// Push a line, wrapping long strings across multiple entries.
    pub fn push(&mut self, s: &str, color: u32) {
        let bytes     = s.as_bytes();
        let max_chars = ((WIN_W - PAD_X * 2) / CHAR_W) as usize;
        if bytes.len() <= max_chars {
            self.push_raw(s, color);
        } else {
            let mut start = 0;
            while start < bytes.len() {
                let end = (start + max_chars).min(bytes.len());
                if let Ok(slice) = core::str::from_utf8(&bytes[start..end]) {
                    self.push_raw(slice, color);
                }
                start = end;
            }
        }
    }

    pub fn push_raw(&mut self, s: &str, color: u32) {
        let idx = if self.count < HISTORY_CAP {
            self.count
        } else {
            // Drop oldest line by shifting the buffer left.
            for i in 0..HISTORY_CAP - 1 { self.lines[i] = self.lines[i + 1]; }
            HISTORY_CAP - 1
        };
        self.lines[idx].text.clear();
        self.lines[idx].text.push_str(s);
        self.lines[idx].color = color;
        if self.count < HISTORY_CAP { self.count += 1; }
        if self.scroll > 0 { self.scroll = self.scroll.saturating_sub(1); }
    }

    /// Return the `(text, color)` for visible row `row` given the current scroll offset.
    pub fn get_visible(&self, row: usize) -> (&str, u32) {
        let view_start = self.count.saturating_sub(VISIBLE_LINES + self.scroll);
        let idx = view_start + self.scroll + row;
        if idx >= self.count { return ("", COL_BG); }
        (self.lines[idx].text.as_str(), self.lines[idx].color)
    }

    pub fn scroll_up(&mut self) {
        let max = self.count.saturating_sub(VISIBLE_LINES);
        if self.scroll < max { self.scroll += 4; }
    }

    pub fn scroll_down(&mut self) { self.scroll = self.scroll.saturating_sub(4); }
}
