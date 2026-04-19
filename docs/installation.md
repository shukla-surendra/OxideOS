# OxideOS Installation Guide

This guide covers all three ways to get OxideOS running on a disk.

---

## Overview: Three Paths

| Method | Steps | Best for |
|--------|-------|---------|
| **A — Pre-built image** | `make install-image` → write to disk | USB boot on real hardware |
| **B — VirtualBox VDI** | `make install-vdi` → attach in VirtualBox | Quick VirtualBox testing |
| **C — Live installer** | Boot ISO → run `/bin/install` | Traditional install workflow, real hardware |

---

## Method A: Pre-built Disk Image

The `oxide_install.img` contains a complete OxideOS installation. Write it to any disk.

### Build

```bash
make install-image
```

Output: `oxide_install.img` (192 MB, MBR-partitioned, BIOS + UEFI bootable)

### Write to USB Drive

```bash
sudo ./install.sh /dev/sdX
```

`install.sh` will show the disk size and ask you to type `YES` before writing.

> **Find your USB device:** Run `lsblk` before and after inserting the USB to identify the device. **Double-check** — writing to the wrong device destroys data.

### Boot

Insert the USB and boot the target machine. Select the USB in the BIOS/UEFI boot menu.  
Both UEFI and legacy BIOS are supported.

---

## Method B: VirtualBox VDI

### Build

```bash
make install-vdi
```

Output: `oxide_install.vdi` (VirtualBox disk image, 192 MB)

### VirtualBox Setup

1. **New VM**
   - Name: `OxideOS`
   - Type: `Other`
   - Version: `Other/Unknown (64-bit)`
   - RAM: 256 MB (512 MB recommended)

2. **Hard Disk**
   - Select *Use an existing virtual hard disk file*
   - Browse to and select `oxide_install.vdi`

3. **Settings → System → Motherboard**
   - Enable EFI for UEFI boot *(recommended)*
   - Or leave disabled for legacy BIOS boot

4. **Start** — OxideOS boots directly from the VDI

---

## Method C: Live Installer (Boot ISO → Install to Disk)

This replicates the traditional OS installation flow: boot from a CD/ISO, run an interactive installer, reboot from the installed disk.

### What You Need

- `oxide_os-x86_64.iso` (build with `make`)
- A blank target disk (virtual HDD or real disk) — minimum **192 MB**

### VirtualBox Step-by-Step

#### 1. Create the VM

- VirtualBox → **New**
- Name: `OxideOS`, Type: `Other`, Version: `Other/Unknown (64-bit)`
- RAM: 256 MB minimum

#### 2. Create the target disk

In the **Hard Disk** step:
- Select *Create a new virtual hard disk*
- Format: VDI
- Size: **256 MB** (or larger)
- This disk starts blank — the installer will write OxideOS to it

#### 3. Attach the boot ISO

- **Settings → Storage**
- Under the IDE controller, click the CD icon
- Choose *Optical Drive* → click the small disk icon → *Choose a disk file*
- Select `oxide_os-x86_64.iso`

Your storage tree should look like:

```
Controller: IDE
  ├── [Optical]   oxide_os-x86_64.iso
  └── [Hard Disk] OxideOS.vdi  (blank, 256 MB)
```

#### 4. Set boot order

- **Settings → System → Motherboard**
- Boot order: **Optical** first, then **Hard Disk**
- Enable EFI if desired (installer supports both BIOS and UEFI)

#### 5. Start and run the installer

Start the VM. OxideOS boots from the ISO.

In the terminal window (or open a shell from the start menu), type:

```
install
```

The installer detects the blank second disk and shows:

```
╔═══════════════════════════════════════════╗
║          OxideOS Installer  v0.1          ║
╚═══════════════════════════════════════════╝

Target disk: 256 MB  (524288 sectors)

This will write OxideOS to the second disk:
  Partition 1  (64 MB, FAT32) — boot partition
  Partition 2  (64 MB, FAT16) — data partition

WARNING: ALL existing data on the target disk will be permanently erased!

Type YES to continue, anything else to abort:
```

Type `YES` and press Enter. The installer runs all steps (takes 10–30 seconds in a VM):

```
  [1/4] Formatting EFI boot partition (FAT32)...
  [2/4] Formatting data partition (FAT16)...
  [3/4] Writing Limine + kernel to boot partition...
  [4/4] Writing MBR partition table...

╔═══════════════════════════════════════════╗
║     Installation complete!                ║
╚═══════════════════════════════════════════╝

Next steps:
  1. Shut down this VM
  2. Remove the OxideOS ISO/CD from the VM settings
  3. Ensure the target disk is set as the primary boot device
  4. Start the VM — OxideOS will boot from disk
```

#### 6. Remove the ISO

- Shut down the VM (start menu → Shutdown, or type `kill 1` in the shell)
- **Settings → Storage** → click the ISO in the optical drive → click the disk icon → *Remove disk from virtual drive*

#### 7. Boot from disk

- **Settings → System → Motherboard** → move **Hard Disk** to the top of the boot order
- **Start** — OxideOS boots from the installed disk

You should see OxideOS's GUI without the ISO attached.

---

### QEMU Equivalent

```bash
# Step 1: Create a blank target disk
dd if=/dev/zero bs=1M count=256 of=install_target.img

# Step 2: Boot ISO with blank disk attached as second drive
qemu-system-x86_64 \
    -M pc \
    -serial stdio \
    -cdrom oxide_os-x86_64.iso \
    -boot d \
    -drive file=oxide_disk.img,format=raw,if=ide,index=0 \
    -drive file=install_target.img,format=raw,if=ide,index=1 \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0

# Step 3: In the OxideOS shell, run:
#   install
# Type YES when prompted.

# Step 4: After install completes, Ctrl+C to quit QEMU

# Step 5: Boot from the installed disk (UEFI)
qemu-system-x86_64 \
    -M q35 \
    -serial stdio \
    -drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
    -drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
    -drive file=install_target.img,format=raw,if=ide,index=0 \
    -display sdl \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0
```

---

## Installed Disk Layout

The installer always writes this exact layout:

```
Disk (192 MB minimum)
│
├── LBA 0       MBR
│               ├── Limine BIOS bootstrap code (bytes 0–439)
│               ├── Partition 1: LBA 2048,   131072 sectors, type 0xEF (EFI)
│               └── Partition 2: LBA 133120, 131072 sectors, type 0x06 (FAT16)
│
├── LBA 2048    Partition 1 — FAT32, 64 MB (EFI System Partition)
│               ├── EFI/BOOT/BOOTX64.EFI          ← UEFI boot entry (Limine)
│               ├── boot/limine/limine-bios.sys   ← BIOS boot stage 2
│               ├── boot/limine/limine.conf        ← boot configuration
│               └── boot/kernel                    ← OxideOS kernel binary
│
└── LBA 133120  Partition 2 — FAT16, 64 MB (OxideOS data)
                └── (empty — mounted at /disk/ after boot)
```

Both UEFI and legacy BIOS boot are supported:
- **UEFI:** Firmware finds `EFI/BOOT/BOOTX64.EFI` on the ESP automatically
- **BIOS:** Limine bootstrap in the MBR loads `limine-bios.sys` from the ESP

---

## Troubleshooting

### "No secondary disk detected"

The installer only sees the **second** ATA disk. If you only have one disk attached, the installer aborts.

- **VirtualBox:** Make sure you have two storage devices — the ISO (optical) and the blank HDD. The HDD must be attached to an IDE controller, not SATA.
- **QEMU:** Use `-drive file=...,if=ide,index=1` for the blank disk. `index=0` is reserved for `oxide_disk.img`.

### Installer stalls / no progress

ATA PIO writes are polling-based and block the scheduler. On a slow machine or with a large disk, each sector write takes ~100 µs. Writing ~500 KB of boot files takes a few seconds — this is normal.

If the installer hangs for more than 60 seconds, the ATA controller may not be in legacy mode. Use `-M pc` (QEMU) or ensure the VirtualBox VM uses an **IDE** controller (not SATA/NVMe).

### Installed disk doesn't boot — "No bootable device"

The installer writes the MBR **last**. If the OS crashed or was killed before step 4, the disk has no MBR and will not attempt to boot. Re-run the installer.

Check that:
- The correct disk is selected as boot device in BIOS/UEFI/VirtualBox boot order
- The ISO is removed (otherwise the machine boots the ISO, not the disk)

### VirtualBox: OS boots from ISO even after removing it

VirtualBox caches the boot order. After removing the ISO:
1. Make sure the optical drive shows *Empty* in Storage settings
2. Move Hard Disk to the top of the boot order in System → Motherboard

### UEFI vs BIOS: which to use?

Both work. UEFI is more reliable on modern VMs and real hardware. Legacy BIOS boot requires the Limine bootstrap code in the MBR, which the installer writes automatically from the embedded `limine-bios.sys`.

---

## Re-installing After a Kernel Update

The kernel binary written to disk is a snapshot of the kernel at install time. After rebuilding OxideOS:

1. Build the new kernel: `make`
2. Boot the ISO (new kernel) with the installed disk still attached
3. Run `install` again and type `YES`
4. The installer overwrites the kernel on the installed disk with the new build
5. Reboot from disk

Alternatively, just rebuild the install image: `make install-image` and write it again.
