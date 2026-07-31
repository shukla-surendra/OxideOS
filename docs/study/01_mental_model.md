# 01 — Mental Model & File Map

Before reading any code, you need a map. This doc ties every concept to a file
in this codebase so you always know where to look.

> **New to Rust?** Read [`00_rust_for_os_readers.md`](00_rust_for_os_readers.md)
> first — this whole series assumes you're comfortable with `Option`/`Result`,
> `match`, structs/enums, and `unsafe`.

> **Scope note:** this file map (and most of docs 02–06) describes the
> **x86-64** kernel, which is the mature, fully-featured build. OxideOS also
> has a growing **aarch64** port — see [ARM at a glance](#arm-aarch64-at-a-glance)
> below and [`docs/arm/README.md`](../arm/README.md) for what's ported so far.

---

## What an OS actually does

An OS manages three physical resources on behalf of software:

| Resource | What it manages                              | Kernel component                          |
|----------|----------------------------------------------|---------------------------------------    |
| CPU      | Which code runs, when, and at what privilege | `scheduler.rs`, `gdt.rs`, `idt.rs`        |
| Memory   | Which bytes belong to which program          | `allocator.rs`, `paging_allocator.rs`     |
| Hardware | Safe, shared access to devices               | `keyboard.rs`, `ata.rs`, `rtl8139.rs`, .. |

Everything else — filesystems, GUIs, network stacks — is software built on top of
those three foundations.

---

## The privilege split: Ring 0 vs Ring 3

x86-64 CPUs enforce two relevant privilege levels:

**Ring 0 (kernel mode)**
- Full CPU access: all instructions, all memory, all ports
- One mistake here crashes or corrupts the whole machine
- This is where all of `kernel/src/` runs

**Ring 3 (user mode)**
- Restricted: cannot execute privileged instructions, cannot access kernel memory
- A crash here only kills that one process
- This is where `userspace/` programs run

The boundary is crossed via:
- **Into kernel**: `int 0x80` (legacy) or `SYSCALL` instruction → handled by `syscall_core.rs`
- **Back to user**: `IRETQ` or `SYSRET` instruction → in `user_mode.rs`

This split is the *most important concept* in OS design. Every other design decision
flows from maintaining it correctly.

**On aarch64**, the same idea exists under a different name: ARM has four
**Exception Levels** (EL0 = least privileged, up to EL3 = firmware/secure
monitor). OxideOS's aarch64 kernel currently runs entirely at **EL1** (the
ARM analog of Ring 0) — the EL0/user-mode split (ARM's Ring 3 equivalent)
isn't wired up yet, which is why the aarch64 build has no scheduler or
userspace processes today (see the ARM section below).

---

## File map: what lives where

### CPU Layer
```
kernel/src/kernel/arch/
  gdt.rs              — Global Descriptor Table: defines kernel/user memory segments
                        and the TSS (which holds the kernel stack pointer for ring switches)
  idt.rs              — Interrupt Descriptor Table: 256-entry table mapping interrupt
                        numbers to handler functions
  interrupts.rs       — The actual ISR (Interrupt Service Routine) functions
  interrupts_asm.rs   — Raw assembly stubs that save/restore CPU state before calling Rust

kernel/src/kernel/arch/aarch64/   — aarch64 equivalents (different mechanism, see below)
  exceptions.rs       — EL1 exception vector table (16 slots) + fault dump
  timer.rs            — ARM generic timer (CNTP_* system registers)
  psci.rs             — power off / reboot via SMC/HVC
  serial.rs           — PL011 UART (MMIO), not port I/O
  virtio_blk.rs        — polled virtio-mmio block device
  virtio_input.rs      — polled virtio-mmio keyboard + mouse
```

### Driver Layer
```
kernel/src/kernel/drivers/
  pic.rs              — Programs the 8259A chip to route hardware IRQs to CPU vectors
  timer.rs            — Configures the 8253 PIT to fire IRQ0 at 100 Hz
  keyboard.rs         — Decodes PS/2 scancodes; maintains a key event queue
  serial.rs           — UART COM1 at 0x3F8; used for debug logging
  rtc.rs              — CMOS real-time clock: current date/time
  ata.rs              — ATA/IDE disk I/O in PIO mode (programmed I/O, no DMA)
  shutdown.rs         — ACPI shutdown via port I/O

  net/
    pci.rs            — Enumerate PCI bus to find the NIC
    rtl8139.rs        — RTL8139 Ethernet driver (works in QEMU)
    e1000.rs          — Intel e1000 driver (VirtualBox/VMware)
    stack.rs          — Wires the NIC driver into the smoltcp TCP/IP stack
    dns.rs            — DNS A-record lookups over UDP
```

### Memory Layer
```
kernel/src/kernel/mem/
  allocator.rs        — Bump allocator: the very first allocator, used at boot before
                        page tables are set up. Just increments a pointer. Never frees.
  paging_allocator.rs — Page-table allocator: manages virtual→physical mappings,
                        per-process CR3, frame freelist
```

### Filesystem Layer
```
kernel/src/kernel/fs/
  vfs.rs              — Virtual FS: mount point table, routes open() calls to the right FS
  ramfs.rs            — In-memory FS: the main writable FS; files live in RAM
  fat.rs              — Read/write FAT16 on the ATA disk
  ext2.rs             — Read-only ext2 on a second disk
  mbr.rs              — Reads the MBR partition table to find partitions
  procfs.rs           — /proc: virtual files exposing kernel state (uptime, memory, tasks)
```

### Process Layer
```
kernel/src/kernel/proc/
  scheduler.rs        — Round-robin preemptive scheduler: 8 tasks max, 2-tick time slices,
                        per-process page tables (CR3 switch on task switch)
  elf_loader.rs       — Loads ELF64 binaries into user memory (PT_LOAD segments, BSS zero)
  user_mode.rs        — Jumps to Ring 3: sets up stack, segment registers, calls SYSRET
  programs.rs         — Embeds userspace binaries as byte arrays (linked into kernel image)
  env.rs              — 32-slot key=value environment variable store
  tty.rs              — Per-process input/output redirection
```

### Syscall Layer
```
kernel/src/kernel/sys/
  syscall_core.rs     — Syscall number constants + dispatch table (~35 syscalls)
  syscall.rs          — Adapter: wires syscall dispatch to actual kernel functions
  syscall_handler.rs  — Sets up LSTAR/STAR/SFMASK MSRs for the SYSCALL instruction
```

### IPC Layer
```
kernel/src/kernel/ipc/
  ipc.rs              — 16 message queues, 256-byte messages, 64 messages deep
  pipe.rs             — Anonymous pipes: 8 concurrent, 4096-byte ring buffers
  shm.rs              — Shared memory pages (per-process attach/detach)
  stdin.rs            — Global stdin queue shared across processes
```

### GUI Layer
```
kernel/src/gui/
  graphics.rs         — Framebuffer wrapper: pixel blitting, fill_rect, draw_rect
  fonts.rs            — Bitmap font: draw_char, draw_string (fixed 9px width)
  window_manager.rs   — Window chrome, z-order, taskbar, drag, resize
  widgets.rs          — Window struct: position, size, title, visibility
  mouse.rs            — Mouse position and button state
  terminal.rs         — In-kernel terminal: 80×25 grid, command parsing
  notepad.rs          — Text editor: selection, undo/redo, find, menus
  menu.rs             — Menu bar widget: dropdowns, checkable items
  compositor.rs       — Receives draw commands from userspace via IPC queue 1
  ... (launcher, start_menu, overview, quick_settings, notifications, calendar)
```

### Entry Point
```
kernel/src/main.rs    — kmain(): initializes every subsystem in order, then runs
                        the main event loop (mouse, keyboard, draw, scheduler tick)
```

---

## Boot order in `kmain()`

Reading `main.rs` top to bottom gives you the initialization sequence:

1. Serial port (for debug output before screen is up)
2. GDT + TSS + IDT + PIC → enables CPU interrupt handling
3. Memory allocator → heap is now available
4. RamFS + procfs + environment
5. ATA disks → FAT/ext2 filesystems
6. Networking (PCI scan → NIC driver → smoltcp)
7. Graphics (acquire Limine framebuffer)
8. Window manager + initial GUI windows
9. Scheduler (start userspace tasks)
10. **Main loop** — runs forever: poll input → update state → draw → repeat

---

## ARM (aarch64) at a glance

`kernel/src/kernel/arch/` holds one subdirectory per architecture; `mod.rs`
(`kernel/src/kernel/mod.rs`) `#[cfg(target_arch = "x86_64")]`-gates the
subsystems that don't exist on ARM yet. As of this doc, the aarch64 kernel
boots straight into the GUI desktop (mouse/keyboard/disk all working) but
has **no scheduler, no userspace processes, and no syscalls** — those are
the next port targets. Full status table:
[`docs/arm/README.md`](../arm/README.md#feature-status).

| x86-64 file/mechanism | aarch64 equivalent | Status |
|---|---|---|
| `gdt.rs`, `idt.rs`, `interrupts_asm.rs` | `arch/aarch64/exceptions.rs` — one EL1 vector table, 16 slots | ✅ done, but every exception just dumps state and halts — no IRQ dispatch yet |
| `pic.rs` (8259A) | GICv2 interrupt controller | 🔜 not wired up — see next row |
| `keyboard.rs` (PS/2, interrupt-driven) | `arch/aarch64/virtio_input.rs` (virtio-mmio, **polled** each GUI frame, not interrupt-driven) | ✅ works today, different mechanism |
| `ata.rs` (ATA PIO) | `arch/aarch64/virtio_blk.rs` (virtio-mmio, polled) | ✅ works today |
| `timer.rs` (8253 PIT) | `arch/aarch64/timer.rs` (ARM generic timer, `CNTP_*` registers) | ✅ ticks, but nothing consumes it yet (no scheduler) |
| `shutdown.rs` (ACPI ports) | `arch/aarch64/psci.rs` (SMC/HVC calls) | ✅ done |
| `scheduler.rs`, `user_mode.rs`, `elf_loader.rs`, `sys/` | — | ⏳ planned |

The **why**: x86 has decades of legacy port-I/O devices (PIC, PIT, PS/2,
ATA) with no ARM equivalent at all — ARM's QEMU `virt` machine is
memory-mapped-I/O only. See `docs/study/07_drivers.md`'s new "Model D"
section for what an MMIO driver looks like, and
[`docs/arm/01-arch-abstraction.md`](../arm/01-arch-abstraction.md) for how
`kernel/src/kernel/arch/cpu.rs` lets portable code (the GUI, the panic
handler) call one function (`cpu::irq_disable()`, etc.) that compiles to
different instructions per architecture instead of scattering `#[cfg(...)]`
through shared code.

---

## Rust patterns in this file map

You don't need new Rust *syntax* to read a file map — but the map itself
*is* Rust module structure, so it's worth naming explicitly:

- **`kernel/src/kernel/mod.rs`** is the literal source of this file map —
  every `pub mod drivers;` / `pub mod arch;` line you see there is a
  directory this doc describes. If a subsystem doesn't apply to the
  current architecture, its `mod` line is prefixed with
  `#[cfg(target_arch = "x86_64")]`, and on an aarch64 build that code isn't
  compiled at all (see `docs/study/00_rust_for_os_readers.md#11`).
- **`pub use drivers::serial;`** style re-exports (also in `mod.rs`) are why
  code elsewhere writes `crate::kernel::serial::SERIAL_PORT` instead of the
  deeper `crate::kernel::drivers::serial::SERIAL_PORT` — same item, shorter
  path, nothing duplicated.
- `main.rs`'s two `kmain()` functions (one `#[cfg(target_arch = "x86_64")]`,
  one `#[cfg(target_arch = "aarch64")]`) are the cleanest single example in
  the whole codebase of "same entry-point name, architecture picks which
  body compiles."

---

## Questions to answer before moving on

1. Why must the GDT be set up before anything else? What breaks without it?
2. What would happen if you called `Vec::new()` before the memory allocator is initialized?
3. Why does the kernel GUI run in Ring 0 instead of userspace? What's the tradeoff?
4. What does "preemptive" mean in the scheduler? How does the timer ISR enable it?
5. Why does each process get its own CR3? What would go wrong if they shared one?

---

## Your notes
<!-- Add your own notes here as you read -->
