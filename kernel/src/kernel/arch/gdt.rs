//! Minimal x86_64 GDT + TSS setup for OxideOS.
//!
//! This gives us:
//! - kernel code/data segments,
//! - user code/data segments,
//! - a TSS with an RSP0 kernel stack for privilege transitions.

use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18;
pub const USER_CODE_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

const GDT_ENTRY_COUNT: usize = 7;
const TSS_STACK_SIZE: usize = 16 * 1024;

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct TaskStateSegment {
    reserved_1: u32,
    rsp: [u64; 3],
    reserved_2: u64,
    ist: [u64; 7],
    reserved_3: u64,
    reserved_4: u16,
    iopb_offset: u16,
}

impl TaskStateSegment {
    const fn new() -> Self {
        Self {
            reserved_1: 0,
            rsp: [0; 3],
            reserved_2: 0,
            ist: [0; 7],
            reserved_3: 0,
            reserved_4: 0,
            iopb_offset: core::mem::size_of::<TaskStateSegment>() as u16,
        }
    }
}

static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut GDT: [u64; GDT_ENTRY_COUNT] = [0; GDT_ENTRY_COUNT];
static mut PRIVILEGE_STACK: [u8; TSS_STACK_SIZE] = [0; TSS_STACK_SIZE];

const fn segment_descriptor(access: u8, flags: u8) -> u64 {
    ((0xFFFFu64) << 0)
        | ((access as u64) << 40)
        | (((flags as u64) & 0x0F) << 52)
        | ((0xF_u64) << 48)
}

fn tss_descriptors(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xFFFF)
        | ((base & 0x00FF_FFFF) << 16)
        | ((0x89u64) << 40)
        | (((limit as u64 >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);

    let high = base >> 32;
    (low, high)
}

unsafe fn load_segments() {
    let data_sel = KERNEL_DATA_SELECTOR as u64;
    asm!(
        "push {code_sel}",
        "lea rax, [rip + 2f]",
        "push rax",
        "retfq",
        "2:",
        "mov eax, {data_sel:e}",
        "mov ds, ax",
        "mov es, ax",
        "mov ss, ax",
        "mov fs, ax",
        "mov gs, ax",
        code_sel = const KERNEL_CODE_SELECTOR as u64,
        data_sel = in(reg) data_sel,
        out("rax") _,
    );
}

pub unsafe fn init() {
    let privilege_stack_top =
        core::ptr::addr_of!(PRIVILEGE_STACK) as u64 + TSS_STACK_SIZE as u64;
    TSS.rsp[0] = privilege_stack_top;

    GDT[0] = 0;
    GDT[1] = segment_descriptor(0x9A, 0xA);
    GDT[2] = segment_descriptor(0x92, 0xC);
    GDT[3] = segment_descriptor(0xF2, 0xC);
    GDT[4] = segment_descriptor(0xFA, 0xA);

    let tss_base = core::ptr::addr_of!(TSS) as u64;
    let tss_limit = (core::mem::size_of::<TaskStateSegment>() - 1) as u32;
    let (tss_low, tss_high) = tss_descriptors(tss_base, tss_limit);
    GDT[5] = tss_low;
    GDT[6] = tss_high;

    let gdtr = DescriptorTablePointer {
        limit: (core::mem::size_of::<[u64; GDT_ENTRY_COUNT]>() - 1) as u16,
        base: core::ptr::addr_of!(GDT) as u64,
    };

    asm!("lgdt [{}]", in(reg) &gdtr, options(readonly, nostack, preserves_flags));
    load_segments();
    asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack, preserves_flags));

    SERIAL_PORT.write_str("x86_64 GDT/TSS initialized\n");
}
