//! OxideOS disk installer.
//!
//! Writes a bootable OxideOS layout to the secondary ATA disk:
//!   LBA 0          : MBR (partition table + Limine BIOS bootstrap)
//!   Partition 1 (EFI, 64 MB, FAT32): Limine UEFI bootloader + kernel binary
//!   Partition 2 (data, 64 MB, FAT16): empty OxideOS data filesystem
//!
//! All writes go to the secondary ATA bus via `ata::write_sector_sec`.
//! The kernel binary is accessed through the pointer captured from Limine at boot.
//! Limine boot files (BOOTX64.EFI, limine-bios.sys) are embedded at compile time.

use crate::kernel::ata;
use crate::kernel::serial::SERIAL_PORT;

// ── Embedded Limine boot files ─────────────────────────────────────────────

static BOOTX64_EFI:     &[u8] = include_bytes!("../../../limine/BOOTX64.EFI");
static LIMINE_BIOS_SYS: &[u8] = include_bytes!("../../../limine/limine-bios.sys");
static LIMINE_CONF:     &[u8] = b"\
timeout: 3\n\
\n\
/OxideOS\n\
    protocol: limine\n\
    kernel_path: boot():/boot/kernel\n\
";

// ── Disk layout ────────────────────────────────────────────────────────────

const EFI_PART_START:    u32 = 2048;    // LBA, 1 MB aligned
const EFI_PART_SECTORS:  u32 = 131072;  // 64 MB
const DATA_PART_START:   u32 = 133120;  // immediately after EFI
const DATA_PART_SECTORS: u32 = 131072;  // 64 MB
const TOTAL_SECTORS:     u32 = 264192;  // minimum disk size

// ── FAT32 layout (EFI partition) ───────────────────────────────────────────
// 131072 sectors, 512 bytes/sector, 8 sectors/cluster (4 KB)
// Reserved: 32 sectors; 2 FATs × 128 sectors each = 256
// Data area starts at sector 32 + 256 = 288 (relative to partition start)

const FAT32_RESERVED:    u32 = 32;
const FAT32_FAT_SIZE:    u32 = 128;   // sectors per FAT copy
const FAT32_FAT_COUNT:   u32 = 2;
const FAT32_SPC:         u32 = 8;     // sectors per cluster
const FAT32_ROOT_CLUSTER: u32 = 2;
// Absolute LBAs
const FAT32_FAT1_LBA:    u32 = EFI_PART_START + FAT32_RESERVED;
const FAT32_FAT2_LBA:    u32 = FAT32_FAT1_LBA + FAT32_FAT_SIZE;
const FAT32_DATA_LBA:    u32 = FAT32_FAT2_LBA + FAT32_FAT_SIZE;
// cluster_to_lba32(c) = FAT32_DATA_LBA + (c - 2) * FAT32_SPC

// ── FAT16 layout (data partition) ─────────────────────────────────────────
// 131072 sectors, 64 sectors/cluster (32 KB), 4 reserved, 2×16 FAT, 32 root-dir sectors

const FAT16_RESERVED:    u32 = 4;
const FAT16_FAT_SIZE:    u32 = 16;    // sectors per FAT copy
const FAT16_FAT_COUNT:   u32 = 2;
const FAT16_SPC:         u32 = 64;    // sectors per cluster
const FAT16_ROOT_ENTRIES: u32 = 512;  // 32 bytes each → 16/sector → 32 sectors
const FAT16_ROOT_SECTORS: u32 = FAT16_ROOT_ENTRIES / 16;
// Absolute LBAs
const FAT16_FAT1_LBA:    u32 = DATA_PART_START + FAT16_RESERVED;
const FAT16_FAT2_LBA:    u32 = FAT16_FAT1_LBA + FAT16_FAT_SIZE;
const FAT16_ROOT_LBA:    u32 = FAT16_FAT2_LBA + FAT16_FAT_SIZE;
const FAT16_DATA_LBA:    u32 = FAT16_ROOT_LBA + FAT16_ROOT_SECTORS;

// ── Progress tracking ──────────────────────────────────────────────────────

pub static mut INSTALL_STEP: u32 = 0;

// ── Helper: write a zero sector ───────────────────────────────────────────

unsafe fn zero_sector(lba: u32) -> bool {
    let buf = [0u8; 512];
    unsafe { ata::write_sector_sec(lba, &buf) }
}

// ── Step 1: MBR ───────────────────────────────────────────────────────────

unsafe fn write_mbr() -> bool {
    let mut mbr = [0u8; 512];

    // Copy Limine BIOS bootstrap (first 440 bytes of limine-bios.sys) into MBR.
    // This enables legacy BIOS boot as a bonus.
    let bootstrap_len = LIMINE_BIOS_SYS.len().min(440);
    mbr[..bootstrap_len].copy_from_slice(&LIMINE_BIOS_SYS[..bootstrap_len]);

    // Disk signature (arbitrary)
    mbr[440] = 0xDE; mbr[441] = 0xAD; mbr[442] = 0xBE; mbr[443] = 0xEF;
    // Reserved
    mbr[444] = 0; mbr[445] = 0;

    // Partition entry 1: EFI System (type 0xEF), bootable
    write_part_entry(&mut mbr[446..462], 0x80, 0xEF, EFI_PART_START, EFI_PART_SECTORS);
    // Partition entry 2: FAT16 data (type 0x06)
    write_part_entry(&mut mbr[462..478], 0x00, 0x06, DATA_PART_START, DATA_PART_SECTORS);
    // Entries 3 and 4: empty (already zero)

    // Boot signature
    mbr[510] = 0x55; mbr[511] = 0xAA;

    unsafe { ata::write_sector_sec(0, &mbr) }
}

fn write_part_entry(entry: &mut [u8], status: u8, ptype: u8, lba_start: u32, sectors: u32) {
    entry[0] = status;
    // CHS first/last — beyond CHS range, use 0xFE FF FF
    entry[1] = 0xFE; entry[2] = 0xFF; entry[3] = 0xFF;
    entry[4] = ptype;
    entry[5] = 0xFE; entry[6] = 0xFF; entry[7] = 0xFF;
    entry[8..12].copy_from_slice(&lba_start.to_le_bytes());
    entry[12..16].copy_from_slice(&sectors.to_le_bytes());
}

// ── Step 2: Format EFI partition (FAT32) ──────────────────────────────────

unsafe fn format_efi_partition() -> bool {
    // Zero reserved sectors
    for s in 0..FAT32_RESERVED {
        if !unsafe { zero_sector(EFI_PART_START + s) } { return false; }
    }

    // Boot sector at EFI_PART_START + 0
    let mut bpb = [0u8; 512];
    bpb[0] = 0xEB; bpb[1] = 0x58; bpb[2] = 0x90; // JMP SHORT + NOP
    bpb[3..11].copy_from_slice(b"OXIDEOS ");
    // BPB fields
    bpb[0x0B] = 0x00; bpb[0x0C] = 0x02;   // BytesPerSector = 512
    bpb[0x0D] = FAT32_SPC as u8;           // SectorsPerCluster
    bpb[0x0E] = (FAT32_RESERVED & 0xFF) as u8;
    bpb[0x0F] = (FAT32_RESERVED >> 8)   as u8; // ReservedSectors
    bpb[0x10] = FAT32_FAT_COUNT as u8;    // NumFATs
    bpb[0x11] = 0x00; bpb[0x12] = 0x00;  // RootEntryCount = 0 (FAT32)
    bpb[0x13] = 0x00; bpb[0x14] = 0x00;  // TotalSectors16 = 0
    bpb[0x15] = 0xF8;                     // MediaType = fixed disk
    bpb[0x16] = 0x00; bpb[0x17] = 0x00;  // FATSize16 = 0 (FAT32)
    bpb[0x18] = 0x3F; bpb[0x19] = 0x00;  // SectorsPerTrack = 63
    bpb[0x1A] = 0xFF; bpb[0x1B] = 0x00;  // NumHeads = 255
    // HiddenSectors = EFI_PART_START
    bpb[0x1C..0x20].copy_from_slice(&EFI_PART_START.to_le_bytes());
    // TotalSectors32
    bpb[0x20..0x24].copy_from_slice(&EFI_PART_SECTORS.to_le_bytes());
    // FAT32-specific BPB extension:
    // FATSize32
    bpb[0x24..0x28].copy_from_slice(&FAT32_FAT_SIZE.to_le_bytes());
    bpb[0x28] = 0x00; bpb[0x29] = 0x00;  // ExtFlags
    bpb[0x2A] = 0x00; bpb[0x2B] = 0x00;  // FSVersion = 0.0
    // RootCluster = 2
    bpb[0x2C] = 0x02; bpb[0x2D] = 0x00; bpb[0x2E] = 0x00; bpb[0x2F] = 0x00;
    bpb[0x30] = 0x01; bpb[0x31] = 0x00;  // FSInfo sector = 1
    bpb[0x32] = 0x06; bpb[0x33] = 0x00;  // BackupBootSector = 6
    // bpb[0x34..0x40] reserved, already zero
    bpb[0x40] = 0x80;  // DriveNumber = 0x80 (hard disk)
    bpb[0x41] = 0x00;  // Reserved1
    bpb[0x42] = 0x29;  // BootSignature (extended)
    bpb[0x43..0x47].copy_from_slice(b"OXIE"); // VolumeID
    bpb[0x47..0x52].copy_from_slice(b"OXIDEEFI   "); // VolumeLabel (11 bytes)
    bpb[0x52..0x5A].copy_from_slice(b"FAT32   "); // FSType
    bpb[0x1FE] = 0x55; bpb[0x1FF] = 0xAA;

    if !unsafe { ata::write_sector_sec(EFI_PART_START, &bpb) } { return false; }

    // FSInfo sector at EFI_PART_START + 1
    let mut fsi = [0u8; 512];
    fsi[0..4].copy_from_slice(&0x41615252u32.to_le_bytes());   // LeadSig
    fsi[484..488].copy_from_slice(&0x61417272u32.to_le_bytes()); // StrucSig
    fsi[488..492].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // FreeCount = unknown
    fsi[492..496].copy_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // NextFree = unknown
    fsi[508] = 0x00; fsi[509] = 0x00; fsi[510] = 0x55; fsi[511] = 0xAA;
    if !unsafe { ata::write_sector_sec(EFI_PART_START + 1, &fsi) } { return false; }

    // Backup boot sector at EFI_PART_START + 6
    if !unsafe { ata::write_sector_sec(EFI_PART_START + 6, &bpb) } { return false; }

    // Zero FAT1
    for s in 0..FAT32_FAT_SIZE {
        if !unsafe { zero_sector(FAT32_FAT1_LBA + s) } { return false; }
    }
    // Zero FAT2
    for s in 0..FAT32_FAT_SIZE {
        if !unsafe { zero_sector(FAT32_FAT2_LBA + s) } { return false; }
    }

    // Initialise FAT entries 0, 1, 2 (media byte, reserved, root dir EOC)
    let mut fat_sec = [0u8; 512];
    // Entry 0: 0x0FFFFFF8 (media)
    fat_sec[0] = 0xF8; fat_sec[1] = 0xFF; fat_sec[2] = 0xFF; fat_sec[3] = 0x0F;
    // Entry 1: 0x0FFFFFFF (reserved)
    fat_sec[4] = 0xFF; fat_sec[5] = 0xFF; fat_sec[6] = 0xFF; fat_sec[7] = 0x0F;
    // Entry 2: 0x0FFFFFFF (root dir EOC)
    fat_sec[8] = 0xFF; fat_sec[9] = 0xFF; fat_sec[10] = 0xFF; fat_sec[11] = 0x0F;

    if !unsafe { ata::write_sector_sec(FAT32_FAT1_LBA, &fat_sec) } { return false; }
    if !unsafe { ata::write_sector_sec(FAT32_FAT2_LBA, &fat_sec) } { return false; }

    // Zero root directory cluster (cluster 2, FAT32_SPC sectors)
    let root_lba = fat32_cluster_lba(FAT32_ROOT_CLUSTER);
    for s in 0..FAT32_SPC {
        if !unsafe { zero_sector(root_lba + s) } { return false; }
    }

    true
}

fn fat32_cluster_lba(cluster: u32) -> u32 {
    FAT32_DATA_LBA + (cluster - 2) * FAT32_SPC
}

// ── Step 3: Format data partition (FAT16) ─────────────────────────────────

unsafe fn format_data_partition() -> bool {
    // Zero reserved sectors
    for s in 0..FAT16_RESERVED {
        if !unsafe { zero_sector(DATA_PART_START + s) } { return false; }
    }

    // FAT16 BPB at DATA_PART_START
    let mut bpb = [0u8; 512];
    bpb[0] = 0xEB; bpb[1] = 0x3C; bpb[2] = 0x90; // JMP SHORT + NOP
    bpb[3..11].copy_from_slice(b"OXIDEOS ");
    bpb[0x0B] = 0x00; bpb[0x0C] = 0x02;   // BytesPerSector = 512
    bpb[0x0D] = FAT16_SPC as u8;           // SectorsPerCluster
    bpb[0x0E] = (FAT16_RESERVED & 0xFF) as u8;
    bpb[0x0F] = (FAT16_RESERVED >> 8)   as u8;
    bpb[0x10] = FAT16_FAT_COUNT as u8;
    bpb[0x11] = (FAT16_ROOT_ENTRIES & 0xFF) as u8;
    bpb[0x12] = (FAT16_ROOT_ENTRIES >> 8)   as u8; // RootEntryCount
    bpb[0x13] = 0x00; bpb[0x14] = 0x00;   // TotalSectors16 = 0 (use 32-bit)
    bpb[0x15] = 0xF8;                      // MediaType
    bpb[0x16] = (FAT16_FAT_SIZE & 0xFF) as u8;
    bpb[0x17] = (FAT16_FAT_SIZE >> 8)   as u8;    // FATSize16
    bpb[0x18] = 0x3F; bpb[0x19] = 0x00;  // SectorsPerTrack
    bpb[0x1A] = 0xFF; bpb[0x1B] = 0x00;  // NumHeads
    bpb[0x1C..0x20].copy_from_slice(&DATA_PART_START.to_le_bytes()); // HiddenSectors
    bpb[0x20..0x24].copy_from_slice(&DATA_PART_SECTORS.to_le_bytes()); // TotalSectors32
    // FAT16 extended BPB
    bpb[0x24] = 0x80;  // DriveNumber
    bpb[0x25] = 0x00;  // Reserved
    bpb[0x26] = 0x29;  // BootSignature
    bpb[0x27..0x2B].copy_from_slice(b"DATA");  // VolumeID
    bpb[0x2B..0x36].copy_from_slice(b"OXIDEDATA  "); // VolumeLabel (11 bytes)
    bpb[0x36..0x3E].copy_from_slice(b"FAT16   "); // FSType
    bpb[0x1FE] = 0x55; bpb[0x1FF] = 0xAA;

    if !unsafe { ata::write_sector_sec(DATA_PART_START, &bpb) } { return false; }

    // Zero FATs
    for s in 0..FAT16_FAT_SIZE {
        if !unsafe { zero_sector(FAT16_FAT1_LBA + s) } { return false; }
        if !unsafe { zero_sector(FAT16_FAT2_LBA + s) } { return false; }
    }

    // Initialise FAT16 entries 0 and 1
    let mut fat_sec = [0u8; 512];
    fat_sec[0] = 0xF8; fat_sec[1] = 0xFF; // Entry 0: media byte
    fat_sec[2] = 0xFF; fat_sec[3] = 0xFF; // Entry 1: reserved EOC
    if !unsafe { ata::write_sector_sec(FAT16_FAT1_LBA, &fat_sec) } { return false; }
    if !unsafe { ata::write_sector_sec(FAT16_FAT2_LBA, &fat_sec) } { return false; }

    // Zero root directory
    for s in 0..FAT16_ROOT_SECTORS {
        if !unsafe { zero_sector(FAT16_ROOT_LBA + s) } { return false; }
    }

    true
}

// ── FAT32 file writing ─────────────────────────────────────────────────────

/// State for walking/writing the FAT32 EFI partition.
struct Fat32Writer {
    next_free_cluster: u32,
}

impl Fat32Writer {
    fn new() -> Self {
        // Clusters 0, 1, 2 (root dir) are reserved/used.
        Self { next_free_cluster: 3 }
    }

    /// Allocate a new cluster in both FAT copies and return its number.
    unsafe fn alloc_cluster(&mut self, eoc: bool) -> Option<u32> {
        let c = self.next_free_cluster;
        // Limit: FAT32 can hold up to ((131072 - 32 - 256) / 8) = 16098 clusters.
        if c > 16100 { return None; }
        self.next_free_cluster += 1;

        let val: u32 = if eoc { 0x0FFF_FFFF } else { 0 }; // will be chained later
        unsafe { self.fat32_write_entry(c, val) };
        Some(c)
    }

    /// Write a FAT32 cluster chain entry to both FAT copies.
    unsafe fn fat32_write_entry(&self, cluster: u32, value: u32) {
        let byte_off    = cluster as usize * 4;
        let sector_off  = byte_off / 512;
        let byte_in_sec = byte_off % 512;

        for fat_start in [FAT32_FAT1_LBA, FAT32_FAT2_LBA] {
            let lba = fat_start + sector_off as u32;
            let mut buf = [0u8; 512];
            // Read-modify-write
            let _ = unsafe { ata::read_sector_sec(lba, &mut buf) };
            buf[byte_in_sec    ] = (value       & 0xFF) as u8;
            buf[byte_in_sec + 1] = (value >>  8 & 0xFF) as u8;
            buf[byte_in_sec + 2] = (value >> 16 & 0xFF) as u8;
            buf[byte_in_sec + 3] = (value >> 24 & 0x0F) as u8; // top 4 bits reserved
            let _ = unsafe { ata::write_sector_sec(lba, &buf) };
        }
    }

    /// Write data into a freshly-allocated cluster chain. Returns the first cluster.
    unsafe fn write_data(&mut self, data: &[u8]) -> Option<u32> {
        let cluster_bytes = (FAT32_SPC * 512) as usize;
        let num_clusters  = (data.len() + cluster_bytes - 1) / cluster_bytes;
        if num_clusters == 0 {
            // Empty file — allocate one cluster, mark EOC.
            return unsafe { self.alloc_cluster(true) };
        }

        let mut first_cluster = 0u32;
        let mut prev_cluster  = 0u32;
        let mut remaining     = data;

        for i in 0..num_clusters {
            let is_last = i == num_clusters - 1;
            let c = unsafe { self.alloc_cluster(is_last) }?;
            if i == 0 { first_cluster = c; }

            // Chain the previous cluster to this one.
            if prev_cluster != 0 {
                unsafe { self.fat32_write_entry(prev_cluster, c); }
            }
            prev_cluster = c;

            // Write data into this cluster's sectors.
            let chunk = &remaining[..remaining.len().min(cluster_bytes)];
            remaining  = &remaining[chunk.len()..];

            let base_lba = fat32_cluster_lba(c);
            let mut chunk_off = 0usize;
            for s in 0..FAT32_SPC {
                let mut sec_buf = [0u8; 512];
                let copy_len = (chunk.len() - chunk_off).min(512);
                sec_buf[..copy_len].copy_from_slice(&chunk[chunk_off..chunk_off + copy_len]);
                chunk_off += copy_len;
                if !unsafe { ata::write_sector_sec(base_lba + s, &sec_buf) } { return None; }
                if chunk_off >= chunk.len() { break; }
            }
        }

        Some(first_cluster)
    }

    /// Add a directory entry in the given directory cluster.
    unsafe fn add_dir_entry(
        &self,
        dir_cluster: u32,
        name83: [u8; 11],
        attr: u8,
        first_cluster: u32,
        file_size: u32,
    ) -> bool {
        let dir_lba = fat32_cluster_lba(dir_cluster);
        for s in 0..FAT32_SPC {
            let lba = dir_lba + s;
            let mut buf = [0u8; 512];
            if !unsafe { ata::read_sector_sec(lba, &mut buf) } { return false; }
            for slot in 0..16usize {
                let off = slot * 32;
                if buf[off] == 0x00 || buf[off] == 0xE5 {
                    buf[off..off+11].copy_from_slice(&name83);
                    buf[off+11] = attr;
                    // first cluster high word
                    buf[off+20] = ((first_cluster >> 16) & 0xFF) as u8;
                    buf[off+21] = ((first_cluster >> 24) & 0xFF) as u8;
                    // first cluster low word
                    buf[off+26] = (first_cluster        & 0xFF) as u8;
                    buf[off+27] = ((first_cluster >>  8) & 0xFF) as u8;
                    // file size
                    buf[off+28] = (file_size        & 0xFF) as u8;
                    buf[off+29] = ((file_size >>  8) & 0xFF) as u8;
                    buf[off+30] = ((file_size >> 16) & 0xFF) as u8;
                    buf[off+31] = ((file_size >> 24) & 0xFF) as u8;
                    return unsafe { ata::write_sector_sec(lba, &buf) };
                }
            }
        }
        false // directory full (shouldn't happen for our small set of files)
    }

    /// Find an existing subdirectory by 8.3 name in `dir_cluster`, or create one.
    /// Returns the first cluster of the subdirectory.
    unsafe fn find_or_create_dir(&mut self, dir_cluster: u32, name83: [u8; 11]) -> Option<u32> {
        let dir_lba = fat32_cluster_lba(dir_cluster);
        // Search for existing entry
        for s in 0..FAT32_SPC {
            let lba = dir_lba + s;
            let mut buf = [0u8; 512];
            if !unsafe { ata::read_sector_sec(lba, &mut buf) } { return None; }
            for slot in 0..16usize {
                let off = slot * 32;
                if buf[off] == 0x00 { break; } // end of directory
                if buf[off] == 0xE5 { continue; } // deleted
                let attr = buf[off+11];
                if attr & 0x10 == 0 { continue; } // not a directory
                if &buf[off..off+11] == &name83 {
                    let lo = u16::from_le_bytes([buf[off+26], buf[off+27]]) as u32;
                    let hi = u16::from_le_bytes([buf[off+20], buf[off+21]]) as u32;
                    return Some(lo | (hi << 16));
                }
            }
        }

        // Create new subdirectory
        let new_cluster = unsafe { self.alloc_cluster(true) }?;
        // Zero the new cluster
        let new_lba = fat32_cluster_lba(new_cluster);
        for s in 0..FAT32_SPC {
            if !unsafe { zero_sector(new_lba + s) } { return None; }
        }
        // Write dot / dotdot entries in new dir
        let mut dotbuf = [0u8; 512];
        // '.' entry
        let dot_name: [u8; 11] = *b".          ";
        dotbuf[0..11].copy_from_slice(&dot_name);
        dotbuf[11] = 0x10; // ATTR_DIRECTORY
        dotbuf[26] = (new_cluster        & 0xFF) as u8;
        dotbuf[27] = ((new_cluster >>  8) & 0xFF) as u8;
        dotbuf[20] = ((new_cluster >> 16) & 0xFF) as u8;
        dotbuf[21] = ((new_cluster >> 24) & 0xFF) as u8;
        // '..' entry
        let dotdot_name: [u8; 11] = *b"..         ";
        dotbuf[32..43].copy_from_slice(&dotdot_name);
        dotbuf[43] = 0x10;
        dotbuf[58] = (dir_cluster        & 0xFF) as u8;
        dotbuf[59] = ((dir_cluster >>  8) & 0xFF) as u8;
        dotbuf[52] = ((dir_cluster >> 16) & 0xFF) as u8;
        dotbuf[53] = ((dir_cluster >> 24) & 0xFF) as u8;
        if !unsafe { ata::write_sector_sec(new_lba, &dotbuf) } { return None; }

        // Add entry in parent directory
        if !unsafe { self.add_dir_entry(dir_cluster, name83, 0x10, new_cluster, 0) } {
            return None;
        }
        Some(new_cluster)
    }

    /// Write a file at the given path (relative to root, components as 8.3 names).
    /// `dirs` = list of directory name83 components; `file_name83` = final file name.
    unsafe fn write_file(
        &mut self,
        dirs: &[[u8; 11]],
        file_name83: [u8; 11],
        data: &[u8],
    ) -> bool {
        // Walk/create directory chain from root (cluster 2)
        let mut dir_cluster = FAT32_ROOT_CLUSTER;
        for &dir_name in dirs {
            match unsafe { self.find_or_create_dir(dir_cluster, dir_name) } {
                Some(c) => dir_cluster = c,
                None    => return false,
            }
        }

        // Write file data
        let first_cluster = match unsafe { self.write_data(data) } {
            Some(c) => c,
            None    => return false,
        };

        // Add directory entry
        unsafe { self.add_dir_entry(dir_cluster, file_name83, 0x20, first_cluster, data.len() as u32) }
    }
}

// ── Name helpers ───────────────────────────────────────────────────────────

fn name83(base: &[u8; 8], ext: &[u8; 3]) -> [u8; 11] {
    let mut n = [b' '; 11];
    n[..8].copy_from_slice(base);
    n[8..11].copy_from_slice(ext);
    n
}

fn name83_no_ext(base: &[u8; 11]) -> [u8; 11] {
    *base
}

// ── Step 4: Write boot files into EFI partition ────────────────────────────

unsafe fn write_boot_files(writer: &mut Fat32Writer) -> bool {
    // EFI/BOOT/BOOTX64.EFI
    let efi_dir  = name83(b"EFI     ", b"   ");
    let boot_dir = name83(b"BOOT    ", b"   ");
    let bootx64  = name83(b"BOOTX64 ", b"EFI");
    if !unsafe { writer.write_file(&[efi_dir, boot_dir], bootx64, BOOTX64_EFI) } {
        return false;
    }

    // boot/limine/limine-bios.sys
    let boot_dir2   = name83(b"BOOT    ", b"   ");
    let limine_dir  = name83(b"LIMINE  ", b"   ");
    let bios_sys    = name83(b"LIMINE-B", b"SYS");
    if !unsafe { writer.write_file(&[boot_dir2, limine_dir], bios_sys, LIMINE_BIOS_SYS) } {
        return false;
    }

    // boot/limine/limine.conf
    let boot_dir3   = name83(b"BOOT    ", b"   ");
    let limine_dir2 = name83(b"LIMINE  ", b"   ");
    let conf_name   = name83(b"LIMINE  ", b"CON");
    if !unsafe { writer.write_file(&[boot_dir3, limine_dir2], conf_name, LIMINE_CONF) } {
        return false;
    }

    // boot/kernel (no extension)
    let boot_dir4   = name83(b"BOOT    ", b"   ");
    let kernel_name = name83_no_ext(b"KERNEL     ");
    let kernel_data = unsafe {
        if crate::KERNEL_BINARY_PTR.is_null() || crate::KERNEL_BINARY_LEN == 0 {
            return false;
        }
        core::slice::from_raw_parts(crate::KERNEL_BINARY_PTR, crate::KERNEL_BINARY_LEN)
    };
    if !unsafe { writer.write_file(&[boot_dir4], kernel_name, kernel_data) } {
        return false;
    }

    true
}

// ── Top-level install entry point ──────────────────────────────────────────

/// Run the full installation to the secondary ATA disk.
/// Returns 0 on success, negative error code on failure.
pub unsafe fn do_install() -> i64 {
    unsafe { INSTALL_STEP = 0; }

    if !ata::is_present_sec() {
        unsafe { SERIAL_PORT.write_str("INSTALL: no secondary disk\n"); }
        return -1;
    }
    let secs = ata::sector_count_sec();
    if secs < TOTAL_SECTORS {
        unsafe { SERIAL_PORT.write_str("INSTALL: secondary disk too small\n"); }
        return -2;
    }

    unsafe { SERIAL_PORT.write_str("INSTALL: step 1 — format EFI partition\n"); }
    unsafe { INSTALL_STEP = 1; }
    if !unsafe { format_efi_partition() } {
        unsafe { SERIAL_PORT.write_str("INSTALL: EFI format failed\n"); }
        return -3;
    }

    unsafe { SERIAL_PORT.write_str("INSTALL: step 2 — format data partition\n"); }
    unsafe { INSTALL_STEP = 2; }
    if !unsafe { format_data_partition() } {
        unsafe { SERIAL_PORT.write_str("INSTALL: data format failed\n"); }
        return -4;
    }

    unsafe { SERIAL_PORT.write_str("INSTALL: step 3 — writing boot files\n"); }
    unsafe { INSTALL_STEP = 3; }
    let mut writer = Fat32Writer::new();
    if !unsafe { write_boot_files(&mut writer) } {
        unsafe { SERIAL_PORT.write_str("INSTALL: boot file write failed\n"); }
        return -5;
    }

    // MBR is written LAST — if anything above fails the disk won't boot (safe failure).
    unsafe { SERIAL_PORT.write_str("INSTALL: step 4 — writing MBR\n"); }
    unsafe { INSTALL_STEP = 4; }
    if !unsafe { write_mbr() } {
        unsafe { SERIAL_PORT.write_str("INSTALL: MBR write failed\n"); }
        return -6;
    }

    unsafe { INSTALL_STEP = 5; }
    unsafe { SERIAL_PORT.write_str("INSTALL: complete!\n"); }
    0
}
