//! Anonymous in-kernel pipes for OxideOS.
//!
//! Provides up to 8 concurrent pipes. File descriptor assignment:
//!   read  end of pipe N → FD  80 + N*2
//!   write end of pipe N → FD  81 + N*2
//!
//! Range is therefore FDs 80-95 (16 FDs for 8 pipes).

pub const PIPE_FD_BASE: i32 = 80;
const PIPE_COUNT: usize = 8;
const PIPE_BUF: usize = 4096;

struct Pipe {
    buf:        [u8; PIPE_BUF],
    head:       usize, // next read position
    tail:       usize, // next write position
    read_open:  bool,
    write_open: bool,
}

impl Pipe {
    const fn new() -> Self {
        Self {
            buf:        [0; PIPE_BUF],
            head:       0,
            tail:       0,
            read_open:  false,
            write_open: false,
        }
    }

    fn is_empty(&self) -> bool { self.head == self.tail }

    fn len(&self) -> usize { (self.tail + PIPE_BUF - self.head) % PIPE_BUF }

    fn available_write(&self) -> usize { PIPE_BUF - 1 - self.len() }
}

static mut PIPES: [Pipe; PIPE_COUNT] = [const { Pipe::new() }; PIPE_COUNT];

// ── FD helpers ─────────────────────────────────────────────────────────────

/// Returns true if `fd` belongs to the pipe FD range.
pub fn is_pipe_fd(fd: i32) -> bool {
    fd >= PIPE_FD_BASE && fd < PIPE_FD_BASE + (PIPE_COUNT as i32) * 2
}

fn pipe_index(fd: i32) -> usize { ((fd - PIPE_FD_BASE) / 2) as usize }
fn is_read_fd(fd: i32)  -> bool { (fd - PIPE_FD_BASE) % 2 == 0 }

// ── Public API ─────────────────────────────────────────────────────────────

/// Allocate a new pipe. Returns `(read_fd, write_fd)` on success.
pub unsafe fn alloc() -> Option<(i32, i32)> {
    let pipes = &raw mut PIPES;
    for i in 0..PIPE_COUNT {
        let p = &raw mut (*pipes)[i];
        if !(*p).read_open && !(*p).write_open {
            (*p).head       = 0;
            (*p).tail       = 0;
            (*p).read_open  = true;
            (*p).write_open = true;
            let read_fd  = PIPE_FD_BASE + (i as i32) * 2;
            let write_fd = read_fd + 1;
            return Some((read_fd, write_fd));
        }
    }
    None
}

/// Write bytes to a pipe's write end. Returns bytes written or negative error.
pub unsafe fn write(fd: i32, data: &[u8]) -> i64 {
    if !is_pipe_fd(fd) || is_read_fd(fd) { return -5; } // EBADF
    let pipes = &raw mut PIPES;
    let p = &raw mut (*pipes)[pipe_index(fd)];
    if !(*p).write_open { return -5; }

    let mut written = 0usize;
    for &byte in data {
        if (*p).available_write() == 0 { break; }
        let next = ((*p).tail + 1) % PIPE_BUF;
        (*p).buf[(*p).tail] = byte;
        (*p).tail = next;
        written += 1;
    }
    written as i64
}

/// Read bytes from a pipe's read end. Returns bytes read or negative error.
pub unsafe fn read(fd: i32, buf: &mut [u8]) -> i64 {
    if !is_pipe_fd(fd) || !is_read_fd(fd) { return -5; } // EBADF
    let pipes = &raw mut PIPES;
    let p = &raw mut (*pipes)[pipe_index(fd)];
    if !(*p).read_open { return -5; }
    if (*p).is_empty() { return -6; } // EAGAIN

    let mut read = 0usize;
    while read < buf.len() && !(*p).is_empty() {
        buf[read] = (*p).buf[(*p).head];
        (*p).head = ((*p).head + 1) % PIPE_BUF;
        read += 1;
    }
    read as i64
}

/// Close one end of a pipe.
pub unsafe fn close(fd: i32) -> i64 {
    if !is_pipe_fd(fd) { return -5; }
    let pipes = &raw mut PIPES;
    let p = &raw mut (*pipes)[pipe_index(fd)];
    if is_read_fd(fd) {
        (*p).read_open = false;
    } else {
        (*p).write_open = false;
    }
    0
}
