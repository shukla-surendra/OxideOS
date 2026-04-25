//! Virtual Filesystem (VFS) layer for OxideOS.
//!
//! # Mount table (priority order)
//!
//! | Prefix       | Backend       |
//! |--------------|---------------|
//! | `/dev/*`     | DevFS         |
//! | `/disk/*`    | FAT16         |
//! | `/ext2/*`    | ext2          |
//! | `/proc/*`    | procfs        |
//! | `/store/*`   | DiskStore     |
//! | `/`          | RamFS         |

use crate::kernel::fs::ramfs::FdBackend;

// ── Resolved ──────────────────────────────────────────────────────────────

pub enum Resolved<'a> {
    RamFS     { path: &'a str },
    Fat16     { fat_path: &'a [u8] },
    Ext2      { path: &'a [u8] },
    Dev       { backend: FdBackend },
    Proc      { path: &'a str },
    /// `/store` or `/store/<id>` — backed by the on-disk record store.
    DiskStore { path: &'a str },
}

pub fn resolve<'a>(path: &'a str) -> Resolved<'a> {
    if path.starts_with("/dev/") || path == "/dev" {
        let dev_name = if path.len() > 5 { &path[5..] } else { "" };
        let backend = match dev_name {
            "tty" => FdBackend::DevTty,
            _     => FdBackend::DevNull,
        };
        return Resolved::Dev { backend };
    }
    if path == "/disk" || path.starts_with("/disk/") {
        return Resolved::Fat16 { fat_path: path.as_bytes() };
    }
    if path == "/ext2" || path.starts_with("/ext2/") {
        return Resolved::Ext2 { path: path.as_bytes() };
    }
    if path == "/proc" || path.starts_with("/proc/") {
        return Resolved::Proc { path };
    }
    if path == "/store" || path.starts_with("/store/") {
        return Resolved::DiskStore { path };
    }
    Resolved::RamFS { path }
}

// ── vfs_open ──────────────────────────────────────────────────────────────

pub unsafe fn vfs_open(path: &str, flags: u32) -> i64 {
    use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX};
    let sched = &raw mut SCHED;
    let idx   = CURRENT_TASK_IDX;
    let fdt   = &raw mut (*sched).tasks[idx].fd_table;

    match resolve(path) {
        Resolved::Dev { backend } => (*fdt).open_dev(backend),

        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
            if fat_path == b"/disk" || fat_path == b"/disk/"
                || crate::kernel::fat::resolve_dir(fat_path).is_some()
            {
                return (*fdt).open_dir(fat_path);
            }
            let raw_fd = unsafe { crate::kernel::fat::open(fat_path, flags) };
            if raw_fd < 0 { return raw_fd; }
            let writable = (flags & crate::kernel::fs::O_WRONLY != 0)
                        || (flags & crate::kernel::fs::O_RDWR   != 0);
            (*fdt).open_fat(raw_fd as i32, writable)
        }

        Resolved::Ext2 { path: ext2_path } => {
            if !crate::kernel::ext2::is_ready() { return -19; }
            if crate::kernel::ext2::is_dir(ext2_path) {
                return (*fdt).open_dir(ext2_path);
            }
            let raw_fd = unsafe { crate::kernel::ext2::open(ext2_path) };
            if raw_fd < 0 { return raw_fd; }
            (*fdt).open_ext2(raw_fd as i32)
        }

        Resolved::RamFS { path } => {
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => {
                    if fs.list_dir(path).is_some() {
                        return (*fdt).open_dir(path.as_bytes());
                    }
                    (*fdt).open(fs, path, flags)
                }
                None => -2,
            }
        }

        Resolved::Proc { path } => {
            crate::kernel::procfs::refresh(path);
            match crate::kernel::fs::ramfs::RAMFS.get() {
                Some(fs) => {
                    if fs.list_dir(path).is_some() {
                        return (*fdt).open_dir(path.as_bytes());
                    }
                    (*fdt).open(fs, path, flags)
                }
                None => -2,
            }
        }

        Resolved::DiskStore { path } => {
            if path == "/store" || path == "/store/" {
                // Directory: refresh all records then open RamFS dir.
                crate::kernel::diskfs::refresh_all_records();
                match crate::kernel::fs::ramfs::RAMFS.get() {
                    Some(fs) => (*fdt).open_dir(path.as_bytes()),
                    None     => -2,
                }
            } else {
                // File: /store/<id>
                match crate::kernel::diskfs::parse_record_id(path) {
                    Some(id) => {
                        crate::kernel::diskfs::refresh_record(id);
                        match crate::kernel::fs::ramfs::RAMFS.get() {
                            Some(fs) => (*fdt).open(fs, path, flags),
                            None     => -2,
                        }
                    }
                    None => -2, // ENOENT — can't parse id
                }
            }
        }
    }
}

// ── vfs_readdir ───────────────────────────────────────────────────────────

pub fn vfs_readdir(path: &str, buf: &mut [u8]) -> i64 {
    match resolve(path) {
        Resolved::Dev { .. } => {
            let listing = b"null\ntty\n";
            let n = listing.len().min(buf.len());
            buf[..n].copy_from_slice(&listing[..n]);
            n as i64
        }
        Resolved::Fat16 { fat_path } => {
            match unsafe { crate::kernel::fat::resolve_dir(fat_path) } {
                Some(loc) => unsafe { crate::kernel::fat::list_dir_raw(loc, buf) },
                None      => -7,
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
        Resolved::Proc { path } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                Some(fs) => fs.read_dir_raw(path, buf),
                None     => -2,
            }
        }
        Resolved::DiskStore { .. } => {
            crate::kernel::diskfs::list_store_raw(buf)
        }
    }
}

// ── vfs_mkdir ─────────────────────────────────────────────────────────────

pub unsafe fn vfs_mkdir(path: &str) -> i64 {
    match resolve(path) {
        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
            unsafe { crate::kernel::fat::mkdir(fat_path) }
        }
        Resolved::Ext2  { .. }       => -1, // read-only
        Resolved::Dev   { .. }       => -1, // EPERM
        Resolved::Proc  { .. }       => -1, // EPERM
        Resolved::DiskStore { .. }   => -1, // EPERM
        Resolved::RamFS { path } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                Some(fs) => fs.create_dir(path).map(|_| 0i64).unwrap_or(-1),
                None     => -2,
            }
        }
    }
}

// ── vfs_chdir ─────────────────────────────────────────────────────────────

pub unsafe fn vfs_chdir(path: &str) -> i64 {
    use crate::kernel::scheduler::{SCHED, CURRENT_TASK_IDX, CWD_MAX};

    let exists = match resolve(path) {
        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
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
        Resolved::Dev       { .. } => false,
        Resolved::Proc { path: rpath } => {
            rpath == "/proc"
            || unsafe { crate::kernel::fs::ramfs::RAMFS.get() }
                   .and_then(|fs| fs.list_dir(rpath))
                   .is_some()
        }
        Resolved::DiskStore { path: dpath } => {
            dpath == "/store" || dpath == "/store/"
        }
    };

    if !exists { return -7; }

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

    unsafe {
        let sched = &raw mut SCHED;
        let idx   = CURRENT_TASK_IDX;
        let task  = &raw mut (*sched).tasks[idx];
        core::ptr::copy_nonoverlapping(norm.as_ptr(), (*task).cwd.as_mut_ptr(), final_len);
        (*task).cwd_len = final_len;
    }
    0
}

// ── vfs_stat helpers ──────────────────────────────────────────────────────

pub const S_IFREG: u32 = 0o100000;
pub const S_IFDIR: u32 = 0o040000;
pub const S_IFCHR: u32 = 0o020000;
pub const S_IFBLK: u32 = 0o060000;
pub const S_IFIFO: u32 = 0o010000;

#[repr(u32)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum StatKind { File = 0, Directory = 1, Device = 2 }

#[repr(C)]
pub struct FileStat { pub size: u64, pub kind: u32, pub _pad: u32 }

#[repr(C)]
pub struct LinuxStat {
    pub st_dev: u64, pub st_ino: u64, pub st_nlink: u64,
    pub st_mode: u32, pub st_uid: u32, pub st_gid: u32, pub __pad0: u32,
    pub st_rdev: u64, pub st_size: i64, pub st_blksize: i64, pub st_blocks: i64,
    pub st_atime: i64, pub st_atime_ns: i64,
    pub st_mtime: i64, pub st_mtime_ns: i64,
    pub st_ctime: i64, pub st_ctime_ns: i64,
    pub __unused: [i64; 3],
}

impl LinuxStat {
    pub fn zeroed() -> Self { unsafe { core::mem::zeroed() } }
    pub fn fill_file(size: u64, ino: u64) -> Self {
        let mut s = Self::zeroed();
        s.st_dev = 1; s.st_ino = ino; s.st_nlink = 1;
        s.st_mode = S_IFREG | 0o644; s.st_uid = 1000; s.st_gid = 1000;
        s.st_size = size as i64; s.st_blksize = 512;
        s.st_blocks = ((size + 511) / 512) as i64; s
    }
    pub fn fill_dir(ino: u64) -> Self {
        let mut s = Self::zeroed();
        s.st_dev = 1; s.st_ino = ino; s.st_nlink = 2;
        s.st_mode = S_IFDIR | 0o755; s.st_uid = 1000; s.st_gid = 1000;
        s.st_blksize = 512; s
    }
    pub fn fill_chardev(ino: u64) -> Self {
        let mut s = Self::zeroed();
        s.st_dev = 1; s.st_ino = ino; s.st_nlink = 1;
        s.st_mode = S_IFCHR | 0o666; s.st_uid = 1000; s.st_gid = 1000; s
    }
}

// ── vfs_stat_linux ────────────────────────────────────────────────────────

pub unsafe fn vfs_stat_linux(path: &str, out: *mut LinuxStat) -> i64 {
    unsafe { *out = LinuxStat::zeroed(); }
    match resolve(path) {
        Resolved::Dev { .. } => {
            unsafe { *out = LinuxStat::fill_chardev(1); }
            0
        }
        Resolved::Fat16 { fat_path } => {
            if !crate::kernel::ata::is_present() { return -19; }
            if fat_path == b"/disk" || fat_path == b"/disk/"
                || crate::kernel::fat::resolve_dir(fat_path).is_some()
            {
                unsafe { *out = LinuxStat::fill_dir(2); }
                return 0;
            }
            let fd = unsafe { crate::kernel::fat::open(fat_path, 0) };
            if fd < 0 { return -2; }
            let size = unsafe { crate::kernel::fat::file_size(fd as i32) } as u64;
            unsafe { crate::kernel::fat::close(fd as i32); }
            unsafe { *out = LinuxStat::fill_file(size, 100 + fat_path.len() as u64); }
            0
        }
        Resolved::Ext2 { path: ext2_path } => {
            if !crate::kernel::ext2::is_ready() { return -19; }
            if ext2_path == b"/ext2" || ext2_path == b"/ext2/"
                || crate::kernel::ext2::is_dir(ext2_path)
            {
                unsafe { *out = LinuxStat::fill_dir(4); }
                return 0;
            }
            let fd = unsafe { crate::kernel::ext2::open(ext2_path) };
            if fd < 0 { return -2; }
            let mut tmp = [0u8; 512]; let mut size = 0u64;
            loop {
                let n = unsafe { crate::kernel::ext2::read_fd(fd as i32, &mut tmp) };
                if n <= 0 { break; }
                size += n as u64;
            }
            unsafe { crate::kernel::ext2::close(fd as i32); }
            unsafe { *out = LinuxStat::fill_file(size, 200 + ext2_path.len() as u64); }
            0
        }
        Resolved::RamFS { path: rpath } => {
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                None => -2,
                Some(fs) => {
                    if let Some(data) = fs.read_file(rpath) {
                        unsafe { *out = LinuxStat::fill_file(data.len() as u64, 300 + rpath.len() as u64); }
                        0
                    } else if fs.list_dir(rpath).is_some() {
                        unsafe { *out = LinuxStat::fill_dir(400 + rpath.len() as u64); }
                        0
                    } else { -2 }
                }
            }
        }
        Resolved::Proc { path: rpath } => {
            crate::kernel::procfs::refresh(rpath);
            match unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
                None => -2,
                Some(fs) => {
                    if let Some(data) = fs.read_file(rpath) {
                        unsafe { *out = LinuxStat::fill_file(data.len() as u64, 500 + rpath.len() as u64); }
                        0
                    } else if fs.list_dir(rpath).is_some() {
                        unsafe { *out = LinuxStat::fill_dir(600 + rpath.len() as u64); }
                        0
                    } else { -2 }
                }
            }
        }
        Resolved::DiskStore { path: dpath } => {
            if dpath == "/store" || dpath == "/store/" {
                unsafe { *out = LinuxStat::fill_dir(700); }
                0
            } else {
                match crate::kernel::diskfs::parse_record_id(dpath) {
                    None     => -2,
                    Some(id) => {
                        let mut buf = [0u8; crate::kernel::disk_store::RECORD_DATA_MAX];
                        match unsafe { crate::kernel::disk_store::read_record(0, id, &mut buf) } {
                            None      => -2,
                            Some(len) => {
                                unsafe { *out = LinuxStat::fill_file(len as u64, 700 + id as u64); }
                                0
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── vfs_stat ──────────────────────────────────────────────────────────────

pub unsafe fn vfs_stat(path: &str, out: *mut FileStat) -> i64 {
    unsafe {
        match resolve(path) {
            Resolved::Dev { .. } => {
                (*out) = FileStat { size: 0, kind: StatKind::Device as u32, _pad: 0 };
                0
            }
            Resolved::Fat16 { fat_path } => {
                if !crate::kernel::ata::is_present() { return -19; }
                if fat_path == b"/disk" || fat_path == b"/disk/"
                    || crate::kernel::fat::resolve_dir(fat_path).is_some()
                {
                    (*out) = FileStat { size: 0, kind: StatKind::Directory as u32, _pad: 0 };
                    return 0;
                }
                let fd = crate::kernel::fat::open(fat_path, 0);
                if fd < 0 { return -7; }
                let size = crate::kernel::fat::file_size(fd as i32) as u64;
                crate::kernel::fat::close(fd as i32);
                (*out) = FileStat { size, kind: StatKind::File as u32, _pad: 0 };
                0
            }
            Resolved::Ext2 { path: ext2_path } => {
                if !crate::kernel::ext2::is_ready() { return -19; }
                if ext2_path == b"/ext2" || ext2_path == b"/ext2/"
                    || crate::kernel::ext2::is_dir(ext2_path)
                {
                    (*out) = FileStat { size: 0, kind: StatKind::Directory as u32, _pad: 0 };
                    return 0;
                }
                let fd = crate::kernel::ext2::open(ext2_path);
                if fd < 0 { return -7; }
                let mut tmp = [0u8; 512]; let mut size = 0u64;
                loop {
                    let n = crate::kernel::ext2::read_fd(fd as i32, &mut tmp);
                    if n <= 0 { break; }
                    size += n as u64;
                }
                crate::kernel::ext2::close(fd as i32);
                (*out) = FileStat { size, kind: StatKind::File as u32, _pad: 0 };
                0
            }
            Resolved::RamFS { path: rpath } => {
                match crate::kernel::fs::ramfs::RAMFS.get() {
                    None => -2,
                    Some(fs) => {
                        if let Some(data) = fs.read_file(rpath) {
                            (*out) = FileStat { size: data.len() as u64, kind: StatKind::File as u32, _pad: 0 };
                            0
                        } else if fs.list_dir(rpath).is_some() {
                            (*out) = FileStat { size: 0, kind: StatKind::Directory as u32, _pad: 0 };
                            0
                        } else { -7 }
                    }
                }
            }
            Resolved::Proc { path: rpath } => {
                crate::kernel::procfs::refresh(rpath);
                match crate::kernel::fs::ramfs::RAMFS.get() {
                    None => -2,
                    Some(fs) => {
                        if let Some(data) = fs.read_file(rpath) {
                            (*out) = FileStat { size: data.len() as u64, kind: StatKind::File as u32, _pad: 0 };
                            0
                        } else if fs.list_dir(rpath).is_some() {
                            (*out) = FileStat { size: 0, kind: StatKind::Directory as u32, _pad: 0 };
                            0
                        } else { -7 }
                    }
                }
            }
            Resolved::DiskStore { path: dpath } => {
                if dpath == "/store" || dpath == "/store/" {
                    (*out) = FileStat { size: 0, kind: StatKind::Directory as u32, _pad: 0 };
                    0
                } else {
                    match crate::kernel::diskfs::parse_record_id(dpath) {
                        None => -7,
                        Some(id) => {
                            let mut buf = [0u8; crate::kernel::disk_store::RECORD_DATA_MAX];
                            match crate::kernel::disk_store::read_record(0, id, &mut buf) {
                                None      => -7,
                                Some(len) => {
                                    (*out) = FileStat { size: len as u64, kind: StatKind::File as u32, _pad: 0 };
                                    0
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
