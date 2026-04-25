// ── Category modules ──────────────────────────────────────────────────────────
pub mod drivers;  // serial, pic, timer, keyboard, ata, shutdown, net/
pub mod arch;     // gdt, idt, interrupts, interrupts_asm
pub mod mem;      // paging_allocator
pub mod fs;       // ramfs, fat, ext2, mbr, vfs, procfs
pub mod proc;     // scheduler, elf_loader, user_mode, programs, env, tty
pub mod ipc;      // ipc, pipe, shm, stdin
pub mod sys;      // syscall_core, syscall, syscall_handler
pub mod gui;      // compositor, gui_proc

// ── Remaining root files ──────────────────────────────────────────────────────
pub mod loggers;
pub mod installer;

// ─────────────────────────────────────────────────────────────────────────────
// Re-export every submodule at the old flat path so all existing imports
// (crate::kernel::serial, crate::kernel::scheduler, etc.) keep working.
// ─────────────────────────────────────────────────────────────────────────────

// drivers/
pub use drivers::serial;
pub use drivers::pic;
pub use drivers::timer;
pub use drivers::keyboard;
pub use drivers::ata;
pub use drivers::shutdown;
pub use drivers::net;

// arch/
pub use arch::gdt;
pub use arch::idt;
pub use arch::interrupts;
pub use arch::interrupts_asm;

// mem/
pub use mem::paging_allocator;

// fs/ (individual submodules)
pub use fs::fat;
pub use fs::ext2;
pub use fs::mbr;
pub use fs::vfs;
pub use fs::procfs;

// proc/
pub use proc::scheduler;
pub use proc::elf_loader;
pub use proc::user_mode;
pub use proc::programs;
pub use proc::env;
pub use proc::tty;

// ipc/ (ipc::Message etc. are re-exported at the ipc module level via ipc/mod.rs)
pub use ipc::pipe;
pub use ipc::shm;
pub use ipc::stdin;

// sys/
pub use sys::syscall_core;
pub use sys::syscall;
pub use sys::syscall_handler;

// gui/ (kernel-side)
pub use gui::compositor;
pub use gui::gui_proc;
