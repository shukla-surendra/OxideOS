# OxideOS Technical Documentation

This documentation provides a detailed, in-depth look into the architecture and internal workings of OxideOS. Each document focuses on a specific subsystem, explaining the core concepts, design choices, and code implementation.

The documents are structured to be read in order, building a complete picture of the operating system from the ground up.

## Table of Contents

1.  [**The Boot Process**](./01_boot_process.md): From firmware to the kernel's entry point.
2.  [**Interrupts & CPU Setup**](./02_interrupts_and_cpu.md): Configuring the GDT, TSS, IDT, and PIC.
3.  [**Memory Management**](./03_memory_management.md): The paging allocator and heap.
4.  [**System Calls & User Mode**](./04_syscalls_and_usermode.md): Privilege separation and the kernel's ABI.
5.  [**Graphics & GUI**](./05_graphics_and_gui.md): The graphics stack, from framebuffer to window manager.
6.  [**The Storage Stack**](./06_storage_stack.md): The ATA PIO driver and the FAT16 filesystem.