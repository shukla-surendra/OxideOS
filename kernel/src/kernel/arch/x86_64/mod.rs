//! x86-64 architecture-specific code: GDT/TSS, IDT, interrupt and
//! exception handlers, and the hand-written interrupt entry stubs.

pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod interrupts_asm;
