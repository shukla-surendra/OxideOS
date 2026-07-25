# OxideOS Development Plan

**Goal: an OS a real person can use.** Someone downloads an image, boots it
on QEMU or a real machine, logs in, saves files that survive a reboot,
browses the network, edits text in a windowed desktop, and installs
software — without reading kernel source to understand what works.

This plan replaces the old subsystem-phase roadmap. It is organized around
**milestones a user can verify**, not kernel internals. Old phase numbers
(P11.3, P18.1, …) are kept as cross-references so commit history and design
docs still line up; the full mapping is in the [appendix](#appendix-a--old-phase-number-map).

Two tracks run in parallel:

- **Track A — Usability (x86-64):** close the gaps between "impressive demo"
  and "daily-usable OS" on the mature x86-64 build.
- **Track B — AArch64 port:** feature-by-feature port on
  `feature/arm-support` (see [docs/arm/](arm/README.md)), one commit per
  stable feature, x86-64 stays green throughout.

---

## Where we are today (July 2026)

### What a user can already do (x86-64)

- **Boot** from ISO or installed disk image (Limine v9, UEFI/BIOS), straight
  into a graphical desktop with taskbar, start menu, and Activities overview.
- **Use real software:** GNU Bash 5.2, BusyBox 1.36 (300+ applets), Lua
  5.4.7, CPython 3.12, plus native coreutils and a nano-like editor.
- **Run desktop apps:** Notepad (find, word-wrap, clipboard), Terminal,
  File Manager, System Monitor, Browser, Calendar, Quick Settings — each in
  its own draggable/resizable/snappable window (syscalls 425–432).
- **Use the network:** DHCP auto-config on boot, DNS resolution,
  `wget`/`nc`/`ping`, TCP/UDP sockets (RTL8139, e1000, PCnet + smoltcp).
- **Run a real Unix userland:** fork/exec/waitpid with copy-on-write,
  Linux x86-64 syscall ABI (80+ syscalls), full POSIX signals, pipes, job
  control (`&`, `jobs`, `fg`), select/poll, termios TTY, env vars, FD
  inheritance.
- **Install to disk:** `/bin/install` + `make install-image` produce a
  bootable 192 MB MBR image (FAT32 EFI + FAT16 data).

### What still breaks the "usable" illusion

These are the gaps a user hits within minutes, in the order they hit them:

| # | Gap | User experience today | Old phase |
|---|-----|----------------------|-----------|
| 1 | **No persistent root** — ext2 is read-only + partial overwrite; root is RamFS | Edit a file, reboot, *your work is gone* | P12.1 |
| 2 | **No real users/login** — uid hardcoded to 1000, permissions stored but never enforced | No accounts, no passwords, everything runs as one user | P18.1, P21.1 |
| 3 | **Clipboard is per-app** — Notepad's Ctrl+C/V doesn't reach Terminal | Can't copy an error message into a note | P16.4 |
| 4 | **`ps`/`top` are approximations** — no `/proc/PID/*` | Can't see what a process is doing or how much memory it uses | P12.3b |
| 5 | **Disk I/O is slow** — ATA PIO, no block cache, ~3 MB/s, every read hits the disk | Launching Python from disk is visibly sluggish | P12.2, P19.2 |
| 6 | **Real hardware is a lottery** — PS/2-only input, ATA-only disk | Boots on QEMU/VirtualBox; on a modern laptop the keyboard may be dead | P19.2, P19.3 |
| 7 | **8×8 bitmap font, ASCII only** | Desktop looks dated; no Unicode | P16.3 |
| 8 | **Single core, no ASLR** | Fine for now, but caps performance and hardening | P17, P18.2 |

### AArch64 port status (Track B, active branch)

Reference platform: QEMU `virt` (Cortex-A72, GICv2, PL011, ramfb), Limine v9
on AAVMF — same boot protocol as x86-64. Detail in [docs/arm/](arm/README.md).

| Feature | Status |
|---------|--------|
| Arch abstraction layer (`arch::cpu` facade, no raw `asm!` outside `arch/`) | ✅ |
| Boot: EL1 exception vectors, PL011 serial console | ✅ |
| Input: virtio-input keyboard + mouse (polled virtio-mmio), works in GUI | ✅ |
| Disk: polled virtio-blk + FAT16/MBR/diskfs — files persist across reboots (ext2 pending) | ✅ |
| GICv2 + generic timer + PSCI power | 🔜 next |
| Memory: frame allocator, heap, TTBR0/1 paging (4 KB granule) | 🔜 |
| Framebuffer GUI desktop | 🔜 |
| Scheduler context switch | ⏳ |
| User mode (EL0) + SVC syscalls (Linux aarch64 ABI) | ⏳ |
| Networking: virtio-net | ⏳ |
| Userspace programs built for aarch64 | ⏳ |

---

## Track A — Usability milestones (x86-64)

Each milestone states the **user-visible outcome first**, then the work
items, then an acceptance test — a concrete action a user (not a kernel
developer) performs to confirm it's done. Milestones are ordered by how
quickly a user hits the gap they close.

### M1 — Your files survive a reboot 🔥 *(current priority)*

> **Outcome:** save a file in Notepad or the shell, power off, boot again —
> it's still there. OxideOS graduates from live-demo to actual tool.

Work items:
- **ext2 write completion** (P12.1): block/inode allocation via bitmaps,
  directory entry insert/delete, file create/truncate/append (`O_TRUNC`,
  `O_APPEND`), inode time updates, dirty write-back on `close()`, `sync`
  syscall (162). Files: `kernel/src/kernel/fs/ext2.rs`, `vfs.rs`.
  (Low-level `write_block_from_scratch` and in-place overwrite already exist.)
- **Block cache** (P12.2): 64-entry LRU keyed by (device, block); dirty
  blocks flushed on eviction/`sync`. Benefits FAT16 and ext2 alike, and
  hides most of the ATA PIO latency (gap #5) before AHCI lands.
- **Symbolic + hard links** (P12.4/12.5): `symlink`(88)/`readlink`(89)
  (currently EINVAL stubs), `link`(86) with inode refcounts, VFS follows up
  to 8 hops, `ls -l` shows `->`.
- **FHS completion** (P12.7): add `/sbin /usr/bin /usr/lib /var /mnt`;
  seed `/etc/hostname`, `/etc/hosts`, `/etc/resolv.conf`.

**Acceptance:** `echo hello > /ext2/note.txt`, reboot, `cat /ext2/note.txt`
prints `hello`. `ln -s` and `ln` behave as on Linux. Second `cat` of a large
file is near-instant (cache hit).

### M2 — The system tells you the truth

> **Outcome:** `ps`, `top`, and System Monitor show real per-process data;
> when something is slow or stuck, a user can find out why.

Work items:
- **Per-process procfs** (P12.3b): `/proc/PID/status` (pid, ppid, state,
  memory), `/proc/PID/maps` (from per-task mmap tracking), `/proc/PID/fd/`,
  `/proc/PID/cmdline`. Extend `kernel/src/kernel/fs/procfs.rs` by walking
  the scheduler task table.
- Point `ps`, `sysmon`, and BusyBox `top` at the new entries.
- **POSIX timers** (P14.2): `setitimer`/`alarm`; verify `clock_gettime`
  CLOCK_REALTIME reads the RTC, so timestamps and `uptime` are honest.

**Acceptance:** `cat /proc/$$/status` works in Bash; System Monitor's
per-process memory numbers change when a Python script allocates.

### M3 — A desktop that feels finished

> **Outcome:** copy text from Terminal into Notepad, read non-ASCII text,
> open an image — the desktop stops feeling like a tech demo.

Work items:
- **Global clipboard** (P16.4): compositor-owned buffer (string + MIME),
  `ClipboardSet`/`ClipboardGet` in the Gui* syscall family; Ctrl+C/V across
  all windows.
- **Window protocol v2** (P16.1b): formal `FocusChange`, `CloseRequest`,
  `Exposed`, `ResizeWindow`, `SetTitle` messages so apps redraw and close
  cleanly instead of relying on ad-hoc `take_closed_window()`.
- **Unicode font** (P16.3): `noto-sans-mono-bitmap` — Latin/Cyrillic/Greek,
  multiple sizes, anti-aliased.
- **Missing small apps** (P16.2): `image_viewer` (PNG decoder already a
  dependency), `calculator`, full `settings`, `about`.
- Stretch: drag-and-drop between windows (P16.5).

**Acceptance:** select an error in Terminal, Ctrl+C, Ctrl+V into Notepad.
Open a PNG from File Manager. `echo Привет` renders correctly.

### M4 — Log in as yourself

> **Outcome:** boot to a login prompt (or greeter), enter a password, land
> in your own `/home/<user>`; other users' files are off-limits.

Work items:
- **Users & groups** (P18.1): real uid/gid on the `Task` struct, wire
  `getuid/setuid/getgid/setgid` (currently stubs at
  `syscall_core.rs:612-619`), enforce mode bits on `open()` (EACCES),
  `/etc/passwd` + `/etc/shadow` (sha256 via `sha2` crate), `su`.
- **Init system** (P21.1): `/sbin/oxide-init` as PID 1 — mount `/proc`
  `/dev` `/tmp`, run `/etc/rc.d/` scripts, spawn `login` on the console or
  launch the desktop, reap zombies, respawn on crash, graceful shutdown on
  SIGTERM. (SIGCHLD plumbing already works.)
- **login** program; first-boot user creation hook for the installer.
- **Syscall pointer hardening** (P18.5): audit every user-pointer syscall
  for `validate_user_range()`; return EFAULT, never panic. A login system
  is only as strong as its weakest syscall.

**Acceptance:** create user `alice`, log out, log in as `alice`,
`cat /home/root/secret` fails with permission denied.

*Depends on M1 (accounts must persist).*

### M5 — Networking you can build on

> **Outcome:** OxideOS serves a web page you can open from the host
> browser; network programs don't hang.

Work items:
- **Non-blocking sockets** (P13.4): `O_NONBLOCK`, EAGAIN semantics.
- **setsockopt/getsockopt** (P13.5): `SO_REUSEADDR`, timeouts, `TCP_NODELAY`.
- **`/bin/httpd`** (P13.6): single-threaded select-based server for
  `/var/www` — also the delivery channel for the M8 package manager.
- **DHCP lease renewal**, TCP keepalive (P13.8).
- Stretch: Unix domain sockets (P14.4) — cleaner IPC foundation for the
  window server and a future dbus-like bus.

**Acceptance:** `httpd &` inside OxideOS; `curl http://localhost:<fwd-port>/`
on the host returns a page. Two rapid server restarts don't hit
"address already in use".

### M6 — Boots on the machine under your desk

> **Outcome:** write the image to a USB stick, boot a real (or modern
> virtual) x86-64 machine: keyboard, mouse, and disk all work; installer
> puts a persistent ext2 root on the internal disk.

Work items:
- **AHCI/SATA** (P19.2): reuse existing PCI enumeration; command
  list/FIS/DMA, interrupt-driven. 100–600 MB/s vs 3 MB/s PIO.
- **USB HID via XHCI** (P19.3): boot-protocol keyboard + mouse, feeding the
  same internal input interface as PS/2; use both if both present.
- **VirtIO-blk** (P19.6, `virtio-drivers` crate): fast disk under QEMU/KVM
  — shared groundwork with Track B's virtio work.
- **ext2-root installer** (P22): partition 1 FAT32 `/boot` (Limine, kernel),
  partition 2 ext2 `/` — extend the existing 582-line
  `kernel/src/kernel/installer.rs`; `/etc/fstab` mounted by init.
- **Live mode** (P22.2): boot from USB with RamFS overlay over read-only
  ext2; `install-oxide` desktop shortcut launches the installer.
- **First-boot wizard** (P22.5): timezone, user account (M4), network.
- Stretch: NVMe (P19.5).

**Acceptance:** dd the image to USB, boot a post-2015 laptop or a
QEMU/VirtualBox VM with AHCI + USB input, install to disk, reboot without
the USB stick, log in, previous session's files intact.

*Depends on M1 (ext2 root) and M4 (accounts in the wizard).*

### M7 — Fast and hardened

> **Outcome:** the OS uses every core, resists memory-corruption exploits,
> and stays responsive under load. Mostly invisible when it works — that's
> the point.

Work items:
- **Memory efficiency**: O(1) physical frame free list (P11.6), demand
  paging / lazy brk (P11.4) — big win for Python and Bash.
- **SMP** (P17): LAPIC + I/O APIC (replace PIC/PIT, P19.1), INIT-SIPI AP
  bring-up, per-CPU data via `gs`, per-CPU run queues with work stealing,
  spinlock audit of `SOCK_TABLE`/`RAMFS`/IPC/allocator/`NET`/compositor.
  The single biggest engineering item in this track.
- **ASLR + stack guard page** (P18.2), NX audit on stack/heap (P18.3).
- Stretch: pledge-style capabilities (P18.4), stack canaries (P18.6).

**Acceptance:** `nproc` reports all cores; a multi-process compile-like
workload scales; `/proc/PID/maps` shows different layouts per run; a
guard-page stack smash is caught, not silently corrupting.

### M8 — A platform others can target

> **Outcome:** developers compile C programs *for* OxideOS against a real
> libc, and users install software with one command.

Work items:
- **File-backed mmap** (P11.3): MAP_PRIVATE COW / MAP_SHARED write-through
  — prerequisite for everything below, and lets CPython mmap its image.
- **Dynamic linking** (P15): `PT_INTERP`, `/lib/ld-oxide.so`, PLT/GOT lazy
  binding, `dlopen`/`dlsym`.
- **oxide-libc** (P20.2): shared minimal libc (malloc, stdio, string,
  process, `libm` math) — complements static musl, shrinks every binary.
- **Package manager `opkg`** (P21.2): gzip'd tar packages via `miniz_oxide`,
  install/remove/list, fetched over HTTP (M5's httpd as test server),
  `opkg upgrade` with Limine fallback boot entry (P22.6).
- **Self-hosting** (P21.3): port `tcc` or `cproc`; OxideOS compiles a C
  program on OxideOS.
- Stretch: Intel HDA audio + `/dev/audio` (P19.4), eventfd/signalfd (P14.3).

**Acceptance:** on OxideOS itself: `opkg install hello && hello`, then
`oxide-cc hello.c -o hello2 && ./hello2`.

---

## Track B — AArch64 port

Runs on `feature/arm-support`, interleaved with Track A. Rules (from
[docs/arm/README.md](arm/README.md)): no raw `asm!` outside `arch/<target>/`;
same Limine boot protocol both arches; **x86-64 must stay green** before any
ARM feature lands; one feature = one commit + design doc in `docs/arm/`.

Ordered feature queue (picks up where the status table above leaves off):

| Step | Feature | Notes | Doc |
|------|---------|-------|-----|
| B1 | GICv2 + generic timer + PSCI | Interrupts, 100 Hz tick, clean shutdown/reboot | 04-gic-timer-psci.md |
| B2 | Memory: frame allocator, heap, TTBR0/1 paging | 4 KB granule; reuse portable allocator logic | 05-memory.md |
| B3 | Framebuffer GUI desktop | ramfb; compositor is already portable Limine-fb code | 06-gui.md |
| B4 | Scheduler context switch | x0–x30/SP/ELR/SPSR save-restore | — |
| B5 | EL0 user mode + SVC syscalls | Linux **aarch64** syscall numbers (differ from x86-64!) | — |
| B6 | ✅ virtio-blk + FAT16 (ext2 pending) | Done ahead of B1–B5: polled virtio-mmio, real FAT/MBR/diskfs stack reused | [04-virtio-blk.md](arm/04-virtio-blk.md) |
| B7 | virtio-net + smoltcp | smoltcp is arch-independent | — |
| B8 | aarch64 userspace builds | oxide-rt SVC stubs; rebuild coreutils/sh; musl aarch64 | — |

**End state:** the Track A milestones apply to both architectures — the
acceptance tests for M1–M5 should pass identically under
`make KARCH=aarch64 run`. Real ARM hardware (Raspberry Pi 4/5) becomes the
Track B counterpart of M6 once virtio is replaced by board drivers.

**Cross-track synergy:** B6/M6 share virtio; the arch abstraction forces the
`#ifdef`-free discipline that also makes SMP (M7) tractable; ext2/procfs/
clipboard/login work from Track A is portable and lands on ARM for free.

---

## Priority order at a glance

```
NOW        M1  Persistence (ext2 write, block cache, links)      ← 🔥
           B1  GIC + timer + PSCI                                ← 🔥 (parallel)
NEXT       M2  Truthful procfs/ps/top          B2–B3  ARM memory + GUI
           M3  Clipboard, protocol v2, fonts   B4–B5  ARM sched + EL0
THEN       M4  Login & permissions (needs M1)
           M5  Server-grade networking         B6–B8  ARM disk/net/userspace
LATER      M6  Real hardware + ext2 installer (needs M1, M4)
           M7  SMP, ASLR, demand paging
           M8  Dynamic linking, opkg, self-hosting
```

Rule of thumb: within a track, finish the current milestone's acceptance
test before starting the next; between tracks, alternate freely — commits
stay small and each leaves both architectures bootable.

---

## The "real OS" scorecard

Crossed off when a user can verify it without reading source:

| A user can… | Milestone | Status |
|-------------|-----------|--------|
| Boot to a desktop and run Bash/Python/Lua/BusyBox | — | ✅ |
| Use pipes, job control, signals, env vars | — | ✅ |
| Reach the internet by hostname (DHCP + DNS + wget) | — | ✅ |
| Run each app in its own movable window | — | ✅ |
| Install to a disk image and boot from it | — | ✅ |
| Boot the same OS on ARM with working input | B (partial) | ✅ |
| Keep files across a reboot | M1 | ⬡ |
| See honest `ps`/`top` output | M2 | ⬡ |
| Copy-paste between windows; read Unicode | M3 | ⬡ |
| Log in with a password; be protected from other users | M4 | ⬡ |
| Serve a web page from OxideOS | M5 | ⬡ |
| Boot real hardware with USB keyboard + SATA disk | M6 | ⬡ |
| Use all CPU cores | M7 | ⬡ |
| `opkg install` software; compile C on-device | M8 | ⬡ |
| Do all of the above on ARM | B1–B8 | ⬡ |

---

## Appendix A — Old phase-number map

For continuity with commit history and older design docs:

| Old phase | Topic | Now in |
|-----------|-------|--------|
| 1–10.6, 11.1, 11.2, 11.5, 12.3, 13.1–13.3, 13.7, 14.1, 22 (FAT) | Completed foundations | Done (see git history) |
| 11.3 file-backed mmap | Memory | M8 |
| 11.4 demand paging | Memory | M7 |
| 11.6 frame free list | Memory | M7 |
| 12.1 ext2 write | Filesystem | **M1** |
| 12.2 block cache | Filesystem | **M1** |
| 12.3b per-PID procfs | Filesystem | M2 |
| 12.4/12.5 sym/hard links | Filesystem | M1 |
| 12.6 flock | Filesystem | M4 (stretch) |
| 12.7 FHS | Filesystem | M1 |
| 13.4/13.5 nonblock, sockopts | Network | M5 |
| 13.6 httpd | Network | M5 |
| 13.8 DHCP renewal etc. | Network | M5 |
| 14.2 timers | IPC | M2 |
| 14.3 eventfd/signalfd | IPC | M8 (stretch) |
| 14.4 unix sockets | IPC | M5 (stretch) |
| 15 dynamic linking | Toolchain | M8 |
| 16.1b protocol v2 | GUI | M3 |
| 16.2 small apps | GUI | M3 |
| 16.3 fonts | GUI | M3 |
| 16.4 clipboard | GUI | M3 |
| 16.5 drag-and-drop | GUI | M3 (stretch) |
| 17 SMP | Kernel | M7 |
| 18.1 users/groups | Security | M4 |
| 18.2 ASLR | Security | M7 |
| 18.3 NX audit | Security | M7 |
| 18.4 capabilities | Security | M7 (stretch) |
| 18.5 pointer hardening | Security | M4 |
| 18.6 stack canary | Security | M7 (stretch) |
| 19.1 APIC timer | Hardware | M7 |
| 19.2 AHCI | Hardware | M6 |
| 19.3 XHCI USB | Hardware | M6 |
| 19.4 HDA audio | Hardware | M8 (stretch) |
| 19.5 NVMe | Hardware | M6 (stretch) |
| 19.6 virtio-blk | Hardware | M6 + B6 |
| 20.1 syscall gaps | POSIX | folded into owning milestones |
| 20.2 oxide-libc | POSIX | M8 |
| 21.1 init + login | System | M4 |
| 21.2 opkg | System | M8 |
| 21.3 self-hosting | System | M8 |
| 22 ext2-root install | System | M6 |

## Appendix B — `no_std` crates on deck

| Crate | Status | For |
|-------|--------|-----|
| `pc-keyboard`, `smoltcp`, `limine`, `lazy_static`, `png`, `linked_list_allocator`, `oxide-gui-core` | ✅ in use | — |
| `noto-sans-mono-bitmap` | planned | M3 fonts |
| `sha2` | planned | M4 password hashing |
| `virtio-drivers` | planned | M6 / B6 |
| `acpi` | planned | M7 MADT/SMP discovery |
| `miniz_oxide` | planned | M8 packages |
| `x86_64`, `heapless`, `libm`, `postcard`, `chacha20poly1305` | available | as needed |
