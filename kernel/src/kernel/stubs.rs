//! aarch64 stand-ins for subsystems the ARM port hasn't reached yet.
//!
//! Each inner module mirrors the public API its x86 counterpart exposes to
//! the GUI layer, so the desktop compiles and runs on aarch64.  As real
//! drivers land (GIC, virtio-blk, virtio-net, EL0 processes), these shrink
//! and disappear — keyboard/mouse input is already real (virtio-input via
//! the shared drivers/keyboard decoder).  Everything here is safe to call:
//! storage and spawn operations report failure.

#![cfg(target_arch = "aarch64")]

// ── keyboard: real driver — drivers/keyboard decode pipeline fed by
// arch/aarch64/virtio_input.rs (no stub needed any more) ─────────────────────

// ── scheduler (x86: proc/scheduler) ──────────────────────────────────────────
pub mod scheduler {
    pub const MAX_TASKS: usize = 8;

    #[derive(Clone, Copy, PartialEq)]
    pub enum TaskState {
        Empty,
        Ready,
        Running,
        Sleeping(u64),
        Waiting(u8),
        WaitingForMsg(u32, u64),
        Dead(i64),
    }

    #[derive(Clone, Copy)]
    pub struct TaskInfo {
        pub pid: u8,
        pub name: [u8; 16],
        pub name_len: usize,
        pub state: TaskState,
    }

    pub fn task_infos() -> [TaskInfo; MAX_TASKS] {
        [TaskInfo { pid: 0, name: [0u8; 16], name_len: 0, state: TaskState::Empty }; MAX_TASKS]
    }

    pub unsafe fn spawn(_code: &[u8], _name: &str) -> Result<u8, &'static str> {
        Err("processes not ported to aarch64 yet")
    }

    pub unsafe fn kill(_pid: u8) -> bool { false }

    pub unsafe fn tick() -> Option<(u8, i64)> { None }

    pub fn task_count() -> usize { 0 }

    pub fn output_drain_task(_idx: usize, _f: impl FnMut(&str)) {}
}

// ── programs (x86: proc/programs — embedded userspace binaries) ──────────────
pub mod programs {
    /// Only the built-in notepad works without a process loader.
    pub const NAMES: &[&str] = &["notepad"];

    pub fn find(_name: &str) -> Option<&'static [u8]> { None }
}

// ── net (x86: drivers/net) ───────────────────────────────────────────────────
pub mod net {
    pub unsafe fn poll() {}
    pub fn is_present() -> bool { false }
    pub fn nic_name() -> &'static str { "none" }
    pub fn get_ip() -> [u8; 4] { [0, 0, 0, 0] }

    pub mod socket {
        pub const SOCK_STREAM: u32 = 1;
        pub const AF_INET: u32 = 2;

        pub unsafe fn sys_socket(_domain: u32, _sock_type: u32, _proto: u32) -> i64 { -1 }
        pub unsafe fn sys_connect(_sfd: i64, _addr_ptr: *const u8, _addr_len: usize) -> i64 { -1 }
        pub unsafe fn tcp_is_connected(_sfd: i64) -> bool { false }
        pub unsafe fn sys_close_socket(_sfd: i64) -> i64 { 0 }
    }
}

// ── stdin (x86: ipc/stdin) ───────────────────────────────────────────────────
pub mod stdin {
    /// No producer/consumer yet — user processes arrive with the EL0 port step.
    pub fn push(_ch: u8) {}
    pub fn pop() -> Option<u8> { None }
}

// ── ata + disk_store + diskfs: REAL storage stack over virtio-blk ────────────
// The virtio-blk driver exposes the exact drivers/ata.rs API, so the portable
// x86 storage modules compile here unchanged (`super::ata` / flat-path uses
// resolve to the re-exports below, same #[path] trick as ramfs).
pub use crate::kernel::arch::aarch64::virtio_blk as ata;

#[path = "drivers/disk_store.rs"]
pub mod disk_store;

#[path = "fs/diskfs.rs"]
pub mod diskfs;

#[path = "fs/mbr.rs"]
pub mod mbr;

// ── ipc/proc shims the real ramfs reaches for ────────────────────────────────
pub mod pipe {
    pub unsafe fn addref(_fd: i32) {}
    pub unsafe fn write(_fd: i32, _data: &[u8]) -> i64 { -1 }
    pub unsafe fn read(_fd: i32, _buf: &mut [u8]) -> i64 { -1 }
    pub unsafe fn close(_fd: i32) -> i64 { 0 }
}

pub mod ext2 {
    pub unsafe fn read_fd(_fd: i32, _buf: &mut [u8]) -> i64 { -1 }
    pub unsafe fn write_fd(_fd: i32, _buf: &[u8]) -> i64 { -1 }
    pub unsafe fn close(_fd: i32) -> i64 { 0 }
}

pub mod user_mode {
    /// Console bytes from (future) user processes — nowhere to go yet.
    pub fn output_write(_bytes: &[u8]) {}
}

// ── fat: REAL FAT16 driver (x86: fs/fat) over virtio-blk ─────────────────────
#[path = "fs/fat.rs"]
pub mod fat;

// ── fs (the real in-RAM filesystem IS portable — reuse it directly) ──────────
// Declared at top level: #[path] here resolves relative to src/kernel/, and
// ramfs's `use super::…` then lands on the constants below.
#[path = "fs/ramfs.rs"]
pub mod ramfs_impl;

// Open-flag + errno constants (mirror fs/mod.rs — ramfs imports them via super).
pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR:   u32 = 2;
pub const O_CREAT:  u32 = 0x40;
pub const O_TRUNC:  u32 = 0x200;
pub const O_APPEND: u32 = 0x400;

pub const ENOENT:  i64 = -2;
pub const EEXIST:  i64 = -17;
pub const EISDIR:  i64 = -21;
pub const ENOTDIR: i64 = -20;
pub const EBADF:   i64 = -9;
pub const EINVAL:  i64 = -22;
pub const ENOSPC:  i64 = -28;
pub const EMFILE:  i64 = -24;
pub const EACCES:  i64 = -13;
pub const ENOTEMPTY: i64 = -39;
pub const EFBIG:    i64 = -27;

pub mod fs {
    pub use super::{
        EACCES, EBADF, EEXIST, EFBIG, EINVAL, EISDIR, EMFILE, ENOENT, ENOSPC,
        ENOTDIR, ENOTEMPTY, O_APPEND, O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY,
    };

    pub use super::ramfs_impl as ramfs;
    pub use ramfs::RAMFS;
}

// ── syscall (x86: sys/syscall — terminal's `pid` and `sysinfo` commands) ─────
pub mod syscall {
    pub enum Syscall {
        GetPid = 39,
    }

    pub struct SyscallResult {
        pub value: i64,
        pub error: bool,
    }

    pub struct SystemInfo {
        pub total_memory: u64,
        pub free_memory: u64,
        pub uptime_ms: u64,
        pub process_count: u64,
    }

    pub unsafe fn handle_syscall(_num: u64, _a1: u64, _a2: u64, _a3: u64, _a4: u64, _a5: u64) -> SyscallResult {
        SyscallResult { value: 0, error: false }
    }

    pub fn snapshot_system_info() -> SystemInfo {
        let ticks = unsafe { crate::kernel::timer::get_ticks() };
        SystemInfo {
            total_memory: 128 * 1024 * 1024,
            free_memory: 64 * 1024 * 1024,
            uptime_ms: ticks * 10,
            process_count: 1,
        }
    }
}

// ── interrupts (x86: arch/x86_64/interrupts — mouse plumbing only) ───────────
pub mod interrupts {
    use crate::gui::mouse::{MouseCursor, PS2Mouse};

    pub static mut MOUSE_CONTROLLER: Option<PS2Mouse> = None;
    pub static mut MOUSE_CURSOR: Option<MouseCursor> = None;
    pub static mut SCREEN_DIMENSIONS: (u64, u64) = (0, 0);

    /// Place the cursor at screen centre; virtio-input moves it from there.
    /// (`PS2Mouse` here is only the arch-neutral button/packet state.)
    pub unsafe fn init_mouse_system(screen_width: u64, screen_height: u64) {
        unsafe {
            SCREEN_DIMENSIONS = (screen_width, screen_height);
            let mut cursor = MouseCursor::new();
            cursor.update(0, 0, screen_width, screen_height);
            *core::ptr::addr_of_mut!(MOUSE_CURSOR) = Some(cursor);
            *core::ptr::addr_of_mut!(MOUSE_CONTROLLER) = Some(PS2Mouse::new());
        }
    }

    /// Drain pending virtio-input events (mouse *and* keyboard — one queue
    /// drain services every device).  Returns true if anything arrived.
    pub unsafe fn poll_mouse_data() -> bool {
        unsafe { crate::kernel::arch::aarch64::virtio_input::poll() }
    }
}

// ── compositor + gui_proc (x86: kernel/gui — need IPC + processes) ───────────
pub mod compositor {
    use crate::gui::graphics::Graphics;

    pub unsafe fn init(
        _graphics: &Graphics,
        _content_x: u64, _content_y: u64,
        _content_w: u64, _content_h: u64,
        _bg_color: u32,
    ) {}
    pub unsafe fn update_geometry(_x: u64, _y: u64, _w: u64, _h: u64) {}
    pub unsafe fn disable() {}
    pub unsafe fn process_messages() -> bool { false }
}

pub mod gui_proc {
    use crate::gui::graphics::Graphics;
    use crate::gui::window_manager::WindowManager;

    pub unsafe fn init(_wm: *mut WindowManager, _gfx: *const Graphics) {}
    pub unsafe fn pop_pending_key() -> Option<u8> { None }
    pub fn is_proc_window(_window_id: u32) -> bool { false }
    pub fn pid_by_window(_window_id: u32) -> Option<u32> { None }
    pub unsafe fn push_key_event(_window_id: u32, _ch: u8) {}
    pub unsafe fn push_mouse_move(_window_id: u32, _rel_x: u16, _rel_y: u16) {}
    pub unsafe fn push_mouse_btn(_window_id: u32, _rel_x: u16, _rel_y: u16, _button: u8, _pressed: bool) {}
    pub unsafe fn push_focus_event(_window_id: u32, _gained: bool) {}
    pub unsafe fn on_window_closed(_window_id: u32) {}
    pub unsafe fn on_process_exit(_pid: u32) {}
    pub unsafe fn has_active_windows() -> bool { false }
    pub unsafe fn take_present_flag() -> bool { false }
    pub unsafe fn composite_window(_graphics: &Graphics, _window_id: u32) -> bool { false }
    pub unsafe fn composite_all(_graphics: &Graphics) {}
}
