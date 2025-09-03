#[repr(C)]
struct MultibootInfo {
    total_size: u32,
    reserved: u32,
    // Tags follow immediately
}

#[repr(C, align(8))]
struct MultibootTag {
    typ: u32,
    size: u32,
    // Payload follows
}

#[repr(C, align(8))]
struct FramebufferTag {
    typ: u32,
    size: u32,
    addr: u64,
    pitch: u32,
    width: u32,
    height: u32,
    bpp: u8,
    kind: u8,
    reserved: u16,
}

/// Walk through all tags
pub unsafe fn parse_multiboot(info_addr: usize) {
    let info = unsafe { &*(info_addr as *const MultibootInfo) };
    let mut tag_ptr =
        (info_addr + core::mem::size_of::<MultibootInfo>()) as *const MultibootTag;

    while unsafe { (*tag_ptr).typ } != 0 {
        match unsafe { (*tag_ptr).typ } {
            8 => {
                let fb = unsafe { &*(tag_ptr as *const FramebufferTag) };

                unsafe {
                    let vga = 0xb8000 as *mut u8;
                    let msg = b"FB!";
                    for (i, &ch) in msg.iter().enumerate() {
                        *vga.add(i * 2) = ch;
                        *vga.add(i * 2 + 1) = 0x0f;
                    }
                }

                let _ = fb; // keep compiler happy
            }
            _ => {}
        }

        let size = unsafe { (*tag_ptr).size as usize };
        tag_ptr = ((tag_ptr as usize + size + 7) & !7) as *const MultibootTag;
    }
}


#[repr(C, align(8))]
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
