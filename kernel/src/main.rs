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

// The desktop (gui/, gui_loop, sysinfo, net_probe, wallpaper) builds for both
// architectures — on aarch64 the kernel-side subsystems it calls are the
// stubs/real drivers wired up in `kernel/mod.rs`.  Only the x86 hardware
// bring-up (boot_init) stays arch-specific.
mod gui;
mod wallpaper;
mod net_probe;
mod sysinfo;
#[cfg(target_arch = "x86_64")]
mod boot_init;
mod gui_loop;

extern crate alloc;

/// aarch64 heap: a linked-list allocator over the largest usable Limine
/// memory-map region, addressed through the HHDM.  (x86 uses the paging
/// allocator in `kernel/mem/`; this moves there too once aarch64 paging lands.)
#[cfg(target_arch = "aarch64")]
mod heap {
    use linked_list_allocator::LockedHeap;

    #[global_allocator]
    static HEAP: LockedHeap = LockedHeap::empty();

    /// Cap so the identity of "largest region" can't hand us all of RAM
    /// before a real frame allocator exists.
    const HEAP_MAX_BYTES: u64 = 256 * 1024 * 1024;

    pub unsafe fn init(memory_map: &limine::request::MemoryMapRequest, hhdm_offset: u64) -> u64 {
        use limine::memory_map::EntryType;

        let Some(resp) = memory_map.get_response() else { return 0 };
        let mut best: Option<(u64, u64)> = None;
        for entry in resp.entries() {
            if entry.entry_type == EntryType::USABLE {
                if best.map(|(_, len)| entry.length > len).unwrap_or(true) {
                    best = Some((entry.base, entry.length));
                }
            }
        }
        let Some((base, len)) = best else { return 0 };
        let size = len.min(HEAP_MAX_BYTES);
        unsafe {
            HEAP.lock().init((hhdm_offset + base) as *mut u8, size as usize);
        }
        size
    }
}

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

/// Storage smoke test, serial-only: read `/disk/bootcnt.txt`, increment the
/// number inside, write it back.  A count that grows across reboots proves
/// the virtio-blk + FAT16 write path end to end without touching the GUI.
#[cfg(target_arch = "aarch64")]
const STORAGE_BOOT_TEST: bool = false;

#[cfg(target_arch = "aarch64")]
unsafe fn storage_boot_test() {
    use kernel::fat;
    use kernel::fs::{O_CREAT, O_TRUNC, O_WRONLY};

    unsafe {
        let mut count: u32 = 0;
        let fd = fat::open(b"bootcnt.txt", 0);
        if fd >= 0 {
            let mut buf = [0u8; 16];
            let n = fat::read_fd(fd as i32, &mut buf);
            fat::close(fd as i32);
            if n > 0 {
                for &b in &buf[..n as usize] {
                    if b.is_ascii_digit() {
                        count = count * 10 + (b - b'0') as u32;
                    }
                }
            }
        }
        count += 1;

        let fd = fat::open(b"bootcnt.txt", O_WRONLY | O_CREAT | O_TRUNC);
        if fd < 0 {
            SERIAL_PORT.write_str("✗ Storage self-test: open /disk/bootcnt.txt failed\n");
            return;
        }
        let mut out = [0u8; 10];
        let mut i = out.len();
        let mut v = count;
        loop {
            i -= 1;
            out[i] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        fat::write_fd(fd as i32, &out[i..]);
        fat::close(fd as i32);

        SERIAL_PORT.write_str("✓ Storage self-test: boot #");
        SERIAL_PORT.write_decimal(count);
        SERIAL_PORT.write_str(" recorded in /disk/bootcnt.txt\n");
    }
}

/// aarch64 entry: serial + exception vectors + heap + virtio-input, then the
/// same desktop the x86 build runs (mouse + keyboard polled each frame).
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
    // Silence the generic timer first: AAVMF can hand over with it enabled and
    // an interrupt already pending, which would fire the moment IRQs unmask.
    unsafe {
        kernel::arch::aarch64::timer::mask();
        exceptions::init();
        SERIAL_PORT.write_str("✓ EL1 exception vectors installed\n");
    }

    // ── Stage 3: Heap ──────────────────────────────────────────────────────
    let hhdm_offset = HHDM_REQUEST.get_response().map(|r| r.offset()).unwrap_or(0);
    let heap_bytes = unsafe { heap::init(&MEMORY_MAP_REQUEST, hhdm_offset) };
    unsafe {
        SERIAL_PORT.write_str("✓ Heap: ");
        SERIAL_PORT.write_decimal((heap_bytes / (1024 * 1024)) as u32);
        SERIAL_PORT.write_str(" MB\n");
    }
    if heap_bytes == 0 {
        unsafe { SERIAL_PORT.write_str("✗ No usable memory region — halting\n"); }
        hcf();
    }

    // ── Stage 3.5: GICv2 + timer interrupt ─────────────────────────────────
    // Everything below this point can sleep in `wfi` instead of spinning.
    unsafe {
        use kernel::arch::aarch64::{gic, timer};
        use kernel::arch::cpu;

        gic::init();
        gic::enable(gic::INTID_TIMER);
        timer::init_irq();
        cpu::irq_enable();

        SERIAL_PORT.write_str("✓ GICv2 up (");
        SERIAL_PORT.write_decimal(gic::num_intids());
        SERIAL_PORT.write_str(" INTIDs), timer IRQ armed at ");
        SERIAL_PORT.write_decimal(timer::TIMER_HZ as u32);
        SERIAL_PORT.write_str(" Hz\n");

        // Prove interrupts actually arrive before anything depends on them.
        // Sleeping in `wfi` is itself the test: without a working GIC no
        // interrupt can ever become pending and this would hang forever.
        let start = timer::get_ticks();
        while timer::get_ticks().wrapping_sub(start) < 25 {
            cpu::wait_for_interrupt();
        }
        let (spurious, unhandled) = exceptions::irq_anomaly_counts();
        SERIAL_PORT.write_str("✓ Timer IRQs delivered: ");
        SERIAL_PORT.write_decimal(timer::irq_count() as u32);
        SERIAL_PORT.write_str(" (spurious ");
        SERIAL_PORT.write_decimal(spurious as u32);
        SERIAL_PORT.write_str(", unhandled ");
        SERIAL_PORT.write_decimal(unhandled as u32);
        SERIAL_PORT.write_str(")\n");
    }

    // ── Stage 4: Input (virtio-mmio keyboard + mouse, polled) ─────────────
    unsafe {
        kernel::keyboard::init();
        kernel::arch::aarch64::virtio_input::init();
    }

    // ── Stage 4.5: Storage (virtio-blk, polled) + filesystems ──────────────
    // Same sequence as the x86 boot path: RamFS root, block driver, record
    // store, mount-point population, then MBR parse + FAT16 mount.
    unsafe {
        kernel::fs::ramfs::RAMFS.init();
        kernel::arch::aarch64::virtio_blk::init();
        if kernel::ata::is_present() { kernel::disk_store::mount(0); }
        if kernel::ata::is_present_sec() { kernel::disk_store::mount(3); }
        kernel::diskfs::populate();
        kernel::mbr::init();
        kernel::fat::init();
        SERIAL_PORT.write_str("✓ Storage initialised\n");
    }
    if STORAGE_BOOT_TEST {
        unsafe { storage_boot_test(); }
    }

    // ── Stage 5: Graphics + GUI desktop ────────────────────────────────────
    if let Some(fb_resp) = FRAMEBUFFER_REQUEST.get_response() {
        if let Some(framebuffer) = fb_resp.framebuffers().next() {
            unsafe { SERIAL_PORT.write_str("✓ Framebuffer acquired\n"); }
            let graphics = Graphics::new(framebuffer);
            let (width, height) = graphics.get_dimensions();
            unsafe {
                interrupts::init_mouse_system(width, height);
                let (terminal_id, sysinfo_id) = gui_loop::create_boot_screen(&graphics);
                SERIAL_PORT.write_str("Entering GUI loop (virtio-input polled)\n");
                gui_loop::run_gui_with_mouse(&graphics, terminal_id, sysinfo_id);
            }
        } else {
            unsafe { SERIAL_PORT.write_str("✗ No framebuffer\n"); }
        }
    } else {
        unsafe { SERIAL_PORT.write_str("✗ No framebuffer response\n"); }
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
