//! forktest — exercise fork() copy-on-write semantics.
//!
//! Forks, has the child overwrite a shared static, and checks that the
//! parent's view of that memory is unaffected (private COW copy made on
//! the child's write fault).
#![no_std]
#![no_main]

use oxide_rt::{println, fork, waitpid, exit, getpid};

static mut SHARED: u64 = 111;

#[unsafe(no_mangle)]
pub extern "C" fn oxide_main() {
    let shared = &raw mut SHARED;
    unsafe {
        println!("forktest: pid={} before fork, SHARED={}", getpid(), *shared);

        let pid = fork();
        if pid == 0 {
            *shared = 222;
            println!("forktest: child pid={} wrote SHARED={}", getpid(), *shared);
            exit(0);
        } else if pid > 0 {
            let status = waitpid(pid as u32);
            let value = *shared;
            println!("forktest: parent pid={} after child exit (code {}), SHARED={}", getpid(), status, value);
            if value == 111 {
                println!("forktest: PASS - parent's copy unchanged (COW worked)");
            } else {
                println!("forktest: FAIL - parent's copy was modified to {}", value);
            }
        } else {
            println!("forktest: fork failed ({})", pid);
        }
    }

    exit(0);
}
