# PCI Bus — Device Discovery

**Source:** `kernel/src/kernel/drivers/net/pci.rs`

---

## What it is

**PCI** (Peripheral Component Interconnect) is the bus standard that lets expansion
cards (network cards, sound cards, GPUs) communicate with the CPU. To talk to a PCI
device, you first need to find it — that's done by enumerating the **PCI
configuration space**.

Every PCI device has a 256-byte configuration space at a known address in
`(bus, device, function)` coordinate space. Reading the configuration space tells
you what the device is and where its registers live.

---

## I/O Ports

PCI uses just two I/O ports, called the **port I/O mechanism** (also called
"Configuration Mechanism #1"):

| Port | Name | Size | Purpose |
|------|------|------|---------|
| `0xCF8` | `PCI_CFG_ADDR` | 32-bit | Write the address of the register you want |
| `0xCFC` | `PCI_CFG_DATA` | 32-bit | Read/write the 32-bit register at that address |

---

## The Address Format

The 32-bit value written to `0xCF8` encodes the target device and register:

```
Bit 31      — Enable bit: must be 1
Bits 30:24  — Reserved (0)
Bits 23:16  — Bus number (0–255)
Bits 15:11  — Device number (0–31)
Bits 10:8   — Function number (0–7)
Bits 7:2    — Register offset (which 32-bit register, aligned to 4 bytes)
Bits 1:0    — Always 0 (registers are 32-bit aligned)
```

In OxideOS (`pci.rs` line 16–22):
```rust
fn pci_addr(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    (1u32 << 31)                // enable bit
        | ((bus  as u32) << 16)
        | ((dev  as u32) << 11)
        | ((func as u32) << 8)
        | ((offset & 0xFC) as u32) // mask low 2 bits → 4-byte aligned
}
```

To read register at offset 0x04 from device (bus=0, dev=3, func=0):
```
addr = 0x80000000 | (0 << 16) | (3 << 11) | (0 << 8) | 0x04
     = 0x80001804
```

---

## Standard Configuration Space Header

Every PCI device's configuration space starts with a standard 64-byte header:

| Offset | Size | Field |
|--------|------|-------|
| `0x00` | 16-bit | **Vendor ID** (0xFFFF = no device present) |
| `0x02` | 16-bit | **Device ID** |
| `0x04` | 16-bit | Command register |
| `0x06` | 16-bit | Status register |
| `0x08` | 8-bit | Revision ID |
| `0x09` | 24-bit | Class code (class/subclass/prog-if) |
| `0x0E` | 8-bit | Header type |
| `0x10` | 32-bit | **BAR0** (Base Address Register 0) |
| `0x14` | 32-bit | BAR1 |
| `0x18`–`0x24` | 32-bit each | BAR2–BAR5 |
| `0x3C` | 8-bit | Interrupt line (which IRQ this device uses) |

**Vendor ID = 0xFFFF** means no device is present at that slot — this is the
signal OxideOS uses to skip empty slots during enumeration.

---

## BARs — Base Address Registers

A BAR tells you where the device's registers live. There are two types:

**I/O BAR (bit 0 = 1):**
The device's registers are at an I/O port address. The base address is in bits 2–31.
```rust
// pci.rs io_bar() — line 75
pub fn io_bar(&self, bar: u8) -> Option<u16> {
    let val = read32(self.bus, self.dev, self.func, 0x10 + bar * 4);
    if val & 1 != 0 { Some((val & 0xFFFC) as u16) } else { None }
}
```

**Memory BAR (bit 0 = 0):**
The device's registers are at a physical memory address (MMIO).
OxideOS's network drivers use I/O BARs, so MMIO BARs are not used here.

---

## Bus Enumeration

OxideOS scans bus 0, all 32 device slots, function 0:
```rust
// pci.rs scan() — finds a device matching vendor:device IDs
pub fn scan(vendor: u16, device: u16) -> Option<PciDevice> {
    for dev in 0..32u8 {
        let id = read32(0, dev, 0, 0x00);    // offset 0x00 = vendor:device
        let vid = id as u16;                 // low 16 bits = vendor ID
        let did = (id >> 16) as u16;         // high 16 bits = device ID
        if vid == 0xFFFF { continue; }       // no device
        if vid == vendor && did == device {
            return Some(PciDevice { bus: 0, dev, func: 0, vendor: vid, device: did });
        }
    }
    None
}
```

To find the RTL8139 network card:
```rust
pci::scan(0x10EC, 0x8139)  // Realtek vendor=0x10EC, RTL8139 device=0x8139
```

---

## Enabling Bus Mastering

Before a device can use DMA or send interrupts, its **Command Register** (offset 0x04)
must have certain bits set:

```
Bit 0: I/O Space Enable      — allow I/O BAR access
Bit 1: Memory Space Enable   — allow MMIO BAR access
Bit 2: Bus Master Enable     — allow the device to initiate DMA transfers
```

```rust
// Enable I/O + Bus Master for the NIC:
let cmd = pci::read16(bus, dev, func, 0x04);
pci::write16(bus, dev, func, 0x04, cmd | 0x0005);
```

---

## Why this matters for the network driver

The RTL8139 driver (`rtl8139.rs`) uses PCI to:
1. Find the card: `pci::scan(0x10EC, 0x8139)`
2. Get its I/O base address: `device.io_bar(0)` → e.g. `0xC000`
3. Enable it: set bus master + I/O space bits in command register
4. Then talk to the card via ports `0xC000`–`0xC0FF`

Without PCI, the driver would have to hardcode the I/O base address — which
differs between virtual machines and is unknown at compile time.

---

## Common gotchas

**1. Forgetting the enable bit (bit 31) in the address.**
Without bit 31, the PCI host bridge ignores the access and returns 0xFFFFFFFF.
Everything looks like "no device present."

**2. Reading offset 0x00 returns 0xFFFF for the vendor ID.**
This is the "no device" sentinel. You must skip these slots or you'll try to
initialize a non-existent device.

**3. BAR size vs. BAR value.**
The value in a BAR is the base address of the device's register space. The *size*
of the register space is determined by writing all-ones to the BAR and reading back —
but OxideOS doesn't need to do this since the driver knows the register layout.

**4. Only bus 0 is scanned.**
OxideOS only scans bus 0 with function 0. A full PCI enumeration would also scan
buses created by PCI-PCI bridges and all 8 functions per device. This is sufficient
for QEMU/VirtualBox since all emulated devices appear on bus 0.

---

## Self-check questions

1. What does it mean when `read32(0, dev, 0, 0)` returns `0xFFFFFFFF`?
2. What is the difference between an I/O BAR and a Memory BAR?
3. Why must bus mastering be explicitly enabled before DMA?
4. A device is at bus=0, device=7, function=0. What is the 32-bit CONFIG_ADDRESS
   value needed to read its Vendor ID (offset 0x00)?
5. Why does OxideOS only scan bus 0? What devices could be missed?
