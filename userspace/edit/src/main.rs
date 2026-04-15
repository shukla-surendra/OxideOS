//! OxideOS Text Editor — nano-like full-screen editor using compositor IPC.
//!
//! Usage: edit [filename]
//!
//! Keys:
//!   Arrow keys   — move cursor
//!   Home / End   — beginning / end of line
//!   PgUp / PgDn  — scroll page
//!   Enter        — insert newline
//!   Backspace    — delete character before cursor
//!   Del (^[[3~)  — delete character at cursor
//!   Ctrl+S       — save file
//!   Ctrl+Q       — quit (prompts if unsaved changes)
//!   Ctrl+X       — quit without saving
#![no_std]
#![no_main]

use oxide_rt::{
    exit, getchar, sleep_ms, get_time,
    comp_fill_rect, comp_draw_text, comp_present,
    msgq_create, open, close, write, read,
    COMPOSITOR_QID,
};

// ── Layout ────────────────────────────────────────────────────────────────────

const WIN_W: u32 = 556;
const WIN_H: u32 = 389;

const CHAR_W: u32 = 9;
const LINE_H: u32 = 16;

// Title bar at top (filename + status)
const TITLE_H: u32 = 18;
// Status/hint bar at bottom
const STATUS_H: u32 = 18;

const TEXT_X: u32 = 4;
const TEXT_Y: u32 = TITLE_H + 2;
const TEXT_W: u32 = WIN_W - TEXT_X - 4;
const TEXT_H: u32 = WIN_H - TITLE_H - STATUS_H - 4;

const VISIBLE_ROWS: usize = (TEXT_H / LINE_H) as usize; // ~22 lines

// ── Document limits ───────────────────────────────────────────────────────────

const MAX_LINES: usize    = 500;
const MAX_LINE_LEN: usize = 256;

// ── Colors ────────────────────────────────────────────────────────────────────

const COL_BG:         u32 = 0xFF1E1E2E;  // dark blue-gray background
const COL_TITLE_BG:   u32 = 0xFF181825;
const COL_STATUS_BG:  u32 = 0xFF181825;
const COL_TEXT:       u32 = 0xFFCDD6F4;  // catppuccin text
const COL_DIM:        u32 = 0xFF585B70;
const COL_ACCENT:     u32 = 0xFF89B4FA;  // blue accent
const COL_CURSOR_BG:  u32 = 0xFF89B4FA;
const COL_CURSOR_FG:  u32 = 0xFF1E1E2E;
const COL_LINE_NUM:   u32 = 0xFF45475A;
const COL_MODIFIED:   u32 = 0xFFF38BA8;  // red when unsaved
const COL_SAVED:      u32 = 0xFFA6E3A1;  // green when saved
const COL_HINT:       u32 = 0xFF585B70;
const COL_SELECTION:  u32 = 0xFF313244;
const COL_WARN:       u32 = 0xFFFAB387;

// Line number gutter width (4 digits + 1 space = 5 chars)
const GUTTER_W: u32 = 5 * CHAR_W;
const EDIT_X:   u32 = TEXT_X + GUTTER_W;
const EDIT_COLS: usize = ((TEXT_W - GUTTER_W) / CHAR_W) as usize;

// ── Fixed-size line ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Line {
    buf: [u8; MAX_LINE_LEN],
    len: usize,
}

impl Line {
    const fn new() -> Self { Self { buf: [0; MAX_LINE_LEN], len: 0 } }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    fn insert(&mut self, pos: usize, b: u8) {
        if self.len >= MAX_LINE_LEN || pos > self.len { return; }
        let mut i = self.len;
        while i > pos { self.buf[i] = self.buf[i - 1]; i -= 1; }
        self.buf[pos] = b;
        self.len += 1;
    }

    fn remove(&mut self, pos: usize) -> u8 {
        if pos >= self.len { return 0; }
        let b = self.buf[pos];
        let mut i = pos;
        while i + 1 < self.len { self.buf[i] = self.buf[i + 1]; i += 1; }
        self.len -= 1;
        b
    }

    // Append all bytes from `other` onto self (used for join-lines).
    fn append(&mut self, other: &Line) {
        let space = MAX_LINE_LEN - self.len;
        let n = other.len.min(space);
        self.buf[self.len..self.len + n].copy_from_slice(&other.buf[..n]);
        self.len += n;
    }

    // Split at `col`: keep [..col] in self, return [col..] as new line.
    fn split_at(&mut self, col: usize) -> Line {
        let mut new_line = Line::new();
        let n = self.len.saturating_sub(col);
        new_line.buf[..n].copy_from_slice(&self.buf[col..col + n]);
        new_line.len = n;
        self.len = col.min(self.len);
        new_line
    }
}

// ── Document ──────────────────────────────────────────────────────────────────

struct Doc {
    lines:    [Line; MAX_LINES],
    count:    usize,
    cur_row:  usize,
    cur_col:  usize,
    scroll:   usize,   // first visible row
    modified: bool,
    filename: [u8; 128],
    fname_len: usize,
    // Brief save-result message
    msg: [u8; 64],
    msg_len: usize,
    msg_ticks: u64,
    msg_ok: bool,
}

impl Doc {
    fn new() -> Self {
        let d = Self {
            lines: [Line::new(); MAX_LINES],
            count: 1,
            cur_row: 0,
            cur_col: 0,
            scroll: 0,
            modified: false,
            filename: [0; 128],
            fname_len: 0,
            msg: [0; 64],
            msg_len: 0,
            msg_ticks: 0,
            msg_ok: true,
        };
        d
    }

    fn filename_str(&self) -> &str {
        core::str::from_utf8(&self.filename[..self.fname_len]).unwrap_or("[no name]")
    }

    fn set_msg(&mut self, s: &str, ok: bool) {
        let n = s.len().min(63);
        self.msg[..n].copy_from_slice(&s.as_bytes()[..n]);
        self.msg_len = n;
        self.msg_ticks = get_time();
        self.msg_ok = ok;
    }

    fn msg_str(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.msg_len]).unwrap_or("")
    }

    // Clamp cur_col to current line length.
    fn clamp_col(&mut self) {
        let max = self.lines[self.cur_row].len;
        if self.cur_col > max { self.cur_col = max; }
    }

    // Scroll view so cur_row is visible.
    fn ensure_visible(&mut self) {
        if self.cur_row < self.scroll {
            self.scroll = self.cur_row;
        } else if self.cur_row >= self.scroll + VISIBLE_ROWS {
            self.scroll = self.cur_row + 1 - VISIBLE_ROWS;
        }
    }

    // Move cursor up.
    fn move_up(&mut self) {
        if self.cur_row > 0 {
            self.cur_row -= 1;
            self.clamp_col();
            self.ensure_visible();
        }
    }

    fn move_down(&mut self) {
        if self.cur_row + 1 < self.count {
            self.cur_row += 1;
            self.clamp_col();
            self.ensure_visible();
        }
    }

    fn move_left(&mut self) {
        if self.cur_col > 0 {
            self.cur_col -= 1;
        } else if self.cur_row > 0 {
            self.cur_row -= 1;
            self.cur_col = self.lines[self.cur_row].len;
            self.ensure_visible();
        }
    }

    fn move_right(&mut self) {
        let line_len = self.lines[self.cur_row].len;
        if self.cur_col < line_len {
            self.cur_col += 1;
        } else if self.cur_row + 1 < self.count {
            self.cur_row += 1;
            self.cur_col = 0;
            self.ensure_visible();
        }
    }

    fn home(&mut self) { self.cur_col = 0; }

    fn end(&mut self) { self.cur_col = self.lines[self.cur_row].len; }

    fn page_up(&mut self) {
        let step = VISIBLE_ROWS.saturating_sub(1).max(1);
        self.cur_row = self.cur_row.saturating_sub(step);
        self.scroll  = self.scroll.saturating_sub(step);
        self.clamp_col();
        self.ensure_visible();
    }

    fn page_down(&mut self) {
        let step = VISIBLE_ROWS.saturating_sub(1).max(1);
        self.cur_row = (self.cur_row + step).min(self.count.saturating_sub(1));
        self.clamp_col();
        self.ensure_visible();
    }

    fn insert_char(&mut self, b: u8) {
        self.lines[self.cur_row].insert(self.cur_col, b);
        self.cur_col += 1;
        self.modified = true;
    }

    fn insert_newline(&mut self) {
        if self.count >= MAX_LINES { return; }
        let new_line = self.lines[self.cur_row].split_at(self.cur_col);
        // Shift lines down.
        let mut i = self.count;
        while i > self.cur_row + 1 {
            self.lines[i] = self.lines[i - 1];
            i -= 1;
        }
        self.lines[self.cur_row + 1] = new_line;
        self.count += 1;
        self.cur_row += 1;
        self.cur_col = 0;
        self.modified = true;
        self.ensure_visible();
    }

    fn backspace(&mut self) {
        if self.cur_col > 0 {
            self.cur_col -= 1;
            self.lines[self.cur_row].remove(self.cur_col);
            self.modified = true;
        } else if self.cur_row > 0 {
            // Join with previous line.
            let prev_len = self.lines[self.cur_row - 1].len;
            let cur_line = self.lines[self.cur_row];
            self.lines[self.cur_row - 1].append(&cur_line);
            // Shift lines up.
            let mut i = self.cur_row;
            while i + 1 < self.count { self.lines[i] = self.lines[i + 1]; i += 1; }
            self.count -= 1;
            self.cur_row -= 1;
            self.cur_col = prev_len;
            self.modified = true;
            self.ensure_visible();
        }
    }

    fn delete_char(&mut self) {
        let line_len = self.lines[self.cur_row].len;
        if self.cur_col < line_len {
            self.lines[self.cur_row].remove(self.cur_col);
            self.modified = true;
        } else if self.cur_row + 1 < self.count {
            // Join next line into current.
            let next_line = self.lines[self.cur_row + 1];
            self.lines[self.cur_row].append(&next_line);
            let mut i = self.cur_row + 1;
            while i + 1 < self.count { self.lines[i] = self.lines[i + 1]; i += 1; }
            self.count -= 1;
            self.modified = true;
        }
    }

    fn load_file(&mut self) -> bool {
        if self.fname_len == 0 { return false; }
        let path = self.filename_str();
        let fd = open(path, 1); // O_RDONLY
        if fd < 0 { return false; }

        // Reset document.
        self.count = 1;
        self.lines[0] = Line::new();
        self.cur_row = 0;
        self.cur_col = 0;
        self.scroll  = 0;

        let mut row = 0usize;
        let mut buf = [0u8; 512];
        loop {
            let n = read(fd, &mut buf);
            if n <= 0 { break; }
            for i in 0..n as usize {
                let b = buf[i];
                if b == b'\n' {
                    if row + 1 < MAX_LINES {
                        row += 1;
                        self.lines[row] = Line::new();
                        self.count = row + 1;
                    }
                } else if b != b'\r' {
                    self.lines[row].insert(self.lines[row].len, b);
                }
            }
        }
        close(fd);
        self.modified = false;
        true
    }

    fn save_file(&mut self) {
        if self.fname_len == 0 {
            self.set_msg("No filename (Ctrl+S with a name first)", false);
            return;
        }
        let path_buf = self.filename;
        let path_len = self.fname_len;
        let path = core::str::from_utf8(&path_buf[..path_len]).unwrap_or("");

        // O_WRONLY | O_CREAT | O_TRUNC = 1 | 0x40 | 0x200 = 0x241
        let fd = open(path, 0x241);
        if fd < 0 {
            self.set_msg("Save failed: cannot open file", false);
            return;
        }
        for r in 0..self.count {
            let line = &self.lines[r];
            if line.len > 0 {
                write(fd, &line.buf[..line.len]);
            }
            write(fd, b"\n");
        }
        close(fd);
        self.modified = false;
        self.set_msg("File saved.", true);
    }
}

// ── Number formatting helpers ─────────────────────────────────────────────────

fn fmt_usize(buf: &mut [u8; 8], v: usize) -> &str {
    let mut i = buf.len();
    let mut vv = v;
    if vv == 0 { i -= 1; buf[i] = b'0'; }
    while vv > 0 { i -= 1; buf[i] = b'0' + (vv % 10) as u8; vv /= 10; }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

// ── Compositor drawing ─────────────────────────────────────────────────────────

fn draw_title(doc: &Doc) {
    comp_fill_rect(0, 0, WIN_W, TITLE_H, COL_TITLE_BG);

    // Separator line
    comp_fill_rect(0, TITLE_H, WIN_W, 2, 0xFF313244);

    // "edit" label
    comp_draw_text(4, 2, COL_ACCENT, "edit");

    // filename
    let fname = doc.filename_str();
    let fname_x = 4 + 5 * CHAR_W; // "edit " = 5 chars
    comp_draw_text(fname_x, 2, COL_TEXT, fname);

    // Modified indicator
    let mod_x = fname_x + fname.len() as u32 * CHAR_W + 4;
    if doc.modified {
        comp_draw_text(mod_x, 2, COL_MODIFIED, "[+]");
    } else {
        comp_draw_text(mod_x, 2, COL_SAVED, "[saved]");
    }

    // Right side: row:col
    let mut rbuf = [0u8; 32];
    let mut rlen = 0usize;
    fn push_usize_r(buf: &mut [u8; 32], pos: &mut usize, v: usize) {
        let mut tmp = [0u8; 8]; let mut i = 8usize;
        let mut vv = v;
        if vv == 0 { i -= 1; tmp[i] = b'0'; }
        while vv > 0 { i -= 1; tmp[i] = b'0' + (vv % 10) as u8; vv /= 10; }
        for &b in &tmp[i..] { if *pos < buf.len() { buf[*pos] = b; *pos += 1; } }
    }
    push_usize_r(&mut rbuf, &mut rlen, doc.cur_row + 1);
    if rlen < rbuf.len() { rbuf[rlen] = b':'; rlen += 1; }
    push_usize_r(&mut rbuf, &mut rlen, doc.cur_col + 1);
    let rstr = core::str::from_utf8(&rbuf[..rlen]).unwrap_or("");
    let rx = WIN_W.saturating_sub(rstr.len() as u32 * CHAR_W + 4);
    comp_draw_text(rx, 2, COL_DIM, rstr);
}

fn draw_status(doc: &Doc, ticks: u64) {
    let sy = WIN_H - STATUS_H;
    comp_fill_rect(0, sy, WIN_W, STATUS_H, COL_STATUS_BG);
    comp_fill_rect(0, sy, WIN_W, 1, 0xFF313244);

    // Show timed message or static hints
    let msg_expired = ticks.saturating_sub(doc.msg_ticks) > 300; // 3 seconds
    if doc.msg_len > 0 && !msg_expired {
        let col = if doc.msg_ok { COL_SAVED } else { COL_WARN };
        comp_draw_text(4, sy + 2, col, doc.msg_str());
    } else {
        comp_draw_text(4, sy + 2, COL_HINT, "^S Save  ^Q Quit  ^X Exit");
        // Show line count on right
        let mut b = [0u8; 8];
        let lc_str = fmt_usize(&mut b, doc.count);
        let mut info = [0u8; 32];
        let mut il = 0usize;
        for &b in b"lines: " { if il < info.len() { info[il] = b; il += 1; } }
        for &b in lc_str.as_bytes() { if il < info.len() { info[il] = b; il += 1; } }
        let istr = core::str::from_utf8(&info[..il]).unwrap_or("");
        let ix = WIN_W.saturating_sub(istr.len() as u32 * CHAR_W + 4);
        comp_draw_text(ix, sy + 2, COL_DIM, istr);
    }
}

fn draw_text_area(doc: &Doc) {
    // Clear text area.
    comp_fill_rect(0, TEXT_Y, WIN_W, TEXT_H, COL_BG);

    for r in 0..VISIBLE_ROWS {
        let row = doc.scroll + r;
        if row >= doc.count { break; }

        let y = TEXT_Y + r as u32 * LINE_H;

        // Gutter: line number
        let mut nbuf = [0u8; 8];
        let nstr = fmt_usize(&mut nbuf, row + 1);
        // Right-align in 4 chars
        let npad = 4usize.saturating_sub(nstr.len());
        let nx = TEXT_X + npad as u32 * CHAR_W;
        comp_draw_text(nx, y, COL_LINE_NUM, nstr);

        // Gutter separator
        comp_fill_rect(TEXT_X + 4 * CHAR_W, y, 1, LINE_H, 0xFF313244);

        // Cursor row highlight
        if row == doc.cur_row {
            comp_fill_rect(EDIT_X, y, TEXT_W - GUTTER_W, LINE_H, COL_SELECTION);
        }

        // Line text, scrolled horizontally if needed
        let line = &doc.lines[row];
        let line_str = line.as_str();

        // Horizontal scroll: keep cursor visible
        let h_scroll = if row == doc.cur_row && doc.cur_col >= EDIT_COLS {
            doc.cur_col.saturating_sub(EDIT_COLS - 1)
        } else {
            0
        };

        let text_bytes = line_str.as_bytes();
        if h_scroll < text_bytes.len() {
            let visible_slice = &text_bytes[h_scroll..];
            let visible_len = visible_slice.len().min(EDIT_COLS + 4);
            if let Ok(s) = core::str::from_utf8(&visible_slice[..visible_len]) {
                comp_draw_text(EDIT_X, y, COL_TEXT, s);
            }
        }

        // Draw cursor on current row
        if row == doc.cur_row {
            let col_in_view = doc.cur_col.saturating_sub(h_scroll);
            let cx = EDIT_X + col_in_view as u32 * CHAR_W;
            if cx + CHAR_W <= TEXT_X + TEXT_W {
                comp_fill_rect(cx, y, CHAR_W, LINE_H, COL_CURSOR_BG);
                // Draw char under cursor in inverse color
                if doc.cur_col < line_str.len() {
                    let ch_byte = line_str.as_bytes()[doc.cur_col];
                    let cs = core::str::from_utf8(core::slice::from_ref(&ch_byte)).unwrap_or(" ");
                    comp_draw_text(cx, y, COL_CURSOR_FG, cs);
                }
            }
        }
    }
}

fn redraw(doc: &Doc, ticks: u64) {
    draw_title(doc);
    draw_text_area(doc);
    draw_status(doc, ticks);
    comp_present();
}

// ── Escape sequence reader ────────────────────────────────────────────────────

fn blocking_byte() -> u8 {
    loop {
        if let Some(b) = getchar() { return b; }
        sleep_ms(5);
    }
}

/// Returns true if the editor should quit.
fn handle_escape(doc: &mut Doc) -> bool {
    let b = blocking_byte();
    if b != b'[' { return false; }
    let code = blocking_byte();
    match code {
        b'A' => doc.move_up(),
        b'B' => doc.move_down(),
        b'C' => doc.move_right(),
        b'D' => doc.move_left(),
        b'H' => doc.home(),
        b'F' => doc.end(),
        b'3' => {
            let _ = blocking_byte(); // consume '~'
            doc.delete_char();
        }
        b'5' => {
            let _ = blocking_byte(); // consume '~'
            doc.page_up();
        }
        b'6' => {
            let _ = blocking_byte(); // consume '~'
            doc.page_down();
        }
        _ => {}
    }
    false
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    msgq_create(COMPOSITOR_QID);

    let mut doc = Doc::new();

    // TODO: parse argv when syscall support is added.
    // For now, use a default filename if available via env.
    // The shell can pass a filename via a known path.
    // Attempt to read from /tmp/edit_arg if it exists (set by kernel terminal).
    {
        let arg_fd = open("/tmp/edit_arg", 1);
        if arg_fd >= 0 {
            let mut abuf = [0u8; 128];
            let n = read(arg_fd, &mut abuf);
            close(arg_fd);
            if n > 0 {
                let len = (n as usize).min(127);
                // Strip trailing newline
                let len = if len > 0 && abuf[len - 1] == b'\n' { len - 1 } else { len };
                doc.filename[..len].copy_from_slice(&abuf[..len]);
                doc.fname_len = len;
                doc.load_file();
            }
        }
    }

    let ticks = get_time();
    redraw(&doc, ticks);

    loop {
        let Some(c) = getchar() else {
            let t = get_time();
            redraw(&doc, t);
            sleep_ms(16);
            continue;
        };

        match c {
            // Ctrl+S — save
            0x13 => {
                doc.save_file();
            }
            // Ctrl+Q — quit (confirm if modified)
            0x11 => {
                if doc.modified {
                    doc.set_msg("Unsaved changes! Ctrl+Q again to quit, any key to cancel.", false);
                    let t = get_time();
                    redraw(&doc, t);
                    // Wait for second keystroke
                    let c2 = blocking_byte();
                    if c2 == 0x11 {
                        exit(0);
                    }
                    // Otherwise cancel
                    doc.set_msg("", true);
                } else {
                    exit(0);
                }
            }
            // Ctrl+X — force quit without saving
            0x18 => exit(0),
            // ESC sequence
            0x1B => {
                handle_escape(&mut doc);
            }
            // Enter
            b'\n' | b'\r' => {
                doc.insert_newline();
            }
            // Backspace
            8 | 127 => {
                doc.backspace();
            }
            // Printable ASCII + Tab
            b if (b >= 32 && b < 127) || b == b'\t' => {
                doc.insert_char(b);
            }
            _ => {}
        }

        let t = get_time();
        redraw(&doc, t);
    }
}
