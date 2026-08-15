# 05 — GICv2, the timer IRQ, and getting off polling

**Status: done.** aarch64 takes real interrupts. The kernel idles in `wfi`
instead of spinning, and the generic timer delivers a 100 Hz tick through
GICv2 INTID 30.

Boot output on QEMU `virt` (Cortex-A72, GICv2, AAVMF):

```
✓ EL1 exception vectors installed
✓ Heap: 256 MB
✓ GICv2 up (288 INTIDs), timer IRQ armed at 100 Hz
✓ Timer IRQs delivered: 25 (spurious 0, unhandled 0)
...
Entering GUI loop (virtio-input polled)
```

PSCI (`arch/aarch64/psci.rs`) already landed with the boot work and was never
part of this step, despite the filename the status tables had been promising.

## Why this was the keystone step

Every aarch64 driver in the tree is **polled**, because nothing can raise an
interrupt yet:

| Driver | How it works today | What it costs |
|---|---|---|
| `timer.rs` | reads `CNTPCT_EL0` on demand | no tick *event* — nothing can be scheduled |
| `virtio_input.rs` | GUI loop drains the used ring every frame | input latency tied to frame rate |
| `virtio_blk.rs` | spins on the used ring per request | a disk read burns a core |

The clearest symptom is in `arch/cpu.rs`: `wait_for_interrupt()` executes
`wfe`, not `wfi`, with the comment explaining why — until the GIC is
programmed **no interrupt can ever become pending, so `wfi` would sleep
forever**. `timer::enable_event_stream()` exists purely to generate a ~0.5 ms
`wfe` wake heartbeat as a stand-in. Both are stopgaps this step deletes.

Downstream, B4 (scheduler context switch) and B5 (EL0 + SVC syscalls) are
blocked outright: preemptive multitasking *is* a timer interrupt that runs the
scheduler. Nothing above the driver layer can be ported until this lands.

## The hardware, concretely (QEMU `virt`)

| | Address / ID |
|---|---|
| GIC distributor (`GICD`) | `0x0800_0000` |
| GIC CPU interface (`GICC`) | `0x0801_0000` |
| INTID space | SGI `0–15`, PPI `16–31`, SPI `32+` |
| EL1 physical timer | PPI 14 → **INTID 30** |
| PL011 UART | SPI 1 → INTID 33 |
| PL031 RTC | SPI 2 → INTID 34 |
| virtio-mmio slot *N* | SPI `16+N` → INTID `48+N` |

Reachable through the Limine HHDM offset like every other MMIO block in the
port — same pattern as `serial.rs` and `virtio_input.rs`, no new mapping
machinery needed.

**Pin the GIC version in the Makefile.** The run targets currently pass
`-M virt` with no `gic-version`, and QEMU's default has changed across
releases; a host QEMU that picks GICv3 would hand us a completely different
programming model (system registers `ICC_*` instead of MMIO) — and GICv3's EL1
system-register interface additionally requires `ICC_SRE_EL1` to have been
enabled by firmware, which is not guaranteed. Change the aarch64 run targets
to `-M virt,gic-version=2` **before** writing any driver code, so the target
can't drift underneath the port.

## Implementation, in three commits

### B1a — `arch/aarch64/gic.rs`: the controller

Distributor init: enable groups in `GICD_CTLR`; per-INTID priority via
`GICD_IPRIORITYR`; route SPIs to CPU 0 via `GICD_ITARGETSR` (SGIs and PPIs are
banked per-CPU and need no targeting); unmask wanted INTIDs via
`GICD_ISENABLER`.

CPU interface init: **`GICC_PMR = 0xFF` first** — the priority mask powers on
at `0`, which blocks every interrupt, and forgetting it is the classic
"everything is configured and nothing fires" bug. Then enable `GICC_CTLR`.

Acknowledge/dispatch/EOI: read `GICC_IAR` for the INTID, dispatch, write the
*same value back* to `GICC_EOIR`. **INTID 1023 is the spurious marker** —
return without an EOI write.

### B1b — the IRQ path in `exceptions.rs`

Two changes to code that currently assumes exceptions never return:

1. **Vector slot 1, not slot 5.** The existing comment in `exceptions.rs`
   records that Limine enters the kernel with `SPSel=0`, so exceptions are
   taken through the **Current EL, SP_EL0** group — the live IRQ vector is
   `OXIDE_VECTOR 1`. Wiring slot 5 (`SP_ELx: IRQ`) is the natural mistake and
   produces an IRQ that silently never arrives.
2. **A real save/restore prologue.** Today `aarch64_exception_entry` funnels
   everything into a handler that dumps `ESR`/`ELR`/`FAR` and parks the CPU —
   nothing is saved because nothing returns. The IRQ path needs the
   caller-saved set (`x0`–`x18`, `x29`, `x30`) pushed on entry, popped on
   exit, and `eret`. Keep the existing park-and-dump behaviour for
   synchronous/SError; only IRQ gets the returning path.

**Mask the timer before installing vectors.** AAVMF uses the generic timer
during firmware execution and may hand over with `CNTP_CTL_EL0.ENABLE` set and
an interrupt already pending. Clear it in `exceptions::init()` before the
first `irq_enable()`, or the first unmask takes an immediate interrupt storm
into a handler that isn't ready.

### B1c — timer tick + retiring the stopgaps

Program the EL1 physical timer as a 100 Hz source, matching the x86 PIT rate
shared code already assumes: `CNTP_TVAL_EL0 = CNTFRQ_EL0 / 100`,
`CNTP_CTL_EL0 = ENABLE` with `IMASK` clear, reload `TVAL` in the handler.

**Keep `CNTPCT_EL0` as the time source; use the IRQ only as the event.**
The current `get_ticks()` derives time from the free-running counter, which
is monotonic and drift-free. Counting handler invocations instead would
accumulate error on every missed or late interrupt. The interrupt's job is to
*wake* the CPU and (later) run the scheduler, not to be the clock.

Then delete the stopgaps: `cpu::wait_for_interrupt()` goes `wfe` → `wfi`, and
`timer::enable_event_stream()` plus its call in `main.rs` Stage 3 goes away.

## Acceptance tests

1. ✅ `make KARCH=aarch64 run` — desktop boots, mouse and keyboard still work.
2. ✅ `cpu::wait_for_interrupt()` is `wfi`, and boot gets past a loop that
   sleeps in it 25 times.
3. ✅ Zero spurious, zero unhandled INTIDs across boot.
4. ✅ `make KARCH=x86_64` still builds — porting rule 3, now also enforced by
   a `build-aarch64` CI job in the other direction.

Test 2 is the one that actually proves the step, because it **fails closed**:
the boot-time wait loop sleeps in `wfi`, which no event stream can wake. A
misprogrammed GIC hangs the machine there instead of limping along, so
reaching the next line of output is itself the evidence.

## A drift bug the test caught

The first working version armed each tick with `CNTP_TVAL_EL0`, a *relative*
countdown reloaded inside the handler. It booted and interrupts arrived — but
the counters disagreed: **20 interrupts across 25 counter-derived ticks**, a
20% slow drift.

The cause is structural, not a tuning problem: a relative reload starts
counting when the handler *runs*, so every period silently absorbs interrupt
latency, and the error accumulates forever. Switching to the **absolute**
comparator `CNTP_CVAL_EL0`, advanced by a fixed interval per tick (with a
resync if we ever fall a full period behind), locks the interrupts to the
counter — 25 IRQs for 25 ticks, exactly.

This is why the two roles are kept separate. Had the IRQ been the clock, the
drift would have been invisible: the tick count and the interrupt count would
have been the same number, wrong together. Keeping `CNTPCT_EL0` as an
independent time source is what made the error observable at all — and it will
matter more once timeslices depend on it.

## Not in this step

- **IRQ-driven drivers.** virtio-input and virtio-blk keep polling; converting
  them (INTID `48+N` per slot) is a follow-up once the dispatch path is
  trusted.
- **Device tree.** MMIO addresses stay hardcoded, consistent with the rest of
  the port. Parsing the DTB QEMU already passes is what real-hardware support
  (the ARM counterpart of Track A's M6 — Raspberry Pi 4/5) will need, and it
  is its own step.
- **SMP.** GICv2 SGIs make multi-core plausible, but per-CPU interfaces and
  `ITARGETSR` routing beyond CPU 0 belong with Track A's M7.

## A note on what comes after (B2 — memory)

`docs/plan.md` describes B2 as "reuse portable allocator logic". Worth
adjusting that expectation before starting it: `mem/paging_allocator.rs` is
~1200 lines built directly on **x86 page-table descriptors** (`PRESENT` bit 0,
`WRITABLE` bit 1, `USER` bit 2, a COW marker in an AVL bit). ARM's descriptors
share none of that layout — different bits, different levels, `AF`/`AP`/`SH`/
`nG` attributes, granule-dependent shape.

What *is* portable is the physical-frame bookkeeping over the Limine memory
map and the `linked_list_allocator` heap on top; what isn't is map/unmap. So
B2 realistically starts as a refactor — split `mem/` into a portable frame
allocator plus a per-arch page-table backend — rather than a straight reuse.
The aarch64 build today runs on Limine's page tables with the bump heap in
`main.rs`, which is enough for the GUI but not for EL0 (B5), where TTBR0 user
tables become mandatory.
