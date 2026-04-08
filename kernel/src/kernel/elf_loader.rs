//! Minimal ELF64 loader for OxideOS.
//!
//! Supports ET_EXEC (statically linked executable) for x86-64.
//! Maps every PT_LOAD segment, copies file data, zeros BSS, returns e_entry.

use crate::kernel::paging_allocator;

const PAGE_SIZE: usize = 4096;
const ELFMAG: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS64:  u8  = 2;
const ELFDATA2LSB: u8  = 1;
const ET_EXEC:     u16 = 2;
const EM_X86_64:   u16 = 0x3E;
const PT_LOAD:     u32 = 1;
const PF_W:        u32 = 2; // writable segment

/// Returns true if `data` begins with the ELF magic bytes.
pub fn is_elf(data: &[u8]) -> bool {
    data.len() >= 4 && data[..4] == ELFMAG
}

#[repr(C)]
struct Elf64Ehdr {
    e_ident:     [u8; 16],
    e_type:      u16,
    e_machine:   u16,
    e_version:   u32,
    e_entry:     u64,
    e_phoff:     u64,
    _e_shoff:    u64,
    _e_flags:    u32,
    _e_ehsize:   u16,
    e_phentsize: u16,
    e_phnum:     u16,
}

#[repr(C)]
struct Elf64Phdr {
    p_type:   u32,
    p_flags:  u32,
    p_offset: u64,
    p_vaddr:  u64,
    _p_paddr: u64,
    p_filesz: u64,
    p_memsz:  u64,
    _p_align: u64,
}

/// Load an ELF64 binary from `data`.
///
/// Maps each PT_LOAD segment into user virtual memory, copies the file
/// content, and zeroes the BSS region. Returns the entry point on success.
pub unsafe fn load(data: &[u8]) -> Result<u64, &'static str> {
    let ehdr_size = core::mem::size_of::<Elf64Ehdr>();
    if data.len() < ehdr_size { return Err("file too small"); }

    let ehdr = unsafe { &*(data.as_ptr() as *const Elf64Ehdr) };

    if ehdr.e_ident[..4] != ELFMAG      { return Err("bad ELF magic"); }
    if ehdr.e_ident[4] != ELFCLASS64    { return Err("not ELF64"); }
    if ehdr.e_ident[5] != ELFDATA2LSB   { return Err("not little-endian"); }
    if ehdr.e_type    != ET_EXEC        { return Err("not an executable"); }
    if ehdr.e_machine != EM_X86_64      { return Err("not x86-64"); }

    let ph_size = ehdr.e_phentsize as usize;
    if ph_size < core::mem::size_of::<Elf64Phdr>() { return Err("phdr too small"); }

    let phoff = ehdr.e_phoff as usize;
    let phnum = ehdr.e_phnum as usize;

    for i in 0..phnum {
        let ph_off = phoff + i * ph_size;
        if ph_off + core::mem::size_of::<Elf64Phdr>() > data.len() {
            return Err("phdr out of bounds");
        }
        let ph = unsafe { &*(data[ph_off..].as_ptr() as *const Elf64Phdr) };

        if ph.p_type != PT_LOAD || ph.p_memsz == 0 { continue; }

        // Page-align the virtual range.
        let va_start = ph.p_vaddr & !0xFFF;
        let va_end   = (ph.p_vaddr + ph.p_memsz + 0xFFF) & !0xFFF;
        let npages   = ((va_end - va_start) / PAGE_SIZE as u64) as usize;
        let writable = (ph.p_flags & PF_W) != 0;

        // Map pages (ignore "already mapped" for overlapping segments).
        let _ = paging_allocator::map_user_region(va_start, npages, true, writable);

        // Zero the entire region first (handles .bss and alignment padding).
        unsafe {
            core::ptr::write_bytes(va_start as *mut u8,
                                   0,
                                   (va_end - va_start) as usize);
        }

        // Copy segment data from the file image.
        if ph.p_filesz > 0 {
            let src_start = ph.p_offset as usize;
            let src_end   = src_start + ph.p_filesz as usize;
            if src_end > data.len() { return Err("segment beyond EOF"); }
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data[src_start..].as_ptr(),
                    ph.p_vaddr as *mut u8,
                    ph.p_filesz as usize,
                );
            }
        }
    }

    Ok(ehdr.e_entry)
}
