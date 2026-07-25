# 04 — Persistent storage: virtio-blk + the portable FAT16 stack

**Status: done.** Files written on aarch64 survive reboots. `/disk` (FAT16)
works in the GUI terminal (`ls /disk`, `cat`, `mkdir`, redirects) and in
Notepad (open/save), backed by a QEMU virtio-blk disk.

## What landed

1. **`arch/aarch64/virtio_blk.rs`** — a polled virtio-blk driver over
   virtio-mmio, the storage sibling of `virtio_input.rs` (doc 03).
2. **The real x86 storage stack compiled on aarch64** — `fs/fat.rs`,
   `fs/mbr.rs`, `fs/diskfs.rs`, `drivers/disk_store.rs` replace their
   `stubs.rs` stand-ins via the same `#[path]` include trick already used
   for ramfs. Zero changes to those files.
3. **Boot wiring** (`main.rs` Stage 4.5): RamFS init → virtio-blk probe →
   record-store mount attempt → `diskfs::populate()` → MBR parse → FAT16
   mount. Same order as the x86 `boot_init` path.
4. **Makefile**: `VBLK_FLAG` attaches `oxide_disk.img` to every aarch64 run
   target as a `virtio-blk-device`; the `disk` target now creates a true
   FAT16 image (see "bugs found" below).

## Driver design

The QEMU `virt` machine exposes block devices as virtio-mmio slots
(base `0x0a00_0000`, stride `0x200`, 32 slots — same window the input
driver probes). We claim every slot with device ID **2** (block), handling
both legacy (version 1) and modern (version 2) transports.

Per device, one request virtqueue with a **permanent three-descriptor
chain**, rebuilt never, reused for every request:

```
desc[0] → request header {type, reserved, sector}   16 B   device-readable
desc[1] → sector data                               512 B  direction flips per request
desc[2] → status byte                               1 B    device-writable
```

A request is: fill header (`VIRTIO_BLK_T_IN`=read / `T_OUT`=write), set
desc[1]'s `WRITE` flag for reads, publish desc 0 in the avail ring, kick
`QUEUE_NOTIFY`, then **spin on the used ring** until the device retires the
chain (bounded spin, then a serial-logged timeout). Polling is fine here:
QEMU retires requests in microseconds, and every existing caller
(FAT16, MBR, record store) is synchronous single-sector anyway. When the
GIC lands, the spin loop is the only part that changes.

Ring and request buffers come from the kernel heap, which lives in the
Limine HHDM, so `phys = virt − hhdm_offset` — no page tables needed.

## The `ata` facade trick

`fat.rs`, `mbr.rs` and `disk_store.rs` call `crate::kernel::ata::…`
(`read_sector(idx, lba, buf)`, `write_sector`, `is_present*`, `disk_info`).
The driver exports **exactly that API**, and on aarch64 the flat path is
re-pointed in `stubs.rs`:

```rust
pub use crate::kernel::arch::aarch64::virtio_blk as ata;

#[path = "drivers/disk_store.rs"] pub mod disk_store;
#[path = "fs/diskfs.rs"]          pub mod diskfs;
#[path = "fs/mbr.rs"]             pub mod mbr;
#[path = "fs/fat.rs"]             pub mod fat;
```

Logical disk indices keep ATA semantics: the first virtio-blk device found
is disk **0** (FAT16 `/disk`), the second maps to disk **3** — the
"secondary" slot the x86 ext2/installer path expects (`PROBE_TO_LOGICAL =
[0, 3, 1, 2]`). GUI callers (terminal, Notepad, File Manager, diskinfo)
needed no changes at all.

## Bugs found in shared code (fixed for both arches)

- **`make disk` produced FAT32, not FAT16.** `mformat -F` forces FAT32, so
  the kernel's FAT16 driver could never mount `oxide_disk.img` (root-dir
  LBA computed equal to data-start LBA — `fat_size_16 = 0` is the FAT32
  tell). The recipe now uses `mkfs.fat -F 16 -s 1`, giving 8192 512-byte
  clusters at 4 MB, comfortably above the 4085-cluster FAT16 minimum.
- **The record store corrupts FAT disks.** `/store` puts its header +
  255 record slots at LBA 2048–2303 — inside the FAT16 data region of the
  4 MB image. This was latent on x86 only because of the FAT32 bug above
  (FAT never mounted, so the store had the disk to itself).
  `disk_store::mount()` now refuses FAT disks with the same detection
  pattern it already used for ext2: boot-sector `0x55AA` plus the `FAT`
  type string at BPB offset `0x36` (FAT12/16) or `0x52` (FAT32).

## Testing

`main.rs` carries a serial-only smoke test behind
`STORAGE_BOOT_TEST: bool` (default `false`): it reads `/disk/bootcnt.txt`,
increments the number, writes it back. Two headless QEMU boots of the same
image logged `boot #1` then `boot #2`, and `mtype -i oxide_disk.img
::/bootcnt.txt` on the host shows the persisted count — write → power-off →
read proven end to end. The FAT guard logs
`[store] disk0 is a FAT filesystem — store disabled on it`, and
`make KARCH=x86_64` still builds (porting rule 3).

## Not yet done

- **ext2 on aarch64** — `fs/ext2.rs` stays stubbed; it follows the same
  facade once ext2 write support (plan M1) stabilises.
- **Interrupt-driven I/O** — polled until the GIC feature lands.
- **Multi-sector requests** — `read_sectors`/`write_sectors` loop over
  single-sector requests; a single chained request per batch is a easy
  later win.
