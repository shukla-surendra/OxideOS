// src/kernel/fs/ramfs.rs
//! In-memory filesystem (RamFS) for OxideOS.
//!
//! Stores files and directories as a flat Vec<INode>.  Each inode knows its
//! parent inode index so the tree can be walked without a hash-map.
//!
//! The global singleton `RAMFS` is an `UnsafeCell<Option<RamFs>>` that is
//! initialised once by `RAMFS.init()` after the heap allocator is ready.

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;

use super::{
    O_WRONLY, O_RDWR, O_CREAT, O_TRUNC, O_APPEND,
    ENOENT, EEXIST, EISDIR, ENOTDIR, EBADF, EINVAL, EMFILE, EACCES,
};

// ── Constants ──────────────────────────────────────────────────────────────
/// Maximum simultaneously open file descriptors (0–2 are stdin/stdout/stderr).
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
}

// ── File-descriptor entry ─────────────────────────────────────────────────
#[derive(Clone, Copy)]
struct FdEntry {
    inode_idx: usize,
    offset:    usize,
    writable:  bool,
    append:    bool,
}

// ── RamFs ─────────────────────────────────────────────────────────────────
pub struct RamFs {
    pub inodes: Vec<INode>,
    fd_table:   [Option<FdEntry>; MAX_FD],
}

impl RamFs {
    /// Build an empty filesystem and pre-populate standard directories/files.
    pub fn new() -> Self {
        let mut fs = Self {
            inodes:   Vec::new(),
            fd_table: [None; MAX_FD],
        };

        // inode 0 = root directory
        fs.inodes.push(INode {
            name:       String::from("/"),
            parent_idx: ROOT_PARENT,
            kind:       NodeKind::Directory,
            data:       Vec::new(),
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
        });
        Ok(idx)
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
    pub fn remove_file(&mut self, path: &str) -> Result<(), i64> {
        let idx = self.resolve(path).ok_or(ENOENT)?;
        if self.inodes[idx].kind == NodeKind::Directory { return Err(EISDIR); }

        // Close any FDs that point to this inode
        for fd in self.fd_table.iter_mut() {
            if let Some(e) = fd { if e.inode_idx == idx { *fd = None; } }
        }
        // Adjust all inode references > idx
        for inode in self.inodes.iter_mut() {
            if inode.parent_idx != ROOT_PARENT && inode.parent_idx > idx {
                inode.parent_idx -= 1;
            }
        }
        for fd in self.fd_table.iter_mut() {
            if let Some(e) = fd { if e.inode_idx > idx { e.inode_idx -= 1; } }
        }
        self.inodes.remove(idx);
        Ok(())
    }

    /// Returns `true` if the path exists.
    pub fn exists(&self, path: &str) -> bool { self.resolve(path).is_some() }

    /// Returns `true` if the path exists and is a directory.
    pub fn is_dir(&self, path: &str) -> bool {
        self.resolve(path).map(|i| self.inodes[i].kind == NodeKind::Directory).unwrap_or(false)
    }

    // ── File-descriptor API (used by syscalls) ────────────────────────────

    fn alloc_fd(&self) -> Option<usize> {
        // Reserve 0=stdin, 1=stdout, 2=stderr
        (3..MAX_FD).find(|&i| self.fd_table[i].is_none())
    }

    /// Open a path and return a file descriptor.
    pub fn open(&mut self, path: &str, flags: u32) -> i64 {
        let writable = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);
        let create   = flags & O_CREAT  != 0;
        let truncate = flags & O_TRUNC  != 0;
        let append   = flags & O_APPEND != 0;

        let inode_idx = if let Some(idx) = self.resolve(path) {
            if truncate && self.inodes[idx].kind == NodeKind::File {
                self.inodes[idx].data.clear();
            }
            idx
        } else if create {
            match self.create_file(path) {
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

        self.fd_table[fd] = Some(FdEntry {
            inode_idx,
            offset:   if append { self.inodes[inode_idx].data.len() } else { 0 },
            writable,
            append,
        });

        fd as i64
    }

    /// Close a file descriptor.
    pub fn close(&mut self, fd: i32) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        if self.fd_table[fd as usize].take().is_none() { return EBADF; }
        0
    }

    /// Read up to `buf.len()` bytes from `fd`.
    pub fn read_fd(&mut self, fd: i32, buf: &mut [u8]) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        let entry = match self.fd_table[fd as usize] {
            Some(e) => e,
            None    => return EBADF,
        };
        let inode = &self.inodes[entry.inode_idx];
        if inode.kind != NodeKind::File { return EISDIR; }

        let available = inode.data.len().saturating_sub(entry.offset);
        if available == 0 { return 0; } // EOF

        let n = available.min(buf.len());
        buf[..n].copy_from_slice(&inode.data[entry.offset..entry.offset + n]);
        if let Some(e) = &mut self.fd_table[fd as usize] { e.offset += n; }
        n as i64
    }

    /// Write `buf` to `fd`.
    pub fn write_fd(&mut self, fd: i32, buf: &[u8]) -> i64 {
        if fd < 0 || fd as usize >= MAX_FD { return EBADF; }
        let entry = match self.fd_table[fd as usize] {
            Some(e) => e,
            None    => return EBADF,
        };
        if !entry.writable { return EACCES; }

        let inode = &mut self.inodes[entry.inode_idx];
        if inode.kind != NodeKind::File { return EISDIR; }

        let offset = entry.offset;
        if offset >= inode.data.len() {
            inode.data.extend_from_slice(buf);
        } else {
            let end = offset + buf.len();
            if end > inode.data.len() { inode.data.resize(end, 0); }
            inode.data[offset..end].copy_from_slice(buf);
        }
        if let Some(e) = &mut self.fd_table[fd as usize] { e.offset += buf.len(); }
        buf.len() as i64
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
