//! Read/write ext2 filesystem driver for OxideOS.
//!
//! Reads and writes the secondary IDE master disk (`ata::read_sector_sec`/
//! `write_sector_sec`). The partition offset (LBA of first block) is set
//! during `init()` either from the MBR partition table or by treating the
//! whole disk as ext2 (LBA 0). Block and inode allocation is bitmap-based
//! (one bitmap block per group); every alloc/free writes its bitmap, BGDT
//! entry, and superblock free-counts through immediately (no buffered dirty
//! state), so `sync`/`fsync` are correct no-ops.
//!
//! # Limitations
//! - 1024 / 2048 / 4096 byte blocks supported
//! - Inode direct blocks only (12 × block_size ≤ 48 KB per file) — no
//!   indirect, double-indirect, or triple-indirect blocks; writes/creates
//!   that would need a 13th block fail with `EFBIG`
//! - No symbolic links, hard links, or file locking
//! - Up to 8 block groups, each bitmap must fit in a single block
//! - Up to 16 simultaneously open files
//!
//! # Mount point
//! The VFS layer mounts this driver at `/ext2/`.

extern crate alloc;
use alloc::{string::String, vec::Vec};

use crate::kernel::ata;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::fs::{ENOENT, EEXIST, ENOSPC, EACCES, ENOTEMPTY, ENOTDIR, EFBIG,
                         O_CREAT, O_TRUNC, O_APPEND, O_WRONLY, O_RDWR};

// ── Constant limits ─────────────────────────────────────────────────────────
pub const EXT2_FD_BASE:  i32   = 80;
const     EXT2_FD_COUNT: usize = 16;
const     MAX_GROUPS:    usize = 8;
const     MAX_BLOCK:     usize = 4096; // max supported block size in bytes

// ── ext2 on-disk magic ──────────────────────────────────────────────────────
const EXT2_MAGIC: u16 = 0xEF53;

// Inode type bits in i_mode
const S_IFREG: u16 = 0x8000;
const S_IFDIR: u16 = 0x4000;

// Directory entry file_type byte
const FT_REG:  u8 = 1;
const FT_DIR:  u8 = 2;

// ── Block group descriptor (32 bytes) ───────────────────────────────────────
#[derive(Clone, Copy)]
struct BlockGroupDesc {
    block_bitmap: u32,
    inode_bitmap: u32,
    inode_table:  u32,
    free_blocks_count: u16,
    free_inodes_count: u16,
    used_dirs_count:   u16,
}

impl BlockGroupDesc {
    const fn zero() -> Self {
        Self {
            block_bitmap: 0, inode_bitmap: 0, inode_table: 0,
            free_blocks_count: 0, free_inodes_count: 0, used_dirs_count: 0,
        }
    }
}

// ── On-disk inode (128 bytes, revision 0 / revision 1 with 128-byte size) ──
#[derive(Clone, Copy)]
struct Inode {
    mode:        u16,
    uid:         u16,
    size_lo:     u32,
    atime:       u32,
    ctime:       u32,
    mtime:       u32,
    dtime:       u32,
    gid:         u16,
    links_count: u16,
    blocks_512:  u32,  // number of 512-byte blocks allocated
    flags:       u32,
    osd1:        u32,
    block:       [u32; 15], // 12 direct + 1 indirect + 1 double + 1 triple
    generation:  u32,
    file_acl:    u32,
    size_hi:     u32,
    faddr:       u32,
    // osd2 (12 bytes) ignored
}

impl Inode {
    fn zero() -> Self {
        Self {
            mode: 0, uid: 0, size_lo: 0, atime: 0, ctime: 0, mtime: 0, dtime: 0,
            gid: 0, links_count: 0, blocks_512: 0, flags: 0, osd1: 0,
            block: [0u32; 15], generation: 0, file_acl: 0, size_hi: 0, faddr: 0,
        }
    }

    fn is_dir(&self)  -> bool { self.mode & 0xF000 == S_IFDIR }
    fn is_file(&self) -> bool { self.mode & 0xF000 == S_IFREG }
    fn size(&self)    -> u64  { self.size_lo as u64 | ((self.size_hi as u64) << 32) }
}

// ── Open file descriptor ────────────────────────────────────────────────────
#[derive(Clone, Copy)]
struct Ext2Fd {
    active:      bool,
    inode_no:    u32,
    file_size:   u32,
    file_offset: u32,
    direct_blocks: [u32; 12],
    writable:    bool,
    append:      bool,
}

impl Ext2Fd {
    const fn empty() -> Self {
        Self {
            active: false, inode_no: 0, file_size: 0, file_offset: 0,
            direct_blocks: [0u32; 12], writable: false, append: false,
        }
    }
}

// ── Global driver state ─────────────────────────────────────────────────────
struct Ext2State {
    ready:             bool,
    lba_offset:        u32,   // partition start in 512-byte sectors
    block_size:        u32,   // bytes per block (1024, 2048, or 4096)
    sects_per_block:   u32,   // block_size / 512
    inodes_per_group:  u32,
    blocks_per_group:  u32,
    inode_size:        u32,   // bytes per inode (128 or larger in rev 1)
    first_data_block:  u32,   // 0 for block_size>1024, 1 for block_size==1024
    groups_count:      u32,
    bgdt:              [BlockGroupDesc; MAX_GROUPS],
    bgdt_block:        u32,   // block number of the BGDT (for write-back)
    sb_free_blocks:    u32,   // superblock free-block count (cached, write-through)
    sb_free_inodes:    u32,   // superblock free-inode count
    fds:               [Ext2Fd; EXT2_FD_COUNT],
}

impl Ext2State {
    const fn new() -> Self {
        Self {
            ready: false,
            lba_offset: 0,
            block_size: 1024,
            sects_per_block: 2,
            inodes_per_group: 0,
            blocks_per_group: 0,
            inode_size: 128,
            first_data_block: 1,
            groups_count: 0,
            bgdt: [const { BlockGroupDesc::zero() }; MAX_GROUPS],
            bgdt_block: 0,
            sb_free_blocks: 0,
            sb_free_inodes: 0,
            fds: [const { Ext2Fd::empty() }; EXT2_FD_COUNT],
        }
    }
}

pub static mut EXT2: Ext2State = Ext2State::new();

// ── Scratch buffer (avoids large stack allocations) ────────────────────────
static mut SCRATCH: [u8; MAX_BLOCK] = [0u8; MAX_BLOCK];

// ── Low-level block I/O ─────────────────────────────────────────────────────

/// Read one block (block_size bytes) into the static SCRATCH buffer.
/// Returns a pointer to `SCRATCH` on success, or a null-terminated error.
unsafe fn read_block_into_scratch(state: &Ext2State, block_no: u32) -> bool {
    let lba = state.lba_offset + block_no * state.sects_per_block;
    let scratch = &raw mut SCRATCH;
    let mut ok = true;
    for s in 0..state.sects_per_block {
        let sector_buf = unsafe {
            core::slice::from_raw_parts_mut(
                (scratch as *mut u8).add((s * 512) as usize),
                512,
            )
        };
        let mut buf512 = [0u8; 512];
        if !unsafe { ata::read_sector_sec(lba + s, &mut buf512) } {
            ok = false;
            break;
        }
        sector_buf.copy_from_slice(&buf512);
    }
    ok
}

/// Write `SCRATCH` back to block `block_no` (block_size bytes).
/// Returns false on I/O error.
unsafe fn write_block_from_scratch(state: &Ext2State, block_no: u32) -> bool {
    let lba = state.lba_offset + block_no * state.sects_per_block;
    let scratch = &raw const SCRATCH;
    for s in 0..state.sects_per_block {
        let sector_buf = unsafe {
            core::slice::from_raw_parts(
                (scratch as *const u8).add((s * 512) as usize),
                512,
            )
        };
        let mut buf512 = [0u8; 512];
        buf512.copy_from_slice(sector_buf);
        if !unsafe { ata::write_sector_sec(lba + s, &buf512) } {
            return false;
        }
    }
    true
}

/// Read a u32 from `SCRATCH` at byte `offset` (little-endian).
unsafe fn scratch_u32(offset: usize) -> u32 {
    let s = &raw const SCRATCH;
    u32::from_le_bytes([(*s)[offset], (*s)[offset+1], (*s)[offset+2], (*s)[offset+3]])
}

unsafe fn scratch_u16(offset: usize) -> u16 {
    let s = &raw const SCRATCH;
    u16::from_le_bytes([(*s)[offset], (*s)[offset+1]])
}

unsafe fn scratch_u8(offset: usize) -> u8 {
    let s = &raw const SCRATCH;
    (*s)[offset]
}

// ── Block/inode bitmap allocator ────────────────────────────────────────────
//
// Each block group has its own block bitmap and inode bitmap (one bit per
// block/inode in that group; bit set = in use). Both bitmaps are assumed to
// fit in a single block (enforced in `init()`), so each alloc/free touches
// exactly one bitmap block plus the BGDT and superblock free-count fields.

/// Map a global block number to (group index, bit index within that group).
fn block_to_group_bit(state: &Ext2State, block_no: u32) -> (usize, u32) {
    let rel = block_no - state.first_data_block;
    ((rel / state.blocks_per_group) as usize, rel % state.blocks_per_group)
}

/// Inverse of `block_to_group_bit`.
fn group_bit_to_block(state: &Ext2State, group: usize, bit: u32) -> u32 {
    state.first_data_block + group as u32 * state.blocks_per_group + bit
}

/// Map a 1-based inode number to (group index, bit index within that group).
fn inode_to_group_bit(state: &Ext2State, ino: u32) -> (usize, u32) {
    let idx = ino - 1;
    ((idx / state.inodes_per_group) as usize, idx % state.inodes_per_group)
}

/// Patch the free_blocks_count/free_inodes_count fields (BGDT offsets +12/+14)
/// of group `group`'s 32-byte descriptor entry. Returns false on I/O error.
unsafe fn write_bgdt_entry(state: &Ext2State, group: usize) -> bool {
    if !unsafe { read_block_into_scratch(state, state.bgdt_block) } { return false; }
    let scratch = &raw mut SCRATCH;
    let base = group * 32;
    let desc = &state.bgdt[group];
    unsafe {
        (&mut *scratch)[base+12..base+14].copy_from_slice(&desc.free_blocks_count.to_le_bytes());
        (&mut *scratch)[base+14..base+16].copy_from_slice(&desc.free_inodes_count.to_le_bytes());
        (&mut *scratch)[base+16..base+18].copy_from_slice(&desc.used_dirs_count.to_le_bytes());
    }
    unsafe { write_block_from_scratch(state, state.bgdt_block) }
}

/// Patch the superblock's free-block/free-inode counts (offsets 12/16 of the
/// 1024-byte superblock). The superblock always lives at byte offset 1024
/// from the partition start (sectors `lba_offset+2`/`+3`) regardless of
/// block size — it is NOT necessarily block-aligned (e.g. for 4096-byte
/// blocks it's the second quarter of block 0), so this uses the same raw
/// sector addressing as `init()`'s superblock read, not `read_block_into_scratch`.
unsafe fn write_superblock_free_counts(state: &Ext2State) -> bool {
    let sb_lba = state.lba_offset + 2;
    let mut sb0 = [0u8; 512];
    if !unsafe { ata::read_sector_sec(sb_lba, &mut sb0) } { return false; }
    sb0[12..16].copy_from_slice(&state.sb_free_blocks.to_le_bytes());
    sb0[16..20].copy_from_slice(&state.sb_free_inodes.to_le_bytes());
    unsafe { ata::write_sector_sec(sb_lba, &sb0) }
}

/// Allocate one free block, preferring `pref_group` for locality and falling
/// back to any group with free space. The returned block is zeroed before
/// being handed back (callers — file data and directory blocks — both
/// expect zeroed storage). Returns 0 on ENOSPC.
unsafe fn ext2_alloc_block(state: &mut Ext2State, pref_group: usize) -> u32 {
    let groups = state.groups_count as usize;
    let order = core::iter::once(pref_group).chain(0..groups);
    for g in order {
        if g >= groups || state.bgdt[g].free_blocks_count == 0 { continue; }
        let bitmap_block = state.bgdt[g].block_bitmap;
        if !unsafe { read_block_into_scratch(state, bitmap_block) } { continue; }

        let mut found_bit: Option<u32> = None;
        {
            let scratch = &raw mut SCRATCH;
            for bit in 0..state.blocks_per_group {
                let byte_idx = (bit / 8) as usize;
                if byte_idx >= state.block_size as usize { break; }
                let mask = 1u8 << (bit % 8);
                if unsafe { (&*scratch)[byte_idx] } & mask == 0 {
                    unsafe { (&mut *scratch)[byte_idx] |= mask; }
                    found_bit = Some(bit);
                    break;
                }
            }
        }

        let Some(bit) = found_bit else { continue };
        if !unsafe { write_block_from_scratch(state, bitmap_block) } { return 0; }

        state.bgdt[g].free_blocks_count -= 1;
        state.sb_free_blocks = state.sb_free_blocks.saturating_sub(1);
        if !unsafe { write_bgdt_entry(state, g) } { return 0; }
        if !unsafe { write_superblock_free_counts(state) } { return 0; }

        let block_no = group_bit_to_block(state, g, bit);
        let scratch = &raw mut SCRATCH;
        unsafe { (&mut *scratch).fill(0); }
        if !unsafe { write_block_from_scratch(state, block_no) } { return 0; }
        return block_no;
    }
    0 // ENOSPC
}

/// Free a previously allocated block (clear its bitmap bit, bump free counts).
unsafe fn ext2_free_block(state: &mut Ext2State, block_no: u32) -> bool {
    if block_no == 0 { return true; } // no-op
    let (g, bit) = block_to_group_bit(state, block_no);
    if g >= state.groups_count as usize { return false; }

    let bitmap_block = state.bgdt[g].block_bitmap;
    if !unsafe { read_block_into_scratch(state, bitmap_block) } { return false; }
    let byte_idx = (bit / 8) as usize;
    let scratch = &raw mut SCRATCH;
    unsafe { (&mut *scratch)[byte_idx] &= !(1u8 << (bit % 8)); }
    if !unsafe { write_block_from_scratch(state, bitmap_block) } { return false; }

    state.bgdt[g].free_blocks_count += 1;
    state.sb_free_blocks += 1;
    let bgdt_ok = unsafe { write_bgdt_entry(state, g) };
    let sb_ok   = unsafe { write_superblock_free_counts(state) };
    bgdt_ok && sb_ok
}

/// Allocate one free inode (1-based number), preferring `pref_group` for
/// locality. Bumps `used_dirs_count` when `is_dir` is set (ext2 tracks
/// directory counts per group; e2fsck flags a mismatch as corruption).
/// Returns 0 on ENOSPC. Does not write the inode's on-disk record — callers
/// must fill it in themselves.
unsafe fn ext2_alloc_inode(state: &mut Ext2State, pref_group: usize, is_dir: bool) -> u32 {
    let groups = state.groups_count as usize;
    let order = core::iter::once(pref_group).chain(0..groups);
    for g in order {
        if g >= groups || state.bgdt[g].free_inodes_count == 0 { continue; }
        let bitmap_block = state.bgdt[g].inode_bitmap;
        if !unsafe { read_block_into_scratch(state, bitmap_block) } { continue; }

        let mut found_bit: Option<u32> = None;
        {
            let scratch = &raw mut SCRATCH;
            for bit in 0..state.inodes_per_group {
                let byte_idx = (bit / 8) as usize;
                if byte_idx >= state.block_size as usize { break; }
                let mask = 1u8 << (bit % 8);
                if unsafe { (&*scratch)[byte_idx] } & mask == 0 {
                    unsafe { (&mut *scratch)[byte_idx] |= mask; }
                    found_bit = Some(bit);
                    break;
                }
            }
        }

        let Some(bit) = found_bit else { continue };
        if !unsafe { write_block_from_scratch(state, bitmap_block) } { return 0; }

        state.bgdt[g].free_inodes_count -= 1;
        state.sb_free_inodes = state.sb_free_inodes.saturating_sub(1);
        if is_dir { state.bgdt[g].used_dirs_count += 1; }
        if !unsafe { write_bgdt_entry(state, g) } { return 0; }
        if !unsafe { write_superblock_free_counts(state) } { return 0; }

        // 1-based global inode number: group*inodes_per_group + local_idx + 1.
        return g as u32 * state.inodes_per_group + bit + 1;
    }
    0 // ENOSPC
}

/// Free a previously allocated inode: clear its bitmap bit, zero its on-disk
/// mode/links_count (enough for `Inode::is_file`/`is_dir` to treat it as
/// gone), and bump free counts.
unsafe fn ext2_free_inode(state: &mut Ext2State, ino: u32, is_dir: bool) -> bool {
    if ino == 0 { return true; }
    let (g, bit) = inode_to_group_bit(state, ino);
    if g >= state.groups_count as usize { return false; }

    let bitmap_block = state.bgdt[g].inode_bitmap;
    if !unsafe { read_block_into_scratch(state, bitmap_block) } { return false; }
    let byte_idx = (bit / 8) as usize;
    let scratch = &raw mut SCRATCH;
    unsafe { (&mut *scratch)[byte_idx] &= !(1u8 << (bit % 8)); }
    if !unsafe { write_block_from_scratch(state, bitmap_block) } { return false; }

    // Zero the inode's on-disk mode + links_count so it reads back as dead.
    let idx = ino - 1;
    let inode_table_block = state.bgdt[g].inode_table;
    let byte_offset = idx % state.inodes_per_group * state.inode_size;
    let block_offset_in_table = byte_offset / state.block_size;
    let byte_in_block = (byte_offset % state.block_size) as usize;
    let block_no = inode_table_block + block_offset_in_table;
    if unsafe { read_block_into_scratch(state, block_no) } {
        let scratch = &raw mut SCRATCH;
        unsafe {
            (&mut *scratch)[byte_in_block..byte_in_block+2].fill(0);   // mode
            (&mut *scratch)[byte_in_block+4..byte_in_block+8].fill(0); // size_lo
            (&mut *scratch)[byte_in_block+26..byte_in_block+28].fill(0); // links_count
        }
        let _ = unsafe { write_block_from_scratch(state, block_no) };
    }

    state.bgdt[g].free_inodes_count += 1;
    state.sb_free_inodes += 1;
    if is_dir { state.bgdt[g].used_dirs_count = state.bgdt[g].used_dirs_count.saturating_sub(1); }
    let bgdt_ok = unsafe { write_bgdt_entry(state, g) };
    let sb_ok   = unsafe { write_superblock_free_counts(state) };
    bgdt_ok && sb_ok
}

// ── Inode reading ───────────────────────────────────────────────────────────

/// Read inode `ino` (1-based) into `out`.  Returns false on I/O error.
unsafe fn read_inode(state: &Ext2State, ino: u32, out: &mut Inode) -> bool {
    if ino == 0 { return false; }
    let idx           = ino - 1;
    let group         = (idx / state.inodes_per_group) as usize;
    let local_idx     = idx % state.inodes_per_group;
    if group >= state.groups_count as usize { return false; }

    let inode_table_block = state.bgdt[group].inode_table;
    let byte_offset = local_idx * state.inode_size;
    let block_offset_in_table = byte_offset / state.block_size;
    let byte_in_block         = (byte_offset % state.block_size) as usize;

    if !unsafe { read_block_into_scratch(state, inode_table_block + block_offset_in_table) } {
        return false;
    }

    let s = &raw const SCRATCH;
    let base = byte_in_block;

    out.mode        = u16::from_le_bytes([(*s)[base],   (*s)[base+1]]);
    out.uid         = u16::from_le_bytes([(*s)[base+2], (*s)[base+3]]);
    out.size_lo     = u32::from_le_bytes([(*s)[base+4], (*s)[base+5], (*s)[base+6], (*s)[base+7]]);
    // skip timestamps (bytes 8–27)
    out.links_count = u16::from_le_bytes([(*s)[base+26], (*s)[base+27]]);
    out.blocks_512  = u32::from_le_bytes([(*s)[base+28], (*s)[base+29], (*s)[base+30], (*s)[base+31]]);
    // skip flags, osd1 (bytes 32–39)
    // direct blocks at bytes 40–87 (12 × 4 bytes)
    for i in 0..15usize {
        let o = base + 40 + i * 4;
        out.block[i] = u32::from_le_bytes([(*s)[o], (*s)[o+1], (*s)[o+2], (*s)[o+3]]);
    }
    out.size_hi = u32::from_le_bytes([(*s)[base+108], (*s)[base+109], (*s)[base+110], (*s)[base+111]]);
    true
}

/// Patch the on-disk `size_lo` field of inode `ino` in place.
/// Returns false on I/O error.
unsafe fn update_inode_size(state: &Ext2State, ino: u32, new_size: u32) -> bool {
    if ino == 0 { return false; }
    let idx       = ino - 1;
    let group     = (idx / state.inodes_per_group) as usize;
    let local_idx = idx % state.inodes_per_group;
    if group >= state.groups_count as usize { return false; }

    let inode_table_block    = state.bgdt[group].inode_table;
    let byte_offset          = local_idx * state.inode_size;
    let block_offset_in_table = byte_offset / state.block_size;
    let byte_in_block        = (byte_offset % state.block_size) as usize;
    let block_no = inode_table_block + block_offset_in_table;

    if !unsafe { read_block_into_scratch(state, block_no) } { return false; }

    let scratch = &raw mut SCRATCH;
    let base = byte_in_block;
    let bytes = new_size.to_le_bytes();
    unsafe { (&mut *scratch)[base+4..base+8].copy_from_slice(&bytes); }

    unsafe { write_block_from_scratch(state, block_no) }
}

/// Compute (block_no, byte_in_block) for inode `ino`'s on-disk record.
fn inode_location(state: &Ext2State, ino: u32) -> Option<(u32, usize)> {
    if ino == 0 { return None; }
    let idx       = ino - 1;
    let group     = (idx / state.inodes_per_group) as usize;
    let local_idx = idx % state.inodes_per_group;
    if group >= state.groups_count as usize { return None; }
    let inode_table_block     = state.bgdt[group].inode_table;
    let byte_offset           = local_idx * state.inode_size;
    let block_offset_in_table = byte_offset / state.block_size;
    let byte_in_block         = (byte_offset % state.block_size) as usize;
    Some((inode_table_block + block_offset_in_table, byte_in_block))
}

/// Patch one direct block pointer (`block[idx]`, idx < 12) of inode `ino`.
unsafe fn update_inode_block_ptr(state: &Ext2State, ino: u32, idx: usize, block_no: u32) -> bool {
    let Some((blk, base)) = inode_location(state, ino) else { return false };
    if !unsafe { read_block_into_scratch(state, blk) } { return false; }
    let off = base + 40 + idx * 4;
    let scratch = &raw mut SCRATCH;
    unsafe { (&mut *scratch)[off..off+4].copy_from_slice(&block_no.to_le_bytes()); }
    unsafe { write_block_from_scratch(state, blk) }
}

/// Shrink or grow a file's logical size to `new_len` (must be <= 12 *
/// block_size — no indirect-block support). Shrinking frees any direct
/// blocks now fully beyond `new_len` (clearing both `direct_blocks[..]` and
/// the on-disk pointer); growing only patches the size field (sparse-hole
/// semantics — `read_fd` zero-fills the gap). Shared by O_TRUNC-on-open and
/// the `truncate`/`ftruncate` syscalls.
unsafe fn resize_blocks(state: &mut Ext2State, ino: u32, direct_blocks: &mut [u32], cur_size: &mut u32, new_len: u32) -> bool {
    let block_size = state.block_size;
    if new_len > 12 * block_size { return false; } // EFBIG
    if new_len < *cur_size {
        let keep = new_len.div_ceil(block_size) as usize;
        for bi in keep..12 {
            if direct_blocks[bi] != 0 {
                unsafe { ext2_free_block(state, direct_blocks[bi]); }
                direct_blocks[bi] = 0;
                if !unsafe { update_inode_block_ptr(state, ino, bi, 0) } { return false; }
            }
        }
    }
    *cur_size = new_len;
    unsafe { update_inode_size(state, ino, new_len) }
}

/// Patch atime/ctime/mtime (offsets 8/12/16) of inode `ino`. Real wall-clock
/// timestamps aren't wired up (no unix-epoch source in the kernel yet); this
/// is a placeholder that writes 0, matching the scope cutoff for this round.
unsafe fn update_inode_times(state: &Ext2State, ino: u32) -> bool {
    let Some((blk, base)) = inode_location(state, ino) else { return false };
    if !unsafe { read_block_into_scratch(state, blk) } { return false; }
    let scratch = &raw mut SCRATCH;
    unsafe { (&mut *scratch)[base+8..base+20].fill(0); } // atime, ctime, mtime
    unsafe { write_block_from_scratch(state, blk) }
}

/// Patch links_count (offset 26) of inode `ino`.
unsafe fn update_inode_links(state: &Ext2State, ino: u32, links_count: u16) -> bool {
    let Some((blk, base)) = inode_location(state, ino) else { return false };
    if !unsafe { read_block_into_scratch(state, blk) } { return false; }
    let scratch = &raw mut SCRATCH;
    unsafe { (&mut *scratch)[base+26..base+28].copy_from_slice(&links_count.to_le_bytes()); }
    unsafe { write_block_from_scratch(state, blk) }
}

/// Read links_count (offset 26) of inode `ino`. Returns 0 on I/O error.
unsafe fn read_inode_links(state: &Ext2State, ino: u32) -> u16 {
    let Some((blk, base)) = inode_location(state, ino) else { return 0 };
    if !unsafe { read_block_into_scratch(state, blk) } { return 0; }
    let s = &raw const SCRATCH;
    u16::from_le_bytes([unsafe { (*s)[base+26] }, unsafe { (*s)[base+27] }])
}

/// Zero out and initialize a freshly allocated inode's on-disk record:
/// mode, links_count, size, and all 15 block pointers. Used by `create()`
/// and `mkdir()` right after `ext2_alloc_inode`.
unsafe fn write_new_inode_record(state: &Ext2State, ino: u32, mode: u16, links_count: u16) -> bool {
    let Some((blk, base)) = inode_location(state, ino) else { return false };
    if !unsafe { read_block_into_scratch(state, blk) } { return false; }
    let scratch = &raw mut SCRATCH;
    unsafe {
        let s = &mut *scratch;
        s[base..base+128].fill(0);
        s[base..base+2].copy_from_slice(&mode.to_le_bytes());
        s[base+26..base+28].copy_from_slice(&links_count.to_le_bytes());
    }
    unsafe { write_block_from_scratch(state, blk) }
}

// ── Directory walking ───────────────────────────────────────────────────────

/// Search directory inode `dir_ino` for an entry named `name`.
/// Returns the inode number of the match, or 0 if not found.
unsafe fn dir_lookup(state: &Ext2State, dir_ino: u32, name: &[u8]) -> u32 {
    let mut dir_inode = Inode::zero();
    if !unsafe { read_inode(state, dir_ino, &mut dir_inode) } { return 0; }
    if !dir_inode.is_dir() { return 0; }

    let dir_size = dir_inode.size_lo as usize;
    let block_size = state.block_size as usize;
    let mut bytes_seen = 0usize;

    // Walk only direct blocks (12 × block_size max = 48 KB for 4096-byte blocks)
    'outer: for bi in 0..12usize {
        let blk = dir_inode.block[bi];
        if blk == 0 || bytes_seen >= dir_size { break; }

        if !unsafe { read_block_into_scratch(state, blk) } { break; }

        let s = &raw const SCRATCH;
        let mut pos = 0usize;
        while pos < block_size && bytes_seen + pos < dir_size {
            let ino   = u32::from_le_bytes([(*s)[pos], (*s)[pos+1], (*s)[pos+2], (*s)[pos+3]]);
            let reclen= u16::from_le_bytes([(*s)[pos+4], (*s)[pos+5]]) as usize;
            let namlen= (*s)[pos+6] as usize;
            if reclen == 0 { break; } // corrupt

            if ino != 0 && namlen == name.len() {
                let s_ref: &[u8] = &*s;
                let entry_name = &s_ref[pos+8..pos+8+namlen];
                if entry_name == name {
                    return ino;
                }
            }
            pos += reclen;
        }
        bytes_seen += block_size;
    }
    0
}

/// Resolve an absolute path (e.g. `/etc/passwd`) to an inode number.
/// Returns 0 if any component is not found.
unsafe fn lookup_path(state: &Ext2State, path: &[u8]) -> u32 {
    // Start at root inode (2)
    let mut cur_ino: u32 = 2;

    // Skip leading slashes
    let path = {
        let mut p = path;
        while !p.is_empty() && p[0] == b'/' { p = &p[1..]; }
        p
    };

    if path.is_empty() { return cur_ino; } // "/" → root

    for component in path.split(|&b| b == b'/') {
        if component.is_empty() { continue; } // trailing slash
        let next = unsafe { dir_lookup(state, cur_ino, component) };
        if next == 0 { return 0; }
        cur_ino = next;
    }
    cur_ino
}

/// Resolve the parent directory inode and final path component of `path`
/// (already stripped of the `/ext2` prefix). Returns `None` if any
/// component before the last doesn't exist, or `path` is empty/root.
unsafe fn resolve_parent<'a>(state: &Ext2State, path: &'a [u8]) -> Option<(u32, &'a [u8])> {
    let path = {
        let mut p = path;
        while !p.is_empty() && p[0] == b'/' { p = &p[1..]; }
        p
    };
    if path.is_empty() { return None; } // root has no parent/name

    let last_slash = path.iter().rposition(|&b| b == b'/');
    let (parent_part, name) = match last_slash {
        Some(i) => (&path[..i], &path[i+1..]),
        None    => (&path[..0], path),
    };
    if name.is_empty() || name.len() > 255 { return None; }

    let parent_ino = if parent_part.is_empty() { 2 } else { unsafe { lookup_path(state, parent_part) } };
    if parent_ino == 0 { return None; }
    Some((parent_ino, name))
}

/// Round a directory-entry size up to the ext2-required 4-byte alignment.
fn dirent_align4(n: usize) -> usize { (n + 3) & !3 }

/// Write a directory entry header+name at byte `pos` of the (already
/// loaded) SCRATCH buffer.
unsafe fn write_dir_entry_at(pos: usize, ino: u32, rec_len: u16, name: &[u8], file_type: u8) {
    let scratch = &raw mut SCRATCH;
    unsafe {
        let s = &mut *scratch;
        s[pos..pos+4].copy_from_slice(&ino.to_le_bytes());
        s[pos+4..pos+6].copy_from_slice(&rec_len.to_le_bytes());
        s[pos+6] = name.len() as u8;
        s[pos+7] = file_type;
        s[pos+8..pos+8+name.len()].copy_from_slice(name);
    }
}

/// Insert a new `(name -> new_ino)` entry into directory `dir_ino`'s data.
/// Reuses a zero-`ino` tombstone slot if one is big enough, else splits an
/// oversized live entry's `rec_len`, else extends the directory with a new
/// allocated+zeroed block (fails with `false` if all 12 direct slots are
/// already used — no indirect-block support).
unsafe fn dir_insert_entry(state: &mut Ext2State, dir_ino: u32, name: &[u8], new_ino: u32, file_type: u8) -> bool {
    if name.is_empty() || name.len() > 255 { return false; }
    let needed = dirent_align4(8 + name.len());

    let mut dir_inode = Inode::zero();
    if !unsafe { read_inode(state, dir_ino, &mut dir_inode) } { return false; }
    if !dir_inode.is_dir() { return false; }

    let dir_size = dir_inode.size_lo as usize;
    let block_size = state.block_size as usize;
    let mut bytes_seen = 0usize;

    for bi in 0..12usize {
        let blk = dir_inode.block[bi];
        if blk == 0 || bytes_seen >= dir_size { break; }
        if !unsafe { read_block_into_scratch(state, blk) } { return false; }

        let mut pos = 0usize;
        let mut done = false;
        while pos < block_size {
            let s = &raw const SCRATCH;
            let ino    = unsafe { u32::from_le_bytes([(*s)[pos], (*s)[pos+1], (*s)[pos+2], (*s)[pos+3]]) };
            let reclen = unsafe { u16::from_le_bytes([(*s)[pos+4], (*s)[pos+5]]) } as usize;
            let namlen = unsafe { (*s)[pos+6] } as usize;
            if reclen == 0 || pos + reclen > block_size { break; }

            if ino == 0 && reclen >= needed {
                unsafe { write_dir_entry_at(pos, new_ino, reclen as u16, name, file_type); }
                done = true;
                break;
            }
            let ideal = dirent_align4(8 + namlen);
            if ino != 0 && reclen >= ideal + needed {
                let slack = reclen - ideal;
                let s = &raw mut SCRATCH;
                unsafe { (&mut *s)[pos+4..pos+6].copy_from_slice(&(ideal as u16).to_le_bytes()); }
                unsafe { write_dir_entry_at(pos + ideal, new_ino, slack as u16, name, file_type); }
                done = true;
                break;
            }
            pos += reclen;
        }

        if done {
            return unsafe { write_block_from_scratch(state, blk) };
        }
        bytes_seen += block_size;
    }

    // No room in any existing block — extend with a new one.
    let free_slot = (0..12usize).find(|&bi| dir_inode.block[bi] == 0);
    let Some(bi) = free_slot else { return false }; // EFBIG: all direct blocks used

    let pref_group = ((dir_ino - 1) / state.inodes_per_group) as usize;
    let new_block = unsafe { ext2_alloc_block(state, pref_group) };
    if new_block == 0 { return false; } // ENOSPC

    if !unsafe { read_block_into_scratch(state, new_block) } { return false; }
    unsafe { write_dir_entry_at(0, new_ino, block_size as u16, name, file_type); }
    if !unsafe { write_block_from_scratch(state, new_block) } { return false; }

    if !unsafe { update_inode_block_ptr(state, dir_ino, bi, new_block) } { return false; }
    unsafe { update_inode_size(state, dir_ino, dir_size as u32 + block_size as u32) }
}

/// Mark the entry named `name` in `dir_ino`'s data as deleted (zero its
/// `ino` field — `dir_lookup`/`list_dir_raw` already skip `ino == 0`
/// entries). Leaves a tombstone slot that `dir_insert_entry` can reuse.
unsafe fn dir_delete_entry(state: &Ext2State, dir_ino: u32, name: &[u8]) -> bool {
    let mut dir_inode = Inode::zero();
    if !unsafe { read_inode(state, dir_ino, &mut dir_inode) } { return false; }
    if !dir_inode.is_dir() { return false; }

    let dir_size = dir_inode.size_lo as usize;
    let block_size = state.block_size as usize;
    let mut bytes_seen = 0usize;

    for bi in 0..12usize {
        let blk = dir_inode.block[bi];
        if blk == 0 || bytes_seen >= dir_size { break; }
        if !unsafe { read_block_into_scratch(state, blk) } { return false; }

        let mut pos = 0usize;
        while pos < block_size && bytes_seen + pos < dir_size {
            let s = &raw const SCRATCH;
            let ino    = unsafe { u32::from_le_bytes([(*s)[pos], (*s)[pos+1], (*s)[pos+2], (*s)[pos+3]]) };
            let reclen = unsafe { u16::from_le_bytes([(*s)[pos+4], (*s)[pos+5]]) } as usize;
            let namlen = unsafe { (*s)[pos+6] } as usize;
            if reclen == 0 { break; }

            if ino != 0 && namlen == name.len() {
                let s_ref: &[u8] = unsafe { &*s };
                let matches = &s_ref[pos+8..pos+8+namlen] == name;
                if matches {
                    let s_mut = &raw mut SCRATCH;
                    unsafe { (&mut *s_mut)[pos..pos+4].fill(0); }
                    return unsafe { write_block_from_scratch(state, blk) };
                }
            }
            pos += reclen;
        }
        bytes_seen += block_size;
    }
    false
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Initialise the ext2 driver.
///
/// Uses the secondary IDE master; if it is absent or the superblock is invalid,
/// the driver stays in `!ready` state (all operations return errors).
///
/// `partition_lba`: LBA of the partition start (0 = whole-disk ext2).
pub unsafe fn init(partition_lba: u32) {
    if !ata::is_present_sec() {
        unsafe { SERIAL_PORT.write_str("ext2: no secondary disk\n"); }
        return;
    }

    let state = &raw mut EXT2;

    // Superblock is at byte offset 1024 from partition start.
    // For 512-byte sectors: LBA 2 relative to partition start.
    let sb_lba = partition_lba + 2;
    let mut sb0 = [0u8; 512];
    let mut sb1 = [0u8; 512];
    if !unsafe { ata::read_sector_sec(sb_lba,     &mut sb0) } { return; }
    if !unsafe { ata::read_sector_sec(sb_lba + 1, &mut sb1) } { return; }

    // Combine two sectors → 1024-byte superblock in SCRATCH.
    let scratch = &raw mut SCRATCH;
    unsafe {
        let scratch_ref: &mut [u8] = &mut *scratch;
        scratch_ref[..512].copy_from_slice(&sb0);
        scratch_ref[512..1024].copy_from_slice(&sb1);
    }

    // Validate magic at offset 56 (within the 1024-byte superblock).
    let magic = u16::from_le_bytes([unsafe { (*scratch)[56] }, unsafe { (*scratch)[57] }]);
    if magic != EXT2_MAGIC {
        unsafe {
            SERIAL_PORT.write_str("ext2: bad magic 0x");
            SERIAL_PORT.write_hex(magic as u32);
            SERIAL_PORT.write_str("\n");
        }
        return;
    }

    let s = &raw const SCRATCH;
    let inodes_count   = u32::from_le_bytes([(*s)[0],  (*s)[1],  (*s)[2],  (*s)[3]]);
    let blocks_count   = u32::from_le_bytes([(*s)[4],  (*s)[5],  (*s)[6],  (*s)[7]]);
    let free_blocks    = u32::from_le_bytes([(*s)[12], (*s)[13], (*s)[14], (*s)[15]]);
    let free_inodes    = u32::from_le_bytes([(*s)[16], (*s)[17], (*s)[18], (*s)[19]]);
    let first_data_blk = u32::from_le_bytes([(*s)[20], (*s)[21], (*s)[22], (*s)[23]]);
    let log_block_size = u32::from_le_bytes([(*s)[24], (*s)[25], (*s)[26], (*s)[27]]);
    let blocks_per_grp = u32::from_le_bytes([(*s)[32], (*s)[33], (*s)[34], (*s)[35]]);
    let inodes_per_grp = u32::from_le_bytes([(*s)[40], (*s)[41], (*s)[42], (*s)[43]]);
    let rev_level      = u32::from_le_bytes([(*s)[76], (*s)[77], (*s)[78], (*s)[79]]);
    let inode_size = if rev_level >= 1 {
        u16::from_le_bytes([(*s)[88], (*s)[89]]) as u32
    } else {
        128
    };

    let block_size = 1024u32 << log_block_size;
    let sects_per_block = block_size / 512;

    if block_size > MAX_BLOCK as u32 {
        unsafe { SERIAL_PORT.write_str("ext2: block size too large\n"); }
        return;
    }

    // The allocator only reads a single bitmap block per group; bail out
    // (mount read-only-equivalent, no writes possible) rather than risk
    // mis-addressing a bitmap that spans multiple blocks.
    if blocks_per_grp > block_size * 8 || inodes_per_grp > block_size * 8 {
        unsafe { SERIAL_PORT.write_str("ext2: bitmap spans multiple blocks, unsupported\n"); }
        return;
    }

    let groups_count = blocks_count.div_ceil(blocks_per_grp).min(MAX_GROUPS as u32);

    unsafe {
        (*state).lba_offset       = partition_lba;
        (*state).block_size       = block_size;
        (*state).sects_per_block  = sects_per_block;
        (*state).inodes_per_group = inodes_per_grp;
        (*state).blocks_per_group = blocks_per_grp;
        (*state).inode_size       = inode_size;
        (*state).first_data_block = first_data_blk;
        (*state).groups_count     = groups_count;
        (*state).sb_free_blocks   = free_blocks;
        (*state).sb_free_inodes   = free_inodes;
    }

    // Block group descriptor table (BGDT) starts immediately after the superblock block.
    // For 1024-byte blocks: superblock is in block 1, BGDT starts at block 2.
    // For larger blocks: superblock is in block 0 (with the boot record), BGDT at block 1.
    let bgdt_block = first_data_blk + 1;
    // Each BGDT entry is 32 bytes; read the first block of the BGDT.
    if !unsafe { read_block_into_scratch(&*state, bgdt_block) } {
        unsafe { SERIAL_PORT.write_str("ext2: failed to read BGDT\n"); }
        return;
    }
    unsafe { (*state).bgdt_block = bgdt_block; }

    let s = &raw const SCRATCH;
    for g in 0..groups_count as usize {
        let base = g * 32;
        unsafe {
            (*state).bgdt[g] = BlockGroupDesc {
                block_bitmap: u32::from_le_bytes([(*s)[base],   (*s)[base+1], (*s)[base+2], (*s)[base+3]]),
                inode_bitmap: u32::from_le_bytes([(*s)[base+4], (*s)[base+5], (*s)[base+6], (*s)[base+7]]),
                inode_table:  u32::from_le_bytes([(*s)[base+8], (*s)[base+9], (*s)[base+10],(*s)[base+11]]),
                free_blocks_count: u16::from_le_bytes([(*s)[base+12], (*s)[base+13]]),
                free_inodes_count: u16::from_le_bytes([(*s)[base+14], (*s)[base+15]]),
                used_dirs_count:   u16::from_le_bytes([(*s)[base+16], (*s)[base+17]]),
            };
        }
    }

    unsafe { (*state).ready = true; }

    unsafe {
        SERIAL_PORT.write_str("ext2: mounted, block_size=");
        SERIAL_PORT.write_decimal(block_size);
        SERIAL_PORT.write_str(" groups=");
        SERIAL_PORT.write_decimal(groups_count);
        SERIAL_PORT.write_str(" inodes=");
        SERIAL_PORT.write_decimal(inodes_count);
        SERIAL_PORT.write_str(" free_blocks=");
        SERIAL_PORT.write_decimal(free_blocks);
        SERIAL_PORT.write_str(" free_inodes=");
        SERIAL_PORT.write_decimal(free_inodes);
        SERIAL_PORT.write_str(" (rw)\n");
    }
}

pub fn is_ready() -> bool { unsafe { EXT2.ready } }

/// Returns `true` if `fd` is in the ext2 raw FD range.
pub fn is_ext2_fd(fd: i32) -> bool {
    fd >= EXT2_FD_BASE && fd < EXT2_FD_BASE + EXT2_FD_COUNT as i32
}

/// Create a new regular file at `path` (already stripped of the `/ext2`
/// prefix — callers pass a root-relative path). Returns the new inode
/// number (> 0) on success, or a negative error.
unsafe fn create(path: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return ENOENT; }

    let Some((parent_ino, name)) = (unsafe { resolve_parent(&*state, path) }) else { return ENOENT; };
    if unsafe { dir_lookup(&*state, parent_ino, name) } != 0 { return EEXIST; }

    let pref_group = ((parent_ino - 1) / (*state).inodes_per_group) as usize;
    let ino = unsafe { ext2_alloc_inode(&mut *state, pref_group, false) };
    if ino == 0 { return ENOSPC; }

    if !unsafe { write_new_inode_record(&*state, ino, S_IFREG | 0o644, 1) } {
        unsafe { ext2_free_inode(&mut *state, ino, false); }
        return -5; // EIO
    }

    if !unsafe { dir_insert_entry(&mut *state, parent_ino, name, ino, FT_REG) } {
        unsafe { ext2_free_inode(&mut *state, ino, false); }
        return ENOSPC;
    }

    ino as i64
}

/// Open a file at `path` (absolute, e.g. `/etc/passwd`).
/// Returns a raw ext2 FD (≥80) on success, or a negative error.
///
/// Supports `O_CREAT` (create if missing), `O_TRUNC` (truncate an existing
/// file to 0 on open), and `O_APPEND` (seed the file offset at EOF).
pub unsafe fn open(path: &[u8], flags: u32) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return ENOENT; }

    // Strip optional `/ext2` prefix from path (VFS passes the full path).
    let path = strip_ext2_prefix(path);

    let mut ino = unsafe { lookup_path(&*state, path) };
    if ino == 0 {
        if flags & O_CREAT == 0 { return ENOENT; }
        let created = unsafe { create(path) };
        if created <= 0 { return created.min(-1); }
        ino = created as u32;
    }

    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, ino, &mut inode) } { return -1; }
    if !inode.is_file() { return -21; } // EISDIR or not-a-file

    if flags & O_TRUNC != 0 {
        let mut size = inode.size_lo;
        if !unsafe { resize_blocks(&mut *state, ino, &mut inode.block[..12], &mut size, 0) } {
            return -5; // EIO
        }
        inode.size_lo = size;
        inode.block[..12].fill(0);
    }

    let writable = (flags & O_WRONLY != 0) || (flags & O_RDWR != 0);

    // Allocate FD slot.
    let fds = &raw mut (*state).fds;
    for i in 0..EXT2_FD_COUNT {
        let slot = &raw mut (*fds)[i];
        if !(*slot).active {
            (*slot).active      = true;
            (*slot).inode_no    = ino;
            (*slot).file_size   = inode.size_lo;
            (*slot).file_offset = if flags & O_APPEND != 0 { inode.size_lo } else { 0 };
            (*slot).direct_blocks.copy_from_slice(&inode.block[..12]);
            (*slot).writable    = writable;
            (*slot).append      = flags & O_APPEND != 0;
            return (EXT2_FD_BASE + i as i32) as i64;
        }
    }
    -24 // EMFILE
}

/// Read up to `buf.len()` bytes from an open ext2 FD.  Returns bytes read.
pub unsafe fn read_fd(fd: i32, buf: &mut [u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready || !is_ext2_fd(fd) { return -5; }
    let idx = (fd - EXT2_FD_BASE) as usize;
    let fds = &raw mut (*state).fds;
    let slot = &raw mut (*fds)[idx];
    if !(*slot).active { return -5; }

    let remaining = (*slot).file_size.saturating_sub((*slot).file_offset) as usize;
    if remaining == 0 { return 0; }

    let to_read = buf.len().min(remaining);
    let block_size = (*state).block_size as usize;
    let mut done = 0usize;

    while done < to_read {
        let file_offset = (*slot).file_offset as usize + done;
        let block_idx   = file_offset / block_size;
        let byte_in_blk = file_offset % block_size;

        if block_idx >= 12 { break; } // no indirect block support yet
        let blk = (*slot).direct_blocks[block_idx];
        if blk == 0 {
            // Sparse hole (e.g. left by a truncate-grow): zero-fill rather
            // than stopping, since file_offset is still < file_size here.
            let avail = (block_size - byte_in_blk).min(to_read - done);
            buf[done..done + avail].fill(0);
            done += avail;
            continue;
        }

        if !unsafe { read_block_into_scratch(&*state, blk) } { break; }

        let avail = (block_size - byte_in_blk).min(to_read - done);
        let s = &raw const SCRATCH;
        for i in 0..avail {
            buf[done + i] = (*s)[byte_in_blk + i];
        }
        done += avail;
    }

    (*slot).file_offset += done as u32;
    done as i64
}

/// Write up to `buf.len()` bytes to an open ext2 FD at the current file offset.
///
/// Allocates new direct blocks on demand as the write grows past the
/// current allocation. Once `block_idx` would reach 12 (no indirect-block
/// support), the write stops there; if nothing could be written at all in
/// that case, returns `EFBIG` (a capability cap, not disk-full).
pub unsafe fn write_fd(fd: i32, buf: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready || !is_ext2_fd(fd) { return -5; }
    let idx = (fd - EXT2_FD_BASE) as usize;
    let fds = &raw mut (*state).fds;
    let slot = &raw mut (*fds)[idx];
    if !(*slot).active { return -5; }
    if !(*slot).writable { return EACCES; }

    let block_size = (*state).block_size as usize;
    let pref_group = (((*slot).inode_no - 1) / (*state).inodes_per_group) as usize;
    let mut done = 0usize;
    let mut hit_efbig = false;

    while done < buf.len() {
        let file_offset = (*slot).file_offset as usize + done;
        let block_idx   = file_offset / block_size;
        let byte_in_blk = file_offset % block_size;

        if block_idx >= 12 { hit_efbig = true; break; } // no indirect block support

        let mut blk = (*slot).direct_blocks[block_idx];
        if blk == 0 {
            blk = unsafe { ext2_alloc_block(&mut *state, pref_group) };
            if blk == 0 { break; } // ENOSPC
            if !unsafe { update_inode_block_ptr(&*state, (*slot).inode_no, block_idx, blk) } { break; }
            (*slot).direct_blocks[block_idx] = blk;
        }

        if !unsafe { read_block_into_scratch(&*state, blk) } { break; }

        let avail = (block_size - byte_in_blk).min(buf.len() - done);
        let scratch = &raw mut SCRATCH;
        unsafe {
            (&mut *scratch)[byte_in_blk..byte_in_blk + avail]
                .copy_from_slice(&buf[done..done + avail]);
        }

        if !unsafe { write_block_from_scratch(&*state, blk) } { break; }

        done += avail;
    }

    if done == 0 {
        if buf.is_empty() { return 0; }
        return if hit_efbig { EFBIG } else { ENOSPC };
    }

    (*slot).file_offset += done as u32;
    if (*slot).file_offset > (*slot).file_size {
        (*slot).file_size = (*slot).file_offset;
        unsafe { update_inode_size(&*state, (*slot).inode_no, (*slot).file_size); }
    }
    unsafe { update_inode_times(&*state, (*slot).inode_no); }

    done as i64
}

/// Close an open ext2 FD.
pub unsafe fn close(fd: i32) -> i64 {
    let state = &raw mut EXT2;
    if !is_ext2_fd(fd) { return -5; }
    let idx = (fd - EXT2_FD_BASE) as usize;
    unsafe { (*state).fds[idx].active = false; }
    0
}

/// List directory entries at `path` into `out` as `<name>\n` lines.
/// Directories are suffixed with `/`.  Returns bytes written.
pub unsafe fn list_dir_raw(path: &[u8], out: &mut [u8]) -> i64 {
    let state = &raw const EXT2;
    if !(*state).ready { return -2; }

    let path = strip_ext2_prefix(path);
    let dir_ino = unsafe { lookup_path(&*state, path) };
    if dir_ino == 0 { return -7; }

    let mut dir_inode = Inode::zero();
    if !unsafe { read_inode(&*state, dir_ino, &mut dir_inode) } { return -1; }
    if !dir_inode.is_dir() { return -20; } // ENOTDIR

    let dir_size = dir_inode.size_lo as usize;
    let block_size = (*state).block_size as usize;
    let mut written = 0usize;
    let mut bytes_seen = 0usize;

    'outer: for bi in 0..12usize {
        let blk = dir_inode.block[bi];
        if blk == 0 || bytes_seen >= dir_size { break; }

        if !unsafe { read_block_into_scratch(&*state, blk) } { break; }

        let s = &raw const SCRATCH;
        let mut pos = 0usize;
        while pos < block_size && bytes_seen + pos < dir_size {
            let ino    = u32::from_le_bytes([(*s)[pos], (*s)[pos+1], (*s)[pos+2], (*s)[pos+3]]);
            let reclen = u16::from_le_bytes([(*s)[pos+4], (*s)[pos+5]]) as usize;
            let namlen = (*s)[pos+6] as usize;
            let ftype  = (*s)[pos+7];
            if reclen == 0 { break; }

            if ino != 0 {
                let s_ref: &[u8] = unsafe { &*s };
                let name = &s_ref[pos+8..pos+8+namlen];
                // Skip . and ..
                if name != b"." && name != b".." {
                    let n = namlen.min(out.len().saturating_sub(written + 2));
                    if n == 0 { break 'outer; }
                    out[written..written+n].copy_from_slice(&name[..n]);
                    written += n;
                    if ftype == FT_DIR && written < out.len() {
                        out[written] = b'/';
                        written += 1;
                    }
                    if written < out.len() {
                        out[written] = b'\n';
                        written += 1;
                    }
                }
            }
            pos += reclen;
        }
        bytes_seen += block_size;
    }
    written as i64
}

/// Check whether `path` is a directory (for chdir validation).
pub unsafe fn is_dir(path: &[u8]) -> bool {
    let state = &raw const EXT2;
    if !(*state).ready { return false; }
    let path = strip_ext2_prefix(path);
    let ino = unsafe { lookup_path(&*state, path) };
    if ino == 0 { return false; }
    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, ino, &mut inode) } { return false; }
    inode.is_dir()
}

/// Create a directory at `path`. Returns 0 on success, or a negative error.
pub unsafe fn mkdir(path: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return ENOENT; }
    let path = strip_ext2_prefix(path);

    let Some((parent_ino, name)) = (unsafe { resolve_parent(&*state, path) }) else { return ENOENT; };
    if unsafe { dir_lookup(&*state, parent_ino, name) } != 0 { return EEXIST; }

    let pref_group = ((parent_ino - 1) / (*state).inodes_per_group) as usize;
    let new_ino = unsafe { ext2_alloc_inode(&mut *state, pref_group, true) };
    if new_ino == 0 { return ENOSPC; }

    let new_block = unsafe { ext2_alloc_block(&mut *state, pref_group) };
    if new_block == 0 {
        unsafe { ext2_free_inode(&mut *state, new_ino, true); }
        return ENOSPC;
    }

    // Write `.` (rec_len=12) and `..` (spans to end of block, splittable
    // later when files are added to this new directory).
    let block_size = (*state).block_size as usize;
    if !unsafe { read_block_into_scratch(&*state, new_block) } { return -5; }
    unsafe { write_dir_entry_at(0, new_ino, 12, b".", FT_DIR); }
    unsafe { write_dir_entry_at(12, parent_ino, (block_size - 12) as u16, b"..", FT_DIR); }
    if !unsafe { write_block_from_scratch(&*state, new_block) } { return -5; }

    if !unsafe { write_new_inode_record(&*state, new_ino, S_IFDIR | 0o755, 2) } { return -5; }
    if !unsafe { update_inode_block_ptr(&*state, new_ino, 0, new_block) } { return -5; }
    if !unsafe { update_inode_size(&*state, new_ino, block_size as u32) } { return -5; }

    if !unsafe { dir_insert_entry(&mut *state, parent_ino, name, new_ino, FT_DIR) } {
        unsafe { ext2_free_block(&mut *state, new_block); }
        unsafe { ext2_free_inode(&mut *state, new_ino, true); }
        return ENOSPC;
    }

    // Each subdirectory's `..` points back at the parent, so the parent's
    // link count goes up by one too.
    let parent_links = unsafe { read_inode_links(&*state, parent_ino) };
    unsafe { update_inode_links(&*state, parent_ino, parent_links + 1); }

    0
}

/// True if directory `dir_ino` has no live entries besides `.`/`..`.
unsafe fn dir_is_empty(state: &Ext2State, dir_ino: u32) -> bool {
    let mut dir_inode = Inode::zero();
    if !unsafe { read_inode(state, dir_ino, &mut dir_inode) } { return false; }
    if !dir_inode.is_dir() { return false; }

    let dir_size = dir_inode.size_lo as usize;
    let block_size = state.block_size as usize;
    let mut bytes_seen = 0usize;

    for bi in 0..12usize {
        let blk = dir_inode.block[bi];
        if blk == 0 || bytes_seen >= dir_size { break; }
        if !unsafe { read_block_into_scratch(state, blk) } { return false; }

        let s = &raw const SCRATCH;
        let mut pos = 0usize;
        while pos < block_size && bytes_seen + pos < dir_size {
            let ino    = unsafe { u32::from_le_bytes([(*s)[pos], (*s)[pos+1], (*s)[pos+2], (*s)[pos+3]]) };
            let reclen = unsafe { u16::from_le_bytes([(*s)[pos+4], (*s)[pos+5]]) } as usize;
            let namlen = unsafe { (*s)[pos+6] } as usize;
            if reclen == 0 { break; }
            if ino != 0 {
                let s_ref: &[u8] = unsafe { &*s };
                let name = &s_ref[pos+8..pos+8+namlen];
                if name != b"." && name != b".." { return false; }
            }
            pos += reclen;
        }
        bytes_seen += block_size;
    }
    true
}

/// Remove a file, or an empty directory, at `path`. Serves both `unlink`
/// and the rmdir-on-empty-dir case (mirrors `fat::unlink`'s dual role).
pub unsafe fn unlink(path: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return ENOENT; }
    let path = strip_ext2_prefix(path);

    let Some((parent_ino, name)) = (unsafe { resolve_parent(&*state, path) }) else { return ENOENT; };
    let target_ino = unsafe { dir_lookup(&*state, parent_ino, name) };
    if target_ino == 0 { return ENOENT; }

    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, target_ino, &mut inode) } { return -5; }

    if inode.is_dir() {
        if !unsafe { dir_is_empty(&*state, target_ino) } { return ENOTEMPTY; }
        for bi in 0..12usize {
            if inode.block[bi] != 0 { unsafe { ext2_free_block(&mut *state, inode.block[bi]); } }
        }
        unsafe { ext2_free_inode(&mut *state, target_ino, true); }
        unsafe { dir_delete_entry(&*state, parent_ino, name); }
        let parent_links = unsafe { read_inode_links(&*state, parent_ino) };
        unsafe { update_inode_links(&*state, parent_ino, parent_links.saturating_sub(1)); }
    } else {
        let links = unsafe { read_inode_links(&*state, target_ino) };
        let new_links = links.saturating_sub(1);
        if new_links == 0 {
            for bi in 0..12usize {
                if inode.block[bi] != 0 { unsafe { ext2_free_block(&mut *state, inode.block[bi]); } }
            }
            unsafe { ext2_free_inode(&mut *state, target_ino, false); }
        } else {
            unsafe { update_inode_links(&*state, target_ino, new_links); }
        }
        unsafe { dir_delete_entry(&*state, parent_ino, name); }
    }
    0
}

/// Remove an empty directory at `path`. Thin wrapper over `unlink` that
/// adds an `ENOTDIR` guard so misuse (rmdir on a file) fails correctly.
pub unsafe fn rmdir(path: &[u8]) -> i64 {
    let state = &raw const EXT2;
    if !(*state).ready { return ENOENT; }
    let stripped = strip_ext2_prefix(path);
    let ino = unsafe { lookup_path(&*state, stripped) };
    if ino == 0 { return ENOENT; }
    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, ino, &mut inode) } { return -5; }
    if !inode.is_dir() { return ENOTDIR; }
    unsafe { unlink(path) }
}

/// Truncate/extend an open ext2 fd to `length` bytes. Shrinking frees
/// direct blocks beyond the new length; growing only patches the size
/// field (sparse-hole semantics — `read_fd` zero-fills the gap).
pub unsafe fn truncate(fd: i32, length: u32) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready || !is_ext2_fd(fd) { return -5; }
    let idx = (fd - EXT2_FD_BASE) as usize;
    let fds = &raw mut (*state).fds;
    let slot = &raw mut (*fds)[idx];
    if !(*slot).active { return -5; }
    if !(*slot).writable { return EACCES; }

    if length > 12 * (*state).block_size { return EFBIG; }

    let mut size = (*slot).file_size;
    let mut blocks = (*slot).direct_blocks;
    if !unsafe { resize_blocks(&mut *state, (*slot).inode_no, &mut blocks, &mut size, length) } {
        return -5;
    }
    (*slot).direct_blocks = blocks;
    (*slot).file_size = size;
    0
}

/// Move/rename `old_path` to `new_path` (same or cross directory). Always
/// inserts the new entry then deletes the old one (ext2's length-prefixed
/// directory entries make FAT's in-place name-byte-rewrite trick unsafe for
/// differently-sized names). Fixes up the moved entry's `..` and both
/// parents' link counts when moving a directory across parents.
pub unsafe fn rename(old_path: &[u8], new_path: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return ENOENT; }
    let old = strip_ext2_prefix(old_path);
    let new = strip_ext2_prefix(new_path);

    let Some((old_parent, old_name)) = (unsafe { resolve_parent(&*state, old) }) else { return ENOENT; };
    let target_ino = unsafe { dir_lookup(&*state, old_parent, old_name) };
    if target_ino == 0 { return ENOENT; }

    let Some((new_parent, new_name)) = (unsafe { resolve_parent(&*state, new) }) else { return ENOENT; };
    if old_parent == new_parent && old_name == new_name { return 0; } // no-op

    if unsafe { dir_lookup(&*state, new_parent, new_name) } != 0 { return EEXIST; }

    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, target_ino, &mut inode) } { return -5; }
    let file_type = if inode.is_dir() { FT_DIR } else { FT_REG };

    if !unsafe { dir_insert_entry(&mut *state, new_parent, new_name, target_ino, file_type) } {
        return ENOSPC;
    }
    unsafe { dir_delete_entry(&*state, old_parent, old_name); }

    if inode.is_dir() && old_parent != new_parent {
        // Patch the moved directory's `..` entry (always at byte offset 12
        // of its first data block, per `mkdir`'s fixed layout).
        let blk = inode.block[0];
        if blk != 0 && unsafe { read_block_into_scratch(&*state, blk) } {
            let scratch = &raw mut SCRATCH;
            unsafe { (&mut *scratch)[12..16].copy_from_slice(&new_parent.to_le_bytes()); }
            let _ = unsafe { write_block_from_scratch(&*state, blk) };
        }
        let old_links = unsafe { read_inode_links(&*state, old_parent) };
        unsafe { update_inode_links(&*state, old_parent, old_links.saturating_sub(1)); }
        let new_links = unsafe { read_inode_links(&*state, new_parent) };
        unsafe { update_inode_links(&*state, new_parent, new_links + 1); }
    }

    0
}

// ── Path helpers ─────────────────────────────────────────────────────────────

/// Strip the `/ext2` prefix so we get a root-relative path.
fn strip_ext2_prefix(path: &[u8]) -> &[u8] {
    if path.starts_with(b"/ext2/") {
        &path[5..] // keep leading '/'
    } else if path == b"/ext2" {
        b"/"
    } else {
        path
    }
}
