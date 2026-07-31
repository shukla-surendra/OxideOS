# Interrupt Controllers — 8259A PIC vs. APIC/x2APIC/GICv3/AIC

**Companion to:** [8259A_pic.md](8259A_pic.md) (`kernel/src/kernel/drivers/pic.rs`).
This doc has no OxideOS source of its own — it explains what replaced the
8259A in real hardware, and why OxideOS still uses the 1976-era chip.

---

## What it is

An interrupt controller's job never changes: let hardware devices signal
the CPU asynchronously instead of being polled. What changed is *scale* —
the 8259A was designed for one CPU and 15 devices; a modern machine has
many CPU cores, dozens of devices, and needs to route any interrupt to any
core on demand (for load balancing, or to wake a sleeping core). The
8259A's single shared "INT pin per chip" model has no way to express that.

---

## x86-64: Local APIC + I/O APIC + x2APIC

| | 8259A PIC | APIC (Advanced PIC) | x2APIC |
|---|---|---|---|
| **Scope** | One shared controller for the whole machine | **Local APIC per core** + one (or more) **I/O APIC** for device routing | Same model, different access method |
| **Access** | Port I/O (`0x20`/`0x21`, `0xA0`/`0xA1`) | Memory-mapped registers (fixed physical address, typically `0xFEE00000`) | **MSRs** (`WRMSR`/`RDMSR`) — no memory access at all, lower latency |
| **IRQ lines** | 15 usable (one lost to cascade) | I/O APIC: up to 24 per chip, servers often have several | Same, but routing table entries can address far more cores |
| **Routing** | Fixed: IRQn always goes to "the CPU" (there's only one) | I/O APIC has a **Redirection Table** — each entry says which vector *and which core* an IRQ goes to | Same table, MSR-addressed |
| **Core-to-core signaling** | None — impossible | **IPIs** (Inter-Processor Interrupts) — how one core wakes/stops another; this is *the* mechanism SMP scheduling depends on | Same, faster |
| **Max CPUs addressable** | 1 | 255 (8-bit APIC ID) | Way more (32-bit ID) — why x2APIC exists: modern servers exceed 255 cores |
| **Own timer** | No (needs the separate PIT) | Yes — each Local APIC has a built-in timer, see [`modern_timers.md`](modern_timers.md) | Yes |

**The Local APIC is why SMP works at all.** Without a per-core interrupt
target, there's no way for the scheduler on core 0 to tell core 3 "stop
what you're doing, a higher-priority task is ready for you" — that's
literally an IPI, a Local-APIC-only concept the 8259A has no equivalent
of. This is also why OxideOS staying on the 8259A is coupled to it having
no SMP support at all (`docs/study/05_processes.md`'s scheduler is
single-core) — the two limitations reinforce each other.

**The old chip is still physically there.** Every x86 motherboard still
answers port `0x20`/`0x21`/`0xA0`/`0xA1` exactly like a real 8259A, purely
for backward compatibility (some firmware and very old OSes still expect
it). A modern OS's real boot sequence is: parse the ACPI **MADT** table to
find the Local APIC and I/O APIC addresses, **mask off the legacy 8259A
entirely** (write `0xFF` to both IMRs, exactly like `pic::init()` already
does as its *starting* state), and never touch it again. OxideOS's driver
is, in that sense, doing the first half of what every real OS does at boot
— it just never takes the second half of the step.

---

## ARM: GICv3/v4 — and the Apple exception

ARM standardized the same problem differently: the **Generic Interrupt
Controller (GIC)**, now at version 3/4 for current cores.

| GIC concept | Role |
|---|---|
| **Distributor** | Global routing — which core(s) can receive which shared interrupt (like the I/O APIC's redirection table) |
| **Redistributor** (one per core, GICv3+) | Delivers per-core interrupts — private timer ticks, IPIs |
| **CPU interface** | What each core actually reads to ack/complete an interrupt |
| **SGI** (Software Generated Interrupt) | ARM's name for an IPI |
| **PPI** (Private Peripheral Interrupt) | Per-core device interrupts (e.g., that core's own timer) |
| **SPI** (Shared Peripheral Interrupt) | A normal device IRQ, routable to any core — the PIC/I-O-APIC-replaced case |
| **ITS** (Interrupt Translation Service, GICv3+) | Message-signaled interrupts (like PCIe MSI-X) translated into GIC interrupts |

`docs/study/02_interrupts.md`'s ARM note already flags that OxideOS's
aarch64 port has an EL1 exception vector table
(`kernel/src/kernel/arch/aarch64/exceptions.rs`) but no GICv2/v3 wiring
yet — which is exactly why the current aarch64 build polls virtio devices
every GUI frame instead of taking real interrupts (`virtio_input.rs`,
`virtio_blk.rs`). QEMU's `virt` machine — the reference platform for this
port — emulates a standard, spec-compliant GICv2/v3, so this gap is purely
"not implemented yet," not a hardware limitation.

**The twist:** the MacBook Pro this conversation has been running on
doesn't use GIC at all. Apple Silicon has its own proprietary **AIC**
(Apple Interrupt Controller) — not GIC-architecture-compliant, entirely
undocumented by Apple, reverse-engineered by the Asahi Linux project to
write a Linux driver for it. So "ARM" doesn't imply one interrupt
controller the way "x86-64" implies APIC — real ARM silicon splits into
standard-GIC designs (essentially everything except Apple) and Apple's own
design. OxideOS's future aarch64 GIC work targets QEMU `virt`'s
standard GICv2, which is the right choice for a portable OS — but be aware
it still wouldn't be the interrupt controller a real M-series Mac uses.

---

## Why OxideOS stays on the 8259A

Same reasoning as every "legacy vs. modern" doc in this folder: the 8259A
is ~200 lines of straightforward port I/O with a stable, single-page
datasheet, sufficient for a single-core QEMU/VirtualBox target. Moving to
APIC means parsing ACPI's MADT table first (a whole ACPI table-walking
subsystem OxideOS doesn't have) just to find the Local APIC's address —
real complexity for a benefit (SMP, >15 IRQ lines) this kernel doesn't
need yet. `docs/plan.md` treats SMP as a future phase for exactly this
reason: APIC only pays for itself once there's more than one core to route
interrupts to.

---

## Self-check questions

1. Why is an IPI structurally impossible to build on top of the 8259A, no
   matter how you program it?
2. `pic::init()` starts by masking every IRQ (`0xFF` to both IMRs) before
   selectively unmasking the ones OxideOS uses. What does a real OS do
   with the 8259A at the *same* point in boot, and why is it the same
   first step for a different final goal?
3. What ACPI table would a from-scratch APIC driver need to parse first,
   and what does it contain?
4. Why does x2APIC use MSRs instead of the memory-mapped registers plain
   APIC uses? What's the practical benefit of *not* going through memory?
5. If you were to add GICv2 support to OxideOS's aarch64 port, why would
   that work correctly on QEMU's `virt` machine but not directly transfer
   to real Apple Silicon hardware?

---

## Sources

- [Apple Interrupt Controller (AIC) — Asahi Linux Documentation](https://asahilinux.org/docs/hw/soc/aic/)
- [Generic Interrupt Controller versions 3 and 4 — OSDev Wiki](https://wiki.osdev.org/Generic_Interrupt_Controller_versions_3_and_4)
- [Arm Generic Interrupt Controller Architecture Specification](https://www.scs.stanford.edu/~zyedidia/docs/arm/gic_v3.pdf)
- `docs/study/02_interrupts.md`, `docs/study/hardware/8259A_pic.md` (this codebase)
