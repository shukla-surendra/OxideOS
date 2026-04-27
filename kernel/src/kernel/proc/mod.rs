//! Process management: scheduling, ELF loading, user mode, env, TTY.
pub mod scheduler;
pub mod elf_loader;
pub mod user_mode;
pub mod programs;
pub mod env;
pub mod tty;
