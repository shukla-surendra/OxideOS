# OxideOS — Roadmap to a Fully Functional OS

This document audits every subsystem in the current codebase, identifies what is missing,
and lays out a phased implementation plan to reach a fully functional general-purpose OS.
Each phase builds directly on the previous one so the OS is bootable and usable after every
milestone.

---

## Current State (as of April 2026)

### What works today

| Subsystem | Status |
|-----------|--------|
| 64-bit boot (Limine, UEFI/BIOS) | ✅ Complete |
| GDT / TSS / IDT | ✅ Complete |
| PIC remapping, timer at 100 Hz | ✅ Complete |
| Physical frame allocator (256 MB bitmap) | ✅ Complete |
| Per-process page tables (CR3 per task) | ✅ Complete |
| User mode (Ring 3) via `iretq` | ✅ Complete |
| Preemptive scheduler — 8-task round-robin | ✅ Complete |
| ELF64 loader (ET_EXEC, static) | ✅ Complete |
| Syscall dispatch (`int 0x80`, 17 syscalls) | ✅ Complete |
| RamFS (in-memory tree, 32 open FDs) | ✅ Complete |
| FAT16 read + write (root dir, ATA PIO) | ✅ Complete |
| Anonymous pipes (8 pairs, 4 KB each) | ✅ Complete |
| Stdin ring buffer → GetChar syscall | ✅ Complete |
| PS/2 keyboard (US QWERTY, IRQ + polling) | ✅ Complete (VirtualBox debug active) |
| PS/2 mouse (packets, buttons, cursor) | ✅ Complete |
| Framebuffer graphics + back-buffer blit | ✅ Complete |
| Window manager (drag, focus, taskbar, clock) | ✅ Complete |
| Start menu (program launcher, categories) | ✅ Complete |
| GUI terminal (real bash-style UI, inline prompt) | ✅ Complete |
| Multiple terminal windows (sh opens new window) | ✅ Complete |
| Shutdown / Reboot (ACPI ports + 8042 reset) | ✅ Complete |
| Shell (`/bin/sh`) with `>` / `>>` redirect | ✅ Complete |
| Serial port debug output | ✅ Complete |
| Fork / exec / waitpid / exit cleanup | ✅ Complete |
| Per-task FD table + dup2 | ✅ Complete |
| brk/sbrk + userspace heap | ✅ Complete |
| kill syscall | ✅ Complete |
| User page-fault → SIGSEGV | ✅ Complete |
| ReadDir syscall | ✅ Complete |
| VFS layer (mount table, /dev/null, /dev/tty) | ✅ Complete |
| IPC message queues (compositor protocol) | ✅ Complete |

### Known gaps

| Subsystem | Gap |
|-----------|-----|
| Keyboard | VirtualBox keyboard input unreliable — debugging active |
| Signals | No full signal delivery (sigaction/trampoline); kill works but no Ctrl+C→SIGINT |
| TTY | No TTY abstraction — no canonical/raw mode switching |
| Filesystem | No subdirectory support on FAT16; no ext2; no partition table parsing |
| Networking | None |
| Dynamic linking | Only static ELF |
| SMP | Single core only |
| Permissions | No users, no file permissions |
| mmap | Anonymous mmap not implemented (only brk) |

---

## Available `no_std` Crates for OxideOS

These Rust crates work in a `#![no_std]` kernel environment. Adding them via `Cargo.toml`
replaces hand-rolled code with battle-tested implementations.

### Immediately useful (drop-in improvements)

| Crate | `alloc`? | What it replaces / adds |
|-------|----------|--------------------------|
| `pc-keyboard` | No | Replace hand-rolled scancode table; proper PS/2 set 1/2 decoder with VirtualBox compatibility |
| `pic8259` | No | Replace `pic.rs` — battle-tested 8259 PIC init/EOI/mask |
| `uart_16550` | No | Replace `serial.rs` — safe 16550 UART driver |
| `spin` | No | Already used — Mutex/RwLock/Once for shared kernel state |
| `heapless` | No | Fixed-capacity `Vec`/`String`/`IndexMap` without allocator |
| `bitvec` | Optional | Replace bitmap allocator with safe bit manipulation |
| `x86_64` | No | Safe wrappers for CR3, VirtAddr, PageTableEntry, CPUID |
| `portable-atomic` | No | Cross-arch safe atomics |

### Medium-term (enable new features)

| Crate | `alloc`? | Purpose |
|-------|----------|---------|
| `smoltcp` | Yes | Complete no_std TCP/IP stack — ARP, IP, UDP, TCP, ICMP. Drops Phase 7 from High to Medium effort |
| `virtio-drivers` | Yes | VirtIO-net and VirtIO-block device drivers for QEMU/VirtualBox |
| `acpi` | Yes | Parse ACPI tables (RSDP/MADT/FADT) — proper ACPI shutdown, SMP core discovery |
| `xmas-elf` | Optional | Replace hand-rolled ELF loader with a tested parser |
| `nom` | Optional | Parser combinators — shell tokenisation, config files, ELF sections |
| `linked_list_allocator` or `talc` | No | Better kernel heap allocator (replace bump allocator) |
| `libm` | No | `sin`/`cos`/`sqrt` etc. — for GUI effects, physics, graphing |
| `miniz_oxide` | No | Deflate/zlib compression — compress ramdisk / ELF binaries |

### Future / optional

| Crate | Purpose |
|-------|---------|
| `embedded-graphics` | 2D graphics primitives (lines, circles, images) for GUI |
| `smolstr` | Small-string optimisation (avoids heap for short strings) |
| `postcard` | Compact binary serialization for IPC messages |
| `sha2` / `md5` | File checksums, future auth |
| `chacha20poly1305` | Authenticated encryption |
| `uuid` | UUIDs for process / file IDs |
| `noto-sans-mono-bitmap` | High-quality mono bitmap font with full Unicode coverage |

### How to add a crate

```toml
# kernel/Cargo.toml
[dependencies]
pc-keyboard = { version = "0.7", default-features = false }
smoltcp = { version = "0.11", default-features = false, features = ["proto-ipv4", "socket-tcp"] }
x86_64 = { version = "0.15", default-features = false, features = ["instructions"] }
```

Most `no_std` crates require `default-features = false` to drop their `std` dependency.

---

## Phase 1 — Solid Process Model ✅ COMPLETE

**Goal:** Any ELF binary from disk can be loaded, forked, exec'd, and waited on.

### 1.1 `exec` syscall ✅ DONE
### 1.2 `fork` syscall ✅ DONE
### 1.3 Per-task FD table ✅ DONE
### 1.4 `waitpid` syscall ✅ DONE
### 1.5 Exit cleanup ✅ DONE

---

## Phase 2 — Virtual Filesystem (VFS) ✅ MOSTLY DONE

### 2.1 VFS layer ✅ DONE
- Mount table: `/` → RamFS, `/disk` → FAT16
- FdBackend enum routes open/read/write/close to the right driver
- `/dev/null`, `/dev/tty` device files

### 2.2 FAT16 write support ✅ DONE
- Cluster allocation (scan FAT for 0x0000, mark 0xFFFF)
- `write_fd` with cluster chain extension
- Directory entry size flush on write/close
- O_CREAT / O_TRUNC / O_APPEND support
- Both FAT copies written
- Shell `>` and `>>` redirect via `sh`

### 2.3 FAT16 subdirectory support ← NEXT after keyboard fix
- Parse ATTR_DIRECTORY entries; follow cluster chains
- `readdir` for subdirectories
- `cd /disk/bin/` in terminal

### 2.4 `/dev` device filesystem ✅ PARTIAL
- `/dev/null` and `/dev/tty` exist
- Missing: `/dev/zero`, `/dev/disk0` raw block device

### 2.5 `stat` / `fstat` syscalls ← TODO
- Return `Stat { size, kind, permissions, inode }`

---

## Phase 3 — Userspace Shell & Standard Programs ✅ MOSTLY DONE

### 3.1 Toolchain ✅ DONE
- `oxide-rt` runtime crate (`_start`, `exit`, `write`, `read`, `open`, `close`, `brk`)
- Programs compile with `--target x86_64-unknown-none`
- `make programs` builds userspace ELF + mcopy to FAT16 disk

### 3.2 Shell (`/bin/sh`) ✅ DONE
- Fork + exec + waitpid
- `>` / `>>` redirect support
- Opens in a new dedicated terminal window
- Terminal UI redesigned to real bash/sh style (inline prompt, block cursor)

### 3.3 Core utilities ← PARTIAL
- Built into kernel terminal: `ls`, `cat`, `mkdir`, `touch`, `rm`, `echo`, `pwd`
- Missing as standalone `/bin/` programs: `cp`, `mv`, `ps`, `kill`, `sleep`

### 3.4 Text editor (`/bin/edit`) ← TODO
- nano-like: full-screen, arrow key navigation, Ctrl+S save, Ctrl+Q quit

### 3.5 `dup2` / `dup` syscalls ✅ DONE

### 3.6 `chdir` / `getcwd` syscalls ← TODO
- Each task tracks a working directory; path resolution relative to it

---

## Phase 4 — Signals & TTY

**Goal:** Processes can be interrupted, killed, and managed the way POSIX programs expect.

### 4.1 Signal infrastructure ✅ PARTIAL
- `kill` syscall (Kill=91) marks task Dead immediately
- Missing: `pending_signals` bitmask, `sigaction`, delivery trampoline

Full implementation:
- `pending_signals: u32` bitmask in `Task`
- `signal_handlers: [u64; 32]` — user-space handler addresses
- `sigaction` syscall (`= 90`)
- Before resuming any user task in `tick()`: deliver pending signals via trampoline

### 4.2 Ctrl+C → SIGINT ← NEXT PRIORITY
- Keyboard ISR: if Ctrl+C detected and foreground PID exists, send SIGINT
- Requires "foreground PID" concept (shell sets it after fork+exec)

### 4.3 TTY subsystem ← TODO
Create `kernel/src/kernel/tty.rs`:
- Canonical mode (cooked): buffer until `\n`; handle Backspace/Ctrl+C/Ctrl+D
- Raw mode: pass every byte immediately (for editors, readline)
- `ioctl` syscall (`= 92`): TCGETS/TCSETS to switch modes
- `/dev/tty` routes through TTY

---

## Phase 5 — Dynamic Memory for User Programs ✅ DONE

### 5.1 `brk` / `sbrk` ✅ DONE
- Brk=11, USER_HEAP_BASE=0x0100_0000, map pages on demand

### 5.2 `mmap` (anonymous) ← TODO
- MAP_ANONYMOUS | MAP_PRIVATE: map zeroed pages above heap_end

### 5.3 Userspace allocator ← TODO
- Ship `alloc.rs` as part of oxide-rt (sbrk-based free-list allocator)

---

## Phase 6 — Extended Filesystem & Persistence

**Goal:** Proper on-disk filesystem with directories, permissions, and large files.

### 6.1 FAT16 subdirectory support ← NEXT (simpler than ext2)
- Implement before ext2; unblocks `cd /disk/subdir/`

### 6.2 ext2 filesystem driver ← TODO
- Superblock, block groups, inodes, direct+indirect blocks, directory entries
- Start read-only; write in second pass

### 6.3 Partition table (MBR) ← TODO
- Parse 64-byte MBR at LBA 0 offset 446
- Support FAT16 (0x06) and ext2 (0x83) partition types

### 6.4 File permissions ← TODO
- uid/gid/mode in VNode; permission check on open
- chmod (=93), chown (=94) syscalls

---

## Phase 7 — Networking

**Goal:** Basic TCP/IP so the OS can ping and host simple services.

**Key insight:** Use `smoltcp` crate instead of writing a network stack from scratch.
This converts Phase 7 from ~6 weeks to ~2 weeks of work.

### 7.1 Network driver ← TODO
Option A: **virtio-net** (use `virtio-drivers` crate)
- Detect PCI vendor 0x1AF4 / device 0x1000
- Negotiate features, set up RX/TX virtqueues
- QEMU flag: `-netdev user,id=net0 -device virtio-net-pci,netdev=net0`

Option B: RTL8139 (simpler, no external crate needed)
- QEMU flag: `-netdev user,id=net0 -device rtl8139,netdev=net0`

### 7.2 smoltcp integration ← TODO
```toml
smoltcp = { version = "0.11", default-features = false,
            features = ["proto-ipv4", "socket-tcp", "socket-udp", "socket-icmp"] }
```
- Implement `smoltcp::phy::Device` trait for the NIC driver
- Wire RX/TX to the virtio-net or RTL8139 driver
- Get DHCP via `smoltcp::socket::dhcpv4`

### 7.3 Socket syscalls ← TODO
- Socket=100, Bind=101, Connect=102, Listen=103, Accept=104
- Send=105, Recv=106 — sockets as file descriptors

### Deliverable
```
$ ping 8.8.8.8
$ wget http://example.com
```

---

## Phase 8 — Multi-Window GUI Applications

**Goal:** Multiple GUI apps run as separate processes with their own windows.

### 8.1 Shared framebuffer / compositor ← TODO
- Kernel WM owns framebuffer
- User processes post draw commands via IPC message queue (already implemented)
- Each process gets a canvas (shared memory region)

### 8.2 Shared memory ← TODO
- shmget (=110), shmat (=111), shmdt (=112)
- Kernel maps same physical frames into two virtual address spaces

### 8.3 Message-passing IPC ✅ DONE
- IPC message queue implemented (`kernel/src/kernel/ipc.rs`)
- Compositor protocol: CreateWindow, DrawRect, PresentCanvas, DestroyWindow

### 8.4 Userspace GUI applications ← TODO

| App | Description |
|-----|-------------|
| `terminal` | Terminal emulator process (current kernel terminal → userspace) |
| `file_manager` | Browse RamFS + FAT16 visually |
| `text_editor` | Full-screen editor with syntax highlighting |
| `clock` | Floating clock widget |

---

## Phase 9 — Stability, Security & Polish

### 9.1 fast syscall path (`syscall`/`sysret`) ← HIGH VALUE
- Replace `int 0x80` with SYSCALL/SYSRET (set STAR, LSTAR, SFMASK MSRs)
- ~3× faster syscall round-trip

### 9.2 SMEP / SMAP enforcement ← TODO
- CR4 bits: prevent kernel from executing/reading user-space without `stac`

### 9.3 SMP (optional, advanced) ← TODO
- AP startup via INIT-SIPI sequence
- Per-CPU scheduler instances

### 9.4 ACPI proper ← TODO
- Use `acpi` crate to parse RSDP → XSDT → FADT
- Proper PM1a shutdown (replaces port-guessing in `shutdown.rs`)

### 9.5 Crash dump ← TODO
- On panic: save registers, dump to serial + screen
- Optionally write to `/var/crash`

---

## Implementation Priority Order

```
✅ Phase 1     Process model (fork/exec/waitpid/exit)
✅ Phase 2.1   VFS layer
✅ Phase 2.2   FAT16 write + sh redirects
✅ Phase 3.1   Toolchain (oxide-rt)
✅ Phase 3.2   Shell (/bin/sh) — new window, real terminal UI
✅ Phase 3.5   dup2 syscall
✅ Phase 4.1   kill syscall (partial)
✅ Phase 5.1   brk/sbrk + userspace heap
✅ Phase 8.3   IPC message queues
✅ GUI         Start menu, taskbar, multi-terminal, shutdown/reboot

✅ DONE         Fix VirtualBox keyboard (replaced scancode table with `pc-keyboard` crate)
✅ DONE        Phase 4.2  Ctrl+C → SIGINT (0x03 via MapLettersToUnicode; terminal kills fg PID)
✅ DONE        Phase 2.3  FAT16 subdirectory support (traverse, open, list, mkdir in subdirs)
✅ DONE        Phase 3.6  chdir(72)/getcwd(73)/mkdir(71) syscalls; Task.cwd; fork copies cwd
✅ DONE        Phase 3.3  Standalone /bin/ls, /bin/cat, /bin/ps, /bin/cp, /bin/mkdir, /bin/pwd
⬡             Phase 3.4  Text editor (/bin/edit)
⬡             Phase 4.3  TTY (canonical/raw mode)
⬡             Phase 5.2  mmap anonymous
⬡             Phase 6.2  ext2 read-only driver
⬡             Phase 6.3  MBR partition table
⬡             Phase 9.1  fast syscall (SYSCALL/SYSRET)
⬡             Phase 7.1  virtio-net driver (use virtio-drivers crate)
⬡             Phase 7.2  smoltcp integration
⬡             Phase 7.3  Socket syscalls
⬡             Phase 8.2  Shared memory
⬡             Phase 8.1  Userspace compositor
⬡             Phase 9.2  SMEP/SMAP
⬡             Phase 9.3  SMP
```

---

## Quick Wins — Add These Crates Now

These are safe, low-risk additions that immediately improve reliability:

```toml
# kernel/Cargo.toml [dependencies]

# Proper PS/2 keyboard decoding — fixes VirtualBox AUXB bit 5 issue
pc-keyboard = { version = "0.7", default-features = false }

# Safe x86_64 hardware abstractions
x86_64 = { version = "0.15", default-features = false, features = ["instructions"] }

# Fixed-capacity collections (no allocator needed, great for kernel data structures)
heapless = { version = "0.8", default-features = false }

# Better kernel heap allocator (drop-in for our bump allocator)
linked_list_allocator = { version = "0.10", default-features = false }
```

Adding `pc-keyboard` in particular is the correct fix for the VirtualBox keyboard problem.
It decodes PS/2 scancode set 1 and 2 with proper handling of all edge cases (extended codes,
key release, pause key, print screen multi-byte sequences) and is used by several well-known
hobby OSes including blog_os.

---

## Complexity Estimates

| Phase | Effort (with crates) | Dependencies |
|-------|----------------------|--------------|
| Keyboard fix (`pc-keyboard`) | Very Low | — |
| Signals + TTY | Medium | Phase 1 fork/exec |
| FAT16 subdirs | Low | Phase 2.2 write |
| Text editor | Low–Medium | TTY/raw mode |
| ext2 read-only | Medium | VFS layer |
| Networking (`smoltcp`) | Medium | virtio-net driver |
| Userspace GUI | Medium | IPC + shared memory |
| SYSCALL/SYSRET | Low | GDT/MSR setup |
| SMEP/SMAP | Low | stable syscall path |
| SMP | High | Everything stable |

---

## File Layout After All Phases

```
kernel/src/
├── kernel/
│   ├── vfs.rs            ✅ done
│   ├── fat.rs            ✅ done (read+write)
│   ├── ext2.rs           ← Phase 6.2
│   ├── tty.rs            ← Phase 4.3
│   ├── signal.rs         ← Phase 4.1 full
│   ├── net/
│   │   ├── virtio.rs     ← Phase 7.1
│   │   └── smoltcp_glue.rs ← Phase 7.2
│   └── ipc.rs            ✅ done
├── gui/
│   ├── terminal.rs       ✅ done (real bash UI)
│   ├── start_menu.rs     ✅ done
│   ├── window_manager.rs ✅ done
│   └── compositor.rs     ✅ done
userspace/
├── oxide-rt/             ✅ done (syscall wrappers, _start)
├── sh/                   ✅ done (fork+exec shell)
├── bin/                  ← Phase 3.3 (standalone coreutils)
│   ├── cat, ls, cp, mv, rm, ps, kill, sleep
├── edit/                 ← Phase 3.4
└── gui/                  ← Phase 8.4
    ├── terminal
    └── file_manager
```
