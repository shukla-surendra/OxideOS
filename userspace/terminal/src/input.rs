//! Raw keyboard input: escape sequences, command history navigation, startup banner.

use oxide_rt::{getchar, sleep_ms};
use crate::constants::*;
use crate::terminal::Terminal;

// ── Escape-sequence handling ──────────────────────────────────────────────────

fn blocking_getchar() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(5);
    }
}

/// Consume the rest of an `ESC [` sequence and dispatch arrow / page keys.
pub fn handle_escape(term: &mut Terminal) {
    if blocking_getchar() != b'[' { return; }
    match blocking_getchar() {
        b'A' => history_up(term),
        b'B' => history_down(term),
        b'C' => { let n = term.input.len; if term.cursor < n { term.cursor += 1; } }
        b'D' => { if term.cursor > 0 { term.cursor -= 1; } }
        b'5' => { let _ = blocking_getchar(); term.history.scroll_up();   term.dirty = true; }
        b'6' => { let _ = blocking_getchar(); term.history.scroll_down(); term.dirty = true; }
        _    => {}
    }
}

// ── Command history navigation ────────────────────────────────────────────────

pub fn history_up(term: &mut Terminal) {
    if term.cmd_count == 0 { return; }
    let next = match term.cmd_cursor {
        None    => term.cmd_count - 1,
        Some(0) => 0,
        Some(i) => i - 1,
    };
    term.cmd_cursor = Some(next);
    term.input.clear();
    term.input.push_str(term.cmd_hist[next].as_str());
    term.cursor = term.input.len;
}

pub fn history_down(term: &mut Terminal) {
    match term.cmd_cursor {
        None => {}
        Some(i) if i + 1 >= term.cmd_count => {
            term.cmd_cursor = None;
            term.input.clear();
            term.cursor = 0;
        }
        Some(i) => {
            term.cmd_cursor = Some(i + 1);
            term.input.clear();
            term.input.push_str(term.cmd_hist[i + 1].as_str());
            term.cursor = term.input.len;
        }
    }
}

// ── Startup banner ────────────────────────────────────────────────────────────

pub fn print_banner(term: &mut Terminal) {
    term.log("", COL_DEFAULT);
    term.log(" +--------------------------------------------------+", 0xFF007ACC);
    term.log(" |                                                  |", 0xFF007ACC);
    term.log(" |   ###  #  #  ###  ###   ##   ###  ###           |", 0xFF00AAFF);
    term.log(" |   # #   ##   # #  #  # #  #  #    #            |", 0xFF00AAFF);
    term.log(" |   # #  #  #  # #  ###  ####  ##   ##           |", 0xFF0088CC);
    term.log(" |   # #  ####  # #  #    #  #  #    #            |", 0xFF0088CC);
    term.log(" |   ###  #  #  ###  #    #  #  ###  ###          |", 0xFF006699);
    term.log(" |                                                  |", 0xFF007ACC);
    term.log(" |         Hobby OS written in Rust                |", 0xFF4A6080);
    term.log(" |                     v0.1.0-dev                  |", 0xFF4A6080);
    term.log(" |                                                  |", 0xFF007ACC);
    term.log(" +--------------------------------------------------+", 0xFF007ACC);
    term.log("", COL_DEFAULT);
    term.log("  Type 'help' for commands  |  'programs' to list apps", COL_DIM);
    term.log("  Tab-completion and up/down arrow history supported", COL_DIM);
    term.log("", COL_DEFAULT);
}
