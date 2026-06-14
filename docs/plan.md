# OxideOS — Roadmap to a Fully Functional OS

This document audits every subsystem, tracks completed work, and lays out a
phased plan to reach a production-grade general-purpose OS. Each phase is
designed so the OS remains bootable and usable after every milestone.

---

## Current State (June 2026)

### What works today

| Subsystem | Status |
|-----------|--------|
| 64-bit boot (Limine v9, UEFI/BIOS) | ✅ |
| GDT / TSS / IDT | ✅ |
| PIC, PIT at 100 Hz | ✅ |
| SMEP (CR4 bit 20) + NX bit on PTEs | ✅ |
| Physical frame allocator (256 MB bitmap) | ✅ |
| Per-process page tables (CR3 per task) | ✅ |
| **Copy-on-write fork** — refcounted shared frames, COW page-fault resolver | ✅ |
| User mode (Ring 3, iretq) | ✅ |
| int 0x80 + SYSCALL/SYSRET fast path | ✅ |
| Preemptive scheduler — 8-task round-robin | ✅ |
| ELF64 loader (ET_EXEC, static) + argv/envp (full SysV AMD64 ABI) | ✅ |
| Linux x86-64 syscall ABI — 80+ syscalls at Linux numbers | ✅ |
| RamFS — in-memory tree, FHS-lite (`/bin /etc /tmp /home`), 32 open FDs | ✅ |
| FAT16 read + write (subdirs, ATA PIO), mounted at `/disk` | ✅ |
| ext2 read (superblock, BGDT, inodes, direct blocks) + **partial write** | ⚠️ |
| MBR partition table (4 entries, type detection) | ✅ |
| VFS layer — `/dev/null`, `/dev/tty`, mount table, procfs, diskfs | ✅ |
| procfs — `/proc/version`, `cpuinfo`, `meminfo`, `uptime`, `mounts` (system-wide only, no per-PID) | ⚠️ |
| diskfs — `/store` (live on-disk record view), `/diskinfo` | ✅ |
| Anonymous pipes (8 pairs, 4 KB) + shell pipes `cmd1 \| cmd2 \| ...` | ✅ |
| fork / exec / waitpid / exit cleanup | ✅ |
| Job control — `&` background, `jobs`, `fg N`, SIGCHLD, pgid tracking | ✅ |
| Per-task FD table, dup2, fcntl | ✅ |
| brk/sbrk heap, mmap anonymous, **real munmap** (unmap + free frames + invlpg) | ✅ |
| Full POSIX signals — sigaction, sigreturn, trampoline, sigprocmask, sigsuspend | ✅ |
| select / poll / pselect6 | ✅ |
| TTY — termios, ioctl TCGETS/TCSETS/TIOCGWINSZ, canonical/raw mode | ✅ |
| PS/2 keyboard (pc-keyboard crate) + mouse | ✅ |
| Framebuffer + double-buffered compositor | ✅ |
| GUI — window manager: drag, resize (edge/corner), snap-to-half, z-order, start menu, taskbar, Activities overview | ✅ |
| **Multi-window per-process GUI** — `gui_proc`, syscalls 125–132 (GuiCreate/Destroy/FillRect/DrawText/Present/PollEvent/GetSize/BlitShm) | ✅ |
| Desktop apps — Notepad (menu bar, find, word wrap, clipboard), Terminal, File Manager, System Monitor (`sysmon`), Browser, Calendar, Notifications, Quick Settings | ✅ |
| IPC — message queues (compositor protocol), shared memory (shmget/shmat/shmdt/shmctl) | ✅ |
| RTL8139 + Intel e1000 + AMD PCnet NIC drivers (auto-detected) + smoltcp (TCP/UDP/ICMP/DHCP/ARP) | ✅ |
| DHCP client — automatic IP configuration on boot | ✅ |
| DNS resolver — UDP A-record query, kernel syscall 435, `oxide-rt::dns_resolve()` | ✅ |
| Socket syscalls — socket/bind/connect/listen/accept/send/recv/sendto/recvfrom | ✅ |
| `/bin/wget` (hostname + URL), `/bin/nc`, `/bin/ping` | ✅ |
| File permissions — mode/uid/gid on RamFS inodes, chmod/chown | ⚠️ stored but **not enforced** (getuid always returns 1000) |
| unlink/rename/truncate syscalls | ✅ |
| ACPI shutdown — RSDP→FADT→PM1a_CNT_BLK | ✅ |
| BSoD crash dump — framebuffer + serial register dump | ✅ |
| Shell `/bin/sh` — fork+exec, pipes, `>`/`>>` redirect, `$VAR`, `export`, job control | ✅ |
| Coreutils — ls cat cp mv rm mkdir pwd ps echo grep wc head tail sort sleep kill touch true false | ✅ |
| Text editor `/bin/edit` — nano-like, VT100 | ✅ |
| **musl libc programs** — `hello_musl`, `musl_test` | ✅ |
| **Lua 5.4.7** (embedded, REPL + scripts) | ✅ |
| **BusyBox 1.36.1** (embedded, 300+ applets) | ✅ |
| **GNU Bash 5.2** (embedded) | ✅ |
| **CPython 3.12** (userspace, loaded from `/disk` at runtime via PATH fallback) | ✅ |
| Installable OS — `/bin/install`, `make install-image`, MBR + FAT32(EFI)/FAT16(data) layout | ✅ |
| Kernel heap allocator — **bump allocator, never frees** | ⚠️ |

### Remaining gaps (priority order)

| Gap | Blocks |
|-----|--------|
| **ext2 write completion** — block/inode allocation, directory-entry insert/delete, file create/truncate/append, write-back | Real persistence, Phase 22 prerequisite |
| **Kernel heap allocator** — replace bump allocator with `linked_list_allocator` (frees memory) | Long-running stability |
| **Physical frame free list** — O(1) alloc/free instead of bitmap scan | Memory efficiency at scale |
| **procfs per-process** — `/proc/PID/status`, `/proc/PID/maps`, `/proc/PID/fd/` | Accurate `ps`/`top`, debugging |
| **Block cache (page cache)** | Disk I/O performance |
| **Symbolic & hard links** | POSIX completeness, `ls -l` parity |
| **Users & groups** — real uid/gid enforcement, `/etc/passwd`, `login`, `su` | Security, multi-user |
| **ASLR** | Security hardening |
| **SMP** (LAPIC + INIT-SIPI, per-CPU scheduler) | Performance on modern CPUs |
| **AHCI/SATA** (replace ATA PIO) | Real-hardware disk performance |
| **USB keyboard/mouse (XHCI)** | Real-hardware input |
| **Audio** (Intel HDA) | Multimedia |
| **Dynamic ELF linking** (`.so`, `ld.so`, oxide-libc) | Binary compatibility, smaller binaries |
| **Window server protocol v2** — cross-window clipboard, drag-and-drop, decorations, exposed/focus events | GUI maturity |
| **Init system + login** — PID 1, `/etc/rc.d`, multi-user sessions | OS maturity |
| **Package manager + self-hosted compiler** | Self-hosting |

---

## Available `no_std` Crates

| Crate | Status | Purpose |
|-------|--------|---------|
| `pc-keyboard` | ✅ in use | PS/2 scancode decoding |
| `smoltcp` | ✅ in use | TCP/IP stack |
| `oxide-gui-core` | ✅ in use | GUI widget framework backend |
| `lazy_static` | ✅ in use | Static initialization with locks |
| `limine` | ✅ in use | Bootloader protocol |
| `png` | ✅ in use | Decode bundled image assets (wallpapers, icons) |
| `acpi` | not yet used | Parse MADT for SMP AP discovery (Phase 17) |
| `virtio-drivers` | not yet used | VirtIO-net/blk for QEMU (Phase 19.6) |
| `x86_64` | not yet used | Safe CR3/VirtAddr/PageTable wrappers |
| `linked_list_allocator` | not yet used | Replace kernel bump allocator (Phase 11.5) |
| `heapless` | not yet used | Fixed-capacity Vec/String in kernel data structures |
| `noto-sans-mono-bitmap` | not yet used | High-quality Unicode font for GUI (Phase 16.3) |
| `libm` | not yet used | Float math (sin/cos/sqrt) for GUI effects |
| `miniz_oxide` | not yet used | Deflate — compress initrd / ELF binaries |
| `postcard` | not yet used | Compact IPC message serialization |
| `sha2` | not yet used | File integrity, future auth |
| `chacha20poly1305` | not yet used | Authenticated encryption |

---

## COMPLETED PHASES

### ✅ Phase 1 — Process Model
fork, exec, waitpid, exit, per-task FD table, dup2

### ✅ Phase 2 — VFS & Filesystem
RamFS, FAT16 r/w + subdirs, ext2 read-only, MBR, VFS mount table, /dev/null, /dev/tty

### ✅ Phase 3 — Userspace Shell & Tools
oxide-rt, /bin/sh, /bin/edit, ls/cat/cp/mv/rm/mkdir/pwd/ps/wget/nc

### ✅ Phase 4 — Signals & TTY
Full POSIX signals (sigaction/sigreturn/trampoline), kill, SIGINT via Ctrl+C, TTY termios

### ✅ Phase 5 — Dynamic Memory
brk/sbrk, mmap anonymous, userspace bump allocator in oxide-rt

### ✅ Phase 6 — Extended Filesystem
ext2 read-only, MBR partition parsing, file permissions (chmod/chown/mode/uid/gid)

### ✅ Phase 7 — Networking
RTL8139 driver, smoltcp integration, socket syscalls (TCP+UDP), /bin/wget, /bin/nc

### ✅ Phase 8 — GUI & IPC
Window manager, compositor IPC, shared memory, MSG_BLIT_SHM, start menu, userspace terminal

### ✅ Phase 8.4 — Userspace GUI API
Per-process window syscalls (GuiCreate/Destroy/FillRect/DrawText/Present/PollEvent/GetSize/BlitShm, syscalls 125–132), keyboard/mouse event routing to focused window, `gui_proc` kernel module, `oxide-rt` GUI bindings, `/bin/filemanager` GUI file manager

### ✅ Phase 9 — Stability & Security
SYSCALL/SYSRET, SMEP, ACPI proper shutdown, BSoD crash dump, ATA alignment fix

### ✅ Phase 10.1 — argv/argc Passing
System V AMD64 ABI argv block written to user stack by kernel. `oxide-rt::arg(i)` / `argc()` helpers. `ExecArgs` syscall 6 lets shell pass argv[1..] to exec'd programs.

### ✅ Phase 10.2 — Environment Variables
Global env store in kernel (32 vars, 256-byte values). Getenv (79) / Setenv (80) syscalls.
`oxide-rt::getenv_bytes()` / `setenv()` wrappers. Shell: `export VAR=val`, `$VAR` expansion.
Default env: PATH=/bin, HOME=/, TERM=vt100, USER=oxide, SHELL=/bin/sh, HOSTNAME=oxideos.

### ✅ Phase 10.3 — Shell Pipes
Pipeline execution up to 8 stages: `cmd1 | cmd2 | ... | cmdN`.
fork/dup2/pipe plumbing in sh. Output redirect applies to last stage.

### ✅ Phase 10.4 — Job Control
`&` background, `jobs` builtin, `fg N`, SIGCHLD delivery, pgid tracking, getpgid/setpgid.

### ✅ Phase 10.5 — More Coreutils
echo, grep, wc (-l/-w/-c), head (-n N), tail (-n N), sort, sleep, kill, touch, true, false.

### ✅ Phase 10.6 — Open-Source Software Support
Linux x86-64 syscall ABI (80+ syscalls renumbered), select/poll/pselect6, real munmap,
full envp on stack, musl libc cross-compilation, **Lua 5.4.7** embedded, **BusyBox 1.36.1**
embedded (getdents64, struct stat, FdBackend::Dir), **GNU Bash 5.2** embedded
(select/pselect6, SIGCHLD, pgid), **CPython 3.12** running from `/disk` via PATH fallback.

### ✅ Phase 11.1 — Copy-on-Write Fork
Parent/child share refcounted physical frames after `fork()`; pages marked read-only +
COW bit. Page-fault handler (`try_resolve_cow_fault`) allocates a private copy on first
write. Stack range and shared-memory ranges are excluded from COW (deep-copied / kept
shared respectively). `kernel/src/kernel/mem/paging_allocator.rs`,
`kernel/src/kernel/proc/scheduler.rs::fork_task`.

### ✅ Phase 11.2 — Real munmap
`munmap_impl` unmaps PTEs, decrements/frees physical frames, flushes TLB (`invlpg`) for
every unmapped page, with per-task mmap region tracking. `kernel/src/kernel/sys/syscall.rs`.

### ✅ Phase 12.3 — procfs (basic, system-wide)
`/proc/version`, `/proc/cpuinfo`, `/proc/meminfo`, `/proc/uptime`, `/proc/mounts` synthesised
on demand. Per-process `/proc/PID/*` is **not yet implemented** — see Phase 12.3b below.

### ✅ Phase 13.1/13.2 — DHCP + DNS
DHCP client auto-configures IP on boot (fallback static 10.0.2.15/24). DNS resolver sends
UDP A-record queries via syscall 435; `oxide-rt::dns_resolve()`; `/bin/wget` accepts
hostnames and URLs.

### ✅ Phase 13.3 — select/poll
`poll(fds, nfds, timeout_ms)` (syscall 7) and `select`/`pselect6` (syscalls 23/270)
implemented; used by Bash for interactive job control.

### ✅ Phase 14.1 — Remaining POSIX Signals
sigprocmask, sigsuspend, SIGCHLD delivery on child exit (enables Bash job control).

### ⚠️ Phase 16.1 (partial) — Multi-Window GUI
Each userspace process gets its own window via `gui_proc` (syscalls 125–132). The window
manager supports drag, edge/corner resize, snap-to-half, and z-order. **Not yet done**:
formal client/server protocol messages (Exposed, FocusChange, CloseRequest), cross-window
clipboard, drag-and-drop — see Phase 16.1b below.

### ✅ Phase 22 — Installable OS
`/bin/install`, `make install-image` (192 MB MBR/FAT32+FAT16 image), `make install-vdi`,
pre-built ISO releases. See Phase 22 section for the full ext2-root variant (still pending
on Phase 12.1).

---

## Phase 11 — Memory Management V2 (remaining work)

**Goal:** Correct, efficient memory handling for multi-process workloads.

### ✅ 11.1 Copy-on-Write fork — DONE (see Completed Phases)

### ✅ 11.2 munmap (real implementation) — DONE (see Completed Phases)

### 11.3 mmap file-backed regions
- `mmap(addr, len, prot, MAP_PRIVATE, fd, offset)` — map a file into address space.
- Read-only initially; COW on write (MAP_PRIVATE) or write-through (MAP_SHARED).
- Required for dynamic ELF loading (Phase 15) and for running larger interpreters
  (CPython currently loads its whole image via `read()`, not `mmap`).

### 11.4 Demand paging & page-level heap growth
- Currently `brk` maps all requested pages eagerly.
- Change to: map only the first page; install a fault handler that maps subsequent pages
  lazily.
- Reduces memory pressure for programs (e.g. Python, Bash) that allocate large buffers but
  use them sparsely.

### 11.5 Linked-list kernel heap allocator  ← HIGH PRIORITY
The kernel currently uses a bump allocator (`kernel/src/kernel/mem/allocator.rs`) that
**never frees** memory. Every kernel-side `Vec`/`String`/`Box` allocation (sockets, IPC
queues, GUI window state, ext2 metadata) permanently consumes heap. Replace with:
```toml
linked_list_allocator = { version = "0.10", default-features = false }
```
- Enables the kernel to free heap memory.
- Critical for long-running systems (desktop sessions, servers) where the kernel
  allocates/frees many short-lived data structures (per-process state, GUI buffers,
  socket entries).

### 11.6 Physical frame free list
- `alloc_frame()` scans the 256 MB bitmap linearly — O(N) worst case.
- Replace with a stack-based free list for O(1) alloc/free.
- `free_frame(phys)` already exists for COW/munmap; extend the free list to also serve
  process-exit cleanup and `fork`-time allocation.

---

## Phase 12 — Filesystem V2

**Goal:** Writable ext2, persistent storage, procfs, symbolic links.

### 12.1 ext2 write support — IN PROGRESS  ← HIGH PRIORITY
Current state (`kernel/src/kernel/fs/ext2.rs`): `write_fd()` exists but **only overwrites
data within already-allocated direct blocks** of an existing file — no new block
allocation, no inode allocation, no directory-entry insertion/deletion, no file creation.

Remaining work:
- `ext2_write_block(block_no, buf)` — ATA sector write through block layer (the low-level
  `write_block_from_scratch` primitive already exists; needs wiring to dirty-block
  allocation).
- Inode `atime`/`mtime`/`ctime` update on read/write.
- Allocate new data blocks (scan block bitmap, update superblock free-block count) when a
  write extends past the current allocation.
- Allocate new inodes (scan inode bitmap) for `ext2_create(path)`.
- Directory entry insertion (`creat`, `mkdir`) and deletion (`unlink`, `rmdir`).
- File truncation and append (`O_TRUNC`, `O_APPEND`).
- Write-back: dirty inode/bitmap/superblock flushed on `close()` or `sync`.
- `sync` syscall (=162): flush all dirty buffers to disk.

Files: `kernel/src/kernel/fs/ext2.rs`, `kernel/src/kernel/fs/vfs.rs` (route writes to ext2
backend for files opened under `/ext2`).

### 12.2 Block cache (page cache)
- Without a cache, every read hits ATA PIO — ~1 ms per sector.
- Add a 64-entry LRU block cache (`[Option<CachedBlock>; 64]`).
- Cache keyed by (device_id, block_no). Hit: return cached data. Miss: read + insert.
- On write: mark block dirty; flush on eviction or `sync`.
- Benefits both FAT16 and ext2 backends.

### 12.3b procfs — per-process `/proc/PID/*`
System-wide `/proc/{version,cpuinfo,meminfo,uptime,mounts}` already exist (Phase 12.3 ✅).
Add per-process entries:

| Path | Content |
|------|---------|
| `/proc/PID/status` | PID, PPID, state, memory usage |
| `/proc/PID/maps` | Virtual memory regions (from per-task mmap region list, Phase 11.6) |
| `/proc/PID/fd/` | Open file descriptors (from per-task FD table) |
| `/proc/PID/cmdline` | argv as NUL-separated string |

Implementation: extend `kernel/src/kernel/fs/procfs.rs` with a `VfsDriver` that synthesises
these paths on demand by walking the scheduler's task table.

### 12.4 Symbolic links
- RamFS: `NodeKind::Symlink(target: String)`.
- VFS path resolution: follow up to 8 symlink hops (ELOOP after that).
- `symlink` syscall (=88), `readlink` syscall (=89) — currently a stub returning EINVAL
  (`kernel/src/kernel/sys/syscall_core.rs:680`).
- `ls -l` shows `->` target.

### 12.5 Hard links
- Multiple directory entries pointing to the same inode (refcount field on INode).
- `link` syscall (=86). `unlink` decrements refcount; data freed only when count reaches 0.

### 12.6 File locking
- `flock` syscall (=143): advisory locks (LOCK_SH / LOCK_EX / LOCK_UN).
- Prevents concurrent writes to the same file from two processes.
- Per-inode lock state tracked in VNode.

### 12.7 Filesystem hierarchy (FHS-lite) — PARTIAL
RamFS already creates `/bin`, `/etc`, `/tmp`, `/home` at boot
(`kernel/src/kernel/fs/ramfs.rs`). Remaining:
```
/sbin    /usr/bin    /usr/lib
/var     /proc (✅)  /dev (✅)   /sys    /mnt
```
- `/etc/passwd` — user database (single user `oxide` for now).
- `/etc/hostname`, `/etc/resolv.conf`, `/etc/hosts`.
- `/tmp` already exists as RAM-backed; confirm automatic cleanup semantics on reboot
  (RamFS is inherently volatile, so this is effectively free).

---

## Phase 13 — Networking V2 (remaining work)

**Goal:** A usable TCP/IP stack with DNS, HTTP, and multiplexed I/O.

### ✅ 13.1 DHCP client activation — DONE
### ✅ 13.2 DNS resolver — DONE
### ✅ 13.3 select / poll syscall — DONE

### 13.4 Non-blocking sockets
- `O_NONBLOCK` flag on socket fd.
- `recv()` on non-blocking socket returns -11 (EAGAIN) immediately if no data.
- Combined with select/poll (already done) for event-driven servers.

### 13.5 Socket options (setsockopt)
- `SO_REUSEADDR` — allow rapid server restart without "address already in use".
- `SO_RCVTIMEO` / `SO_SNDTIMEO` — per-socket timeouts.
- `TCP_NODELAY` — disable Nagle for interactive sessions.
- syscall `setsockopt=132`, `getsockopt=133`.

### 13.6 Simple HTTP server (`/bin/httpd`)
- Single-threaded, select-based.
- Serves files from `/var/www` or `/disk/www`.
- `GET /path HTTP/1.0` → read file → send headers + body.
- `QEMU` user-mode NAT: accessible at `http://10.0.2.15/` from the host.

### ✅ 13.7 ICMP ping (`/bin/ping`) — DONE

### 13.8 Network stack improvements
- **DHCP renewal** — handle lease expiry and renewal.
- **IPv6 basics** — smoltcp already supports it; enable `proto-ipv6` feature.
- **TCP keepalive** — detect dead connections.

---

## Phase 14 — Advanced IPC (remaining work)

### ✅ 14.1 POSIX signals (remaining) — DONE
sigprocmask, sigpending, sigsuspend, SIGCHLD all implemented (Phase 14.1 ✅).

### 14.2 POSIX timers
- `setitimer` (=38 on Linux) — repeating interval timer (ITIMER_REAL → SIGALRM).
- `alarm` syscall.
- `clock_gettime` already exists (228); confirm CLOCK_REALTIME reads RTC (CMOS ports
  0x70/0x71) rather than just ticks.

### 14.3 eventfd / signalfd (optional)
- `eventfd` (=290) — lightweight notification primitive.
- `signalfd` (=282) — receive signals as readable file descriptor.
- Allows signal-safe select/poll integration.

### 14.4 Unix domain sockets
- `AF_UNIX` socket type — IPC via filesystem path (no network).
- Enables `X11`-style GUI IPC, `dbus`-like message buses — a cleaner foundation for the
  Phase 16.1b window-server protocol than the current ad-hoc message queues.
- `socket(AF_UNIX, SOCK_STREAM, 0)` + `bind("/tmp/my.sock")`.

---

## Phase 15 — Dynamic ELF Linking

**Goal:** Shared libraries (.so). Programs can link against a shared libc.

### 15.1 Dynamic ELF loader (ld.so)
- Parse `PT_INTERP` segment — specifies the dynamic linker path.
- Kernel loads the ELF binary AND the dynamic linker into the process.
- Dynamic linker (`/lib/ld-oxide.so`) runs first, resolves PLT/GOT relocations.
- Depends on Phase 11.3 (file-backed mmap) to map `.so` segments efficiently.

### 15.2 oxide-libc (minimal shared C library)
- `malloc` / `free` / `realloc` backed by mmap + free list.
- `printf` / `scanf` / `fopen` / `fclose` — stdio.
- `pthread_create` (eventually) — backed by `clone` syscall (requires SMP, Phase 17, or at
  least cooperative green threads on a single core).
- `execve`, `fork`, `wait` wrappers.
- Enables compiling existing C programs for OxideOS with minimal changes — today this is
  done via static musl (Phase 10.6), which works but produces larger binaries and
  duplicates libc in every executable.

### 15.3 Shared library ABI
- `.so` files loaded from `/lib` and `/usr/lib`.
- Symbol lookup table (hash table of (name → address) per loaded library).
- Lazy binding: PLT stub resolves on first call (avoids startup overhead).
- `dlopen` / `dlsym` / `dlclose` for runtime loading.

---

## Phase 16 — Multi-Window GUI V2

**Goal:** Each userspace process manages its own windows. The compositor
becomes a proper display server rather than a single-window renderer.

### ⚠️ 16.1 Window server protocol (WayOxide) — PARTIALLY DONE
**Done:** per-process windows via `gui_proc` (syscalls 125–132: GuiCreate, GuiDestroy,
GuiFillRect, GuiDrawText, GuiPresent, GuiPollEvent, GuiGetSize, GuiBlitShm). Window
manager supports drag, resize, snap, z-order (`kernel/src/gui/window_manager.rs`).

### 16.1b Remaining protocol surface
Formalize the client → server / server → client message set on top of the existing IPC
message queues (or migrate to AF_UNIX sockets, Phase 14.4):

Client → server messages:
```
ResizeWindow(window_id, w, h)        ← GuiGetSize exists; resize request does not
SetTitle(window_id, title)
RequestFocus(window_id)
```

Server → client events:
```
FocusChange(window_id, gained: bool)
CloseRequest(window_id)              ← currently handled ad-hoc via take_closed_window()
Exposed(window_id, rect)
```

- Z-order management (click-to-front) and window decorations already exist in the WM;
  formalize as protocol events so GUI apps can react (e.g. redraw on focus change).

### 16.2 GUI applications — mostly done
Implemented: Notepad, Terminal, File Manager, System Monitor, Browser, Calendar,
Notifications, Quick Settings, Activities Overview.

Remaining:

| App | Description |
|-----|-------------|
| `/bin/image_viewer` | Display PNG/PPM images from disk (PNG decoder already a dependency via `png` crate). |
| `/bin/calculator` | Basic arithmetic with button grid. |
| `/bin/settings` | Screen resolution, font size, color scheme selector (Quick Settings panel exists; this would be the full app). |
| `/bin/about` | OxideOS version/build info dialog. |

### 16.3 Font improvements
Replace 8×8 bitmap font with:
```toml
noto-sans-mono-bitmap = { version = "0.3", default-features = false }
```
- Full Unicode coverage (Latin, Cyrillic, Greek, symbols).
- Multiple sizes (8px, 14px, 16px, 24px).
- Anti-aliased rendering via sub-pixel blending.

### 16.4 Clipboard
- Notepad has an internal Ctrl+C/V clipboard (`kernel/src/gui/notepad.rs`) but it is
  **local to Notepad**, not shared across windows.
- Promote to a global clipboard buffer (string + MIME type) in the compositor.
- `ClipboardSet(data)` / `ClipboardGet() → data` IPC messages (or syscalls, consistent
  with the existing Gui* syscall family).
- Ctrl+C/V works across windows (Notepad ↔ Terminal ↔ future apps).

### 16.5 Drag-and-drop
- Mouse button press on a window decorates the drag operation.
- Compositor tracks drag target; sends `DropEvent(data)` to target window.

---

## Phase 17 — SMP (Symmetric Multiprocessing)

**Goal:** All available CPU cores run kernel + user code.

### 17.1 APIC initialization (replace PIC)
- Detect Local APIC via CPUID.
- Parse MADT from ACPI to enumerate all LAPIC IDs (use the `acpi` crate, not yet a
  dependency).
- Remap APIC registers (LAPIC at physical 0xFEE00000 via HHDM).
- Initialize BSP LAPIC: SpuriousVector, timer divisor, TPR.
- Replace PIC with APIC: mask all 8259 IRQs, enable APIC.
- APIC timer replaces PIT for per-CPU preemption.

Files: `kernel/src/kernel/drivers/apic.rs` (new), `kernel/src/kernel/drivers/pic.rs`
(disable), `kernel/src/kernel/arch/interrupts.rs` (route IRQs via I/O APIC).

### 17.2 I/O APIC
- Parse MADT for I/O APIC address and GSI base.
- Program I/O APIC redirect table: keyboard (GSI 1), RTC (GSI 8), NIC (GSI N).
- Enables per-core IRQ delivery.

### 17.3 AP startup (INIT-SIPI)
```
BSP:
  1. Write AP trampoline code to 0x8000 (real-mode startup page).
  2. Send INIT IPI to AP LAPIC ID.
  3. Wait 10 ms.
  4. Send SIPI (vector = 0x08 → startup at 0x8000).
  5. AP enters real mode → 32-bit → 64-bit (copy BSP GDT/IDT/CR3).
  6. AP jumps to ap_main() in Rust.
```
- AP trampoline: 16→32→64 bit mode switch, load BSP GDT, enable paging with BSP CR3.
- `ap_main(cpu_id: u8)`: initialize LAPIC, set up per-CPU stack, enter scheduler loop.

### 17.4 Per-CPU data structures
- `CpuLocal` struct: current task index, idle task, LAPIC ID, scheduler lock.
- Access via `gs` segment register (write per-CPU base to IA32_GS_BASE MSR on each AP).
- `cpu_id()` → read from `gs:0`.
- `current_task()` → read from `gs:8`.

### 17.5 SMP-safe scheduler
- Per-CPU run queues (one round-robin list per core).
- Global task table protected by a spinlock; per-queue lock for fast operations.
- Load balancing: idle CPU steals a task from the busiest queue (work stealing).
- IPI for task wakeup: when task becomes runnable, send IPI to the owning CPU.
- `SpinLock<T>` wrapper using `lock xchg` (no_std, no std::sync needed) — note the kernel
  already uses `lazy_static` + spinlocks in several places; audit and reuse that pattern.

### 17.6 SMP-safe kernel data structures
Audit and lock (currently single-core, mostly global statics):
- `SOCK_TABLE` → `SpinLock<[Option<SocketEntry>; 16]>`
- `RAMFS` → `SpinLock<Option<RamFs>>`
- IPC queues → `SpinLock` per queue
- Physical allocator bitmap → `SpinLock` or atomic operations
- `NET` (smoltcp state) → `SpinLock` (smoltcp is not Send)
- `gui_proc` window table, compositor state → `SpinLock`

---

## Phase 18 — Security

**Goal:** A multi-user system with proper privilege separation.

### 18.1 Users and groups
Current state: `getuid`/`geteuid` are stubs returning `1000`; `setuid` is a stub returning
success without changing anything (`kernel/src/kernel/sys/syscall_core.rs:612-619`). File
permission bits (mode/uid/gid) are stored on RamFS inodes but never checked.

- `/etc/passwd`: `oxide:x:1000:1000:OxideOS User:/home/oxide:/bin/sh`
- `/etc/shadow`: hashed passwords (sha256-crypt, via `sha2` crate).
- `uid_t` / `gid_t` fields added to `Task` struct: effective UID/GID, saved set-UID.
- Syscalls `getuid=102`, `getgid=104`, `setuid=105`, `setgid=106` — wire to real per-task
  fields instead of constants.
- Permission check on `open()`: compare (uid, gid) against inode mode bits; return EACCES.
- `su` program — switch user after password verification.
- `login` program — presented at boot before shell (depends on Phase 21.1 init).

### 18.2 ASLR (Address Space Layout Randomization)
- Randomize base addresses of: user stack, heap, mmap region, shared libraries.
- Use RDRAND instruction (or RDTSC, already used in `kernel/src/kernel/drivers/timer.rs`,
  as an LCG seed if RDRAND unavailable).
- Stack guard page: map a non-present page below the stack to catch overflows.
- Reduces exploitability of memory corruption bugs.

### 18.3 NX / XD enforcement — MOSTLY DONE
- NX bit (`PageTableFlags::NO_EXECUTE`, bit 63) already implemented in
  `kernel/src/kernel/mem/paging_allocator.rs`, and SMEP is enabled in `boot_init.rs`.
- Audit: confirm user stack and heap pages are mapped non-executable, and only ELF
  PT_LOAD segments with `PF_X` get the executable bit cleared from NO_EXECUTE.

### 18.4 Capability system (pledge-style)
Inspired by OpenBSD `pledge()`:
- Each process has a `capabilities: u64` bitmask (INET, STDIO, RPATH, WPATH, EXEC, …).
- `pledge(caps)` syscall (=200) — permanently restrict capabilities.
- Kernel checks caps on sensitive syscalls (connect checks INET, open checks RPATH/WPATH).
- Programs self-restrict after startup to limit damage from exploits.

### 18.5 Syscall parameter hardening
- Every syscall that takes a user pointer must call `validate_user_range()`.
- Check all string lengths to prevent kernel reads past user buffers.
- Return `EFAULT` (-14) instead of panicking on bad pointers.
- Audit all existing syscall implementations (1700+ lines in `syscall_core.rs` +
  `syscall.rs`).

### 18.6 Stack canary
- `_start` in oxide-rt writes a random canary value below the return address.
- Function epilogues check it. (Requires compiler support — `-Z stack-protector=all`.)

---

## Phase 19 — Hardware V2

**Goal:** Support real hardware beyond QEMU defaults.

### 19.1 APIC timer (replace PIT)
- PIT generates only a single timer at 100 Hz for the whole system.
- APIC timer is per-CPU and much more flexible.
- Calibrate APIC timer against PIT: run PIT for 10 ms, count APIC ticks.
- Program APIC timer in periodic mode at 100 Hz per CPU.
- Enables true per-core preemption in SMP (Phase 17).

### 19.2 AHCI / SATA (replace ATA PIO)
**Problem:** ATA PIO blocks the CPU during disk I/O. Throughput ≈ 3 MB/s. Three NIC
drivers (RTL8139/e1000/PCnet) are already auto-detected via PCI
(`kernel/src/kernel/drivers/net/pci.rs`) — the same PCI enumeration infrastructure can be
reused for AHCI.

Implementation:
- Enumerate PCIe for class=0x01, subclass=0x06 (Mass Storage, SATA).
- Map AHCI HBA memory-mapped registers.
- Set up command list and FIS structures for each port.
- DMA transfers: kernel allocates bounce buffers, HBA DMAs into them.
- Interrupt-driven: AHCI fires an MSI/legacy IRQ on completion.
- Throughput: 100–600 MB/s (SATA II/III).

Files: `kernel/src/kernel/drivers/ahci.rs` (new), `kernel/src/kernel/fs/vfs.rs` (route to
AHCI, behind the same block-device abstraction FAT16/ext2 already use).

### 19.3 USB keyboard & mouse (XHCI)
**Problem:** PS/2 is obsolete; modern machines (and most VMs) use USB HID.

Steps:
- Enumerate PCIe for XHCI controller (class=0x0C, subclass=0x03, progif=0x30) — reuse
  `pci.rs`.
- Initialize XHCI: reset HC, set up command/event rings.
- Enumerate USB devices; find HID boot-protocol keyboard and mouse.
- Process USB HID reports in interrupt handler → feed to `stdin` / mouse delta (same
  internal interface PS/2 currently feeds, `kernel/src/kernel/drivers/keyboard.rs` +
  `kernel/src/gui/mouse.rs`).
- Fallback: if PS/2 absent, use USB. If both present, use both.

### 19.4 Intel HDA Audio
- Detect HDA controller (PCIe class=0x04, subclass=0x03).
- Enumerate codec via HDA command verbs; find DAC widget → Line Out.
- Set up BDL (Buffer Descriptor List) for DMA audio output.
- `/dev/audio` device — write PCM samples (16-bit stereo 44100 Hz).
- `/bin/beep` — play simple tones.
- `/bin/play` — stream raw PCM from file.

### 19.5 NVMe support
- PCIe class=0x01, subclass=0x08.
- Admin queue + I/O queue setup.
- 64-bit DMA reads/writes; interrupt on completion.
- Much faster than AHCI for modern SSDs.

### 19.6 VirtIO block device (for QEMU/KVM)
- Use `virtio-drivers` crate for VirtIO-blk (currently an unused dependency option).
- Faster than ATA PIO in QEMU; supports discard, flush.
- Auto-detected alongside RTL8139/e1000/PCnet at boot.

---

## Phase 20 — POSIX Compatibility & libc

**Goal:** Run real-world C programs compiled for OxideOS without modification.

### 20.1 Extended syscall surface
Current syscall table already covers most high-value POSIX calls at Linux numbers
(read/write/open/close/stat/fstat/lstat/poll/lseek/mmap/mprotect/munmap/brk/
sigaction/sigprocmask/sigreturn/ioctl/readv/writev/access/pipe/sched_yield/mremap/
madvise/dup/dup2/nanosleep/getpid/fork/vfork/execve/exit/waitpid/kill/uname/fcntl/
fsync/truncate/ftruncate/getdents64/getcwd/chdir/rename/mkdir/rmdir/unlink/readlink/
chmod/fchmod/chown/fchown/umask/gettimeofday/getrlimit/getrusage/sysinfo/getuid/getgid/
getpgrp/setsid/getppid/gettid/arch_prctl/set_tid_address/clock_gettime/exit_group/pipe2/
pread64/pwrite64/socket/bind/connect/listen/accept/sendto/recvfrom).

Remaining gaps for full POSIX:
| Syscall | Purpose | Status |
|---------|---------|--------|
| `setpgid`/`getpgid` | Process group | ✅ (Phase 10.4) |
| `mprotect` | Change page permissions | check enforcement vs stub |
| `msync` | Flush mmap to disk | needed once 11.3 (file-backed mmap) lands |
| `mlock`/`munlock` | Pin pages in RAM | low priority |
| `prctl` | Process control | low priority |
| `sendfile` | Zero-copy file-to-socket | useful for Phase 13.6 httpd |
| `flock` | Advisory file locks | Phase 12.6 |
| `symlink`/`link` | Links | Phase 12.4/12.5 |
| `setitimer`/`alarm` | POSIX timers | Phase 14.2 |

### 20.2 C standard library (oxide-libc)
A minimal `libc.so` for OxideOS, complementing the static-musl approach from Phase 10.6:
- Memory: `malloc`, `calloc`, `realloc`, `free`, `mmap`, `munmap`
- I/O: `open`, `close`, `read`, `write`, `lseek`, `stat`, `fstat`
- Stdio: `fopen`, `fclose`, `fread`, `fwrite`, `fprintf`, `printf`, `scanf`, `fgets`
- String: `strlen`, `strcpy`, `strcmp`, `strcat`, `strtol`, `sprintf`, `snprintf`
- Process: `fork`, `exec`, `waitpid`, `exit`, `getenv`, `setenv`
- Math: backed by `libm` crate
- Enables compiling musl/busybox-style programs against a smaller, OxideOS-native libc and
  is a prerequisite for Phase 15 (dynamic linking).

### 20.3 POSIX shell — DONE via Bash
GNU Bash 5.2 is embedded (Phase 10.6 ✅). `/bin/sh` (the hand-written shell) remains as a
lightweight option.

### 20.4 Python 3 — DONE
CPython 3.12 runs from `/disk` (Phase 10.6 ✅, see README "Running CPython 3 from disk").
Future: embed Python in the kernel image once `miniz_oxide`-based compression or ext2
write makes shipping a ~30 MB interpreter in the ISO practical.

---

## Phase 21 — Package Manager & Init System

**Goal:** Self-hosting development environment.

### 21.1 Init system (oxide-init)
Replace kernel-launched GUI/terminal with a proper init:
```
/sbin/oxide-init:
  1. Mount /proc, /dev, /tmp (RamFS already provides these at boot)
  2. Run /etc/rc.d/ scripts (network up, daemons start)
  3. Spawn /bin/login on tty0, or launch the GUI desktop directly
  4. Monitor children; respawn on crash
  5. Handle signals: SIGTERM → graceful shutdown sequence
```
- PID 1 always running; SIGCHLD delivery already works (Phase 10.4/14.1), so init can
  `waitpid` zombies.
- Service files in `/etc/rc.d/` (simple shell scripts: `start`, `stop`, `status`).

### 21.2 Package manager (opkg)
- Simple tarball-based packages (.opkg = gzip'd POSIX tar, via `miniz_oxide`).
- `/etc/pkg/` database: installed name → file list + metadata.
- `opkg install <name>` — fetch from HTTP server (Phase 13.6), extract to `/`, update db.
- `opkg remove <name>` — unlink files listed in db.
- `opkg list` — show installed packages.

### 21.3 Self-hosted compiler (oxide-cc)
- Cross-compile `tcc` (Tiny C Compiler) or `cproc` against musl/oxide-libc — similar
  process to Lua/BusyBox in Phase 10.6.
- Run on OxideOS itself: `oxide-cc hello.c -o hello` → produces ELF.
- Full self-hosting: OxideOS can compile OxideOS.

---

## Implementation Priority Order (Updated June 2026)

```
✅ DONE  Phases 1–9, 10.1–10.6 (argv, env vars, pipes, job control, coreutils,
                                 Linux ABI, musl, Lua, BusyBox, Bash, Python3)
✅ DONE  Phase 11.1–11.2 (COW fork, real munmap)
✅ DONE  Phase 12.3 (procfs, system-wide)
✅ DONE  Phase 13.1–13.3, 13.7 (DHCP, DNS, select/poll, ping)
✅ DONE  Phase 14.1 (sigprocmask, sigsuspend, SIGCHLD)
✅ DONE  Phase 22 (installable OS, /bin/install, pre-built image)
⚠️ PARTIAL  Phase 16.1 (multi-window GUI via gui_proc; protocol v2 pending)

── NEXT: Correctness & Persistence ─────────────────────────────────

🔥 Phase 12.1   ext2 write completion (block/inode alloc, dir entries, create) ← unblocks Phase 22 ext2-root variant
🔥 Phase 11.5   Linked-list kernel heap allocator ← long-running stability
🔥 Phase 12.3b  Per-process procfs (/proc/PID/status, maps, fd)
📌 Phase 11.6   Physical frame free list (O(1) alloc/free)
📌 Phase 12.2   Block cache (LRU, page cache)
📌 Phase 12.4/12.5  Symbolic & hard links

── MEDIUM PRIORITY: Security & GUI Maturity ────────────────────────

📌 Phase 18.1   Users & groups — real uid/gid enforcement, /etc/passwd, login
📌 Phase 18.2   ASLR
📌 Phase 16.1b  Window server protocol v2 (clipboard, drag-drop, decorations)
📌 Phase 16.3   Unicode font (noto-sans-mono-bitmap)
📌 Phase 13.4/13.5  Non-blocking sockets, setsockopt
📌 Phase 13.6   /bin/httpd

── ADVANCED ────────────────────────────────────────────────────────

⚙  Phase 17    SMP (LAPIC, INIT-SIPI, per-CPU sched) ← Big milestone
⚙  Phase 11.3/11.4  File-backed mmap, demand paging
⚙  Phase 15    Dynamic ELF linking (depends on 11.3)
⚙  Phase 19    Hardware V2 (AHCI, USB, audio, NVMe, VirtIO-blk)
⚙  Phase 20    POSIX libc compatibility (oxide-libc)
⚙  Phase 21    Package manager + init + self-host
```

---

## Phase 22 — Installable OS (ext2-root variant)

**Goal:** OxideOS can be written to a USB stick, booted on real x86-64 hardware, and
installed to an internal disk with a persistent **ext2** filesystem — `/bin`, `/etc`,
`/home` survive reboot just like Ubuntu.

> The MBR/FAT32+FAT16 variant of this phase is **done** (see Completed Phases). This
> section describes the upgrade to a writable ext2 root, which depends on Phase 12.1.

### Prerequisites (must complete first)
- Phase 12.1 (ext2 write) — persistent root filesystem
- Phase 12.7 (FHS-lite) — `/etc`, `/home`, `/bin`, `/usr` directory structure (RamFS
  already creates `/bin /etc /tmp /home`; ext2 needs the same layout)
- Phase 19.2 (AHCI/SATA) — real disk I/O beyond QEMU ATA PIO (ATA PIO works for QEMU/VM
  installs today; AHCI needed for real hardware throughput)
- Phase 21.1 (init system) — PID 1 mounts filesystems, starts services

### 22.1 Bootable USB image — DONE (FAT variant)
`make install-image` already produces a 192 MB MBR image with FAT32 (EFI boot) + FAT16
(data) partitions. The ext2-root upgrade changes partition 2 to ext2:
```
Disk layout (example, 2 GB):
  Partition 1: FAT32  64 MB  → /boot (Limine, kernel ELF, initrd)
  Partition 2: ext2  ~2 GB   → /     (root filesystem, writable)
```

### 22.2 Live mode (run without installing)
On first boot from USB, default to **live mode**:
- Root mounts the ext2 read-only; overlay RamFS on top for writes (RamFS already exists
  and is the current root — this becomes the overlay rather than the root).
- User can explore OxideOS, run programs, connect to network.
- A desktop shortcut / shell command `install-oxide` launches the installer (the existing
  `/bin/install` is the starting point).

Implementation:
- Kernel detects `live=1` boot parameter (Limine config entry).
- VFS overlay: writes go to RAM, reads fall through to ext2.

### 22.3 Installer (`/sbin/oxide-install`) — extend existing `/bin/install`
`kernel/src/kernel/installer.rs` (582 lines) already implements the FAT32+FAT16 installer.
Extend with:
```
Step 1: Detect disks (PCI/ATA enumeration — reuse existing ATA driver)
Step 2: Partition target disk (MBR write — already implemented)
Step 3: Format partitions — add mkfs.ext2 for partition 2 (needs Phase 12.1)
Step 4: Copy root filesystem — walk live RamFS/ext2, write each file to target ext2
Step 5: Install bootloader — Limine MBR + limine.conf (already implemented)
Step 6: Configure — /etc/hostname, /etc/passwd (Phase 18.1), /etc/shadow
Step 7: Done — reboot from internal disk
```

### 22.4 Persistent filesystem layout on disk
After installation, the disk has a real FHS-compliant ext2 root:
```
/bin/         → shell, coreutils, busybox applets, bash, lua, python3
/sbin/        → init, oxide-install, fsck
/etc/         → hostname, passwd, shadow, resolv.conf, hosts, fstab
/etc/rc.d/    → startup scripts (network, sshd, gui)
/home/        → per-user home directories (writable, persists)
/lib/         → shared libraries (Phase 15)
/usr/bin/     → additional programs
/usr/lib/     → more shared libs
/var/log/     → system logs
/var/www/     → httpd document root
/tmp/         → tmpfs (cleared each boot)
/dev/         → devfs (populated at boot)
/proc/        → procfs (Phase 12.3, 12.3b)
/mnt/         → mount points for removable media
```

`/etc/fstab`:
```
UUID=<root-uuid>  /      ext2  defaults        0 1
UUID=<boot-uuid>  /boot  fat32 ro,defaults     0 2
tmpfs             /tmp   tmpfs size=64M        0 0
```

Kernel reads `fstab` at boot via init to mount all partitions.

### 22.5 First-boot setup wizard
On the very first boot after installation (detected by `/etc/firstboot` marker):
- Text-UI wizard: set timezone, create user account (Phase 18.1), configure network.
- Removes `/etc/firstboot` when complete.
- Equivalent of Ubuntu's OOBE (out-of-box experience).

### 22.6 Upgrade path
```
opkg upgrade   →  download new kernel + packages, install to /boot and /usr,
                  keep /etc and /home untouched, reboot into new version
```
- `/boot/limine.conf` keeps a fallback entry pointing to the previous kernel.
- On bad boot, user can select previous version from Limine menu.

---

## What Makes OxideOS a "Real OS"

The milestones that cross the line from hobby OS to real OS:

| Milestone | Phase | Status |
|-----------|-------|--------|
| Programs receive argv/envp | 10.1 | ✅ |
| musl libc + first C program | 10.6 | ✅ |
| Lua / BusyBox / Bash run | 10.6 | ✅ |
| Python 3 runs | 20.4 / 10.6 | ✅ |
| Shell pipes + job control | 10.3 / 10.4 | ✅ |
| DNS + wget by hostname | 13.2 | ✅ |
| Copy-on-write fork | 11.1 | ✅ |
| Real munmap | 11.2 | ✅ |
| select/poll | 13.3 | ✅ |
| procfs (system-wide) | 12.3 | ✅ |
| Bootable USB image + installer | 22.1/22.3 | ✅ |
| Multiple per-process GUI windows | 16.1 | ⚠️ partial |
| Kernel heap that frees memory | 11.5 | ⬡ |
| ext2 writable | 12.1 | ⬡ |
| procfs per-process | 12.3b | ⬡ |
| Symbolic/hard links | 12.4/12.5 | ⬡ |
| Window clipboard / drag-drop | 16.4/16.5 | ⬡ |
| Users + permission enforcement | 18.1 | ⬡ |
| ASLR | 18.2 | ⬡ |
| SMP (N cores used) | 17 | ⬡ |
| Dynamic ELF linking (.so) | 15 | ⬡ |
| AHCI/USB/audio (real hardware) | 19 | ⬡ |
| Persistent /home + /etc on ext2 disk | 22.4 | ⬡ |
| Package manager / self-hosting | 21.2/21.3 | ⬡ |

---

## File Layout Target

```
kernel/src/
├── kernel/
│   ├── drivers/
│   │   ├── apic.rs          ← Phase 17.1 (LAPIC)
│   │   ├── ahci.rs          ← Phase 19.2 (SATA)
│   │   ├── usb/             ← Phase 19.3
│   │   │   ├── xhci.rs
│   │   │   └── hid.rs
│   │   ├── audio/           ← Phase 19.4
│   │   │   └── hda.rs
│   │   └── net/
│   │       ├── rtl8139.rs   ✅
│   │       ├── e1000.rs     ✅
│   │       ├── pcnet.rs     ✅
│   │       ├── stack.rs     ✅ (smoltcp)
│   │       ├── socket.rs    ✅
│   │       ├── dns.rs       ✅
│   │       └── virtio.rs    ← Phase 19.6 (currently empty stub)
│   ├── fs/
│   │   ├── ramfs.rs     ✅
│   │   ├── fat.rs       ✅
│   │   ├── ext2.rs      ✅ (read) + ⚠️ (partial write) ← complete Phase 12.1
│   │   ├── procfs.rs    ✅ (system-wide) ← extend Phase 12.3b
│   │   ├── diskfs.rs    ✅
│   │   └── block_cache.rs ← Phase 12.2
│   ├── mem/
│   │   ├── allocator.rs      ⚠️ bump, never frees ← Phase 11.5
│   │   └── paging_allocator.rs ✅ (COW, munmap, NX)
│   ├── security/        ← Phase 18
│   │   ├── users.rs
│   │   ├── capabilities.rs
│   │   └── aslr.rs
│   └── smp/             ← Phase 17
│       ├── apic.rs
│       ├── trampoline.rs
│       └── percpu.rs
userspace/
├── oxide-rt/            ✅ (syscall wrappers, _start, bump alloc)
├── oxide-widgets/       ✅ (GUI widget toolkit)
├── oxide-libc/          ← Phase 20.2 (C library)
├── sh/, bash            ✅ (shells)
├── terminal/            ✅ (GUI terminal emulator)
├── filemanager/, sysmon/, browser/  ✅ (GUI apps)
├── coreutils/           ✅
├── wget/, nc/, ping/, edit/, install/  ✅
├── hello_musl/, musl_test/  ✅ (musl reference programs)
├── lua, busybox, bash (embedded ELFs) ✅
├── python3 (FAT-disk ELF)  ✅
├── httpd/               ← Phase 13.6
├── image_viewer/        ← Phase 16.2
├── login/               ← Phase 21.1
└── init/                ← Phase 21.1
```
