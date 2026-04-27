//! Disk filesystem visibility layer.
//!
//! `populate()` is called once at boot to create the visible mount-point
//! directories in RamFS so that `ls /` shows them.  It also creates a
//! `/diskinfo` file with a summary of detected disks.
//!
//! `refresh_store(record_id)` is called by vfs_open whenever a file inside
//! `/store/` is opened; it reads the record from disk and writes a temporary
//! RamFS file so normal read syscalls work on it.

extern crate alloc;
use alloc::vec::Vec;

fn push_str(v: &mut Vec<u8>, s: &str) { v.extend_from_slice(s.as_bytes()); }
fn push_u64(v: &mut Vec<u8>, mut n: u64) {
    if n == 0 { v.push(b'0'); return; }
    let mut buf = [0u8; 20]; let mut i = 20;
    while n > 0 { i -= 1; buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    v.extend_from_slice(&buf[i..]);
}
fn push_u32(v: &mut Vec<u8>, n: u32) { push_u64(v, n as u64); }

// ── Boot population ───────────────────────────────────────────────────────

pub fn populate() {
    let Some(fs) = (unsafe { crate::kernel::fs::ramfs::RAMFS.get() }) else { return };

    // ── Visible mount-point directories ──────────────────────────────────
    // These show up in `ls /` even when the real filesystem is not yet mounted.
    let _ = fs.create_dir("/disk");    // FAT16 on primary disk
    let _ = fs.create_dir("/ext2");    // ext2  on secondary disk
    let _ = fs.create_dir("/store");   // disk record store (disk 0)

    // ── /diskinfo — human-readable summary ───────────────────────────────
    let mut info: Vec<u8> = Vec::new();
    push_str(&mut info, "OxideOS Disk Summary\n");
    push_str(&mut info, "====================\n\n");

    push_str(&mut info, "ATA Drives:\n");
    let labels = ["disk0 (primary master)",  "disk1 (primary slave)",
                  "disk2 (secondary master)","disk3 (secondary slave)"];
    let mounts = ["/disk", "(no mount)", "/ext2", "(no mount)"];
    for i in 0..4usize {
        push_str(&mut info, "  ");
        push_str(&mut info, labels[i]);
        push_str(&mut info, ": ");
        if crate::kernel::ata::is_present_at(i) {
            if let Some((secs, slave, lba48)) = crate::kernel::ata::disk_info(i) {
                push_u64(&mut info, secs / 2048);
                push_str(&mut info, " MB");
                if lba48 { push_str(&mut info, " LBA48"); }
                push_str(&mut info, "  mount=");
                push_str(&mut info, mounts[i]);
            }
        } else {
            push_str(&mut info, "not present");
        }
        push_str(&mut info, "\n");
    }

    push_str(&mut info, "\nDisk Record Store (/store):\n");
    push_str(&mut info, "  Disk 0: ");
    if crate::kernel::disk_store::is_mounted(0) {
        let mut ids = [0u32; 255];
        let n = unsafe { crate::kernel::disk_store::list_records(0, &mut ids) };
        push_u32(&mut info, n as u32);
        push_str(&mut info, " record(s)");
    } else {
        push_str(&mut info, "not mounted");
    }
    push_str(&mut info, "\n");

    push_str(&mut info, "\nMount Points:\n");
    push_str(&mut info, "  /        ramfs  (volatile, cleared on reboot)\n");
    push_str(&mut info, "  /disk    fat16  (persistent, primary disk)\n");
    push_str(&mut info, "  /ext2    ext2   (read-only, secondary disk)\n");
    push_str(&mut info, "  /store   oxds   (persistent record store, primary disk)\n");
    push_str(&mut info, "  /dev     devfs  (devices: null, tty)\n");
    push_str(&mut info, "  /proc    procfs (runtime info: uptime, meminfo, ...)\n");

    let _ = fs.write_file("/diskinfo", &info);

    // ── /store/<id> — pre-populate records from disk ─────────────────────
    refresh_all_records();
}

/// Re-read all records from disk 0 and create/update their RamFS shadow files.
pub fn refresh_all_records() {
    let Some(fs) = (unsafe { crate::kernel::fs::ramfs::RAMFS.get() }) else { return };
    if !crate::kernel::disk_store::is_mounted(0) { return; }

    let mut ids = [0u32; 255];
    let n = unsafe { crate::kernel::disk_store::list_records(0, &mut ids) };

    for &id in &ids[..n] {
        let path = record_path(id);
        let mut buf = [0u8; crate::kernel::disk_store::RECORD_DATA_MAX];
        if let Some(len) = unsafe { crate::kernel::disk_store::read_record(0, id, &mut buf) } {
            let _ = fs.write_file(&path, &buf[..len]);
        }
    }
}

/// Refresh a single record from disk into its RamFS shadow file.
/// Called by vfs_open on every `/store/ID` access.
pub fn refresh_record(record_id: u32) {
    let Some(fs) = (unsafe { crate::kernel::fs::ramfs::RAMFS.get() }) else { return };
    if !crate::kernel::disk_store::is_mounted(0) { return; }

    let path = record_path(record_id);
    let mut buf = [0u8; crate::kernel::disk_store::RECORD_DATA_MAX];
    if let Some(len) = unsafe { crate::kernel::disk_store::read_record(0, record_id, &mut buf) } {
        let _ = fs.write_file(&path, &buf[..len]);
    }
}

/// Write data for a record into both disk_store and RamFS.
pub fn write_record(record_id: u32, data: &[u8]) -> bool {
    let ok = unsafe { crate::kernel::disk_store::write_record(0, record_id, data) };
    if ok {
        if let Some(fs) = unsafe { crate::kernel::fs::ramfs::RAMFS.get() } {
            let path = record_path(record_id);
            let _ = fs.write_file(&path, data);
        }
    }
    ok
}

/// Build the RamFS path for a record: `/store/<id>`.
fn record_path(id: u32) -> alloc::string::String {
    extern crate alloc;
    let mut s = alloc::string::String::from("/store/");
    push_u32_to_str(&mut s, id);
    s
}

fn push_u32_to_str(s: &mut alloc::string::String, mut n: u32) {
    if n == 0 { s.push('0'); return; }
    let mut buf = [0u8; 10]; let mut i = 10;
    while n > 0 { i -= 1; buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    if let Ok(part) = core::str::from_utf8(&buf[i..]) { s.push_str(part); }
}

/// List all record IDs as a newline-delimited byte buffer (for vfs_readdir).
pub fn list_store_raw(buf: &mut [u8]) -> i64 {
    if !crate::kernel::disk_store::is_mounted(0) { return 0; }

    let mut ids = [0u32; 255];
    let n = unsafe { crate::kernel::disk_store::list_records(0, &mut ids) };

    let mut pos = 0usize;
    for &id in &ids[..n] {
        let mut tmp = [0u8; 12];
        let mut ti = 12usize;
        let mut v = id;
        if v == 0 { ti -= 1; tmp[ti] = b'0'; }
        else { while v > 0 { ti -= 1; tmp[ti] = b'0' + (v % 10) as u8; v /= 10; } }
        let s = &tmp[ti..];
        if pos + s.len() + 1 >= buf.len() { break; }
        buf[pos..pos + s.len()].copy_from_slice(s);
        pos += s.len();
        buf[pos] = b'\n';
        pos += 1;
    }
    pos as i64
}

/// Parse the record ID from a `/store/<id>` path.
pub fn parse_record_id(path: &str) -> Option<u32> {
    let suffix = path.strip_prefix("/store/")?;
    if suffix.is_empty() { return None; }
    let mut n = 0u32;
    for c in suffix.bytes() {
        if c < b'0' || c > b'9' { return None; }
        n = n.wrapping_mul(10).wrapping_add((c - b'0') as u32);
    }
    Some(n)
}
