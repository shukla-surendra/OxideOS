//! Compositor drawing — stateless rendering functions that take `&Terminal`.
//!
//! Each function draws exactly one UI region and is called from `redraw_full`
//! or directly for partial updates (input area only, etc.).

use oxide_rt::{comp_fill_rect, comp_draw_text, comp_present, get_time};
use crate::constants::*;
use crate::fmt::{fmt_u32, fmt_i64};
use crate::terminal::Terminal;

// ── Primitives ────────────────────────────────────────────────────────────────

pub fn draw_hline(y: u32, color: u32) {
    comp_fill_rect(0, y, WIN_W, 1, color);
}

// ── Full redraw ───────────────────────────────────────────────────────────────

/// Clear the window and redraw every region.
pub fn redraw_full(term: &Terminal, pid: u32) {
    comp_fill_rect(0, 0, WIN_W, WIN_H, COL_BG);
    draw_status_bar(pid);
    draw_history(term);
    draw_input_area(term);
    comp_present();
}

// ── Status bar ────────────────────────────────────────────────────────────────

pub fn draw_status_bar(pid: u32) {
    comp_fill_rect(0, 0, WIN_W, STATUS_H + PAD_Y + 2, COL_STATUS_BG);
    draw_hline(STATUS_H + PAD_Y + 2, COL_ACCENT);
    comp_draw_text(PAD_X, PAD_Y + 1, COL_STATUS_FG, " OxideOS Terminal");

    // Right-side pid + uptime
    let ticks = get_time();
    let secs  = ticks / 100;
    let h = (secs / 3600) % 100;
    let m = (secs / 60)   % 60;
    let s = secs           % 60;

    let mut rbuf = [0u8; 48];
    let mut rlen = 0usize;

    macro_rules! push_str {
        ($s:expr) => { for b in $s.bytes() { if rlen < rbuf.len() { rbuf[rlen] = b; rlen += 1; } } };
    }
    macro_rules! push_d2 {
        ($v:expr) => {
            if rlen < rbuf.len() { rbuf[rlen] = b'0' + ($v / 10) as u8; rlen += 1; }
            if rlen < rbuf.len() { rbuf[rlen] = b'0' + ($v % 10) as u8; rlen += 1; }
        };
    }
    macro_rules! push_u64 {
        ($v:expr) => {{
            let mut b16 = [0u8; 16];
            let s = fmt_u32(&mut b16, $v as u32);
            push_str!(s);
        }};
    }

    push_str!("pid:"); push_u64!(pid);
    push_str!("  up:"); push_d2!(h); rbuf[rlen] = b':'; rlen += 1;
    push_d2!(m);        rbuf[rlen] = b':'; rlen += 1; push_d2!(s);

    let rstr = core::str::from_utf8(&rbuf[..rlen]).unwrap_or("");
    let rx = WIN_W.saturating_sub(rlen as u32 * CHAR_W + PAD_X);
    comp_draw_text(rx, PAD_Y + 1, 0xFF2A4A6A, rstr);
}

// ── History area ──────────────────────────────────────────────────────────────

pub fn draw_history(term: &Terminal) {
    comp_fill_rect(0, HIST_Y, WIN_W, HIST_H, COL_BG);

    for row in 0..VISIBLE_LINES {
        let y = HIST_Y + row as u32 * LINE_H;
        if y + LINE_H > HIST_Y + HIST_H { break; }
        let (text, color) = term.history.get_visible(row);
        if !text.is_empty() { comp_draw_text(PAD_X, y, color, text); }
    }

    if term.history.scroll > 0 {
        comp_draw_text(WIN_W - 3 * CHAR_W - 2, HIST_Y, COL_WARN, "^^^");
    }
}

// ── Input area ────────────────────────────────────────────────────────────────

pub fn draw_input_area(term: &Terminal) {
    let strip_y = WIN_H - INPUT_H - 2;
    comp_fill_rect(0, strip_y - 1, WIN_W, INPUT_H + 3, COL_INPUT_BG);
    draw_hline(strip_y - 1, COL_ACCENT);

    let prompt = "$ ";
    comp_draw_text(PAD_X, strip_y + 2, COL_PROMPT, prompt);
    let text_x = PAD_X + prompt.len() as u32 * CHAR_W;

    let input  = term.input.as_str();
    let cur    = term.cursor.min(input.len());
    let before = &input[..cur];
    let after  = &input[cur..];

    comp_draw_text(text_x, strip_y + 2, COL_DEFAULT, before);
    let cx = text_x + before.len() as u32 * CHAR_W;

    comp_fill_rect(cx, strip_y + 1, CHAR_W, LINE_H - 2, COL_CURSOR);
    if let Some(ch) = after.chars().next() {
        let mut tmp = [0u8; 4];
        let cs = ch.encode_utf8(&mut tmp);
        comp_draw_text(cx, strip_y + 2, COL_BG, cs);
        comp_draw_text(cx + CHAR_W, strip_y + 2, COL_DEFAULT, &after[ch.len_utf8()..]);
    }
}
