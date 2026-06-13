//! `FixStr<N>` — heap-free, fixed-capacity UTF-8 string.
//!
//! Backed by a `[u8; N]` stack array.  Writes silently truncate once the
//! buffer is full.  All methods are inlineable and produce no allocations.

#[derive(Clone, Copy)]
pub struct FixStr<const N: usize> {
    pub(crate) buf: [u8; N],
    pub(crate) len: usize,
}

impl<const N: usize> FixStr<N> {
    pub const fn new() -> Self { Self { buf: [0; N], len: 0 } }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }

    pub fn push(&mut self, b: u8) {
        if self.len < N { self.buf[self.len] = b; self.len += 1; }
    }

    pub fn push_str(&mut self, s: &str) { for b in s.bytes() { self.push(b); } }

    pub fn clear(&mut self) { self.len = 0; }

    pub fn is_empty(&self) -> bool { self.len == 0 }

    /// Remove and return the last byte, if any.
    pub fn pop(&mut self) -> Option<u8> {
        if self.len == 0 { return None; }
        self.len -= 1;
        let b = self.buf[self.len];
        self.buf[self.len] = 0;
        Some(b)
    }

    /// Append a `u64` as decimal digits.
    pub fn push_u64(&mut self, mut v: u64) {
        let mut tmp = [0u8; 20];
        let mut i = tmp.len();
        if v == 0 { i -= 1; tmp[i] = b'0'; }
        while v > 0 { i -= 1; tmp[i] = b'0' + (v % 10) as u8; v /= 10; }
        self.push_str(core::str::from_utf8(&tmp[i..]).unwrap_or(""));
    }

    pub fn push_usize(&mut self, v: usize) { self.push_u64(v as u64); }

    /// Append a byte count in human-readable form: `NB`, `NK`, or `N.MM`.
    pub fn push_size(&mut self, bytes: u64) {
        const MB: u64 = 1024 * 1024;
        const KB: u64 = 1024;
        if bytes >= MB {
            self.push_u64(bytes / MB); self.push(b'.');
            self.push_u64((bytes % MB) * 10 / MB); self.push_str("M");
        } else if bytes >= KB {
            self.push_u64(bytes / KB); self.push(b'K');
        } else {
            self.push_u64(bytes); self.push(b'B');
        }
    }
}
