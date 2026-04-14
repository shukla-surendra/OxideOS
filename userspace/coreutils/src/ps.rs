//! ps — show running processes
//! Reads /proc/status (future) or uses GetSystemInfo syscall for basic info.
#![no_std]
#![no_main]

use oxide_rt::{exit, getpid, print_str};

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    print_str("  PID  NAME\n");
    print_str("  ---  ----\n");

    let my_pid = getpid();
    // For now just show current PID. Full ps would require a ListTasks syscall.
    // That's a future enhancement (Phase 3.3 partial).
    print_str("  ");
    print_u32(my_pid);
    print_str("  ps (self)\n");

    exit(0);
}

fn print_u32(mut n: u32) {
    if n == 0 {
        oxide_rt::print_str("0");
        return;
    }
    let mut buf = [0u8; 10];
    let mut i = 10usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    oxide_rt::print_bytes(&buf[i..]);
}
