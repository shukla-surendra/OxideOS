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
| DHCP client — automatic IP configuration on boot | ✅ |
| DNS resolver — A-record via UDP; kernel syscall 435; oxide-rt wrapper | ✅ |
| wget — accepts http://hostname/path, DNS resolution, URL parsing | ✅ |
| Socket syscalls — socket/bind/connect/listen/accept/send/recv | ✅ |
| Socket syscalls — sendto/recvfrom (UDP) | ✅ |
| Job control — & background, jobs, fg N | ✅ |
| File permissions — mode/uid/gid on RamFS inodes, chmod/chown | ✅ |
| unlink/rename/truncate syscalls | ✅ |
| SMEP (CR4 bit 20) | ✅ |
| ACPI shutdown — RSDP→FADT→PM1a_CNT_BLK | ✅ |
| BSoD crash dump — framebuffer + serial register dump | ✅ |
| Shell `/bin/sh` — fork+exec, >, >> redirect | ✅ |
| Coreutils: ls, cat, cp, mv, rm, mkdir, pwd, ps, wget, edit, nc | ✅ |
| Text editor `/bin/edit` — nano-like, VT100 | ✅ |
| **argv/argc passing** — System V AMD64 ABI argv block on user stack, `oxide-rt::arg()`/`argc()`, `ExecArgs` syscall 6, shell passes args | ✅ |
| Netcat `/bin/nc` — TCP listen/connect, UDP send/listen | ✅ |
| **GUI userspace API** — per-process windows (GuiCreate/Destroy/FillRect/DrawText/Present/PollEvent/GetSize/BlitShm, syscalls 125–132) | ✅ |
| **GUI file manager** `/bin/filemanager` — directory navigation, file listing, keyboard+mouse input | ✅ |

### Remaining gaps (priority order)

| Gap | Blocks |
|-----|--------|
| ~~**argv passing**~~ | ✅ Done |
| **Open-source C programs** — musl libc + Linux ABI compat | Running real programs |
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
### ✅ Phase 10.2 — Environment Variables
Global env store in kernel (32 vars, 256-byte values). Getenv (79) / Setenv (80) syscalls.
`oxide-rt::getenv_bytes()` / `setenv()` wrappers. Shell: `export VAR=val`, `$VAR` expansion.
Default env: PATH=/bin, HOME=/, TERM=vt100, USER=oxide, SHELL=/bin/sh, HOSTNAME=oxideos.

### ✅ Phase 10.3 — Shell Pipes
Pipeline execution up to 8 stages: `cmd1 | cmd2 | ... | cmdN`.
fork/dup2/pipe plumbing in sh. Output redirect applies to last stage. Works end-to-end:
`ls | grep sh | wc -l`, `ls | sort`, `cat file | head -n 5`, etc.

### ✅ Phase 10.5 — More Coreutils
echo, grep (substring match), wc (-l/-w/-c), head (-n N), tail (-n N), sort (lexicographic),
sleep (seconds), kill (-signal pid), touch (create file), true (exit 0), false (exit 1).

### ✅ Phase 10.1 — argv/argc Passing
System V AMD64 ABI argv block written to user stack by kernel. `oxide-rt::arg(i)` / `argc()` helpers. `ExecArgs` syscall 6 lets shell pass argv[1..] to exec'd programs. Shell updated to call `exec_args(prog, args)`. `ls` and `cat` coreutils updated to use argv.
SYSCALL/SYSRET, SMEP, ACPI proper shutdown, BSoD crash dump, ATA alignment fix

---

## Phase 10 — argv, Environment Variables & Shell Pipes

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

---

## Phase 10.6 — Open-Source Software Support  ← NEW GOAL

**Goal:** Compile and run real open-source projects (Lua, BusyBox, curl, etc.) on OxideOS,
starting with simple single-binary C programs and building up to interactive tools.

### Why this is the critical path

OxideOS already has: processes, ELF loading, a filesystem, networking, argv, a shell.
The missing piece is a **C runtime** and **Linux ABI compatibility** so that programs
compiled with standard toolchains just work.

### 10.6.1 Linux x86-64 syscall ABI shim  ← FIRST STEP

**Problem:** OxideOS syscall numbers don't match Linux. A program compiled for Linux
calls `write` as syscall 1, but OxideOS maps 1 to `Fork`.

**Options:**
- **A (recommended): Remap OxideOS to Linux numbers.**
  Change OxideOS syscall constants to match Linux x86-64 ABI. Update `oxide-rt` and
  all userspace programs. One-time churn, then all standard toolchains just work.
- **B: Personality-based shim.** Detect Linux binaries (ELF note `NT_GNU_ABI_TAG`)
  and translate syscall numbers at the `int 0x80` / `syscall` entry point.

Recommended: **Option A** (renumber OxideOS). Affected numbers:

| Syscall | Linux | OxideOS now | Action |
|---------|-------|-------------|--------|
| read    | 0     | 20          | renumber |
| write   | 1     | 21          | renumber |
| open    | 2     | 22          | renumber |
| close   | 3     | 23          | renumber |
| mmap    | 9     | 9           | ✅ already matches |
| brk     | 12    | 11          | renumber |
| exit    | 60    | 0           | renumber |
| fork    | 57    | 1           | renumber |
| execve  | 59    | 5           | renumber |
| getpid  | 39    | 3           | renumber |
| wait4   | 61    | 2           | renumber |

Files: `kernel/src/kernel/syscall_core.rs`, `userspace/oxide-rt/src/lib.rs`,
all `userspace/*/src/main.rs`.

### 10.6.2 musl libc cross-compilation

**Goal:** Build a musl libc static library targeted at OxideOS.

Steps:
1. Clone musl 1.2.x: `git clone https://git.musl-libc.org/git/musl`
2. Configure: `CC=x86_64-linux-musl-gcc ./configure --prefix=/opt/oxide-musl --target=x86_64-oxide`
3. Patch musl's `src/internal/syscall.h` to use OxideOS syscall numbers.
4. Build: `make -j$(nproc)`
5. Result: `/opt/oxide-musl/lib/libc.a` and `/opt/oxide-musl/include/`

Cross-compile a test program:
```bash
x86_64-linux-musl-gcc -static -nostdinc \
  -I/opt/oxide-musl/include \
  -L/opt/oxide-musl/lib \
  hello.c -o hello-oxide
```
Load `hello-oxide` into OxideOS RamFS and run via the shell.

### 10.6.3 select/poll syscalls  ← required by most programs

Almost every C program that does I/O uses `select` or `poll`. Without them, programs
block on reads and can't multiplex input.

Implementation:
- `poll(fds: *mut pollfd, nfds: u64, timeout_ms: i64) -> i64` (syscall 7 on Linux)
  - Check each fd for POLLIN/POLLOUT readiness without blocking.
  - If nothing ready and `timeout_ms > 0`, put task to sleep until fd ready or timeout.
- `select(nfds, readfds, writefds, exceptfds, timeout)` (syscall 23 on Linux)
  - Implemented on top of `poll`.

Files: `kernel/src/kernel/syscall_core.rs`, `kernel/src/kernel/syscall.rs`.

### 10.6.4 Real munmap

Currently `munmap` is a no-op. Programs that use mmap for dynamic memory (musl's
allocator) will leak unless munmap actually unmaps pages.

Implementation:
- Track per-task mmap regions: `Vec<(virt_base: u64, len: u64)>` in `Task`.
- `munmap(addr, len)`: find the region, unmap PTEs, return physical frames.
- TLB flush (`invlpg`) for every unmapped page.

### 10.6.5 argv via execve (argc+argv+envp on stack)

musl's `_start` reads `argc`, `argv`, and `envp` from the initial stack using the
full System V AMD64 ABI layout. OxideOS currently writes `argc`/`argv` but no `envp`
beyond two NULL terminators. musl is tolerant of empty envp, but we should verify.

Add minimal environment variables (PATH, HOME, TERM, USER) to the envp block:
- Kernel writes them after the argv NULL.
- `oxide-rt::getenv(key)` walks the envp block.

### 10.6.6 Target: Run Lua 5.4

Lua is an ideal first target:
- Single-file C99 codebase (~30 KLOC)
- Minimal syscall surface: open/read/write/close/exit/mmap/munmap/time
- No threads, no signals
- Compiles as a static binary with musl libc

Cross-compile steps:
```bash
make PLAT=generic CC="x86_64-linux-musl-gcc" \
  MYCFLAGS="-static -fno-stack-protector" \
  MYLDFLAGS="-static" lua
```
Load `lua` into RamFS, run `lua hello.lua`.

### 10.6.7 Target: Run BusyBox

BusyBox provides 300+ Unix tools in a single binary — a fully functional userland.

```bash
make ARCH=x86_64 CC=x86_64-linux-musl-gcc \
  CONFIG_STATIC=y CONFIG_PREFIX=/bin busybox
```

Copy to OxideOS FAT16 or RamFS. Symlinks for each applet:
```
/bin/busybox → busybox
/bin/ls      → busybox
/bin/grep    → busybox
...
```

Requires working: `fork`, `exec`, `wait`, `open`, `read`, `write`, `stat`, `mmap`,
`munmap`, `getdents64`, `select`/`poll`, `ioctl`, `uname`.

### 10.6.8 Future targets (stretch)

| Program | Difficulty | Key requirements |
|---------|-----------|-----------------|
| coreutils (GNU) | Medium | full POSIX stat, links |
| Python 3 | Hard | threads, select, complex signal handling |
| curl/wget2 | Medium | select, DNS, TLS (mbedtls) |
| Nginx | Hard | fork, epoll (or select), signals |
| SQLite | Easy | file I/O, mmap, no threads |
| Redis | Medium | select/epoll, fork |
| Lua 5.4 | Easy | minimal syscalls |
| MicroPython | Easy | mmap, minimal stdlib |

### Implementation order

```
✅ 10.6.1  Remap syscall numbers to Linux ABI
✅ 10.6.3  select/poll (poll syscall 7 implemented)
✅ 10.6.4  real munmap — unmap+free physical frames, per-task region tracking
✅ 10.6.5  envp on stack — full SysV ABI (argc+argv[]+NULL+envp[]+NULL)
           + uname(63), getppid(110), clock_gettime(228), mprotect(10),
             fcntl(72), set_tid_address(218), arch_prctl(158)
✅ 10.6.2  musl cross-compilation setup (musl-gcc, hello_musl, musl_test)
✅ 10.6.6  Lua 5.4.7 embedded — syscalls: sigprocmask(14), lstat(6), readv(19),
           mremap(25→ENOMEM), getuid/getgid(102/104)→1000, gettid(186)→pid,
           futex(202)→0, pipe2(293), lseek(8), exit_group(231)
✅ 10.6.7  Run BusyBox — struct stat (144 B), getdents64, FdBackend::Dir, fstat, stat
```

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
✅ DONE  Phases 1–9, 10.1–10.3, 10.5 (argv, env vars, pipes, coreutils)
✅ DONE  Phase 10.6.1–10.6.5 (Linux ABI, select/poll, munmap, envp, new syscalls)
✅ DONE  Phase 22 (installable OS, /bin/install, pre-built image)

── NEXT: Open-Source Software ──────────────────────────────────────

🔥 Phase 10.6.2  musl libc cross-compilation           ← first real C programs
🔥 Phase 10.6.6  Run Lua 5.4                           ← proof of concept
🔥 Phase 10.6.7  Run BusyBox                           ← full Unix userland
✅ Phase 13.1  DHCP client activation — blocking spin-poll in init(); fallback static 10.0.2.15/24
✅ Phase 13.2  DNS resolver — UDP query to NET_CONFIG.dns; kernel syscall 435 DnsResolve; oxide-rt::dns_resolve(); wget updated to accept URLs/hostnames
✅ Phase 10.4  Job control — & background, jobs builtin, fg N

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
✅ Phase 22    Installable OS (USB image, installer, pre-built image, /bin/install) ← done
```

---

## Phase 22 — Installable OS

**Goal:** OxideOS can be written to a USB stick, booted on real x86-64 hardware, and
installed to an internal disk with a persistent filesystem — `/bin`, `/etc`, `/home`
survive reboot just like Ubuntu.

### Prerequisites (must complete first)
- Phase 12.1 (ext2 write) — persistent root filesystem
- Phase 12.7 (FHS-lite) — `/etc`, `/home`, `/bin`, `/usr` directory structure
- Phase 19.2 (AHCI/SATA) — real disk I/O beyond QEMU ATA PIO
- Phase 21.1 (init system) — PID 1 mounts filesystems, starts services

### 22.1 Bootable USB image

Create a single-file `.img` that can be written to USB with `dd`:
- MBR partition table: partition 1 = FAT32 (EFI system / Limine boot), partition 2 = ext2 root
- Limine bootloader installed to MBR + FAT32 `/EFI/BOOT/BOOTX64.EFI` (UEFI) and `/limine-bios.sys` (BIOS)
- Root partition pre-populated with `/bin`, `/etc`, `/lib`, `/home`
- Build target: `make usb-image` → `oxideos.img`

```
Disk layout (example, 2 GB):
  Partition 1: FAT32  64 MB  → /boot (Limine, kernel ELF, initrd)
  Partition 2: ext2  ~2 GB   → /     (root filesystem, writable)
```

### 22.2 Live mode (run without installing)

On first boot from USB, default to **live mode**:
- Root mounts the ext2 read-only; overlay RamFS on top for writes.
- User can explore OxideOS, run programs, connect to network.
- A desktop shortcut / shell command `install-oxide` launches the installer.

Implementation:
- Kernel detects `live=1` boot parameter (Limine config entry).
- VFS overlay: writes go to RAM, reads fall through to ext2.

### 22.3 Installer (`/sbin/oxide-install`)

A text-UI (or GUI) installer program:

```
Step 1: Detect disks
  - Enumerate ATA/AHCI devices via PCI scan
  - Show: /dev/sda (320 GB, WD), /dev/sdb (64 GB, SSD)

Step 2: Partition target disk
  - Option A: Use entire disk (automatic)
  - Option B: Manual (show current layout, let user pick partition)
  - Write MBR partition table via syscall or direct ATA write

Step 3: Format partitions
  - mkfs.fat32 → boot partition
  - mkfs.ext2 → root partition

Step 4: Copy root filesystem
  - rsync-style: walk live ext2, copy each inode to target ext2
  - Show progress bar

Step 5: Install bootloader
  - Write Limine MBR stage to disk sector 0
  - Copy limine-bios.sys + kernel ELF to boot partition
  - Write /boot/limine.conf with correct root UUID

Step 6: Configure
  - Set hostname (/etc/hostname)
  - Create first user (/etc/passwd entry, /home/<user>)
  - Set root password hash (/etc/shadow)

Step 7: Done
  - Eject USB, reboot from internal disk
```

Files: `userspace/install/src/main.rs` (new), `kernel/src/kernel/fs/ext2_write.rs` (Phase 12.1)

### 22.4 Persistent filesystem layout on disk

After installation, the disk has a real FHS-compliant ext2 root:
```
/bin/         → shell, coreutils, busybox applets
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
/proc/        → procfs (Phase 12.3)
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
- Text-UI wizard: set timezone, create user account, configure network.
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
| Programs receive argv | 10.1 | ✅ |
| musl libc + first C program | 10.6 | ⬡ |
| BusyBox runs | 10.6.7 | ⬡ |
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
| Persistent /home + /etc on real disk | 22.4 | ⬡ |
| Bootable USB image | 22.1 | ✅ |
| Installer program | 22.3 | ✅ |

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
