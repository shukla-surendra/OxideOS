//! AArch64 (ARMv8-A) architecture-specific code.
//!
//! Port status — implemented feature by feature, mirroring `arch/x86_64/`:
//!   [x] serial.rs       PL011 UART (QEMU virt)        (x86: drivers/serial)
//!   [x] exceptions.rs   EL1 vector table + handlers   (x86: idt + interrupts)
//!   [x] timer.rs        ARM generic timer, polled     (x86: PIT in drivers/timer)
//!   [x] rtc.rs          PL031 real-time clock         (x86: CMOS in drivers/rtc)
//!   [x] psci.rs         PSCI power off / reset        (x86: drivers/shutdown)
//!   [ ] gic.rs          GICv2 interrupt controller    (x86: pic)
//!
//! See docs/arm/ for the full porting plan and design notes.

pub mod exceptions;
pub mod psci;
pub mod rtc;
pub mod serial;
pub mod timer;
