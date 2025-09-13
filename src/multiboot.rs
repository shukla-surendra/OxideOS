#[repr(C, align(8))]
#[derive(Debug)]
struct Multiboot2Header {
    magic: u32,
    architecture: u32,
    header_length: u32,
    checksum: u32,
    end_tag_type: u32,
    end_tag_flags: u32,
    end_tag_size: u32,
    end_tag_reserved: u32,
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".multiboot2_header")]
#[used] // prevent optimization from removing it
pub static MULTIBOOT2_HEADER: Multiboot2Header = Multiboot2Header {
    magic: 0xE85250D6,
    architecture: 0, // 0 = i386
    header_length: core::mem::size_of::<Multiboot2Header>() as u32,
    checksum: (0u32)
        .wrapping_sub(0xE85250D6 + 0 + core::mem::size_of::<Multiboot2Header>() as u32),
    end_tag_type: 0,
    end_tag_flags: 0,
    end_tag_size: 8,
    end_tag_reserved: 0,
};