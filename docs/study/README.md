# OxideOS Study Journal

This folder is a hands-on study companion — not theory, but a reading guide tied directly
to actual files and line numbers in this codebase. Each doc covers one topic, points you
at the exact code to read, and ends with exercises to do yourself.

The goal: go from "I have a working OS I don't fully understand" to "I can explain and
extend any part of this OS from scratch."

---

## The Layered Mental Model

```
┌─────────────────────────────────────────────────────────────┐
│  Ring 3 (userspace)  — user programs, shell, filemanager    │
├─────────────────────────────────────────────────────────────┤
│  Syscall boundary    — int 0x80 / SYSCALL instruction        │
├─────────────────────────────────────────────────────────────┤
│  Ring 0 (kernel)                                            │
│    Process layer     — scheduler, ELF loader                │
│    Memory layer      — allocator, paging, page tables       │
│    Filesystem layer  — VFS → ramfs / fat / ext2             │
│    Driver layer      — keyboard, ATA, NIC, RTC              │
│    CPU layer         — GDT, IDT, interrupts, PIC            │
│    GUI layer         — window manager, widgets, apps        │
└─────────────────────────────────────────────────────────────┘
│  Hardware            — x86-64 CPU, PS/2, SATA, Ethernet     │
```

Each layer only talks to the layer directly below it.  
You understand the OS when you can explain *why* each layer exists.

---

## Study Path

Work through these in order. Each builds on the previous.

| # | Topic | File | Status |
|---|-------|------|--------|
| 01 | [Mental model + file map](01_mental_model.md) | — | |
| 02 | [Interrupts: from hardware to handler](02_interrupts.md) | `pic.rs`, `idt.rs`, `interrupts.rs` | |
| 03 | [Keyboard: tracing a keypress end-to-end](03_keypress_trace.md) | `keyboard.rs`, `terminal.rs` | |
| 04 | [Memory: bump allocator and page tables](04_memory.md) | `allocator.rs`, `paging_allocator.rs` | |
| 05 | [Processes: what a task actually is](05_processes.md) | `scheduler.rs`, `elf_loader.rs` | |
| 06 | [Syscalls: crossing the ring boundary](06_syscalls.md) | `syscall_core.rs`, `syscall_handler.rs` | |
| 07 | [Drivers: writing new hardware code](07_drivers.md) | `ata.rs`, `rtc.rs` | |

Update the Status column as you go: `reading` → `understood` → `exercised`.

---

## How to Use This

1. **Read** — open the study doc and the code files side by side
2. **Answer** — each doc ends with questions; answer them in your own words
3. **Exercise** — each doc has one concrete thing to implement yourself
4. **Notes** — add your own notes at the bottom of each doc as you go

> Tip: use `grep -n "fn_name" kernel/src/path/to/file.rs` to find exact line numbers
> as code evolves. Line numbers in these docs are approximate — the *function names*
> are the stable reference.
