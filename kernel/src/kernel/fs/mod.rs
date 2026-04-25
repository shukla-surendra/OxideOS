// src/kernel/fs/mod.rs
//! Virtual filesystem layer for OxideOS.
//!
//! Currently backed by a single in-memory RamFS.  All kernel code and
//! syscall handlers talk to the filesystem through the public API exposed
//! by `ramfs::RAMFS`.

pub mod ramfs;
pub mod fat;
pub mod ext2;
pub mod mbr;
pub mod vfs;
pub mod procfs;
pub mod diskfs;

pub use ramfs::RAMFS;

// ── Open-flag constants (Linux-compatible subset) ─────────────────────────
pub const O_RDONLY: u32 = 0;
pub const O_WRONLY: u32 = 1;
pub const O_RDWR:   u32 = 2;
pub const O_CREAT:  u32 = 0x40;
pub const O_TRUNC:  u32 = 0x200;
pub const O_APPEND: u32 = 0x400;

// ── Error codes ───────────────────────────────────────────────────────────
pub const ENOENT:  i64 = -2;
pub const EEXIST:  i64 = -17;
pub const EISDIR:  i64 = -21;
pub const ENOTDIR: i64 = -20;
pub const EBADF:   i64 = -9;
pub const EINVAL:  i64 = -22;
pub const ENOSPC:  i64 = -28;
pub const EMFILE:  i64 = -24;
pub const EACCES:  i64 = -13;
