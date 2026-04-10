//! Virtual Filesystem (VFS) layer for OxideOS.
//!
//! Provides a single unified `open` path that routes to the correct backend
//! (RamFS, FAT16, or devfs) based on the path prefix.  All other operations
//! (read, write, close, dup2) are dispatched through `FdTable` which already
//! stores the backend tag in each `FdEntry`.
//!
//! # Mount table (static, in priority order)
//!
//! | Prefix      | Backend  |
//! |-------------|----------|
//! | `/dev/null` | DevNull  |
//! | `/dev/tty`  | DevTty   |
//! | `/dev/…`   | DevNull  | (catch-all)
//! | `/disk/…`  | FAT16    |
//! | `/disk`     | FAT16    |
//! | `/`         | RamFS    |

use crate::kernel::fs::ramfs::FdBackend;

// ── Resolved path ────────────────────────────────────────────────────────────

/// The result of resolving a VFS path.
pub enum Resolved<'a> {
    /// Route to the in-memory RamFS; full original path passed as-is.
    RamFS { path: &'a str },
    /// Route to the FAT16 driver; `fat_path` is the full original path
    /// (the FAT driver strips the `/disk` prefix itself).
    Fat16 { fat_path: &'a [u8] },
    /// Route to a device-file backend.
    Dev { backend: FdBackend },
}

/// Resolve `path` to its VFS backend.
///
/// The mount table is checked in priority order so that `/disk/…` is matched
/// before the catch-all `/` → RamFS rule.
pub fn resolve<'a>(path: &'a str) -> Resolved<'a> {
    // /dev/null, /dev/tty, generic /dev/* (treat unknown as null)
    if path.starts_with("/dev/") || path == "/dev" {
        let dev_name = if path.len() > 5 { &path[5..] } else { "" };
        let backend = match dev_name {
            "tty"  => FdBackend::DevTty,
            _      => FdBackend::DevNull, // /dev/null or unknown /dev/*
        };
        return Resolved::Dev { backend };
    }

    // /disk or /disk/…  → FAT16
    if path == "/disk" || path.starts_with("/disk/") {
        return Resolved::Fat16 { fat_path: path.as_bytes() };
    }

    // Everything else → RamFS
    Resolved::RamFS { path }
}

// ── VFS open ─────────────────────────────────────────────────────────────────

/// Open `path` for the current task, returning the new FD or a negative error.
///
/// Routing:
/// - `/dev/*` → `FdTable::open_dev`
/// - `/disk/*` → `fat::open` then `FdTable::open_fat`
/// - everything else → `FdTable::open` (RamFS)
pub unsafe fn vfs_open(path: &str, flags: u32) -> i64 {
    use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};

    let sched = &raw mut SCHED;
    let idx   = CURRENT_TASK_IDX;
    let fdt   = &raw mut (*sched).tasks[idx].fd_table;

    match resolve(path) {
        Resolved::Dev { backend } => {
            (*fdt).open_dev(backend)
        }

        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; } // ENODEV
            let raw_fd = unsafe { crate::kernel::fat::open(fat_path, flags) };
            if raw_fd < 0 { return raw_fd; }
            let writable = (flags & crate::kernel::fs::O_WRONLY != 0)
                        || (flags & crate::kernel::fs::O_RDWR   != 0);
            (*fdt).open_fat(raw_fd as i32, writable)
        }

        Resolved::RamFS { path } => {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => (*fdt).open(fs, path, flags),
                None     => -2,
            }
        }
    }
}

// ── VFS readdir ───────────────────────────────────────────────────────────────

/// List directory entries for `path` into `buf` (entries separated by `\n`).
/// Returns bytes written, or a negative error.
pub fn vfs_readdir(path: &str, buf: &mut [u8]) -> i64 {
    match resolve(path) {
        Resolved::Dev { .. } => {
            // Synthesise a minimal /dev listing.
            let listing = b"null\ntty\n";
            let n = listing.len().min(buf.len());
            buf[..n].copy_from_slice(&listing[..n]);
            n as i64
        }
        Resolved::Fat16 { .. } => {
            // FAT16 directory listing: list root dir entries.
            unsafe { crate::kernel::fat::list_root_raw(buf) }
        }
        Resolved::RamFS { path } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                Some(fs) => fs.read_dir_raw(path, buf),
                None     => -2,
            }
        }
    }
}
