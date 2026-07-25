// ── Category modules ──────────────────────────────────────────────────────────
// Subsystems full of x86 port I/O and inline asm are compiled only on x86_64;
// the aarch64 port re-enables them one feature at a time (see docs/arm/).
pub mod drivers;  // serial, pic, timer, keyboard (cross-arch), ata, shutdown, net/
pub mod arch;     // cpu facade + per-architecture code (x86_64/, aarch64/)
#[cfg(target_arch = "x86_64")]
pub mod mem;      // paging_allocator
#[cfg(target_arch = "x86_64")]
pub mod fs;       // ramfs, fat, ext2, mbr, vfs, procfs
#[cfg(target_arch = "x86_64")]
pub mod proc;     // scheduler, elf_loader, user_mode, programs, env, tty
#[cfg(target_arch = "x86_64")]
pub mod ipc;      // ipc, pipe, shm, stdin
#[cfg(target_arch = "x86_64")]
pub mod sys;      // syscall_core, syscall, syscall_handler
#[cfg(target_arch = "x86_64")]
pub mod gui;      // compositor, gui_proc

// ── Remaining root files ──────────────────────────────────────────────────────
pub mod loggers;
#[cfg(target_arch = "x86_64")]
pub mod installer;

// ─────────────────────────────────────────────────────────────────────────────
// Re-export every submodule at the old flat path so all existing imports
// (crate::kernel::serial, crate::kernel::scheduler, etc.) keep working.
// ─────────────────────────────────────────────────────────────────────────────

// drivers/
#[cfg(target_arch = "x86_64")]
pub use drivers::serial;
#[cfg(target_arch = "x86_64")]
pub use drivers::pic;
#[cfg(target_arch = "x86_64")]
pub use drivers::timer;
#[cfg(target_arch = "x86_64")]
pub use drivers::rtc;
pub use drivers::keyboard;
#[cfg(target_arch = "x86_64")]
pub use drivers::ata;
#[cfg(target_arch = "x86_64")]
pub use drivers::disk_store;
#[cfg(target_arch = "x86_64")]
pub use drivers::shutdown;
#[cfg(target_arch = "x86_64")]
pub use drivers::net;

// arch/ (portable facade is arch::cpu; the rest is per-architecture)
pub use arch::cpu;
#[cfg(target_arch = "x86_64")]
pub use arch::gdt;
#[cfg(target_arch = "x86_64")]
pub use arch::idt;
#[cfg(target_arch = "x86_64")]
pub use arch::interrupts;
#[cfg(target_arch = "x86_64")]
pub use arch::interrupts_asm;

// ── aarch64 flat-path wiring ─────────────────────────────────────────────────
// Real aarch64 drivers (PL011, generic timer, PL031, PSCI) and API-compatible
// stubs (see stubs.rs) take the same flat names the x86 modules re-export, so
// the GUI layer and shared code compile unchanged.
#[cfg(target_arch = "aarch64")]
pub mod stubs;

#[cfg(target_arch = "aarch64")]
pub use arch::aarch64::serial;
#[cfg(target_arch = "aarch64")]
pub use arch::aarch64::timer;
#[cfg(target_arch = "aarch64")]
pub use arch::aarch64::rtc;
#[cfg(target_arch = "aarch64")]
pub use arch::aarch64::psci as shutdown;

#[cfg(target_arch = "aarch64")]
pub use stubs::{
    ata, compositor, disk_store, diskfs, ext2, fat, fs, gui_proc, interrupts,
    mbr, net, pipe, programs, scheduler, stdin, syscall, user_mode,
};

// mem/
#[cfg(target_arch = "x86_64")]
pub use mem::paging_allocator;

// fs/ (individual submodules)
#[cfg(target_arch = "x86_64")]
pub use fs::fat;
#[cfg(target_arch = "x86_64")]
pub use fs::ext2;
#[cfg(target_arch = "x86_64")]
pub use fs::mbr;
#[cfg(target_arch = "x86_64")]
pub use fs::vfs;
#[cfg(target_arch = "x86_64")]
pub use fs::procfs;
#[cfg(target_arch = "x86_64")]
pub use fs::diskfs;

// proc/
#[cfg(target_arch = "x86_64")]
pub use proc::scheduler;
#[cfg(target_arch = "x86_64")]
pub use proc::elf_loader;
#[cfg(target_arch = "x86_64")]
pub use proc::user_mode;
#[cfg(target_arch = "x86_64")]
pub use proc::programs;
#[cfg(target_arch = "x86_64")]
pub use proc::env;
#[cfg(target_arch = "x86_64")]
pub use proc::tty;

// ipc/ (ipc::Message etc. are re-exported at the ipc module level via ipc/mod.rs)
#[cfg(target_arch = "x86_64")]
pub use ipc::pipe;
#[cfg(target_arch = "x86_64")]
pub use ipc::shm;
#[cfg(target_arch = "x86_64")]
pub use ipc::stdin;

// sys/
#[cfg(target_arch = "x86_64")]
pub use sys::syscall_core;
#[cfg(target_arch = "x86_64")]
pub use sys::syscall;
#[cfg(target_arch = "x86_64")]
pub use sys::syscall_handler;

// gui/ (kernel-side)
#[cfg(target_arch = "x86_64")]
pub use gui::compositor;
#[cfg(target_arch = "x86_64")]
pub use gui::gui_proc;
