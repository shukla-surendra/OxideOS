// src/kernel/fs/ramfs.rs
//! In-memory filesystem (RamFS) for OxideOS.
//!
//! Stores files and directories as a flat Vec<INode>.  Each inode knows its
//! parent inode index so the tree can be walked without a hash-map.
//!
//! The global singleton `RAMFS` is an `UnsafeCell<Option<RamFs>>` that is
//! initialised once by `RAMFS.init()` after the heap allocator is ready.
//!
//! # Per-task FD table
//! `FdTable` is a Copy-able, const-constructible struct that each `Task` owns.
//! It holds up to `MAX_FD` open file descriptors for one process.
//! `RamFs` itself no longer owns any FD state.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;

use super::{
    O_WRONLY, O_RDWR, O_CREAT, O_TRUNC, O_APPEND,
    ENOENT, EEXIST, EISDIR, ENOTDIR, EBADF, EINVAL, EMFILE, EACCES,
};

// ── Constants ──────────────────────────────────────────────────────────────
/// Maximum simultaneously open file descriptors per task (FDs 0–2 = stdin/stdout/stderr).
pub const MAX_FD: usize = 32;
/// Sentinel: parent_idx of the root directory.
pub const ROOT_PARENT: usize = usize::MAX;

// ── Node kind ─────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    File,
    Directory,
}

// ── INode ─────────────────────────────────────────────────────────────────
pub struct INode {
    /// Filename (not full path).
    pub name: String,
    /// Index of the parent inode; `ROOT_PARENT` for the root directory.
    pub parent_idx: usize,
    pub kind: NodeKind,
    /// File content (always empty for directories).
    pub data: Vec<u8>,
    /// POSIX permission bits (e.g. 0o644 for rw-r--r--).
    pub mode: u16,
    /// Owner user ID.
    pub uid: u32,
    /// Owner group ID.
    pub gid: u32,
}

// ── FD backend tag ────────────────────────────────────────────────────────
/// Which underlying filesystem or subsystem backs an open file descriptor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FdBackend {
    /// In-memory RamFS file; `inode_idx` identifies the node.
    RamFS,
    /// FAT16 disk file; `raw_fd` is the internal FAT slot (FAT_FD_BASE + i).
    Fat16,
    /// ext2 disk file (read-only); `raw_fd` is the ext2 slot (EXT2_FD_BASE + i).
    Ext2,
    /// Anonymous pipe end; `raw_fd` is the pipe's raw fd, `writable` tells direction.
    Pipe,
    /// /dev/null — writes discard, reads return EOF.
    DevNull,
    /// /dev/tty  — reads come from stdin ring; writes go to console.
    DevTty,
}

// ── Per-task file-descriptor entry ────────────────────────────────────────
/// A single open-file record stored inside a task's `FdTable`.
#[derive(Clone, Copy)]
pub struct FdEntry {
    pub backend:   FdBackend,
    /// RamFS: inode index.
    pub inode_idx: usize,
    /// Fat16: internal FAT raw fd.  Pipe: raw pipe fd.
    pub raw_fd:    i32,
    pub offset:    usize,
    pub writable:  bool,
    pub append:    bool,
}

// ── Per-task FD table ──────────────────────────────────────────────────────
/// Owned by each `Task`.  Holds up to `MAX_FD` open RamFS file descriptors.
///
/// FDs 0/1/2 (stdin/stdout/stderr) are reserved — `alloc_fd` never returns
/// them.  Their semantics are handled in `syscall_core.rs`.
#[derive(Clone, Copy)]
pub struct FdTable {
    pub entries: [Option<FdEntry>; MAX_FD],
}

impl FdTable {
    /// Create an empty FD table (all slots vacant).
    pub const fn new() -> Self {
        Self { entries: [None; MAX_FD] }
    }

    /// Find the lowest free FD slot >= 3.
    fn alloc_fd(&self) -> Option<usize> {
        (3..MAX_FD).find(|&i| self.entries[i].is_none())
    }

    /// Open `path` on `fs`, returning the new FD number or a negative error.
    pub fn open(&mut self, fs: &mut RamFs, path: &str, flags: u32) -> i64 {
        let writable = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
        let create   = flags & O_CREAT  != 0;
        let truncate = flags & O_TRUNC  != 0;
        let append   = flags & O_APPEND != 0;

        let inode_idx = if let Some(idx) = fs.resolve(path) {
            if truncate && fs.inodes[idx].kind == NodeKind::File {
                fs.inodes[idx].data.clear();
            }
            idx
        } else if create {
            match fs.create_file(path) {
                Ok(idx) => idx,
                Err(_)  => return ENOENT,
            }
        } else {
            return ENOENT;
        };

        let fd = match self.alloc_fd() {
            Some(fd) => fd,
            None     => return EMFILE,
        };

        let initial_offset = if append { fs.inodes[inode_idx].data.len() } else { 0 };
        self.entries[fd] = Some(FdEntry {
            backend:   FdBackend::RamFS,
            inode_idx,
            raw_fd:    0,
            offset:    initial_offset,
            writable,
            append,
        });
        fd as i64
    }

    /// Allocate two FD slots backed by a raw pipe pair.  Returns `(read_slot, write_slot)`.
    pub fn open_pipe(&mut self, raw_read_fd: i32, raw_write_fd: i32) -> Option<(usize, usize)> {
        let rslot = self.alloc_fd()?;
        self.entries[rslot] = Some(FdEntry {
            backend: FdBackend::Pipe,
            inode_idx: 0, raw_fd: raw_read_fd, offset: 0, writable: false, append: false,
        });
        let wslot = self.alloc_fd()?;
        self.entries[wslot] = Some(FdEntry {
            backend: FdBackend::Pipe,
            inode_idx: 0, raw_fd: raw_write_fd, offset: 0, writable: true, append: false,
        });
        Some((rslot, wslot))
    }

    /// Allocate one FD slot backed by an internal FAT16 raw fd.
    pub fn open_fat(&mut self, fat_raw_fd: i32, writable: bool) -> i64 {
        match self.alloc_fd() {
            None   => -24, // EMFILE
            Some(fd) => {
                self.entries[fd] = Some(FdEntry {
                    backend: FdBackend::Fat16,
                    inode_idx: 0, raw_fd: fat_raw_fd, offset: 0, writable, append: false,
                });
                fd as i64
            }
        }
    }

    /// Allocate one FD slot backed by an ext2 raw fd (read-only).
    pub fn open_ext2(&mut self, ext2_raw_fd: i32) -> i64 {
        match self.alloc_fd() {
            None     => -24, // EMFILE
            Some(fd) => {
                self.entries[fd] = Some(FdEntry {
                    backend: FdBackend::Ext2,
                    inode_idx: 0, raw_fd: ext2_raw_fd, offset: 0,
                    writable: false, append: false,
                });
                fd as i64
            }
        }
    }

    /// Allocate one FD slot for a /dev file.
    pub fn open_dev(&mut self, backend: FdBackend) -> i64 {
        match self.alloc_fd() {
            None   => -24, // EMFILE
            Some(fd) => {
                self.entries[fd] = Some(FdEntry {
                    backend,
                    inode_idx: 0, raw_fd: 0, offset: 0,
                    writable: backend != FdBackend::DevNull, // DevNull and DevTty both accept writes
                    append: false,
                });
                fd as i64
            }
        }
    }

    /// Close `fd`.  Runs backend-specific cleanup.
    pub fn close(&mut self, fd: i32) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        match self.entries[fd as usize].take() {
            None    => EBADF,
            Some(e) => {
                match e.backend {
                    FdBackend::Pipe  => unsafe { crate::kernel::pipe::close(e.raw_fd); }
                    FdBackend::Fat16 => unsafe { crate::kernel::fat::close(e.raw_fd); }
                    FdBackend::Ext2  => unsafe { crate::kernel::ext2::close(e.raw_fd); }
                    _ => {}
                }
                0
            }
        }
    }

    /// Read up to `buf.len()` bytes from `fd`.
    pub fn read_fd(&mut self, fs: &RamFs, fd: i32, buf: &mut [u8]) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        let entry = match self.entries[fd as usize] {
            Some(e) => e,
            None    => return EBADF,
        };
        match entry.backend {
            FdBackend::Pipe => {
                return unsafe { crate::kernel::pipe::read(entry.raw_fd, buf) };
            }
            FdBackend::Fat16 => {
                return unsafe { crate::kernel::fat::read_fd(entry.raw_fd, buf) };
            }
            FdBackend::Ext2 => {
                return unsafe { crate::kernel::ext2::read_fd(entry.raw_fd, buf) };
            }
            FdBackend::DevNull => return 0,
            FdBackend::DevTty  => {
                // One character at a time from the stdin ring.
                if buf.is_empty() { return 0; }
                return match crate::kernel::stdin::pop() {
                    Some(ch) => { buf[0] = ch; 1 }
                    None     => -6, // EAGAIN
                };
            }
            FdBackend::RamFS => {}
        }
        if entry.inode_idx >= fs.inodes.len() { return EBADF; }
        let inode = &fs.inodes[entry.inode_idx];
        if inode.kind != NodeKind::File { return EISDIR; }

        let available = inode.data.len().saturating_sub(entry.offset);
        if available == 0 { return 0; } // EOF

        let n = available.min(buf.len());
        buf[..n].copy_from_slice(&inode.data[entry.offset..entry.offset + n]);
        if let Some(e) = &mut self.entries[fd as usize] { e.offset += n; }
        n as i64
    }

    /// Write `buf` to `fd`.
    pub fn write_fd(&mut self, fs: &mut RamFs, fd: i32, buf: &[u8]) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        let entry = match self.entries[fd as usize] {
            Some(e) => e,
            None    => return EBADF,
        };
        match entry.backend {
            FdBackend::Pipe  => {
                return unsafe { crate::kernel::pipe::write(entry.raw_fd, buf) };
            }
            FdBackend::Fat16 => {
                return unsafe { crate::kernel::fat::write_fd(entry.raw_fd, buf) };
            }
            FdBackend::Ext2 => {
                return EACCES; // ext2 is read-only
            }
            FdBackend::DevNull | FdBackend::DevTty => {
                // DevNull discards; DevTty mirrors to the output capture path
                if entry.backend == FdBackend::DevTty {
                    crate::kernel::user_mode::output_write(buf);
                }
                return buf.len() as i64;
            }
            FdBackend::RamFS => {}
        }
        if !entry.writable { return EACCES; }
        if entry.inode_idx >= fs.inodes.len() { return EBADF; }

        let inode = &mut fs.inodes[entry.inode_idx];
        if inode.kind != NodeKind::File { return EISDIR; }

        let offset = entry.offset;
        if offset >= inode.data.len() {
            inode.data.extend_from_slice(buf);
        } else {
            let end = offset + buf.len();
            if end > inode.data.len() { inode.data.resize(end, 0); }
            inode.data[offset..end].copy_from_slice(buf);
        }
        if let Some(e) = &mut self.entries[fd as usize] { e.offset += buf.len(); }
        buf.len() as i64
    }

    /// Duplicate `old_fd` to `new_fd`.  Returns `new_fd` or negative error.
    /// `new_fd` 0–2 are allowed so that stdout/stdin can be redirected.
    pub fn dup2(&mut self, old_fd: i32, new_fd: i32) -> i64 {
        if old_fd < 0 || old_fd as usize >= MAX_FD { return -5; }
        if new_fd < 0 || new_fd as usize >= MAX_FD { return -5; }
        match self.entries[old_fd as usize] {
            None    => -5,
            Some(e) => {
                // Addref the resource being duplicated.
                match e.backend {
                    FdBackend::Pipe  => unsafe { crate::kernel::pipe::addref(e.raw_fd); }
                    _ => {}
                }
                // Close whatever is currently at new_fd.
                if let Some(old) = self.entries[new_fd as usize] {
                    match old.backend {
                        FdBackend::Pipe  => unsafe { crate::kernel::pipe::close(old.raw_fd); }
                        FdBackend::Fat16 => unsafe { crate::kernel::fat::close(old.raw_fd); }
                        _ => {}
                    }
                }
                self.entries[new_fd as usize] = Some(e);
                new_fd as i64
            }
        }
    }

    /// After `RamFs::remove_file` removes the inode at `removed_idx`, fix up
    /// all open RamFS FD entries so indices above the gap are decremented.
    /// FDs pointing directly at the removed inode are closed (set to None).
    pub fn on_inode_removed(&mut self, removed_idx: usize) {
        for slot in self.entries.iter_mut() {
            if let Some(e) = slot {
                if e.backend != FdBackend::RamFS { continue; }
                if e.inode_idx == removed_idx {
                    *slot = None;
                } else if e.inode_idx > removed_idx {
                    e.inode_idx -= 1;
                }
            }
        }
    }
}

// ── RamFs ─────────────────────────────────────────────────────────────────
/// The filesystem tree.  Owns only inodes — FD state lives in each task's
/// `FdTable` so it is naturally per-process.
pub struct RamFs {
    pub inodes: Vec<INode>,
}

impl RamFs {
    /// Build an empty filesystem and pre-populate standard directories/files.
    pub fn new() -> Self {
        let mut fs = Self { inodes: Vec::new() };

        // inode 0 = root directory
        fs.inodes.push(INode {
            name:       String::from("/"),
            parent_idx: ROOT_PARENT,
            kind:       NodeKind::Directory,
            data:       Vec::new(),
            mode:       0o755,
            uid:        0,
            gid:        0,
        });

        // Standard directories
        let _ = fs.create_dir("/etc");
        let _ = fs.create_dir("/tmp");
        let _ = fs.create_dir("/home");
        let _ = fs.create_dir("/bin");

        // Pre-populated files
        let _ = fs.write_file("/etc/hostname", b"oxideos\n");
        let _ = fs.write_file("/etc/version",  b"OxideOS 0.1.0 - Hobby Kernel\n");
        let _ = fs.write_file(
            "/etc/motd",
            b"Welcome to OxideOS!\nType 'help' in the terminal for commands.\n",
        );

        fs
    }

    // ── Path helpers ──────────────────────────────────────────────────────

    /// Resolve an absolute path to an inode index.
    pub fn resolve(&self, path: &str) -> Option<usize> {
        if path == "/" || path.is_empty() {
            return Some(0);
        }
        let stripped = if path.starts_with('/') { &path[1..] } else { path };
        let mut cur = 0usize;
        for component in stripped.split('/') {
            if component.is_empty() { continue; }
            cur = self.find_child(cur, component)?;
        }
        Some(cur)
    }

    fn find_child(&self, parent_idx: usize, name: &str) -> Option<usize> {
        self.inodes.iter().position(|n| n.parent_idx == parent_idx && n.name == name)
    }

    /// Split "/foo/bar" → ("/foo", "bar").  Returns None for root.
    fn split_path(path: &str) -> Option<(&str, &str)> {
        let path = path.trim_end_matches('/');
        if path.is_empty() || path == "/" { return None; }
        match path.rfind('/') {
            Some(0)   => Some(("/", &path[1..])),
            Some(pos) => Some((&path[..pos], &path[pos + 1..])),
            None      => Some(("/", path)),
        }
    }

    // ── Directory operations ──────────────────────────────────────────────

    /// Create a directory.  Returns `EEXIST` if path already exists.
    pub fn create_dir(&mut self, path: &str) -> Result<usize, i64> {
        if self.resolve(path).is_some() { return Err(EEXIST); }
        let (parent_path, name) = Self::split_path(path).ok_or(EINVAL)?;
        let parent_idx = self.resolve(parent_path).ok_or(ENOENT)?;
        if self.inodes[parent_idx].kind != NodeKind::Directory { return Err(ENOTDIR); }
        let idx = self.inodes.len();
        self.inodes.push(INode {
            name:       String::from(name),
            parent_idx,
            kind:       NodeKind::Directory,
            data:       Vec::new(),
            mode:       0o755,
            uid:        0,
            gid:        0,
        });
        Ok(idx)
    }

    /// Write directory entries for `path` into `buf` as `<name>\n` (file)
    /// or `<name>/\n` (directory).  Returns bytes written, or negative error.
    pub fn read_dir_raw(&self, path: &str, buf: &mut [u8]) -> i64 {
        let dir_idx = match self.resolve(path) {
            Some(i) => i,
            None    => return -7, // ENOENT
        };
        if self.inodes[dir_idx].kind != NodeKind::Directory { return -1; } // EINVAL
        let mut pos = 0usize;
        for node in self.inodes.iter().filter(|n| n.parent_idx == dir_idx) {
            let name  = node.name.as_bytes();
            let trail = if node.kind == NodeKind::Directory { b"/" as &[u8] } else { b"" };
            let need  = name.len() + trail.len() + 1; // +1 for '\n'
            if pos + need > buf.len() { break; }
            buf[pos..pos + name.len()].copy_from_slice(name);
            pos += name.len();
            if !trail.is_empty() { buf[pos] = b'/'; pos += 1; }
            buf[pos] = b'\n'; pos += 1;
        }
        pos as i64
    }

    /// List a directory.  Returns `None` if path does not exist or is a file.
    pub fn list_dir(&self, path: &str) -> Option<Vec<(String, NodeKind)>> {
        let dir_idx = self.resolve(path)?;
        if self.inodes[dir_idx].kind != NodeKind::Directory { return None; }
        let entries = self.inodes.iter()
            .filter(|n| n.parent_idx == dir_idx)
            .map(|n| (n.name.clone(), n.kind))
            .collect();
        Some(entries)
    }

    // ── File operations ───────────────────────────────────────────────────

    /// Create or truncate a file.  Returns the inode index.
    pub fn create_file(&mut self, path: &str) -> Result<usize, i64> {
        if let Some(idx) = self.resolve(path) {
            if self.inodes[idx].kind == NodeKind::Directory { return Err(EISDIR); }
            self.inodes[idx].data.clear();
            return Ok(idx);
        }
        let (parent_path, name) = Self::split_path(path).ok_or(EINVAL)?;
        let parent_idx = self.resolve(parent_path).ok_or(ENOENT)?;
        if self.inodes[parent_idx].kind != NodeKind::Directory { return Err(ENOTDIR); }
        let idx = self.inodes.len();
        self.inodes.push(INode {
            name:       String::from(name),
            parent_idx,
            kind:       NodeKind::File,
            data:       Vec::new(),
            mode:       0o644,
            uid:        0,
            gid:        0,
        });
        Ok(idx)
    }

    /// Write `data` to a file, creating it if it does not exist.
    pub fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), i64> {
        let idx = self.create_file(path)?;
        self.inodes[idx].data.extend_from_slice(data);
        Ok(())
    }

    /// Append `data` to an existing file (creates it if needed).
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<(), i64> {
        if let Some(idx) = self.resolve(path) {
            if self.inodes[idx].kind != NodeKind::File { return Err(EISDIR); }
            self.inodes[idx].data.extend_from_slice(data);
            return Ok(());
        }
        self.write_file(path, data)
    }

    /// Read the full content of a file.
    pub fn read_file(&self, path: &str) -> Option<&[u8]> {
        let idx = self.resolve(path)?;
        if self.inodes[idx].kind != NodeKind::File { return None; }
        Some(&self.inodes[idx].data)
    }

    /// File size in bytes, or None if the path doesn't exist / is a dir.
    pub fn file_size(&self, path: &str) -> Option<usize> {
        let idx = self.resolve(path)?;
        if self.inodes[idx].kind != NodeKind::File { return None; }
        Some(self.inodes[idx].data.len())
    }

    /// Remove a file.  Directories must be removed with `remove_dir`.
    /// Returns the inode index of the removed entry so callers can call
    /// `FdTable::on_inode_removed(idx)` on all live tasks.
    pub fn remove_file(&mut self, path: &str) -> Result<usize, i64> {
        let idx = self.resolve(path).ok_or(ENOENT)?;
        if self.inodes[idx].kind == NodeKind::Directory { return Err(EISDIR); }

        // Shift all inode parent references that are above the removed index.
        for inode in self.inodes.iter_mut() {
            if inode.parent_idx != ROOT_PARENT && inode.parent_idx > idx {
                inode.parent_idx -= 1;
            }
        }
        self.inodes.remove(idx);
        Ok(idx)
    }

    /// Rename a file from `old_path` to `new_path`.
    /// Moves the file to a different directory if needed.
    pub fn rename(&mut self, old_path: &str, new_path: &str) -> Result<(), i64> {
        // Validate source.
        let _ = self.resolve(old_path).ok_or(ENOENT)?;
        if self.inodes[self.resolve(old_path).unwrap()].kind == NodeKind::Directory {
            return Err(EISDIR);
        }

        // If destination already exists, remove it first.
        if self.resolve(new_path).is_some() {
            let _ = self.remove_file(new_path)?;
        }

        // Re-resolve source (index may have changed after removal).
        let idx = self.resolve(old_path).ok_or(ENOENT)?;

        // Parse new path into (parent_dir, name).
        let (new_dir, new_name) = Self::split_path(new_path).ok_or(EINVAL)?;
        let new_parent = if new_dir == "/" || new_dir.is_empty() {
            0usize // root
        } else {
            self.resolve(new_dir).ok_or(ENOENT)?
        };

        // Update the inode in-place.
        self.inodes[idx].name  = String::from(new_name);
        self.inodes[idx].parent_idx = new_parent;
        Ok(())
    }

    /// Truncate a file to `length` bytes.
    pub fn truncate(&mut self, path: &str, length: usize) -> Result<(), i64> {
        let idx = self.resolve(path).ok_or(ENOENT)?;
        if self.inodes[idx].kind == NodeKind::Directory { return Err(EISDIR); }
        self.inodes[idx].data.truncate(length);
        if length > self.inodes[idx].data.len() {
            self.inodes[idx].data.resize(length, 0);
        }
        Ok(())
    }

    /// Truncate an already-open inode by index.
    pub fn truncate_by_idx(&mut self, inode_idx: usize, length: usize) {
        if inode_idx < self.inodes.len() {
            self.inodes[inode_idx].data.truncate(length);
            if length > self.inodes[inode_idx].data.len() {
                self.inodes[inode_idx].data.resize(length, 0);
            }
        }
    }

    /// Returns `true` if the path exists.
    pub fn exists(&self, path: &str) -> bool { self.resolve(path).is_some() }

    /// Returns `true` if the path exists and is a directory.
    pub fn is_dir(&self, path: &str) -> bool {
        self.resolve(path).map(|i| self.inodes[i].kind == NodeKind::Directory).unwrap_or(false)
    }
}

// ============================================================================
// GLOBAL SINGLETON
// ============================================================================

pub struct RamFsGlobal(UnsafeCell<Option<RamFs>>);

// Single-core hobby kernel – no concurrent access.
unsafe impl Sync for RamFsGlobal {}

impl RamFsGlobal {
    pub const fn new() -> Self { Self(UnsafeCell::new(None)) }

    /// Call once after the heap allocator is ready.
    pub unsafe fn init(&self) {
        *self.0.get() = Some(RamFs::new());
    }

    /// Get a mutable reference to the inner RamFs.
    pub unsafe fn get(&self) -> Option<&mut RamFs> {
        unsafe { (*self.0.get()).as_mut() }
    }
}

pub static RAMFS: RamFsGlobal = RamFsGlobal::new();
