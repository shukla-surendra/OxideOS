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
#![feature(abi_x86_interrupt)]

mod panic;
mod kernel;
mod gui;
mod wallpaper;
mod version;

// Modules extracted from the original main.rs
mod net_probe;
mod sysinfo;
mod boot_init;
mod gui_loop;

extern crate alloc;
use gui::graphics::Graphics;
use kernel::serial::SERIAL_PORT;
use kernel::interrupts;

use limine::BaseRevision;
use limine::request::{
    FramebufferRequest, MemoryMapRequest, RsdpRequest,
    HhdmRequest, ExecutableFileRequest,
    RequestsEndMarker, RequestsStartMarker,
};

// ── Limine boot protocol requests ─────────────────────────────────────────────
// Must stay in the crate root so the linker can place them in .requests sections.

#[used] #[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

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

pub static mut WINDOW_MANAGER: gui::window_manager::WindowManager =
    gui::window_manager::WindowManager::new();

/// Kernel ELF binary as mapped by Limine — read by the installer.
pub static mut KERNEL_BINARY_PTR: *const u8 = core::ptr::null();
pub static mut KERNEL_BINARY_LEN: usize      = 0;

// ── Entry point ───────────────────────────────────────────────────────────────

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
