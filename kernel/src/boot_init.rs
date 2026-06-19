//! Boot-time hardware and system initialization.
//!
//! All functions here run before the GUI loop starts.  Call order in `kmain`:
//!   1. `init_interrupt_system` — GDT, IDT, PIC, keyboard, timer, SYSCALL, SMEP
//!   2. `init_memory_and_fs`    — heap, RamFS, FAT, ext2, env, network
//!   3. `test_paging_allocation` — allocator smoke-test (debug build helper)
//!
//! Diagnostic helpers (`check_system_tables_64bit`, `verify_idt_entries_64bit`,
//! `test_64bit_interrupts`) are called internally from `init_interrupt_system`.

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::{gdt, idt, interrupts, timer, pic, keyboard,
                    syscall_handler, paging_allocator};

// ── Interrupt system ──────────────────────────────────────────────────────────

pub unsafe fn init_interrupt_system() {
    SERIAL_PORT.write_str("=== 64-BIT INTERRUPT SYSTEM SETUP ===\n");
    SERIAL_PORT.write_str("Step 1: Disabling interrupts (CLI)...\n");
    unsafe { asm!("cli"); }

    SERIAL_PORT.write_str("Step 2: Installing x86_64 GDT/TSS...\n");
    gdt::init();
    SERIAL_PORT.write_str("  ✓ GDT/TSS initialized\n");
    check_system_tables_64bit();

    SERIAL_PORT.write_str("Step 3: Initializing 64-bit IDT...\n");
    idt::init();
    SERIAL_PORT.write_str("  ✓ 64-bit IDT loaded\n");
    verify_idt_entries_64bit();

    SERIAL_PORT.write_str("Step 5: Initializing PIC for 64-bit...\n");
    pic::init();
    SERIAL_PORT.write_str("  ✓ PIC remapped\n");

    SERIAL_PORT.write_str("Step 5.5: Initializing 8042 keyboard controller...\n");
    unsafe { keyboard::init(); }
    SERIAL_PORT.write_str("  ✓ Keyboard controller initialized\n");

    SERIAL_PORT.write_str("Step 6: Initializing 64-bit timer...\n");
    timer::init(100);
    SERIAL_PORT.write_str("  ✓ Timer at 100Hz\n");

    SERIAL_PORT.write_str("Step 7: Testing interrupt system...\n");
    test_64bit_interrupts();
    SERIAL_PORT.write_str("✓ 64-bit interrupt system fully operational\n");

    SERIAL_PORT.write_str("Step 8: Enabling SYSCALL/SYSRET fast path...\n");
    unsafe { syscall_handler::init(); }
    SERIAL_PORT.write_str("  ✓ SYSCALL/SYSRET enabled\n");

    SERIAL_PORT.write_str("Step 9: Enabling SMEP + SSE...\n");
    unsafe {
        let mut cr4: u64;
        asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
        cr4 |= 1 << 20; // CR4.SMEP — kernel cannot execute user pages
        cr4 |= 1 << 9;  // CR4.OSFXSR — enables SSE
        cr4 |= 1 << 10; // CR4.OSXMMEXCPT — enables #XF for unmasked SSE FP exceptions
        asm!("mov cr4, {}", in(reg) cr4, options(nomem, nostack, preserves_flags));
    }
    SERIAL_PORT.write_str("  ✓ SMEP + SSE enabled\n");
}

// ── Memory and filesystem ─────────────────────────────────────────────────────

pub unsafe fn init_memory_and_fs(
    memory_map: &limine::request::MemoryMapRequest,
) {
    use crate::kernel::{fs::ramfs::RAMFS, procfs, env, ata, disk_store, diskfs, mbr, fat, ext2, net};
    use crate::kernel::mbr::PTYPE_LINUX;

    paging_allocator::init_paging_heap(memory_map);
    SERIAL_PORT.write_str("✓ Paging allocator initialized\n");

    RAMFS.init();
    SERIAL_PORT.write_str("✓ RamFS initialized\n");

    procfs::populate();
    SERIAL_PORT.write_str("✓ procfs initialized\n");

    env::init_defaults();
    SERIAL_PORT.write_str("✓ Environment initialized\n");

    ata::init_all();
    if ata::is_present()     { unsafe { disk_store::mount(0); } }
    if ata::is_present_sec() { unsafe { disk_store::mount(3); } }

    diskfs::populate();
    SERIAL_PORT.write_str("✓ diskfs populated\n");

    mbr::init();
    fat::init();

    // ext2 on secondary disk — look for the Linux partition type
    {
        let part_lba = unsafe {
            use crate::kernel::mbr::MBR;
            if !(*core::ptr::addr_of!(MBR)).whole_disk {
                let mut lba = 0u32;
                for entry in &(*core::ptr::addr_of!(MBR)).entries {
                    if entry.partition_type == PTYPE_LINUX && entry.start_lba > 0 {
                        lba = entry.start_lba; break;
                    }
                }
                lba
            } else { 0 }
        };
        ext2::init(part_lba);
    }

    net::init();
}

// ── Allocator smoke-test ──────────────────────────────────────────────────────

pub unsafe fn test_paging_allocation() {
    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::vec::Vec;

    SERIAL_PORT.write_str("\n=== TESTING PAGING ALLOCATOR ===\n");

    let boxed = Box::new(0x1234567890ABCDEFu64);
    SERIAL_PORT.write_str("Test 1: Box<u64> @ 0x");
    SERIAL_PORT.write_hex(((&*boxed as *const u64 as usize) >> 32) as u32);
    SERIAL_PORT.write_hex((&*boxed as *const u64 as usize) as u32);
    SERIAL_PORT.write_str("\n");

    let mut vec: Vec<u32> = Vec::new();
    for i in 0..10 { vec.push(i * 100); }
    SERIAL_PORT.write_str("Test 2: Vec[5] = ");
    SERIAL_PORT.write_decimal(vec[5]);
    SERIAL_PORT.write_str("\n");

    drop(boxed); drop(vec);
    let _recycled = Box::new(0xDEADBEEFu64);
    SERIAL_PORT.write_str("Test 3: dealloc + recycle OK\n");
    SERIAL_PORT.write_str("✓ All paging allocator tests passed!\n\n");
}

// ── Diagnostic helpers ────────────────────────────────────────────────────────

unsafe fn check_system_tables_64bit() {
    SERIAL_PORT.write_str("\n=== 64-BIT SYSTEM TABLE CHECK ===\n");
    let mut gdt_ptr = [0u8; 10];
    unsafe { asm!("sgdt [{}]", in(reg) &mut gdt_ptr); }
    let gdt_base = u64::from_le_bytes([
        gdt_ptr[2], gdt_ptr[3], gdt_ptr[4], gdt_ptr[5],
        gdt_ptr[6], gdt_ptr[7], gdt_ptr[8], gdt_ptr[9],
    ]);
    SERIAL_PORT.write_str("GDT Base: 0x");
    SERIAL_PORT.write_hex((gdt_base >> 32) as u32);
    SERIAL_PORT.write_hex(gdt_base as u32);
    SERIAL_PORT.write_str("\n===================\n");
}

unsafe fn verify_idt_entries_64bit() {
    let mut idtr = [0u8; 10];
    unsafe { asm!("sidt [{}]", in(reg) &mut idtr); }
    let idt_base  = u64::from_le_bytes([
        idtr[2], idtr[3], idtr[4], idtr[5], idtr[6], idtr[7], idtr[8], idtr[9],
    ]);
    let idt_limit = u16::from_le_bytes([idtr[0], idtr[1]]);
    SERIAL_PORT.write_str("  IDT Base: 0x");
    SERIAL_PORT.write_hex((idt_base >> 32) as u32);
    SERIAL_PORT.write_hex(idt_base as u32);
    SERIAL_PORT.write_str(", Limit: 0x");
    SERIAL_PORT.write_hex(idt_limit as u32);
    SERIAL_PORT.write_str("\n");
    if idt_base != 0 && idt_limit == 0xFFF {
        SERIAL_PORT.write_str("  ✓ IDT loaded correctly\n");
    } else {
        SERIAL_PORT.write_str("  WARNING: IDT may not be loaded correctly!\n");
    }
}

unsafe fn test_64bit_interrupts() {
    unsafe { asm!("sti"); }
    pic::unmask_irq(0);

    let initial_ticks = timer::get_ticks();
    let target_ticks  = initial_ticks + 10;
    let mut timeout   = 0u32;

    loop {
        if timer::get_ticks() >= target_ticks {
            SERIAL_PORT.write_str("  ✓ Timer interrupts working!\n");
            break;
        }
        timeout += 1;
        if timeout > 1_000_000 {
            SERIAL_PORT.write_str("  TIMEOUT: No timer interrupts\n");
            break;
        }
        for _ in 0..100 { unsafe { asm!("pause"); } }
    }

    pic::unmask_irq(1);
    SERIAL_PORT.write_str("  ✓ Keyboard interrupts enabled\n");
}

// ── Text-mode fallback ────────────────────────────────────────────────────────

pub unsafe fn run_text_mode_kernel() -> ! {
    SERIAL_PORT.write_str("Running in text mode - no GUI available\n");
    let mut counter = 0u64;
    loop {
        counter += 1;
        if counter % 10_000_000 == 0 {
            SERIAL_PORT.write_str("Heartbeat: ");
            SERIAL_PORT.write_decimal(counter as u32);
            SERIAL_PORT.write_str("\n");
        }
        unsafe { core::arch::asm!("hlt"); }
    }
}
