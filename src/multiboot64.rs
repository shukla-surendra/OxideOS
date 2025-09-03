#[repr(C, align(8))]
#[derive(Debug)]
struct MultibootHeader {
    magic: u32,
    architecture: u32,
    header_length: u32,
    checksum: u32,
    // End tag
    end_type: u16,
    end_flags: u16,
    end_size: u32,
}

#[unsafe(link_section = ".multiboot2_header")]
#[unsafe(no_mangle)]
pub static MULTIBOOT2_HEADER: MultibootHeader = MultibootHeader {
    magic: 0xE85250D6,   // Multiboot2 magic
    architecture: 0,     // 0 = i386
    header_length: 16,   // header size in bytes
    checksum: (0u32)
        .wrapping_sub(0xE85250D6u32 + 0 + 16), // checksum so sum=0
    end_type: 0,         // End tag
    end_flags: 0,
    end_size: 8,
};
