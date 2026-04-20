# Chapter 4: System Calls & The Journey to User Mode

A fundamental responsibility of an operating system is to run untrusted user code in a restricted environment (user mode) while providing a secure way for that code to request services from the kernel (system calls). This document explains the architecture of privilege separation and system calls in OxideOS.

---

## The Need for Privilege Separation

Imagine the operating system kernel is the manager of a busy restaurant. The user applications (your web browser, a game, a text editor) are the customers. We cannot allow customers to just walk into the kitchen, use the ovens, and grab ingredients from the fridge. This would lead to chaos, stolen food, and potential fires.

In computing, the "kitchen" is the computer's hardware: the CPU, memory, hard disks, and network card. If any application could directly access this hardware, it could easily crash the entire system, steal data from other programs, or corrupt the kernel itself. This is why we need a system of rules and privileges.

The CPU enforces this separation through **privilege levels** or "rings". OxideOS uses two levels:

*   **Ring 0 (Kernel Mode)**: This is the "Manager Mode." The OS kernel runs here, with full, unrestricted access to all hardware and memory. It's the most privileged and trusted part of the system.

*   **Ring 3 (User Mode)**: This is "Customer Mode." User applications run here. It is a restricted, untrusted environment. Any attempt to perform a privileged operation (like talking directly to the hard disk or modifying another program's memory) from Ring 3 will cause a CPU exception, called a **General Protection Fault**. This fault immediately transfers control back to the kernel (the manager) to handle the misbehaving application, usually by terminating it.

## System Calls: The Bridge Between Rings

If a customer can't go into the kitchen, how do they get food? They place an order from a menu. In an OS, if user code can't access hardware, how does it do anything useful like writing to the screen or reading from a file? It must ask the kernel to do it on its behalf. This formal request mechanism is called a **system call** (or syscall). The list of available syscalls is the "menu" of services the kernel offers to applications.

### The `int 0x80` Syscall ABI

OxideOS uses the traditional `int 0x80` (interrupt vector 128) mechanism for system calls. While modern CPUs have faster instructions like `syscall`/`sysret`, the interrupt-based method is simpler to implement initially and serves the same purpose.

The **Application Binary Interface (ABI)** defines the contract for how user code makes a syscall:

1.  **Syscall Number in `RAX`**: The user program places the unique number of the desired syscall into the `RAX` CPU register. For example, `sys_exit` might be 60.
2.  **Arguments in other registers**: Arguments for the syscall (e.g., what to write, where to write it) are placed in a specific sequence of other registers: `RDI`, `RSI`, `RDX`, `R10`, `R8`.
3.  **Trigger the Interrupt**: The user code executes the `int 0x80` instruction. `int` stands for "interrupt," and this instruction tells the CPU to stop what it's doing and jump to the kernel's special interrupt handler for vector `0x80` (128).
4.  **Return Value in `RAX`**: After the kernel finishes the request, it places the result (e.g., number of bytes written, or an error code) back into the `RAX` register for the user program to inspect.

#### Assembly Code Example

Here is what a simple "exit" syscall looks like in x86_64 assembly:
```assembly
; We want to call sys_exit(0)
mov rax, 60   ; Syscall number for exit is 60
mov rdi, 0    ; The exit code is 0
int 0x80      ; Trigger the syscall
```

### The Full Syscall Flow: From User to Kernel and Back

1.  **Trap**: The user application executes `int 0x80`. This is a "trap"—a deliberate, software-triggered interrupt.
2.  **Privilege Check**: The CPU consults the Interrupt Descriptor Table (IDT). The entry for vector 128 has its privilege level (DPL) set to 3, explicitly allowing this call from user mode.
3.  **Stack Switch**: The CPU automatically and securely switches from the user's stack to the kernel's trusted stack. The address of this trusted stack is pre-loaded by the kernel into a special structure called the Task State Segment (TSS). This is a critical security step: the kernel cannot trust the user's stack, which might be invalid or maliciously crafted to attack the kernel.
4.  **State Save**: The CPU pushes the user application's state (including its instruction pointer, stack pointer, and flags) onto the new kernel stack.
5.  **Jump to Handler**: The CPU jumps to the kernel's interrupt handler for vector 128, which is `handle_syscall` in `kernel/src/kernel/syscall.rs`. The CPU is now in Ring 0.
6.  **Dispatch**: The kernel's handler reads the syscall number from the `RAX` register (which was saved on the stack) and dispatches to the appropriate kernel function.
7.  **Pointer Validation**: **This is a non-negotiable security check.** If a syscall argument is a pointer (e.g., a buffer for a `write` call), the kernel *must* validate it before use. It checks that the pointer points to memory in the user-space range (e.g., below `0x0000_8000_0000_0000`), not into the kernel's own memory. Dereferencing an unvalidated user pointer is a classic and severe security vulnerability that could allow an attacker to read or write kernel data.
8.  **Execution**: The kernel performs the requested service (e.g., writes the buffer's contents to the console).
9.  **Return**: The kernel places the return value in the `RAX` register's location on the stack.
10. **`iretq`**: The kernel executes the `iretq` (Interrupt Return) instruction. This is the magic instruction that reverses the process. It pops the saved state off the kernel stack, switching the CPU back to Ring 3, restoring the user stack, and resuming the user application right after the `int 0x80` instruction.

### Code Deep Dive: The First Journey to User Mode

OxideOS does not yet load programs from a disk. Instead, to test the entire privilege transition and syscall mechanism, it includes a simple demo in `user_mode::run_demo()`. This function is a perfect case study of how an OS manually performs the privilege transition.

1.  **Allocate Memory**: The function first calls `paging_allocator::map_user_region` to allocate and map memory for the user task's code and stack. This creates a safe, isolated sandbox in the virtual address space.

2.  **Inject Machine Code**: It then copies a tiny, pre-compiled program into the user code page. Let's decode the `USER_PROGRAM` from `kernel/src/kernel/user_mode.rs`:

    ```rust
    const USER_PROGRAM: [u8; 23] = [
        // mov rax, 40 (syscall: print "hello")
        0x48, 0xC7, 0xC0, 0x28, 0x00, 0x00, 0x00,
        // int 0x80
        0xCD, 0x80,
        // mov rdi, rax (use return value as arg)
        0x48, 0x89, 0xC7,
        // mov rax, 0 (syscall: sys_exit)
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,
        // int 0x80
        0xCD, 0x80,
        // jmp . (loop forever if exit fails)
        0xEB, 0xFE,
    ];
    ```
    This tiny program first calls a custom syscall (number 40) to print a message, then it calls `sys_exit` (syscall number 0 in this old demo) to terminate itself.

3.  **Prepare for `iretq`**: The most complex part is preparing the CPU to jump to user mode. This is done by the assembly code in `enter_user_mode_trampoline`. This code manually constructs a special stack frame that the `iretq` instruction expects. It pushes five key values onto the stack in a specific order:
    *   `ss`: The **Stack Segment** selector for user mode.
    *   `rsp`: The initial **Stack Pointer** for the user program.
    *   `rflags`: The CPU flags register, with the interrupt flag (`IF`) enabled so the program isn't deaf to hardware events.
    *   `cs`: The **Code Segment** selector for user mode.
    *   `rip`: The **Instruction Pointer**—the address of our `USER_PROGRAM` code.

4.  **Execute `iretq`**: The final instruction in the trampoline is `iretq`. This powerful instruction does everything at once: it pops all five values from the stack into their corresponding CPU registers and, most importantly, **changes the CPU's privilege level from Ring 0 to Ring 3**.

The user code runs, triggers the `sys_exit` syscall, and the kernel's handler safely terminates the task. This proves that the entire round-trip from kernel to user and back to kernel via a syscall is working correctly.