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

| # | Topic | File | Arch | Status |
|---|-------|------|------|--------|
| 00 | [Rust for OS readers](00_rust_for_os_readers.md) | — (new to Rust? start here) | both | |
| 01 | [Mental model + file map](01_mental_model.md) | — | both (has ARM section) | |
| 02 | [Interrupts: from hardware to handler](02_interrupts.md) | `pic.rs`, `idt.rs`, `interrupts.rs` | x86-64 only¹ | |
| 03 | [Keyboard: tracing a keypress end-to-end](03_keypress_trace.md) | `keyboard.rs`, `terminal.rs` | both² | |
| 04 | [Memory: bump allocator and page tables](04_memory.md) | `allocator.rs`, `paging_allocator.rs` | x86-64 only³ | |
| 05 | [Processes: what a task actually is](05_processes.md) | `scheduler.rs`, `elf_loader.rs` | x86-64 only⁴ | |
| 06 | [Syscalls: crossing the ring boundary](06_syscalls.md) | `syscall_core.rs`, `syscall_handler.rs` | x86-64 only⁴ | |
| 07 | [Drivers: writing new hardware code](07_drivers.md) | `ata.rs`, `rtc.rs`, `virtio_*.rs` | both — has ARM Model D | |

Update the Status column as you go: `reading` → `understood` → `exercised`.

**Arch column footnotes** — see [`docs/arm/README.md`](../arm/README.md)
for the full ARM port status table:
1. aarch64 has an EL1 exception vector table (`arch/aarch64/exceptions.rs`)
   that plays the IDT's hardware role, but no GICv2 yet — no device fires a
   real hardware interrupt on aarch64 today (devices are polled instead).
2. Same *trace* on both architectures — aarch64's virtio-input driver
   feeds the identical `keyboard.rs` decoder x86's PS/2 driver does.
3. aarch64 has a much simpler `linked_list_allocator` heap (no page
   tables, no virtual memory) — see the note at the top of doc 04.
4. Not ported yet — aarch64 has no scheduler or userspace processes, so
   there's nothing to make a syscall from.

---

## How to Use This

1. **Read** — open the study doc and the code files side by side
2. **Answer** — each doc ends with questions; answer them in your own words
3. **Exercise** — each doc has one concrete thing to implement yourself
4. **Notes** — add your own notes at the bottom of each doc as you go

> Tip: use `grep -n "fn_name" kernel/src/path/to/file.rs` to find exact line numbers
> as code evolves. Line numbers in these docs are approximate — the *function names*
> are the stable reference.
