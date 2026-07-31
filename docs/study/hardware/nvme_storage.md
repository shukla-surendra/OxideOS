# Storage — ATA/IDE PIO vs. NVMe over PCIe

**Companion to:** [ata_ide.md](ata_ide.md) (`kernel/src/kernel/drivers/ata.rs`).
Reference-only.

---

## What it is

ATA PIO's whole model — CPU blocks, reads 256 words one at a time off a
single data port, one command in flight — was designed around spinning
disks where seek time (milliseconds) dwarfed any transfer-protocol
overhead. Flash storage broke that assumption: NAND flash can service many
requests in parallel with microsecond latency, so a protocol that only
lets you have **one command outstanding at a time** becomes the
bottleneck, not the storage medium. NVMe was designed from scratch around
that fact.

---

## Side by side

| | ATA/IDE PIO | NVMe |
|---|---|---|
| **Bus** | Parallel ATA cable / emulated ISA-era ports | PCIe (typically x4 lanes on an M.2 slot) |
| **Command model** | One command register, one command in flight, CPU polls status bits (BSY/DRQ) | **Submission/Completion Queue pairs** — up to 65,535 queues, 65,536 commands deep *each* |
| **Data path** | CPU reads/writes the data port 2 bytes at a time (`in`/`out` instructions) | **DMA directly to/from host RAM** — the CPU never touches individual bytes; it just builds a queue entry and rings a doorbell |
| **How the device is told "go"** | Write the command register, then poll status | Write a command descriptor into a submission queue **in host RAM**, then one MMIO write to a **doorbell register** — the device DMAs the descriptor itself |
| **Completion notification** | CPU polls the status register (or takes a legacy IRQ, unused here) | Device DMAs a completion entry into a completion queue, then fires an **MSI-X** interrupt |
| **Probing a device** | `IDENTIFY` command (`0xEC`) → 512-byte struct describing the drive | **Identify** *admin* command — same concept, same name, vastly larger returned structure |
| **Practical throughput** | CPU-bound PIO — negligible by modern standards | Multiple **GB/s** — PCIe 4.0 SSDs commonly exceed 7 GB/s sequential reads; PCIe 5.0 drives pass 14 GB/s |
| **Queue depth** | 1 | Deep, multi-queue — designed so a fast SSD is never left idle waiting for the CPU to issue the next command |

The **doorbell-plus-ring-buffer** pattern is the real conceptual leap, and
it's worth naming explicitly because it recurs everywhere in modern
hardware, not just storage: NVMe, USB's xHCI (see
[`usb_xhci.md`](usb_xhci.md)), and even the virtio-mmio devices this
codebase's own aarch64 port already implements
(`arch/aarch64/virtio_blk.rs`) all use some variant of "write a descriptor
into a ring in RAM, ring a doorbell, device DMAs and processes
asynchronously, completion shows up in a second ring." ATA PIO predates
that pattern entirely — it's the one interface in this comparison series
with genuinely no queue or DMA concept at all.

---

## Where things stand in 2026

- **NVMe 2.0** is broadly implemented across 2025–2026 drives, adding
  improved power efficiency (PCIe APST — Autonomous Power State
  Transition, letting an idle SSD drop into a low-power state without
  host intervention) and better multipath I/O for enterprise use.
- **PCIe 4.0 SSDs** (~7–7.5 GB/s) are the mainstream sweet spot; **PCIe
  5.0** (>14 GB/s) drives exist but are mostly a professional/workstation
  buy. **PCIe 6.0** SSDs are not expected for consumer PCs until roughly
  2030 — it's currently an enterprise/AI-datacenter technology, per
  industry reporting (see sources).

---

## Why OxideOS uses ATA PIO

Zero queue/doorbell/interrupt-completion machinery to build, works
identically (and simply) under both QEMU and VirtualBox emulation, and —
worth calling out — the conceptual echo is real: OxideOS's own
`probe_disk()` sending `CMD_IDENTIFY` and parsing a fixed-layout response
for model string and sector count is *structurally* the same idea NVMe's
Identify admin command performs, just against a 512-byte structure instead
of NVMe's much larger one. Understanding `ata.rs`'s IDENTIFY flow is a
genuinely useful mental model for NVMe's, even though the transport
underneath is unrecognizable.

---

## Self-check questions

1. Why does a deep command queue matter so much more for NVMe than it ever
   would have for ATA PIO against a spinning disk?
2. What does "ring a doorbell" actually mean at the hardware level — what
   register operation triggers the device to start processing?
3. ATA PIO's data path has the CPU read every single word off the data
   port. NVMe's doesn't. What performs that data movement instead, and
   why does that free up the CPU?
4. Both `ata.rs`'s `IDENTIFY` and NVMe's Identify command return a
   fixed-size struct describing the drive. What's one piece of information
   NVMe's Identify structure would need to expose that ATA's has no
   concept of at all? (Hint: think about the queue model above.)
5. Why would PCIe 6.0 land in enterprise/AI datacenters years before
   consumer SSDs, when it's ostensibly a faster version of the exact same
   PCIe an NVMe drive already uses?

---

## Sources

- [NVMe SSD Buying Guide 2026: Speeds & Sizes — Ramseeker](https://ramseeker.com/nvme-ssd-buying-guide-2026-m2-speeds-storage/)
- [SSD Buying Guide 2026: PCIe Gen 4, Gen 5, and NVMe Speeds Explained](https://ramseeker.com/ssd-buying-guide-2026-pcie-gen4-gen5/)
- [PCIe 6.0 SSDs for PCs won't arrive until 2030 — Tom's Hardware](https://www.tomshardware.com/pc-components/ssds/pcie-6-0-ssds-for-pcs-wont-arrive-until-2030-costs-and-complexity-mean-pcie-5-0-ssds-are-here-to-stay-for-some-time)
- `docs/study/hardware/ata_ide.md` (this codebase)
