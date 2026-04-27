//! procfs — virtual /proc filesystem for OxideOS.
//!
//! `populate()` is called once at boot to create the /proc directory tree in
//! RamFS and write the static files.  `refresh(path)` is called by vfs_open
//! every time a dynamic file is opened so its contents are up-to-date.

extern crate alloc;
use alloc::vec::Vec;

// ── tiny write-to-vec helpers ─────────────────────────────────────────────────

fn push_str(v: &mut Vec<u8>, s: &str) {
    v.extend_from_slice(s.as_bytes());
}

fn push_u64(v: &mut Vec<u8>, mut n: u64) {
    if n == 0 { v.push(b'0'); return; }
    let mut buf = [0u8; 20];
    let mut i = 20usize;
    while n > 0 { i -= 1; buf[i] = b'0' + (n % 10) as u8; n /= 10; }
    v.extend_from_slice(&buf[i..]);
}

// ── populate (called once at boot) ───────────────────────────────────────────

pub fn populate() {
    let Some(fs) = (unsafe { crate::kernel::fs::ramfs::RAMFS.get() }) else { return };

    let _ = fs.create_dir("/proc");

    // /proc/version — sourced from the central version module
    let _ = fs.write_file(
        "/proc/version",
        crate::version::PROC_VERSION.as_bytes(),
    );

    // /proc/cpuinfo — static (single CPU)
    let _ = fs.write_file(
        "/proc/cpuinfo",
        b"processor\t: 0\nvendor_id\t: GenuineIntel\ncpu family\t: 6\n\
model name\t: OxideOS Virtual CPU @ 100Hz\narch\t\t: x86_64\n",
    );

    // /proc/mounts — static snapshot of the mount table
    let _ = fs.write_file(
        "/proc/mounts",
        b"ramfs / ramfs rw 0 0\nfat16 /disk fat16 rw 0 0\next2 /ext2 ext2 rw 0 0\ndevfs /dev devfs rw 0 0\n",
    );

    // placeholder content for the dynamic files (will be refreshed on open)
    let _ = fs.write_file("/proc/uptime",  b"0.00 0.00\n");
    let _ = fs.write_file("/proc/meminfo", b"MemTotal: 0 kB\n");
}

// ── refresh (called on every vfs_open for /proc/* dynamic files) ─────────────

pub fn refresh(path: &str) {
    match path {
        "/proc/uptime"  => refresh_uptime(),
        "/proc/meminfo" => refresh_meminfo(),
        _ => {}
    }
}

fn refresh_uptime() {
    let ticks  = unsafe { crate::kernel::timer::get_ticks() };
    let secs   = ticks / 100;
    let frac   = (ticks % 100) / 10; // one decimal place

    let mut buf: Vec<u8> = Vec::new();
    push_u64(&mut buf, secs);
    buf.push(b'.');
    push_u64(&mut buf, frac);
    push_str(&mut buf, " 0.00\n");

    write_proc_file("/proc/uptime", &buf);
}

fn refresh_meminfo() {
    let (alloc_frames, total_frames) =
        crate::kernel::paging_allocator::frame_stats();

    let total_kb = (total_frames * 4096 / 1024) as u64;
    let used_kb  = (alloc_frames * 4096 / 1024) as u64;
    let free_kb  = total_kb.saturating_sub(used_kb);

    let mut buf: Vec<u8> = Vec::new();
    push_str(&mut buf, "MemTotal:     "); push_u64(&mut buf, total_kb); push_str(&mut buf, " kB\n");
    push_str(&mut buf, "MemFree:      "); push_u64(&mut buf, free_kb);  push_str(&mut buf, " kB\n");
    push_str(&mut buf, "MemAvailable: "); push_u64(&mut buf, free_kb);  push_str(&mut buf, " kB\n");
    push_str(&mut buf, "MemUsed:      "); push_u64(&mut buf, used_kb);  push_str(&mut buf, " kB\n");

    write_proc_file("/proc/meminfo", &buf);
}

fn write_proc_file(path: &str, data: &[u8]) {
    let Some(fs) = (unsafe { crate::kernel::fs::ramfs::RAMFS.get() }) else { return };
    if let Some(idx) = fs.resolve(path) {
        fs.inodes[idx].data.clear();
        fs.inodes[idx].data.extend_from_slice(data);
    }
}
