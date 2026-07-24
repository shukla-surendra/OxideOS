//! Architecture-specific code.
//!
//! Layout
//! ──────
//!   arch/cpu.rs      Portable CPU-control facade (irq on/off, halt, spin hint).
//!                    Shared code must use this instead of raw `asm!`.
//!   arch/x86_64/     GDT, IDT, PIC-era interrupt handlers, syscall stubs.
//!   arch/aarch64/    EL1 exception vectors, GIC, generic timer (ARM port).
//!
//! Each architecture module re-exports its submodules here under stable
//! names so the rest of the kernel can say `crate::kernel::arch::interrupts`
//! without caring which architecture is being built.

pub mod cpu;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::{gdt, idt, interrupts, interrupts_asm};

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
