//! Global stdin ring buffer.
//!
//! The keyboard ISR pushes every printable key (and Enter/Backspace) here.
//! User programs drain it via the GetChar syscall or Read(fd=0).

const BUF_SIZE: usize = 256;

static mut BUF:  [u8; BUF_SIZE] = [0; BUF_SIZE];
static mut HEAD: usize = 0; // next read position
static mut TAIL: usize = 0; // next write position

/// Push one byte. Called from the keyboard ISR — no locks needed on x86
/// single-core since interrupts are disabled during ISR execution.
pub fn push(ch: u8) {
    unsafe {
        let next = (TAIL + 1) % BUF_SIZE;
        if next != HEAD {          // only if not full
            BUF[TAIL] = ch;
            TAIL = next;
        }
    }
}

/// Pop one byte. Returns `None` if the buffer is empty.
pub fn pop() -> Option<u8> {
    unsafe {
        if HEAD == TAIL { return None; }
        let ch = BUF[HEAD];
        HEAD = (HEAD + 1) % BUF_SIZE;
        Some(ch)
    }
}

/// Number of bytes currently available.
pub fn available() -> usize {
    unsafe { (TAIL + BUF_SIZE - HEAD) % BUF_SIZE }
}
