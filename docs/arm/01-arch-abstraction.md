# Feature 1 — Architecture Abstraction Layer

Foundation for the ARM port: separate what is x86-specific from what is
portable, without changing any x86-64 behavior.

## What changed

### Directory restructure

```
kernel/src/kernel/arch/
├── mod.rs          cfg-dispatch + stable re-export names
├── cpu.rs          portable CPU-control facade (both arches)
├── x86_64/
│   ├── mod.rs
│   ├── gdt.rs           (moved from arch/gdt.rs)
│   ├── idt.rs           (moved from arch/idt.rs)
│   ├── interrupts.rs    (moved from arch/interrupts.rs)
│   └── interrupts_asm.rs(moved from arch/interrupts_asm.rs)
└── aarch64/
    └── mod.rs      skeleton — filled in feature by feature
```

`arch/mod.rs` re-exports the x86 modules under their old names
(`arch::gdt`, `arch::idt`, `arch::interrupts`, `arch::interrupts_asm`), and
`kernel/mod.rs` keeps its flat re-exports (now `#[cfg(target_arch =
"x86_64")]`-gated), so **no call site outside `arch/` had to change its
imports**.

### The `arch::cpu` facade

`arch/cpu.rs` is the one place portable code touches the CPU directly.
Each function has an implementation per architecture:

| Facade | x86-64 | aarch64 |
|--------|--------|---------|
| `irq_disable()` | `cli` | `msr daifset, #2` |
| `irq_enable()` | `sti` | `msr daifclr, #2` |
| `irqs_enabled()` | `pushfq` → RFLAGS.IF | `mrs DAIF` → !I |
| `irq_save()` / `irq_restore()` | flags save + `cli` | DAIF save + mask |
| `wait_for_interrupt()` | `hlt` | `wfi` |
| `spin_hint()` | `pause` | `yield` |
| `halt_forever()` | `cli; hlt` loop | mask + `wfi` loop |

Note the inverted sense on ARM: x86's RFLAGS.IF bit set means interrupts
*enabled*; ARM's DAIF.I bit set means IRQs *masked*. The facade hides this
— `irqs_enabled()` answers the same question on both.

Call sites converted to the facade (all portable code):

- `gui/terminal.rs` — `with_interrupts_disabled` critical section now uses
  `irq_save`/`irq_restore` instead of open-coded `pushfq`/`cli`/`sti`.
- `gui_loop.rs` — main-loop idle `hlt` → `cpu::wait_for_interrupt()`.
- `panic.rs` — panic entry `cli` → `cpu::irq_disable()`; `halt_system` →
  `cpu::halt_forever()`. (The x86 register dumps were already cfg-gated.)
- `boot_init.rs` — `cli`/`sti`/`pause`/`hlt` → facade equivalents.
  (The rest of `init_interrupt_system` is genuinely x86 — GDT, IDT, PIC,
  CR4 — and will be split behind per-arch `arch::init()` in Feature 2.)

`main.rs`'s `#![feature(abi_x86_interrupt)]` is now
`#![cfg_attr(target_arch = "x86_64", ...)]` so the crate parses on ARM.

### Drive-by repair

`kernel/tests/syscall_core.rs` pointed at the pre-reorg path
`src/kernel/syscall_core.rs`; corrected to `src/kernel/sys/syscall_core.rs`.
(The harness needs further work — `syscall_core.rs` has since grown
kernel-internal imports — tracked separately, unrelated to the port.)

## Rules going forward

1. Raw `asm!` is allowed only in `arch/<target>/` and in drivers that are
   inherently arch-specific (e.g. the x86 port-I/O drivers, which will be
   cfg-gated out on ARM in Feature 2).
2. New portable code needing a CPU primitive extends `arch::cpu` with both
   implementations, not one inline `asm!`.
3. `arch/mod.rs` is the only file that knows which architectures exist.

## Verification

- `cargo build --target x86_64-unknown-none` — builds and links (dev
  profile) after the restructure; only import plumbing changed, no logic.
- Boot regression on QEMU x86-64 requires the machine to have the full
  toolchain (`install_dep.sh`); see the environment note in the ARM
  README's build section.

## What Feature 2 builds on this

`boot_init::init_interrupt_system()` becomes `arch::init()` with an x86
body (current GDT/IDT/PIC/PIT sequence) and an ARM body (EL1 vector table,
PL011 console, GICv2, generic timer), letting `kmain` stay portable.
