# Chapter 2: Interrupts & CPU Setup - The Kernel's Control Center

After the kernel gains control from the bootloader, its first major task is to configure the CPU to handle events and enforce security. This is the purpose of the `init_interrupt_system()` function. This entire setup is performed with interrupts disabled (`cli`) to prevent any events from occurring in a partially configured state.

---

### GDT and TSS (Global Descriptor Table & Task State Segment)

*   **Concept: The GDT (Global Descriptor Table)** is a fundamental data structure that tells the CPU about different memory segments. While its role has diminished in 64-bit mode (where paging is dominant), it's still essential for defining privilege levels and setting up the Task State Segment. OxideOS uses the GDT to define two main privilege levels, often called "rings":
    *   **Ring 0 (Kernel Mode)**: This is the most privileged level. The OS kernel runs here, with full, unrestricted access to all hardware, memory, and CPU instructions. It's the "god mode" of the system.
    *   **Ring 3 (User Mode)**: This is a restricted level. User applications (like your web browser or a game) run here. Code in Ring 3 has limited access to hardware and cannot directly execute privileged instructions. Any attempt to do so will result in a CPU fault, which the kernel then handles. This separation is crucial for system stability and security.

*   **Concept: The TSS (Task State Segment)** is another vital structure, though its name can be a bit misleading in 64-bit mode. It doesn't manage entire tasks as it once did. Instead, its most important job in modern OSes is to provide the CPU with the address of the **kernel's trusted stack**. Specifically, it holds a pointer called `RSP0` (Ring 0 Stack Pointer). When the CPU transitions from a lower privilege level (like Ring 3) to a higher one (like Ring 0) due to an interrupt or system call, it *automatically* switches to the stack pointed to by `RSP0`. This is a critical security feature: the kernel can never trust a user program's stack, which could be maliciously crafted. By switching to its own clean stack, the kernel protects itself from user-space attacks.

*   **Code Flow**: The `gdt::init()` function, typically called early in `kmain`, is responsible for creating and loading a new GDT and TSS. This establishes the fundamental privilege foundation for the entire operating system.

### IDT (Interrupt Descriptor Table)

*   **Concept: The IDT (Interrupt Descriptor Table)** is like the CPU's "event handler lookup table." It's a table containing up to 256 entries, each corresponding to a unique "vector number" (0-255). When an interrupt or exception occurs, the CPU uses this vector number as an index into the IDT to find the address of the specific code (an **Interrupt Service Routine, or ISR**) that should handle that event.

*   **Types of Events Handled by the IDT**:
    1.  **CPU Exceptions (Vectors 0-31)**: These are internal CPU events, usually errors. Examples include a "Divide by Zero" error (vector 0), a "Page Fault" (vector 14) when a program tries to access memory it shouldn't, or a "General Protection Fault" (vector 13) for other privilege violations. The kernel must have handlers for these to prevent crashes.
    2.  **Hardware Interrupts (IRQs)**: These are signals from external hardware devices. Think of your keyboard sending an interrupt when you press a key, the mouse moving, or the timer chip sending a periodic "tick." These are typically mapped to vectors 32 and above after PIC remapping.
    3.  **Software Interrupts**: These are interrupts intentionally triggered by software using instructions like `int`. They are often used for system calls, providing a controlled way for user programs to request services from the kernel.

*   **The Syscall Gate (Vector 128 / `0x80`)**: OxideOS configures **Vector 128 (0x80)** as its primary system call entry point. The IDT entry for this vector is set with a special flag called **Descriptor Privilege Level (DPL) of 3**. This is extremely important because it explicitly allows code running in Ring 3 (user mode) to trigger this specific interrupt. This creates a secure, controlled "gate" through which user programs can request kernel services without gaining full kernel privileges.

*   **Code Flow**: The `idt::init()` function is responsible for populating all 256 entries of the IDT. It points each vector to its corresponding Interrupt Service Routine (ISR), which is the assembly code that first receives control when an interrupt occurs.

### PIC (Programmable Interrupt Controller)

*   **Concept: The PIC (Programmable Interrupt Controller)**, specifically the Intel 8259A, is a legacy hardware chip that manages hardware interrupts (IRQs). Think of it as a traffic cop for hardware signals. Since a CPU has a limited number of physical interrupt lines, the PIC acts as a multiplexer, gathering interrupt requests from multiple devices (like the timer, keyboard, disk controller, etc.) and feeding them to the CPU one at a time, based on priority.

*   **The Remapping Problem**: By default, the PIC is hardwired to map hardware IRQs (0-15) to CPU interrupt vectors 8-23. This is a huge problem! These vectors (8-23) are already reserved by the CPU for its own critical exceptions (like "Double Fault" or "Invalid Opcode"). If we didn't change this, a timer interrupt (IRQ 0) would trigger the same handler as a "Divide by Zero" exception, leading to chaos.

*   **The Solution: PIC Remapping**: Every robust operating system must **remap the PIC**. OxideOS performs this crucial step by sending a specific sequence of commands to the PIC's I/O ports. This reconfigures the PIC to map its IRQs to a safe, unused range of interrupt vectors, typically starting at **Vector 32**.
    *   IRQ 0 (Timer) → Vector 32
    *   IRQ 1 (Keyboard) → Vector 33
    *   ...and so on.
    This ensures that hardware interrupts have their own dedicated handlers in the IDT, separate from CPU exceptions.

*   **Code Flow**: The `pic::init()` function orchestrates this complex remapping sequence by writing specific byte commands to the PIC's command and data ports.

### Timer Interrupt

*   **Concept: The Timer Interrupt** is the "heartbeat" of the operating system. It's generated by a dedicated hardware chip, the **Programmable Interval Timer (PIT)**, at a regular, configurable frequency. OxideOS configures the PIT to generate an interrupt (IRQ 0, which is remapped to Vector 32) at **100Hz** (100 times per second).

*   **Importance: Preemptive Multitasking**: This interrupt is absolutely fundamental for **preemptive multitasking**. Imagine if a user program could run forever without the OS getting a chance to intervene. The system would freeze! By periodically interrupting any running code (whether it's kernel or user mode), the timer interrupt guarantees that the OS kernel regains control. This allows the kernel to:
    *   Increment a global "tick" counter, used for tracking time.
    *   Perform periodic tasks (like updating the GUI clock).
    *   Most importantly, trigger the **scheduler** to switch to another task, ensuring that no single program can monopolize the CPU.

*   **Code Flow**: `timer::init(100)` configures the PIT. The interrupt handler for Vector 32 (the remapped timer IRQ) is then responsible for processing each timer tick.

### EOI (End of Interrupt)

*   **Concept: EOI (End of Interrupt)** is a crucial command that the kernel *must* send back to the PIC after it has finished handling a hardware interrupt.

*   **Why it's Critical**: When the PIC sends an interrupt to the CPU, it typically holds that interrupt line "active" and won't send further interrupts of the same or lower priority until it receives an EOI signal. If the kernel forgets to send an EOI, the PIC will assume the interrupt is still being processed. This would effectively "freeze" the corresponding device: for example, if the timer handler didn't send an EOI, you'd get only one timer tick, and then the system clock would stop. Forgetting EOI for the keyboard would mean only the first key press is ever registered.

*   **Code Flow**: The EOI command (a byte value of `0x20`) is sent to the PIC's command port (I/O port `0x20` for the master PIC, and `0xA0` for the slave PIC if the interrupt came from there) at the very end of a hardware interrupt handler. This signals to the PIC that it can now process new interrupts.