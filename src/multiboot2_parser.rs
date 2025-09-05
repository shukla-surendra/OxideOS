#[allow(dead_code)] // Suppress dead code warnings for structs if not all are used
use crate::println;

#[repr(C)]
pub struct MultibootInfo {
    total_size: u32,
    reserved: u32,
    // Tags follow immediately
}

#[repr(C, align(8))]
pub struct MultibootTag {
    typ: u32,
    size: u32,
    // Payload follows
}

#[derive(Copy, Clone)]
#[repr(C, align(8))]
pub struct FramebufferTag {
    pub typ: u32,
    pub size: u32,
    pub addr: u64,
    pub pitch: u32,
    pub width: u32,
    pub height: u32,
    pub bpp: u8,
    pub kind: u8,
    pub reserved: u16,
    // For kind = 1 (RGB)
    pub red_field_position: u32,
    pub red_mask_size: u32,
    pub green_field_position: u32,
    pub green_mask_size: u32,
    pub blue_field_position: u32,
    pub blue_mask_size: u32,
}

#[repr(C, align(8))]
pub struct CommandLineTag {
    typ: u32,
    size: u32,
    // NUL-terminated string follows
}

#[repr(C, align(8))]
pub struct BootLoaderNameTag {
    typ: u32,
    size: u32,
    // NUL-terminated string follows
}

#[repr(C, align(8))]
pub struct BasicMemoryInfoTag {
    typ: u32,
    size: u32,
    mem_lower: u32,
    mem_upper: u32,
}

#[repr(C, align(8))]
pub struct MemoryMapTag {
    typ: u32,
    size: u32,
    entry_size: u32,
    entry_version: u32,
    // Variable number of MemoryMapEntry
}

#[repr(C)]
pub struct MemoryMapEntry {
    base_addr: u64,
    length: u64,
    typ: u32,
    reserved: u32,
}

static mut FRAMEBUFFER_INFO: Option<FramebufferTag> = None;

pub fn get_framebuffer_info() -> Option<FramebufferTag> {
    unsafe { FRAMEBUFFER_INFO }
}

unsafe fn store_framebuffer_info(fb_tag: &FramebufferTag) {
    unsafe {
        FRAMEBUFFER_INFO = Some(*fb_tag);
    }
}

unsafe fn clear_framebuffer(fb_tag: &FramebufferTag) {
    let fb_ptr = fb_tag.addr as *mut u32;
    let pixel_count = (fb_tag.width * fb_tag.height) as usize;
    for i in 0..pixel_count {
        fb_ptr.add(i).write_volatile(0); // Black (0x00000000)
    }
}

pub unsafe fn draw_pixel(fb_tag: &FramebufferTag, x: u32, y: u32, red: u8, green: u8, blue: u8) {
    if x < fb_tag.width && y < fb_tag.height && fb_tag.kind == 1 {
        let offset = (y * fb_tag.pitch + x * (fb_tag.bpp as u32 / 8)) as usize;
        let fb_ptr = (fb_tag.addr as *mut u32).add(offset / 4);
        let color = (red as u32) << fb_tag.red_field_position |
                    (green as u32) << fb_tag.green_field_position |
                    (blue as u32) << fb_tag.blue_field_position;
        fb_ptr.write_volatile(color);
    }
}

pub unsafe fn draw_rectangle(fb_tag: &FramebufferTag, x: u32, y: u32, width: u32, height: u32, red: u8, green: u8, blue: u8) {
    if fb_tag.kind == 1 {
        for dy in 0..height {
            for dx in 0..width {
                if x + dx < fb_tag.width && y + dy < fb_tag.height {
                    draw_pixel(fb_tag, x + dx, y + dy, red, green, blue);
                }
            }
        }
    }
}

pub unsafe fn parse_multiboot(info_addr: usize) {
    let info = &*(info_addr as *const MultibootInfo);
    println!("Total size: {}, Reserved: {}", info.total_size, info.reserved);

    let mut tag_ptr = (info_addr + core::mem::size_of::<MultibootInfo>()) as *const MultibootTag;

    while (*tag_ptr).typ != 0 {
        let tag_type = (*tag_ptr).typ;
        let tag_size = (*tag_ptr).size;

        println!("Tag type: {}, size: {}", tag_type, tag_size);

        match tag_type {
            1 => {
                let cmdline_tag = &*(tag_ptr as *const CommandLineTag);
                let cmdline_ptr = (tag_ptr as usize + 8) as *const u8;
                let cmdline = core::str::from_utf8_unchecked(core::slice::from_raw_parts(cmdline_ptr, tag_size as usize - 8 - 1));
                println!("Command line: {}", cmdline);
            }
            2 => {
                let bootloader_tag = &*(tag_ptr as *const BootLoaderNameTag);
                let name_ptr = (tag_ptr as usize + 8) as *const u8;
                let name = core::str::from_utf8_unchecked(core::slice::from_raw_parts(name_ptr, tag_size as usize - 8 - 1));
                println!("Boot loader name: {}", name);
            }
            4 => {
                let mem_info_tag = &*(tag_ptr as *const BasicMemoryInfoTag);
                println!("Lower memory: {} KiB, Upper memory: {} KiB", mem_info_tag.mem_lower, mem_info_tag.mem_upper);
            }
            6 => {
                let mem_map_tag = &*(tag_ptr as *const MemoryMapTag);
                println!("Memory map entry size: {}, version: {}", mem_map_tag.entry_size, mem_map_tag.entry_version);
                let mut entry_ptr = (tag_ptr as usize + 16) as *const MemoryMapEntry;
                let num_entries = ((tag_size as usize) - 16) / (mem_map_tag.entry_size as usize);
                for i in 0..num_entries {
                    let entry = &*entry_ptr;
                    println!("Memory region {}: base {}, length {}, type {}", i, entry.base_addr, entry.length, entry.typ);
                    entry_ptr = entry_ptr.add(1);
                }
            }
            8 => {
                let fb_tag = &*(tag_ptr as *const FramebufferTag);
                println!("Framebuffer: addr 0x{:x}, pitch {}, width {}, height {}, bpp {}, type {}, reserved {}", fb_tag.addr, fb_tag.pitch, fb_tag.width, fb_tag.height, fb_tag.bpp, fb_tag.kind, fb_tag.reserved);
                store_framebuffer_info(fb_tag); // Store regardless of kind
                if fb_tag.kind == 1 {
                    println!("RGB info: red ({}, {}), green ({}, {}), blue ({}, {})", fb_tag.red_field_position, fb_tag.red_mask_size, fb_tag.green_field_position, fb_tag.green_mask_size, fb_tag.blue_field_position, fb_tag.blue_mask_size);
                    clear_framebuffer(fb_tag);
                    draw_pixel(fb_tag, 10, 10, 0xFF, 0x00, 0x00); // Red pixel at (10, 10)
                } else {
                    println!("Framebuffer type {} not supported (expected RGB, kind=1)", fb_tag.kind);
                }
            }
            _ => {
                println!("Unknown tag type: {}", tag_type);
            }
        }

        let size = (*tag_ptr).size as usize;
        tag_ptr = ((tag_ptr as usize + size + 7) & !7) as *const MultibootTag;
    }
}