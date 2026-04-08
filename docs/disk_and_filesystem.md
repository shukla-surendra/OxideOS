# Disk Support & FAT16 Filesystem

This document covers how OxideOS attaches a persistent disk in QEMU, how the ATA PIO driver
detects it, and how the FAT16 filesystem driver exposes files to userspace.

---

## 1. Why `-M pc` Instead of `-M q35`

QEMU supports several machine types. The default OxideOS targets (`run`, `run-gui`) use
`-M q35`, which emulates the Intel ICH9 chipset and boots via UEFI (OVMF firmware).

**The problem:** OVMF on q35 programs the ICH9 IDE controller into *native PCI mode*. In
native PCI mode the IDE data registers are assigned PCI BARs rather than the fixed legacy
addresses 0x1F0–0x1F7. Reading port 0x1F7 (ATA status) returns `0xFF` — the floating-bus
sentinel — because nothing is listening there.

**The fix:** Use `-M pc`, which emulates the older i440FX + PIIX4 chipset. The PIIX4 IDE
controller powers up in *legacy compatibility mode* and unconditionally exposes the primary
channel at 0x1F0–0x1F7 (control port 0x3F6). This is exactly what the ATA PIO driver
expects.

| Machine type | Chipset       | IDE mode           | 0x1F0 accessible? |
|--------------|---------------|--------------------|-------------------|
| `-M q35`     | ICH9          | Native PCI (OVMF)  | No — reads 0xFF   |
| `-M pc`      | i440FX/PIIX4  | Legacy compat      | Yes               |

The `run-bios` Makefile target uses `-M pc`:

```makefile
.PHONY: run-bios
run-bios: $(IMAGE_NAME).iso
    qemu-system-x86_64 \
        -M pc \
        -serial stdio \
        -cdrom $(IMAGE_NAME).iso \
        -boot d \
        $(if $(wildcard $(DISK_IMAGE)),-drive file=$(DISK_IMAGE),format=raw,if=ide) \
        $(QEMUFLAGS)
```

---

## 2. Creating the Disk Image

The disk image is a raw 4 MB FAT16 volume. The `disk` Makefile target creates it:

```bash
make disk
```

This runs:

```makefile
dd if=/dev/zero bs=512 count=8192 of=oxide_disk.img
mformat -i oxide_disk.img -F -v OXIDEDISK ::
```

- **`dd`** — writes 8192 × 512 B = 4 MB of zeroes.
- **`mformat`** — from the `mtools` package; formats the image as FAT16 with volume label
  `OXIDEDISK`. (`mkfs.fat` from dosfstools is an alternative if mtools is unavailable.)

The image is created once and persists across builds. Delete it with `make clean-disk`.

> **Note:** `oxide_disk.img` is listed in `.gitignore`. Commit it separately if you want to
> ship pre-populated disk contents.

---

## 3. Attaching the Disk to QEMU

The Makefile detects whether `oxide_disk.img` exists and appends the drive flag automatically:

```makefile
comma     := ,
DISK_FLAG := $(if $(wildcard $(DISK_IMAGE)),-drive file=$(DISK_IMAGE)$(comma)format=raw$(comma)if=ide)
```

The `comma` variable must be defined *before* `DISK_FLAG` because `:=` (simply-expanded
assignment) evaluates the right-hand side immediately. If `comma` came after, `$(comma)`
would expand to an empty string and produce a malformed QEMU argument.

When the image is present, QEMU receives:

```
-drive file=oxide_disk.img,format=raw,if=ide
```

This attaches it as the primary IDE master — the same device the ATA driver probes.

---

## 4. ATA PIO Driver (`kernel/src/kernel/ata.rs`)

The driver speaks to the disk over x86 I/O ports. No DMA or IRQs are used — all transfers
are polled.

### Port Map (Primary Bus)

| Port   | Read         | Write      |
|--------|--------------|------------|
| 0x1F0  | Data (16-bit)| Data       |
| 0x1F1  | Error        | Features   |
| 0x1F2  | Sector count | Sector count |
| 0x1F3–0x1F5 | LBA[0–23] | LBA[0–23] |
| 0x1F6  | Drive/Head   | Drive/Head |
| 0x1F7  | Status       | Command    |
| 0x3F6  | Alt-Status   | Device Control |

### Initialisation (`ata::init()`)

Called once from `kmain` after interrupts are enabled.

1. **Software Reset** — write `SRST | nIEN` (0x06) to the Device Control port, hold for
   ~20 port reads (~1 µs), then clear SRST while keeping `nIEN` (0x02). `nIEN` disables
   the drive's IRQ line so we never get a spurious interrupt during probing.

2. **BSY poll** — spin up to ~10 M iterations waiting for the BSY flag to clear. On a cold
   reset this typically takes < 1 ms in QEMU.

3. **Floating bus check** — if status reads `0xFF`, no controller is present.

4. **Select master** — write `0xA0` to the Drive/Head register.

5. **IDENTIFY** — issue command `0xEC`. Read status; `0x00` means no drive. Poll until
   BSY clears. Check LBA1/LBA2: non-zero means ATAPI (CD-ROM) — skip. Wait for DRQ. Read
   the 256-word IDENTIFY data block.

6. **Sector count** — words 60–61 of IDENTIFY hold the 28-bit LBA sector count. Stored in
   the `DISK_SECTORS` static.

### Read / Write

```rust
pub unsafe fn read_sector(lba: u32, buf: &mut [u8; 512]) -> bool { ... }
pub unsafe fn write_sector(lba: u32, buf: &[u8; 512]) -> bool { ... }
```

Both follow the same pattern:

1. Wait for BSY to clear.
2. Load Drive/Head, Sector Count, LBA bytes 0–2.
3. Issue READ (0x20) or WRITE (0x30).
4. For reads: wait BSY, wait DRQ, read 256 × 16-bit words.
5. For writes: wait DRQ, write 256 × 16-bit words, issue FLUSH CACHE (0xE7), wait BSY.

### Public API

```rust
pub fn is_present() -> bool       // true after successful init
pub fn sector_count() -> u32      // total LBA28 sectors
pub unsafe fn read_sector(lba: u32, buf: &mut [u8; 512]) -> bool
pub unsafe fn write_sector(lba: u32, buf: &[u8; 512])    -> bool
```

---

## 5. FAT16 Filesystem Driver (`kernel/src/kernel/fat.rs`)

A read-only FAT16 driver layered on top of the ATA sector reads.

### On-Disk Layout

```
Sector 0        Boot Sector (BPB)
Sectors 1..R    FAT tables (two copies)
Sectors R..D    Root Directory (fixed 512 entries × 32 bytes)
Sectors D..     Data clusters (clusters 2, 3, …)
```

Key BPB fields read at mount time:

| Offset | Size | Field              |
|--------|------|--------------------|
| 0x0B   | 2    | Bytes per sector   |
| 0x0D   | 1    | Sectors per cluster|
| 0x0E   | 2    | Reserved sectors   |
| 0x10   | 1    | Number of FATs     |
| 0x11   | 2    | Root entry count   |
| 0x16   | 2    | Sectors per FAT    |

### File Descriptor Range

FAT files occupy FD slots **64–79** (16 slots). RAM-FS files use lower FDs; pipes use
80–95. The dispatcher in `syscall.rs` routes based on FD value.

### Path Routing

Paths beginning with `/disk/` are always directed to the FAT driver. Bare paths (e.g.
`hello.txt`) are also sent to FAT when a disk is present, so userspace programs can read
disk files without a full path prefix.

### Usage from the Terminal

Once booted with the disk image attached (`make run-bios`), the virtual filesystem is
visible:

```
> ls
```

Shows directories like `bin  var  home` that were created on the FAT16 volume.

```
> cat /disk/hello.txt
```

Reads and prints a file from the disk.

---

## 6. Quick Start

```bash
# 1. Create the disk image (once)
make disk

# 2. Build and boot — disk is auto-attached
make run-bios
```

The serial log will contain:

```
ATA: disk detected, sectors=8192 (~4 MB)
FAT16: mounted, root_start=5, data_start=7, cluster_size=4096
```

And the System Info panel in the GUI will show **ATA detected**.

---

## 7. Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `ATA: reset timeout, status=0xFF` | Running with `-M q35` | Use `make run-bios` (which uses `-M pc`) |
| `ATA: no disk` | Disk image not created or not attached | Run `make disk` first; confirm `oxide_disk.img` exists |
| `make disk` fails | `mformat` not installed | `sudo apt install mtools` |
| Files not visible in `ls` | Disk image is blank (no files copied) | Copy files onto the image: `mcopy -i oxide_disk.img myfile.txt ::` |
