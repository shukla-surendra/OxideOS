//! Hardware drivers for OxideOS.
//!
//! Each submodule owns a single hardware interface:
//!   serial   — UART serial port (COM1)
//!   pic      — 8259A Programmable Interrupt Controller
//!   timer    — 8253/8254 Programmable Interval Timer
//!   keyboard — keyboard decode pipeline (cross-arch) + 8042 PS/2 front end (x86)
//!   mouse    — PS/2 mouse packet decoder + cursor state (cross-arch)
//!   ata      — ATA/IDE disk controller
//!   shutdown — ACPI power management
//!   net/     — network subsystem (PCI, NIC drivers, IP stack)
//!
//! Everything except `keyboard` and `mouse` is built on x86_64 only; the aarch64 port
//! supplies its own serial/timer/rtc/shutdown in `arch/aarch64/` and feeds
//! the shared keyboard decoder from virtio-input.

#[cfg(target_arch = "x86_64")]
pub mod serial;
#[cfg(target_arch = "x86_64")]
pub mod pic;
#[cfg(target_arch = "x86_64")]
pub mod timer;
#[cfg(target_arch = "x86_64")]
pub mod rtc;
pub mod keyboard;
pub mod mouse;
#[cfg(target_arch = "x86_64")]
pub mod ata;
#[cfg(target_arch = "x86_64")]
pub mod disk_store;
#[cfg(target_arch = "x86_64")]
pub mod shutdown;
#[cfg(target_arch = "x86_64")]
pub mod net;
