<div align="center">

# OxideOS

**A hobby operating system written in Rust**

x86-64 · Limine bootloader · BIOS + UEFI · Ring 3 userspace · GUI · TCP/IP · musl libc · Lua 5.4 · BusyBox 1.36

[![Build](https://github.com/SurendraShuklaOfficial/OxideOS/actions/workflows/build.yml/badge.svg)](https://github.com/SurendraShuklaOfficial/OxideOS/actions/workflows/build.yml)
[![License](https://img.shields.io/badge/license-Custom%20Open%20Source-blue)](#license)
[![Rust](https://img.shields.io/badge/language-Rust%20(nightly)-orange?logo=rust)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-x86--64-lightgrey)](#)
[![Latest Release](https://img.shields.io/github/v/release/SurendraShuklaOfficial/OxideOS?label=latest%20ISO)](https://github.com/SurendraShuklaOfficial/OxideOS/releases/latest)

![OxideOS Screenshot](./oxideos.png)

</div>

---

OxideOS is a fully preemptive, multi-process operating system written from scratch in Rust (`no_std`). It boots on real hardware and in QEMU, runs a composited GUI with a window manager, has a TCP/IP network stack, and can execute programs compiled with **musl libc** — including the **Lua 5.4 interpreter** and **BusyBox 1.36**.

## Highlights

| | |
|---|---|
| **Boots on real hardware** | BIOS and UEFI via Limine v9 |
| **Preemptive multitasking** | Round-robin scheduler, Ring 3, fork/exec/waitpid |
| **GUI** | Composited window manager, start menu, PS/2 mouse |
| **Full TCP/IP stack** | RTL8139 NIC + smoltcp (TCP, UDP, ICMP, DHCP, ARP, DNS) |
| **Linux syscall ABI** | 80+ syscalls at Linux x86-64 numbers — musl programs just work |
| **musl libc** | Compile any C program with `musl-gcc -static` and run it |
| **Lua 5.4.7** | Full REPL and script execution, embedded in the kernel |
| **BusyBox 1.36.1** | 300+ Unix applets — ash, awk, sed, find, gzip, tar, … |
| **Installable** | `/bin/install` writes OxideOS to a blank disk from inside the OS |

---

## Quick Start

### Download and run (no build needed)

Download the latest ISO from [Releases](https://github.com/SurendraShuklaOfficial/OxideOS/releases/latest):

```bash
qemu-system-x86_64 \
    -M pc -serial stdio -cdrom oxide_os-x86_64.iso -boot d \
    -m 2G -cpu max \
    -netdev user,id=net0 -device rtl8139,netdev=net0
```

### Build from source

```bash
# 1. Install dependencies (Ubuntu/Debian)
sudo apt install build-essential qemu-system-x86 xorriso mtools \
                 dosfstools e2fsprogs nasm gcc-x86-64-linux-gnu

# 2. Install Rust nightly
curl https://sh.rustup.rs -sSf | sh
rustup override set nightly

# 3. Build and run
git clone https://github.com/SurendraShuklaOfficial/OxideOS
cd OxideOS
make run-bios          # BIOS boot, serial output — best for development
make run-gui-x86_64    # UEFI boot, SDL window with mouse and GUI
```

---

## What's Inside

### Kernel

- **Boot**: Limine v9 (BIOS + UEFI), GDT/TSS, IDT, PIC, PIT at 100 Hz
- **CPU**: `int 0x80` legacy gate + `SYSCALL/SYSRET` fast path, Ring 3
- **Scheduler**: Preemptive round-robin, up to 8 tasks, per-process CR3
- **Processes**: `fork` / `exec` / `waitpid` / `exit`, ELF64 loader, argv/envp (SysV ABI)
- **Memory**: Physical frame allocator, `mmap(MAP_ANONYMOUS)`, real `munmap`, `brk/sbrk`
- **Signals**: `sigaction`, `sigreturn`, trampoline page

### Filesystems

| Filesystem | Access | Mounted at |
|-----------|--------|-----------|
| RamFS | Read/write | `/bin`, `/tmp`, `/` |
| FAT16 | Read/write | `/disk` |
| ext2 | Read-only | `/ext2` |
| VFS devices | — | `/dev/null`, `/dev/tty` |

### Userspace Programs

```
Shell:        sh (pipes, $VAR, export, redirection)
Coreutils:    ls cat ps cp mv rm mkdir pwd echo grep wc
              head tail sort sleep kill touch true false
Network:      wget nc (netcat — TCP/UDP)
Editor:       edit (nano-like)
GUI:          terminal filemanager
System:       install (live disk installer)
musl/C:       hello_musl musl_test
Interpreters: lua busybox
```

### Linux-Compatible Syscalls (80+)

OxideOS uses Linux x86-64 syscall numbers so musl-compiled binaries run without modification:

```
read write open close stat fstat lstat poll lseek mmap mprotect munmap brk
sigaction sigprocmask sigreturn ioctl readv writev access pipe sched_yield
mremap madvise dup dup2 nanosleep getpid fork vfork execve exit waitpid
kill uname fcntl fsync truncate ftruncate getdents64 getcwd chdir rename
mkdir rmdir unlink readlink chmod fchmod chown fchown umask gettimeofday
getrlimit getrusage sysinfo getuid getgid getpgrp setsid getppid gettid
arch_prctl set_tid_address clock_gettime exit_group pipe2 pread64 pwrite64
socket bind connect listen accept sendto recvfrom … (+OxideOS-specific ≥400)
```

---

## Demo

### Lua 5.4 REPL

```
$ lua
Lua 5.4.7  Copyright (C) 1994-2024 Lua.org, PUC-Rio
> print("Hello from OxideOS!")
Hello from OxideOS!
> for i = 1, 5 do io.write(i*i .. " ") end
1 4 9 16 25
```

### BusyBox

```
$ busybox ls /bin
busybox.elf  cat.elf  cp.elf  edit.elf  grep.elf ...
$ busybox ash
~ $ echo "BusyBox shell on OxideOS"
BusyBox shell on OxideOS
```

### Run your own C program with musl libc

```bash
# On your host:
musl-gcc -static -O2 -o myprogram myprogram.c
# Copy to userspace/bin/myprogram.elf
# Add to kernel/src/kernel/programs.rs
# make && make run-bios
```

---

## Architecture

```
OxideOS/
├── kernel/                  # no_std Rust kernel
│   └── src/kernel/
│       ├── main.rs          # entry point, subsystem init
│       ├── scheduler.rs     # preemptive scheduler, Task struct
│       ├── syscall_core.rs  # Linux x86-64 syscall dispatch
│       ├── syscall.rs       # KernelRuntime — real syscall bodies
│       ├── vfs.rs           # VFS layer
│       ├── fat.rs           # FAT16 r/w driver
│       ├── ext2.rs          # ext2 read-only driver
│       ├── paging_allocator.rs
│       ├── programs.rs      # embedded ELF binaries
│       └── net/             # RTL8139 + smoltcp
├── userspace/               # Rust userspace crates
│   ├── oxide-rt/            # no_std syscall wrappers
│   ├── sh/                  # /bin/sh
│   ├── coreutils/           # ls cat grep wc head tail sort …
│   ├── terminal/            # GUI terminal
│   └── hello_musl/          # musl libc reference programs
└── docs/
    ├── plan.md              # full feature roadmap
    ├── installation.md
    └── dev_workflow.md
```

---

## Developer Workflow

### Dependencies

| Tool | Purpose | Install |
|------|---------|---------|
| `qemu-system-x86_64` | Run the OS | `apt install qemu-system-x86` |
| `xorriso` | Build the boot ISO | `apt install xorriso` |
| `mtools` (`mformat`, `mcopy`) | Create FAT disk images | `apt install mtools` |
| `dosfstools` (`mkfs.fat`) | FAT formatter | `apt install dosfstools` |
| `e2fsprogs` (`mke2fs`) | Create ext2 disk | `apt install e2fsprogs` |
| `sfdisk` | Partition the install image | `apt install util-linux` |
| Rust nightly | Compile kernel + userspace | `rustup override set nightly` |
| `nasm` | Assemble `.asm` programs | `apt install nasm` |
| `gcc` cross | Compile C userspace programs | `apt install gcc-x86-64-linux-gnu` |

### QEMU Run Targets

| Target | Machine | Display | Disk | Notes |
|--------|---------|---------|------|-------|
| `make run-bios` | `-M pc` (i440FX) | stdio serial | FAT16 on ATA | **Best for dev** — ATA works, fast boot |
| `make run-gui-x86_64` | q35 + UEFI | SDL window | FAT16 on ATA | Full GUI + mouse; grab with first click, release Ctrl+Alt+G |
| `make run-x86_64` | q35 + UEFI | stdio serial | none | Headless UEFI boot |
| `make run-kvm-x86_64` | q35 + KVM | GTK | FAT16 | Hardware-accelerated (WSL2: enable nested virt) |
| `make run-install-x86_64` | q35 + UEFI | SDL | install image | Test the pre-built install image |
| `make run-install-bios` | `-M pc` | stdio | install image | BIOS-boot the install image |

### Typical Dev Loop

```bash
# Edit kernel source
$EDITOR kernel/src/kernel/syscall.rs

# Rebuild and test (fastest path)
make kernel && make run-bios

# After editing userspace
make userspace && make kernel && make run-bios

# Full clean rebuild
make distclean && make run-bios
```

### Persistent FAT16 Data Disk

The kernel mounts a FAT16 disk at `/disk/`. Create it once and it persists across boots:

```bash
make disk              # creates oxide_disk.img (4 MB FAT16, only needed once)
make run-bios          # ATA disk auto-detected on this target
```

Files placed in `oxide_disk.img` are visible as `/disk/<name>`. To populate from the host:

```bash
sudo mount -o loop oxide_disk.img /mnt
sudo cp myfile.txt /mnt/
sudo umount /mnt
```

### Secondary ext2 Disk (optional)

```bash
make ext2-disk         # creates oxide_ext2.img (32 MB ext2)
sudo mount -o loop oxide_ext2.img /mnt && sudo cp files /mnt/ && sudo umount /mnt
make run-bios          # both disks attached automatically if the images exist
```

### Full Makefile Reference

| Target | Description |
|--------|-------------|
| `make` | Build ISO (`oxide_os-x86_64.iso`) |
| `make kernel` | Rebuild kernel only |
| `make userspace` | Rebuild userspace only |
| `make run-bios` | QEMU BIOS boot, ATA disk, serial output |
| `make run-gui-x86_64` | QEMU UEFI boot, SDL window + mouse |
| `make run-x86_64` | QEMU UEFI boot, serial only |
| `make run-kvm-x86_64` | QEMU with KVM acceleration |
| `make disk` | Create 4 MB FAT16 data disk (once) |
| `make ext2-disk` | Create 32 MB ext2 secondary disk (once) |
| `make install-image` | Build 192 MB bootable install image |
| `make install-vdi` | Convert install image to VirtualBox VDI |
| `make run-install-x86_64` | Boot install image in QEMU (UEFI, SDL) |
| `make run-install-bios` | Boot install image in QEMU (BIOS) |
| `make clean` | Remove ISO and build artefacts |
| `make clean-disk` | Remove `oxide_disk.img` |
| `make clean-install` | Remove install images |
| `make distclean` | Remove all build output including `limine/` and `ovmf/` |

### ATA Disk and QEMU Machine Types

OxideOS uses ATA PIO for disk I/O (port `0x1F0`). This requires the legacy IDE controller:

| QEMU flag | Chipset | IDE at 0x1F0? | Use case |
|-----------|---------|--------------|----------|
| `-M pc` | i440FX/PIIX4 | ✓ Yes | Dev testing, BIOS boot |
| `-M q35` + UEFI | ICH9 | ✗ No | GUI testing (no disk needed for ISO boot) |
| `-M q35` + IDE drive | ICH9 | ✓ Yes | Installed disk boot |

### WSL2 Notes

- SDL display requires an X server (VcXsrv, WSLg, or X410).
- KVM acceleration requires nested virtualisation: add `nestedVirtualization=true` to `~/.wslconfig` and restart WSL.
- All non-GUI targets (`run-bios`, `run-x86_64`) work without an X server.

---

## Installation on Real Hardware / VirtualBox

### Method A — Pre-built image (simplest)

```bash
make install-image          # builds oxide_install.img (192 MB)
sudo ./install.sh /dev/sdX  # DANGER: replaces ALL data on /dev/sdX
```

Boot the USB on any x86_64 machine with UEFI or legacy BIOS firmware.

#### VirtualBox with pre-built VDI

```bash
make install-vdi            # converts oxide_install.img → oxide_install.vdi
```

1. **New VM** → Name: `OxideOS`, Type: `Other`, Version: `Other/Unknown (64-bit)`
2. **Hardware** → RAM: 256 MB minimum
3. **Hard Disk** → *Use an existing virtual hard disk file* → select `oxide_install.vdi`
4. **Settings → System → Motherboard** → Enable EFI for UEFI boot *(or leave disabled for BIOS boot)*
5. **Start**

---

### Method B — Live installer (boot from ISO, install to disk)

This is the traditional "boot from CD, install to disk" workflow.

#### VirtualBox Setup

**Step 1 — Create the VM**

1. Open VirtualBox → **New**
2. Name: `OxideOS`, Type: `Other`, Version: `Other/Unknown (64-bit)`
3. RAM: 256 MB (512 MB recommended)
4. **Hard Disk** → *Create a new virtual hard disk*
   - Format: VDI (or VMDK), Size: **256 MB minimum**
   - This is the **installation target** — starts blank

**Step 2 — Attach the boot ISO**

1. **Settings → Storage**
2. Click the CD icon → *Choose a disk file* → select `oxide_os-x86_64.iso`
3. Ensure the controller type is **IDE** (not SATA) for compatibility

**Step 3 — Configure boot order**

1. **Settings → System → Motherboard**
2. Boot order: **Optical** first, then **Hard Disk**
3. Enable EFI if you want UEFI boot (optional)

**Step 4 — Start and install**

1. **Start** the VM — OxideOS boots from the ISO
2. In the terminal window, type:
   ```
   install
   ```
3. The installer shows the target disk size and asks for confirmation:
   ```
   OxideOS Installer  v0.1
   Target disk: 256 MB  (524288 sectors)

   Type YES to continue, anything else to abort: YES

   Installing OxideOS...
     [1/4] Formatting EFI boot partition (FAT32)...
     [2/4] Formatting data partition (FAT16)...
     [3/4] Writing Limine + kernel to boot partition...
     [4/4] Writing MBR partition table...

   Installation complete!
   ```
4. **Shut down** the VM (start menu → Shutdown, or `kill 1` in shell)

**Step 5 — Remove the ISO and boot from disk**

1. **Settings → Storage** → remove the ISO from the optical drive
2. **Settings → System → Motherboard** → move **Hard Disk** to first in boot order
3. **Start** — OxideOS boots from the installed disk

#### QEMU equivalent

```bash
# 1. Create blank target disk
dd if=/dev/zero bs=1M count=256 of=install_target.img

# 2. Boot ISO with blank disk as second drive
qemu-system-x86_64 \
    -M pc -serial stdio \
    -cdrom oxide_os-x86_64.iso -boot d \
    -drive file=oxide_disk.img,format=raw,if=ide,index=0 \
    -drive file=install_target.img,format=raw,if=ide,index=1 \
    -m 2G -cpu max

# 3. Run 'install' in the OxideOS shell, type YES

# 4. Boot the installed disk (UEFI)
qemu-system-x86_64 \
    -M q35 -serial stdio \
    -drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
    -drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
    -drive file=install_target.img,format=raw,if=ide,index=0 \
    -display sdl -m 2G -cpu max
```

#### What the installer writes

| Step | Action |
|------|--------|
| 1 | Format partition 1 (64 MB, FAT32) — EFI boot |
| 2 | Format partition 2 (64 MB, FAT16) — OxideOS data |
| 3 | Write `EFI/BOOT/BOOTX64.EFI`, Limine BIOS stage, `limine.conf`, kernel binary |
| 4 | Write MBR — **written last** (safe failure state) |

#### Installed Disk Layout

```
oxide_install.img  (192 MB)
├── MBR  [LBA 0]
│   ├── Limine BIOS bootstrap
│   ├── Partition 1: EFI  (FAT32, 64 MB)
│   └── Partition 2: Data (FAT16, 64 MB)
│
├── Partition 1 — FAT32 (EFI System)
│   ├── EFI/BOOT/BOOTX64.EFI     ← UEFI entry point
│   ├── boot/limine/limine.conf
│   ├── boot/limine/limine-bios.sys
│   └── boot/kernel              ← full kernel + all userspace embedded
│
└── Partition 2 — FAT16 (user data, mounted as /disk/)
```

---

## Roadmap

See [docs/plan.md](docs/plan.md) for the full feature roadmap. Key upcoming milestones:
- [ ] DHCP auto-activation (DNS resolver already implemented)
- [ ] ext2 write support
- [ ] Copy-on-write fork
- [ ] Job control (`bg`, `fg`, `Ctrl+Z`)
- [ ] procfs (`/proc/PID/maps`, `/proc/meminfo`)
- [ ] SMP (multi-core)

---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a syscall, write a userspace program, or fix a bug.

Good first issues: [issues labeled `good first issue`](https://github.com/SurendraShuklaOfficial/OxideOS/issues?q=label%3A%22good+first+issue%22)

---

## Similar Projects

- [Redox OS](https://www.redox-os.org/) — production-grade OS in Rust
- [Writing an OS in Rust](https://os.phil-opp.com/) — the definitive tutorial series
- [xv6](https://github.com/mit-pdos/xv6-riscv) — MIT teaching OS (RISC-V, C)
- [SerenityOS](https://serenityos.org/) — full desktop OS in C++

---

## License

Copyright © 2025 Surendra Shukla. See [LICENSE](LICENSE) for terms.

Attribution required. Commercial redistribution of OxideOS itself is not permitted — building products on top of it is fine.
