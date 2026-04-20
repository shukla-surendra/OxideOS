# Chapter 3: Memory Management - Organizing the Computer's Brain

Once the CPU and interrupt systems are configured, the kernel needs the ability to dynamically allocate memory. Without this, it's impossible to create data structures whose size isn't known at compile time, such as lists of tasks, open files, or network packets. This document explains how OxideOS initializes its memory management system.

---

## Virtual vs. Physical Memory

Imagine your computer's RAM as a huge set of numbered mailboxes (physical addresses). Now imagine every program on your computer (your web browser, a game, the OS kernel itself) has its own personal map of these mailboxes. This personal map uses its own set of addresses, which are called **virtual addresses**.

Modern operating systems use **virtual memory** to give each program the illusion that it has its own private, contiguous block of memory, starting from address 0. In reality, the program's virtual addresses are constantly being translated by the CPU's **Memory Management Unit (MMU)** into actual **physical addresses** in the computer's RAM. This translation is done using special data structures called **page tables**.

This system is incredibly powerful because it provides:
*   **Memory Isolation**: One program cannot accidentally (or maliciously) access or corrupt another program's memory, or the kernel's memory, because their virtual address spaces are separate.
*   **Security**: It prevents user programs from directly accessing sensitive kernel data or hardware.
*   **Flexibility**: Programs don't need to be loaded into contiguous physical memory. The OS can scatter their data across RAM, and the virtual memory system makes it appear contiguous to the program.

OxideOS, like other 64-bit systems, divides the vast 64-bit virtual address space into two main regions:

*   **Lower Half**: This region is primarily used for **user-space applications**. Each user program gets its own unique lower-half virtual address space.
*   **Higher Half**: This region is reserved for the **kernel**. The kernel's code and data always reside in the higher half, and this mapping is typically shared across all processes. This means the kernel is always present and accessible, regardless of which user program is currently running.

## The Paging Allocator

The core component responsible for managing the computer's physical RAM is the **physical frame allocator**, often simply called the **paging allocator**. Its job is to keep track of every single piece of physical memory. It treats all physical memory as an array of fixed-size blocks called **frames** (in x86-64, these are typically 4KB). The allocator's primary function is to know which frames are currently free and available for use, and which are already occupied by the kernel or user programs.

### Getting the Memory Map

But how does our kernel know which parts of the physical RAM are actually available to use? It doesn't just guess! It gets this crucial information from the **Limine bootloader**. During the boot process (as described in Chapter 1), our kernel makes a `MemoryMapRequest` to Limine. Limine then provides a detailed list of all physical memory regions, tagging each one with a specific type:

*   `Usable`: This is the good stuff! Normal RAM that the OS can freely allocate and use.
*   `Reserved`: Areas used by hardware, firmware, or the bootloader that the OS *must not* touch.
*   `ACPI Reclaimable`, `Bootloader Reclaimable`: These are special regions that were used by the firmware or bootloader but can be reclaimed and used by the OS *after* boot.
*   `Bad Memory`: Physical memory that has been detected as faulty and should be avoided.

### Initializing the Heap

The `paging_allocator::init_paging_heap()` function, called early in `kmain`, is the orchestrator for setting up this physical frame allocator:

1.  **Retrieve Memory Map**: It first retrieves the detailed memory map response from Limine.
2.  **Identify Usable Regions**: It then carefully iterates through this map, identifying all the `Usable` regions of physical RAM.
3.  **Build Free List**: For each usable region, it builds an internal data structure (often a **bitmap** or a **linked list** of free blocks, known as a "free list"). This structure's purpose is to keep track of every single 4KB physical frame within those usable regions. Initially, all these frames are marked as "free" and ready for allocation.
4.  **Identity Mapping**: It also sets up an "identity mapping" for the kernel's higher-half direct map. This means that if physical address `X` exists, the kernel can access it directly at a virtual address like `0xFFFF800000000000 + X`. This simplifies kernel memory access significantly.

## The Global Allocator

Once the low-level physical frame allocator is operational, it can be used to build a higher-level **heap allocator**. This is the allocator that the rest of the kernel's Rust code will use for dynamic memory. This allows us to use familiar Rust standard library types like `Box<T>`, `Vec<T>`, and `String`, which all rely on a working heap.

OxideOS wires this up using Rust's `#[global_allocator]` attribute. This attribute tells the Rust compiler which static instance of an allocator implementation should be used for all dynamic memory allocations in the `no_std` environment. When `init_paging_heap` completes, this global allocator becomes fully functional, making dynamic memory allocation available throughout the kernel.

The `test_paging_allocation` function in `main.rs` serves as a crucial "smoke test" to verify that the entire memory management system is working as expected. It attempts to:

1.  **Allocate a `Box`**: This is a simple heap allocation for a single value.
2.  **Allocate a `Vec`**: This involves allocating a dynamically sized array, which often requires multiple reallocations as it grows.

If any of these allocations fail (e.g., due to an incorrect memory map or a bug in the allocator logic), the test will panic, indicating a problem. Successful completion of these tests confirms that the entire memory management stack—from the raw physical memory map provided by Limine, through our paging allocator, all the way up to Rust's high-level `Box` and `Vec` types—is operational and ready for the rest of the kernel to use.

This robust memory management foundation is essential for building a complex operating system, as almost every advanced feature (like process management, file systems, and networking) relies heavily on dynamic memory allocation.