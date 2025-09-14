// interrupts.rs - Modified with extensive debugging

use core::arch::global_asm;
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;

// Assembly trampolines with debug output
global_asm!(r#"
    .section .text
    .intel_syntax noprefix

    /* Timer interrupt (IRQ0/ISR32) - with debug */
    .global isr32
    .type isr32, @function
isr32:
    /* Debug: Signal entry */
    push eax
    push edx
    mov dx, 0x3F8  /* Serial port */
    mov al, 'T'
    out dx, al
    pop edx
    pop eax
    
    /* Save all registers we might clobber */
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi
    push ds
    push es
    
    /* Load kernel data segment */
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    
    /* Call Rust timer handler */
    call timer_interrupt_handler
    
    /* Send EOI to PIC */
    mov al, 0x20
    out 0x20, al
    
    /* Restore registers */
    pop es
    pop ds
    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax
    
    /* Debug: Signal exit */
    push eax
    push edx
    mov dx, 0x3F8
    mov al, 't'
    out dx, al
    pop edx
    pop eax
    
    iret

    /* Keyboard interrupt (IRQ1/ISR33) */
    .global isr33
    .type isr33, @function
isr33:
    /* Debug marker */
    push eax
    push edx
    mov dx, 0x3F8
    mov al, 'K'
    out dx, al
    pop edx
    pop eax
    
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi
    push ds
    push es
    
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    
    /* Call Rust keyboard handler */
    call keyboard_interrupt_handler
    
    /* Send EOI to PIC */
    mov al, 0x20
    out 0x20, al
    
    pop es
    pop ds
    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax
    
    push eax
    push edx
    mov dx, 0x3F8
    mov al, 'k'
    out dx, al
    pop edx
    pop eax
    
    iret

    /* General Protection Fault (ISR13) */
    .global isr13
    .type isr13, @function
isr13:
    /* Debug: GPF occurred! */
    push eax
    push edx
    mov dx, 0x3F8
    mov al, 'G'
    out dx, al
    mov al, 'P'
    out dx, al
    mov al, 'F'
    out dx, al
    mov al, '!'
    out dx, al
    pop edx
    pop eax
    
    /* Error code is already on stack */
    push eax
    push ecx
    push edx
    push ebx
    push ebp
    push esi
    push edi
    push ds
    push es
    
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    
    /* Pass error code to Rust handler */
    mov eax, [esp + 40]  /* Get error code (after all pushes) */
    push eax
    call gpf_handler
    add esp, 4
    
    pop es
    pop ds
    pop edi
    pop esi
    pop ebp
    pop ebx
    pop edx
    pop ecx
    pop eax
    add esp, 4  /* Remove error code */
    iret

    /* Double Fault (ISR8) */
    .global isr8
    .type isr8, @function
isr8:
    /* Debug: Double fault! */
    push eax
    push edx
    mov dx, 0x3F8
    mov al, 'D'
    out dx, al
    mov al, 'F'
    out dx, al
    mov al, '!'
    out dx, al
    pop edx
    pop eax
    
    push eax
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    call double_fault_handler
    
    /* Double fault is fatal - halt */
    cli
halt_loop:
    hlt
    jmp halt_loop

    /* Default ISR for unhandled interrupts */
    .global default_isr
    .type default_isr, @function
default_isr:
    /* Debug: Unexpected interrupt */
    push eax
    push edx
    mov dx, 0x3F8
    mov al, '?'
    out dx, al
    pop edx
    pop eax
    
    push eax
    push ecx
    push edx
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    call default_interrupt_handler
    
    pop ds
    pop edx
    pop ecx
    pop eax
    iret

    .att_syntax
"#);

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