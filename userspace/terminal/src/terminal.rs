//! `Terminal` — top-level application state.
//!
//! Owns the output history, the current input line, and the command history
//! for up/down-arrow navigation.  Logging helpers live here; rendering lives
//! in `draw.rs` and command dispatch in `commands.rs`.

use crate::constants::*;
use crate::fixstr::FixStr;
use crate::history::History;

pub struct Terminal {
    pub history:    History,
    pub input:      FixStr<256>,
    pub cursor:     usize,           // byte offset into `input`
    pub cmd_hist:   [FixStr<256>; CMD_HIST_CAP],
    pub cmd_count:  usize,
    pub cmd_cursor: Option<usize>,   // None = editing fresh input
    pub dirty:      bool,
}

impl Terminal {
    pub const fn new() -> Self {
        const EMPTY: FixStr<256> = FixStr::new();
        Self {
            history:    History::new(),
            input:      FixStr::new(),
            cursor:     0,
            cmd_hist:   [EMPTY; CMD_HIST_CAP],
            cmd_count:  0,
            cmd_cursor: None,
            dirty:      true,
        }
    }

    /// Append `s` to the output history with the given color.
    pub fn log(&mut self, s: &str, color: u32) {
        self.history.push(s, color);
        self.dirty = true;
    }

    /// Append raw output, splitting on newlines.
    pub fn log_output(&mut self, raw: &str) {
        let mut start = 0;
        for (i, &b) in raw.as_bytes().iter().enumerate() {
            if b == b'\n' {
                self.log(&raw[start..i], COL_DEFAULT);
                start = i + 1;
            }
        }
        if start < raw.len() {
            self.log(&raw[start..], COL_DEFAULT);
        }
    }
}
