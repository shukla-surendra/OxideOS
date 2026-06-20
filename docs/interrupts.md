# Interrupts & CPU Setup — Design Notes

For the IDT/GDT/PIC walkthrough, see
[oxide_cocepts/02_interrupts_and_cpu.md](oxide_cocepts/02_interrupts_and_cpu.md)
and [study/02_interrupts.md](study/02_interrupts.md). This doc covers the
*why* behind the choices.

## PIC (8259), not APIC

Interrupt routing uses the legacy 8259 PIC (`kernel/src/kernel/drivers/pic.rs`),
remapping IRQ0–15 to vectors 32–47, rather than the Local/IO APIC.

- The PIC is simpler to program correctly (a handful of I/O-port writes vs.
  parsing ACPI's MADT table to discover LAPIC/IOAPIC addresses) and is
  sufficient for a single-core kernel.
- The real cost is architectural, not implementation effort: PIC is
  inherently single-CPU-target — there's no way to route an interrupt to a
  specific core. Moving to SMP (Phase 17 in `docs/plan.md`) requires
  switching to APIC first; this is a known, deliberate piece of debt taken
  on to get interrupts working at all before tackling multi-core.

## Dual syscall ABI: `int 0x80` *and* `SYSCALL`/`SYSRET`

Both the legacy software-interrupt path (vector 128) and the fast
`SYSCALL`/`SYSRET` instruction pair (via `STAR`/`LSTAR`/`FMASK` MSRs, see
`kernel/src/kernel/sys/syscall_handler.rs`) are wired up, dispatching to the
same handler.

- This isn't redundancy — it's compatibility surface. Different libc/runtime
  combinations emit different syscall entry instructions (older or
  statically-linked code tends to use `int 0x80`; musl and most modern
  runtimes use `syscall`). Supporting only one would silently break whichever
  userspace binaries assume the other.
- The cost is two entry points to keep in sync (register conventions differ
  slightly — `SYSCALL` clobbers `rcx`/`r11` for return address/flags) rather
  than one, but both funnel into the same dispatch table immediately, so the
  duplication is contained to the two short asm trampolines.

## TSS: stack switch only, no IST

The TSS's `RSP0` field supplies the kernel stack for ring 3 → ring 0
transitions (interrupts and `int 0x80`/`syscall` from user mode). The IST
(Interrupt Stack Table) entries exist in the struct but aren't used — every
exception handler runs on the current stack rather than a dedicated
fault-isolation stack.

- This is a real gap, not a non-issue: a double fault or stack-overflow
  fault that occurs *because* the kernel stack is already corrupt has
  nowhere safe to land and will likely triple-fault instead of producing
  the BSOD crash-dump cleanly. Wiring `IST[0]` for the double-fault handler
  is cheap and is the most likely first thing to add here.

## No locks — interrupts-disabled is the concurrency model

There are no spinlocks anywhere in the scheduler or interrupt path. Shared
kernel state is protected by disabling interrupts (`cli`) around the
critical section, which is sufficient *only* because the kernel is
single-core with cooperative-ish preemption (a task can't be interrupted by
another core, only by an IRQ on the same core, and IRQs are masked during
the section).

- This is the single biggest reason SMP is a separate, later phase rather
  than an incremental add-on: every "protect with `cli`" pattern in the
  current codebase becomes a real data race the moment a second core can
  run kernel code concurrently. Adding SMP means auditing and replacing
  every one of these sections with actual locks, not just starting
  additional cores.
