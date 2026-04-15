//! Read-only ext2 filesystem driver for OxideOS.
//!
//! Reads from the secondary IDE master disk (`ata::read_sector_sec`).
//! The partition offset (LBA of first block) is set during `init()` either
//! from the MBR partition table or by treating the whole disk as ext2 (LBA 0).
//!
//! # Limitations (read-only v1)
//! - 1024 / 2048 / 4096 byte blocks supported
//! - Inode direct blocks only (12 × block_size ≤ 48 KB per file)
//! - No indirect, double-indirect, or triple-indirect blocks
//! - Up to 8 block groups
//! - Up to 16 simultaneously open files
//!
//! # Mount point
//! The VFS layer mounts this driver at `/ext2/`.

extern crate alloc;
use alloc::{string::String, vec::Vec};

use crate::kernel::ata;
use crate::kernel::serial::SERIAL_PORT;

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
}

impl BlockGroupDesc {
    const fn zero() -> Self { Self { block_bitmap: 0, inode_bitmap: 0, inode_table: 0 } }
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
}

impl Ext2Fd {
    const fn empty() -> Self {
        Self {
            active: false, inode_no: 0, file_size: 0, file_offset: 0,
            direct_blocks: [0u32; 12],
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
    inode_size:        u32,   // bytes per inode (128 or larger in rev 1)
    first_data_block:  u32,   // 0 for block_size>1024, 1 for block_size==1024
    groups_count:      u32,
    bgdt:              [BlockGroupDesc; MAX_GROUPS],
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
            inode_size: 128,
            first_data_block: 1,
            groups_count: 0,
            bgdt: [const { BlockGroupDesc::zero() }; MAX_GROUPS],
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

    let groups_count = blocks_count.div_ceil(blocks_per_grp).min(MAX_GROUPS as u32);

    unsafe {
        (*state).lba_offset       = partition_lba;
        (*state).block_size       = block_size;
        (*state).sects_per_block  = sects_per_block;
        (*state).inodes_per_group = inodes_per_grp;
        (*state).inode_size       = inode_size;
        (*state).first_data_block = first_data_blk;
        (*state).groups_count     = groups_count;
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

    let s = &raw const SCRATCH;
    for g in 0..groups_count as usize {
        let base = g * 32;
        unsafe {
            (*state).bgdt[g] = BlockGroupDesc {
                block_bitmap: u32::from_le_bytes([(*s)[base],   (*s)[base+1], (*s)[base+2], (*s)[base+3]]),
                inode_bitmap: u32::from_le_bytes([(*s)[base+4], (*s)[base+5], (*s)[base+6], (*s)[base+7]]),
                inode_table:  u32::from_le_bytes([(*s)[base+8], (*s)[base+9], (*s)[base+10],(*s)[base+11]]),
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
        SERIAL_PORT.write_str("\n");
    }
}

pub fn is_ready() -> bool { unsafe { EXT2.ready } }

/// Returns `true` if `fd` is in the ext2 raw FD range.
pub fn is_ext2_fd(fd: i32) -> bool {
    fd >= EXT2_FD_BASE && fd < EXT2_FD_BASE + EXT2_FD_COUNT as i32
}

/// Open a file at `path` (absolute, e.g. `/etc/passwd`).
/// Returns a raw ext2 FD (≥80) on success, or a negative error.
pub unsafe fn open(path: &[u8]) -> i64 {
    let state = &raw mut EXT2;
    if !(*state).ready { return -2; }

    // Strip optional `/ext2` prefix from path (VFS passes the full path).
    let path = strip_ext2_prefix(path);

    let ino = unsafe { lookup_path(&*state, path) };
    if ino == 0 { return -7; } // ENOENT

    let mut inode = Inode::zero();
    if !unsafe { read_inode(&*state, ino, &mut inode) } { return -1; }
    if !inode.is_file() { return -21; } // EISDIR or not-a-file

    // Allocate FD slot.
    let fds = &raw mut (*state).fds;
    for i in 0..EXT2_FD_COUNT {
        let slot = &raw mut (*fds)[i];
        if !(*slot).active {
            (*slot).active      = true;
            (*slot).inode_no    = ino;
            (*slot).file_size   = inode.size_lo;
            (*slot).file_offset = 0;
            (*slot).direct_blocks.copy_from_slice(&inode.block[..12]);
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
        if blk == 0 { break; }

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
