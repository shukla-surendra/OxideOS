// at top of src/kernel/mod.rs or src/kernel/interrupts.rs (before any `extern` declarations)
// #![feature(global_asm)]
// core::arch::global_asm!(include_str!("interrupt_stubs.s"));

pub mod interrupts_asm;
pub mod interrupts;
pub mod serial;
pub mod loggers;
pub mod fb_console;
pub mod idt;
pub mod pic;
pub mod ports;
pub mod timer;