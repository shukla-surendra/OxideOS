//! System-info panel — draws the "System Info" window content.
//!
//! The panel shows OS version, clock, uptime, memory bar, disk status,
//! task count, NIC info, IP address, and the "Test Internet Connection" button.
//! It is drawn from the main GUI loop every time `needs_redraw` is true.

use gui::{fonts, graphics::Graphics, window_manager::WindowManager};
use crate::kernel::{ata, net, scheduler, timer, rtc};
use crate::net_probe::{NetProbe, NetProbePhase, ProbeFailReason};

/// Draw all content inside the System Info window.
///
/// `cy` is incremented row-by-row so adding a new field only requires
/// inserting one block without touching any y-offsets below it.
pub unsafe fn draw_sysinfo_panel(
    graphics: &Graphics,
    wm: &WindowManager,
    window_id: usize,
    net_probe: &NetProbe,
) {
    if !wm.is_window_visible(window_id) { return; }
    let Some(win) = wm.get_window(window_id) else { return; };

    let cx    = win.x + 12;
    let mut cy = win.y + 42; // 34px title bar + 8px gap
    let row   = 20u64;
    let bar_w = win.width.saturating_sub(24);

    // ── OS name ───────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, crate::version::DISPLAY_NAME_VER, 0xFF7FC8FF);
    cy += row - 2;
    fonts::draw_string(graphics, cx, cy, crate::version::DISPLAY_ARCH_LINE, 0xFF4A6080);
    cy += row + 2;
    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840); cy += 8;

    // ── Real-time clock ───────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "TIME", 0xFF007ACC);
    let mut tbuf = [0u8; 11];
    rtc::format_time_ampm(&mut tbuf);
    if let Ok(ts) = core::str::from_utf8(&tbuf) {
        fonts::draw_string(graphics, cx + 54, cy, ts, 0xFFE0F0FF);
    }
    cy += row;

    // ── Uptime ────────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "UPTIME", 0xFF007ACC);
    let ticks = unsafe { timer::get_ticks() };
    let mut ubuf = [0u8; 8];
    {
        let total = ticks / 100;
        let h = (total / 3600) % 100; let m = (total / 60) % 60; let s = total % 60;
        ubuf[0] = b'0'+(h/10) as u8; ubuf[1] = b'0'+(h%10) as u8; ubuf[2] = b':';
        ubuf[3] = b'0'+(m/10) as u8; ubuf[4] = b'0'+(m%10) as u8; ubuf[5] = b':';
        ubuf[6] = b'0'+(s/10) as u8; ubuf[7] = b'0'+(s%10) as u8;
    }
    fonts::draw_string(graphics, cx + 72, cy, core::str::from_utf8(&ubuf).unwrap_or(""), 0xFF8090A8);
    cy += row;

    // ── Memory bar ────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "MEMORY", 0xFF007ACC);
    fonts::draw_string(graphics, cx + 72, cy, "128 MB total", 0xFF4A6080);
    cy += row - 4;
    graphics.draw_progress_bar(cx, cy, bar_w, 12, 30, 0xFF0D1B2A, 0xFF007ACC, 0xFF1A4060);
    fonts::draw_string(graphics, cx + bar_w + 2, cy - 1, "30%", 0xFF4A6080);
    cy += 18;
    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840); cy += 8;

    // ── Disk ──────────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "DISK", 0xFF007ACC);
    let (disk_str, disk_col) = if ata::is_present() {
        ("ATA detected", 0xFF40C040u32)
    } else {
        ("No disk", 0xFF806040u32)
    };
    fonts::draw_string(graphics, cx + 54, cy, disk_str, disk_col);
    cy += row;

    // ── Tasks ─────────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "TASKS", 0xFF007ACC);
    let n = scheduler::task_count();
    let (task_str, task_col): (&str, u32) = match n {
        0 => ("idle",         0xFF4A6080),
        1..=7 => ("running",  0xFF40C040),
        _ => ("many running", 0xFF40C040),
    };
    fonts::draw_string(graphics, cx + 63, cy, task_str, task_col);
    cy += row;

    // ── Network ───────────────────────────────────────────────────────────────
    graphics.fill_rect(cx, cy, bar_w, 1, 0xFF1E2840); cy += 8;

    fonts::draw_string(graphics, cx, cy, "NETWORK", 0xFF007ACC);
    let net_up = net::is_present();
    let (dot_col, net_label, net_col) = if net_up {
        (0xFF30C040u32, net::nic_name(), 0xFF40D050u32)
    } else {
        (0xFF803030u32, "No NIC ", 0xFF805050u32)
    };
    graphics.fill_rounded_rect(cx + 80, cy + 3, 8, 8, 2, dot_col);
    fonts::draw_string(graphics, cx + 94, cy, net_label, net_col);
    cy += row;

    // ── IP row ────────────────────────────────────────────────────────────────
    fonts::draw_string(graphics, cx, cy, "IP", 0xFF007ACC);
    if net_up {
        let ip = net::get_ip();
        let mut ip_buf = [0u8; 20]; let mut pos = 0usize;
        for (i, &octet) in ip.iter().enumerate() {
            if i > 0 { ip_buf[pos] = b'.'; pos += 1; }
            let mut n = octet as u32;
            if n >= 100 { ip_buf[pos] = b'0'+(n/100) as u8; pos += 1; n %= 100; }
            if n >= 10  { ip_buf[pos] = b'0'+(n/10)  as u8; pos += 1; n %= 10; }
            ip_buf[pos] = b'0'+n as u8; pos += 1;
        }
        if let Ok(s) = core::str::from_utf8(&ip_buf[..pos]) {
            fonts::draw_string(graphics, cx + 27, cy, s, 0xFFB0C8E8);
        }
    } else {
        fonts::draw_string(graphics, cx + 27, cy, "—", 0xFF3A4050);
    }
    cy += row;

    // ── Test button ───────────────────────────────────────────────────────────
    // cy == win.y + 260 here — must match NetProbe::is_button_hit
    let btn_h = 22u64;
    let (bt, bb, bd, btxt) = if net_up {
        (0xFF0D5FA0u32, 0xFF072C50u32, 0xFF00AAFFu32, 0xFFE8F4FFu32)
    } else {
        (0xFF1C2030u32, 0xFF111520u32, 0xFF2A3044u32, 0xFF404060u32)
    };
    graphics.fill_rounded_rect(cx, cy, bar_w, btn_h, 4, bt);
    graphics.fill_rect_gradient_v(cx + 1, cy + 1, bar_w - 2, btn_h - 2, bt, bb);
    graphics.draw_rounded_rect(cx, cy, bar_w, btn_h, 4, bd, 1);
    let label_px = 24u64 * 9; // "Test Internet Connection" is 24 chars × 9 px
    fonts::draw_string(graphics, cx + bar_w.saturating_sub(label_px) / 2, cy + 7,
                       "Test Internet Connection", btxt);
    cy += btn_h + 8;

    // ── Probe status line ─────────────────────────────────────────────────────
    match net_probe.phase {
        NetProbePhase::Idle => {
            fonts::draw_string(graphics, cx, cy, "Status: idle", 0xFF3A4860);
        }
        NetProbePhase::Connecting { start_tick, .. } => {
            let cur = unsafe { timer::get_ticks() };
            let anim = match ((cur.wrapping_sub(start_tick)) / 20) % 4 {
                0 => "Connecting.   ", 1 => "Connecting..  ",
                2 => "Connecting... ", _ => "Connecting....",
            };
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFFC8A020);
            fonts::draw_string(graphics, cx + 14, cy, anim, 0xFFC8A020);
        }
        NetProbePhase::Connected { ms } => {
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFF30C040);
            fonts::draw_string(graphics, cx + 14, cy, "Connected!", 0xFF40D050);
            let mut mbuf = [0u8; 8]; let mlen = fmt_decimal(ms, &mut mbuf);
            mbuf[mlen] = b'm'; mbuf[mlen + 1] = b's';
            if let Ok(s) = core::str::from_utf8(&mbuf[..mlen + 2]) {
                fonts::draw_string(graphics, cx + 86, cy, s, 0xFF60B070);
            }
        }
        NetProbePhase::Failed { reason } => {
            graphics.fill_rounded_rect(cx, cy + 3, 8, 8, 2, 0xFFC03030);
            let msg = match reason {
                ProbeFailReason::NoSocket  => "No socket (NIC/stack error)",
                ProbeFailReason::NoConnect => "Connect rejected by stack",
                ProbeFailReason::Timeout   => "Timeout — no reply from host",
            };
            fonts::draw_string(graphics, cx + 14, cy, msg, 0xFFD04040);
        }
    }
}

/// Format `n` as ASCII decimal into `buf`.  Returns the number of bytes written.
pub fn fmt_decimal(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 { if !buf.is_empty() { buf[0] = b'0'; } return 1; }
    let mut tmp = n; let mut len = 0;
    while tmp > 0 { len += 1; tmp /= 10; }
    let len = len.min(buf.len());
    for i in (0..len).rev() { buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    len
}
