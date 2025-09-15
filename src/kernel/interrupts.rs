// src/kernel/interrupts.rs - Modified with extensive debugging
use core::arch::global_asm;
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

// interrupt_stubs.rs - Intel syntax for Rust inline assembly
// Produces isr0..isr31 (exceptions) and isr32..isr47 (IRQs)

// External Rust handler - needs unsafe block
unsafe extern "C" {
    unsafe fn isr_common_handler(regs_ptr: *const u32, int_no: u32, err_code: u32);
}

// Intel syntax assembly for interrupt handlers
global_asm!(r#"
.section .text
.intel_syntax noprefix

// Macro: no-error (CPU did NOT push an error code)
.macro ISR_NOERR name num
    .global \name
\name:
    cli
    push ds
    push es

    // Save general registers
    push eax
    push ecx
    push edx
    push ebx
    push esp    // saved esp (dummy)
    push ebp
    push esi
    push edi

    // regs_ptr -> pointer to the saved regs block
    mov eax, esp

    // push args for isr_common_handler: (regs_ptr, int_no, err_code) right-to-left
    push 0           // dummy error code
    push \num        // interrupt number
    push eax         // regs_ptr

    call isr_common_handler

    // if handler returns, clean up args
    add esp, 12

    // restore registers in reverse order
    pop edi
    pop esi
    pop ebp
    pop esp    // discard saved ESP value
    pop ebx
    pop edx
    pop ecx
    pop eax

    pop es
    pop ds
    sti
    iret
.endm

// Macro: with-error (CPU pushed an error code automatically)
.macro ISR_WITHERR name num
    .global \name
\name:
    cli
    push ds
    push es

    // Save general registers
    push eax
    push ecx
    push edx
    push ebx
    push esp    // saved esp (dummy)
    push ebp
    push esi
    push edi

    // regs_ptr -> pointer to the saved regs block
    mov eax, esp

    // CPU error code is at esp+44 (8 regs * 4 + 12 for eip/cs/eflags)
    mov edx, [eax + 44]    // edx = error_code

    // push args: err_code, int_no, regs_ptr (right-to-left)
    push edx
    push \num
    push eax

    call isr_common_handler

    // if returns
    add esp, 12

    // restore registers
    pop edi
    pop esi
    pop ebp
    pop esp
    pop ebx
    pop edx
    pop ecx
    pop eax

    pop es
    pop ds
    sti
    iret
.endm

// Expand macros for exceptions & IRQs

// Exceptions with error codes per Intel: 8, 10, 11, 12, 13, 14, 17
ISR_WITHERR isr8 8
ISR_WITHERR isr10 10
ISR_WITHERR isr11 11
ISR_WITHERR isr12 12
ISR_WITHERR isr13 13
ISR_WITHERR isr14 14
ISR_WITHERR isr17 17

// Exceptions without error codes
ISR_NOERR isr0 0
ISR_NOERR isr1 1
ISR_NOERR isr2 2
ISR_NOERR isr3 3
ISR_NOERR isr4 4
ISR_NOERR isr5 5
ISR_NOERR isr6 6
ISR_NOERR isr7 7
ISR_NOERR isr9 9
ISR_NOERR isr15 15
ISR_NOERR isr16 16
ISR_NOERR isr18 18
ISR_NOERR isr19 19
ISR_NOERR isr20 20
ISR_NOERR isr21 21
ISR_NOERR isr22 22
ISR_NOERR isr23 23
ISR_NOERR isr24 24
ISR_NOERR isr25 25
ISR_NOERR isr26 26
ISR_NOERR isr27 27
ISR_NOERR isr28 28
ISR_NOERR isr29 29
ISR_NOERR isr30 30
ISR_NOERR isr31 31

// IRQs (IRQ0..IRQ15 -> ISR32..ISR47)
ISR_NOERR isr32 32
ISR_NOERR isr33 33
ISR_NOERR isr34 34
ISR_NOERR isr35 35
ISR_NOERR isr36 36
ISR_NOERR isr37 37
ISR_NOERR isr38 38
ISR_NOERR isr39 39
ISR_NOERR isr40 40
ISR_NOERR isr41 41
ISR_NOERR isr42 42
ISR_NOERR isr43 43
ISR_NOERR isr44 44
ISR_NOERR isr45 45
ISR_NOERR isr46 46
ISR_NOERR isr47 47

.att_syntax prefix
"#);

// Declare the ISR functions as extern so Rust knows about them
unsafe extern "C" {
    pub unsafe fn isr0();
    pub unsafe fn isr1();
    pub unsafe fn isr2();
    pub unsafe fn isr3();
    pub unsafe fn isr4();
    pub unsafe fn isr5();
    pub unsafe fn isr6();
    pub unsafe fn isr7();
    pub unsafe fn isr8();
    pub unsafe fn isr9();
    pub unsafe fn isr10();
    pub unsafe fn isr11();
    pub unsafe fn isr12();
    pub unsafe fn isr13();
    pub unsafe fn isr14();
    pub unsafe fn isr15();
    pub unsafe fn isr16();
    pub unsafe fn isr17();
    pub unsafe fn isr18();
    pub unsafe fn isr19();
    pub unsafe fn isr20();
    pub unsafe fn isr21();
    pub unsafe fn isr22();
    pub unsafe fn isr23();
    pub unsafe fn isr24();
    pub unsafe fn isr25();
    pub unsafe fn isr26();
    pub unsafe fn isr27();
    pub unsafe fn isr28();
    pub unsafe fn isr29();
    pub unsafe fn isr30();
    pub unsafe fn isr31();
    pub unsafe fn isr32();
    pub unsafe fn isr33();
    pub unsafe fn isr34();
    pub unsafe fn isr35();
    pub unsafe fn isr36();
    pub unsafe fn isr37();
    pub unsafe fn isr38();
    pub unsafe fn isr39();
    pub unsafe fn isr40();
    pub unsafe fn isr41();
    pub unsafe fn isr42();
    pub unsafe fn isr43();
    pub unsafe fn isr44();
    pub unsafe fn isr45();
    pub unsafe fn isr46();
    pub unsafe fn isr47();
}

// Helper function to get ISR handler addresses for IDT setup
pub fn get_isr_handler(n: u8) -> unsafe extern "C" fn() {
    unsafe {
        match n {
            0 => isr0,
            1 => isr1,
            2 => isr2,
            3 => isr3,
            4 => isr4,
            5 => isr5,
            6 => isr6,
            7 => isr7,
            8 => isr8,
            9 => isr9,
            10 => isr10,
            11 => isr11,
            12 => isr12,
            13 => isr13,
            14 => isr14,
            15 => isr15,
            16 => isr16,
            17 => isr17,
            18 => isr18,
            19 => isr19,
            20 => isr20,
            21 => isr21,
            22 => isr22,
            23 => isr23,
            24 => isr24,
            25 => isr25,
            26 => isr26,
            27 => isr27,
            28 => isr28,
            29 => isr29,
            30 => isr30,
            31 => isr31,
            32 => isr32,
            33 => isr33,
            34 => isr34,
            35 => isr35,
            36 => isr36,
            37 => isr37,
            38 => isr38,
            39 => isr39,
            40 => isr40,
            41 => isr41,
            42 => isr42,
            43 => isr43,
            44 => isr44,
            45 => isr45,
            46 => isr46,
            47 => isr47,
            _ => panic!("Invalid ISR number"),
        }
    }
}

// Define the interrupt frame structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    // Pushed by our ISR stubs
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub esp_dummy: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
    pub es: u32,
    pub ds: u32,
    
    // Pushed by CPU on interrupt
    pub int_no: u32,      // Added by our stub
    pub err_code: u32,    // Error code (0 if not applicable)
    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
    pub esp: u32,  // Only if privilege level changed
    pub ss: u32,   // Only if privilege level changed
}

// Rust interrupt handlers
pub static mut TIMER_TICKS: u64 = 0;

/// Timer interrupt handler (IRQ0)
#[unsafe(no_mangle)]
pub extern "C" fn timer_interrupt_handler() {
    unsafe {
        // Immediately write to serial to confirm we reached Rust code
        SERIAL_PORT.write_str("[TMR]");
        
        TIMER_TICKS += 1;
        
        // Only print first few ticks
        if TIMER_TICKS <= 3 {
            SERIAL_PORT.write_decimal(TIMER_TICKS as u32);
            SERIAL_PORT.write_str(" ");
        }
        
        // Dot every second
        if TIMER_TICKS % 100 == 0 {
            SERIAL_PORT.write_str(".");
        }
    }
}

/// Keyboard interrupt handler (IRQ1)
#[unsafe(no_mangle)]
pub extern "C" fn keyboard_interrupt_handler() {
    unsafe {
        SERIAL_PORT.write_str("[KBD]");
        
        // Read scan code from keyboard controller
        let scancode: u8;
        asm!("in al, 0x60", out("al") scancode);
        
        SERIAL_PORT.write_hex(scancode as u32);
        SERIAL_PORT.write_str(" ");
        
        handle_keyboard_input(scancode);
    }
}

/// General Protection Fault handler
#[unsafe(no_mangle)]
pub extern "C" fn gpf_handler(error_code: u32) {
    unsafe {
        SERIAL_PORT.write_str("\n!!! GENERAL PROTECTION FAULT !!!\n");
        SERIAL_PORT.write_str("Error code: 0x");
        SERIAL_PORT.write_hex(error_code);
        SERIAL_PORT.write_str("\n");
        
        // Extract error code fields
        let external = (error_code & 1) != 0;
        let table = (error_code >> 1) & 0b11;
        let index = (error_code >> 3) & 0x1FFF;
        
        SERIAL_PORT.write_str("External: ");
        if external { SERIAL_PORT.write_str("yes"); } else { SERIAL_PORT.write_str("no"); }
        SERIAL_PORT.write_str("\nTable: ");
        match table {
            0b00 => SERIAL_PORT.write_str("GDT"),
            0b01 | 0b11 => SERIAL_PORT.write_str("IDT"),
            0b10 => SERIAL_PORT.write_str("LDT"),
            _ => SERIAL_PORT.write_str("?"),
        }
        SERIAL_PORT.write_str("\nIndex: ");
        SERIAL_PORT.write_decimal(index as u32);
        SERIAL_PORT.write_str("\n");
    }
    
    // Halt after GPF
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}

/// Double Fault handler
#[unsafe(no_mangle)]
pub extern "C" fn double_fault_handler() {
    unsafe {
        SERIAL_PORT.write_str("\n!!! DOUBLE FAULT !!!\n");
        SERIAL_PORT.write_str("System halted.\n");
    }
    
    // Double fault is always fatal
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}

/// Default handler for unhandled interrupts
#[unsafe(no_mangle)]
pub extern "C" fn default_interrupt_handler() {
    unsafe {
        SERIAL_PORT.write_str("[UNK_INT]");
    }
}

// Helper function for keyboard handling
fn handle_keyboard_input(scancode: u8) {
    // Map scancode to ASCII or handle special keys
    match scancode {
        0x01 => unsafe { SERIAL_PORT.write_str("[ESC]"); },
        0x1C => unsafe { SERIAL_PORT.write_str("[ENTER]"); },
        0x0E => unsafe { SERIAL_PORT.write_str("[BACKSPACE]"); },
        _ => {}
    }
}