#![no_std]
#![no_main]

use oxide_rt::{print_str, println, getchar, sleep_ms, exit};

const SYS_INSTALL_QUERY: u64 = 433;
const SYS_INSTALL_BEGIN: u64 = 434;

fn install_query() -> i64 {
    unsafe { oxide_rt::raw::syscall0(SYS_INSTALL_QUERY) }
}

fn install_begin() -> i64 {
    unsafe { oxide_rt::raw::syscall0(SYS_INSTALL_BEGIN) }
}

/// Blocking read of one character.
fn getchar_blocking() -> u8 {
    loop {
        if let Some(c) = getchar() { return c; }
        sleep_ms(10);
    }
}

/// Read a line from stdin (echo chars, handle backspace). Returns length.
fn read_line(buf: &mut [u8]) -> usize {
    let mut len = 0;
    loop {
        let b = getchar_blocking();
        if b == b'\n' || b == b'\r' {
            println!();
            break;
        }
        if b == 8 || b == 127 {
            if len > 0 {
                len -= 1;
                print_str("\x08 \x08");
            }
            continue;
        }
        if len < buf.len() - 1 {
            buf[len] = b;
            len += 1;
            let s = core::str::from_utf8(&buf[len-1..len]).unwrap_or("?");
            print_str(s);
        }
    }
    buf[len] = 0;
    len
}

fn fmt_mb(sectors: i64) -> (i64, i64) {
    let mb = sectors / 2048;
    (mb, sectors)
}

#[no_mangle]
pub extern "C" fn oxide_main() {
    println!();
    println!("╔═══════════════════════════════════════════╗");
    println!("║          OxideOS Installer  v0.1          ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();

    // Query the secondary disk
    let sectors = install_query();
    if sectors <= 0 {
        println!("ERROR: No secondary disk detected.");
        println!();
        println!("In VirtualBox/QEMU, attach a blank HDD as the");
        println!("second IDE disk (IDE secondary master) and");
        println!("reboot, then run /bin/install again.");
        exit(1);
    }

    let (mb, _) = fmt_mb(sectors);
    println!("Target disk: {} MB  ({} sectors)", mb, sectors);
    println!();
    println!("This will write OxideOS to the second disk:");
    println!("  Partition 1  (64 MB, FAT32) — boot partition");
    println!("  Partition 2  (64 MB, FAT16) — data partition");
    println!();
    println!("WARNING: ALL existing data on the target disk");
    println!("         will be permanently erased!");
    println!();
    print_str("Type YES to continue, anything else to abort: ");

    let mut line = [0u8; 16];
    let len = read_line(&mut line);

    if len != 3 || &line[..3] != b"YES" {
        println!("Aborted. No changes made.");
        exit(0);
    }

    println!();
    println!("Installing OxideOS...");
    println!("  [1/4] Formatting EFI boot partition (FAT32)...");

    let result = install_begin();

    if result < 0 {
        println!();
        println!("INSTALLATION FAILED  (error code: {})", result);
        println!();
        println!("The disk may be in an incomplete state.");
        println!("Do NOT boot from it. Try again or check");
        println!("the serial log for details.");
        exit(1);
    }

    println!("  [2/4] Formatting data partition (FAT16)...");
    println!("  [3/4] Writing Limine + kernel to boot partition...");
    println!("  [4/4] Writing MBR partition table...");
    println!();
    println!("╔═══════════════════════════════════════════╗");
    println!("║     Installation complete!                ║");
    println!("╚═══════════════════════════════════════════╝");
    println!();
    println!("Next steps:");
    println!("  1. Shut down this VM");
    println!("  2. Remove the OxideOS ISO/CD from the VM settings");
    println!("  3. Ensure the target disk is set as the primary boot device");
    println!("  4. Start the VM — OxideOS will boot from disk");
    println!();

    exit(0);
}
