//! OxideBrowser — lightweight HTTP text browser for OxideOS.
//!
//! Network fetch runs in a forked child process so the GUI stays responsive.
//! The child writes the parsed result into a shared-memory segment, then
//! writes one byte to a pipe to signal completion.  The parent polls the pipe
//! each frame (non-blocking) and reads the SHM when ready.
#![no_std]
#![no_main]

extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::vec::Vec;
use oxide_rt::{
    exit, sleep_ms,
    gui_create, gui_fill_rect, gui_draw_text, gui_present, gui_poll_event,
    GuiWindow, GuiEvent,
    dns_resolve, socket, connect, send, recv, close_socket,
    AF_INET, SOCK_STREAM, SockAddrIn,
    fork, getpid,
    pipe  as os_pipe,
    close as fd_close,
    read  as fd_read,
    write as fd_write,
    shmget, shmat, shmdt,
};

// ── Layout ─────────────────────────────────────────────────────────────────────

const WIN_W: u32 = 760;
const WIN_H: u32 = 520;

const CHAR_W: u32 = 9;
const LINE_H: u32 = 16;
const PAD_X:  u32 = 8;

const TOOLBAR_H:   u32 = 34;
const STATUSBAR_H: u32 = 18;

const BTN_BACK_X:    u32 = 4;
const BTN_BACK_W:    u32 = 30;
const BTN_FWD_X:     u32 = 38;
const BTN_FWD_W:     u32 = 30;
const BTN_URL_X:     u32 = 72;
const BTN_GO_W:      u32 = 56;
const BTN_Y:         u32 = 5;
const BTN_H:         u32 = 24;

const CONTENT_Y:  u32 = TOOLBAR_H + 1;
const CONTENT_H:  u32 = WIN_H - CONTENT_Y - 1 - STATUSBAR_H;
const STATUS_Y:   u32 = WIN_H - STATUSBAR_H;

const COLS:          usize = ((WIN_W - PAD_X * 2) / CHAR_W) as usize;
const VISIBLE_LINES: usize = (CONTENT_H / LINE_H) as usize;

// ── Colour palette ─────────────────────────────────────────────────────────────

const C_BG:        u32 = 0xFF0D1117;
const C_PANEL:     u32 = 0xFF161B22;
const C_BORDER:    u32 = 0xFF30363D;
const C_TEXT:      u32 = 0xFFB4BEC9;
const C_DIM:       u32 = 0xFF6E7681;
const C_H1:        u32 = 0xFFF0F6FC;
const C_H2:        u32 = 0xFFCDD9E5;
const C_H3:        u32 = 0xFF8B949E;
const C_LINK:      u32 = 0xFF58A6FF;
const C_LINK_HOV:  u32 = 0xFF79C0FF;
const C_BTN_BG:    u32 = 0xFF21262D;
const C_BTN_FG:    u32 = 0xFFC9D1D9;
const C_BTN_BORD:  u32 = 0xFF444C56;
const C_INPUT_BG:  u32 = 0xFF010409;
const C_INPUT_FG:  u32 = 0xFFE6EDF3;
const C_CURSOR:    u32 = 0xFF58A6FF;
const C_STATUS_FG: u32 = 0xFF7D8590;
const C_OK:        u32 = 0xFF3FB950;
const C_ERR:       u32 = 0xFFF85149;
const C_WARN:      u32 = 0xFFD29922;

// Status color codes stored in SHM (mapped back to actual colours on read)
const SCOL_STATUS: u32 = 0;
const SCOL_OK:     u32 = 1;
const SCOL_ERR:    u32 = 2;
const SCOL_WARN:   u32 = 3;
const SCOL_DIM:    u32 = 4;

fn decode_status_color(code: u32) -> u32 {
    match code {
        SCOL_OK   => C_OK,
        SCOL_ERR  => C_ERR,
        SCOL_WARN => C_WARN,
        SCOL_DIM  => C_DIM,
        _         => C_STATUS_FG,
    }
}

// ── SHM layout ─────────────────────────────────────────────────────────────────
//
// The forked child writes parsed page data here; the parent reads it back.
//
// Header (512 bytes):
//   [0..4]   u32 status: 1=ok  2=err
//   [4..8]   u32 line_count
//   [8..12]  u32 link_count
//  [12..16]  u32 title_len
//  [16..144] title bytes (max 128)
// [144..148] u32 status_msg_len
// [148..268] status_msg bytes (max 120)
// [268..272] u32 status_color_code
// [272..512] reserved
//
// Lines (starting at SHMO_LINES, 110 bytes each):
//   [0..100]   text bytes
//   [100..104] u32 len
//   [104..108] u32 color
//   [108..110] u16 link_idx
//
// Links (starting at SHMO_LINKS, 260 bytes each):
//   [0..4]    u32 len
//   [4..260]  url bytes (max 256)

const SHM_SIZE:      u64   = 300 * 1024;   // 300 KB
const MAX_LINES_SHM: usize = 2000;
const MAX_LINKS_SHM: usize = 100;
const LINE_BYTES:    usize = 110;
const LINK_BYTES:    usize = 260;

const SHMO_STATUS:    usize = 0;
const SHMO_LINE_CNT:  usize = 4;
const SHMO_LINK_CNT:  usize = 8;
const SHMO_TITLE_LEN: usize = 12;
const SHMO_TITLE:     usize = 16;
const SHMO_SMSG_LEN:  usize = 144;
const SHMO_SMSG:      usize = 148;
const SHMO_SCOL:      usize = 268;
const SHMO_LINES:     usize = 512;
const SHMO_LINKS:     usize = SHMO_LINES + MAX_LINES_SHM * LINE_BYTES;
// Total bytes used: 512 + 2000*110 + 100*260 = 512 + 220000 + 26000 = 246512 < 300KB

unsafe fn shm_write_u32(base: *mut u8, off: usize, v: u32) {
    let b = v.to_le_bytes();
    unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), base.add(off), 4); }
}
unsafe fn shm_write_u16(base: *mut u8, off: usize, v: u16) {
    let b = v.to_le_bytes();
    unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), base.add(off), 2); }
}
unsafe fn shm_read_u32(base: *const u8, off: usize) -> u32 {
    let mut b = [0u8; 4];
    unsafe { core::ptr::copy_nonoverlapping(base.add(off), b.as_mut_ptr(), 4); }
    u32::from_le_bytes(b)
}
unsafe fn shm_read_u16(base: *const u8, off: usize) -> u16 {
    let mut b = [0u8; 2];
    unsafe { core::ptr::copy_nonoverlapping(base.add(off), b.as_mut_ptr(), 2); }
    u16::from_le_bytes(b)
}

// ── FixStr ─────────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
struct FixStr<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> FixStr<N> {
    const fn new() -> Self { Self { buf: [0; N], len: 0 } }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    fn len(&self) -> usize { self.len }
    fn is_empty(&self) -> bool { self.len == 0 }
    fn clear(&mut self) { self.len = 0; }

    fn push_byte(&mut self, b: u8) {
        if self.len < N { self.buf[self.len] = b; self.len += 1; }
    }

    fn push_str(&mut self, s: &str) {
        for b in s.bytes() { self.push_byte(b); }
    }

    fn insert(&mut self, pos: usize, b: u8) {
        if self.len >= N || pos > self.len { return; }
        let mut i = self.len;
        while i > pos { self.buf[i] = self.buf[i - 1]; i -= 1; }
        self.buf[pos] = b;
        self.len += 1;
    }

    fn remove(&mut self, pos: usize) {
        if pos >= self.len { return; }
        let mut i = pos;
        while i + 1 < self.len { self.buf[i] = self.buf[i + 1]; i += 1; }
        self.len -= 1;
    }
}

// ── Rendered line ──────────────────────────────────────────────────────────────

const LINE_BUF: usize = 100;

#[derive(Copy, Clone)]
struct RenderLine {
    text: [u8; LINE_BUF],
    len:  usize,
    color: u32,
    link_idx: u16, // 0xFFFF = no link
}

impl RenderLine {
    const fn empty() -> Self {
        Self { text: [0; LINE_BUF], len: 0, color: C_TEXT, link_idx: 0xFFFF }
    }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.text[..self.len]).unwrap_or("")
    }
}

// ── URL parsing ────────────────────────────────────────────────────────────────

struct ParsedUrl {
    host: FixStr<256>,
    port: u16,
    path: FixStr<512>,
    is_https: bool,
}

fn parse_url(url: &str) -> Option<ParsedUrl> {
    let (is_https, rest) = if url.starts_with("https://") {
        (true, &url[8..])
    } else if url.starts_with("http://") {
        (false, &url[7..])
    } else if url.contains("://") {
        return None;
    } else {
        (false, url)
    };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None    => (rest, "/"),
    };

    let (host, port) = if let Some(i) = host_port.rfind(':') {
        let p = host_port[i+1..].parse::<u16>().unwrap_or(if is_https { 443 } else { 80 });
        (&host_port[..i], p)
    } else {
        (host_port, if is_https { 443 } else { 80 })
    };

    if host.is_empty() { return None; }

    let mut pu = ParsedUrl { host: FixStr::new(), port, path: FixStr::new(), is_https };
    pu.host.push_str(host);
    pu.path.push_str(path);
    Some(pu)
}

fn resolve_url(href: &str, base: &str) -> FixStr<512> {
    let mut out: FixStr<512> = FixStr::new();

    if href.starts_with("http://") || href.starts_with("https://") {
        out.push_str(href); return out;
    }
    if href.starts_with("//") {
        out.push_str("http:"); out.push_str(href); return out;
    }
    if href.starts_with('#') || href.is_empty() {
        out.push_str(base); return out;
    }

    let (scheme_host, _) = if let Some(_i) = base.strip_prefix("http://").and_then(|r| r.find('/').map(|j| (j, r))) {
        let after_scheme = &base[7..];
        let slash = after_scheme.find('/').unwrap_or(after_scheme.len());
        (&base[..7+slash], &base[7+slash..])
    } else if let Some(after) = base.strip_prefix("https://") {
        let slash = after.find('/').unwrap_or(after.len());
        (&base[..8+slash], &base[8+slash..])
    } else {
        ("http://localhost", "/")
    };

    if href.starts_with('/') {
        out.push_str(scheme_host);
        out.push_str(href);
    } else {
        let dir_end = base.rfind('/').unwrap_or(base.len());
        out.push_str(&base[..dir_end + 1]);
        out.push_str(href);
    }
    out
}

// ── HTML attribute extractor ───────────────────────────────────────────────────

fn extract_attr(tag: &[u8], attr: &[u8]) -> FixStr<256> {
    let mut result: FixStr<256> = FixStr::new();
    let alen = attr.len();
    if alen == 0 || tag.len() <= alen { return result; }

    let mut i = 0;
    while i + alen <= tag.len() {
        let matched = tag[i..i+alen].iter().zip(attr.iter())
            .all(|(&a, &b)| a.to_ascii_lowercase() == b.to_ascii_lowercase());

        if matched {
            let mut j = i + alen;
            while j < tag.len() && (tag[j] == b' ' || tag[j] == b'\t' || tag[j] == b'=') {
                j += 1;
            }
            if j < tag.len() {
                let quote = if tag[j] == b'"' || tag[j] == b'\'' { tag[j] } else { 0 };
                if quote != 0 { j += 1; }
                while j < tag.len() {
                    let c = tag[j];
                    if quote != 0 && c == quote { break; }
                    if quote == 0 && (c == b' ' || c == b'>' || c == b'\t' || c == b'\r' || c == b'\n') { break; }
                    result.push_byte(c);
                    j += 1;
                }
                return result;
            }
        }
        i += 1;
    }
    result
}

// ── HTML parser ────────────────────────────────────────────────────────────────

struct PageContent {
    lines: Vec<RenderLine>,
    links: Vec<FixStr<512>>,
    title: FixStr<128>,
}

fn parse_html(body: &[u8], cols: usize, base_url: &str) -> PageContent {
    let mut lines: Vec<RenderLine> = Vec::new();
    let mut links: Vec<FixStr<512>> = Vec::new();
    let mut title: FixStr<128> = FixStr::new();

    let mut in_tag      = false;
    let mut in_script   = false;
    let mut in_style    = false;
    let mut in_comment  = false;
    let mut in_title    = false;
    let mut tag_buf     = [0u8; 512];
    let mut tag_len     = 0usize;

    let mut in_entity   = false;
    let mut ent_buf     = [0u8; 16];
    let mut ent_len     = 0usize;

    let mut cur_buf     = [0u8; LINE_BUF];
    let mut cur_len     = 0usize;
    let mut cur_color   = C_TEXT;
    let mut cur_link: u16 = 0xFFFF;

    let mut heading: u8 = 0;
    let mut in_link     = false;
    let mut active_link: u16 = 0xFFFF;

    macro_rules! emit_line {
        () => {
            if lines.len() < 4096 && cur_len > 0 {
                let mut line = RenderLine::empty();
                let n = cur_len.min(LINE_BUF);
                line.text[..n].copy_from_slice(&cur_buf[..n]);
                line.len = n;
                line.color = cur_color;
                line.link_idx = cur_link;
                lines.push(line);
                cur_len = 0;
            }
        }
    }

    macro_rules! newline {
        () => { emit_line!(); }
    }

    macro_rules! blank_line {
        () => {
            emit_line!();
            let last_empty = lines.last().map(|l| l.len == 0).unwrap_or(true);
            if !last_empty && lines.len() < 4096 {
                lines.push(RenderLine::empty());
            }
        }
    }

    macro_rules! push_char {
        ($b:expr) => {
            let b = $b;
            if in_title {
                title.push_byte(b);
            } else {
                cur_color = if in_link { C_LINK }
                    else { match heading { 1 => C_H1, 2 => C_H2, 3..=6 => C_H3, _ => C_TEXT } };
                cur_link = active_link;
                if cur_len < LINE_BUF {
                    cur_buf[cur_len] = b;
                    cur_len += 1;
                }
                if cur_len >= cols.min(LINE_BUF) {
                    let wrap_at = {
                        let mut w = cur_len;
                        for k in (0..cur_len).rev() {
                            if cur_buf[k] == b' ' { w = k + 1; break; }
                            if cur_len - k > 15 { break; }
                        }
                        w
                    };
                    if lines.len() < 4096 {
                        let mut line = RenderLine::empty();
                        let n = wrap_at.min(LINE_BUF);
                        line.text[..n].copy_from_slice(&cur_buf[..n]);
                        line.len = n;
                        line.color = cur_color;
                        line.link_idx = cur_link;
                        lines.push(line);
                        let remainder = cur_len.saturating_sub(wrap_at);
                        if remainder > 0 && wrap_at < cur_len {
                            for k in 0..remainder { cur_buf[k] = cur_buf[wrap_at + k]; }
                        }
                        cur_len = remainder;
                    }
                }
            }
        }
    }

    macro_rules! push_text {
        ($s:expr) => {
            for b in $s.bytes() {
                if b.is_ascii() { push_char!(b); }
            }
        }
    }

    macro_rules! process_tag {
        () => {{
            let mut local = [0u8; 512];
            let tlen = tag_len.min(511);
            local[..tlen].copy_from_slice(&tag_buf[..tlen]);
            let raw = core::str::from_utf8(&local[..tlen]).unwrap_or("").trim();

            if raw.starts_with("!--") {
                // handled in stream
            } else if !raw.is_empty() {
                let closing = raw.starts_with('/');
                let name_start = if closing { 1 } else { 0 };
                let name_bytes = raw[name_start..].as_bytes();
                let name_end = name_bytes.iter()
                    .position(|&b| b == b' ' || b == b'\t' || b == b'/' || b == b'>')
                    .unwrap_or(name_bytes.len());
                let mut name_lc = [0u8; 16];
                let nlen = name_end.min(16);
                for (i, &b) in name_bytes[..nlen].iter().enumerate() {
                    name_lc[i] = b.to_ascii_lowercase();
                }
                let name = core::str::from_utf8(&name_lc[..nlen]).unwrap_or("");

                match name {
                    "script" => { in_script = !closing; if closing { newline!(); } }
                    "style"  => { in_style  = !closing; }
                    "title"  => { in_title  = !closing; }
                    _ if in_script || in_style => {}
                    "h1" => {
                        if closing { heading = 0; newline!(); } else { newline!(); heading = 1; }
                    }
                    "h2" => {
                        if closing { heading = 0; newline!(); } else { newline!(); heading = 2; }
                    }
                    "h3" | "h4" | "h5" | "h6" => {
                        if closing { heading = 0; newline!(); } else { newline!(); heading = 3; }
                    }
                    "p" | "section" | "article" | "header" | "footer" | "main" | "nav" | "aside" => {
                        if closing { blank_line!(); } else { newline!(); }
                    }
                    "div" | "form" | "fieldset" => { newline!(); }
                    "br"  => { newline!(); }
                    "hr"  => {
                        newline!();
                        let mut line = RenderLine::empty();
                        let n = cols.min(80).min(LINE_BUF);
                        for k in 0..n { line.text[k] = b'-'; }
                        line.len = n;
                        line.color = C_BORDER;
                        if lines.len() < 4096 { lines.push(line); }
                    }
                    "li" => { if !closing { newline!(); push_text!("  * "); } }
                    "ul" | "ol" | "menu" => { if closing { newline!(); } }
                    "blockquote" => { newline!(); if !closing { push_text!("  "); } }
                    "pre" | "code" => { if closing { newline!(); } }
                    "a" => {
                        if closing {
                            in_link = false; active_link = 0xFFFF;
                        } else {
                            let href = extract_attr(&local[..tlen], b"href");
                            if !href.is_empty() && links.len() < 512 {
                                let resolved = resolve_url(href.as_str(), base_url);
                                let idx = links.len() as u16;
                                links.push(resolved);
                                in_link = true; active_link = idx;
                            }
                        }
                    }
                    "img" => {
                        let alt = extract_attr(&local[..tlen], b"alt");
                        if !alt.is_empty() {
                            push_char!(b'[');
                            for b in alt.as_str().bytes().take(40) { if b.is_ascii() { push_char!(b); } }
                            push_char!(b']');
                        }
                    }
                    "table" | "tr" => { newline!(); }
                    "td" | "th" => {
                        if closing { push_text!(" | "); } else { push_text!("  "); }
                    }
                    "input" => {
                        let itype = extract_attr(&local[..tlen], b"type");
                        let placeholder = extract_attr(&local[..tlen], b"placeholder");
                        if !itype.is_empty() {
                            push_char!(b'[');
                            for b in itype.as_str().bytes().take(12) { if b.is_ascii() { push_char!(b); } }
                            if !placeholder.is_empty() {
                                push_text!(": ");
                                for b in placeholder.as_str().bytes().take(20) { if b.is_ascii() { push_char!(b); } }
                            }
                            push_char!(b']');
                        }
                    }
                    "button" => { if !closing { push_char!(b'['); } else { push_char!(b']'); } }
                    _ => {}
                }
            }
            tag_len = 0;
        }}
    }

    macro_rules! process_entity {
        () => {{
            let entity = core::str::from_utf8(&ent_buf[..ent_len]).unwrap_or("");
            let decoded: &str = match entity {
                "amp" => "&", "lt" => "<", "gt" => ">", "nbsp" => " ",
                "quot" => "\"", "apos" => "'", "copy" => "(c)", "reg" => "(R)",
                "trade" => "(TM)", "mdash" => "--", "ndash" => "-",
                "laquo" => "<<", "raquo" => ">>", "hellip" => "...",
                "bull" => "* ", "middot" => "*", "times" => "x",
                "divide" => "/", "euro" => "EUR", "pound" => "GBP",
                _ => {
                    if entity.starts_with('#') {
                        let n: u32 = if entity.get(1..2) == Some("x") || entity.get(1..2) == Some("X") {
                            u32::from_str_radix(entity.get(2..).unwrap_or(""), 16).unwrap_or(0)
                        } else {
                            entity.get(1..).unwrap_or("").parse::<u32>().unwrap_or(0)
                        };
                        if (32..128).contains(&n) { push_char!(n as u8); }
                        else if n == 160 { push_char!(b' '); }
                    }
                    ent_len = 0;
                    ""
                }
            };
            push_text!(decoded);
            ent_len = 0;
        }}
    }

    for &b in body {
        if in_comment {
            if tag_len < 511 { tag_buf[tag_len] = b; tag_len += 1; }
            if tag_len >= 3
                && tag_buf[tag_len-3] == b'-'
                && tag_buf[tag_len-2] == b'-'
                && tag_buf[tag_len-1] == b'>'
            {
                in_comment = false; tag_len = 0;
            }
            continue;
        }

        if in_entity {
            if b == b';' {
                in_entity = false; process_entity!();
            } else if ent_len < 16 && (b.is_ascii_alphanumeric() || b == b'#') {
                ent_buf[ent_len] = b; ent_len += 1;
            } else {
                push_char!(b'&');
                for k in 0..ent_len { let c = ent_buf[k]; push_char!(c); }
                in_entity = false; ent_len = 0;
                if b == b'<' { in_tag = true; tag_len = 0; }
                else if b != b';' && b.is_ascii() && b >= 32 { push_char!(b); }
            }
            continue;
        }

        if in_tag {
            if b == b'>' {
                in_tag = false;
                if tag_len >= 3 && &tag_buf[..3] == b"!--" {
                    if tag_len < 5 || !(tag_buf[tag_len-2] == b'-' && tag_buf[tag_len-1] == b'-') {
                        in_comment = true; tag_len = 0; continue;
                    }
                }
                process_tag!();
            } else {
                if tag_len < 511 { tag_buf[tag_len] = b; tag_len += 1; }
                if tag_len == 3 && &tag_buf[..3] == b"!--" {
                    in_comment = true; in_tag = false; tag_len = 0;
                }
            }
            continue;
        }

        match b {
            b'<' => { in_tag = true; tag_len = 0; }
            b'&' if !in_script && !in_style => { in_entity = true; ent_len = 0; }
            b'\n' | b'\r' => {
                if !in_script && !in_style {
                    let last = if cur_len > 0 { cur_buf[cur_len - 1] } else { b' ' };
                    if last != b' ' { push_char!(b' '); }
                }
            }
            b'\t' => { if !in_script && !in_style { push_char!(b' '); } }
            _ => {
                if !in_script && !in_style && b >= 32 && b < 128 { push_char!(b); }
            }
        }
    }

    emit_line!();
    PageContent { lines, links, title }
}

// ── HTTP fetch ─────────────────────────────────────────────────────────────────

enum FetchResult {
    Body(Vec<u8>),
    Err(FixStr<128>),
}

fn fetch_url(url: &str) -> FetchResult {
    let pu = match parse_url(url) {
        Some(p) => p,
        None => {
            let mut e: FixStr<128> = FixStr::new();
            e.push_str("Invalid URL: ");
            e.push_str(&url[..url.len().min(60)]);
            return FetchResult::Err(e);
        }
    };

    if pu.is_https {
        let mut e: FixStr<128> = FixStr::new();
        e.push_str("HTTPS not supported (no TLS). Try http://");
        return FetchResult::Err(e);
    }

    let ip = match dns_resolve(pu.host.as_str().as_bytes()) {
        Some(ip) => ip,
        None => {
            let mut e: FixStr<128> = FixStr::new();
            e.push_str("DNS failed: ");
            e.push_str(pu.host.as_str());
            return FetchResult::Err(e);
        }
    };

    let sfd = socket(AF_INET, SOCK_STREAM, 0);
    if sfd < 0 {
        let mut e: FixStr<128> = FixStr::new();
        e.push_str("socket() failed");
        return FetchResult::Err(e);
    }

    let addr = SockAddrIn::new(ip, pu.port);
    if connect(sfd, &addr) < 0 {
        close_socket(sfd);
        let mut e: FixStr<128> = FixStr::new();
        e.push_str("connect() failed: ");
        e.push_str(pu.host.as_str());
        return FetchResult::Err(e);
    }

    sleep_ms(200);

    let req = alloc::format!(
        "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: OxideBrowser/0.1\r\nAccept: text/html,text/plain\r\nConnection: close\r\n\r\n",
        pu.path.as_str(),
        pu.host.as_str()
    );

    let mut sent = false;
    for _ in 0..20 {
        if send(sfd, req.as_bytes()) > 0 { sent = true; break; }
        sleep_ms(100);
    }
    if !sent {
        close_socket(sfd);
        let mut e: FixStr<128> = FixStr::new();
        e.push_str("Request timed out");
        return FetchResult::Err(e);
    }

    let mut raw: Vec<u8> = Vec::with_capacity(256 * 1024);
    let mut chunk = [0u8; 4096];
    let mut idle = 0u32;

    loop {
        let n = recv(sfd, &mut chunk);
        match n {
            0    => break,
            -11  => { idle += 1; if idle > 500 { break; } sleep_ms(10); }
            n if n > 0 => {
                idle = 0;
                let nb = n as usize;
                if raw.len() + nb <= 512 * 1024 {
                    raw.extend_from_slice(&chunk[..nb]);
                } else {
                    let avail = 512 * 1024 - raw.len();
                    raw.extend_from_slice(&chunk[..avail]);
                    break;
                }
            }
            _ => break,
        }
    }
    close_socket(sfd);

    if raw.starts_with(b"HTTP/") {
        let status_line_end = raw.iter().position(|&b| b == b'\n').unwrap_or(raw.len());
        let status_line = &raw[..status_line_end];
        let code_start = status_line.windows(4)
            .position(|w| w[0] == b' ')
            .map(|i| i + 1)
            .unwrap_or(9);
        let code_bytes = &status_line[code_start..code_start.min(status_line.len()).saturating_add(3)];
        let code: u16 = core::str::from_utf8(code_bytes).unwrap_or("0").trim().parse().unwrap_or(0);

        if code == 301 || code == 302 || code == 307 || code == 308 {
            if let Some(loc_start) = find_header(&raw, b"Location: ") {
                let loc_end = raw[loc_start..].iter().position(|&b| b == b'\r' || b == b'\n')
                    .map(|i| loc_start + i)
                    .unwrap_or(raw.len());
                if let Ok(location) = core::str::from_utf8(&raw[loc_start..loc_end]) {
                    let location = location.trim();
                    if !location.is_empty() { return fetch_url(location); }
                }
            }
        }
    }

    let body_start = raw.windows(4).position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
        .or_else(|| raw.windows(2).position(|w| w == b"\n\n").map(|i| i + 2))
        .unwrap_or(0);

    FetchResult::Body(raw[body_start..].to_vec())
}

fn find_header(raw: &[u8], header: &[u8]) -> Option<usize> {
    raw.windows(header.len()).position(|w| {
        w.iter().zip(header.iter()).all(|(a, b)| a.to_ascii_lowercase() == b.to_ascii_lowercase())
    }).map(|i| i + header.len())
}

// ── SHM serialisation (called in child process) ────────────────────────────────

unsafe fn shm_write_page(ptr: *mut u8, content: &PageContent, base_url: &str, body_size: usize) {
    let line_count = content.lines.len().min(MAX_LINES_SHM);
    let link_count = content.links.len().min(MAX_LINKS_SHM);

    let title_str = if !content.title.is_empty() { content.title.as_str() } else { base_url };
    let title_bytes = title_str.as_bytes();
    let title_len = title_bytes.len().min(128);

    let mut stat: FixStr<120> = FixStr::new();
    stat.push_str("Loaded: ");
    stat.push_str(&title_str[..title_str.len().min(60)]);
    stat.push_str("  (");
    let mut sz_buf = [0u8; 32];
    stat.push_str(fmt_size(body_size, &mut sz_buf));
    stat.push_str(")");
    let stat_bytes = stat.as_str().as_bytes();
    let stat_len = stat_bytes.len().min(120);

    unsafe {
        shm_write_u32(ptr, SHMO_STATUS,    1); // ok
        shm_write_u32(ptr, SHMO_LINE_CNT,  line_count as u32);
        shm_write_u32(ptr, SHMO_LINK_CNT,  link_count as u32);
        shm_write_u32(ptr, SHMO_TITLE_LEN, title_len as u32);
        core::ptr::copy_nonoverlapping(title_bytes.as_ptr(), ptr.add(SHMO_TITLE), title_len);
        shm_write_u32(ptr, SHMO_SMSG_LEN, stat_len as u32);
        core::ptr::copy_nonoverlapping(stat_bytes.as_ptr(), ptr.add(SHMO_SMSG), stat_len);
        shm_write_u32(ptr, SHMO_SCOL, SCOL_OK);

        for (i, line) in content.lines.iter().take(line_count).enumerate() {
            let off = SHMO_LINES + i * LINE_BYTES;
            core::ptr::copy_nonoverlapping(line.text.as_ptr(), ptr.add(off), 100);
            shm_write_u32(ptr, off + 100, line.len as u32);
            shm_write_u32(ptr, off + 104, line.color);
            shm_write_u16(ptr, off + 108, line.link_idx);
        }

        for (i, link) in content.links.iter().take(link_count).enumerate() {
            let off = SHMO_LINKS + i * LINK_BYTES;
            let lb = link.as_str().as_bytes();
            let ll = lb.len().min(256);
            shm_write_u32(ptr, off, ll as u32);
            core::ptr::copy_nonoverlapping(lb.as_ptr(), ptr.add(off + 4), ll);
        }
    }
}

unsafe fn shm_write_error(ptr: *mut u8, msg: &str) {
    let msg_bytes = msg.as_bytes();
    let msg_len = msg_bytes.len().min(120);

    unsafe {
        shm_write_u32(ptr, SHMO_STATUS,   2); // err
        shm_write_u32(ptr, SHMO_LINE_CNT, 0);
        shm_write_u32(ptr, SHMO_LINK_CNT, 0);
        shm_write_u32(ptr, SHMO_TITLE_LEN, 0);
        shm_write_u32(ptr, SHMO_SMSG_LEN, msg_len as u32);
        core::ptr::copy_nonoverlapping(msg_bytes.as_ptr(), ptr.add(SHMO_SMSG), msg_len);
        shm_write_u32(ptr, SHMO_SCOL, SCOL_ERR);
    }
}

// ── SHM deserialisation (called in parent after child signals done) ────────────

struct FetchedPage {
    content:    PageContent,
    status_msg: FixStr<120>,
    status_col: u32,
    is_err:     bool,
}

unsafe fn shm_read_page(ptr: *const u8) -> FetchedPage {
    unsafe {
        let line_count = shm_read_u32(ptr, SHMO_LINE_CNT) as usize;
        let link_count = shm_read_u32(ptr, SHMO_LINK_CNT) as usize;

        let title_len = shm_read_u32(ptr, SHMO_TITLE_LEN) as usize;
        let title_len = title_len.min(128);
        let mut title: FixStr<128> = FixStr::new();
        for i in 0..title_len { title.push_byte(*ptr.add(SHMO_TITLE + i)); }

        let smsg_len = shm_read_u32(ptr, SHMO_SMSG_LEN) as usize;
        let smsg_len = smsg_len.min(120);
        let mut status_msg: FixStr<120> = FixStr::new();
        for i in 0..smsg_len { status_msg.push_byte(*ptr.add(SHMO_SMSG + i)); }

        let scol_code = shm_read_u32(ptr, SHMO_SCOL);
        let status_col = decode_status_color(scol_code);
        let is_err = shm_read_u32(ptr, SHMO_STATUS) == 2;

        let line_count = line_count.min(MAX_LINES_SHM);
        let mut lines: Vec<RenderLine> = Vec::with_capacity(line_count);
        for i in 0..line_count {
            let off = SHMO_LINES + i * LINE_BYTES;
            let mut line = RenderLine::empty();
            core::ptr::copy_nonoverlapping(ptr.add(off), line.text.as_mut_ptr(), 100);
            line.len      = shm_read_u32(ptr, off + 100) as usize;
            line.color    = shm_read_u32(ptr, off + 104);
            line.link_idx = shm_read_u16(ptr, off + 108);
            lines.push(line);
        }

        let link_count = link_count.min(MAX_LINKS_SHM);
        let mut links: Vec<FixStr<512>> = Vec::with_capacity(link_count);
        for i in 0..link_count {
            let off = SHMO_LINKS + i * LINK_BYTES;
            let ll = shm_read_u32(ptr, off) as usize;
            let ll = ll.min(256);
            let mut link: FixStr<512> = FixStr::new();
            for j in 0..ll { link.push_byte(*ptr.add(off + 4 + j)); }
            links.push(link);
        }

        let content = PageContent { lines, links, title };
        FetchedPage { content, status_msg, status_col, is_err }
    }
}

// ── Draw helpers ───────────────────────────────────────────────────────────────

fn draw_button(win: GuiWindow, x: u32, y: u32, w: u32, h: u32, label: &str, active: bool) {
    let bg = if active { C_LINK } else { C_BTN_BG };
    let fg = if active { 0xFF0D1117 } else { C_BTN_FG };
    gui_fill_rect(win, x, y, w, h, bg);
    gui_fill_rect(win, x, y, w, 1, C_BTN_BORD);
    gui_fill_rect(win, x, y+h-1, w, 1, C_BTN_BORD);
    gui_fill_rect(win, x, y, 1, h, C_BTN_BORD);
    gui_fill_rect(win, x+w-1, y, 1, h, C_BTN_BORD);
    let text_w = label.len() as u32 * CHAR_W;
    let tx = x + w.saturating_sub(text_w) / 2;
    let ty = y + h.saturating_sub(LINE_H) / 2;
    gui_draw_text(win, tx, ty, fg, label);
}

fn draw_toolbar(
    win: GuiWindow,
    url: &str,
    cursor: usize,
    url_focused: bool,
    can_back: bool,
    can_fwd: bool,
    loading: bool,
    anim_tick: u32,
) {
    let url_input_w = win.width.saturating_sub(BTN_URL_X + BTN_GO_W + 8);

    gui_fill_rect(win, 0, 0, win.width, TOOLBAR_H, C_PANEL);
    gui_fill_rect(win, 0, TOOLBAR_H, win.width, 1, C_BORDER);

    draw_button(win, BTN_BACK_X, BTN_Y, BTN_BACK_W, BTN_H, "<-", false);
    draw_button(win, BTN_FWD_X,  BTN_Y, BTN_FWD_W,  BTN_H, "->", false);
    if !can_back { gui_fill_rect(win, BTN_BACK_X, BTN_Y, BTN_BACK_W, BTN_H, 0xFF0D0D14); gui_draw_text(win, BTN_BACK_X+3, BTN_Y+4, C_DIM, "<-"); }
    if !can_fwd  { gui_fill_rect(win, BTN_FWD_X,  BTN_Y, BTN_FWD_W,  BTN_H, 0xFF0D0D14); gui_draw_text(win, BTN_FWD_X+3,  BTN_Y+4, C_DIM, "->"); }

    // URL bar
    let border_col = if url_focused { C_CURSOR } else { C_BTN_BORD };
    gui_fill_rect(win, BTN_URL_X, BTN_Y, url_input_w, BTN_H, C_INPUT_BG);
    gui_fill_rect(win, BTN_URL_X, BTN_Y, url_input_w, 1, border_col);
    gui_fill_rect(win, BTN_URL_X, BTN_Y + BTN_H - 1, url_input_w, 1, border_col);
    gui_fill_rect(win, BTN_URL_X, BTN_Y, 1, BTN_H, border_col);
    gui_fill_rect(win, BTN_URL_X + url_input_w - 1, BTN_Y, 1, BTN_H, border_col);

    let max_url_chars = (url_input_w.saturating_sub(8) / CHAR_W) as usize;
    let url_bytes = url.as_bytes();
    let disp_start = if url.len() > max_url_chars { url.len() - max_url_chars } else { 0 };
    let disp = core::str::from_utf8(&url_bytes[disp_start..]).unwrap_or(url);
    gui_draw_text(win, BTN_URL_X + 4, BTN_Y + 5, C_INPUT_FG, disp);

    if url_focused {
        let cur_x = BTN_URL_X + 4 + cursor.min(url.len()).saturating_sub(disp_start) as u32 * CHAR_W;
        gui_fill_rect(win, cur_x, BTN_Y + 3, 2, BTN_H - 6, C_CURSOR);
    }

    // Go / Stop button
    let go_x = BTN_URL_X + url_input_w + 4;
    if loading {
        // Pulsing stop button
        let pulse = (anim_tick / 8) % 2 == 0;
        let bg = if pulse { 0xFFB22222 } else { 0xFF8B0000 };
        gui_fill_rect(win, go_x, BTN_Y, BTN_GO_W, BTN_H, bg);
        gui_fill_rect(win, go_x, BTN_Y, BTN_GO_W, 1, 0xFFFF4444);
        gui_fill_rect(win, go_x, BTN_Y+BTN_H-1, BTN_GO_W, 1, 0xFFFF4444);
        gui_fill_rect(win, go_x, BTN_Y, 1, BTN_H, 0xFFFF4444);
        gui_fill_rect(win, go_x+BTN_GO_W-1, BTN_Y, 1, BTN_H, 0xFFFF4444);
        gui_draw_text(win, go_x + 10, BTN_Y + 5, 0xFFFFAAAA, " Stop");
    } else {
        draw_button(win, go_x, BTN_Y, BTN_GO_W, BTN_H, "  Go  ", false);
    }
}

fn draw_content(win: GuiWindow, lines: &[RenderLine], scroll: usize, hover_line: Option<usize>) {
    gui_fill_rect(win, 0, CONTENT_Y, win.width, CONTENT_H, C_BG);

    for row in 0..VISIBLE_LINES {
        let line_idx = scroll + row;
        if line_idx >= lines.len() { break; }
        let line = &lines[line_idx];
        let y = CONTENT_Y + row as u32 * LINE_H;
        if y + LINE_H > CONTENT_Y + CONTENT_H { break; }

        let color = if hover_line == Some(line_idx) && line.link_idx != 0xFFFF {
            C_LINK_HOV
        } else {
            line.color
        };

        if line.len > 0 {
            gui_draw_text(win, PAD_X, y + 1, color, line.as_str());
        }
    }
}

fn draw_status(win: GuiWindow, status: &str, status_color: u32) {
    gui_fill_rect(win, 0, STATUS_Y - 1, win.width, 1, C_BORDER);
    gui_fill_rect(win, 0, STATUS_Y, win.width, STATUSBAR_H, C_PANEL);
    gui_draw_text(win, PAD_X, STATUS_Y + 2, status_color, status);
}

// ── Number formatting ──────────────────────────────────────────────────────────

fn fmt_size(bytes: usize, buf: &mut [u8; 32]) -> &str {
    let mut len = 0usize;
    let (val, unit) = if bytes >= 1024 * 1024 {
        (bytes / 1024 / 1024, " MB")
    } else if bytes >= 1024 {
        (bytes / 1024, " KB")
    } else {
        (bytes, " B")
    };
    let mut tmp = [0u8; 16];
    let mut i = tmp.len();
    let mut v = val;
    if v == 0 { i -= 1; tmp[i] = b'0'; }
    while v > 0 { i -= 1; tmp[i] = b'0' + (v % 10) as u8; v /= 10; }
    for &b in &tmp[i..] { buf[len] = b; len += 1; }
    for &b in unit.as_bytes() { buf[len] = b; len += 1; }
    core::str::from_utf8(&buf[..len]).unwrap_or("?")
}

// ── Navigation history ─────────────────────────────────────────────────────────

const HIST_CAP: usize = 16;
const URL_CAP:  usize = 256;

struct NavHistory {
    entries: [[u8; URL_CAP]; HIST_CAP],
    lens:    [usize; HIST_CAP],
    count:   usize,
    pos:     usize,
}

impl NavHistory {
    fn new() -> Self {
        Self { entries: [[0u8; URL_CAP]; HIST_CAP], lens: [0usize; HIST_CAP], count: 0, pos: 0 }
    }

    fn push(&mut self, url: &str) {
        if self.pos + 1 < self.count { self.count = self.pos + 1; }
        if self.count >= HIST_CAP {
            for i in 0..HIST_CAP - 1 {
                self.entries[i] = self.entries[i + 1];
                self.lens[i] = self.lens[i + 1];
            }
            self.count = HIST_CAP - 1;
            if self.pos > 0 { self.pos -= 1; }
        }
        let idx = self.count;
        let n = url.len().min(URL_CAP);
        self.entries[idx][..n].copy_from_slice(&url.as_bytes()[..n]);
        self.lens[idx] = n;
        self.count += 1;
        self.pos = idx;
    }

    fn current(&self) -> &str {
        if self.count == 0 { return ""; }
        core::str::from_utf8(&self.entries[self.pos][..self.lens[self.pos]]).unwrap_or("")
    }

    fn can_back(&self) -> bool { self.pos > 0 }
    fn can_fwd(&self)  -> bool { self.pos + 1 < self.count }

    fn go_back(&mut self) -> &str {
        if self.can_back() { self.pos -= 1; }
        self.current()
    }

    fn go_fwd(&mut self) -> &str {
        if self.can_fwd() { self.pos += 1; }
        self.current()
    }
}

// ── Async fetch state ──────────────────────────────────────────────────────────

enum FetchState {
    Idle,
    Loading {
        pipe_r:   i32,   // read end; non-blocking EAGAIN until child done
        shm_id:   i32,   // SHM segment id (for detach)
        shm_ptr:  usize, // parent's virtual address of SHM
        anim_tick: u32,
    },
}

// ── Browser state ──────────────────────────────────────────────────────────────

struct Browser {
    win:         GuiWindow,
    url_bar:     FixStr<URL_CAP>,
    url_cursor:  usize,
    url_focused: bool,
    history:     NavHistory,
    page:        Option<PageContent>,
    scroll:      usize,
    hover_line:  Option<usize>,
    status:      FixStr<120>,
    status_col:  u32,
    dirty:       bool,
    fetch_state: FetchState,
    shm_key:     u32, // fixed SHM key for this browser instance
}

impl Browser {
    fn new(win: GuiWindow) -> Self {
        let mut s = Self {
            win,
            url_bar:     FixStr::new(),
            url_cursor:  0,
            url_focused: true,
            history:     NavHistory::new(),
            page:        None,
            scroll:      0,
            hover_line:  None,
            status:      FixStr::new(),
            status_col:  C_STATUS_FG,
            dirty:       true,
            fetch_state: FetchState::Idle,
            shm_key:     getpid() ^ 0x4242_0000,
        };
        s.set_status("Type a URL and press Enter  (e.g. http://example.com)", C_DIM);
        s
    }

    fn set_status(&mut self, msg: &str, color: u32) {
        self.status.clear();
        self.status.push_str(msg);
        self.status_col = color;
    }

    fn is_loading(&self) -> bool {
        matches!(self.fetch_state, FetchState::Loading { .. })
    }

    // Cancel an in-progress fetch (e.g. user clicked Stop).
    fn cancel_fetch(&mut self) {
        if let FetchState::Loading { pipe_r, shm_id: _, shm_ptr, .. } = self.fetch_state {
            fd_close(pipe_r);
            unsafe { shmdt(shm_ptr); }
            self.fetch_state = FetchState::Idle;
            self.set_status("Cancelled", C_WARN);
            self.dirty = true;
        }
    }

    // Start an async navigation to `url`.  If already loading, cancel first.
    fn start_navigate(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() { return; }

        if self.is_loading() {
            self.cancel_fetch();
        }

        // Build full URL
        let mut full: FixStr<512> = FixStr::new();
        if !url.starts_with("http://") && !url.starts_with("https://") {
            full.push_str("http://");
        }
        full.push_str(url);
        let full_str = full.as_str();

        // Update URL bar
        self.url_bar.clear();
        self.url_bar.push_str(full_str);
        self.url_cursor = self.url_bar.len();
        self.url_focused = false;

        // Create / reuse SHM
        let shm_id = shmget(self.shm_key, SHM_SIZE as usize);
        if shm_id < 0 {
            self.set_status("Error: could not allocate shared memory", C_ERR);
            self.dirty = true;
            return;
        }

        let shm_ptr = unsafe { shmat(shm_id as u32) } as usize;
        if shm_ptr == 0 || shm_ptr as i64 == -1 {
            self.set_status("Error: could not map shared memory", C_ERR);
            self.dirty = true;
            return;
        }

        // Create pipe for done-signal
        let mut pipe_r: i32 = -1;
        let mut pipe_w: i32 = -1;
        if unsafe { os_pipe(&mut pipe_r as *mut i32, &mut pipe_w as *mut i32) } < 0 {
            unsafe { shmdt(shm_ptr); }
            self.set_status("Error: could not create pipe", C_ERR);
            self.dirty = true;
            return;
        }

        // Copy URL to a stack buffer before fork (so child has a self-contained copy)
        let mut url_buf = [0u8; 512];
        let ulen = full_str.len().min(511);
        url_buf[..ulen].copy_from_slice(&full_str.as_bytes()[..ulen]);

        // ── FORK ──────────────────────────────────────────────────────────────
        let child_pid = fork();

        if child_pid == 0 {
            // ── CHILD ────────────────────────────────────────────────────────
            fd_close(pipe_r);  // child doesn't read

            let child_url = core::str::from_utf8(&url_buf[..ulen]).unwrap_or("");
            let child_shm = unsafe { shmat(shm_id as u32) } as *mut u8;

            if child_shm.is_null() || child_shm as i64 == -1 {
                fd_write(pipe_w, &[2u8]);
                fd_close(pipe_w);
                exit(1);
            }

            match fetch_url(child_url) {
                FetchResult::Body(body) => {
                    let size = body.len();
                    let content = parse_html(&body, COLS, child_url);
                    unsafe { shm_write_page(child_shm, &content, child_url, size); }
                }
                FetchResult::Err(e) => {
                    unsafe { shm_write_error(child_shm, e.as_str()); }
                }
            }

            fd_write(pipe_w, &[1u8]);
            fd_close(pipe_w);
            unsafe { shmdt(child_shm as usize); }
            exit(0);
        }

        // ── PARENT ────────────────────────────────────────────────────────────
        if child_pid < 0 {
            fd_close(pipe_r);
            fd_close(pipe_w);
            unsafe { shmdt(shm_ptr); }
            self.set_status("Error: fork failed", C_ERR);
            self.dirty = true;
            return;
        }

        fd_close(pipe_w); // parent only reads
        self.set_status("Loading...", C_WARN);
        self.fetch_state = FetchState::Loading {
            pipe_r,
            shm_id: shm_id as i32,
            shm_ptr,
            anim_tick: 0,
        };
        self.dirty = true;
    }

    // Called each frame.  Polls the pipe non-blocking; when child signals done,
    // reads the page from SHM and updates the browser display.
    fn poll_fetch(&mut self) {
        let (pipe_r, shm_id, shm_ptr) = match self.fetch_state {
            FetchState::Loading { pipe_r, shm_id, shm_ptr, ref mut anim_tick } => {
                *anim_tick += 1;
                if *anim_tick % 15 == 0 { self.dirty = true; } // animate status dots
                (pipe_r, shm_id, shm_ptr)
            }
            FetchState::Idle => return,
        };

        let mut sig = [0u8; 1];
        let n = fd_read(pipe_r, &mut sig);

        if n == -6 { return; } // EAGAIN: still loading

        // EOF (0) or received byte (1+): child finished or died
        fd_close(pipe_r);

        let fetched = unsafe { shm_read_page(shm_ptr as *const u8) };
        unsafe { shmdt(shm_ptr); }

        let _ = shm_id; // SHM stays in kernel table; reused next fetch via same key

        // Update loading animation to show what the status will look like
        let url_str = self.url_bar.as_str().to_owned();

        if fetched.is_err {
            // Show error page
            let mut lines: Vec<RenderLine> = Vec::new();
            let add = |lines: &mut Vec<RenderLine>, text: &str, color: u32| {
                let mut line = RenderLine::empty();
                let b = text.as_bytes();
                let n = b.len().min(LINE_BUF);
                line.text[..n].copy_from_slice(&b[..n]);
                line.len = n;
                line.color = color;
                lines.push(line);
            };
            add(&mut lines, "", C_TEXT);
            add(&mut lines, "  Cannot load page", C_H1);
            add(&mut lines, "", C_TEXT);
            let msg = fetched.status_msg.as_str();
            let mut err_line = RenderLine::empty();
            let prefix = b"  ";
            let err_bytes = msg.as_bytes();
            let n = (prefix.len() + err_bytes.len()).min(LINE_BUF);
            err_line.text[..prefix.len().min(n)].copy_from_slice(&prefix[..prefix.len().min(n)]);
            if prefix.len() < n {
                err_line.text[prefix.len()..n].copy_from_slice(&err_bytes[..n - prefix.len()]);
            }
            err_line.len = n;
            err_line.color = C_ERR;
            lines.push(err_line);
            self.page = Some(PageContent { lines, links: Vec::new(), title: FixStr::new() });
        } else {
            self.history.push(&url_str);
            self.page = Some(fetched.content);
        }

        self.status = fetched.status_msg;
        self.status_col = fetched.status_col;
        self.scroll = 0;
        self.hover_line = None;
        self.fetch_state = FetchState::Idle;
        self.dirty = true;
    }

    fn redraw(&mut self) {
        let (lines, _links) = self.page.as_ref()
            .map(|p| (p.lines.as_slice(), p.links.as_slice()))
            .unwrap_or((&[], &[]));

        let (loading, anim_tick) = match &self.fetch_state {
            FetchState::Loading { anim_tick, .. } => (true, *anim_tick),
            FetchState::Idle => (false, 0),
        };

        // Animate status dots while loading
        let status_str: FixStr<120> = if loading {
            let dots = match (anim_tick / 15) % 4 { 0 => "   ", 1 => ".  ", 2 => ".. ", _ => "..." };
            let mut s: FixStr<120> = FixStr::new();
            s.push_str("Loading");
            s.push_str(dots);
            s.push_str("  ");
            s.push_str(&self.url_bar.as_str()[..self.url_bar.len().min(60)]);
            s
        } else {
            self.status
        };

        draw_toolbar(self.win, self.url_bar.as_str(), self.url_cursor,
                     self.url_focused, self.history.can_back(), self.history.can_fwd(),
                     loading, anim_tick);
        draw_content(self.win, lines, self.scroll, self.hover_line);
        draw_status(self.win, status_str.as_str(), if loading { C_WARN } else { self.status_col });
        gui_present(self.win);
    }

    fn scroll_down(&mut self) {
        if let Some(p) = &self.page {
            let max_scroll = p.lines.len().saturating_sub(VISIBLE_LINES);
            if self.scroll < max_scroll {
                self.scroll = (self.scroll + 3).min(max_scroll);
                self.dirty = true;
            }
        }
    }

    fn scroll_up(&mut self) {
        if self.scroll > 0 {
            self.scroll = self.scroll.saturating_sub(3);
            self.dirty = true;
        }
    }

    fn page_down(&mut self) {
        if let Some(p) = &self.page {
            let max_scroll = p.lines.len().saturating_sub(VISIBLE_LINES);
            self.scroll = (self.scroll + VISIBLE_LINES).min(max_scroll);
            self.dirty = true;
        }
    }

    fn page_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(VISIBLE_LINES);
        self.dirty = true;
    }

    fn handle_key(&mut self, k: u8) {
        if self.url_focused {
            match k {
                b'\r' | b'\n' => {
                    let url = self.url_bar.as_str().to_owned();
                    self.start_navigate(&url);
                }
                b'\x1b' => { self.url_focused = false; self.dirty = true; }
                8 | 127 => {
                    if self.url_cursor > 0 {
                        self.url_cursor -= 1;
                        self.url_bar.remove(self.url_cursor);
                        self.dirty = true;
                    }
                }
                3 => { self.url_bar.clear(); self.url_cursor = 0; self.dirty = true; }
                c if c >= 32 && c < 127 => {
                    self.url_bar.insert(self.url_cursor, c);
                    self.url_cursor += 1;
                    self.dirty = true;
                }
                _ => {}
            }
        } else {
            match k {
                b'\x1b' => {}
                12 => { self.url_focused = true; self.dirty = true; } // Ctrl-L
                _ => {}
            }
        }
    }

    fn handle_escape_seq(&mut self, seq: u8) {
        match seq {
            b'A' => self.scroll_up(),
            b'B' => self.scroll_down(),
            b'5' => self.page_up(),
            b'6' => self.page_down(),
            b'C' => {
                if self.url_focused && self.url_cursor < self.url_bar.len() {
                    self.url_cursor += 1; self.dirty = true;
                }
            }
            b'D' => {
                if self.url_focused && self.url_cursor > 0 {
                    self.url_cursor -= 1; self.dirty = true;
                }
            }
            _ => {}
        }
    }

    fn handle_mouse_click(&mut self, mx: u16, my: u16) {
        let x = mx as u32;
        let y = my as u32;

        let url_input_w = self.win.width.saturating_sub(BTN_URL_X + BTN_GO_W + 8);

        // URL bar
        if x >= BTN_URL_X && x < BTN_URL_X + url_input_w && y >= BTN_Y && y < BTN_Y + BTN_H {
            self.url_focused = true;
            self.url_cursor = self.url_bar.len();
            self.dirty = true;
            return;
        }
        self.url_focused = false;

        // Back
        if x >= BTN_BACK_X && x < BTN_BACK_X + BTN_BACK_W && y >= BTN_Y && y < BTN_Y + BTN_H {
            if self.history.can_back() && !self.is_loading() {
                let url = self.history.go_back().to_owned();
                self.url_bar.clear(); self.url_bar.push_str(&url);
                self.url_cursor = self.url_bar.len();
                self.start_navigate(&url);
            }
            return;
        }

        // Forward
        if x >= BTN_FWD_X && x < BTN_FWD_X + BTN_FWD_W && y >= BTN_Y && y < BTN_Y + BTN_H {
            if self.history.can_fwd() && !self.is_loading() {
                let url = self.history.go_fwd().to_owned();
                self.url_bar.clear(); self.url_bar.push_str(&url);
                self.url_cursor = self.url_bar.len();
                self.start_navigate(&url);
            }
            return;
        }

        // Go / Stop button
        let go_x = BTN_URL_X + url_input_w + 4;
        if x >= go_x && x < go_x + BTN_GO_W && y >= BTN_Y && y < BTN_Y + BTN_H {
            if self.is_loading() {
                self.cancel_fetch();
            } else {
                let url = self.url_bar.as_str().to_owned();
                self.start_navigate(&url);
            }
            return;
        }

        // Click content area → follow link
        if y >= CONTENT_Y && y < CONTENT_Y + CONTENT_H && !self.is_loading() {
            let row = (y - CONTENT_Y) / LINE_H;
            let line_idx = self.scroll + row as usize;
            if let Some(page) = &self.page {
                if line_idx < page.lines.len() {
                    let link_idx = page.lines[line_idx].link_idx;
                    if link_idx != 0xFFFF && (link_idx as usize) < page.links.len() {
                        let link_url = page.links[link_idx as usize].as_str().to_owned();
                        self.url_bar.clear();
                        self.url_bar.push_str(&link_url);
                        self.url_cursor = self.url_bar.len();
                        self.start_navigate(&link_url);
                        return;
                    }
                }
            }
        }

        self.dirty = true;
    }

    fn handle_mouse_move(&mut self, _mx: u16, my: u16) {
        let y = my as u32;
        if y >= CONTENT_Y && y < CONTENT_Y + CONTENT_H {
            let row = (y - CONTENT_Y) / LINE_H;
            let line_idx = self.scroll + row as usize;
            let has_link = self.page.as_ref()
                .and_then(|p| p.lines.get(line_idx))
                .map(|l| l.link_idx != 0xFFFF)
                .unwrap_or(false);
            let new_hover = if has_link { Some(line_idx) } else { None };
            if new_hover != self.hover_line { self.hover_line = new_hover; self.dirty = true; }
        } else if self.hover_line.is_some() {
            self.hover_line = None; self.dirty = true;
        }
    }
}

// ── Home page ──────────────────────────────────────────────────────────────────

fn make_home_page() -> PageContent {
    let mut lines: Vec<RenderLine> = Vec::new();

    let add = |lines: &mut Vec<RenderLine>, text: &[u8], color: u32| {
        let mut line = RenderLine::empty();
        let n = text.len().min(LINE_BUF);
        line.text[..n].copy_from_slice(&text[..n]);
        line.len = n;
        line.color = color;
        lines.push(line);
    };

    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  OxideBrowser", C_H1);
    add(&mut lines, b"  Lightweight HTTP browser for OxideOS", C_H2);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  ----------------------------------------", C_BORDER);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  Usage:", C_H2);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  1. Click the URL bar (or press Ctrl+L)", C_TEXT);
    add(&mut lines, b"  2. Type a URL:  http://example.com", C_TEXT);
    add(&mut lines, b"  3. Press Enter or click [Go]", C_TEXT);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  While loading: click [Stop] to cancel.", C_TEXT);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  Keyboard shortcuts:", C_H2);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  Ctrl+L       Focus URL bar", C_TEXT);
    add(&mut lines, b"  Up / Down    Scroll content", C_TEXT);
    add(&mut lines, b"  PgUp / PgDn  Page scroll", C_TEXT);
    add(&mut lines, b"  Click link   Navigate to link", C_TEXT);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  Limitations:", C_H2);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  * HTTP only (no HTTPS/TLS)", C_TEXT);
    add(&mut lines, b"  * Text rendering (no images, no CSS)", C_TEXT);
    add(&mut lines, b"  * No JavaScript", C_TEXT);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  ----------------------------------------", C_BORDER);
    add(&mut lines, b"", C_TEXT);
    add(&mut lines, b"  Try: http://example.com", C_LINK);
    add(&mut lines, b"", C_TEXT);

    PageContent { lines, links: Vec::new(), title: FixStr::new() }
}

// ── Entry point ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let win = match gui_create("OxideBrowser", WIN_W, WIN_H) {
        Some(w) => w,
        None    => exit(1),
    };

    let mut browser = Browser::new(win);
    browser.page = Some(make_home_page());
    browser.dirty = true;

    browser.redraw();
    browser.dirty = false;

    loop {
        // Poll async fetch first (non-blocking).
        browser.poll_fetch();

        let Some(ev) = gui_poll_event(win) else {
            if browser.dirty {
                browser.redraw();
                browser.dirty = false;
            }
            sleep_ms(16);
            continue;
        };

        match ev.kind {
            GuiEvent::CLOSE => {
                if browser.is_loading() { browser.cancel_fetch(); }
                exit(0);
            }

            GuiEvent::KEY => {
                let k = ev.data[0];
                match k {
                    0x1B => {
                        sleep_ms(5);
                        if let Some(ev2) = gui_poll_event(win) {
                            if ev2.kind == GuiEvent::KEY && ev2.data[0] == b'[' {
                                sleep_ms(5);
                                if let Some(ev3) = gui_poll_event(win) {
                                    if ev3.kind == GuiEvent::KEY {
                                        browser.handle_escape_seq(ev3.data[0]);
                                    }
                                }
                            } else if ev2.kind == GuiEvent::KEY {
                                match ev2.data[0] {
                                    b'b' | b'B' => {
                                        if browser.history.can_back() && !browser.is_loading() {
                                            let url = browser.history.go_back().to_owned();
                                            browser.url_bar.clear();
                                            browser.url_bar.push_str(&url);
                                            browser.url_cursor = browser.url_bar.len();
                                            browser.start_navigate(&url);
                                        }
                                    }
                                    b'f' | b'F' => {
                                        if browser.history.can_fwd() && !browser.is_loading() {
                                            let url = browser.history.go_fwd().to_owned();
                                            browser.url_bar.clear();
                                            browser.url_bar.push_str(&url);
                                            browser.url_cursor = browser.url_bar.len();
                                            browser.start_navigate(&url);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            browser.handle_key(k);
                        }
                    }
                    _ => browser.handle_key(k),
                }
            }

            GuiEvent::MOUSE_BTN => {
                let x = u16::from_le_bytes([ev.data[0], ev.data[1]]);
                let y = u16::from_le_bytes([ev.data[2], ev.data[3]]);
                let pressed = ev.data[5] != 0;
                if pressed { browser.handle_mouse_click(x, y); }
            }

            GuiEvent::MOUSE_MOVE => {
                let x = u16::from_le_bytes([ev.data[0], ev.data[1]]);
                let y = u16::from_le_bytes([ev.data[2], ev.data[3]]);
                browser.handle_mouse_move(x, y);
            }

            _ => {}
        }

        if browser.dirty {
            browser.redraw();
            browser.dirty = false;
        }
    }
}
