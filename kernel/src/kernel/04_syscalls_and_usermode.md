# 4. System Calls & User Mode

A fundamental responsibility of an operating system is to run untrusted user code in a restricted environment (user mode) while providing a secure way for that code to request services from the kernel (system calls). This document explains the architecture of privilege separation and system calls in OxideOS.

---

## The Need for Privilege Separation

We cannot allow user applications to have direct access to hardware. If a program could write to any memory address or I/O port, it could easily crash the entire system, steal data from other programs, or corrupt the kernel itself.

The CPU enforces this separation through **privilege levels** or "rings". OxideOS uses two levels:

*   **Ring 0 (Kernel Mode)**: The most privileged level. The OS kernel runs here, with full access to all hardware and memory.
*   **Ring 3 (User Mode)**: A restricted level. User applications run here. Any attempt to perform a privileged operation (like disabling interrupts or accessing a hardware device) from Ring 3 will cause a fault, trapping back to the kernel.

## System Calls: The Bridge Between Rings

If user code can't access hardware, how does it do anything useful, like writing to the screen or reading from a file? It must ask the kernel to do it on its behalf. This formal request mechanism is called a **system call** (or syscall).

### The `int 0x80` Syscall ABI

OxideOS uses the traditional `int 0x80` (interrupt vector 128) mechanism for system calls. While modern CPUs have faster instructions like `syscall`/`sysret`, the interrupt-based method is simpler to implement initially and serves the same purpose.

The **Application Binary Interface (ABI)** defines the contract for how user code makes a syscall:

1.  The desired syscall is identified by a number, which is placed in the `RAX` register.
2.  Up to five arguments for the syscall are placed in registers `RDI`, `RSI`, `RDX`, `R10`, and `R8`.
3.  The user code executes the `int 0x80` instruction.
4.  The kernel performs the operation and places the return value back into `RAX`.

### The Full Syscall Flow: From User to Kernel and Back

1.  **Trap**: The user application executes `int 0x80`. This is a "trap"—a deliberate, software-triggered interrupt.
2.  **Privilege Check**: The CPU consults the Interrupt Descriptor Table (IDT). The entry for vector 128 has its privilege level (DPL) set to 3, explicitly allowing this call from user mode.
3.  **Stack Switch**: The CPU automatically and securely switches from the user stack to the kernel's trusted stack, using the `RSP0` pointer from the Task State Segment (TSS). This is a critical security step.
4.  **State Save**: The CPU pushes the user application's state (including its instruction pointer, stack pointer, and flags) onto the new kernel stack.
5.  **Jump to Handler**: The CPU jumps to the kernel's interrupt handler for vector 128, which is `handle_syscall` in `kernel/src/kernel/syscall.rs`. The CPU is now in Ring 0.
6.  **Dispatch**: The kernel's handler reads the syscall number from the `RAX` register (which was saved on the stack) and dispatches to the appropriate kernel function.
7.  **Pointer Validation**: **This is a non-negotiable security check.** If a syscall argument is a pointer (e.g., a buffer for a `write` call), the kernel *must* validate it before use. It checks that the pointer points to memory in the user-space range, not into kernel memory. Dereferencing an unvalidated user pointer is a classic and severe security vulnerability.
8.  **Execution**: The kernel performs the requested service (e.g., writes the buffer's contents to the console).
9.  **Return**: The kernel places the return value in the `RAX` register's location on the stack.
10. **`iretq`**: The kernel executes the `iretq` (Interrupt Return) instruction. This is the magic instruction that reverses the process. It pops the saved state off the kernel stack, switching the CPU back to Ring 3, restoring the user stack, and resuming the user application right after the `int 0x80` instruction.

### The User Mode Demo

OxideOS does not yet have a full process loader. Instead, to test the syscall mechanism, it includes a simple demo in `user_mode::run_demo()`. This function demonstrates a manual privilege transition:

1.  It allocates two pages of memory for the user task: one for code and one for the stack.
2.  It injects a tiny piece of pre-compiled machine code into the code page. This code's only job is to load the `sys_exit` syscall number into `RAX` and execute `int 0x80`.
3.  It manually constructs a stack frame that looks like the state the CPU would save during a hardware interrupt.
4.  It executes `iretq`. This pops the crafted stack frame, causing the CPU to "return" to the injected user code in Ring 3.

The user code runs, triggers the `sys_exit` syscall, and the kernel's handler safely terminates the task. This proves that the entire round-trip from kernel to user and back to kernel via a syscall is working correctly.