# OxideOS Developer Workflow

Quick reference for building, running, and testing OxideOS during development.

---

## First-Time Setup

```bash
# 1. Rust nightly
curl https://sh.rustup.rs -sSf | sh
rustup override set nightly

# 2. System tools (Ubuntu/Debian)
sudo apt install \
    build-essential nasm \
    qemu-system-x86 \
    xorriso \
    mtools dosfstools \
    e2fsprogs \
    util-linux \
    gcc-x86-64-linux-gnu

# 3. One-time assets (downloaded automatically on first build)
make                  # fetches limine/, ovmf/ and builds the ISO

# 4. Create the persistent data disk (needed for ATA tests)
make disk             # creates oxide_disk.img (4 MB FAT16)
```

---

## Daily Build Loop

```bash
# Fastest: rebuild kernel, reuse cached userspace
make kernel && make run-bios

# After userspace changes
make userspace && make kernel && make run-bios

# After touching both
make && make run-bios

# Full clean rebuild (e.g. after changing the linker script)
make distclean && make run-bios
```

---

## Choosing the Right QEMU Target

| What you're testing | Use |
|--------------------|-----|
| Kernel logic, syscalls, shell | `make run-bios` — fastest boot, ATA disk works |
| GUI, window manager, mouse | `make run-gui-x86_64` — SDL window with mouse |
| Networking | Either target — RTL8139 attached to both |
| Installer (`/bin/install`) | See [Testing the Installer](#testing-the-installer) below |
| Installed disk boot | `make run-install-x86_64` or manual QEMU command |

---

## Kernel Development

### Adding a Syscall

1. Add a new variant to the `Syscall` enum in `kernel/src/kernel/syscall_core.rs`
2. Add the name to `Syscall::name()`
3. Add the number mapping in `impl From<u64> for Syscall`
4. Add a default implementation to the `SyscallRuntime` trait (returns `ENOSYS`)
5. Implement it in `KernelRuntime` in `kernel/src/kernel/syscall.rs`
6. Add the dispatch arm in `dispatch()` in `syscall_core.rs`
7. Add a wrapper in `userspace/oxide-rt/src/lib.rs`

OxideOS-specific syscalls use numbers ≥ 400. Current highest: `434` (InstallBegin).

### Adding a Userspace Program

1. Create `userspace/<name>/Cargo.toml` and `src/main.rs`
   - `#![no_std]` `#![no_main]`, implement `pub extern "C" fn oxide_main()`
2. Add the crate to `userspace/Cargo.toml` `[workspace.members]`
3. Add `-p <name>` to the cargo build line in `userspace/Makefile`
4. Add `cp target/.../release/<name> bin/<name>.elf` to `userspace/Makefile`
5. Add `pub static NAME: &[u8] = include_bytes!(.../<name>.elf)` to `kernel/src/kernel/programs.rs`
6. Add `"<name>" => Some(NAME)` to `programs::find()`
7. Add `"<name>"` to `programs::NAMES`

### Adding a Kernel Module

1. Create `kernel/src/kernel/<module>.rs`
2. Add `pub mod <module>;` to `kernel/src/kernel/mod.rs`
3. Call `crate::kernel::<module>::init()` at the appropriate place in `kmain()` in `main.rs`

### Debugging

Serial output is always available — check the terminal running QEMU for `SERIAL_PORT.write_str(...)` messages. The kernel writes detailed boot progress to serial.

For userspace, `println!()` from `oxide-rt` writes to the terminal compositor and serial simultaneously.

---

## Filesystem Layout (Runtime)

| Path | Filesystem | Notes |
|------|-----------|-------|
| `/disk/` | FAT16 (primary ATA) | Persistent, writable. Maps to `oxide_disk.img` in QEMU |
| `/ext2/` | ext2 (secondary ATA) | Read-only. Maps to `oxide_ext2.img` if attached |
| `/dev/null` | VFS | Discard writes, EOF on reads |
| `/dev/tty` | VFS | Terminal I/O |
| `/ram/` | RamFS | In-memory, lost on reboot |

Built-in programs are embedded in the kernel binary and are found by name via `programs::find()`. They do not appear on any filesystem path but are accessible to the shell and start menu by name.

---

## Testing the Installer

### Quick test in QEMU

```bash
# 1. Create a blank target disk
dd if=/dev/zero bs=1M count=256 of=test_target.img

# 2. Boot the ISO with the blank disk as second drive
qemu-system-x86_64 \
    -M pc -serial stdio \
    -cdrom oxide_os-x86_64.iso -boot d \
    -drive file=oxide_disk.img,format=raw,if=ide,index=0 \
    -drive file=test_target.img,format=raw,if=ide,index=1 \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0

# 3. In OxideOS shell: type  install  and then  YES

# 4. Ctrl+C to quit QEMU after install completes

# 5. Boot from the installed disk (UEFI)
qemu-system-x86_64 \
    -M q35 -serial stdio \
    -drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
    -drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
    -drive file=test_target.img,format=raw,if=ide,index=0 \
    -display sdl -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0
```

### Inspecting the installed disk

```bash
# Show partition table
sfdisk -l test_target.img

# List files on the EFI partition (starts at byte offset 1 MB = sector 2048)
mdir -i test_target.img@@1M -/ ::

# List files in boot/limine on the EFI partition
mdir -i test_target.img@@1M :: /boot/limine/

# Check FAT type on data partition (starts at byte 65 MB = sector 133120)
minfo -i test_target.img@@65M ::
```

### What to verify after install

```
✓ sfdisk shows two partitions: type EF (64 MB) and type 06 (64 MB)
✓ mdir shows: EFI/BOOT/BOOTX64.EFI, boot/limine/limine.conf,
              boot/limine/limine-bios.sys, boot/kernel
✓ minfo shows FAT16 on data partition
✓ QEMU boots from installed disk without ISO
✓ OxideOS GUI appears and shell works
```

---

## Pre-built Install Image

For sharing or testing without running the installer:

```bash
make install-image      # builds oxide_install.img (192 MB)
make run-install-x86_64 # UEFI boot from install image in QEMU
make run-install-bios   # BIOS boot from install image in QEMU
make install-vdi        # convert to VirtualBox VDI
```

The pre-built image is functionally equivalent to a freshly-installed disk. It is rebuilt from scratch each time and includes the latest kernel binary.

---

## Common Issues

### "ATA: no disk" in serial output

The ATA disk requires legacy IDE mode (`-M pc`). On q35 + UEFI, the IDE controller is in native PCI mode and `0x1F0` returns `0xFF`.

- Use `make run-bios` for development (uses `-M pc`)
- The installed disk works on q35 because it's attached as an IDE HDD device, which q35 exposes in legacy-compatible mode

### Kernel panic / BSOD

Check the serial output (the terminal running QEMU). The panic handler prints a register dump and the fault address. Common causes:
- Stack overflow: bump the task stack size in `scheduler.rs`
- Page fault in ring-3: the user ELF might access unmapped memory

### Userspace program not found

Ensure the binary was:
1. Built by the userspace Makefile and copied to `userspace/bin/<name>.elf`
2. Added to `programs.rs` with `include_bytes!`
3. Registered in `programs::find()` and `programs::NAMES`

Run `make userspace` before `make kernel` — the kernel embeds binaries at compile time.

### Installer: "No secondary disk detected"

The installer uses the secondary ATA bus (`0x170`). In QEMU, use `-drive if=ide,index=1`. In VirtualBox, make sure the blank HDD is on an IDE controller (Controller: IDE), not SATA.

---

## Project Structure

```
OxideOS/
├── kernel/src/
│   ├── main.rs                  ← kernel entry point, boot sequence, GUI loop
│   ├── kernel/
│   │   ├── ata.rs               ← ATA PIO driver (primary + secondary bus)
│   │   ├── fat.rs               ← FAT16 r/w filesystem
│   │   ├── ext2.rs              ← ext2 read-only filesystem
│   │   ├── mbr.rs               ← MBR partition table parser
│   │   ├── installer.rs         ← disk installer (FAT32 writer, MBR writer)
│   │   ├── scheduler.rs         ← preemptive round-robin scheduler
│   │   ├── syscall_core.rs      ← syscall numbers, dispatch, trait
│   │   ├── syscall.rs           ← KernelRuntime: wires syscalls to kernel services
│   │   ├── programs.rs          ← embedded userspace binaries
│   │   └── ...
│   └── gui/                     ← window manager, compositor, terminal, fonts
├── userspace/
│   ├── oxide-rt/                ← no_std runtime: syscall wrappers, allocator
│   ├── sh/                      ← shell
│   ├── coreutils/               ← ls, cat, cp, grep, wc, head, tail, sort, …
│   ├── install/                 ← /bin/install (installer UI)
│   └── bin/                     ← compiled .elf binaries (gitignored)
├── limine.conf                  ← bootloader configuration
├── Makefile                     ← build system
├── install.sh                   ← write oxide_install.img to a real device
└── docs/
    ├── installation.md          ← full installation guide
    └── dev_workflow.md          ← this file
```
