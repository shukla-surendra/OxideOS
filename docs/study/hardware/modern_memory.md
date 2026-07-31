# Memory — OxideOS's Flat Physical Map vs. Real DRAM Subsystems

**Source:** `kernel/src/kernel/mem/paging_allocator.rs` — see also
[`../04_memory.md`](../04_memory.md) and
[`../../oxide_cocepts/03_memory_management.md`](../../oxide_cocepts/03_memory_management.md)
for how OxideOS's frame allocator itself works. This doc is about the
*physical DRAM* underneath that allocator, which OxideOS never has any
visibility into at all.

---

## What OxideOS assumes about RAM

`PhysicalFrameAllocator::init()` reads Limine's memory map — a flat list
of `(base, length, type)` regions — and tracks every 4 KB frame in the
first 256 MB as either free or used, with a bitmap. That's the entire
model: RAM is a uniform, flat, undifferentiated address space where every
frame is interchangeable and costs the same to access. This is a
deliberate simplification `docs/memory.md` already documents as a known
bound — but it's also, not coincidentally, **exactly the abstraction every
real memory controller presents to software**, even though the physical
reality underneath is far from flat.

---

## What's actually underneath "RAM" on a real machine

| Concept | What it means | Visible to OxideOS? |
|---|---|---|
| **Integrated memory controller** | Since ~2003 (AMD)/2008 (Intel), the memory controller lives inside the CPU package itself — there's no separate "Northbridge" chip anymore | No — this is exactly why a flat physical address space works: the controller does the real work invisibly |
| **Channels** | Modern controllers run multiple DRAM channels in parallel (dual/quad-channel configurations) and interleave addresses across them to multiply effective bandwidth | No — interleaving is done entirely in hardware; software just sees one contiguous range |
| **Ranks/banks** | Within a channel, DIMMs are organized into ranks and banks that can be accessed somewhat independently, which is what lets a controller pipeline multiple in-flight requests | No |
| **NUMA** (multi-socket servers) | Each CPU socket has memory physically local to it; accessing another socket's memory is slower. A NUMA-aware allocator prefers local frames | No — irrelevant to any single-socket QEMU/VirtualBox target anyway |
| **ECC** (Error-Correcting Code) | Server/workstation RAM carries extra bits per word to detect/correct single-bit flips (cosmic rays, electrical noise) transparently in hardware | Partially — a real OS still has to handle the **machine-check exception** path when ECC reports an *uncorrectable* error; OxideOS's panic handler treats every CPU exception the same way and has no ECC-specific handling |
| **Apple's Unified Memory Architecture** | On Apple Silicon, LPDDR5/5X is soldered directly on-package next to the SoC die, and CPU cores, GPU cores, and the Neural Engine all share the *same* physical RAM pool — no separate VRAM to manage at all | N/A — OxideOS doesn't run on Apple Silicon and has no GPU driver regardless (see `modern_gpu_display.md`) |

**The key insight:** almost none of this is something software is
*supposed* to see. The memory controller's entire job is presenting a
uniform flat address space to the CPU — channel interleaving, rank
scheduling, and ECC correction all happen below the abstraction level any
normal allocator (including OxideOS's bitmap frame allocator) operates
at. This is genuinely one of the few places in this whole doc series where
"the legacy approach and the modern approach look at hardware the same
way" — a flat physical-address bitmap is still exactly how Linux, Windows,
and macOS treat ordinary RAM for everyday allocation. NUMA-awareness is
the one place that changes, and it only matters on multi-socket servers —
out of scope for anything a single-socket QEMU/VirtualBox target
(or a MacBook) would ever expose.

---

## Where the physical standard itself is heading (2026)

- **LPDDR5X** is the current mainstream mobile/laptop standard, now also
  appearing in the new **CAMM2** module form factor (replacing traditional
  SO-DIMM sticks) — the Lenovo ThinkPad P1 Gen 7 is cited as the first
  commercially available laptop shipping LPDDR5X in CAMM2 form.
- **LPDDR6** is JEDEC-finalized (JESD209-6, ratified July 2025) — the
  first completed DDR6-generation spec, focused on low power/mobile use,
  with broader device adoption expected through 2026 as supply matures.
- **DDR6** (the desktop/server DIMM standard) is still in CPU-platform
  validation with Intel and AMD as of 2026; real DDR6 systems aren't
  expected until late 2026 at the earliest, more likely 2027, with wide
  adoption further out still — PCs and laptops are expected to stay on
  DDR5/LPDDR5X/LPDDR6 considerably longer than HPC/AI clusters, which may
  adopt DDR6 earlier for the bandwidth.

None of this changes anything about how OxideOS's allocator works — a
DDR6 system still presents a flat physical address space via Limine's
memory map exactly like DDR5 does today. The generational shift is
entirely about bandwidth and power efficiency at the electrical level,
invisible above the memory controller.

---

## The one place this *would* start to matter

If OxideOS ever targeted true multi-socket SMP (see
`interrupt_controller_modern.md`'s APIC/IPI discussion — SMP is the
recurring prerequisite this whole series keeps circling back to), NUMA
would become the first real crack in the flat-memory assumption: the
frame allocator would need to know *which* socket a given physical range
belongs to, and the scheduler would want to prefer scheduling a task's
memory allocations on frames local to the core it's running on. That's a
strictly harder problem than anything `paging_allocator.rs` solves today,
and — like SMP itself — is explicitly out of scope for the single-core
target this kernel is built for.

---

## Self-check questions

1. Why can a memory controller's channel interleaving stay completely
   invisible to software, when something like NUMA locality can't?
2. `paging_allocator.rs`'s bitmap treats every frame as equally "costly"
   to access. Under what real-hardware condition would that assumption
   actually be false?
3. What's the practical difference between a *correctable* and
   *uncorrectable* ECC error, and why does only the second one need to
   reach the OS at all?
4. Why does Apple's Unified Memory Architecture make "no separate VRAM to
   manage" true, when a discrete GPU with its own VRAM would make it
   false?
5. If DDR6 desktop systems arrive in 2027, what — if anything — would
   OxideOS's `paging_allocator.rs` need to change to run correctly on one?

---

## Sources

- [CAMM2 is replacing the RAM stick as we know it — XDA Developers](https://www.xda-developers.com/camm2-will-replace-ram-sticks-in-your-next-laptop/)
- [JEDEC Previews LPDDR6 Roadmap — TechPowerUp](https://www.techpowerup.com/348441/jedec-previews-lpddr6-roadmap-512-gb-densities-and-socamm2-standard-in-development)
- [DDR6 Release Date: Is Next-Gen RAM Coming Out in 2026? — DirectMacro](https://directmacro.com/blog/post/ddr6-release-date)
- `kernel/src/kernel/mem/paging_allocator.rs`, `docs/memory.md`, `docs/oxide_cocepts/03_memory_management.md` (this codebase)
