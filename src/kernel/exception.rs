// src/kernel/exception.rs
#![no_std]

use crate::kernel::serial::SERIAL_PORT;

#[repr(C)]
pub struct SavedRegs {
    // This struct matches the exact push order used in the stubs below:
    // we explicitly push: eax, ecx, edx, ebx, esp_dummy, ebp, esi, edi
    pub eax: u32,
    pub ecx: u32,
    pub edx: u32,
    pub ebx: u32,
    pub esp_dummy: u32,
    pub ebp: u32,
    pub esi: u32,
    pub edi: u32,
    // Immediately following this in memory the stub pushes:
    // saved_eip (u32), saved_cs (u32), saved_eflags (u32)
}

#[unsafe(no_mangle)]
pub extern "C" fn isr_common_handler(regs_ptr: *const SavedRegs, int_no: u32, err_code: u32) {
    let regs = unsafe { &*regs_ptr };
    unsafe {

        SERIAL_PORT.write_str("\n\n=== CPU EXCEPTION ===\n");
        SERIAL_PORT.write_str("Interrupt #: 0x");
        SERIAL_PORT.write_hex(int_no);
        SERIAL_PORT.write_str(", Error code: 0x");
        SERIAL_PORT.write_hex(err_code);
        SERIAL_PORT.write_str("\n");

        SERIAL_PORT.write_str("EAX: 0x"); SERIAL_PORT.write_hex(regs.eax);
        SERIAL_PORT.write_str(" EBX: 0x"); SERIAL_PORT.write_hex(regs.ebx);
        SERIAL_PORT.write_str(" ECX: 0x"); SERIAL_PORT.write_hex(regs.ecx);
        SERIAL_PORT.write_str(" EDX: 0x"); SERIAL_PORT.write_hex(regs.edx);
        SERIAL_PORT.write_str("\nESI: 0x"); SERIAL_PORT.write_hex(regs.esi);
        SERIAL_PORT.write_str(" EDI: 0x"); SERIAL_PORT.write_hex(regs.edi);
        SERIAL_PORT.write_str(" EBP: 0x"); SERIAL_PORT.write_hex(regs.ebp);
        SERIAL_PORT.write_str(" ESP: 0x"); SERIAL_PORT.write_hex(regs.esp_dummy);
        SERIAL_PORT.write_str("\n");



    }
  
    // saved_eip, saved_cs, saved_eflags are right after SavedRegs in memory
    unsafe {
        let p = (regs_ptr as *const u8).add(core::mem::size_of::<SavedRegs>()) as *const u32;
        let saved_eip = *p;
        let saved_cs = *p.add(1);
        let saved_eflags = *p.add(2);

        SERIAL_PORT.write_str("EIP: 0x"); SERIAL_PORT.write_hex(saved_eip);
        SERIAL_PORT.write_str(" CS: 0x"); SERIAL_PORT.write_hex(saved_cs);
        SERIAL_PORT.write_str(" EFLAGS: 0x"); SERIAL_PORT.write_hex(saved_eflags);
        SERIAL_PORT.write_str("\n");
    }

    unsafe {


            match int_no {
        0 => SERIAL_PORT.write_str("Divide Error (INT 0)\n"),
        6 => SERIAL_PORT.write_str("Invalid Opcode (INT 6)\n"),
        8 => SERIAL_PORT.write_str("Double Fault (INT 8)\n"),
        13 => SERIAL_PORT.write_str("General Protection Fault (INT 13)\n"),
        14 => SERIAL_PORT.write_str("Page Fault (INT 14)\n"),
        _ => SERIAL_PORT.write_str("Exception (other)\n"),
    }

    SERIAL_PORT.write_str("Kernel halted due to exception\n");


    }



    loop {
        unsafe { core::arch::asm!("hlt"); }
    }
}
