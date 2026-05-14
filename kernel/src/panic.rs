//! Enhanced Kernel Panic Handler
//!
//! This module handles kernel panics with proper message formatting,
//! detailed error reporting, and a Blue-Screen-of-Death framebuffer display.

use core::panic::PanicInfo;
use core::arch::asm;
use crate::kernel::loggers::LOGGER;
use crate::kernel::serial::SERIAL_PORT;

/// Kernel panic handler - called when the kernel encounters a fatal error
#[panic_handler]
pub fn panic_handler(info: &PanicInfo) -> ! {
    // Immediately disable interrupts to prevent further damage
    unsafe {
        asm!("cli", options(nostack, nomem));
    }

    unsafe {
        // Print panic header
        SERIAL_PORT.write_str("\n");
        SERIAL_PORT.write_str("=====================================\n");
        SERIAL_PORT.write_str("       KERNEL PANIC OCCURRED!       \n");
        SERIAL_PORT.write_str("=====================================\n");

        // Log through both serial and logger if available
        LOGGER.error("KERNEL PANIC - SYSTEM HALTING");

        // Print location information if available
        if let Some(location) = info.location() {
            SERIAL_PORT.write_str("Panic Location:\n");
            SERIAL_PORT.write_str("  File: ");
            SERIAL_PORT.write_str(location.file());
            SERIAL_PORT.write_str("\n  Line: ");
            SERIAL_PORT.write_decimal(location.line());
            SERIAL_PORT.write_str("\n  Column: ");
            SERIAL_PORT.write_decimal(location.column());
            SERIAL_PORT.write_str("\n");
        } else {
            SERIAL_PORT.write_str("Panic Location: Unknown\n");
        }

        // Print panic message using write_fmt
        SERIAL_PORT.write_str("Panic Message: ");
        let message = info.message();
        // Use the write_fmt method you already have in SerialPort
        SERIAL_PORT.write_fmt(format_args!("{}", message));
        SERIAL_PORT.write_str("\n");

        // Additional panic payload information (if any)
        if let Some(payload) = info.payload().downcast_ref::<&str>() {
            SERIAL_PORT.write_str("Payload: ");
            SERIAL_PORT.write_str(payload);
            SERIAL_PORT.write_str("\n");
        }

        // TODO: Add more debugging info
        // - Register dump
        // - Stack trace
        // - Memory state
        // - Recent kernel activity log
        print_register_dump();

        SERIAL_PORT.write_str("\nSystem State:\n");
        SERIAL_PORT.write_str("  Interrupts: DISABLED\n");
        SERIAL_PORT.write_str("  CPU: HALTED\n");
        SERIAL_PORT.write_str("  System: UNRECOVERABLE\n");

        SERIAL_PORT.write_str("\n");
        SERIAL_PORT.write_str("=====================================\n");
        SERIAL_PORT.write_str("System has been halted for safety.\n");
        SERIAL_PORT.write_str("Restart required.\n");
        SERIAL_PORT.write_str("=====================================\n");

        // Final log entry
        LOGGER.error("System halted due to kernel panic - restart required");
    }

    // Draw BSoD on framebuffer (best effort — silently skips if not initialised).
    draw_bsod(info);

    // Halt the CPU indefinitely
    halt_system();
}

/// Print basic CPU register dump for debugging
unsafe fn print_register_dump() {
    SERIAL_PORT.write_str("\nRegister Dump:\n");

    // For x86_64, capture registers in smaller batches to avoid running out of registers
    #[cfg(target_arch = "x86_64")]
    {
        // Batch 1: General purpose registers
        let rax: u64;
        let rbx: u64;
        let rcx: u64;
        let rdx: u64;

        asm!(
            "mov {rax}, rax",
            "mov {rbx}, rbx",
            "mov {rcx}, rcx",
            "mov {rdx}, rdx",
            rax = out(reg) rax,
            rbx = out(reg) rbx,
            rcx = out(reg) rcx,
            rdx = out(reg) rdx,
            options(nostack, nomem)
        );

        SERIAL_PORT.write_str("  RAX: 0x");
        print_hex64(rax);
        SERIAL_PORT.write_str("  RBX: 0x");
        print_hex64(rbx);
        SERIAL_PORT.write_str("\n");

        SERIAL_PORT.write_str("  RCX: 0x");
        print_hex64(rcx);
        SERIAL_PORT.write_str("  RDX: 0x");
        print_hex64(rdx);
        SERIAL_PORT.write_str("\n");

        // Batch 2: Stack and base pointers
        let rsp: u64;
        let rbp: u64;

        asm!(
            "mov {rsp}, rsp",
            "mov {rbp}, rbp",
            rsp = out(reg) rsp,
            rbp = out(reg) rbp,
            options(nostack, nomem)
        );

        SERIAL_PORT.write_str("  RSP: 0x");
        print_hex64(rsp);
        SERIAL_PORT.write_str("  RBP: 0x");
        print_hex64(rbp);
        SERIAL_PORT.write_str("\n");

        // Batch 3: Index registers
        let rsi: u64;
        let rdi: u64;

        asm!(
            "mov {rsi}, rsi",
            "mov {rdi}, rdi",
            rsi = out(reg) rsi,
            rdi = out(reg) rdi,
            options(nostack, nomem)
        );

        SERIAL_PORT.write_str("  RSI: 0x");
        print_hex64(rsi);
        SERIAL_PORT.write_str("  RDI: 0x");
        print_hex64(rdi);
        SERIAL_PORT.write_str("\n");
    }

    #[cfg(target_arch = "x86")]
    {
        // For 32-bit, also use smaller batches
        let eax: u32;
        let ebx: u32;
        let ecx: u32;
        let edx: u32;

        asm!(
            "mov {eax}, eax",
            "mov {ebx}, ebx",
            "mov {ecx}, ecx",
            "mov {edx}, edx",
            eax = out(reg) eax,
            ebx = out(reg) ebx,
            ecx = out(reg) ecx,
            edx = out(reg) edx,
            options(nostack, nomem)
        );

        SERIAL_PORT.write_str("  EAX: 0x");
        SERIAL_PORT.write_hex(eax);
        SERIAL_PORT.write_str("  EBX: 0x");
        SERIAL_PORT.write_hex(ebx);
        SERIAL_PORT.write_str("\n");

        SERIAL_PORT.write_str("  ECX: 0x");
        SERIAL_PORT.write_hex(ecx);
        SERIAL_PORT.write_str("  EDX: 0x");
        SERIAL_PORT.write_hex(edx);
        SERIAL_PORT.write_str("\n");
    }
}

/// Helper to print 64-bit hex values
unsafe fn print_hex64(mut value: u64) {
    if value == 0 {
        SERIAL_PORT.write_str("0000000000000000");
        return;
    }

    let mut digits = [0u8; 16];
    let mut i = 0;

    // Convert to hex, pad to 16 digits
    for _ in 0..16 {
        let digit = (value & 0xF) as u8;
        digits[i] = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + (digit - 10)
        };
        value >>= 4;
        i += 1;
    }

    // Write in reverse order (most significant first)
    for j in (0..16).rev() {
        SERIAL_PORT.write_byte(digits[j]);
    }
}

// ── BSoD framebuffer renderer ──────────────────────────────────────────────────

/// 8×8 bitmap font for printable ASCII (0x20–0x7E), inlined so the BSoD
/// renderer has zero dependency on the GUI font module (which may itself have
/// triggered the panic).  Each entry is one character: 8 bytes, one per row,
/// MSB = leftmost pixel.
#[rustfmt::skip]
const BSOD_FONT: [[u8; 8]; 128] = [
    // 0x00-0x1F control characters (not rendered)
    [0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],
    [0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],
    [0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],
    [0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],[0;8],
    // 0x20 ' '
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
    // 0x21 '!'
    [0x18,0x18,0x18,0x18,0x18,0x00,0x18,0x00],
    // 0x22 '"'
    [0x66,0x66,0x66,0x00,0x00,0x00,0x00,0x00],
    // 0x23 '#'
    [0x6C,0x6C,0xFE,0x6C,0xFE,0x6C,0x6C,0x00],
    // 0x24 '$'
    [0x18,0x7E,0x60,0x7C,0x06,0x7E,0x18,0x00],
    // 0x25 '%'
    [0xC6,0xCC,0x18,0x30,0x60,0xC6,0x86,0x00],
    // 0x26 '&'
    [0x38,0x6C,0x38,0x76,0xDC,0xCC,0x76,0x00],
    // 0x27 '\''
    [0x18,0x18,0x30,0x00,0x00,0x00,0x00,0x00],
    // 0x28 '('
    [0x0C,0x18,0x30,0x30,0x30,0x18,0x0C,0x00],
    // 0x29 ')'
    [0x30,0x18,0x0C,0x0C,0x0C,0x18,0x30,0x00],
    // 0x2A '*'
    [0x00,0x66,0x3C,0xFF,0x3C,0x66,0x00,0x00],
    // 0x2B '+'
    [0x00,0x18,0x18,0x7E,0x18,0x18,0x00,0x00],
    // 0x2C ','
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x30],
    // 0x2D '-'
    [0x00,0x00,0x00,0x7E,0x00,0x00,0x00,0x00],
    // 0x2E '.'
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00],
    // 0x2F '/'
    [0x06,0x0C,0x18,0x30,0x60,0xC0,0x80,0x00],
    // 0x30-0x39 '0'-'9'
    [0x7C,0xC6,0xCE,0xDE,0xF6,0xE6,0x7C,0x00],
    [0x18,0x38,0x18,0x18,0x18,0x18,0x7E,0x00],
    [0x7C,0xC6,0x06,0x1C,0x30,0x60,0xFE,0x00],
    [0x7C,0xC6,0x06,0x3C,0x06,0xC6,0x7C,0x00],
    [0x1C,0x3C,0x6C,0xCC,0xFE,0x0C,0x1E,0x00],
    [0xFE,0xC0,0xC0,0xFC,0x06,0xC6,0x7C,0x00],
    [0x38,0x60,0xC0,0xFC,0xC6,0xC6,0x7C,0x00],
    [0xFE,0xC6,0x0C,0x18,0x30,0x30,0x30,0x00],
    [0x7C,0xC6,0xC6,0x7C,0xC6,0xC6,0x7C,0x00],
    [0x7C,0xC6,0xC6,0x7E,0x06,0x0C,0x78,0x00],
    // 0x3A ':'
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x00],
    // 0x3B ';'
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x30],
    // 0x3C '<'
    [0x0E,0x18,0x30,0x60,0x30,0x18,0x0E,0x00],
    // 0x3D '='
    [0x00,0x00,0x7E,0x00,0x7E,0x00,0x00,0x00],
    // 0x3E '>'
    [0x70,0x18,0x0C,0x06,0x0C,0x18,0x70,0x00],
    // 0x3F '?'
    [0x7C,0xC6,0x0C,0x18,0x18,0x00,0x18,0x00],
    // 0x40 '@'
    [0x7C,0xC6,0xDE,0xDE,0xDE,0xC0,0x78,0x00],
    // 0x41-0x5A 'A'-'Z'
    [0x38,0x6C,0xC6,0xFE,0xC6,0xC6,0xC6,0x00],
    [0xFC,0x66,0x66,0x7C,0x66,0x66,0xFC,0x00],
    [0x3C,0x66,0xC0,0xC0,0xC0,0x66,0x3C,0x00],
    [0xF8,0x6C,0x66,0x66,0x66,0x6C,0xF8,0x00],
    [0xFE,0x62,0x68,0x78,0x68,0x62,0xFE,0x00],
    [0xFE,0x62,0x68,0x78,0x68,0x60,0xF0,0x00],
    [0x3C,0x66,0xC0,0xC0,0xCE,0x66,0x3E,0x00],
    [0xC6,0xC6,0xC6,0xFE,0xC6,0xC6,0xC6,0x00],
    [0x3C,0x18,0x18,0x18,0x18,0x18,0x3C,0x00],
    [0x1E,0x0C,0x0C,0x0C,0xCC,0xCC,0x78,0x00],
    [0xE6,0x66,0x6C,0x78,0x6C,0x66,0xE6,0x00],
    [0xF0,0x60,0x60,0x60,0x62,0x66,0xFE,0x00],
    [0xC6,0xEE,0xFE,0xFE,0xD6,0xC6,0xC6,0x00],
    [0xC6,0xE6,0xF6,0xDE,0xCE,0xC6,0xC6,0x00],
    [0x7C,0xC6,0xC6,0xC6,0xC6,0xC6,0x7C,0x00],
    [0xFC,0x66,0x66,0x7C,0x60,0x60,0xF0,0x00],
    [0x7C,0xC6,0xC6,0xC6,0xD6,0xDE,0x7C,0x0C],
    [0xFC,0x66,0x66,0x7C,0x6C,0x66,0xE6,0x00],
    [0x7C,0xC6,0x60,0x38,0x0C,0xC6,0x7C,0x00],
    [0x7E,0x7E,0x5A,0x18,0x18,0x18,0x3C,0x00],
    [0xC6,0xC6,0xC6,0xC6,0xC6,0xC6,0x7C,0x00],
    [0xC6,0xC6,0xC6,0xC6,0xC6,0x6C,0x38,0x00],
    [0xC6,0xC6,0xC6,0xD6,0xFE,0xEE,0xC6,0x00],
    [0xC6,0xC6,0x6C,0x38,0x6C,0xC6,0xC6,0x00],
    [0x66,0x66,0x66,0x3C,0x18,0x18,0x3C,0x00],
    [0xFE,0xC6,0x8C,0x18,0x32,0x66,0xFE,0x00],
    // 0x5B '['
    [0x3C,0x30,0x30,0x30,0x30,0x30,0x3C,0x00],
    // 0x5C '\'
    [0xC0,0x60,0x30,0x18,0x0C,0x06,0x02,0x00],
    // 0x5D ']'
    [0x3C,0x0C,0x0C,0x0C,0x0C,0x0C,0x3C,0x00],
    // 0x5E '^'
    [0x10,0x38,0x6C,0xC6,0x00,0x00,0x00,0x00],
    // 0x5F '_'
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xFF],
    // 0x60 '`'
    [0x30,0x18,0x0C,0x00,0x00,0x00,0x00,0x00],
    // 0x61-0x7A 'a'-'z'
    [0x00,0x00,0x78,0x0C,0x7C,0xCC,0x76,0x00],
    [0xE0,0x60,0x60,0x7C,0x66,0x66,0xDC,0x00],
    [0x00,0x00,0x78,0xCC,0xC0,0xCC,0x78,0x00],
    [0x1C,0x0C,0x0C,0x7C,0xCC,0xCC,0x76,0x00],
    [0x00,0x00,0x78,0xCC,0xFC,0xC0,0x78,0x00],
    [0x38,0x6C,0x60,0xF0,0x60,0x60,0xF0,0x00],
    [0x00,0x00,0x76,0xCC,0xCC,0x7C,0x0C,0xF8],
    [0xE0,0x60,0x6C,0x76,0x66,0x66,0xE6,0x00],
    [0x18,0x00,0x38,0x18,0x18,0x18,0x3C,0x00],
    [0x06,0x00,0x0E,0x06,0x06,0x66,0x66,0x3C],
    [0xE0,0x60,0x66,0x6C,0x78,0x6C,0xE6,0x00],
    [0x38,0x18,0x18,0x18,0x18,0x18,0x3C,0x00],
    [0x00,0x00,0xCC,0xFE,0xFE,0xD6,0xC6,0x00],
    [0x00,0x00,0xDC,0x66,0x66,0x66,0x66,0x00],
    [0x00,0x00,0x78,0xCC,0xCC,0xCC,0x78,0x00],
    [0x00,0x00,0xDC,0x66,0x66,0x7C,0x60,0xF0],
    [0x00,0x00,0x76,0xCC,0xCC,0x7C,0x0C,0x1E],
    [0x00,0x00,0xDC,0x76,0x66,0x60,0xF0,0x00],
    [0x00,0x00,0x7C,0xC0,0x78,0x0C,0xF8,0x00],
    [0x10,0x30,0x7C,0x30,0x30,0x34,0x18,0x00],
    [0x00,0x00,0xCC,0xCC,0xCC,0xCC,0x76,0x00],
    [0x00,0x00,0xCC,0xCC,0xCC,0x78,0x30,0x00],
    [0x00,0x00,0xC6,0xD6,0xFE,0xFE,0x6C,0x00],
    [0x00,0x00,0xC6,0x6C,0x38,0x6C,0xC6,0x00],
    [0x00,0x00,0xCC,0xCC,0xCC,0x7C,0x0C,0xF8],
    [0x00,0x00,0xFC,0x98,0x30,0x64,0xFC,0x00],
    // 0x7B '{'
    [0x1C,0x30,0x30,0xE0,0x30,0x30,0x1C,0x00],
    // 0x7C '|'
    [0x18,0x18,0x18,0x00,0x18,0x18,0x18,0x00],
    // 0x7D '}'
    [0xE0,0x30,0x30,0x1C,0x30,0x30,0xE0,0x00],
    // 0x7E '~'
    [0x76,0xDC,0x00,0x00,0x00,0x00,0x00,0x00],
    // 0x7F DEL
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00],
];

fn bsod_glyph(ch: u8) -> [u8; 8] {
    let idx = ch as usize;
    if idx < 128 { BSOD_FONT[idx] } else { [0xFF,0x81,0x81,0x81,0x81,0x81,0x81,0xFF] }
}

/// Draw a single character directly to the framebuffer (no back-buffer).
unsafe fn bsod_putchar(fb: *mut u32, pitch_px: usize, x: usize, y: usize, ch: u8, fg: u32, bg: u32) {
    let glyph = bsod_glyph(ch);
    for row in 0..8usize {
        let bits = glyph[row];
        for col in 0..8usize {
            let color = if (bits >> (7 - col)) & 1 != 0 { fg } else { bg };
            unsafe { fb.add((y + row) * pitch_px + x + col).write_volatile(color); }
        }
    }
}

/// Draw a string at (x, y) in 8×8 font directly to the framebuffer.
unsafe fn bsod_puts(fb: *mut u32, pitch_px: usize, x: &mut usize, y: usize, s: &str, fg: u32, bg: u32) {
    for &b in s.as_bytes() {
        if b == b'\n' { break; }
        unsafe { bsod_putchar(fb, pitch_px, *x, y, b, fg, bg); }
        *x += 8;
    }
}

/// Write a decimal number as ASCII.
unsafe fn bsod_putu64(fb: *mut u32, pitch_px: usize, x: &mut usize, y: usize, mut v: u64, fg: u32, bg: u32) {
    if v == 0 {
        unsafe { bsod_putchar(fb, pitch_px, *x, y, b'0', fg, bg); }
        *x += 8;
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 20usize;
    while v > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    for &d in &buf[i..] {
        unsafe { bsod_putchar(fb, pitch_px, *x, y, d, fg, bg); }
        *x += 8;
    }
}

/// Write a 64-bit hex number (16 digits, no prefix).
unsafe fn bsod_hex64(fb: *mut u32, pitch_px: usize, x: &mut usize, y: usize, v: u64, fg: u32, bg: u32) {
    let hex = b"0123456789ABCDEF";
    for shift in (0..16).rev() {
        let nibble = ((v >> (shift * 4)) & 0xF) as usize;
        unsafe { bsod_putchar(fb, pitch_px, *x, y, hex[nibble], fg, bg); }
        *x += 8;
    }
}

/// Fill a rectangle directly on the framebuffer.
unsafe fn bsod_fill(fb: *mut u32, pitch_px: usize, x: usize, y: usize, w: usize, h: usize, color: u32) {
    for row in y..y+h {
        for col in x..x+w {
            unsafe { fb.add(row * pitch_px + col).write_volatile(color); }
        }
    }
}

/// Render a full Blue-Screen-of-Death on the framebuffer.
fn draw_bsod(info: &PanicInfo) {
    let pfb = unsafe { crate::gui::graphics::PANIC_FB };
    let fb_info = match pfb { Some(f) => f, None => return };

    let fb       = fb_info.addr;
    let w        = fb_info.width  as usize;
    let h        = fb_info.height as usize;
    let pitch_px = (fb_info.pitch / 4) as usize;

    // Palette
    const BG:     u32 = 0xFF0000AA; // classic blue
    const FG:     u32 = 0xFFFFFFFF;
    const TITLE:  u32 = 0xFFAAAAAA;
    const ACCENT: u32 = 0xFF00AAAA;

    // Fill background.
    unsafe { bsod_fill(fb, pitch_px, 0, 0, w, h, BG); }

    let margin = 40usize;
    let mut cy = margin;

    // Sad face.
    let msg0 = ":( OxideOS";
    let mut cx = margin;
    unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, msg0, ACCENT, BG); }
    cy += 20;

    // Separator line.
    unsafe { bsod_fill(fb, pitch_px, margin, cy, w - margin * 2, 2, ACCENT); }
    cy += 12;

    // Panic message.
    let pmsg = "KERNEL PANIC";
    cx = margin;
    unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, pmsg, FG, BG); }
    cy += 20;

    // Location: file:line.
    if let Some(loc) = info.location() {
        cx = margin;
        unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, "at ", TITLE, BG); }
        unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, loc.file(), FG, BG); }
        unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, ":", TITLE, BG); }
        unsafe { bsod_putu64(fb, pitch_px, &mut cx, cy, loc.line() as u64, FG, BG); }
        cy += 16;
    }

    cy += 8;

    // Register dump.
    #[cfg(target_arch = "x86_64")]
    {
        let (rax, rbx, rcx, rdx, rsi, rdi, rsp, rbp): (u64,u64,u64,u64,u64,u64,u64,u64);
        unsafe {
            core::arch::asm!(
                "mov {rax}, rax",
                "mov {rbx}, rbx",
                "mov {rcx}, rcx",
                "mov {rdx}, rdx",
                rax = out(reg) rax,
                rbx = out(reg) rbx,
                rcx = out(reg) rcx,
                rdx = out(reg) rdx,
                options(nostack, nomem)
            );
            core::arch::asm!(
                "mov {rsi}, rsi",
                "mov {rdi}, rdi",
                "mov {rsp}, rsp",
                "mov {rbp}, rbp",
                rsi = out(reg) rsi,
                rdi = out(reg) rdi,
                rsp = out(reg) rsp,
                rbp = out(reg) rbp,
                options(nostack, nomem)
            );
        }

        let regs: [(&str, u64); 8] = [
            ("RAX", rax), ("RBX", rbx), ("RCX", rcx), ("RDX", rdx),
            ("RSI", rsi), ("RDI", rdi), ("RSP", rsp), ("RBP", rbp),
        ];

        for (i, (name, val)) in regs.iter().enumerate() {
            cx = margin + (i % 2) * 300;
            if i % 2 == 0 && i > 0 { cy += 14; }
            unsafe {
                bsod_puts(fb, pitch_px, &mut cx, cy, name, TITLE, BG);
                bsod_puts(fb, pitch_px, &mut cx, cy, ": 0x", TITLE, BG);
                bsod_hex64(fb, pitch_px, &mut cx, cy, *val, FG, BG);
            }
        }
        cy += 20;
    }

    cy += 8;
    cx = margin;
    unsafe { bsod_puts(fb, pitch_px, &mut cx, cy, "System halted. Please restart.", ACCENT, BG); }
}

/// Halt the system safely
fn halt_system() -> ! {
    unsafe {
        loop {
            asm!("hlt", options(nostack, nomem));
        }
    }
}

/// Enhanced panic function with custom message (for internal kernel use)
pub fn kernel_panic(subsystem: &str, reason: &str) -> ! {
    unsafe {
        SERIAL_PORT.write_str("KERNEL PANIC in ");
        SERIAL_PORT.write_str(subsystem);
        SERIAL_PORT.write_str(": ");
        SERIAL_PORT.write_str(reason);
        SERIAL_PORT.write_str("\n");
    }

    panic!("Kernel subsystem failure: {}: {}", subsystem, reason);
}

/// Panic with formatted message (using write_fmt capability)
pub fn kernel_panic_fmt(subsystem: &str, args: core::fmt::Arguments) -> ! {
    unsafe {
        SERIAL_PORT.write_str("KERNEL PANIC in ");
        SERIAL_PORT.write_str(subsystem);
        SERIAL_PORT.write_str(": ");
        SERIAL_PORT.write_fmt(args);
        SERIAL_PORT.write_str("\n");
    }

    panic!("Kernel subsystem failure in {}", subsystem);
}

/// Enhanced assert macro for kernel debugging
#[macro_export]
macro_rules! kernel_assert {
    ($condition:expr) => {
        if !($condition) {
            $crate::panic::kernel_panic("assertion", stringify!($condition));
        }
    };
    ($condition:expr, $message:expr) => {
        if !($condition) {
            $crate::panic::kernel_panic("assertion", $message);
        }
    };
    ($condition:expr, $($args:tt)*) => {
        if !($condition) {
            $crate::panic::kernel_panic_fmt("assertion", format_args!($($args)*));
        }
    };
}

/// Convenience macro for formatted kernel panics
#[macro_export]
macro_rules! kernel_panic {
    ($subsystem:expr, $($args:tt)*) => {
        $crate::panic::kernel_panic_fmt($subsystem, format_args!($($args)*))
    };
}