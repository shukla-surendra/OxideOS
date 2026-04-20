//! OxideOS System Monitor
//!
//! A GUI application showing real-time memory, uptime, and process stats.
//! Demonstrates the oxide-widgets UI library.
#![no_std]
#![no_main]

use oxide_rt::{exit, sleep_ms, gui_create, GuiEvent};
use oxide_widgets::{Canvas, Theme, Rect, Label, ProgressBar, Button, fmt_u32, fmt_percent};

const WIN_W: u32 = 480;
const WIN_H: u32 = 340;

const PAD:    u32 = 16;
const ROW_H:  u32 = 22;
const BAR_H:  u32 = 16;
const CHAR_W: u32 = 9;

// ── SystemInfo (must match kernel layout) ──────────────────────────────────────
#[repr(C)]
struct SystemInfo {
    total_memory:  u64,
    free_memory:   u64,
    uptime_ms:     u64,
    process_count: u32,
}

fn get_system_info() -> SystemInfo {
    let mut info = SystemInfo {
        total_memory: 0, free_memory: 0, uptime_ms: 0, process_count: 0,
    };
    unsafe {
        core::arch::asm!(
            "int 0x80",
            inlateout("rax") 402u64 => _,
            in("rdi") &mut info as *mut SystemInfo as u64,
            options(nostack)
        );
    }
    info
}

// ── Tiny fixed-capacity string for dynamic content ─────────────────────────────
#[derive(Copy, Clone)]
struct Str<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Str<N> {
    const fn new() -> Self { Self { buf: [0u8; N], len: 0 } }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("?")
    }

    fn set(&mut self, s: &str) {
        let src = s.as_bytes();
        let n = src.len().min(N);
        self.buf[..n].copy_from_slice(&src[..n]);
        self.len = n;
    }
}

fn write_str<const N: usize>(s: &mut Str<N>, text: &str) {
    s.set(text);
}

fn append_str<const N: usize>(s: &mut Str<N>, text: &str) {
    let src = text.as_bytes();
    let avail = N - s.len;
    let n = src.len().min(avail);
    s.buf[s.len..s.len + n].copy_from_slice(&src[..n]);
    s.len += n;
}

fn append_u32<const N: usize>(s: &mut Str<N>, n: u32) {
    let mut tmp = [0u8; 20];
    let num_str = fmt_u32(n, &mut tmp);
    append_str(s, num_str);
}

fn format_bytes<const N: usize>(s: &mut Str<N>, bytes: u64) {
    s.len = 0;
    if bytes >= 1024 * 1024 * 1024 {
        append_u32(s, (bytes / (1024 * 1024 * 1024)) as u32);
        append_str(s, ".");
        append_u32(s, ((bytes % (1024 * 1024 * 1024)) / (100 * 1024 * 1024)) as u32);
        append_str(s, " GB");
    } else if bytes >= 1024 * 1024 {
        append_u32(s, (bytes / (1024 * 1024)) as u32);
        append_str(s, " MB");
    } else if bytes >= 1024 {
        append_u32(s, (bytes / 1024) as u32);
        append_str(s, " KB");
    } else {
        append_u32(s, bytes as u32);
        append_str(s, " B");
    }
}

fn format_uptime<const N: usize>(s: &mut Str<N>, ms: u64) {
    s.len = 0;
    let secs  = ms / 1000;
    let mins  = secs / 60;
    let hours = mins / 60;
    let days  = hours / 24;

    if days > 0 {
        append_u32(s, days as u32);
        append_str(s, "d ");
    }
    if hours > 0 || days > 0 {
        append_u32(s, (hours % 24) as u32);
        append_str(s, "h ");
    }
    append_u32(s, (mins % 60) as u32);
    append_str(s, "m ");
    append_u32(s, (secs % 60) as u32);
    append_str(s, "s");
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let win = match gui_create("System Monitor", WIN_W, WIN_H) {
        Some(w) => w,
        None    => exit(1),
    };

    let canvas  = Canvas::new(win);
    let theme       = Theme::DARK;

    let mut mem_bar  = ProgressBar::new(Rect::new(PAD, 80, WIN_W - PAD * 2, BAR_H));
    let mut quit_btn = Button::new(
        Rect::new(WIN_W - PAD - 80, WIN_H - PAD - 26, 80, 26),
        "Quit",
    );

    let _header = Label::new(
        Rect::new(PAD, PAD, WIN_W - PAD * 2, ROW_H),
        "OxideOS System Monitor",
        theme.accent,
    );

    let mut tick: u32 = 0;
    let mut info = get_system_info();

    loop {
        // Refresh system info every 20 frames (~1 s at 50 ms per frame)
        if tick % 20 == 0 {
            info = get_system_info();
        }
        tick = tick.wrapping_add(1);

        // ── Draw background ──────────────────────────────────────────────────
        canvas.clear(theme.bg);

        // Title bar
        canvas.fill_rect(Rect::new(0, 0, WIN_W, 36), theme.panel);
        canvas.fill_rect(Rect::new(0, 36, WIN_W, 1), theme.border);
        canvas.draw_text(PAD, 10, theme.accent, "OxideOS System Monitor");
        canvas.draw_text(WIN_W - PAD - CHAR_W * 7, 10, theme.text_dim, "v0.1.0");

        // ── Memory section ───────────────────────────────────────────────────
        let y0 = 50u32;
        canvas.draw_text(PAD, y0, theme.text, "Memory");

        let used = info.total_memory.saturating_sub(info.free_memory);
        let pct  = if info.total_memory > 0 { (used * 100 / info.total_memory) as u32 } else { 0 };
        mem_bar.set(pct);
        mem_bar.rect.y = y0 + ROW_H;
        mem_bar.draw(&canvas, &theme);

        let mut label_buf: Str<48> = Str::new();
        let mut tmp_buf = [0u8; 24];
        let pct_str = fmt_percent(used as u32, info.total_memory as u32, &mut tmp_buf);

        write_str(&mut label_buf, "Used: ");
        let mut m: Str<16> = Str::new(); format_bytes(&mut m, used);
        append_str(&mut label_buf, m.as_str());
        append_str(&mut label_buf, " / ");
        format_bytes(&mut m, info.total_memory);
        append_str(&mut label_buf, m.as_str());
        append_str(&mut label_buf, "  (");
        append_str(&mut label_buf, pct_str);
        append_str(&mut label_buf, ")");
        canvas.draw_text(PAD, y0 + ROW_H + BAR_H + 4, theme.text_dim, label_buf.as_str());

        // ── Uptime ───────────────────────────────────────────────────────────
        let y1 = y0 + ROW_H + BAR_H + 4 + ROW_H + 12;
        canvas.fill_rect(Rect::new(PAD, y1 - 4, WIN_W - PAD * 2, 1), theme.border);
        canvas.draw_text(PAD, y1 + 4, theme.text_dim, "Uptime");
        let mut up: Str<32> = Str::new();
        format_uptime(&mut up, info.uptime_ms);
        canvas.draw_text(PAD + CHAR_W * 8, y1 + 4, theme.text, up.as_str());

        // ── Process count ────────────────────────────────────────────────────
        let y2 = y1 + ROW_H + 8;
        canvas.draw_text(PAD, y2, theme.text_dim, "Processes");
        let mut procs: Str<8> = Str::new();
        procs.len = 0;
        append_u32(&mut procs, info.process_count);
        canvas.draw_text(PAD + CHAR_W * 10, y2, theme.text, procs.as_str());

        // ── Free memory ──────────────────────────────────────────────────────
        let y3 = y2 + ROW_H;
        canvas.draw_text(PAD, y3, theme.text_dim, "Free mem");
        let mut free: Str<16> = Str::new();
        format_bytes(&mut free, info.free_memory);
        canvas.draw_text(PAD + CHAR_W * 10, y3, theme.text, free.as_str());

        // ── Uptime ms (raw) ──────────────────────────────────────────────────
        let y4 = y3 + ROW_H;
        canvas.draw_text(PAD, y4, theme.text_dim, "Tick (ms)");
        let mut ms_str: Str<16> = Str::new();
        ms_str.len = 0;
        append_u32(&mut ms_str, info.uptime_ms as u32);
        canvas.draw_text(PAD + CHAR_W * 10, y4, theme.text_dim, ms_str.as_str());

        // ── Status bar ───────────────────────────────────────────────────────
        let status_y = WIN_H - 32;
        canvas.fill_rect(Rect::new(0, status_y, WIN_W, 1), theme.border);
        canvas.fill_rect(Rect::new(0, status_y + 1, WIN_W, 31), theme.panel);
        canvas.draw_text(PAD, status_y + 8, theme.text_dim, "Auto-refreshes every second");

        // ── Quit button ──────────────────────────────────────────────────────
        quit_btn.draw(&canvas, &theme);

        canvas.present();

        // ── Handle events ────────────────────────────────────────────────────
        while let Some(ev) = canvas.poll_event() {
            if ev.is_close() { exit(0); }
            if ev.kind == GuiEvent::KEY && ev.data[0] == b'q' { exit(0); }
            if quit_btn.handle_event(&ev) { exit(0); }
        }

        sleep_ms(50);
    }
}
