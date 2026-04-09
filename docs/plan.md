# OxideOS — Roadmap to a Fully Functional OS

This document audits every subsystem in the current codebase, identifies what is missing,
and lays out a phased implementation plan to reach a fully functional general-purpose OS.
Each phase builds directly on the previous one so the OS is bootable and usable after every
milestone.

---

## Current State (as of this writing)

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
| FAT16 read-only (root directory, ATA PIO) | ✅ Complete |
| Anonymous pipes (8 pairs, 4 KB each) | ✅ Complete |
| Stdin ring buffer → GetChar syscall | ✅ Complete |
| PS/2 keyboard (full US QWERTY) | ✅ Complete |
| PS/2 mouse (packets, buttons, cursor) | ✅ Complete |
| Framebuffer graphics + back-buffer blit | ✅ Complete |
| Window manager (drag, focus, taskbar) | ✅ Complete |
| GUI terminal (21 commands, tab-complete) | ✅ Complete |
| Serial port debug output | ✅ Complete |

### Known gaps

| Subsystem | Gap |
|-----------|-----|
| Process model | No `fork`, `exec`, `waitpid` — can only `spawn` built-in binaries; per-task FD table ✅ |
| Memory | No `mmap`/`brk` — user programs cannot allocate heap dynamically |
| Filesystem | FAT16 write not implemented; no subdirectory traversal; no VFS layer |
| Signals | No signal delivery, no `Ctrl+C` interrupt to process |
| TTY | No TTY abstraction — keyboard goes straight to terminal widget |
| Shell | No userspace shell — `run` built into kernel terminal |
| Networking | None |
| Block device | ATA sector read only; no write; no partition support |
| Permissions | No users, no file permissions |
| Dynamic linking | Only static ELF — no shared libraries |
| SMP | Single core only |

---

## Phase 1 — Solid Process Model

**Goal:** Any ELF binary from disk can be loaded, forked, exec'd, and waited on.
This is the single most important gap — everything else depends on real processes.

### 1.1 `exec` syscall ✅ DONE

- `Exec = 5` added to syscall table and dispatch.
- `KernelRuntime::exec_program` resolves the path (built-in registry → RamFS → FAT16),
  creates a fresh CR3, maps stack, loads ELF or flat binary via `load_in`.
- Old CR3's user pages freed via new `paging_allocator::free_user_page_table`.
- Replaces current task by updating `task.{cr3, entry, first_run, fd_table}` then
  calling `exit_to_kernel(EXIT_PREEMPTED)` — non-local goto back to `tick()`.
  Next scheduler tick calls `launch_at(new_entry, stack, new_cr3)`.
- Supports: built-in registry (`hello`, `counter`, …), `/path` in RamFS, `/disk/` on FAT16.

### 1.2 `fork` syscall

- Add `Fork = 1` to the syscall table.
- Allocate a new task slot; call `create_user_page_table()` for the child.
- Copy-on-write is complex — for a first pass, do a **full copy**: walk the parent's
  user page table (entries 0–255 of L4), allocate fresh physical frames for each mapped
  page, and copy the content. Stack and code pages are both duplicated.
- Child inherits open FD table (copy the RamFS FD array).
- Return 0 to child, child PID to parent.
- Prerequisite: per-task FD table (currently RamFS uses a global FD table — move it
  into `Task`).

### 1.3 Per-task FD table ✅ DONE

- `FdTable` (`Copy`, `const`-constructible) extracted from `RamFs` and moved into `Task`.
- `RamFs` now owns only inodes; all FD state lives in `Task.fd_table`.
- `FdTable::open/close/read_fd/write_fd` take `&mut RamFs` / `&RamFs` as needed.
- Syscall `KernelRuntime` routes all FD ops to `SCHED.tasks[CURRENT_TASK_IDX].fd_table`.
- `spawn()` resets the table with `FdTable::new()` on each task launch.
- `FdTable::on_inode_removed(idx)` fixup helper added for callers of `remove_file`.
- Stdin/stdout/stderr (FDs 0/1/2) are reserved; real files start at FD 3.

### 1.4 `waitpid` syscall

- Add `Waitpid = 2`.
- Parent blocks (state → `Sleeping`) until child state becomes `Dead`.
- On wakeup, return child's exit code; mark child slot `Empty`.
- Implement as a poll loop in `tick()`: each tick check if the waited-on child is Dead.

### 1.5 Exit cleanup

- When a task exits, free its user-space physical frames (walk L4 indices 0–255,
  free every present leaf frame, then free intermediate table frames).
- Free the CR3 frame itself.
- This prevents physical memory exhaustion on long-running systems.

### Deliverable
```
> run /bin/sh          # userspace shell starts (phase 3)
$ fork_test            # forks, child prints, parent waits
```

---

## Phase 2 — Virtual Filesystem (VFS)

**Goal:** One unified file abstraction — same `open`/`read`/`write`/`close` regardless
of whether the file lives in RamFS, on the FAT16 disk, or on a future ext2 partition.

### 2.1 VFS layer

Create `kernel/src/kernel/vfs.rs`:

```
VNode {
    kind: File | Dir | Device | Symlink,
    ops:  &'static VnodeOps,   // open/read/write/readdir/stat/…
    data: *mut (),             // filesystem-private state
}

VnodeOps {
    open:    fn(&VNode, flags) -> Result<FD>,
    read:    fn(&VNode, offset, buf) -> Result<usize>,
    write:   fn(&VNode, offset, buf) -> Result<usize>,
    readdir: fn(&VNode) -> Vec<DirEntry>,
    stat:    fn(&VNode) -> Stat,
    create:  fn(&VNode, name, kind) -> Result<VNode>,
    unlink:  fn(&VNode, name) -> Result<()>,
}
```

- Register mount points: `/` → RamFS, `/disk` → FAT16 (when ATA present).
- Path resolution walks the mount table then follows directory entries.
- All existing syscalls (`Open`, `Read`, `Write`, `Close`) route through VFS.

### 2.2 FAT16 write support

Implement write-back to the ATA disk:

- **Allocate cluster**: scan the FAT for a free entry (value `0x0000`), mark it
  `0xFFFF` (end-of-chain).
- **Write sector**: call `ata::write_sector`.
- **Update directory entry**: find the file's 32-byte entry in the root dir, update
  file size and first-cluster fields.
- **Flush**: issue ATA FLUSH CACHE (`0xE7`) after writes.
- Expose via VFS `write` op on FAT vnodes.

### 2.3 FAT16 subdirectory support

- Parse `ATTR_DIRECTORY` (0x10) entries; follow their cluster chain to read sub-dir sectors.
- Implement `readdir` for subdirectories.
- Make `cd /disk/bin/` work in the terminal.

### 2.4 `/dev` device filesystem

- Mount a simple devfs at `/dev`.
- `/dev/null` — reads return 0 bytes; writes discard.
- `/dev/zero` — reads return zeroed bytes.
- `/dev/tty` — reads from stdin ring; writes to terminal output.
- `/dev/disk0` — raw block device backed by ATA.

### 2.5 `stat` / `fstat` syscalls

- `Stat = 70`, `Fstat = 71`.
- Return `Stat { size, kind, permissions, inode }`.

### Deliverable
```
$ echo hello > /disk/greeting.txt   # writes to FAT16
$ cat /disk/greeting.txt            # reads back
$ ls /disk/bin/                     # subdirectory listing
```

---

## Phase 3 — Userspace Shell & Standard Programs

**Goal:** The kernel `run` command is replaced by a real userspace shell that can execute
arbitrary programs, pipe output, and redirect I/O.

### 3.1 Toolchain

To compile C or Rust programs that target OxideOS:

- Write a minimal `libc.h` / `syscall.h` that wraps `int 0x80` calls.
- Alternatively, write a small Rust `no_std` runtime crate (`oxide-rt`) that implements
  `_start`, `exit`, `write`, `read`, `open`, `close`.
- Programs compile with `--target x86_64-unknown-none`, linked with a custom linker
  script that sets `BASE = 0x400000`.
- Add a `Makefile` target `make programs` that builds all userspace ELF binaries and
  `mcopy`s them to `oxide_disk.img` under `/bin/`.

### 3.2 Shell (`/bin/sh`)

A minimal POSIX-ish shell written in C or Rust no_std:

- **Prompt**: print `$`, read a line via `read(0, buf, N)`.
- **Tokenise**: split on whitespace, handle `>`, `>>`, `<`, `|`.
- **Execute**: `fork` + `exec /bin/<cmd>` + `waitpid`.
- **Pipes**: call `pipe()`, fork two children, dup2 read/write ends, exec both sides.
- **Redirects**: `open` file, `dup2` FD to 0 or 1 before `exec`.
- Built-ins: `cd`, `pwd`, `exit`, `echo`.

### 3.3 Core utilities

| Program | Purpose |
|---------|---------|
| `/bin/echo` | Print arguments |
| `/bin/cat` | Read and print files |
| `/bin/ls` | Directory listing |
| `/bin/cp` | Copy file |
| `/bin/mv` | Move/rename file |
| `/bin/rm` | Delete file |
| `/bin/mkdir` | Create directory |
| `/bin/pwd` | Print working directory |
| `/bin/ps` | List processes |
| `/bin/kill` | Send signal to process |
| `/bin/sleep` | Sleep N seconds |
| `/bin/true` / `/bin/false` | Exit 0 / Exit 1 |

### 3.4 Text editor (`/bin/edit`)

A terminal-based text editor (nano-like):

- Full-screen mode: clear terminal, draw lines with line numbers.
- Arrow keys move cursor; `Ctrl+S` saves; `Ctrl+Q` quits.
- Read file into a `Vec<String>` line buffer; write back on save.
- Display status bar: filename, line/col, modified flag.

### 3.5 `dup2` / `dup` syscalls

- `Dup = 80`, `Dup2 = 81`.
- Copy FD to a new number; used by shell for pipe/redirect setup.

### 3.6 `chdir` / `getcwd` syscalls

- `Chdir = 82`, `Getcwd = 83`.
- Each task tracks a current working directory VNode; path resolution is relative to it.
- Shell `cd` uses `Chdir`; prompt uses `Getcwd`.

### Deliverable
```
$ ls /bin
$ cat /etc/motd
$ echo "hello" | cat
$ edit /disk/notes.txt
```

---

## Phase 4 — Signals & TTY

**Goal:** Processes can be interrupted, killed, and managed the way POSIX programs expect.

### 4.1 Signal infrastructure

- Add a `pending_signals: u32` bitmask to `Task` (one bit per signal 1–31).
- Add `signal_handlers: [u64; 32]` — user-space handler addresses (0 = default, 1 = ignore).
- `sigaction` syscall (`= 90`): set handler for signal N.
- `kill` syscall (`= 91`): send signal to PID (sets bit in target task's `pending_signals`).
- Before resuming any user task in `tick()`, check `pending_signals`; if nonzero, deliver
  the highest-priority pending signal by injecting a trampoline frame onto the user stack.

### 4.2 Default signal actions

| Signal | Default | Use |
|--------|---------|-----|
| SIGKILL (9) | Terminate | Unconditional kill |
| SIGTERM (15)| Terminate | Graceful kill |
| SIGINT (2) | Terminate | Ctrl+C |
| SIGCHLD (17)| Ignore | Child exited |
| SIGSEGV (11)| Terminate | Page fault |

### 4.3 Ctrl+C → SIGINT

- In the keyboard ISR: if `Ctrl+C` is detected and a foreground process group exists,
  send `SIGINT` to that group instead of pushing to the stdin ring.
- This requires a concept of a "foreground PID" (set by the shell after `fork`+`exec`).

### 4.4 TTY subsystem

Create `kernel/src/kernel/tty.rs`:

- A TTY owns an input queue (line-discipline) and an output queue.
- **Canonical mode** (cooked): buffer input until `\n`; handle `Backspace`/`Ctrl+C`/`Ctrl+D`.
- **Raw mode**: pass every byte immediately (used by editors and the shell readline).
- `ioctl` syscall (`= 92`): `TCGETS`/`TCSETS` to switch modes.
- `/dev/tty` device file routes through the TTY subsystem.
- Each task inherits a TTY pointer (or `None` if background); FDs 0/1/2 point to it.

### 4.5 Page fault handler → SIGSEGV

- The existing `#PF` handler (IDT vector 14) currently panics.
- If the fault is in user space (CS & 3 == 3), send `SIGSEGV` to the current task
  instead of halting the kernel.

### Deliverable
```
$ run_forever &         # background process
$ kill 3                # sends SIGTERM → process exits
$ Ctrl+C               # sends SIGINT to foreground
```

---

## Phase 5 — Dynamic Memory for User Programs

**Goal:** User programs can call `malloc`/`free` (or Rust's allocator) without the kernel
pre-mapping a fixed region.

### 5.1 `brk` / `sbrk` syscall

- Add `Brk = 9`.
- Each task tracks `heap_end: u64` starting just above the last loaded ELF segment.
- `brk(new_end)`: if `new_end > heap_end`, map additional pages; update `heap_end`.
- Userspace `malloc` (a tiny bump allocator) calls `sbrk` when it needs more space.

### 5.2 `mmap` (anonymous)

- Add `Mmap = 10`, `Munmap = 11`.
- For `MAP_ANONYMOUS | MAP_PRIVATE`: find a free virtual range above `heap_end`,
  call `map_user_region_in` with zeroed pages, return the virtual address.
- `Munmap`: unmap pages, free physical frames.
- Thread stacks, `malloc` arenas, and `dlopen` will use `mmap`.

### 5.3 Userspace allocator

Ship a minimal `malloc.c` or `alloc.rs` as part of the OS standard library:

```c
// A simple sbrk-based bump allocator with free-list
void *malloc(size_t n);
void  free(void *p);
void *realloc(void *p, size_t n);
```

### 5.4 Stack growth (optional)

- The kernel can detect stack-overflow page faults (address just below the mapped stack)
  and grow the stack by one page automatically, up to a configurable limit.

### Deliverable
```c
// user program
int *arr = malloc(1000 * sizeof(int));
// ... use it ...
free(arr);
```

---

## Phase 6 — Extended Filesystem & Persistence

**Goal:** A proper on-disk filesystem that supports directories, permissions, and large files.

### 6.1 ext2 filesystem driver

FAT16 is limiting (8.3 names, no permissions, root-dir-only subdirs). Implement a
read/write ext2 driver:

- **Superblock** at byte offset 1024: magic `0xEF53`, block size, inode count.
- **Block group descriptors** immediately after superblock.
- **Inode table**: 128-byte inodes with `i_mode`, `i_size`, `i_block[15]`.
- **Direct + indirect blocks** for file data.
- **Directory entries**: 4-byte inode, 2-byte rec_len, 1-byte name_len, name.
- Start with read-only; add write in a second pass (bitmap allocation).

### 6.2 Partition table (MBR)

- Parse the 64-byte MBR partition table at LBA 0 offset 446.
- Find the first `0x83` (Linux ext2) or `0x06` (FAT16) partition.
- Pass the partition start LBA + size to the filesystem driver.
- This allows a single `oxide_disk.img` to hold both a FAT16 boot partition
  and an ext2 root partition.

### 6.3 File permissions

- Add `uid: u16`, `gid: u16`, `mode: u16` to VNode / inode.
- Each task carries `uid` and `gid` (initially 0 = root for all).
- Permission check on `open`: verify `(mode >> shift) & 0x7 & requested`.
- `chmod` (`= 93`), `chown` (`= 94`) syscalls.

### 6.4 Symlinks and hard links

- `symlink` syscall: create a VNode of kind `Symlink`, stores a target path string.
- VFS path resolution follows symlinks (with a depth limit of 8).
- `link` syscall: increment inode reference count; add a new directory entry.

### 6.5 `rename` syscall

- Atomic rename within the same filesystem.

### Deliverable
```
$ mkfs.ext2 /dev/disk0p2     # format second partition
$ mount /dev/disk0p2 /home   # mount at /home
$ ls /home/user/             # full ext2 directory tree
$ chmod 600 /home/user/.key
```

---

## Phase 7 — Networking

**Goal:** Basic TCP/IP so the OS can fetch a web page or host a simple server.

### 7.1 virtio-net driver

- Detect virtio-net PCI device (vendor `0x1AF4`, device `0x1000`).
- Read BAR0 (I/O base); negotiate features (VIRTIO_NET_F_MAC).
- Set up two virtqueues (RX + TX) with DMA-accessible descriptor tables.
- Implement `send_packet(buf)` and `recv_packet() -> Option<Vec<u8>>`.
- Wire receive IRQ to a new ISR (unmask the appropriate PCI IRQ line).

Alternative: RTL8139 driver (simpler, widely emulated):
- I/O port at BAR0; 4 RX descriptors, TX circular buffer.
- QEMU flag: `-netdev user,id=net0 -device rtl8139,netdev=net0`

### 7.2 Network stack (`kernel/src/net/`)

**Ethernet layer:**
- Parse/build Ethernet II frames (dst MAC, src MAC, EtherType).
- ARP table: map IPv4 → MAC; send ARP requests; handle ARP replies.

**IPv4:**
- Parse IP header (version, IHL, TTL, protocol, src/dst IP).
- Implement ICMP echo reply (ping response).
- Checksum calculation.

**UDP:**
- Parse UDP header; dispatch to registered port handlers.
- `udp_send(dst_ip, dst_port, src_port, data)`.

**TCP (basic):**
- State machine: CLOSED → SYN_SENT → ESTABLISHED → FIN_WAIT → CLOSED.
- Three-way handshake for connections.
- Sliding window (fixed size, no congestion control for first pass).
- `tcp_connect(ip, port) -> Socket`, `tcp_listen(port) -> Socket`.

### 7.3 Socket syscalls

- `Socket = 100`, `Bind = 101`, `Connect = 102`, `Listen = 103`, `Accept = 104`.
- `Send = 105`, `Recv = 106`, `Close` reuses existing FD close.
- Sockets appear as file descriptors; `read`/`write` work on them.

### 7.4 DHCP client

- On boot (or on `ifup`): broadcast DHCP DISCOVER, parse OFFER, send REQUEST, use ACK
  to configure IP/mask/gateway/DNS.

### Deliverable
```
$ ping 8.8.8.8            # ICMP echo
$ wget http://example.com # TCP + HTTP/1.0
$ nc -l 8080              # netcat TCP listener
```

---

## Phase 8 — Multi-Window GUI Applications

**Goal:** Multiple GUI apps run in separate processes, each drawing into their own window.

### 8.1 Shared framebuffer / window protocol

- The kernel window manager owns the framebuffer.
- User processes communicate with it via a **message-passing IPC** (see 8.3).
- Each process gets a "canvas" — a shared memory region mapped into both the process
  and the compositor.

### 8.2 Shared memory

- `shmget` (`= 110`) / `shmat` (`= 111`) / `shmdt` (`= 112`).
- Kernel allocates physical frames, maps them into two different virtual address spaces.
- Used for the window-canvas protocol and for IPC data transfer.

### 8.3 Message-passing IPC

- `msgq_create` (`= 115`), `msgsnd` (`= 116`), `msgrcv` (`= 117`).
- Fixed-size message queue in kernel memory.
- Window manager exposes a well-known queue ID; apps post `CreateWindow`, `DrawRect`,
  `PresentCanvas`, `DestroyWindow` messages.

### 8.4 GUI applications

With the above primitives, port existing kernel widgets to userspace:

| App | Description |
|-----|-------------|
| `wm` | Compositor / window manager process (replaces kernel WM) |
| `terminal` | Terminal emulator process (replaces kernel terminal) |
| `file_manager` | Browse RamFS + FAT16 visually |
| `text_editor` | Full-screen editor with syntax highlighting |
| `clock` | Floating clock widget |
| `settings` | Change resolution, key repeat, etc. |

### Deliverable
Multiple resizable, draggable windows, each running an independent process.

---

## Phase 9 — Stability, Security & Polish

**Goal:** The OS is robust, doesn't crash on bad input, and enforces basic security.

### 9.1 Kernel address-space layout

- Move to a proper kernel ASLR: randomise the physical offset at boot.
- Guard pages around kernel stack (unmapped page below RSP0).

### 9.2 SMEP / SMAP enforcement

- Set SMEP bit in CR4 to prevent kernel from executing user-space code.
- Set SMAP bit in CR4 to prevent kernel from reading user-space without explicit `stac`.
- Use `copy_from_user` / `copy_to_user` helpers in syscall handlers.

### 9.3 Capabilities / privilege separation

- `setuid` / `setgid` syscalls.
- Processes run as unprivileged by default; only `root` (uid=0) can `mount`, `mknod`,
  bind ports <1024.

### 9.4 `fast syscall` path (`syscall`/`sysret`)

- Replace `int 0x80` with the `syscall`/`sysret` instruction pair.
- Set MSRs: `STAR`, `LSTAR`, `SFMASK`.
- ~3× faster syscall round-trip on modern CPUs.

### 9.5 SMP (optional, advanced)

- AP startup via INIT-SIPI sequence.
- Per-CPU `SCHED` instances; run-queue migration.
- Spinlock / mutex primitives for shared kernel data.

### 9.6 ACPI

- Parse ACPI tables from firmware (RSDP → RSDT/XSDT → MADT, FADT).
- Power off: `ACPI_PM1a_CNT` shutdown sequence.
- `shutdown` command in shell triggers ACPI S5 state.

### 9.7 Crash dump / kernel panic improvements

- On panic: save register state, dump to serial port and screen.
- Optionally write a crash dump to `/var/crash` if the filesystem is available.

---

## Implementation Priority Order

The following is the recommended sequence to work through these phases, ordered so each
addition has maximum visible impact:

```
Phase 1.3  Per-task FD table             ✅ DONE
Phase 1.1  exec syscall                  ✅ DONE
Phase 3.1  Toolchain (libc + build)      ← compile real programs    ← NEXT
Phase 3.1  Toolchain (libc + build)      ← compile real programs
Phase 2.1  VFS layer                     ← unified file access
Phase 2.2  FAT16 write                   ← persistent storage
Phase 1.2  fork syscall                  ← real process creation
Phase 1.4  waitpid                       ← process lifecycle
Phase 3.2  Shell (/bin/sh)               ← real command interpreter
Phase 5.1  brk / sbrk                    ← malloc in userspace
Phase 4.1  Signals (SIGKILL, SIGINT)     ← Ctrl+C, kill
Phase 4.4  TTY (canonical/raw mode)      ← proper line editing
Phase 3.3  Core utilities                ← ls, cat, cp, rm, …
Phase 3.4  Text editor                   ← edit files interactively
Phase 4.2  Page fault → SIGSEGV         ← survive bad programs
Phase 6.1  ext2 driver (read-only)       ← better filesystem
Phase 6.2  Partition table               ← real disk layout
Phase 5.2  mmap (anonymous)              ← richer allocator
Phase 7.1  virtio-net driver             ← network hardware
Phase 7.2  Network stack (ARP/IP/UDP)    ← basic networking
Phase 7.3  Socket syscalls               ← userspace networking
Phase 8.3  Message-passing IPC           ← GUI app protocol
Phase 8.1  Userspace compositor          ← multi-app GUI
Phase 9.4  fast syscall path             ← performance
Phase 9.2  SMEP/SMAP                     ← security
```

---

## Complexity Estimates

| Phase | Effort | Dependencies |
|-------|--------|--------------|
| 1 — Process model | Medium | Per-task FD table first |
| 2 — VFS | Medium | Phase 1 exec path |
| 3 — Shell & utils | Medium | Phases 1 + 2 complete |
| 4 — Signals & TTY | Medium | Phase 1 fork/exec |
| 5 — Dynamic memory | Low–Medium | Phase 1 |
| 6 — ext2 / partitions | High | VFS layer |
| 7 — Networking | High | Independent of above |
| 8 — GUI apps | Medium | IPC + shared memory |
| 9 — Security & SMP | High | Everything else stable |

---

## File Layout After All Phases

```
kernel/src/
├── kernel/
│   ├── vfs.rs            ← Phase 2.1
│   ├── ext2.rs           ← Phase 6.1
│   ├── tty.rs            ← Phase 4.4
│   ├── signal.rs         ← Phase 4.1
│   ├── net/
│   │   ├── virtio.rs     ← Phase 7.1
│   │   ├── arp.rs
│   │   ├── ipv4.rs
│   │   ├── tcp.rs
│   │   └── udp.rs
│   └── ipc.rs            ← Phase 8.3
userspace/
├── libc/                 ← Phase 3.1
│   ├── syscall.h
│   └── malloc.c
├── sh/                   ← Phase 3.2
├── coreutils/            ← Phase 3.3
│   ├── ls.c, cat.c, …
├── edit/                 ← Phase 3.4
└── gui/                  ← Phase 8.4
    ├── wm.c
    └── terminal.c
```
