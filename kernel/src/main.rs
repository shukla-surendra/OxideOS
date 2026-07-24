//! OxideOS 64-bit Kernel — entry point.
//!
//! Module map (this file only wires modules together; logic lives elsewhere)
//! ─────────────────────────────────────────────────────────────────────────────
//! kernel/        Architecture, drivers, memory, fs, process, syscall, IPC, GUI
//! gui/           Desktop environment: WM, terminal, notepad, launcher, etc.
//! panic          Panic handler and serial debug output
//! version        Build-time version strings
//! wallpaper      Desktop wallpaper data
//! Extracted from the original monolithic main.rs:
//!   boot_init    Hardware init (GDT/IDT/PIC/keyboard/timer/SYSCALL/SMEP),
//!                memory and filesystem init, allocator smoke-test
//!   net_probe    NetProbe state machine — "Test Internet Connection" feature
//!   sysinfo      draw_sysinfo_panel — System Info window renderer
//!   gui_loop     run_gui_with_mouse — main 60-fps GUI event + render loop
//! ─────────────────────────────────────────────────────────────────────────────
#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]

mod panic;
mod kernel;
mod version;

// Desktop + subsystem glue: x86-only until the ARM port reaches the GUI step.
#[cfg(target_arch = "x86_64")]
mod gui;
#[cfg(target_arch = "x86_64")]
mod wallpaper;
#[cfg(target_arch = "x86_64")]
mod net_probe;
#[cfg(target_arch = "x86_64")]
mod sysinfo;
#[cfg(target_arch = "x86_64")]
mod boot_init;
#[cfg(target_arch = "x86_64")]
mod gui_loop;

// No heap on aarch64 yet — the allocator arrives with the memory port step.
#[cfg(target_arch = "x86_64")]
extern crate alloc;

/// Placeholder allocator so the aarch64 image links: every allocation fails,
/// which routes stray heap use to the panic handler instead of corruption.
#[cfg(target_arch = "aarch64")]
mod no_heap {
    use core::alloc::{GlobalAlloc, Layout};

    struct NoHeap;

    unsafe impl GlobalAlloc for NoHeap {
        unsafe fn alloc(&self, _layout: Layout) -> *mut u8 { core::ptr::null_mut() }
        unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
    }

    #[global_allocator]
    static NO_HEAP: NoHeap = NoHeap;
}
#[cfg(target_arch = "x86_64")]
use gui::graphics::Graphics;
use kernel::serial::SERIAL_PORT;
#[cfg(target_arch = "x86_64")]
use kernel::interrupts;

use limine::BaseRevision;
use limine::request::{
    FramebufferRequest, MemoryMapRequest, RsdpRequest,
    HhdmRequest, ExecutableFileRequest,
    RequestsEndMarker, RequestsStartMarker,
};

// ── Limine boot protocol requests ─────────────────────────────────────────────
// Must stay in the crate root so the linker can place them in .requests sections.

#[cfg(target_arch = "x86_64")]
#[used] #[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

// Revision 2 keeps Limine's unconditional direct map of the first 4 GiB, which
// is what lets us reach the PL011 UART MMIO (phys 0x0900_0000) through the
// HHDM before we own page tables.  Revision 3 maps only RAM + framebuffer;
// switch back once the aarch64 memory step maps device memory explicitly.
#[cfg(target_arch = "aarch64")]
#[used] #[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::with_revision(2);

#[used] #[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used] #[unsafe(link_section = ".requests")]
static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

#[used] #[unsafe(link_section = ".requests")]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used] #[unsafe(link_section = ".requests")]
pub static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used] #[unsafe(link_section = ".requests")]
static KERNEL_FILE_REQUEST: ExecutableFileRequest = ExecutableFileRequest::new();

#[used] #[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

#[used] #[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

// ── Kernel globals ─────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
pub static mut WINDOW_MANAGER: gui::window_manager::WindowManager =
    gui::window_manager::WindowManager::new();

/// Kernel ELF binary as mapped by Limine — read by the installer.
#[cfg(target_arch = "x86_64")]
pub static mut KERNEL_BINARY_PTR: *const u8 = core::ptr::null();
#[cfg(target_arch = "x86_64")]
pub static mut KERNEL_BINARY_LEN: usize      = 0;

// ── Entry point ───────────────────────────────────────────────────────────────

#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    // ── Stage 1: Serial console ────────────────────────────────────────────
    unsafe { SERIAL_PORT.init(); }
    unsafe { SERIAL_PORT.write_str("\n=== OXIDEOS 64-BIT KERNEL BOOT ===\n"); }
    assert!(BASE_REVISION.is_supported());

    // Capture kernel binary pointer for the installer.
    if let Some(resp) = KERNEL_FILE_REQUEST.get_response() {
        let f = resp.file();
        unsafe { KERNEL_BINARY_PTR = f.addr(); KERNEL_BINARY_LEN = f.size() as usize; }
        unsafe { SERIAL_PORT.write_str("Kernel file captured\n"); }
    }

    // ── Stage 2: Interrupts ────────────────────────────────────────────────
    unsafe { boot_init::init_interrupt_system(); }
    kernel::syscall::run_boot_self_tests();

    // ── Stage 3: Memory + filesystems ─────────────────────────────────────
    unsafe { boot_init::init_memory_and_fs(&MEMORY_MAP_REQUEST); }
    unsafe { boot_init::test_paging_allocation(); }

    // ── Stage 4: Graphics + GUI ────────────────────────────────────────────
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(framebuffer) = fb_resp.framebuffers().next() {
            unsafe { SERIAL_PORT.write_str("✓ Framebuffer acquired\n"); }
            let graphics = Graphics::new(framebuffer);
            let (width, height) = graphics.get_dimensions();
            unsafe {
                SERIAL_PORT.write_str("=== INITIALIZING MOUSE ===\n");
                interrupts::init_mouse_system(width, height);
                SERIAL_PORT.write_str("=== MOUSE INIT DONE ===\n");
                let (terminal_id, sysinfo_id) = gui_loop::create_boot_screen(&graphics);
                gui_loop::run_gui_with_mouse(&graphics, terminal_id, sysinfo_id);
            }
        } else {
            unsafe { SERIAL_PORT.write_str("✗ No framebuffer\n"); }
            unsafe { boot_init::run_text_mode_kernel(); }
        }
    } else {
        unsafe { SERIAL_PORT.write_str("✗ No framebuffer response\n"); }
        unsafe { boot_init::run_text_mode_kernel(); }
    }

    hcf()
}

/// aarch64 bring-up entry: serial console + exception vectors, then park.
/// Next port steps light up the GIC, generic timer, and memory management.
#[cfg(target_arch = "aarch64")]
#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    use kernel::arch::aarch64::exceptions;

    // ── Stage 1: Serial console (PL011 via Limine HHDM) ───────────────────
    unsafe { SERIAL_PORT.init(); }
    unsafe { SERIAL_PORT.write_str("\n=== OXIDEOS AARCH64 KERNEL BOOT ===\n"); }
    assert!(BASE_REVISION.is_supported());

    unsafe {
        SERIAL_PORT.write_str("Current EL: ");
        SERIAL_PORT.write_decimal(exceptions::current_el() as u32);
        SERIAL_PORT.write_str("\n");
    }

    // ── Stage 2: EL1 exception vectors ─────────────────────────────────────
    unsafe {
        exceptions::init();
        SERIAL_PORT.write_str("✓ EL1 exception vectors installed\n");
    }

    if let Some(resp) = HHDM_REQUEST.get_response() {
        unsafe {
            SERIAL_PORT.write_str("HHDM offset: ");
            exceptions::write_hex64(resp.offset());
            SERIAL_PORT.write_str("\n");
        }
    }

    // ── Stage 3: Framebuffer smoke test ────────────────────────────────────
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(fb) = fb_resp.framebuffers().next() {
            unsafe {
                SERIAL_PORT.write_str("✓ Framebuffer ");
                SERIAL_PORT.write_decimal(fb.width() as u32);
                SERIAL_PORT.write_str("x");
                SERIAL_PORT.write_decimal(fb.height() as u32);
                SERIAL_PORT.write_str(" — painting test pattern\n");
            }
            // Unmissable test pattern: eight bright color bars inside a white
            // frame — proves mapping, pitch, and scanout at a glance.
            const BARS: [u32; 8] = [
                0x00FF0000, 0x00FF8000, 0x00FFFF00, 0x0000FF00,
                0x0000FFFF, 0x000000FF, 0x00FF00FF, 0x00FFFFFF,
            ];
            let addr = fb.addr() as *mut u32;
            let (w, h, pitch) = (fb.width() as usize, fb.height() as usize, fb.pitch() as usize / 4);
            for y in 0..h {
                for x in 0..w {
                    let border = x < 8 || y < 8 || x >= w - 8 || y >= h - 8;
                    let color = if border { 0x00FFFFFF } else { BARS[x * 8 / w] };
                    unsafe { addr.add(y * pitch + x).write_volatile(color) };
                }
            }
        }
    } else {
        unsafe { SERIAL_PORT.write_str("✗ No framebuffer response\n"); }
    }

    unsafe {
        SERIAL_PORT.write_str("aarch64 bring-up complete — parking CPU (next: GIC + timer)\n");
    }
    hcf()
}

// ── Halt and catch fire ───────────────────────────────────────────────────────

fn hcf() -> ! {
    loop {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            core::arch::asm!("hlt");
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            core::arch::asm!("wfi");
            #[cfg(target_arch = "loongarch64")]
            core::arch::asm!("idle 0");
        }
    }
}
