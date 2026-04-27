//! Inter-process communication: IPC message queues, pipes, shared memory, stdin.

// Re-export everything from ipc.rs at this level so callers can still write
// `crate::kernel::ipc::Message`, `crate::kernel::ipc::msgq_create`, etc.
mod ipc;
pub use self::ipc::*;

pub mod pipe;
pub mod shm;
pub mod stdin;
