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
//! | `/ext2/…`  | ext2     |
//! | `/ext2`     | ext2     |
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
    /// Route to the ext2 driver on the secondary disk.
    Ext2 { path: &'a [u8] },
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

    // /ext2 or /ext2/…  → ext2 on secondary disk
    if path == "/ext2" || path.starts_with("/ext2/") {
        return Resolved::Ext2 { path: path.as_bytes() };
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

        Resolved::Ext2 { path: ext2_path } => {
            if !crate::kernel::ext2::is_ready() { return -19; } // ENODEV
            let raw_fd = unsafe { crate::kernel::ext2::open(ext2_path) };
            if raw_fd < 0 { return raw_fd; }
            (*fdt).open_ext2(raw_fd as i32)
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
            let listing = b"null\ntty\n";
            let n = listing.len().min(buf.len());
            buf[..n].copy_from_slice(&listing[..n]);
            n as i64
        }
        Resolved::Fat16 { fat_path } => {
            // Resolve to the specific directory (supports subdirs).
            match unsafe { crate::kernel::fat::resolve_dir(fat_path) } {
                Some(loc) => unsafe { crate::kernel::fat::list_dir_raw(loc, buf) },
                None      => -7, // ENOENT
            }
        }
        Resolved::Ext2 { path: ext2_path } => {
            if !crate::kernel::ext2::is_ready() { return -19; }
            unsafe { crate::kernel::ext2::list_dir_raw(ext2_path, buf) }
        }
        Resolved::RamFS { path } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                Some(fs) => fs.read_dir_raw(path, buf),
                None     => -2,
            }
        }
    }
}

// ── VFS mkdir ────────────────────────────────────────────────────────────────

/// Create a directory at `path`.  Returns 0 on success.
pub unsafe fn vfs_mkdir(path: &str) -> i64 {
    match resolve(path) {
        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
            unsafe { crate::kernel::fat::mkdir(fat_path) }
        }
        Resolved::Ext2 { .. } => -1, // EPERM: ext2 is read-only
        Resolved::RamFS { path } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                Some(fs) => fs.create_dir(path).map(|_| 0i64).unwrap_or(-1),
                None     => -2,
            }
        }
        Resolved::Dev { .. } => -1, // EPERM: can't mkdir in /dev
    }
}

// ── VFS chdir ────────────────────────────────────────────────────────────────

/// Change the current task's working directory to `path`.
/// Verifies the path is a real directory before accepting it.
pub unsafe fn vfs_chdir(path: &str) -> i64 {
    use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, CWD_MAX};

    // Validate that the path is an existing directory.
    let exists = match resolve(path) {
        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
            // Root of disk ("/disk") is always valid; otherwise check.
            fat_path == b"/disk" || fat_path == b"/disk/"
            || unsafe { crate::kernel::fat::resolve_dir(fat_path).is_some() }
        }
        Resolved::Ext2 { path: ext2_path } => {
            crate::kernel::ext2::is_ready()
            && (ext2_path == b"/ext2" || ext2_path == b"/ext2/"
                || unsafe { crate::kernel::ext2::is_dir(ext2_path) })
        }
        Resolved::RamFS { path: rpath } => {
            rpath == "/"
            || unsafe { crate::kernel::fs::ramfs::RAMFS.get() }
                   .and_then(|fs| fs.list_dir(rpath))
                   .is_some()
        }
        Resolved::Dev { .. } => false,
    };

    if !exists { return -7; } // ENOENT

    // Normalise: ensure trailing slash.
    let mut norm = [0u8; CWD_MAX];
    let bytes = path.as_bytes();
    let len = bytes.len().min(CWD_MAX - 1);
    norm[..len].copy_from_slice(&bytes[..len]);
    let final_len = if norm[len.saturating_sub(1)] == b'/' {
        len
    } else if len < CWD_MAX - 1 {
        norm[len] = b'/';
        len + 1
    } else {
        len
    };

    // Write the new cwd into the current task.
    unsafe {
        let sched = &raw mut SCHED;
        let idx   = CURRENT_TASK_IDX;
        let task  = &raw mut (*sched).tasks[idx];
        core::ptr::copy_nonoverlapping(norm.as_ptr(), (*task).cwd.as_mut_ptr(), final_len);
        (*task).cwd_len = final_len;
    }
    0
}

// ── VFS stat ─────────────────────────────────────────────────────────────────

/// Kind returned by `vfs_stat`.
#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum StatKind {
    File      = 0,
    Directory = 1,
    Device    = 2,
}

/// Minimal file metadata returned by the `stat` syscall.
#[repr(C)]
pub struct FileStat {
    /// File size in bytes (0 for directories and device files).
    pub size: u64,
    /// Entry type: 0=file, 1=directory, 2=device.
    pub kind: u32,
    pub _pad: u32,
}

/// Return metadata for `path`.  Writes into `*out` and returns 0 on success,
/// or a negative error code if the path does not exist.
pub unsafe fn vfs_stat(path: &str, out: *mut FileStat) -> i64 {
    unsafe {
        match resolve(path) {
            Resolved::Dev { .. } => {
                (*out).size = 0;
                (*out).kind = StatKind::Device as u32;
                (*out)._pad = 0;
                0
            }

            Resolved::Fat16 { fat_path } => {
                if !crate::kernel::ata::is_present() { return -19; }
                // Try as a directory first, then as a file.
                if fat_path == b"/disk" || fat_path == b"/disk/" {
                    (*out).size = 0;
                    (*out).kind = StatKind::Directory as u32;
                    (*out)._pad = 0;
                    return 0;
                }
                if crate::kernel::fat::resolve_dir(fat_path).is_some() {
                    (*out).size = 0;
                    (*out).kind = StatKind::Directory as u32;
                    (*out)._pad = 0;
                    return 0;
                }
                // Not a directory — try opening as a file to get its size.
                let fd = crate::kernel::fat::open(fat_path, 0);
                if fd < 0 { return -7; } // ENOENT
                let fd = fd as i32;
                // Seek to end to obtain file size.
                let size = crate::kernel::fat::file_size(fd) as u64;
                crate::kernel::fat::close(fd);
                (*out).size = size;
                (*out).kind = StatKind::File as u32;
                (*out)._pad = 0;
                0
            }

            Resolved::Ext2 { path: ext2_path } => {
                if !crate::kernel::ext2::is_ready() { return -19; }
                if ext2_path == b"/ext2" || ext2_path == b"/ext2/"
                    || crate::kernel::ext2::is_dir(ext2_path)
                {
                    (*out).size = 0;
                    (*out).kind = StatKind::Directory as u32;
                    (*out)._pad = 0;
                    return 0;
                }
                // Try opening as file to get its size.
                let fd = crate::kernel::ext2::open(ext2_path);
                if fd < 0 { return -7; }
                let fd = fd as i32;
                // Read all to count size (simple approach for now).
                let mut tmp = [0u8; 512];
                let mut size = 0u64;
                loop {
                    let n = crate::kernel::ext2::read_fd(fd, &mut tmp);
                    if n <= 0 { break; }
                    size += n as u64;
                }
                crate::kernel::ext2::close(fd);
                (*out).size = size;
                (*out).kind = StatKind::File as u32;
                (*out)._pad = 0;
                0
            }

            Resolved::RamFS { path: rpath } => {
                match crate::kernel::fs::ramfs::RAMFS.get() {
                    None => -2,
                    Some(fs) => {
                        if let Some(data) = fs.read_file(rpath) {
                            (*out).size = data.len() as u64;
                            (*out).kind = StatKind::File as u32;
                            (*out)._pad = 0;
                            0
                        } else if fs.list_dir(rpath).is_some() {
                            (*out).size = 0;
                            (*out).kind = StatKind::Directory as u32;
                            (*out)._pad = 0;
                            0
                        } else {
                            -7 // ENOENT
                        }
                    }
                }
            }
        }
    }
}
