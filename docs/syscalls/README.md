# Syscalls in OxideOS

This directory tracks syscall work as a staged kernel feature instead of a single big jump.

## Current Stage

Steps 1 and 2 are now implemented:

- A real `int 0x80` entry is installed in the IDT.
- Syscall dispatch rules live in a shared core module.
- The dispatch core is covered by host-side tests.
- A kernel-owned GDT/TSS path can enter ring 3 for a tiny demo task.
- The demo task uses a mapped user code page and user stack, then returns through `sys_exit`.
- `make -C kernel test-syscalls` runs the current syscall test harness.
- The old `syscall/sysret` fast path remains in the tree as future work, not as the current supported ABI.

## Document Map

- `step_01_int80_dispatch.md`: theory, design, and implementation notes for the first milestone.
- `step_02_ring3_demo.md`: theory, design, and implementation notes for the first ring-3 demo task.

## Roadmap

1. Step 1: `int 0x80` dispatcher, argument flow, pointer validation, and tests.
2. Step 2: GDT entries, TSS, ring 3 stacks, and a tiny user task.
3. Step 3: add safe copy-in/copy-out helpers, memory-backed syscalls, and process-aware state.
4. Step 4: replace the hard-coded demo blob with a loaded user binary format.
5. Step 5: choose whether OxideOS keeps `int 0x80` as the stable ABI or adds `syscall/sysret` as a fast-path ABI.
6. Step 6: add end-to-end user-space tests inside QEMU.
