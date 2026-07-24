//! AArch64 (ARMv8-A) architecture-specific code.
//!
//! Port status — implemented feature by feature, mirroring `arch/x86_64/`:
//!   [x] serial.rs       PL011 UART (QEMU virt)        (x86: drivers/serial)
//!   [x] exceptions.rs   EL1 vector table + handlers   (x86: idt + interrupts)
//!   [ ] gic.rs          GICv2 interrupt controller    (x86: pic)
//!   [ ] timer.rs        ARM generic timer             (x86: PIT in drivers/timer)
//!   [ ] psci.rs         PSCI power off / reset        (x86: drivers/shutdown)
//!
//! See docs/arm/ for the full porting plan and design notes.

pub mod exceptions;
pub mod serial;
