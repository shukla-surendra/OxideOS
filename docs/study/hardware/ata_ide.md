# ATA/IDE — Disk Controller

**Source:** `kernel/src/kernel/drivers/ata.rs`

---

## What it is

**ATA** (Advanced Technology Attachment, also called IDE) is the protocol that
PC hard drives used from the late 1980s until SATA replaced it in the mid-2000s.
QEMU and VirtualBox still emulate ATA for simplicity, making it the right choice
for an educational OS.

OxideOS uses **PIO mode** (Programmed I/O) — the CPU directly reads and writes
disk data through I/O ports, 2 bytes (one word) at a time. This is the simplest
possible approach: no DMA, no interrupts for data transfer, the CPU blocks while
waiting for each sector to arrive.

---

## Physical layout

A PC has two ATA buses (primary and secondary), each supporting two drives
(master and slave) — four drive positions total:

```
DISKS[0] = Primary Master   (IO=0x1F0, CTRL=0x3F6, slave=false)
DISKS[1] = Primary Slave    (IO=0x1F0, CTRL=0x3F6, slave=true)
DISKS[2] = Secondary Master (IO=0x170, CTRL=0x376, slave=false)
DISKS[3] = Secondary Slave  (IO=0x170, CTRL=0x376, slave=true)
```

---

## I/O Ports (Primary bus, base=0x1F0)

| Port | Offset | Read | Write |
|------|--------|------|-------|
| `0x1F0` | `+0` | Data (16-bit words) | Data (16-bit words) |
| `0x1F1` | `+1` | Error register | Features register |
| `0x1F2` | `+2` | Sector count | Sector count |
| `0x1F3` | `+3` | LBA bits 0–7 | LBA bits 0–7 |
| `0x1F4` | `+4` | LBA bits 8–15 | LBA bits 8–15 |
| `0x1F5` | `+5` | LBA bits 16–23 | LBA bits 16–23 |
| `0x1F6` | `+6` | Drive/head select | Drive/head select |
| `0x1F7` | `+7` | Status register | Command register |
| `0x3F6` | ctrl | Alt status (no side effects) | Device control |

Secondary bus uses `0x170`–`0x177` and `0x376` for the same registers.

---

## Key Registers

### Status Register (port 0x1F7, read)

```
Bit 7 (BSY — Busy):
  1 = drive is busy; don't send commands or read data
  0 = drive is ready for commands

Bit 3 (DRQ — Data Request):
  1 = drive is ready to transfer data (read or write a word from 0x1F0)
  0 = not ready for data transfer

Bit 0 (ERR — Error):
  1 = previous command ended in error; check Error register (0x1F1)
```

OxideOS waits for BSY=0 before commands, waits for DRQ=1 before data transfer:
```rust
// ata.rs line 98–101
unsafe fn wait_busy(io: u16, timeout: u32) -> bool {
    for _ in 0..timeout {
        if disk_inb(io, OFF_CMD) & SR_BSY == 0 { return true; }
    }
    false  // timeout
}
```

### Drive/Head Register (port 0x1F6, write)

```
Bit 6 (LBA mode): 1 = use LBA addressing (not CHS)
Bit 4 (DEV):      0 = master, 1 = slave
Bits [3:0]:       LBA bits 24–27 (for LBA28 addressing)
```

To select the master drive in LBA mode:
```rust
let dev_sel: u8 = if slave { 0xB0 } else { 0xA0 };
// 0xA0 = 1010 0000
//   bit 7: 1 (obsolete, always 1)
//   bit 6: 0 ... wait, let me recheck
// Actually:
// 0xA0 = 1010 0000: bit7=1, bit6=0(LBA?), bit5=1, bit4=0(master)
// 0xB0 = 1011 0000: bit4=1 (slave)
// LBA bit is set in the read/write command itself
disk_outb(io, OFF_DRVHD, dev_sel);
```

---

## ATA Commands

| Command | Hex | What it does |
|---------|-----|--------------|
| `CMD_IDENTIFY` | `0xEC` | Return 512 bytes of drive info (model, sector count, features) |
| `CMD_READ` | `0x20` | Read sectors using LBA28 (up to 28-bit LBA, ~128 GB) |
| `CMD_WRITE` | `0x30` | Write sectors using LBA28 |
| `CMD_FLUSH` | `0xE7` | Flush write cache to disk |
| `CMD_READ48` | `0x24` | Read sectors using LBA48 (up to 48-bit LBA, 128 PB) |
| `CMD_WRITE48` | `0x34` | Write sectors using LBA48 |
| `CMD_FLUSH48` | `0xEA` | Flush cache (LBA48 variant) |

---

## IDENTIFY — probing a drive

`CMD_IDENTIFY` returns a 256-word (512-byte) response packed with drive information.
OxideOS reads it in `probe_disk()`:

```rust
// Send IDENTIFY command
disk_outb(io, OFF_CMD, CMD_IDENTIFY);
delay400ns(ctrl);          // wait 400 ns for drive to process

// Wait for DRQ (data ready) or ERR
wait_drq(io, 1_000_000);

// Read 256 words (512 bytes) from data port
let mut id = [0u16; 256];
for w in id.iter_mut() { *w = disk_inw(io, OFF_DATA); }

// Word 60–61: LBA28 sector count
let lba28 = (id[60] as u64) | ((id[61] as u64) << 16);

// Word 83 bit 10: LBA48 supported?
let lba48 = (id[83] & (1 << 10)) != 0;

// Words 100–103: LBA48 sector count
let sectors = (id[100] as u64) | ((id[101] as u64) << 16)
            | ((id[102] as u64) << 32) | ((id[103] as u64) << 48);

// Words 27–46: model string (big-endian byte pairs)
for i in 0..20usize {
    let w = id[27 + i];
    model[i*2]   = (w >> 8) as u8;
    model[i*2+1] = (w & 0xFF) as u8;
}
```

---

## Reading a sector (LBA28)

A **sector** is 512 bytes, the minimum addressable unit on a disk.
**LBA** (Logical Block Addressing) numbers sectors from 0 to N-1 sequentially.

To read LBA sector 42 from the master drive:

```
1. Wait for BSY=0
2. Write to drive/head: 0xE0 | (lba >> 24 & 0x0F)  ← LBA bits 24-27 + LBA mode
3. Write sector count:  1
4. Write LBA byte 0:    lba & 0xFF
5. Write LBA byte 1:    (lba >> 8) & 0xFF
6. Write LBA byte 2:    (lba >> 16) & 0xFF
7. Write command:       0x20 (CMD_READ)
8. Wait for DRQ=1
9. Read 256 words from data port (0x1F0), 2 bytes each = 512 bytes total
```

OxideOS implements this in `read_sectors()` in `ata.rs`.

---

## The 400ns delay

After selecting a drive or sending a command, the ATA spec requires a 400 ns
settling time before reading the status register. OxideOS implements this
by reading the alternate status register (0x3F6) four times — each read takes
~100 ns on ISA-era hardware:

```rust
unsafe fn delay400ns(ctrl: u16) {
    for _ in 0..4 { let _ = inb(ctrl); }
}
```

---

## ATAPI signature check

ATAPI devices (CD-ROM drives) respond to IDENTIFY but with specific non-zero
values in the LBA1/LBA2 registers. OxideOS detects and skips them:

```rust
let sig1 = disk_inb(io, OFF_LBA1);
let sig2 = disk_inb(io, OFF_LBA2);
if (sig1 != 0 || sig2 != 0) && !(sig1 == 0x3C && sig2 == 0xC3) {
    // ATAPI or SATA device — skip
    return;
}
```

`0x3C/0xC3` is the SATA test-pattern signature — also skipped.

---

## Common gotchas

**1. Not waiting for BSY=0 before sending a command.**
If the drive is busy processing a previous command and you send a new one,
the behavior is undefined. Always wait for BSY=0.

**2. Reading the status register has a side effect on some hardware.**
Reading port `0x1F7` (command/status) clears the interrupt pending flag.
That's why the alternate status at `0x3F6` exists — same status bits, no side effects.
OxideOS reads `0x3F6` for the 400ns delay for exactly this reason.

**3. PIO is CPU-blocking.**
The entire CPU is busy while reading 256 words from `0x1F0`. For a single 512-byte
sector this is ~1 µs on modern hardware — negligible. For large transfers
(megabytes), DMA would be used instead. OxideOS uses PIO exclusively.

**4. LBA28 vs LBA48.**
LBA28 supports 2^28 = 268,435,456 sectors × 512 bytes = 128 GB max.
Modern disks larger than 128 GB require LBA48. OxideOS detects LBA48 support
from the IDENTIFY response and uses the extended commands automatically.

---

## Self-check questions

1. Why is there both a status register (0x1F7) and an alternate status (0x3F6)?
2. What does `delay400ns()` actually wait for? Why 4 reads and not 1?
3. If a read of sector 0 returns all zeros, does that mean the disk is empty?
4. What is the maximum disk size addressable with LBA28? Show the calculation.
5. Why does OxideOS check for the ATAPI signature after sending IDENTIFY?
