# OxideOS: Technical Architecture & Execution Flow

This document provides a comprehensive technical overview of OxideOS, a 64-bit hobby operating system written in Rust. It details the system's architecture, execution flow from boot to user-space, and the core OS concepts implemented so far.

---

## 1. The Boot Process (Limine)

When the computer starts, the BIOS/UEFI firmware initializes the hardware and hands control to a **bootloader**. OxideOS uses the **Limine Bootloader**.

### Limine's Role

Limine is responsible for preparing a standardized environment for the kernel. Its key tasks include:

1.  **Entering 64-bit Long Mode:** It configures the CPU to operate in 64-bit mode, which is essential for a modern OS.
2.  **Setting Up Initial Page Tables:** It creates a basic virtual memory layout, typically identity-mapping the first few megabytes of physical memory and mapping the kernel into a "higher-half" of the virtual address space (e.g., starting at `0xFFFF800000000000`).
3.  **Gathering System Information:** It collects vital hardware information and passes it to the kernel via a structured request/response protocol. This includes:
    *   **Memory Map:** A list of all physical RAM regions, detailing which are usable, reserved, or unusable.
    *   **Framebuffer:** A pointer to a linear memory region that directly corresponds to the pixels on the screen.

### Kernel Entry Point

Once Limine's work is done, it jumps to the kernel's entry point: the `kmain()` function in `kernel/src/main.rs`.

**Code Flow (`kmain`):**
1.  **Serial Port Initialization:** The very first action is initializing the serial port (`SERIAL_PORT.init()`). This provides a crucial debugging channel before any graphics are available.
2.  **Limine Revision Check:** The kernel asserts that the bootloader's revision is supported (`BASE_REVISION.is_supported()`).

---

## 2. Core CPU & Interrupt Initialization

After the initial handshake with Limine, the kernel must configure the CPU to handle events and enforce memory protection. This is orchestrated by the `init_interrupt_system()` function. The entire process is wrapped in `cli` (disable interrupts) and `sti` (enable interrupts) to prevent race conditions.

### GDT and TSS (Global Descriptor Table & Task State Segment)

*   **Concept:** The GDT defines memory "segments" for the CPU, primarily to enforce privilege levels. OxideOS defines segments for:
    *   **Ring 0 (Kernel Mode):** The highest privilege level, where the OS kernel runs.
    *   **Ring 3 (User Mode):** A restricted level for running applications.
*   **TSS:** The Task State Segment is a special structure referenced by the GDT. Its most critical role in OxideOS is to store the **`RSP0` (Ring 0 Stack Pointer)**. When an interrupt or syscall occurs in user mode (Ring 3), the CPU automatically switches to the kernel's trusted stack by loading the address from `RSP0`. Without this, a malicious user program could corrupt the kernel.
*   **Code Flow:** `gdt::init()` is called to load the new GDT and TSS.

### IDT (Interrupt Descriptor Table)

*   **Concept:** The IDT is a table of 256 entries that tells the CPU where to jump when an interrupt occurs. Each entry points to an **Interrupt Service Routine (ISR)**.
*   **Code Flow:** `idt::init()` populates the IDT with handlers for CPU exceptions (e.g., Page Fault, Divide by Zero) and hardware interrupts.
*   **Syscall Gate:** A special entry, **Vector 128 (0x80)**, is configured with a **Descriptor Privilege Level (DPL) of 3**. This explicitly allows user-mode (Ring 3) code to trigger this interrupt, which is the entry point for system calls.

### PIC (Programmable Interrupt Controller)

*   **Concept:** The Intel 8259A PIC is a legacy chip that manages hardware interrupts (IRQs) from devices like the timer, keyboard, and mouse. It multiplexes these signals into a single interrupt line to the CPU.
*   **Remapping:** By default, the PIC's IRQs (0-15) overlap with the CPU's exception vectors (0-31). This is problematic. A standard OS practice, followed by OxideOS, is to **remap the PIC** so its interrupts start at a safe offset, typically **Vector 32**.
    *   IRQ 0 (Timer) -> Vector 32
    *   IRQ 1 (Keyboard) -> Vector 33
    *   ...and so on.
*   **Code Flow:** `pic::init()` sends a sequence of commands to the PIC's I/O ports to perform this remapping.

### Timer Interrupt

*   **Concept:** The timer interrupt is the "heartbeat" of the OS. It fires at a programmable frequency (OxideOS uses 100Hz). Its primary purpose is to allow the OS to regain control from a running program, enabling **preemptive multitasking**.
*   **Code Flow:** `timer::init(100)` configures the Programmable Interval Timer (PIT) chip to generate IRQ 0 at 100Hz. The corresponding ISR in the IDT (Vector 32) is responsible for incrementing a global tick counter.

### EOI (End of Interrupt)

*   **Concept:** After the kernel handles a hardware interrupt from the PIC, it **must** send an EOI command back to the PIC. This signal tells the PIC that it is now ready to receive the next interrupt on that line.
*   **Consequence of Failure:** If the EOI is not sent, the PIC will not generate any more interrupts for that IRQ level or lower, effectively freezing devices like the keyboard or timer.

---

## 3. Memory Management

Once the interrupt system is live, the kernel initializes its memory allocator so it can dynamically create data structures using Rust's `Box`, `Vec`, etc.

### Paging Allocator

*   **Concept:** OxideOS uses a paging-based allocator. It manages physical memory in 4KB chunks called **pages** or **frames**. It uses the memory map provided by Limine to know which physical addresses are available for use. This system is responsible for mapping virtual addresses used by software to physical addresses in RAM.
*   **Code Flow:**
    1.  `kmain` calls `paging_allocator::init_paging_heap()`.
    2.  This function reads the `MEMORY_MAP_REQUEST` from Limine.
    3.  It builds a data structure (like a free-list or bitmap) to track all available physical frames.
    4.  It initializes the `#[global_allocator]` with this new heap.
    5.  A series of tests in `test_paging_allocation()` confirms that allocations of various sizes succeed.

---

## 4. System Calls & User Mode (Ring 3)

A key function of an OS is to run untrusted user code in a restricted environment (Ring 3) while providing secure access to kernel services via **system calls**. OxideOS has the foundational pieces for this.

### The Syscall ABI (`int 0x80`)

OxideOS uses the traditional `int 0x80` mechanism for syscalls.

*   **Register Convention:**
    *   `RAX`: The system call number.
    *   `RDI`, `RSI`, `RDX`, `R10`, `R8`: Arguments 1 through 5.
    *   The return value is placed back in `RAX`.

### The Ring 3 Demo Task

As documented in `step_02_ring3_demo.md`, OxideOS does not yet have a scheduler or ELF loader. Instead, it runs a single, hardcoded demo task to prove the privilege transition mechanism works.

**Execution Flow:**
1.  The `user_mode::run_demo()` function is called from `kmain`.
2.  **Memory Mapping:** It allocates and maps two pages in the lower, non-kernel part of the virtual address space: one for the user code and one for the user stack.
3.  **Code Injection:** A tiny, pre-compiled blob of x86_64 machine code is copied into the user code page. This blob's only job is to set `RAX` to the `sys_exit` syscall number and execute `int 0x80`.
4.  **Privilege Drop:** The kernel executes an `iretq` (Interrupt Return) instruction with a carefully crafted stack frame. This instruction simultaneously sets the instruction pointer to the user code, the stack pointer to the user stack, and changes the CPU's privilege level from Ring 0 to Ring 3.
5.  **User Execution:** The CPU is now in user mode, executing the tiny blob.
6.  **Syscall Trigger:** The blob executes `int 0x80`.
7.  **Privilege Elevation:** The CPU traps this interrupt. It consults the IDT (Vector 128), sees it's a valid call from Ring 3, and uses the `RSP0` from the TSS to switch to the kernel stack. It then jumps to the kernel's syscall handler.
8.  **Syscall Dispatch:** The handler (`handle_system_call`) reads the syscall number from `RAX`, sees it's `sys_exit`, and returns control back to the `run_demo` function.
9.  **Return to Kernel:** The `run_demo` function completes, and `kmain` continues its boot sequence.

### Pointer Validation

A critical security measure in the syscall dispatcher is pointer validation. Before the kernel dereferences any pointer provided by the user program (e.g., for a `write` buffer), it checks that the pointer is in a valid user-space memory range (not pointing into the kernel's higher-half).

---

## 5. GUI & Graphics System

After the core systems are initialized, OxideOS sets up its graphical user interface.

### Framebuffer

*   **Concept:** Instead of a legacy text mode, Limine provides a **linear framebuffer**. This is a simple, contiguous block of memory where each set of 4 bytes represents the color of a single pixel on the screen (e.g., in ARGB format). To draw something, the kernel just writes color values to the correct memory addresses.
*   **Code Flow:** `kmain` retrieves the framebuffer address from `FRAMEBUFFER_REQUEST.get_response()`.

### Graphics and Window Manager

*   **Graphics:** A `Graphics` struct wraps the raw framebuffer, providing safe abstractions for drawing pixels, clearing the screen, and getting screen dimensions.
*   **WindowManager:** A global `WINDOW_MANAGER` is responsible for managing a list of windows. It handles:
    *   Drawing windows with titles and borders.
    *   Determining which window is in focus.
    *   Handling mouse clicks to change focus.
    *   Handling mouse drags to move windows.
    *   Redrawing the screen using a **Painter's Algorithm** (drawing windows from back to front).

### The GUI Event Loop

The kernel's final state is the main GUI loop in `run_gui_with_mouse()`. This is an infinite loop that forms a cooperative event-driven system.

**Loop Cycle:**
1.  **Poll Input:** It polls for new mouse data (`interrupts::poll_mouse_data()`).
2.  **Process Events:** It checks the current mouse position and button state against the previous state to detect clicks, drags, and releases.
3.  **Update State:** It calls `WindowManager` methods (`handle_click`, `handle_drag`) to update the state of the windows. This sets a `needs_redraw` flag if anything changed.
4.  **Redraw (if needed):** If `needs_redraw` is true, the entire screen is redrawn: background, taskbar, and all windows.
5.  **Draw Cursor:** The pixels under the current mouse position are saved, and the cursor is drawn on top.
6.  **Sleep:** The `hlt` (Halt) instruction is executed. This is crucial for efficiency. It puts the CPU into a low-power sleep state until the next hardware interrupt occurs (e.g., a timer tick or mouse movement). When the interrupt handler finishes, execution resumes right after the `hlt`.

---

## 6. Building and Running

The project uses `make` to automate the build and run process.

*   **Dependencies:** `rust`, `qemu`, `xorriso`, `mtools`, and `gmake`.
*   **Kernel Compilation:** Rust compiles the kernel into a Multiboot2-compliant ELF file. The Multiboot2 header is a special section in the ELF file that a compatible bootloader (like Limine or GRUB) can recognize.
*   **ISO Creation:** The `make all` or `make run` targets use tools like `xorriso` to package the compiled kernel ELF file and a configuration file into a bootable `.iso` image.
*   **Running in QEMU:** The `make run` command then launches the QEMU emulator, telling it to boot from the newly created ISO image.

---

## 7. Next Logical Steps (Roadmap)

Based on the current state and documentation, the path forward involves maturing the single-task model into a true multi-tasking OS.

1.  **Task/Process Structure:** Introduce a `Task` or `Process` struct to hold state (PID, registers, memory maps, kernel stack pointer).
2.  **Scheduler:** Implement a simple scheduler (e.g., Round-Robin) that is triggered by the timer interrupt. It will be responsible for context switching between different tasks.
3.  **ELF Loader:** Replace the hardcoded user-mode blob with a proper ELF file loader that can parse an executable from a ramdisk (loaded by Limine) and map its sections into a new process's virtual address space.
4.  **Input Queue:** Decouple interrupt handlers from the main GUI loop by using a ring buffer. The mouse/keyboard IRQ handlers should only read the hardware and place scancodes/packets into the buffer. The GUI loop will then consume events from this buffer, preventing missed input and race conditions.