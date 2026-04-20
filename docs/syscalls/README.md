# Syscalls in OxideOS

This directory is dedicated to the design and implementation of system calls (syscalls) in OxideOS. It outlines the journey from a basic `int 0x80` handler to a full-fledged Linux-compatible syscall interface, treated as a staged kernel feature rather than a single, monolithic implementation.

## Current Stage

OxideOS has made significant progress, and the foundational steps for syscalls and user-mode execution are now robustly implemented:

-   A real `int 0x80` entry is correctly installed in the Interrupt Descriptor Table (IDT), allowing user-mode programs to trigger kernel services.
-   Syscall dispatch rules are cleanly separated into a shared core module (`syscall_core.rs`), promoting modularity and testability.
-   The syscall dispatch core is thoroughly covered by host-side unit tests, ensuring its correctness and reliability.
-   The kernel's GDT/TSS path is fully capable of transitioning the CPU into Ring 3 (user mode) to execute user code.
-   A tiny, hard-coded demo task successfully runs in user mode, utilizing its own mapped user code page and user stack, and gracefully returns control to the kernel via `sys_exit`. This demonstrates the complete user-mode entry and exit flow.
-   The `make -C kernel test-syscalls` command executes the current syscall test harness, verifying the integrity of the syscall mechanism.
-   While the `syscall`/`sysret` fast path for syscalls is present in the codebase, it is currently considered future work and is not the primary, supported ABI for user programs. The `int 0x80` mechanism is the current stable interface.
-   **Full Linux Syscall ABI Compatibility**: OxideOS now supports over 80 Linux x86-64 syscalls, enabling the execution of complex musl libc programs like Lua and BusyBox. This includes robust `fork`/`exec`/`waitpid` for process management, `mmap`/`munmap` for memory management, and a comprehensive VFS layer.

## Document Map

-   `docs/oxide_cocepts/04_syscalls_and_usermode.md`: Provides a detailed theoretical and practical explanation of privilege separation, the `int 0x80` syscall mechanism, and the journey to user mode, including a deep dive into the demo task.
-   `kernel/src/kernel/syscall_core.rs`: Defines the `Syscall` enum, the `SyscallRuntime` trait, and the `dispatch` function, which forms the core logic for handling syscall requests.
-   `kernel/src/kernel/syscall.rs`: Implements the `KernelRuntime` trait, wiring the abstract syscall definitions to concrete kernel services (e.g., serial port, timer, scheduler, filesystem).
-   `kernel/src/kernel/user_mode.rs`: Contains the low-level assembly trampolines and Rust functions for entering and exiting user mode, managing user task contexts, and launching the initial user program.
-   `userspace/oxide-rt/src/lib.rs`: Provides Rust wrappers for making syscalls from user-space programs, abstracting away the low-level assembly.

## Roadmap

The development of syscalls and user-mode features is an ongoing process. Here are the key milestones on the roadmap:

1.  **Refine `syscall`/`sysret` Fast Path**: Fully implement and integrate the `syscall`/`sysret` instructions for a faster syscall ABI, potentially making it the primary interface while retaining `int 0x80` for compatibility.
2.  **Advanced Process Management**: Implement more sophisticated process management features, including copy-on-write `fork`, robust signal handling, and job control.
3.  **Virtual Memory Enhancements**: Expand virtual memory capabilities with features like demand paging, memory protection flags, and more flexible `mmap` options.
4.  **Inter-Process Communication (IPC)**: Develop more advanced IPC mechanisms beyond basic message queues, such as shared memory segments with proper synchronization.
5.  **Robust User-Space Testing**: Develop a comprehensive suite of end-to-end user-space tests that run inside QEMU, covering all syscalls and core utilities.
6.  **Security Hardening**: Continuously review and improve security measures, including stricter pointer validation, privilege separation, and exploit mitigation techniques.
