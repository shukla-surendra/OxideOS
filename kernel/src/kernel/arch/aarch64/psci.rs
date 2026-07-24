//! PSCI power control (aarch64 counterpart of drivers/shutdown).
//!
//! QEMU virt without EL2/EL3 exposes PSCI through the HVC conduit.  If the
//! call somehow returns (or the conduit is wrong and traps), the exception
//! handler or the fallback park loop below takes over — either way the
//! machine stops.

use core::arch::asm;

use crate::kernel::arch::cpu;
use crate::kernel::serial::SERIAL_PORT;

const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;

fn psci_call(function: u64) {
    unsafe {
        asm!(
            "hvc #0",
            in("x0") function,
            in("x1") 0u64,
            in("x2") 0u64,
            in("x3") 0u64,
            options(nomem, nostack),
        );
    }
}

pub fn poweroff() -> ! {
    unsafe { SERIAL_PORT.write_str("PSCI SYSTEM_OFF...\n"); }
    psci_call(PSCI_SYSTEM_OFF);
    cpu::halt_forever()
}

pub fn reboot() -> ! {
    unsafe { SERIAL_PORT.write_str("PSCI SYSTEM_RESET...\n"); }
    psci_call(PSCI_SYSTEM_RESET);
    cpu::halt_forever()
}
