// src/kernel/syscall_core.rs
//! Core syscall types and dispatch rules.
//!
//! Written against `core` only so the same logic can be reused from the
//! kernel and from host-side unit-tests.

use core::{mem::size_of, ptr, slice};

pub const TIMER_HZ: u64 = 100;
pub const USER_SPACE_START: u64 = 0x1000;
pub const KERNEL_SPACE_BASE: u64 = 0xFFFF_8000_0000_0000;

// ── Syscall numbers (Linux x86-64 ABI) ────────────────────────────────────
//
// Standard syscalls use Linux x86-64 numbers so that musl-compiled binaries
// work without translation.  OxideOS-specific calls use numbers ≥ 400 to
// avoid conflicting with any current or future Linux syscall.
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    // ── POSIX / Linux-compatible (Linux x86-64 numbers) ─────────────────
    Read          = 0,
    Write         = 1,
    Open          = 2,
    Close         = 3,   // closes files and sockets
    Stat          = 4,
    Fstat         = 5,
    Poll          = 7,   // poll(fds, nfds, timeout_ms)
    Mmap          = 9,   // already matched Linux
    Munmap        = 11,
    Brk           = 12,
    Sigaction     = 13,  // rt_sigaction
    Sigreturn     = 15,  // rt_sigreturn
    Ioctl         = 16,
    Pipe          = 22,
    Dup2          = 33,
    Sleep         = 35,  // nanosleep (OxideOS accepts seconds in rdi)
    GetPid        = 39,
    Socket        = 41,
    Connect       = 42,
    Accept        = 43,
    Sendto        = 44,
    Recvfrom      = 45,
    Bind          = 49,
    Listen        = 50,
    Fork          = 57,
    Exec          = 59,  // execve
    Exit          = 60,
    Wait          = 61,  // wait4
    Kill          = 62,
    Truncate      = 76,
    ReadDir       = 78,  // getdents64
    Getcwd        = 79,
    Chdir         = 80,
    Rename        = 82,
    Mkdir         = 83,
    Unlink        = 87,
    Chmod         = 90,
    Chown         = 92,
    GetTime       = 96,  // gettimeofday
    // ── SysV shared memory (Linux x86-64 numbers) ───────────────────────
    Shmget        = 29,
    Shmat         = 30,
    Shmdt         = 67,
    // ── OxideOS-specific (≥ 400, no Linux conflict) ──────────────────────
    Print         = 400,
    GetChar       = 401,
    GetSystemInfo = 402,
    Getenv        = 403,
    Setenv        = 404,
    ExecArgs      = 405,
    Send          = 406,
    Recv          = 407,
    CloseSocket   = 408,
    MsgqCreate    = 415,
    Msgsnd        = 416,
    Msgrcv        = 417,
    MsgqDestroy   = 418,
    MsgrcvWait    = 419,
    MsgqLen       = 420,
    // ── GUI syscalls (OxideOS-specific, 425–432) ─────────────────────────
    GuiCreate     = 425,
    GuiDestroy    = 426,
    GuiFillRect   = 427,
    GuiDrawText   = 428,
    GuiPresent    = 429,
    GuiPollEvent  = 430,
    GuiGetSize    = 431,
    GuiBlitShm    = 432,
    // ── Installer syscalls (OxideOS-specific, 433–434) ────────────────────
    /// Returns sector count of secondary disk, or negative if not present.
    InstallQuery  = 433,
    /// Run full installation to secondary disk; blocks until done.
    InstallBegin  = 434,
    Invalid       = u64::MAX,
}

impl Syscall {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Exit          => "exit",
            Self::Fork          => "fork",
            Self::Wait          => "wait",
            Self::GetPid        => "getpid",
            Self::Exec          => "exec",
            Self::ExecArgs      => "exec_args",
            Self::Mmap          => "mmap",
            Self::Munmap        => "munmap",
            Self::Brk           => "brk",
            Self::Read          => "read",
            Self::Write         => "write",
            Self::Open          => "open",
            Self::Close         => "close",
            Self::Print         => "print",
            Self::GetChar       => "getchar",
            Self::GetTime       => "gettime",
            Self::Sleep         => "sleep",
            Self::GetSystemInfo => "get_system_info",
            Self::Pipe          => "pipe",
            Self::ReadDir       => "readdir",
            Self::Mkdir         => "mkdir",
            Self::Chdir         => "chdir",
            Self::Getcwd        => "getcwd",
            Self::Stat          => "stat",
            Self::Fstat         => "fstat",
            Self::Poll          => "poll",
            Self::Unlink        => "unlink",
            Self::Rename        => "rename",
            Self::Truncate      => "truncate",
            Self::Getenv        => "getenv",
            Self::Setenv        => "setenv",
            Self::Dup2          => "dup2",
            Self::Kill          => "kill",
            Self::Ioctl         => "ioctl",
            Self::Sigaction     => "sigaction",
            Self::Sigreturn     => "sigreturn",
            Self::Chmod         => "chmod",
            Self::Chown         => "chown",
            Self::Socket        => "socket",
            Self::Bind          => "bind",
            Self::Connect       => "connect",
            Self::Listen        => "listen",
            Self::Accept        => "accept",
            Self::Send          => "send",
            Self::Recv          => "recv",
            Self::CloseSocket   => "close_socket",
            Self::Sendto        => "sendto",
            Self::Recvfrom      => "recvfrom",
            Self::MsgqCreate    => "msgq_create",
            Self::Msgsnd        => "msgsnd",
            Self::Msgrcv        => "msgrcv",
            Self::MsgqDestroy   => "msgq_destroy",
            Self::MsgrcvWait    => "msgrcv_wait",
            Self::MsgqLen       => "msgq_len",
            Self::Shmget        => "shmget",
            Self::Shmat         => "shmat",
            Self::Shmdt         => "shmdt",
            Self::GuiCreate     => "gui_create",
            Self::GuiDestroy    => "gui_destroy",
            Self::GuiFillRect   => "gui_fill_rect",
            Self::GuiDrawText   => "gui_draw_text",
            Self::GuiPresent    => "gui_present",
            Self::GuiPollEvent  => "gui_poll_event",
            Self::GuiGetSize    => "gui_get_size",
            Self::GuiBlitShm    => "gui_blit_shm",
            Self::InstallQuery  => "install_query",
            Self::InstallBegin  => "install_begin",
            Self::Invalid       => "invalid",
        }
    }
}

impl From<u64> for Syscall {
    fn from(num: u64) -> Self {
        match num {
            // ── POSIX / Linux-compatible ─────────────────────────────────
            0   => Self::Read,
            1   => Self::Write,
            2   => Self::Open,
            3   => Self::Close,
            4   => Self::Stat,
            5   => Self::Fstat,
            7   => Self::Poll,
            9   => Self::Mmap,
            11  => Self::Munmap,
            12  => Self::Brk,
            13  => Self::Sigaction,
            15  => Self::Sigreturn,
            16  => Self::Ioctl,
            22  => Self::Pipe,
            29  => Self::Shmget,
            30  => Self::Shmat,
            33  => Self::Dup2,
            35  => Self::Sleep,
            39  => Self::GetPid,
            41  => Self::Socket,
            42  => Self::Connect,
            43  => Self::Accept,
            44  => Self::Sendto,
            45  => Self::Recvfrom,
            49  => Self::Bind,
            50  => Self::Listen,
            57  => Self::Fork,
            59  => Self::Exec,
            60  => Self::Exit,
            61  => Self::Wait,
            62  => Self::Kill,
            67  => Self::Shmdt,
            76  => Self::Truncate,
            78  => Self::ReadDir,
            79  => Self::Getcwd,
            80  => Self::Chdir,
            82  => Self::Rename,
            83  => Self::Mkdir,
            87  => Self::Unlink,
            90  => Self::Chmod,
            92  => Self::Chown,
            96  => Self::GetTime,
            // ── OxideOS-specific ─────────────────────────────────────────
            400 => Self::Print,
            401 => Self::GetChar,
            402 => Self::GetSystemInfo,
            403 => Self::Getenv,
            404 => Self::Setenv,
            405 => Self::ExecArgs,
            406 => Self::Send,
            407 => Self::Recv,
            408 => Self::CloseSocket,
            415 => Self::MsgqCreate,
            416 => Self::Msgsnd,
            417 => Self::Msgrcv,
            418 => Self::MsgqDestroy,
            419 => Self::MsgrcvWait,
            420 => Self::MsgqLen,
            425 => Self::GuiCreate,
            426 => Self::GuiDestroy,
            427 => Self::GuiFillRect,
            428 => Self::GuiDrawText,
            429 => Self::GuiPresent,
            430 => Self::GuiPollEvent,
            431 => Self::GuiGetSize,
            432 => Self::GuiBlitShm,
            433 => Self::InstallQuery,
            434 => Self::InstallBegin,
            _   => Self::Invalid,
        }
    }
}

// ── Result / request types ─────────────────────────────────────────────────
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallResult {
    pub value: i64,
    pub error: bool,
}

impl SyscallResult {
    pub const fn ok(value: i64) -> Self { Self { value, error: false } }
    pub const fn err(error_code: i64) -> Self { Self { value: error_code, error: true } }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemInfo {
    pub total_memory:  u64,
    pub free_memory:   u64,
    pub uptime_ms:     u64,
    pub process_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyscallRequest {
    pub number: u64,
    pub arg1:   u64,
    pub arg2:   u64,
    pub arg3:   u64,
    pub arg4:   u64,
    pub arg5:   u64,
}

impl SyscallRequest {
    pub const fn new(number: u64, arg1: u64, arg2: u64,
                     arg3: u64, arg4: u64, arg5: u64) -> Self {
        Self { number, arg1, arg2, arg3, arg4, arg5 }
    }
    pub fn syscall(self) -> Syscall { Syscall::from(self.number) }
}

// ── Error codes ────────────────────────────────────────────────────────────
pub const EINVAL: i64 = -1;
pub const ENOSYS: i64 = -2;
pub const EACCES: i64 = -3;
pub const ENOMEM: i64 = -4;
pub const EBADF:  i64 = -5;
pub const EAGAIN: i64 = -6;
pub const ENOENT: i64 = -7;

// ── Runtime trait ──────────────────────────────────────────────────────────
pub trait SyscallRuntime {
    fn trace(&mut self, _syscall: Syscall) {}
    fn current_pid(&self) -> u64 { 1 }
    fn current_ticks(&self) -> u64;
    fn write_console(&mut self, bytes: &[u8]);

    fn fill_system_info(&self, info: &mut SystemInfo) {
        *info = SystemInfo {
            total_memory:  128 * 1024 * 1024,
            free_memory:   64  * 1024 * 1024,
            uptime_ms:     self.current_ticks() * 1000 / TIMER_HZ,
            process_count: 1,
        };
    }

    fn sleep_until_tick(&mut self, target_tick: u64);
    fn exit(&mut self, code: i32) -> !;

    // ── stdin ──────────────────────────────────────────────────────────────
    /// Pop one byte from the stdin ring. Returns EAGAIN (-6) if empty.
    fn get_char(&mut self) -> i64 { EAGAIN }

    // ── Process model ──────────────────────────────────────────────────────
    /// Replace the current process image with the ELF at `path`.
    /// On success this never returns (jumps directly into the new image).
    /// On failure it returns a negative error code.
    fn exec_program(&mut self, _path: &[u8]) -> i64 { ENOSYS }

    /// Like `exec_program` but also passes a space-separated argument string.
    /// `args` is the argv[1..] content (does NOT include the program name).
    fn exec_program_args(&mut self, path: &[u8], _args: &[u8]) -> i64 {
        self.exec_program(path) // default: ignore args
    }

    /// Fork the current process.  Returns child PID to parent, 0 to child.
    fn fork_child(&mut self) -> i64 { ENOSYS }

    /// Block until the child with `pid` exits; return its exit code.
    /// May never return if it blocks (calls exit_to_kernel internally).
    fn waitpid_impl(&mut self, _pid: u64) -> i64 { ENOSYS }

    /// Set/query the userspace heap break.  `new_end == 0` returns current end.
    fn brk_program(&mut self, _new_end: u64) -> i64 { ENOSYS }

    /// Map `len` bytes of anonymous zeroed memory.
    /// `addr` is a hint (0 = kernel chooses). Returns the mapped virtual address
    /// or a negative error code. Only MAP_ANONYMOUS|MAP_PRIVATE is supported.
    fn mmap_anon(&mut self, _addr: u64, _len: u64) -> i64 { ENOSYS }

    /// Unmap a previously mapped region. No-op stub (returns 0).
    fn munmap_impl(&mut self, _addr: u64, _len: u64) -> i64 { 0 }

    /// Send `signum` to the process with the given PID.  Returns 0 on success.
    fn kill_pid_sig(&mut self, _pid: u64, _signum: u8) -> i64 { ENOSYS }

    /// Perform a device-specific control operation.
    /// `fd`: file descriptor (0=stdin, 1=stdout), `request`: ioctl code, `arg`: pointer/value.
    fn ioctl_impl(&mut self, _fd: i32, _request: u64, _arg: u64) -> i64 { ENOSYS }

    /// Register a signal handler. `handler` = user fn ptr, `SIG_DFL` (0), or `SIG_IGN` (1).
    /// If `old_ptr` is non-zero it receives the previous handler address.
    fn sigaction_impl(&mut self, _signum: u32, _handler: u64, _old_ptr: u64) -> i64 { ENOSYS }

    /// Restore the pre-signal execution context (called by the trampoline after a handler returns).
    fn sigreturn_impl(&mut self) -> i64 { ENOSYS }

    // ── File permission syscalls ───────────────────────────────────────────
    /// Change permission bits on a file at `path`.  `mode` is the POSIX mode (e.g. 0o644).
    fn chmod_impl(&mut self, _path: &[u8], _mode: u16) -> i64 { ENOSYS }
    /// Change owner/group of a file at `path`.
    fn chown_impl(&mut self, _path: &[u8], _uid: u32, _gid: u32) -> i64 { ENOSYS }

    // ── Shared memory syscalls ─────────────────────────────────────────────
    /// Create or open a shared memory segment. Returns a shm-id ≥ 0 or negative error.
    fn shmget_impl(&mut self, _key: u32, _size: u64, _flags: u32) -> i64 { ENOSYS }
    /// Attach a shared memory segment into the calling process's address space.
    /// Returns the virtual address on success, or a negative error.
    fn shmat_impl(&mut self, _shmid: u32, _addr_hint: u64) -> i64 { ENOSYS }
    /// Detach a shared memory segment previously attached at `addr`.
    fn shmdt_impl(&mut self, _addr: u64) -> i64 { ENOSYS }

    // ── Socket syscalls ────────────────────────────────────────────────────
    fn socket_impl(&mut self, _domain: u32, _type_: u32, _proto: u32) -> i64 { ENOSYS }
    unsafe fn bind_impl(&mut self, _sfd: u64, _addr_ptr: u64, _addr_len: usize) -> i64 { ENOSYS }
    unsafe fn connect_impl(&mut self, _sfd: u64, _addr_ptr: u64, _addr_len: usize) -> i64 { ENOSYS }
    fn listen_impl(&mut self, _sfd: u64, _backlog: i32) -> i64 { ENOSYS }
    fn accept_impl(&mut self, _sfd: u64) -> i64 { ENOSYS }
    unsafe fn send_impl(&mut self, _sfd: u64, _buf_ptr: u64, _len: usize, _flags: u32) -> i64 { ENOSYS }
    unsafe fn recv_impl(&mut self, _sfd: u64, _buf_ptr: u64, _len: usize, _flags: u32) -> i64 { ENOSYS }
    fn close_socket_impl(&mut self, _sfd: u64) -> i64 { ENOSYS }
    unsafe fn sendto_impl(&mut self, _sfd: u64, _buf_ptr: u64, _len: usize, _flags: u32,
                          _addr_ptr: u64, _addr_len: usize) -> i64 { ENOSYS }
    unsafe fn recvfrom_impl(&mut self, _sfd: u64, _buf_ptr: u64, _len: usize, _flags: u32,
                            _addr_ptr: u64, _addr_len_ptr: u64) -> i64 { ENOSYS }

    /// Fill `buf` with newline-separated directory entries under `path`.
    /// Each entry is `<name>\n` for files, `<name>/\n` for directories.
    /// Returns bytes written, or negative on error.
    fn readdir_impl(&mut self, _path: &[u8], _buf: &mut [u8]) -> i64 { ENOSYS }

    /// Create a directory at `path`.  Returns 0 on success.
    fn mkdir_impl(&mut self, _path: &[u8]) -> i64 { ENOSYS }

    /// Change the calling process's working directory.  Returns 0 on success.
    fn chdir_impl(&mut self, _path: &[u8]) -> i64 { ENOSYS }

    /// Copy the current working directory into `buf`.  Returns bytes written.
    fn getcwd_impl(&mut self, _buf: &mut [u8]) -> i64 { ENOSYS }

    /// Stat a file by path.  Writes a `FileStat` into the buffer at `buf_ptr`.
    /// Returns 0 on success or a negative error code.
    fn stat_impl(&mut self, _path: &[u8], _buf_ptr: u64) -> i64 { ENOSYS }

    /// Stat an open file descriptor.  Writes a `FileStat` into `buf_ptr`.
    fn fstat_impl(&mut self, _fd: i32, _buf_ptr: u64) -> i64 { ENOSYS }

    /// poll(fds, nfds, timeout_ms) — check FD readiness.
    /// Returns number of ready FDs, 0 on timeout, or negative error.
    fn poll_impl(&mut self, _fds_ptr: u64, _nfds: u64, _timeout_ms: i64) -> i64 { ENOSYS }

    /// Remove a file at `path`.
    fn unlink_impl(&mut self, _path: &[u8]) -> i64 { ENOSYS }

    /// Rename/move a file from `old_path` to `new_path`.
    fn rename_impl(&mut self, _old_path: &[u8], _new_path: &[u8]) -> i64 { ENOSYS }

    /// Truncate an open file descriptor to `length` bytes.
    fn truncate_impl(&mut self, _fd: i32, _length: u64) -> i64 { ENOSYS }

    /// Read the value of environment variable `key` into `buf`.
    /// Returns bytes written on success, -ENOENT if not found.
    fn getenv_impl(&mut self, _key: &[u8], _buf: &mut [u8]) -> i64 { ENOSYS }

    /// Set environment variable `key` to `val`.  Empty val deletes the key.
    /// Returns 0 on success.
    fn setenv_impl(&mut self, _key: &[u8], _val: &[u8]) -> i64 { ENOSYS }

    /// Duplicate `old_fd` to `new_fd`.  Returns `new_fd` on success.
    fn dup2_impl(&mut self, _old_fd: i32, _new_fd: i32) -> i64 { ENOSYS }

    // ── Filesystem hooks (default: not supported) ──────────────────────────
    /// Open a file; `path` is raw bytes from user space.
    fn fs_open(&mut self, path: &[u8], flags: u32) -> i64 { ENOSYS }
    /// Close a file descriptor.
    fn fs_close(&mut self, fd: i32) -> i64 { ENOSYS }
    /// Read from a file descriptor into `buf`.
    fn fs_read(&mut self, fd: i32, buf: &mut [u8]) -> i64 { ENOSYS }
    /// Write `buf` to a file descriptor ≥ 3 (FD 1/2 go to `write_console`).
    fn fs_write_file(&mut self, fd: i32, buf: &[u8]) -> i64 { ENOSYS }

    // ── IPC ────────────────────────────────────────────────────────────────
    /// Allocate a pipe; write (read_fd, write_fd) to the two user pointers.
    fn pipe_alloc(&mut self, read_fd_ptr: u64, write_fd_ptr: u64) -> i64 { ENOSYS }

    /// Create or open a message queue.
    fn msgq_create(&mut self, _id: u32) -> i64 { ENOSYS }
    /// Send a message to a queue.
    fn msgsnd(&mut self, _id: u32, _type_id: u32, _data: &[u8]) -> i64 { ENOSYS }
    /// Non-blocking receive from a queue (EAGAIN if empty).
    fn msgrcv(&mut self, _id: u32, _msg_out_ptr: u64) -> i64 { ENOSYS }
    /// Destroy a message queue and free its slot.
    fn msgq_destroy(&mut self, _id: u32) -> i64 { ENOSYS }
    /// Blocking receive — task sleeps until a message is available.
    fn msgrcv_wait(&mut self, _id: u32, _msg_out_ptr: u64) -> i64 { ENOSYS }
    /// Return the number of pending messages in the queue.
    fn msgq_len(&mut self, _id: u32) -> i64 { ENOSYS }

    // ── GUI process syscalls ───────────────────────────────────────────────
    /// Create a window for this process.  Returns window_id or negative error.
    unsafe fn gui_create_impl(&mut self, _pid: u64, _title: &[u8], _w: u32, _h: u32) -> i64 { ENOSYS }
    /// Destroy a window owned by this process.
    fn gui_destroy_impl(&mut self, _pid: u64, _win_id: u32) -> i64 { ENOSYS }
    /// Fill a rectangle in the window's content area (window-relative coords).
    fn gui_fill_rect_impl(&mut self, _pid: u64, _win_id: u32,
                          _x: u32, _y: u32, _w: u32, _h: u32, _color: u32) -> i64 { ENOSYS }
    /// Draw text in the window's content area (window-relative coords).
    unsafe fn gui_draw_text_impl(&mut self, _pid: u64, _win_id: u32,
                                 _x: u32, _y: u32, _color: u32, _text: &[u8]) -> i64 { ENOSYS }
    /// Signal that the process is done composing a frame.
    fn gui_present_impl(&mut self, _pid: u64, _win_id: u32) -> i64 { ENOSYS }
    /// Read the next pending GUI event.  Returns 0 on success, -EAGAIN if empty.
    fn gui_poll_event_impl(&mut self, _pid: u64, _win_id: u32, _event_ptr: u64) -> i64 { ENOSYS }
    /// Write the content-area width and height to `w_ptr` / `h_ptr`.
    fn gui_get_size_impl(&mut self, _pid: u64, _win_id: u32, _w_ptr: u64, _h_ptr: u64) -> i64 { ENOSYS }
    /// Blit a shared-memory framebuffer into the window.
    fn gui_blit_shm_impl(&mut self, _pid: u64, _win_id: u32, _shm_id: u32,
                         _sx: u32, _sy: u32, _sw: u32, _sh: u32,
                         _dx: u32, _dy: u32) -> i64 { ENOSYS }
    /// Return secondary disk sector count (positive), or -1 if no disk.
    fn install_query_impl(&mut self) -> i64 { -1 }
    /// Run the full installer to the secondary disk. Blocks until done.
    fn install_begin_impl(&mut self) -> i64 { ENOSYS }
}

// ── Validation ─────────────────────────────────────────────────────────────
pub fn validate_user_range(ptr_addr: u64, len: u64) -> Result<(), i64> {
    if len == 0 { return Ok(()); }
    if ptr_addr < USER_SPACE_START || len > usize::MAX as u64 { return Err(EINVAL); }
    let end = ptr_addr.checked_add(len - 1).ok_or(EINVAL)?;
    if end >= KERNEL_SPACE_BASE { return Err(EINVAL); }
    Ok(())
}

unsafe fn write_user_value<T>(ptr_addr: u64, value: T) -> Result<(), i64> {
    validate_user_range(ptr_addr, size_of::<T>() as u64)?;
    unsafe { ptr::write_unaligned(ptr_addr as *mut T, value); }
    Ok(())
}

// ── Dispatcher ─────────────────────────────────────────────────────────────
pub unsafe fn dispatch<R: SyscallRuntime>(
    runtime: &mut R, request: SyscallRequest,
) -> SyscallResult {
    let syscall = request.syscall();
    runtime.trace(syscall);

    match syscall {
        Syscall::Exit          => runtime.exit(request.arg1 as i32),
        Syscall::Fork          => sys_fork(runtime),
        Syscall::Wait          => sys_waitpid(runtime, request.arg1),
        Syscall::Exec          => unsafe { sys_exec(runtime, request.arg1, request.arg2) },
        Syscall::ExecArgs      => unsafe { sys_exec_args(runtime, request.arg1, request.arg2,
                                                          request.arg3, request.arg4) },
        Syscall::GetPid        => SyscallResult::ok(runtime.current_pid() as i64),
        Syscall::Mmap          => {
            // arg1=addr, arg2=len (prot/flags/fd/offset ignored — anon only)
            let r = runtime.mmap_anon(request.arg1, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Munmap        => {
            let r = runtime.munmap_impl(request.arg1, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Brk           => sys_brk(runtime, request.arg1),
        Syscall::Read          => unsafe { sys_read(runtime, request.arg1 as i32,
                                                    request.arg2, request.arg3) },
        Syscall::Write         => unsafe { sys_write(runtime, request.arg1 as i32,
                                                     request.arg2, request.arg3) },
        Syscall::Open          => unsafe { sys_open(runtime, request.arg1,
                                                    request.arg2, request.arg3) },
        Syscall::Close         => sys_close(runtime, request.arg1 as i32),
        Syscall::Print         => unsafe { sys_print(runtime, request.arg1, request.arg2) },
        Syscall::GetChar       => {
            let r = runtime.get_char();
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GetTime       => SyscallResult::ok(runtime.current_ticks() as i64),
        Syscall::Sleep         => sys_sleep(runtime, request.arg1),
        Syscall::GetSystemInfo => unsafe { sys_get_system_info(runtime, request.arg1) },
        Syscall::Pipe          => unsafe { sys_pipe(runtime, request.arg1, request.arg2) },
        Syscall::ReadDir       => unsafe { sys_readdir(runtime, request.arg1, request.arg2,
                                                        request.arg3, request.arg4) },
        Syscall::Mkdir         => unsafe { sys_mkdir(runtime, request.arg1, request.arg2) },
        Syscall::Chdir         => unsafe { sys_chdir(runtime, request.arg1, request.arg2) },
        Syscall::Getcwd        => unsafe { sys_getcwd(runtime, request.arg1, request.arg2) },
        Syscall::Stat          => unsafe { sys_stat(runtime, request.arg1, request.arg2, request.arg3) },
        Syscall::Fstat         => {
            let r = runtime.fstat_impl(request.arg1 as i32, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Poll => {
            // arg1=fds_ptr, arg2=nfds, arg3=timeout_ms (signed)
            let r = runtime.poll_impl(request.arg1, request.arg2, request.arg3 as i64);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Unlink => unsafe {
            if let Err(e) = validate_user_range(request.arg1, request.arg2) {
                return SyscallResult::err(e);
            }
            let path = slice::from_raw_parts(request.arg1 as *const u8, request.arg2 as usize);
            let r = runtime.unlink_impl(path);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Rename => unsafe {
            // arg1=old_ptr, arg2=old_len, arg3=new_ptr, arg4=new_len
            if let Err(e) = validate_user_range(request.arg1, request.arg2) { return SyscallResult::err(e); }
            if let Err(e) = validate_user_range(request.arg3, request.arg4) { return SyscallResult::err(e); }
            let old_path = slice::from_raw_parts(request.arg1 as *const u8, request.arg2 as usize);
            let new_path = slice::from_raw_parts(request.arg3 as *const u8, request.arg4 as usize);
            let r = runtime.rename_impl(old_path, new_path);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Truncate => {
            let r = runtime.truncate_impl(request.arg1 as i32, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Getenv => unsafe { sys_getenv(runtime, request.arg1, request.arg2, request.arg3, request.arg4) },
        Syscall::Setenv => unsafe { sys_setenv(runtime, request.arg1, request.arg2, request.arg3, request.arg4) },
        Syscall::Dup2          => sys_dup2(runtime, request.arg1 as i32, request.arg2 as i32),
        Syscall::Kill          => sys_kill(runtime, request.arg1, request.arg2),
        Syscall::Ioctl         => {
            let r = runtime.ioctl_impl(request.arg1 as i32, request.arg2, request.arg3);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Sigaction     => {
            let r = runtime.sigaction_impl(request.arg1 as u32, request.arg2, request.arg3);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Sigreturn     => {
            let r = runtime.sigreturn_impl();
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Chmod => unsafe {
            if let Err(e) = validate_user_range(request.arg1, request.arg2) {
                return SyscallResult::err(e);
            }
            let path = slice::from_raw_parts(request.arg1 as *const u8, request.arg2 as usize);
            let r = runtime.chmod_impl(path, request.arg3 as u16);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Chown => unsafe {
            if let Err(e) = validate_user_range(request.arg1, request.arg2) {
                return SyscallResult::err(e);
            }
            let path = slice::from_raw_parts(request.arg1 as *const u8, request.arg2 as usize);
            let r = runtime.chown_impl(path, request.arg3 as u32, request.arg4 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        // ── Socket syscalls ──────────────────────────────────────────────
        Syscall::Socket => {
            let r = runtime.socket_impl(request.arg1 as u32, request.arg2 as u32, request.arg3 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Bind => unsafe {
            let r = runtime.bind_impl(request.arg1, request.arg2, request.arg3 as usize);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Connect => unsafe {
            let r = runtime.connect_impl(request.arg1, request.arg2, request.arg3 as usize);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Listen => {
            let r = runtime.listen_impl(request.arg1, request.arg2 as i32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Accept => {
            let r = runtime.accept_impl(request.arg1);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Send => unsafe {
            let r = runtime.send_impl(request.arg1, request.arg2, request.arg3 as usize, request.arg4 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Recv => unsafe {
            let r = runtime.recv_impl(request.arg1, request.arg2, request.arg3 as usize, request.arg4 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::CloseSocket => {
            let r = runtime.close_socket_impl(request.arg1);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Sendto => unsafe {
            let r = runtime.sendto_impl(request.arg1, request.arg2, request.arg3 as usize,
                                        request.arg4 as u32, request.arg5, 16);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Recvfrom => unsafe {
            let r = runtime.recvfrom_impl(request.arg1, request.arg2, request.arg3 as usize,
                                          request.arg4 as u32, request.arg5, 0);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::MsgqCreate    => sys_msgq_create(runtime, request.arg1 as u32),
        Syscall::Msgsnd        => unsafe { sys_msgsnd(runtime, request.arg1 as u32, request.arg2 as u32, request.arg3, request.arg4) },
        Syscall::Msgrcv        => unsafe { sys_msgrcv(runtime, request.arg1 as u32, request.arg2) },
        Syscall::MsgqDestroy   => sys_msgq_destroy(runtime, request.arg1 as u32),
        Syscall::MsgrcvWait    => unsafe { sys_msgrcv_wait(runtime, request.arg1 as u32, request.arg2) },
        Syscall::MsgqLen       => sys_msgq_len(runtime, request.arg1 as u32),
        Syscall::Shmget => {
            let r = runtime.shmget_impl(request.arg1 as u32, request.arg2, request.arg3 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Shmat => {
            let r = runtime.shmat_impl(request.arg1 as u32, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Shmdt => {
            let r = runtime.shmdt_impl(request.arg1);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        // ── GUI syscalls ─────────────────────────────────────────────────
        Syscall::GuiCreate  => unsafe { sys_gui_create(runtime, request) },
        Syscall::GuiDestroy => {
            let pid = runtime.current_pid();
            let r = runtime.gui_destroy_impl(pid, request.arg1 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GuiFillRect => {
            // Encoding: arg1=win_id, arg2=packed(x,y), arg3=packed(w,h), arg4=color
            let win_id = request.arg1 as u32;
            let x      = (request.arg2 & 0xFFFF_FFFF) as u32;
            let y      = ((request.arg2 >> 32) & 0xFFFF_FFFF) as u32;
            let w      = (request.arg3 & 0xFFFF_FFFF) as u32;
            let h      = ((request.arg3 >> 32) & 0xFFFF_FFFF) as u32;
            let color  = request.arg4 as u32;
            let pid    = runtime.current_pid();
            let r = runtime.gui_fill_rect_impl(pid, win_id, x, y, w, h, color);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GuiDrawText => unsafe { sys_gui_draw_text(runtime, request) },
        Syscall::GuiPresent  => {
            let pid = runtime.current_pid();
            let r = runtime.gui_present_impl(pid, request.arg1 as u32);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GuiPollEvent => {
            let pid = runtime.current_pid();
            let r = runtime.gui_poll_event_impl(pid, request.arg1 as u32, request.arg2);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GuiGetSize => {
            let pid = runtime.current_pid();
            let r = runtime.gui_get_size_impl(pid, request.arg1 as u32, request.arg2, request.arg3);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::GuiBlitShm => {
            // arg1=win_id, arg2=shm_id, arg3=packed(sx,sy), arg4=packed(sw,sh), arg5=packed(dx,dy)
            let win_id = request.arg1 as u32;
            let shm_id = request.arg2 as u32;
            let sx     = (request.arg3 & 0xFFFF_FFFF) as u32;
            let sy     = ((request.arg3 >> 32) & 0xFFFF_FFFF) as u32;
            let sw     = (request.arg4 & 0xFFFF_FFFF) as u32;
            let sh     = ((request.arg4 >> 32) & 0xFFFF_FFFF) as u32;
            let dx     = (request.arg5 & 0xFFFF_FFFF) as u32;
            let dy     = ((request.arg5 >> 32) & 0xFFFF_FFFF) as u32;
            let pid    = runtime.current_pid();
            let r = runtime.gui_blit_shm_impl(pid, win_id, shm_id, sx, sy, sw, sh, dx, dy);
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::InstallQuery => {
            let r = runtime.install_query_impl();
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::InstallBegin => {
            let r = runtime.install_begin_impl();
            if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
        }
        Syscall::Invalid       => SyscallResult::err(ENOSYS),
    }
}

// ── Individual syscall implementations ────────────────────────────────────

unsafe fn sys_open<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64, flags: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(path_ptr, path_len) {
        return SyscallResult::err(code);
    }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let result = runtime.fs_open(path, flags as u32);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

fn sys_close<R: SyscallRuntime>(runtime: &mut R, fd: i32) -> SyscallResult {
    let result = runtime.fs_close(fd);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

unsafe fn sys_read<R: SyscallRuntime>(
    runtime: &mut R, fd: i32, buf_ptr: u64, count: u64,
) -> SyscallResult {
    if count == 0 { return SyscallResult::ok(0); }
    if let Err(code) = validate_user_range(buf_ptr, count) {
        return SyscallResult::err(code);
    }
    let buf = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, count as usize) };
    if fd == 0 {
        // Try FD-table first (supports dup2-redirected stdin from a pipe).
        // fs_read returns EBADF (-5) when fd=0 has no FdTable entry.
        let r = runtime.fs_read(fd, buf);
        if r != EBADF {
            return if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) };
        }
        // Fallback: stdin ring buffer (one byte per call).
        let r = runtime.get_char();
        if r < 0 { return SyscallResult::err(r); }
        buf[0] = r as u8;
        return SyscallResult::ok(1);
    }
    let result = runtime.fs_read(fd, buf);
    if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) }
}

unsafe fn sys_write<R: SyscallRuntime>(
    runtime: &mut R, fd: i32, buf_ptr: u64, count: u64,
) -> SyscallResult {
    if fd < 0 { return SyscallResult::err(EBADF); }
    if let Err(code) = validate_user_range(buf_ptr, count) {
        return SyscallResult::err(code);
    }
    let buf = unsafe { slice::from_raw_parts(buf_ptr as *const u8, count as usize) };
    if fd == 1 || fd == 2 {
        // Try FD-table first (supports dup2-redirected stdout/stderr to a pipe).
        // write_fd returns fs::EBADF (-9) when fd=1/2 has no FdTable entry.
        let r = runtime.fs_write_file(fd, buf);
        if r != crate::kernel::fs::EBADF {
            return if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) };
        }
        // Fallback: plain console.
        runtime.write_console(buf);
        return SyscallResult::ok(count as i64);
    }
    // fd >= 3 (or fd == 0 which is always EBADF for writes)
    if fd >= 3 {
        let result = runtime.fs_write_file(fd, buf);
        return if result < 0 { SyscallResult::err(result) } else { SyscallResult::ok(result) };
    }
    SyscallResult::err(EBADF)
}

unsafe fn sys_print<R: SyscallRuntime>(
    runtime: &mut R, msg_ptr: u64, len: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(msg_ptr, len) {
        return SyscallResult::err(code);
    }
    let msg = unsafe { slice::from_raw_parts(msg_ptr as *const u8, len as usize) };
    runtime.write_console(msg);
    SyscallResult::ok(len as i64)
}

fn sys_sleep<R: SyscallRuntime>(runtime: &mut R, ms: u64) -> SyscallResult {
    let start = runtime.current_ticks();
    let ticks = ms.saturating_mul(TIMER_HZ) / 1000;
    runtime.sleep_until_tick(start.saturating_add(ticks));
    SyscallResult::ok(0)
}

unsafe fn sys_get_system_info<R: SyscallRuntime>(
    runtime: &mut R, info_ptr: u64,
) -> SyscallResult {
    let mut info = SystemInfo::default();
    runtime.fill_system_info(&mut info);
    match unsafe { write_user_value(info_ptr, info) } {
        Ok(())     => SyscallResult::ok(0),
        Err(code)  => SyscallResult::err(code),
    }
}

unsafe fn sys_exec<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(path_ptr, path_len) {
        return SyscallResult::err(code);
    }
    if path_len == 0 { return SyscallResult::err(EINVAL); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    // On success exec_program diverges (never returns).
    // On failure it returns a negative error code.
    let err = runtime.exec_program(path);
    SyscallResult::err(err)
}

/// ExecArgs syscall: like Exec but also receives a space-separated args string.
/// arg1=path_ptr, arg2=path_len, arg3=args_ptr, arg4=args_len
unsafe fn sys_exec_args<R: SyscallRuntime>(
    runtime: &mut R,
    path_ptr: u64, path_len: u64,
    args_ptr: u64, args_len: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(path_ptr, path_len) {
        return SyscallResult::err(code);
    }
    if path_len == 0 { return SyscallResult::err(EINVAL); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let args = if args_len > 0 {
        if let Err(code) = validate_user_range(args_ptr, args_len) {
            return SyscallResult::err(code);
        }
        unsafe { slice::from_raw_parts(args_ptr as *const u8, args_len as usize) }
    } else {
        b""
    };
    let err = runtime.exec_program_args(path, args);
    SyscallResult::err(err)
}

unsafe fn sys_pipe<R: SyscallRuntime>(
    runtime: &mut R, read_fd_ptr: u64, write_fd_ptr: u64,
) -> SyscallResult {
    if let Err(code) = validate_user_range(read_fd_ptr, 4) { return SyscallResult::err(code); }
    if let Err(code) = validate_user_range(write_fd_ptr, 4) { return SyscallResult::err(code); }
    let r = runtime.pipe_alloc(read_fd_ptr, write_fd_ptr);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_fork<R: SyscallRuntime>(runtime: &mut R) -> SyscallResult {
    let r = runtime.fork_child();
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_waitpid<R: SyscallRuntime>(runtime: &mut R, pid: u64) -> SyscallResult {
    // waitpid_impl either returns immediately (child already dead) or
    // diverges via exit_to_kernel (blocks the calling task).
    let r = runtime.waitpid_impl(pid);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_brk<R: SyscallRuntime>(runtime: &mut R, new_end: u64) -> SyscallResult {
    let r = runtime.brk_program(new_end);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_kill<R: SyscallRuntime>(runtime: &mut R, pid: u64, signum: u64) -> SyscallResult {
    // arg2 = 0 is treated as SIGKILL for backward compatibility.
    let sig = if signum == 0 { 9 } else { signum };
    let r = runtime.kill_pid_sig(pid, sig as u8);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_readdir<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64, buf_ptr: u64, buf_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(path_ptr, path_len) { return SyscallResult::err(e); }
    if let Err(e) = validate_user_range(buf_ptr,  buf_len)  { return SyscallResult::err(e); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let buf  = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    let r = runtime.readdir_impl(path, buf);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_mkdir<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(path_ptr, path_len) { return SyscallResult::err(e); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let r = runtime.mkdir_impl(path);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_chdir<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(path_ptr, path_len) { return SyscallResult::err(e); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let r = runtime.chdir_impl(path);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_getcwd<R: SyscallRuntime>(
    runtime: &mut R, buf_ptr: u64, buf_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(buf_ptr, buf_len) { return SyscallResult::err(e); }
    let buf = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    let r = runtime.getcwd_impl(buf);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

/// stat(path_ptr, path_len, stat_buf_ptr)
unsafe fn sys_stat<R: SyscallRuntime>(
    runtime: &mut R, path_ptr: u64, path_len: u64, stat_buf: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(path_ptr, path_len) { return SyscallResult::err(e); }
    // FileStat is 16 bytes (size:u64 + kind:u32 + _pad:u32)
    if let Err(e) = validate_user_range(stat_buf, 16) { return SyscallResult::err(e); }
    let path = unsafe { slice::from_raw_parts(path_ptr as *const u8, path_len as usize) };
    let r = runtime.stat_impl(path, stat_buf);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

/// Getenv: arg1=key_ptr, arg2=key_len, arg3=val_buf_ptr, arg4=val_buf_len
unsafe fn sys_getenv<R: SyscallRuntime>(
    runtime: &mut R, key_ptr: u64, key_len: u64, buf_ptr: u64, buf_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(key_ptr, key_len) { return SyscallResult::err(e); }
    if let Err(e) = validate_user_range(buf_ptr, buf_len)  { return SyscallResult::err(e); }
    let key = unsafe { slice::from_raw_parts(key_ptr as *const u8, key_len as usize) };
    let buf = unsafe { slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len as usize) };
    let r = runtime.getenv_impl(key, buf);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

/// Setenv: arg1=key_ptr, arg2=key_len, arg3=val_ptr, arg4=val_len
unsafe fn sys_setenv<R: SyscallRuntime>(
    runtime: &mut R, key_ptr: u64, key_len: u64, val_ptr: u64, val_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(key_ptr, key_len) { return SyscallResult::err(e); }
    let key = unsafe { slice::from_raw_parts(key_ptr as *const u8, key_len as usize) };
    let val = if val_len > 0 {
        if let Err(e) = validate_user_range(val_ptr, val_len) { return SyscallResult::err(e); }
        unsafe { slice::from_raw_parts(val_ptr as *const u8, val_len as usize) }
    } else {
        &[]
    };
    let r = runtime.setenv_impl(key, val);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_dup2<R: SyscallRuntime>(runtime: &mut R, old_fd: i32, new_fd: i32) -> SyscallResult {
    let r = runtime.dup2_impl(old_fd, new_fd);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_msgq_create<R: SyscallRuntime>(runtime: &mut R, id: u32) -> SyscallResult {
    let r = runtime.msgq_create(id);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_msgsnd<R: SyscallRuntime>(
    runtime: &mut R, id: u32, type_id: u32, data_ptr: u64, data_len: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(data_ptr, data_len) { return SyscallResult::err(e); }
    let data = unsafe { slice::from_raw_parts(data_ptr as *const u8, data_len as usize) };
    let r = runtime.msgsnd(id, type_id, data);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

unsafe fn sys_msgrcv<R: SyscallRuntime>(
    runtime: &mut R, id: u32, msg_out_ptr: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(msg_out_ptr, size_of::<crate::kernel::ipc::Message>() as u64) {
        return SyscallResult::err(e);
    }
    let r = runtime.msgrcv(id, msg_out_ptr);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_msgq_destroy<R: SyscallRuntime>(runtime: &mut R, id: u32) -> SyscallResult {
    let r = runtime.msgq_destroy(id);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

/// Blocking receive.  Returns ENOSYS if the runtime does not implement it
/// (e.g. test/stub runtimes); real kernel runtime diverges into the scheduler.
unsafe fn sys_msgrcv_wait<R: SyscallRuntime>(
    runtime: &mut R, id: u32, msg_out_ptr: u64,
) -> SyscallResult {
    if let Err(e) = validate_user_range(msg_out_ptr, size_of::<crate::kernel::ipc::Message>() as u64) {
        return SyscallResult::err(e);
    }
    let r = runtime.msgrcv_wait(id, msg_out_ptr);
    // If the call diverged (blocking path), we never reach here.
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

fn sys_msgq_len<R: SyscallRuntime>(runtime: &mut R, id: u32) -> SyscallResult {
    let r = runtime.msgq_len(id);
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

// ── GUI syscall helpers ────────────────────────────────────────────────────────

/// GuiCreate: arg1=title_ptr, arg2=title_len, arg3=packed(w,h)
unsafe fn sys_gui_create<R: SyscallRuntime>(
    runtime: &mut R, request: SyscallRequest,
) -> SyscallResult {
    let title_ptr = request.arg1;
    let title_len = request.arg2 as usize;
    let width     = (request.arg3 & 0xFFFF_FFFF) as u32;
    let height    = ((request.arg3 >> 32) & 0xFFFF_FFFF) as u32;

    if let Err(e) = validate_user_range(title_ptr, title_len as u64) {
        return SyscallResult::err(e);
    }
    let title = unsafe { slice::from_raw_parts(title_ptr as *const u8, title_len) };
    let pid = runtime.current_pid();
    let r = unsafe { runtime.gui_create_impl(pid, title, width, height) };
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}

/// GuiDrawText: arg1=win_id, arg2=packed(x,y), arg3=color, arg4=text_ptr, arg5=text_len
unsafe fn sys_gui_draw_text<R: SyscallRuntime>(
    runtime: &mut R, request: SyscallRequest,
) -> SyscallResult {
    let win_id   = request.arg1 as u32;
    let x        = (request.arg2 & 0xFFFF) as u32;
    let y        = ((request.arg2 >> 32) & 0xFFFF) as u32;
    let color    = request.arg3 as u32;
    let text_ptr = request.arg4;
    let text_len = request.arg5 as usize;

    if let Err(e) = validate_user_range(text_ptr, text_len as u64) {
        return SyscallResult::err(e);
    }
    let text = unsafe { slice::from_raw_parts(text_ptr as *const u8, text_len) };
    let pid = runtime.current_pid();
    let r = unsafe { runtime.gui_draw_text_impl(pid, win_id, x, y, color, text) };
    if r < 0 { SyscallResult::err(r) } else { SyscallResult::ok(r) }
}
