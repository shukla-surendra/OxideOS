//! OxideOS Userspace Terminal — entry point and event loop.
//!
//! Module map
//! ─────────────────────────────────────────────────────────────────────────────
//! constants   Layout, colors, and program list.
//! fixstr      `FixStr<N>` — heap-free fixed-capacity string.
//! history     `HistLine` + `History` — scrollable output ring-buffer.
//! terminal    `Terminal` struct — owns history, input line, cmd history.
//! fmt         Number-to-string helpers shared by draw and commands.
//! draw        Compositor drawing: status bar, history, input area.
//! commands    Tab-complete, built-ins, fork/exec, pipes, submit.
//! input       Escape-sequence handling, history navigation, banner.
//! ─────────────────────────────────────────────────────────────────────────────
#![no_std]
#![no_main]

mod constants;
mod fixstr;
mod history;
mod terminal;
mod fmt;
mod draw;
mod commands;
mod input;

use oxide_rt::{getchar, sleep_ms, getpid, comp_present, msgq_create, COMPOSITOR_QID};
use terminal::Terminal;
use draw::{redraw_full, draw_input_area};
use commands::{submit, tab_complete};
use input::{handle_escape, print_banner};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    msgq_create(COMPOSITOR_QID);

    let my_pid  = getpid();
    let mut term = Terminal::new();

    print_banner(&mut term);
    redraw_full(&term, my_pid);
    term.dirty = false;

    loop {
        let Some(c) = getchar() else {
            // No input this tick — always push a fresh frame so the compositor
            // has content to overlay after window drag/resize clears the area.
            redraw_full(&term, my_pid);
            term.dirty = false;
            sleep_ms(10);
            continue;
        };

        match c {
            b'\n' | b'\r' => {
                submit(&mut term);
                redraw_full(&term, my_pid);
                term.dirty = false;
            }
            b'\t' => {
                tab_complete(&mut term);
                if term.dirty { redraw_full(&term, my_pid); term.dirty = false; }
                else          { draw_input_area(&term); comp_present(); }
            }
            // Backspace / DEL
            8 | 127 => {
                if term.cursor > 0 {
                    term.cursor -= 1;
                    term.input.remove(term.cursor);
                    draw_input_area(&term); comp_present();
                }
            }
            // Ctrl-C — cancel input
            3 => {
                term.input.clear(); term.cursor = 0; term.cmd_cursor = None;
                draw_input_area(&term); comp_present();
            }
            // Ctrl-L — clear screen
            12 => {
                term.history.count  = 0;
                term.history.scroll = 0;
                redraw_full(&term, my_pid);
            }
            // ESC sequence (arrow / page keys)
            0x1B => {
                handle_escape(&mut term);
                if term.dirty { redraw_full(&term, my_pid); term.dirty = false; }
                else          { draw_input_area(&term); comp_present(); }
            }
            // Printable ASCII
            c if c >= 32 && c < 127 => {
                term.input.insert(term.cursor, c);
                term.cursor += 1;
                term.cmd_cursor = None;
                draw_input_area(&term); comp_present();
            }
            _ => {}
        }
    }
}
