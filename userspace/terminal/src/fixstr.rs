//! `FixStr<N>` — heap-free fixed-capacity string (terminal crate copy).
//!
//! Identical semantics to the filemanager crate's copy; kept separate
//! because each `no_std` binary is its own crate with no shared state.

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

    pub fn insert(&mut self, pos: usize, b: u8) {
        if self.len >= N || pos > self.len { return; }
        let mut i = self.len;
        while i > pos { self.buf[i] = self.buf[i - 1]; i -= 1; }
        self.buf[pos] = b;
        self.len += 1;
    }

    pub fn remove(&mut self, pos: usize) {
        if pos >= self.len { return; }
        let mut i = pos;
        while i + 1 < self.len { self.buf[i] = self.buf[i + 1]; i += 1; }
        self.len -= 1;
    }

    pub fn clear(&mut self) { self.len = 0; }

    pub fn is_empty(&self) -> bool { self.len == 0 }

    pub fn starts_with(&self, prefix: &str) -> bool {
        self.as_str().starts_with(prefix)
    }
}
