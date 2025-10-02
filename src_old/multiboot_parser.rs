use core::mem::size_of;
use core::ptr::read_unaligned;

/// Multiboot2 info header (first two u32 words in the multiboot2 info structure).
#[repr(C)]
struct MbInfoHeader { total_size: u32, reserved: u32 }

/// Generic tag header
#[repr(C)]
struct TagHeader { typ: u32, size: u32 }

/// Framebuffer tag layout (common / typical layout). Adjust if bootloader differs.
#[repr(C)]
struct FramebufferInfo {
    typ: u32,
    size: u32,
    framebuffer_addr: u64,
    framebuffer_pitch: u32,
    framebuffer_width: u32,
    framebuffer_height: u32,
    framebuffer_bpp: u8,
    // color info (variable) follows — not represented here
}

/// Value-type describing the framebuffer for kernel use.
/// Marked Copy so callers receive it by-value safely (no shared refs to `static mut`).
#[derive(Copy, Clone, Debug)]
pub struct Framebuffer {
    pub phys_addr: usize,
    pub pitch: usize,
    pub width: usize,
    pub height: usize,
    pub bpp: usize,
}

/// Optional global storage for the discovered framebuffer info.
/// We write during initialization (unsafe) and accessors return a copy.
static mut FRAMEBUFFER: Option<Framebuffer> = None;

/// Find and parse the framebuffer inside the multiboot2 info block.
///
/// `mbi_ptr` is the pointer value passed by the bootloader (EBX / multiboot info pointer),
/// expressed as a `u32` (common for i386 multiboot2 setups).
///
/// Safety: caller must ensure `mbi_ptr` is valid while this function reads it.
pub fn find_framebuffer(mbi_ptr: u32) -> Option<Framebuffer> {
    let base = mbi_ptr as usize as *const u8;

    // read total_size from header (first u32)
    // Note: read_unaligned will read from the pointer safely even if not aligned
    let header_ptr = base as *const MbInfoHeader;
    let header = unsafe { read_unaligned(header_ptr) };
    let total_size = header.total_size as usize;
    if total_size == 0 { return None; }

    // tags begin after the 8-byte header
    let mut offset: usize = size_of::<MbInfoHeader>();

    while offset + size_of::<TagHeader>() <= total_size {
        let tag_ptr = unsafe { base.add(offset) as *const TagHeader };
        let tag = unsafe { read_unaligned(tag_ptr) };

        // end tag (type 0, size 8) stops the list
        if tag.typ == 0 && tag.size == 8 { break; }

        if tag.typ == 8 {
            // found framebuffer tag; ensure it fits
            if offset + (tag.size as usize) > total_size {
                return None;
            }
            let fb_ptr = unsafe { base.add(offset) as *const FramebufferInfo };
            let fb = unsafe { read_unaligned(fb_ptr) };

            let addr = fb.framebuffer_addr as usize;
            let pitch = fb.framebuffer_pitch as usize;
            let width = fb.framebuffer_width as usize;
            let height = fb.framebuffer_height as usize;
            let bpp = fb.framebuffer_bpp as usize;

            return Some(Framebuffer { phys_addr: addr, pitch, width, height, bpp });
        }

        // advance to next tag (tags are padded to 8 bytes)
        let next = offset + ((tag.size as usize + 7) & !7usize);
        if next <= offset { break; } // sanity
        offset = next;
    }

    None
}

/// Unsafe helper to parse multiboot and store the framebuffer into a global for later use.
/// Accepts `u32` pointer like the rest of this file.
pub unsafe fn parse_multiboot(mbi_ptr: u32) -> Result<(), &'static str> {
    if let Some(fb) = find_framebuffer(mbi_ptr) {
        FRAMEBUFFER = Some(fb);
        Ok(())
    } else {
        Err("no framebuffer found in multiboot info")
    }
}

/// Return a copy of the framebuffer info if set (no shared references leaked).
pub fn get_framebuffer_info() -> Option<Framebuffer> {
    unsafe { FRAMEBUFFER } // Framebuffer is Copy so Option<Framebuffer> copies out
}
