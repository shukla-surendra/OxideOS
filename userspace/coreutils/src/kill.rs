//! kill — send a signal to a process
//! Usage: kill [-<signal>] <pid>
//! Default signal: SIGTERM (15)
#![no_std]
#![no_main]

use oxide_rt::{exit, kill_signal, write, arg, argc};

const STDOUT: i32 = 1;

fn parse_u32(s: &str) -> Option<u32> {
    let mut n = 0u32;
    let mut has_digit = false;
    for c in s.bytes() {
        if c.is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((c - b'0') as u32);
            has_digit = true;
        } else {
            return None;
        }
    }
    if has_digit { Some(n) } else { None }
}

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    if argc() < 2 {
        let _ = write(STDOUT, b"Usage: kill [-<signal>] <pid>\n");
        exit(1);
    }

    let mut sig  = 15u32; // SIGTERM
    let mut pidx = 1usize;

    if let Some(a) = arg(1) {
        if a.starts_with('-') {
            if let Some(s) = parse_u32(&a[1..]) { sig = s; }
            pidx = 2;
        }
    }

    if pidx >= argc() {
        let _ = write(STDOUT, b"kill: missing pid\n");
        exit(1);
    }

    for i in pidx..argc() {
        if let Some(p) = arg(i) {
            match parse_u32(p) {
                Some(pid) => { let _ = kill_signal(pid, sig); }
                None => {
                    let _ = write(STDOUT, b"kill: invalid pid: ");
                    let _ = write(STDOUT, p.as_bytes());
                    let _ = write(STDOUT, b"\n");
                }
            }
        }
    }
    exit(0);
}
