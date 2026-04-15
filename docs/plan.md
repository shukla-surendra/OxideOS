# OxideOS — Roadmap to a Fully Functional OS

This document audits every subsystem, tracks completed work, and lays out a
phased plan to reach a production-grade general-purpose OS. Each phase is
designed so the OS remains bootable and usable after every milestone.

---

## Current State (April 2026)

### What works today

| Subsystem | Status |
|-----------|--------|
| 64-bit boot (Limine, UEFI/BIOS) | ✅ |
| GDT / TSS / IDT | ✅ |
| PIC, PIT at 100 Hz | ✅ |
| Physical frame allocator (256 MB bitmap) | ✅ |
| Per-process page tables (CR3 per task) | ✅ |
| User mode (Ring 3, iretq) | ✅ |
| Preemptive scheduler — 8-task round-robin | ✅ |
| ELF64 loader (ET_EXEC, static) | ✅ |
| int 0x80 + SYSCALL/SYSRET fast path | ✅ |
| RamFS — in-memory tree, 32 open FDs | ✅ |
| FAT16 read + write (subdirs, ATA PIO) | ✅ |
| ext2 read-only (superblock, BGDT, inodes, direct blocks) | ✅ |
| MBR partition table (4 entries, type detection) | ✅ |
| VFS layer — /dev/null, /dev/tty, mount table | ✅ |
| Anonymous pipes (8 pairs, 4 KB) | ✅ |
| fork / exec / waitpid / exit cleanup | ✅ |
| Per-task FD table, dup2 | ✅ |
| brk/sbrk heap, mmap anonymous | ✅ |
| Full POSIX signals — sigaction, sigreturn, trampoline, pending bitmask | ✅ |
| TTY — termios, ioctl TCGETS/TCSETS/TIOCGWINSZ, canonical/raw mode | ✅ |
| PS/2 keyboard (pc-keyboard crate) + mouse | ✅ |
| Framebuffer + double-buffered compositor | ✅ |
| GUI — window manager, start menu, taskbar, drag/focus | ✅ |
| IPC message queues (compositor protocol) | ✅ |
| Shared memory — shmget/shmat/shmdt, physical frame sharing | ✅ |
| RTL8139 NIC + smoltcp (TCP, UDP, ICMP, DHCP, ARP) | ✅ |
| Socket syscalls — socket/bind/connect/listen/accept/send/recv | ✅ |
| Socket syscalls — sendto/recvfrom (UDP) | ✅ |
| File permissions — mode/uid/gid on RamFS inodes, chmod/chown | ✅ |
| unlink/rename/truncate syscalls | ✅ |
| SMEP (CR4 bit 20) | ✅ |
| ACPI shutdown — RSDP→FADT→PM1a_CNT_BLK | ✅ |
| BSoD crash dump — framebuffer + serial register dump | ✅ |
| Shell `/bin/sh` — fork+exec, >, >> redirect | ✅ |
| Coreutils: ls, cat, cp, mv, rm, mkdir, pwd, ps, wget, edit, nc | ✅ |
| Text editor `/bin/edit` — nano-like, VT100 | ✅ |
| Netcat `/bin/nc` — TCP listen/connect, UDP send/listen | ✅ |
| **GUI userspace API** — per-process windows (GuiCreate/Destroy/FillRect/DrawText/Present/PollEvent/GetSize/BlitShm, syscalls 125–132) | ✅ |
| **GUI file manager** `/bin/filemanager` — directory navigation, file listing, keyboard+mouse input | ✅ |

### Remaining gaps (priority order)

| Gap | Blocks |
|-----|--------|
| **argv passing** — programs use interactive prompts | Every CLI tool |
| **Environment variables** (PATH, HOME, USER) | Shell usability |
| **Shell pipes** (cmd1 \| cmd2) | Shell usability |
| **Job control** (bg, fg, &) | Shell usability |
| **select/poll** | Non-blocking I/O |
| **munmap** (currently no-op) | Memory correctness |
| **COW fork** | Memory efficiency |
| **ext2 write** | Persistence |
| **procfs** (/proc/pid, /proc/meminfo) | Observability |
| **DNS resolver** | Networking usability |
| **Dynamic ELF linking** | Binary compatibility |
| ~~**Multi-window compositor** (window IDs per process)~~ | ✅ Done via gui_proc |
| **SMP** (LAPIC + INIT-SIPI) | Performance |
| **USB keyboard/mouse** | Real hardware |
| **AHCI/SATA** (replace PIO) | Disk performance |
| **Users & groups** (uid enforcement, login) | Security |
| **ASLR** | Security |
| **Audio** (Intel HDA) | Multimedia |

---

## Available `no_std` Crates

| Crate | Purpose |
|-------|---------|
| `pc-keyboard` ✅ | PS/2 scancode decoding |
| `smoltcp` ✅ | TCP/IP stack |
| `acpi` | Parse MADT for SMP AP discovery |
| `virtio-drivers` | VirtIO-net/blk for QEMU |
| `x86_64` | Safe CR3/VirtAddr/PageTable wrappers |
| `linked_list_allocator` | Better kernel heap (replace bump) |
| `heapless` | Fixed-capacity Vec/String in kernel data structures |
| `noto-sans-mono-bitmap` | High-quality Unicode font for GUI |
| `libm` | Float math (sin/cos/sqrt) for GUI effects |
| `miniz_oxide` | Deflate — compress initrd / ELF binaries |
| `postcard` | Compact IPC message serialization |
| `sha2` | File integrity, future auth |
| `chacha20poly1305` | Authenticated encryption |

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

---

## Phase 10 — argv, Environment Variables & Shell Pipes  ← NEXT

**Goal:** Programs receive command-line arguments. The shell gains pipes and job control.
This single phase makes OxideOS feel like a real Unix system.

### 10.1 argv / argc passing  ← HIGH PRIORITY
**Problem:** All programs today prompt interactively because argv is not wired up.

Implementation:
- Kernel `exec_program()` writes an **argument block** just below the user stack:
  ```
  [argc: u64] [argv[0] ptr] [argv[1] ptr] ... [NULL] [string data]
  ```
  Total size capped at 4 KB. Stack pointer on entry points to `argc`.
- `oxide-rt/_start` reads `argc` from `rsp`, builds `argv: &[&[u8]]`, calls `oxide_main(argv)`.
- All existing programs gain `fn oxide_main(args: &[&[u8]])` signature.
- Shell splits command tokens and passes them as argv on exec.

Files: `kernel/src/kernel/scheduler.rs` (spawn), `kernel/src/kernel/elf_loader.rs`,
`userspace/oxide-rt/src/lib.rs` (_start), all `userspace/*/src/main.rs`

### 10.2 Environment variables
- Kernel stores a per-process env block (PATH, HOME, SHELL, USER, TERM) alongside argv.
- `syscall Getenv=79` / `Setenv=80` — read/write env vars.
- `oxide-rt::getenv(key) -> Option<&str>`, `setenv(key, val)`.
- Shell inherits env from parent; `export VAR=val` sets it.
- `/bin/env` prints all variables.

### 10.3 Shell pipes (cmd1 | cmd2)
**Problem:** `sh` has `>` redirect but no inter-process pipes.

Implementation:
- Shell tokenises `|` — creates an anonymous pipe per `|`.
- Left command: stdout → pipe write fd. Right command: stdin ← pipe read fd.
- Shell forks both sides, `dup2`s fds, then waits for both.
- Example: `ls | cat`, `ps | grep sh`

### 10.4 Job control (background & foreground)
- `&` at end of command → don't waitpid, print `[1] <pid>`.
- Built-in `jobs` — list background PIDs + status.
- Built-in `fg <n>` — bring job to foreground (SIGCONT).
- Built-in `bg <n>` — resume stopped job in background.
- Ctrl+Z → SIGTSTP → task state: Stopped. Shell notices via waitpid WIFSTOPPED.
- `TaskState::Stopped` added to scheduler.

### 10.5 More coreutils
With argv wired:
- `/bin/echo` — print arguments
- `/bin/grep` — regex line filter (use `regex-lite` no_std crate)
- `/bin/head` / `/bin/tail` — first/last N lines
- `/bin/wc` — word/line/byte count
- `/bin/sort` — sort lines
- `/bin/sleep` — sleep N seconds
- `/bin/kill` — send signal to PID
- `/bin/touch` — create empty file / update mtime
- `/bin/find` — walk directory tree with name filter
- `/bin/env` — print/set environment
- `/bin/true` / `/bin/false` — exit 0 / exit 1

**Deliverable:** `ls /bin | grep sh | wc -l` works end-to-end.

---

## Phase 11 — Memory Management V2

**Goal:** Correct, efficient memory handling for multi-process workloads.

### 11.1 Copy-on-Write (COW) fork
**Problem:** `fork()` currently copies all pages, which is slow and wastes RAM.

Implementation:
- Mark all user pages read-only in both parent and child PTEs after fork.
- Page fault handler: if fault is a write to a COW page, allocate new frame, copy data,
  re-map as writable.
- Kernel tracks COW refcount per physical frame (add `cow_refs: u8` to frame metadata).
- Reduces fork from O(pages) memcpy to O(1) PTE walk.

Files: `paging_allocator.rs` (frame metadata), `interrupts.rs` (page fault handler),
`scheduler.rs` (fork_task).

### 11.2 munmap (real implementation)
- Currently a no-op. Implement by unmapping PTEs and returning frames to the allocator.
- Track mapped regions in `Task` as a `Vec<(virt_base, len)>`.
- On `munmap(addr, len)`: unmap PTEs, decrement physical frame refcounts, free zero-ref frames.

### 11.3 mmap file-backed regions
- `mmap(addr, len, prot, MAP_PRIVATE, fd, offset)` — map a file into address space.
- Read-only initially; COW on write (MAP_PRIVATE) or write-through (MAP_SHARED).
- Required for dynamic ELF loading (Phase 15).

### 11.4 Demand paging & page-level heap growth
- Currently `brk` maps all requested pages eagerly.
- Change to: map only the first page; install a fault handler that maps subsequent pages lazily.
- Reduces memory pressure for programs that allocate large buffers but use them sparsely.

### 11.5 Linked-list kernel heap allocator
Replace the kernel bump allocator (`allocator.rs`) with `linked_list_allocator` crate:
```toml
linked_list_allocator = { version = "0.10", default-features = false }
```
- Enables kernel to free heap memory (currently it never frees).
- Critical for long-running systems where kernel allocates/frees many data structures.

### 11.6 Physical frame free list
- `alloc_frame()` scans bitmap linearly — O(N) worst case.
- Replace with a stack-based free list for O(1) alloc/free.
- Add `free_frame(phys)` so munmap and process exit can actually return memory.

---

## Phase 12 — Filesystem V2

**Goal:** Writable ext2, persistent storage, procfs, symbolic links.

### 12.1 ext2 write support
Current: read-only (superblock, BGDT, inodes, direct blocks).

Add:
- `ext2_write_block(block_no, buf)` — ATA sector write through block layer.
- Inode `atime`/`mtime`/`ctime` update on read/write.
- Allocate new data blocks (scan block bitmap, update superblock free count).
- Allocate new inodes (scan inode bitmap).
- Directory entry insertion and deletion.
- File creation: `ext2_create(path)`.
- File truncation and append.
- Write-back: dirty inode/bitmap/superblock flushed on close or sync.
- `sync` syscall (=162): flush all dirty buffers to disk.

Files: `kernel/src/kernel/ext2.rs` (new write functions),
`kernel/src/kernel/vfs.rs` (route writes to ext2 backend).

### 12.2 Block cache (page cache)
- Without a cache, every read hits ATA PIO — ~1 ms per sector.
- Add a 64-entry LRU block cache (`[Option<CachedBlock>; 64]`).
- Cache keyed by (device_id, block_no). Hit: return cached data. Miss: read + insert.
- On write: mark block dirty; flush on eviction or sync.

### 12.3 procfs — `/proc`
Mount a virtual filesystem at `/proc`:

| Path | Content |
|------|---------|
| `/proc/pid/` | Directory per process |
| `/proc/pid/status` | PID, PPID, state, memory usage |
| `/proc/pid/maps` | Virtual memory regions |
| `/proc/pid/fd/` | Open file descriptors |
| `/proc/meminfo` | Total/free RAM, heap stats |
| `/proc/cpuinfo` | Vendor, model, frequency |
| `/proc/uptime` | Ticks since boot |
| `/proc/version` | Kernel version string |
| `/proc/mounts` | Mounted filesystems |

Implementation: `VfsDriver` trait implementation for procfs. Reads are synthesised on demand.

### 12.4 Symbolic links
- RamFS: `NodeKind::Symlink(target: String)`.
- VFS path resolution: follow up to 8 symlink hops (ELOOP after that).
- `symlink` syscall (=88), `readlink` syscall (=89).
- `ls -l` shows `->` target.

### 12.5 Hard links
- Multiple directory entries pointing to the same inode (refcount field on INode).
- `link` syscall (=86). `unlink` decrements refcount; data freed only when count reaches 0.

### 12.6 File locking
- `flock` syscall (=143): advisory locks (LOCK_SH / LOCK_EX / LOCK_UN).
- Prevents concurrent writes to the same file from two processes.
- Per-inode lock state tracked in VNode.

### 12.7 Filesystem hierarchy (FHS-lite)
Populate `/` with standard directories at boot:
```
/bin    /sbin    /usr/bin    /usr/lib
/etc    /var     /tmp        /home/user
/proc   /dev     /sys        /mnt
```
- `/etc/passwd` — user database (single user `oxide` for now).
- `/etc/hostname`, `/etc/resolv.conf`, `/etc/hosts`.
- `/tmp` — RAM-backed tmpfs with automatic cleanup on reboot.

---

## Phase 13 — Networking V2

**Goal:** A usable TCP/IP stack with DNS, HTTP, and multiplexed I/O.

### 13.1 DHCP client activation
- smoltcp has `socket::dhcpv4`. Wire it into `net::init()` to configure IP automatically.
- Print `IP: 10.0.2.15 GW: 10.0.2.2` on serial after lease obtained.
- Store resolved IP/GW in `NET_CONFIG` global for use by programs.

### 13.2 DNS resolver
- Parse `/etc/resolv.conf` for nameserver IP.
- `dns_resolve(hostname) -> Option<[u8;4]>`: send UDP DNS query to port 53, parse A record.
- `oxide-rt::resolve(host) -> Option<[u8;4]>` — userspace wrapper.
- Update `/bin/wget` to accept hostnames in addition to IP addresses.

### 13.3 select / poll syscall
**Problem:** recv() blocks the entire task. Multiplexed I/O is impossible.

Implementation:
- `select(nfds, readfds, writefds, exceptfds, timeout)` (syscall=23 conflict — use 130).
- Kernel checks readiness of each fd without blocking; if none ready, task sleeps.
- `poll(fds, nfds, timeout)` (syscall=7 — use 131) — simpler API.
- Enables a single-threaded server to handle multiple connections.

### 13.4 Non-blocking sockets
- `O_NONBLOCK` flag on socket fd.
- `recv()` on non-blocking socket returns -11 (EAGAIN) immediately if no data.
- Combined with `select/poll` for event-driven servers.

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

### 13.7 ICMP ping (`/bin/ping`)
- Use smoltcp `socket::icmp`.
- Send echo requests, receive replies, print RTT.
- Requires raw socket access to the ICMP layer.

### 13.8 Network stack improvements
- **DHCP renewal** — handle lease expiry and renewal.
- **IPv6 basics** — smoltcp already supports it; enable `proto-ipv6` feature.
- **TCP keepalive** — detect dead connections.

---

## Phase 14 — select/poll & Advanced IPC

**Goal:** Standard POSIX I/O multiplexing and inter-process communication.

### 14.1 POSIX signals (remaining)
- `sigprocmask` (=135) — block/unblock signals during critical sections.
- `sigpending` (=136) — query pending signals.
- `sigsuspend` (=137) — atomically unblock and sleep until signal.
- `SIGCHLD` — parent notified when child exits (enables async waitpid).
- `SIGALRM` — alarm(N) sends SIGALRM after N seconds.
- `alarm` syscall (=138).

### 14.2 POSIX timers
- `setitimer` (=138) — repeating interval timer (ITIMER_REAL → SIGALRM).
- `clock_gettime` (=139) — CLOCK_MONOTONIC (ticks), CLOCK_REALTIME (RTC time).
- Read RTC from CMOS I/O ports 0x70/0x71 for wall-clock time.

### 14.3 eventfd / signalfd (optional)
- `eventfd` (=290) — lightweight notification primitive.
- `signalfd` (=282) — receive signals as readable file descriptor.
- Allows signal-safe select/poll integration.

### 14.4 Unix domain sockets
- `AF_UNIX` socket type — IPC via filesystem path (no network).
- Enables `X11`-style GUI IPC, `dbus`-like message buses.
- `socket(AF_UNIX, SOCK_STREAM, 0)` + `bind("/tmp/my.sock")`.

---

## Phase 15 — Dynamic ELF Linking

**Goal:** Shared libraries (.so). Programs can link against a shared libc.

### 15.1 Dynamic ELF loader (ld.so)
- Parse `PT_INTERP` segment — specifies the dynamic linker path.
- Kernel loads the ELF binary AND the dynamic linker into the process.
- Dynamic linker (`/lib/ld-oxide.so`) runs first, resolves PLT/GOT relocations.

### 15.2 oxide-libc (minimal shared C library)
- `malloc` / `free` / `realloc` backed by mmap + free list.
- `printf` / `scanf` / `fopen` / `fclose` — stdio.
- `pthread_create` (eventually) — backed by clone syscall.
- `execve`, `fork`, `wait` wrappers.
- Enables compiling existing C programs for OxideOS with minimal changes.

### 15.3 Shared library ABI
- `.so` files loaded from `/lib` and `/usr/lib`.
- Symbol lookup table (hash table of (name → address) per loaded library).
- Lazy binding: PLT stub resolves on first call (avoids startup overhead).
- `dlopen` / `dlsym` / `dlclose` for runtime loading.

---

## Phase 16 — Multi-Window GUI V2

**Goal:** Each userspace process manages its own windows. The compositor
becomes a proper display server rather than a single-window renderer.

### 16.1 Window server protocol (WayOxide)
Replace the current single-CONTENT_X/Y compositor with a proper protocol:

Client → server messages:
```
CreateWindow(width, height) → window_id: u32
DestroyWindow(window_id)
ResizeWindow(window_id, w, h)
SetTitle(window_id, title)
BlitBuffer(window_id, shm_id, src_rect, dst_rect)
RequestFocus(window_id)
```

Server → client events:
```
KeyEvent(window_id, keycode, modifiers, pressed)
MouseEvent(window_id, x, y, buttons)
FocusChange(window_id, gained: bool)
CloseRequest(window_id)
Exposed(window_id, rect)
```

- Use existing IPC message queues; each window gets its own queue pair.
- Compositor maintains a `windows: Vec<Window>` with position, size, z-order.
- Z-order management: click-to-front, window decorations, resize handles.

### 16.2 GUI applications
With the new protocol, implement:

| App | Description |
|-----|-------------|
| `/bin/file_manager` | Browse RamFS + FAT16 visually. Double-click to open. |
| `/bin/image_viewer` | Display BMP/PPM images from disk. |
| `/bin/calculator` | Basic arithmetic with button grid. |
| `/bin/clock_widget` | Floating analog or digital clock overlay. |
| `/bin/settings` | Screen resolution, font size, color scheme selector. |
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
- Global clipboard buffer (string + MIME type) in kernel.
- `ClipboardSet(data)` / `ClipboardGet() → data` IPC messages.
- Ctrl+C/V works across windows.

### 16.5 Drag-and-drop
- Mouse button press on a window decorates the drag operation.
- Compositor tracks drag target; sends `DropEvent(data)` to target window.

---

## Phase 17 — SMP (Symmetric Multiprocessing)

**Goal:** All available CPU cores run kernel + user code.

### 17.1 APIC initialization (replace PIC)
- Detect Local APIC via CPUID.
- Parse MADT from ACPI to enumerate all LAPIC IDs.
- Remap APIC registers (LAPIC at physical 0xFEE00000 via HHDM).
- Initialize BSP LAPIC: SpuriousVector, timer divisor, TPR.
- Replace PIC with APIC: mask all 8259 IRQs, enable APIC.
- APIC timer replaces PIT for per-CPU preemption.

Files: `kernel/src/kernel/apic.rs` (new), `kernel/src/kernel/pic.rs` (disable),
`kernel/src/kernel/interrupts.rs` (route IRQs via I/O APIC).

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
- `SpinLock<T>` wrapper using `lock xchg` (no_std, no std::sync needed).

### 17.6 SMP-safe kernel data structures
Audit and lock:
- `SOCK_TABLE` → `SpinLock<[Option<SocketEntry>; 16]>`
- `RAMFS` → `SpinLock<Option<RamFs>>`
- IPC queues → `SpinLock` per queue
- Physical allocator bitmap → `SpinLock` or atomic operations
- `NET` (smoltcp state) → `SpinLock` (smoltcp is not Send)

---

## Phase 18 — Security

**Goal:** A multi-user system with proper privilege separation.

### 18.1 Users and groups
- `/etc/passwd`: `oxide:x:1000:1000:OxideOS User:/home/oxide:/bin/sh`
- `/etc/shadow`: hashed passwords (sha256-crypt).
- `uid_t` / `gid_t` in `Task`: effective UID/GID, saved set-UID.
- Syscalls `getuid=102`, `getgid=104`, `setuid=105`, `setgid=106`.
- Permission check on `open()`: compare (uid, gid) against inode mode bits.
- `su` program — switch user after password verification.
- `login` program — presented at boot before shell.

### 18.2 ASLR (Address Space Layout Randomization)
- Randomize base addresses of: user stack, heap, mmap region, shared libraries.
- Use RDRAND instruction or a simple LCG seeded from HPET/TSC.
- Stack guard page: map a non-present page below the stack to catch overflows.
- Reduces exploitability of memory corruption bugs.

### 18.3 NX / XD enforcement
- Mark data pages as NX (No-Execute) in PTEs (bit 63 of PTE).
- Already have SMEP (kernel can't execute user pages).
- Add: user stack and heap are NX; only ELF PT_LOAD segments with PF_X are executable.

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
- Audit all existing syscall implementations.

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
- Enables true per-core preemption in SMP.

### 19.2 AHCI / SATA (replace ATA PIO)
**Problem:** ATA PIO blocks the CPU during disk I/O. Throughput ≈ 3 MB/s.

Implementation:
- Enumerate PCIe for class=0x01, subclass=0x06 (Mass Storage, SATA).
- Map AHCI HBA memory-mapped registers.
- Set up command list and FIS structures for each port.
- DMA transfers: kernel allocates bounce buffers, HBA DMAs into them.
- Interrupt-driven: AHCI fires an MSI/legacy IRQ on completion.
- Throughput: 100–600 MB/s (SATA II/III).

Files: `kernel/src/kernel/ahci.rs` (new), `kernel/src/kernel/vfs.rs` (route to AHCI).

### 19.3 USB keyboard & mouse (XHCI)
**Problem:** PS/2 is obsolete; modern machines (and most VMs) use USB HID.

Steps:
- Enumerate PCIe for XHCI controller (class=0x0C, subclass=0x03, progif=0x30).
- Initialize XHCI: reset HC, set up command/event rings.
- Enumerate USB devices; find HID boot-protocol keyboard and mouse.
- Process USB HID reports in interrupt handler → feed to `stdin` / mouse delta.
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
- Use `virtio-drivers` crate for VirtIO-blk.
- Faster than ATA PIO in QEMU; supports discard, flush.
- Auto-detected alongside RTL8139 at boot.

---

## Phase 20 — POSIX Compatibility & libc

**Goal:** Run real-world C programs compiled for OxideOS without modification.

### 20.1 Extended syscall surface
Add remaining high-value POSIX syscalls:

| Syscall | Number | Purpose |
|---------|--------|---------|
| `getpid` | 3 ✅ | Process ID |
| `getppid` | 140 | Parent PID |
| `setsid` | 141 | Create session |
| `setpgid` | 142 | Set process group |
| `getpgid` | 143 | Get process group |
| `uname` | 150 | Kernel version string |
| `time` | 151 | Seconds since epoch |
| `gettimeofday` | 152 | Microsecond precision |
| `nanosleep` | 153 | Sleep with ns precision |
| `clock_gettime` | 154 | Monotonic + realtime |
| `fcntl` | 155 | FD flags (O_NONBLOCK, FD_CLOEXEC) |
| `ioctl` | 92 ✅ | Device control |
| `mprotect` | 156 | Change page permissions |
| `msync` | 157 | Flush mmap to disk |
| `mlock` | 158 | Pin pages in RAM |
| `getrusage` | 159 | Resource usage stats |
| `sysinfo` | 160 | System memory/load info |
| `prctl` | 161 | Process control |
| `sendfile` | 162 | Zero-copy file-to-socket |
| `pread64` | 163 | Read at offset |
| `pwrite64` | 164 | Write at offset |
| `readv` | 165 | Scatter read |
| `writev` | 166 | Gather write |

### 20.2 C standard library (oxide-libc)
A minimal `libc.so` for OxideOS:
- Memory: `malloc`, `calloc`, `realloc`, `free`, `mmap`, `munmap`
- I/O: `open`, `close`, `read`, `write`, `lseek`, `stat`, `fstat`
- Stdio: `fopen`, `fclose`, `fread`, `fwrite`, `fprintf`, `printf`, `scanf`, `fgets`
- String: `strlen`, `strcpy`, `strcmp`, `strcat`, `strtol`, `sprintf`, `snprintf`
- Process: `fork`, `exec`, `waitpid`, `exit`, `getenv`, `setenv`
- Math: backed by `libm` crate
- Enables compiling musl/busybox programs with a OxideOS-targeted toolchain.

### 20.3 POSIX shell (ash/dash port)
- Cross-compile `dash` (POSIX shell) or `busybox sh` against oxide-libc.
- Replaces hand-written `/bin/sh` with a battle-tested POSIX shell.
- Enables running existing shell scripts.

### 20.4 Python 3 (stretch goal)
- Cross-compile CPython 3.x with oxide-libc and oxide-toolchain.
- Requires: mmap, select, clock_gettime, signal, threads (Phase 17).
- Marks the OS as "real" — can run interpreted programs.

---

## Phase 21 — Package Manager & Init System

**Goal:** Self-hosting development environment.

### 21.1 Init system (oxide-init)
Replace kernel-launched terminal with a proper init:
```
/sbin/oxide-init:
  1. Mount /proc, /dev, /sys, /tmp
  2. Run /etc/rc.d/ scripts (network up, daemons start)
  3. Spawn /bin/login on /dev/tty0
  4. Monitor children; respawn on crash
  5. Handle signals: SIGTERM → graceful shutdown sequence
```
- PID 1 always running; kernel sends SIGCHLD when any process exits.
- Service files in `/etc/rc.d/` (simple shell scripts: `start`, `stop`, `status`).

### 21.2 Package manager (opkg)
- Simple tarball-based packages (.opkg = gzip'd POSIX tar).
- `/etc/pkg/` database: installed name → file list + metadata.
- `opkg install <name>` — fetch from HTTP server, extract to /, update db.
- `opkg remove <name>` — unlink files listed in db.
- `opkg list` — show installed packages.

### 21.3 Self-hosted compiler (oxide-cc)
- Cross-compile `tcc` (Tiny C Compiler) or `cproc` against oxide-libc.
- Run on OxideOS itself: `oxide-cc hello.c -o hello` → produces ELF.
- Full self-hosting: OxideOS can compile OxideOS.

---

## Implementation Priority Order (Updated)

```
✅ DONE  Phases 1–9 (see completed list above)

── NEXT ────────────────────────────────────────────────────────────

🔥 Phase 10.1  argv/argc passing                    ← MOST IMPACTFUL
🔥 Phase 10.3  Shell pipes (cmd1 | cmd2)            ← Core shell feature
🔥 Phase 10.2  Environment variables                ← PATH, HOME, TERM
🔥 Phase 10.5  More coreutils (grep, echo, kill...) ← Daily usability
🔥 Phase 13.1  DHCP client activation               ← Network auto-config
🔥 Phase 13.2  DNS resolver                         ← wget by hostname

── MEDIUM PRIORITY ─────────────────────────────────────────────────

📌 Phase 11.1  COW fork                             ← Memory efficiency
📌 Phase 11.5  Linked-list kernel heap              ← Correctness
📌 Phase 12.1  ext2 write support                   ← Persistence
📌 Phase 12.3  procfs (/proc)                       ← Observability
📌 Phase 10.4  Job control (bg, fg, &)              ← Shell completeness
📌 Phase 13.3  select/poll syscall                  ← I/O multiplexing
📌 Phase 16.1  Window server protocol               ← GUI apps
📌 Phase 12.2  Block cache                          ← Performance
📌 Phase 14.1  Remaining signals (sigprocmask etc.) ← POSIX compat

── ADVANCED ────────────────────────────────────────────────────────

⚙  Phase 17    SMP (LAPIC, INIT-SIPI, per-CPU sched) ← Big milestone
⚙  Phase 15    Dynamic ELF linking                  ← Binary compat
⚙  Phase 18    Security (users, ASLR, pledge)       ← Production ready
⚙  Phase 19    Hardware V2 (AHCI, USB, audio, NVMe) ← Real hardware
⚙  Phase 20    POSIX libc compatibility             ← C program support
⚙  Phase 21    Package manager + init + self-host   ← OS maturity
```

---

## What Makes OxideOS a "Real OS"

The milestones that cross the line from hobby OS to real OS:

| Milestone | Phase | Status |
|-----------|-------|--------|
| Programs receive argv | 10.1 | ⬡ |
| Shell pipes work | 10.3 | ⬡ |
| DNS + wget by hostname | 13.2 | ⬡ |
| ext2 writable | 12.1 | ⬡ |
| procfs exists | 12.3 | ⬡ |
| select/poll | 13.3 | ⬡ |
| Multiple GUI windows | 16.1 | ⬡ |
| SMP (N cores used) | 17 | ⬡ |
| Users + permissions | 18.1 | ⬡ |
| libc + C programs | 20.2 | ⬡ |
| Python 3 runs | 20.4 | ⬡ |
| Self-compiling | 21.3 | ⬡ |

---

## File Layout Target

```
kernel/src/
├── kernel/
│   ├── apic.rs          ← Phase 17.1 (LAPIC)
│   ├── ahci.rs          ← Phase 19.2 (SATA)
│   ├── usb/             ← Phase 19.3
│   │   ├── xhci.rs
│   │   └── hid.rs
│   ├── audio/           ← Phase 19.4
│   │   └── hda.rs
│   ├── fs/
│   │   ├── ramfs.rs     ✅
│   │   ├── fat.rs       ✅
│   │   ├── ext2.rs      ✅ (read) ← write Phase 12.1
│   │   ├── procfs.rs    ← Phase 12.3
│   │   └── block_cache.rs ← Phase 12.2
│   ├── net/
│   │   ├── rtl8139.rs   ✅
│   │   ├── stack.rs     ✅ (smoltcp)
│   │   ├── socket.rs    ✅
│   │   ├── dns.rs       ← Phase 13.2
│   │   └── dhcp.rs      ← Phase 13.1
│   ├── mm/              ← Phase 11
│   │   ├── cow.rs       (COW fork)
│   │   ├── mmap.rs      (file-backed mmap)
│   │   └── freelist.rs  (frame free list)
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
├── oxide-libc/          ← Phase 20.2 (C library)
├── sh/                  ✅ (fork+exec, >, >>)
├── terminal/            ✅ (GUI terminal emulator)
├── coreutils/           ✅ + more in Phase 10.5
├── wget/                ✅
├── edit/                ✅
├── nc/                  ✅
├── httpd/               ← Phase 13.6
├── ping/                ← Phase 13.7
├── file_manager/        ← Phase 16.2
├── image_viewer/        ← Phase 16.2
├── login/               ← Phase 21.1
└── init/                ← Phase 21.1
```
