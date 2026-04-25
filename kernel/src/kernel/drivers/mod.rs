//! Hardware drivers for OxideOS.
//!
//! Each submodule owns a single hardware interface:
//!   serial   — UART serial port (COM1)
//!   pic      — 8259A Programmable Interrupt Controller
//!   timer    — 8253/8254 Programmable Interval Timer
//!   keyboard — PS/2 keyboard controller
//!   ata      — ATA/IDE disk controller
//!   shutdown — ACPI power management
//!   net/     — network subsystem (PCI, NIC drivers, IP stack)

pub mod serial;
pub mod pic;
pub mod timer;
pub mod keyboard;
pub mod ata;
pub mod shutdown;
pub mod net;
