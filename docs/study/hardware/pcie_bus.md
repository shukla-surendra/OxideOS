# PCIe — What Replaced the PCI Bus

**Companion to:** [pci_bus.md](pci_bus.md) (`kernel/src/kernel/drivers/net/pci.rs`).
Reference-only.

---

## What it is

Legacy PCI is, literally, a **bus** — a shared set of parallel wires every
device on it is electrically connected to, taking turns via arbitration.
PCIe replaced the wire topology entirely (point-to-point serial links in
a tree) while deliberately keeping the *software model* — device
enumeration, the 256-byte configuration header, vendor/device IDs, BARs —
close enough to identical that old PCI-scanning code conceptually still
applies. That backward-compatible software layer is exactly why
`pci.rs`'s `find_device(0x10EC, 0x8139)` pattern still works against
QEMU's RTL8139, e1000, and virtio devices even though all of them are
presented over a PCIe-compatible virtual root complex under the hood.

---

## Bus topology: shared wire vs. point-to-point tree

| | Legacy PCI | PCIe |
|---|---|---|
| **Topology** | Shared parallel bus — every device electrically shares the same wires | Point-to-point serial **lanes**, tree-structured (root complex → switches → endpoints) |
| **Bandwidth sharing** | All devices on a bus divide up one shared ~266 MB/s pool | Each device gets **dedicated** lanes — no arbitration with other devices |
| **Width** | Fixed 32-bit (or 64-bit) parallel bus | Scalable — x1, x4, x8, x16 lanes negotiated per device at link training (a GPU gets x16, an NVMe drive typically x4, a small peripheral x1) |
| **Clock** | Shared bus clock (33/66 MHz) | Each lane self-clocked (embedded clock in the serial signal) |

---

## Config space: same shape, new access mechanism

PCIe's config space **is** legacy PCI's 256-byte header, byte-for-byte
compatible at the same offsets `pci_bus.md` documents (vendor ID at
`0x00`, BARs starting at `0x10`, etc.) — extended to 4 KB per function for
new capabilities. What changed is how you *reach* it:

| | Legacy PCI ("Mechanism #1") | PCIe (ECAM) |
|---|---|---|
| **Access** | Two I/O ports: write the target address to `0xCF8`, read/write the value at `0xCFC` | **Enhanced Configuration Access Mechanism** — a flat memory-mapped region, found via the ACPI **MCFG** table |
| **How a modern OS reads a register** | `outl(0xCF8, addr); inl(0xCFC)` | A plain memory load at `mcfg_base + (bus<<20 \| dev<<15 \| func<<12 \| offset)` |
| **Why it changed** | Two port-I/O round trips per access is slow, and the 32-bit address format (`pci_bus.md`'s bit layout) has no room for the 4 KB extended config space PCIe added | ECAM maps the *entire* config space of *every* function into one linear memory range — one memory read, no round trip, room for the extended registers |

`pci.rs` uses Mechanism #1 exclusively, which is why it can only reach the
first 256 bytes of any device's config space — enough for everything
OxideOS's drivers currently need (vendor/device ID, command register,
BARs), but not the extended capability structures (MSI-X capability, PCIe
Advanced Error Reporting, etc.) that live past offset `0x100` and require
ECAM to reach at all.

---

## Interrupts: the other big change (MSI/MSI-X)

Legacy PCI devices signal interrupts over shared physical **INTx** lines,
routed through the PIC or I/O APIC exactly like any other device IRQ (see
[`interrupt_controller_modern.md`](interrupt_controller_modern.md)) — and
because the lines are *shared* across multiple PCI devices, the OS has to
check "was it actually you?" on every shared-IRQ fire. PCIe replaced this
with **MSI/MSI-X (Message Signaled Interrupts)**: a device signals an
interrupt by performing an ordinary memory *write* to a special address
the interrupt controller is watching — no physical line, no sharing, no
ambiguity about which device fired. This is the exact mechanism this
folder's `modern_wifi_nics.md` doc flags on the aarch64 virtio drivers
(`virtio_blk.rs`/`virtio_input.rs`) as the reason they'd use "many
independent interrupt vectors, one bus transaction each" once GIC support
lands — MSI-X (or its ARM ITS equivalent) is what makes that possible.
RTL8139, being a legacy-era PCI device even when instantiated over QEMU's
PCIe-capable virtual root complex, still uses old-style INTx — which is
part of why OxideOS's `rtl8139.rs` polls `REG_ISR` rather than relying
purely on a clean interrupt vector.

---

## Generations: how much bandwidth per lane

Bandwidth roughly doubles each generation (with a bigger jump at Gen6 from
a different signal encoding, PAM4):

| Generation | Per-lane bandwidth (per direction) | 2026 status |
|---|---|---|
| Gen3 | ~985 MB/s | Long-mainstream |
| Gen4 | ~1.97 GB/s | Mainstream consumer sweet spot (SSDs, most laptops) |
| Gen5 | ~3.94 GB/s | Available, mostly a workstation/enthusiast buy |
| Gen6 | ~7.6 GB/s (PAM4 encoding) | **Enterprise/AI-datacenter only** — not expected in consumer PCs until roughly 2030, per industry reporting |

---

## Why OxideOS uses legacy Mechanism #1

No ACPI MCFG table parsing required — QEMU's `-M pc` machine type (and
VirtualBox) both still answer the legacy `0xCF8`/`0xCFC` port-I/O
mechanism for full backward compatibility, even for devices that are
technically PCIe underneath. That's the whole reason `pci.rs`'s
30-year-old access method still finds real, modern virtio-mmio-adjacent
devices in this codebase's own QEMU targets — the software-compatibility
layer PCIe's designers built in is doing exactly the job it was designed
for.

---

## Self-check questions

1. Why can two devices on legacy PCI's shared bus not both use their full
   theoretical bandwidth at the same time, while two PCIe devices with
   dedicated lanes can?
2. `pci.rs`'s `pci_addr()` builds a 32-bit CONFIG_ADDRESS value. Why does
   that format have no room to address PCIe's extended 4 KB config space,
   and what mechanism replaces it?
3. Why does MSI/MSI-X not require the interrupt controller to have a
   dedicated physical pin for every possible device, the way the 8259A's
   IRQ0–IRQ15 lines did?
4. RTL8139 uses legacy INTx interrupts even under QEMU's PCIe-capable
   virtual machine. What does that tell you about whether "the device is
   plugged into a PCIe slot" and "the device uses PCIe-native interrupt
   delivery" are actually the same thing?
5. Why would a GPU negotiate x16 lanes at link training while an NVMe SSD
   only negotiates x4, given both could physically fit in a slot with more
   lanes available?

---

## Sources

- `docs/study/hardware/pci_bus.md`, `docs/study/hardware/modern_wifi_nics.md` (this codebase)
- [PCIe 6.0 SSDs for PCs won't arrive until 2030 — Tom's Hardware](https://www.tomshardware.com/pc-components/ssds/pcie-6-0-ssds-for-pcs-wont-arrive-until-2030-costs-and-complexity-mean-pcie-5-0-ssds-are-here-to-stay-for-some-time)
- [PCIe 5.0, USB4, and Wi-Fi 7 on Motherboards in 2026 — Newegg Insider](https://www.newegg.com/insider/pcie-5-0-usb4-and-wi-fi-7-understanding-motherboard-connectivity-standards-in-2026/)
