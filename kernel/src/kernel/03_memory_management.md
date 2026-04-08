# 3. Memory Management

Once the CPU and interrupt systems are configured, the kernel needs the ability to dynamically allocate memory. Without this, it's impossible to create data structures whose size isn't known at compile time, such as lists of tasks, open files, or network packets. This document explains how OxideOS initializes its memory management system.

---

## Virtual vs. Physical Memory

Modern operating systems use **virtual memory** to give each process the illusion of having its own private, contiguous address space. The CPU and the Memory Management Unit (MMU) work together to translate these virtual addresses into actual **physical addresses** in RAM. This translation is done through a set of data structures called **page tables**.

This system provides memory isolation, security, and flexibility. OxideOS, like other 64-bit systems, divides the virtual address space into two main regions:

*   **Lower Half**: For user-space applications.
*   **Higher Half**: For the kernel.

## The Paging Allocator

The core of memory management is the **physical frame allocator** (or paging allocator). Its job is to manage all of the system's physical memory, which it treats as an array of fixed-size blocks called **frames** (typically 4KB). The allocator needs to know which frames are free to be allocated and which are already in use.

### Getting the Memory Map

But how does the allocator know which parts of physical RAM are available in the first place? It gets this information from the **Limine bootloader**. During boot, the kernel makes a `MemoryMapRequest`. Limine responds with a list of all physical memory regions, tagging each one with a type, such as:

*   `Usable`: Normal RAM that the OS can use.
*   `Reserved`, `ACPI Reclaimable`, `Bootloader Reclaimable`: Areas used by firmware or the bootloader that the OS should not touch (or can reclaim after boot).

### Initializing the Heap

The `paging_allocator::init_paging_heap()` function in `kmain` is responsible for setting up the allocator:

1.  It retrieves the memory map response from Limine.
2.  It iterates through the memory map, identifying all `Usable` regions.
3.  It builds a data structure (often a bitmap or a linked list, known as a "free list") to keep track of every single 4KB frame within the usable regions. Initially, all these frames are marked as "free".

## The Global Allocator

Once the physical frame allocator is ready, it can be used to build a higher-level **heap allocator**. This is the allocator that the rest of the kernel will use for dynamic memory via Rust's standard library types like `Box`, `Vec`, and `String`.

OxideOS wires this up using the `#[global_allocator]` attribute on a static instance of its allocator implementation. When `init_paging_heap` completes, this global allocator becomes functional.

The `test_paging_allocation` function in `main.rs` serves as a "smoke test" to verify this. It attempts to create a `Box` and a `Vec`, which will panic if the allocator is not working correctly. Success confirms that the entire memory management stack, from the Limine memory map to the Rust `Box`, is operational.