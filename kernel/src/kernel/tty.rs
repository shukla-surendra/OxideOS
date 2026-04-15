//! TTY subsystem for OxideOS — POSIX termios interface.
//!
//! Provides a global TTY state and handles `ioctl` requests for terminal
//! attribute queries (TCGETS) and updates (TCSETS/TCSETSW/TCSETSF).
//! Also handles TIOCGWINSZ for terminal window size queries.
//!
//! The current implementation supports:
//! - TCGETS/TCSETS: get/set termios (raw vs canonical mode flag tracking)
//! - TIOCGWINSZ: return fixed terminal size (80×24 chars, 720×384 pixels)
//! - TIOCSPGRP/TIOCGPGRP: process-group stubs (return current PID)
//!
//! Actual canonical mode line-buffering is not yet implemented; the stdin
//! ring is still raw. Programs that call TCSETS to set raw mode will find
//! they are already effectively in raw mode.

use crate::kernel::serial::SERIAL_PORT;

// ── ioctl request numbers (matching Linux) ────────────────────────────────

pub const TCGETS:    u64 = 0x5401;
pub const TCSETS:    u64 = 0x5402;
pub const TCSETSW:   u64 = 0x5403;
pub const TCSETSF:   u64 = 0x5404;
pub const TIOCGPGRP: u64 = 0x540F;
pub const TIOCSPGRP: u64 = 0x5410;
pub const TIOCGWINSZ:u64 = 0x5413;
pub const TIOCSWINSZ:u64 = 0x5414;

// ── termios c_lflag bits ─────────────────────────────────────────────────

pub const ISIG:   u32 = 0x0001;   // signal generation (Ctrl+C → SIGINT)
pub const ICANON: u32 = 0x0002;   // canonical mode (line-buffered)
pub const ECHO:   u32 = 0x0008;   // echo input characters
pub const ECHOE:  u32 = 0x0010;   // echo erase as BS-SP-BS
pub const ECHOK:  u32 = 0x0020;   // echo NL after kill char
pub const ECHONL: u32 = 0x0040;   // echo NL even if ECHO is off
pub const NOFLSH: u32 = 0x0080;   // don't flush on signal chars

// ── termios c_iflag bits ─────────────────────────────────────────────────

pub const ICRNL:  u32 = 0x0100;   // map CR to NL
pub const IXON:   u32 = 0x0400;   // XON/XOFF flow control

// ── termios c_oflag bits ─────────────────────────────────────────────────

pub const OPOST:  u32 = 0x0001;   // output processing
pub const ONLCR:  u32 = 0x0004;   // map NL to CR+NL

// ── c_cc indices ─────────────────────────────────────────────────────────

pub const VINTR:  usize = 0;    // Ctrl+C → SIGINT
pub const VQUIT:  usize = 1;    // Ctrl+\ → SIGQUIT
pub const VERASE: usize = 2;    // Backspace / DEL
pub const VKILL:  usize = 3;    // Ctrl+U — kill line
pub const VEOF:   usize = 4;    // Ctrl+D — EOF
pub const VMIN:   usize = 6;    // min bytes for read()
pub const VTIME:  usize = 7;    // read() timeout in 0.1 s units

// ── On-disk / in-memory struct ────────────────────────────────────────────

/// POSIX termios (matching Linux kernel layout for interoperability).
/// Size: 4+4+4+4+1+32 = 49 bytes, padded to 52 with `_pad`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line:  u8,
    pub c_cc: [u8; 32],
    _pad: [u8; 3],
}

/// Window size returned by TIOCGWINSZ.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Winsize {
    pub ws_row:    u16,  // rows in characters
    pub ws_col:    u16,  // columns in characters
    pub ws_xpixel: u16,  // width in pixels
    pub ws_ypixel: u16,  // height in pixels
}

/// Default "cooked" (canonical) termios matching a typical Linux terminal.
fn default_termios() -> Termios {
    let mut t = Termios {
        c_iflag: ICRNL | IXON,
        c_oflag: OPOST | ONLCR,
        c_cflag: 0x00BF,     // CS8 | CREAD | HUPCL
        c_lflag: ISIG | ICANON | ECHO | ECHOE | ECHOK,
        c_line:  0,
        c_cc: [0u8; 32],
        _pad: [0u8; 3],
    };
    t.c_cc[VINTR]  = 3;    // Ctrl+C
    t.c_cc[VQUIT]  = 28;   // Ctrl+\
    t.c_cc[VERASE] = 127;  // DEL
    t.c_cc[VKILL]  = 21;   // Ctrl+U
    t.c_cc[VEOF]   = 4;    // Ctrl+D
    t.c_cc[VMIN]   = 1;
    t.c_cc[VTIME]  = 0;
    t
}

/// Default terminal size — 80×24 text, 720×384 pixels.
const DEFAULT_WINSIZE: Winsize = Winsize {
    ws_row: 24, ws_col: 80, ws_xpixel: 720, ws_ypixel: 384,
};

// ── Global TTY state ──────────────────────────────────────────────────────

struct TtyState {
    termios: Termios,
    winsize: Winsize,
}

impl TtyState {
    fn new() -> Self {
        Self { termios: default_termios(), winsize: DEFAULT_WINSIZE }
    }
}

static mut TTY: TtyState = TtyState {
    termios: Termios {
        c_iflag: ICRNL | IXON,
        c_oflag: OPOST | ONLCR,
        c_cflag: 0x00BF,
        c_lflag: ISIG | ICANON | ECHO | ECHOE | ECHOK,
        c_line:  0,
        c_cc: [0u8; 32],
        _pad: [0u8; 3],
    },
    winsize: DEFAULT_WINSIZE,
};

// ── Public ioctl handler ──────────────────────────────────────────────────

/// Handle an `ioctl(fd, request, arg)` call.
///
/// `fd` 0/1/2 (or any tty-like fd) routes here.
/// Returns 0 on success, negative error on failure.
///
/// # Safety
/// `arg` is a raw user-space pointer; caller must validate it points to a
/// valid `Termios` or `Winsize` buffer in user space.
pub unsafe fn ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    let _ = fd; // for now all fds share one global TTY

    let state = &raw mut TTY;

    match request {
        // TCGETS — copy current termios into user buffer
        TCGETS => {
            if let Err(_) = crate::kernel::syscall_core::validate_user_range(
                arg, core::mem::size_of::<Termios>() as u64,
            ) {
                return -1; // EINVAL
            }
            unsafe {
                core::ptr::write_unaligned(arg as *mut Termios, (*state).termios);
            }
            0
        }

        // TCSETS / TCSETSW / TCSETSF — update termios from user buffer
        TCSETS | TCSETSW | TCSETSF => {
            if let Err(_) = crate::kernel::syscall_core::validate_user_range(
                arg, core::mem::size_of::<Termios>() as u64,
            ) {
                return -1; // EINVAL
            }
            let new_termios = unsafe { core::ptr::read_unaligned(arg as *const Termios) };
            unsafe { (*state).termios = new_termios; }
            0
        }

        // TIOCGWINSZ — copy window size into user buffer
        TIOCGWINSZ => {
            if let Err(_) = crate::kernel::syscall_core::validate_user_range(
                arg, core::mem::size_of::<Winsize>() as u64,
            ) {
                return -1; // EINVAL
            }
            unsafe {
                core::ptr::write_unaligned(arg as *mut Winsize, (*state).winsize);
            }
            0
        }

        // TIOCSWINSZ — update window size from user buffer
        TIOCSWINSZ => {
            if let Err(_) = crate::kernel::syscall_core::validate_user_range(
                arg, core::mem::size_of::<Winsize>() as u64,
            ) {
                return -1; // EINVAL
            }
            let ws = unsafe { core::ptr::read_unaligned(arg as *const Winsize) };
            unsafe { (*state).winsize = ws; }
            0
        }

        // TIOCGPGRP — get foreground process group (return current PID as stub)
        TIOCGPGRP => {
            if let Err(_) = crate::kernel::syscall_core::validate_user_range(arg, 4) {
                return -1;
            }
            let pid = unsafe {
                let sched = &raw const crate::kernel::scheduler::SCHED;
                let idx   = crate::kernel::scheduler::CURRENT_TASK_IDX;
                (*sched).tasks[idx].pid as u32
            };
            unsafe { core::ptr::write_unaligned(arg as *mut u32, pid); }
            0
        }

        // TIOCSPGRP — set foreground process group (stub: accept and ignore)
        TIOCSPGRP => 0,

        _ => {
            // Unknown ioctl — not supported
            -1 // EINVAL
        }
    }
}

/// Returns `true` if the TTY is in canonical (line-buffered) mode.
pub fn is_canonical() -> bool {
    unsafe { TTY.termios.c_lflag & ICANON != 0 }
}

/// Returns `true` if echo is enabled.
pub fn is_echo() -> bool {
    unsafe { TTY.termios.c_lflag & ECHO != 0 }
}
