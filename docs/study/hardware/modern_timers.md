# Timers — 8253/8254 PIT vs. HPET / LAPIC Timer / TSC-Deadline / ARM Generic Timer

**Companion to:** [8253_pit.md](8253_pit.md) (`kernel/src/kernel/drivers/timer.rs`).
Reference-only — this doc explains what replaced the PIT as the primary
system tick in real hardware.

---

## What it is

Every OS needs a source of "time has passed" events: to preempt tasks, to
implement `sleep()`, to timestamp things. The PIT gives you exactly one
global 100 Hz-ish tick, port-I/O programmed, ~microsecond jitter from the
I/O round-trip itself. Modern hardware needs the same concept per-core
(SMP again), higher precision, and — ideally — a clock source that doesn't
drift with CPU power-state changes.

---

## The x86-64 lineup

| Timer | Access | Per-core? | Precision | Status in 2026 |
|---|---|---|---|---|
| **8253/8254 PIT** | Port I/O (`0x40`–`0x43`) | No — one global counter | ~1.19 MHz base, coarse | Legacy; still present in every chipset for boot-time compatibility, rarely used as the primary tick past early boot |
| **HPET** (High Precision Event Timer) | Memory-mapped, address found via the ACPI **HPET** table | No — shared, but has multiple independent comparators | ≥10 MHz counter, much finer than PIT | Present on essentially every x86 board since ~2005; itself now considered legacy — kept mainly for calibration and compatibility, not as the OS's main tick |
| **Local APIC Timer** | Same Local APIC each core already has for interrupts (see `interrupt_controller_modern.md`) | **Yes — one per core** | Depends on mode; can run off the CPU's own bus clock or a fixed crystal | The default choice for per-core scheduler ticks on modern OSes |
| **TSC-deadline mode** | `WRMSR` to `IA32_TSC_DEADLINE` | Yes (per core) | Nanosecond-class, and — critically — **invariant**: modern CPUs guarantee the TSC increments at a fixed rate regardless of turbo/power state | The preferred mechanism on any CPU new enough to advertise the `invariant TSC` and `TSC-deadline` CPUID feature bits (essentially everything since ~2011) |
| **ARM generic timer** | System registers (`CNTP_*`), not MMIO or port I/O at all | Yes — one per core | Fixed frequency from `CNTFRQ_EL0` | The ARM-world equivalent of the LAPIC timer; **already implemented** in this codebase's `arch/aarch64/timer.rs` |

`rdtsc()` already exists in OxideOS's `timer.rs` as a high-resolution
cycle counter, but the doc itself notes it's "not used for the scheduler
because it's CPU-frequency-dependent" — that caveat used to be true on
older CPUs where the TSC's rate changed with clock throttling, but on any
CPU with the invariant-TSC guarantee (checkable via `CPUID` leaf
`0x80000007`, bit 8), that concern no longer applies, which is exactly why
real modern kernels lean on TSC-deadline mode instead of a PIT-style rate
generator.

---

## Why the shift happened

The PIT model — "program a divisor, get a periodic pulse on one fixed
line, whoever's running gets interrupted" — has two problems at scale:

1. **No per-core targeting**, same root issue as the PIC (see
   `interrupt_controller_modern.md`): with N cores each needing their own
   time slice, a single global timer forces all scheduling decisions
   through one interrupt, then software has to fan out via IPIs anyway.
2. **Fixed granularity.** A rate generator fires at one frequency for
   everything. TSC-deadline lets each core arm a timer for the *exact*
   nanosecond the next scheduling event needs to happen — no wasted
   interrupts, better for power management (a core with nothing scheduled
   can go fully idle instead of waking every 10 ms for a tick it doesn't
   need — this is the basis of "tickless" kernels).

HPET's own history is a smaller version of the same story: it replaced the
PIT as a *higher-precision* shared timer in the mid-2000s, but was itself
mostly superseded once TSC-deadline mode became reliable — it's kept
around today largely so firmware and legacy OSes have a known-good
calibration reference, not because anything modern prefers it as a primary
tick.

---

## Why OxideOS still uses the PIT

Single core, single global tick, no power-management ambitions — the PIT
is sufficient and, like the 8259A, needs no ACPI table parsing to locate
(it's always at fixed ports `0x40`–`0x43`). Moving to the LAPIC
timer/TSC-deadline mode is coupled to the same APIC work
`interrupt_controller_modern.md` describes as future SMP-motivated work —
they're not really separable: you'd adopt the Local APIC for interrupt
routing *and* get its timer "for free" as part of the same step, rather
than upgrading the timer in isolation.

---

## Self-check questions

1. Why does "tickless" scheduling require a per-core, arbitrary-deadline
   timer instead of a fixed-frequency rate generator like the PIT?
2. `timer.rs`'s doc already notes RDTSC isn't used for scheduling because
   it's frequency-dependent. What CPUID feature bit would tell you that
   concern no longer applies on a given CPU?
3. HPET is memory-mapped and found via an ACPI table, same discovery
   pattern as the Local APIC. Why do these newer mechanisms all lean on
   ACPI tables instead of fixed port addresses the way the PIT/PIC do?
4. ARM's generic timer is accessed via system registers, not MMIO or port
   I/O. What does that imply about how expensive reading the current time
   is, compared to x86's HPET (a real memory read)?
5. If OxideOS added Local APIC timer support tomorrow, what would still be
   needed before it could fully replace the PIT for scheduling — not just
   technically, but for the scheduler's actual mental model of "one core,
   one tick"?

---

## Sources

- `docs/study/hardware/8253_pit.md`, `kernel/src/kernel/arch/aarch64/timer.rs` (this codebase)
- Standard x86-64/ARMv8-A architecture references (Intel SDM Vol. 3 Ch. 10/18, ARM Architecture Reference Manual — Generic Timer chapter)
