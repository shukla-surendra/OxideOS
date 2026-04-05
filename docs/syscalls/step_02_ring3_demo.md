# Step 2: Ring-3 User Program Demo

## Goal

Run one real x86_64 user-mode program under OxideOS and return to the kernel through `sys_exit`.

This step does **not** add a scheduler or ELF loader yet. It is a bootstrap milestone focused on privilege transitions.

## What This Step Adds

- a kernel-owned GDT with ring-0 and ring-3 descriptors,
- a TSS with `RSP0` for safe CPL3 -> CPL0 interrupts,
- user-page mapping helpers for low virtual addresses,
- a tiny hard-coded user program blob,
- a controlled `sys_exit` path that returns control to the kernel.

## Why GDT + TSS Matter

For an `int 0x80` from ring 3 to ring 0, the CPU needs:

- an IDT gate that is callable from CPL3,
- a valid ring-0 code segment,
- and a TSS whose `RSP0` points at a kernel stack.

Without `RSP0`, the CPU would not know which kernel stack to use when switching privilege levels.

## Current Demo Model

The current user-mode model is deliberately small:

1. map a user code page at a low virtual address,
2. map a user stack,
3. copy a tiny position-independent user program blob into that page,
4. `iretq` into ring 3,
5. let the program invoke `int 0x80`,
6. return to the kernel when the program calls `sys_exit`.

## Important Limitation

This is a **single demo task** model.

It is not yet:

- a process table,
- a scheduler,
- separate address spaces per process,
- or an ELF binary loader.

## Next Logical Steps

1. Add safe copy-in / copy-out routines tied to mapped user pages.
2. Replace the hard-coded program blob with a loaded flat binary or ELF image.
3. Add a task struct so `PID`, kernel stack, user stack, and exit code belong to a real task object.
4. Add a simple scheduler so `sys_exit` does not only return to the boot path.
