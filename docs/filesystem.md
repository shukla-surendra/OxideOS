# Filesystem — Design Notes

For the storage-stack walkthrough, see
[oxide_cocepts/06_storage_stack.md](oxide_cocepts/06_storage_stack.md) and
[disk_and_filesystem.md](disk_and_filesystem.md). This doc covers the *why*
behind the choices.

## A real VFS layer, not one hardcoded filesystem

`kernel/src/kernel/fs/vfs.rs` resolves every path to one of several
independent backends by prefix — RamFS (`/`), FAT16 (`/disk`), ext2
(`/ext2`), procfs (`/proc`), a custom record store (`/store`) — rather than
the kernel committing to a single on-disk format.

- RamFS exists first because it has zero hardware dependency (no disk
  needed to boot and get a working `/bin`, `/etc`, `/tmp`) and is trivially
  fast — most of the FHS-lite layout the OS needs at boot lives there.
- FAT16 was added before ext2 specifically because it's the simpler format
  to implement write support for (a flat cluster-chain table, fixed
  32-byte directory entries) — it proved out the open/create/write/mkdir/
  unlink pattern that ext2's bitmap-based allocator and variable-length
  directory entries later mirrored.
- Each backend's write path (block/cluster allocation, directory-entry
  insert/delete, growth-on-write) independently reimplements the same
  *shape* of logic rather than sharing an abstraction, because the on-disk
  geometry differs enough (FAT chains vs. ext2 bitmaps, fixed vs.
  variable-length dirents) that a shared abstraction would have been
  thinner than the duplication it replaced.

## ext2 write support: write-through, no block cache, no indirect blocks

The ext2 driver (`kernel/src/kernel/fs/ext2.rs`) writes every bitmap, inode,
and directory-entry change straight to disk synchronously — there's no
dirty-buffer cache. `sync`/`fsync` are correct no-ops as a direct
consequence: there's nothing buffered to flush.

- This was a deliberate scope cut, not an oversight. A block cache (Phase
  12.2) is valuable purely for performance (every read currently re-hits
  ATA PIO, ~1 ms/sector) — it doesn't change correctness, so it was left
  for later rather than entangling cache-invalidation logic with getting
  the on-disk format right first.
- Files are capped at 12 direct blocks (48 KB at 4 KB blocks) — no
  indirect/double-indirect block support. Writes or creates needing a
  13th block fail with `EFBIG`, a capability-limit error, deliberately
  distinct from `ENOSPC` (disk full) so userspace can tell the two apart.
  Indirect blocks are a contained, well-understood addition later; cutting
  them kept the bitmap-allocator-plus-directory-entry work (the actually
  novel part) reviewable on its own.

## Two subsystems silently claimed the same disk slot

`disk_store` (`/store`, a custom record-keyed store) and `ext2` both
default to mounting on "the secondary ATA disk" with no coordination
between them. `disk_store::mount()` used to format *any* disk that didn't
match its own magic header — including a disk that was actually a valid
ext2 filesystem, corrupting it.

- This is the kind of bug that stays latent for a long time: it only
  surfaces when both features are actually exercised against the same
  physical disk in the same boot, which most single-feature manual testing
  never does. The fix was to make `disk_store::mount()` check for the
  ext2 superblock magic (`0xEF53`) before formatting and refuse if found,
  rather than trying to give each subsystem a separate disk by convention
  (conventions silently violated are how this bug existed in the first
  place).
- A related, equally latent issue: the secondary-disk ATA wrappers
  (`ata::is_present_sec`/`read_sector_sec`/`write_sector_sec`) targeted
  "secondary master," which is exactly where QEMU's `-cdrom` boot device
  auto-attaches — meaning the secondary disk was unreachable in the
  *documented* boot configuration the whole time this code existed. Fixed
  by moving to secondary *slave* instead.

## Current limitations

- No symbolic links, hard links, or file locking on ext2 (separate, later
  plan phases).
- No per-process `/proc/PID/*` yet — procfs is system-wide only
  (`/proc/version`, `/proc/meminfo`, etc.).
- File permissions (uid/gid/mode) are stored but not enforced —
  `getuid()` always returns a fixed value, so there's no real multi-user
  access control yet.
