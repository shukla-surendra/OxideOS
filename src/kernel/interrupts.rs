// src/kernel/interrupts.rs - Combined interrupt and exception handling
#![no_std]

use core::arch::global_asm;
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::pic;

// ============================================================================
// GLOBAL STATE
// ============================================================================

pub static mut TIMER_TICKS: u64 = 0;

// ============================================================================
// INTERRUPT FRAME STRUCTURE
// ============================================================================

#[repr(C)]
pub struct InterruptFrame {
    // Pushed by our assembly stub (pushad)
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub esp_dummy: u32,
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,
    // Pushed by our stub
    pub int_no: u32,
    pub err_code: u32,
    // Pushed by CPU
    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
    // Only present if privilege level changed
    pub esp: u32,
    pub ss: u32,
}

// ============================================================================
// MAIN INTERRUPT HANDLER
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn isr_common_handler(regs_ptr: *const InterruptFrame, int_no: u32, err_code: u32) {
    unsafe {
        // Quick acknowledgment we reached handler
        SERIAL_PORT.write_str("[");
        SERIAL_PORT.write_decimal(int_no);
        SERIAL_PORT.write_str("]");
        
        match int_no {
            // CPU Exceptions (0-31)
            0..=31 => handle_cpu_exception(regs_ptr, int_no, err_code),
            
            // Timer IRQ (32 = IRQ0)
            32 => {
                TIMER_TICKS += 1;
                if TIMER_TICKS <= 5 || TIMER_TICKS % 100 == 0 {
                    SERIAL_PORT.write_str("T");
                }
                pic::send_eoi(0);
            },
            
            // Keyboard IRQ (33 = IRQ1)
            33 => {
                let scancode: u8;
                asm!("in al, 0x60", out("al") scancode);
                SERIAL_PORT.write_str("K:");
                SERIAL_PORT.write_hex(scancode as u32);
                SERIAL_PORT.write_str(" ");
                handle_keyboard_scancode(scancode);
                pic::send_eoi(1);
            },
            
            // Other hardware IRQs (34-47 = IRQ2-15)
            34..=47 => {
                SERIAL_PORT.write_str("IRQ");
                SERIAL_PORT.write_decimal(int_no - 32);
                pic::send_eoi((int_no - 32) as u8);
            },
            
            // Unknown interrupt
            _ => {
                SERIAL_PORT.write_str("?INT");
                SERIAL_PORT.write_decimal(int_no);
            }
        }
    }
}

// ============================================================================
// CPU EXCEPTION HANDLER
// ============================================================================

fn handle_cpu_exception(regs_ptr: *const InterruptFrame, int_no: u32, err_code: u32) -> ! {
    unsafe {
        let regs = &*regs_ptr;
        
        SERIAL_PORT.write_str("\n\n=== CPU EXCEPTION ===\n");
        SERIAL_PORT.write_str("Exception #");
        SERIAL_PORT.write_decimal(int_no);
        
        let name = match int_no {
            0 => " (Divide by Zero)",
            1 => " (Debug)",
            2 => " (NMI)",
            3 => " (Breakpoint)",
            4 => " (Overflow)",
            5 => " (Bound Range Exceeded)",
            6 => " (Invalid Opcode)",
            7 => " (Device Not Available)",
            8 => " (Double Fault)",
            9 => " (Coprocessor Segment Overrun)",
            10 => " (Invalid TSS)",
            11 => " (Segment Not Present)",
            12 => " (Stack Segment Fault)",
            13 => " (General Protection Fault)",
            14 => " (Page Fault)",
            15 => " (Reserved)",
            16 => " (x87 FPU Error)",
            17 => " (Alignment Check)",
            18 => " (Machine Check)",
            19 => " (SIMD Floating-Point)",
            _ => " (Reserved/Unknown)",
        };
        
        SERIAL_PORT.write_str(name);
        SERIAL_PORT.write_str("\nError Code: 0x");
        SERIAL_PORT.write_hex(err_code);
        
        // Register dump
        SERIAL_PORT.write_str("\n\nRegisters:\n");
        SERIAL_PORT.write_str("EAX=0x"); SERIAL_PORT.write_hex(regs.eax);
        SERIAL_PORT.write_str(" EBX=0x"); SERIAL_PORT.write_hex(regs.ebx);
        SERIAL_PORT.write_str(" ECX=0x"); SERIAL_PORT.write_hex(regs.ecx);
        SERIAL_PORT.write_str(" EDX=0x"); SERIAL_PORT.write_hex(regs.edx);
        SERIAL_PORT.write_str("\nESI=0x"); SERIAL_PORT.write_hex(regs.esi);
        SERIAL_PORT.write_str(" EDI=0x"); SERIAL_PORT.write_hex(regs.edi);
        SERIAL_PORT.write_str(" EBP=0x"); SERIAL_PORT.write_hex(regs.ebp);
        SERIAL_PORT.write_str(" ESP=0x"); SERIAL_PORT.write_hex(regs.esp_dummy);
        SERIAL_PORT.write_str("\nEIP=0x"); SERIAL_PORT.write_hex(regs.eip);
        SERIAL_PORT.write_str(" CS=0x"); SERIAL_PORT.write_hex(regs.cs);
        SERIAL_PORT.write_str(" EFLAGS=0x"); SERIAL_PORT.write_hex(regs.eflags);
        
        // Special handling for specific exceptions
        match int_no {
            13 => {
                // GPF - decode selector error code
                if err_code != 0 {
                    SERIAL_PORT.write_str("\n\nGPF Details:");
                    SERIAL_PORT.write_str("\n  External: ");
                    SERIAL_PORT.write_str(if err_code & 1 != 0 { "yes" } else { "no" });
                    SERIAL_PORT.write_str("\n  Table: ");
                    match (err_code >> 1) & 0b11 {
                        0b00 => SERIAL_PORT.write_str("GDT"),
                        0b01 | 0b11 => SERIAL_PORT.write_str("IDT"),
                        0b10 => SERIAL_PORT.write_str("LDT"),
                        _ => SERIAL_PORT.write_str("?"),
                    }
                    SERIAL_PORT.write_str("\n  Index: ");
                    SERIAL_PORT.write_decimal(((err_code >> 3) & 0x1FFF) as u32);
                }
            },
            14 => {
                // Page Fault - get faulting address from CR2
                let cr2: u32;
                asm!("mov {}, cr2", out(reg) cr2);
                SERIAL_PORT.write_str("\n\nPage Fault Details:");
                SERIAL_PORT.write_str("\n  Faulting Address (CR2): 0x");
                SERIAL_PORT.write_hex(cr2);
                SERIAL_PORT.write_str("\n  Cause: ");
                SERIAL_PORT.write_str(if err_code & 1 != 0 { "Protection violation" } else { "Page not present" });
                SERIAL_PORT.write_str(", ");
                SERIAL_PORT.write_str(if err_code & 2 != 0 { "Write" } else { "Read" });
                SERIAL_PORT.write_str(", ");
                SERIAL_PORT.write_str(if err_code & 4 != 0 { "User mode" } else { "Kernel mode" });
            },
            _ => {}
        }
        
        SERIAL_PORT.write_str("\n\n=== SYSTEM HALTED ===\n");
    }
    
    // Halt forever
    unsafe {
        asm!("cli");
        loop {
            asm!("hlt");
        }
    }
}

// ============================================================================
// KEYBOARD HANDLER
// ============================================================================

fn handle_keyboard_scancode(scancode: u8) {
    unsafe {
        // Basic scancode to ASCII mapping for common keys
        let key = match scancode {
            0x01 => Some("[ESC]"),
            0x0E => Some("[BACKSPACE]"),
            0x0F => Some("[TAB]"),
            0x1C => Some("[ENTER]"),
            0x1D => Some("[CTRL]"),
            0x2A => Some("[LSHIFT]"),
            0x36 => Some("[RSHIFT]"),
            0x38 => Some("[ALT]"),
            0x39 => Some("[SPACE]"),
            0x3A => Some("[CAPS]"),
            0x3B..=0x44 => Some("[F1-F10]"),
            0x45 => Some("[NUMLOCK]"),
            0x46 => Some("[SCROLL]"),
            _ => None,
        };
        
        if let Some(key_name) = key {
            SERIAL_PORT.write_str(key_name);
        }
    }
}

// ============================================================================
// ASSEMBLY INTERRUPT STUBS
// ============================================================================

global_asm!(r#"
.section .text
.intel_syntax noprefix

// Macro for interrupts without error code
.macro ISR_NOERR name num
    .global \name
\name:
    push 0          // Dummy error code for uniform stack layout
    push \num       // Interrupt number
    pushad          // Save all general registers (EAX, ECX, EDX, EBX, ESP, EBP, ESI, EDI)
    
    // Call handler with (regs_ptr, int_no, err_code)
    mov eax, esp
    push dword ptr [esp + 36]  // error code (at esp+36 after pushad)
    push dword ptr [esp + 36]  // interrupt number (was at esp+32, now esp+36 after push)
    push eax                    // regs pointer
    
    call isr_common_handler
    
    add esp, 12     // Clean up 3 pushed arguments
    popad           // Restore all general registers
    add esp, 8      // Remove int_no and error code
    iret            // Return from interrupt
.endm

// Macro for interrupts with error code (pushed by CPU)
.macro ISR_WITHERR name num
    .global \name
\name:
    push \num       // Interrupt number (error code already on stack from CPU)
    pushad          // Save all general registers
    
    // Call handler with (regs_ptr, int_no, err_code)
    mov eax, esp
    push dword ptr [esp + 36]  // error code (at esp+36 after pushad)
    push dword ptr [esp + 32]  // interrupt number (at esp+32 after pushad)
    push eax                    // regs pointer
    
    call isr_common_handler
    
    add esp, 12     // Clean up 3 pushed arguments
    popad           // Restore all general registers
    add esp, 8      // Remove int_no and error code
    iret            // Return from interrupt
.endm

// CPU Exceptions (0-31)
ISR_NOERR isr0 0      // Divide by zero
ISR_NOERR isr1 1      // Debug
ISR_NOERR isr2 2      // NMI
ISR_NOERR isr3 3      // Breakpoint
ISR_NOERR isr4 4      // Overflow
ISR_NOERR isr5 5      // Bound range exceeded
ISR_NOERR isr6 6      // Invalid opcode
ISR_NOERR isr7 7      // Device not available
ISR_WITHERR isr8 8    // Double fault (has error code)
ISR_NOERR isr9 9      // Coprocessor segment overrun
ISR_WITHERR isr10 10  // Invalid TSS (has error code)
ISR_WITHERR isr11 11  // Segment not present (has error code)
ISR_WITHERR isr12 12  // Stack segment fault (has error code)
ISR_WITHERR isr13 13  // General protection fault (has error code)
ISR_WITHERR isr14 14  // Page fault (has error code)
ISR_NOERR isr15 15    // Reserved
ISR_NOERR isr16 16    // x87 FPU error
ISR_WITHERR isr17 17  // Alignment check (has error code)
ISR_NOERR isr18 18    // Machine check
ISR_NOERR isr19 19    // SIMD floating-point
ISR_NOERR isr20 20    // Virtualization
ISR_NOERR isr21 21    // Reserved
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

// Hardware IRQs (32-47)
ISR_NOERR isr32 32    // Timer (IRQ0)
ISR_NOERR isr33 33    // Keyboard (IRQ1)
ISR_NOERR isr34 34    // Cascade (IRQ2)
ISR_NOERR isr35 35    // COM2 (IRQ3)
ISR_NOERR isr36 36    // COM1 (IRQ4)
ISR_NOERR isr37 37    // LPT2 (IRQ5)
ISR_NOERR isr38 38    // Floppy (IRQ6)
ISR_NOERR isr39 39    // LPT1/Spurious (IRQ7)
ISR_NOERR isr40 40    // RTC (IRQ8)
ISR_NOERR isr41 41    // Free (IRQ9)
ISR_NOERR isr42 42    // Free (IRQ10)
ISR_NOERR isr43 43    // Free (IRQ11)
ISR_NOERR isr44 44    // Mouse (IRQ12)
ISR_NOERR isr45 45    // FPU/Coprocessor (IRQ13)
ISR_NOERR isr46 46    // Primary ATA (IRQ14)
ISR_NOERR isr47 47    // Secondary ATA (IRQ15)

.att_syntax prefix
"#);

// ============================================================================
// DEBUG AND VERIFICATION FUNCTIONS
// ============================================================================

/// Verify that ISR handlers are at valid addresses
pub fn verify_handlers() {
    unsafe {
        SERIAL_PORT.write_str("Verifying ISR addresses:\n");
        
        // Check a few key handlers
        SERIAL_PORT.write_str("  isr0 (Divide by Zero) at: 0x");
        SERIAL_PORT.write_hex(isr0 as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  isr13 (GPF) at: 0x");
        SERIAL_PORT.write_hex(isr13 as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  isr14 (Page Fault) at: 0x");
        SERIAL_PORT.write_hex(isr14 as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  isr32 (Timer/IRQ0) at: 0x");
        SERIAL_PORT.write_hex(isr32 as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("  isr33 (Keyboard/IRQ1) at: 0x");
        SERIAL_PORT.write_hex(isr33 as usize as u32);
        SERIAL_PORT.write_str("\n");
        
        // Verify addresses are in reasonable range (above 1MB, below 16MB for typical kernel)
        let timer_addr = isr32 as usize as u32;
        if timer_addr < 0x100000 || timer_addr > 0x1000000 {
            SERIAL_PORT.write_str("  WARNING: ISR addresses may be invalid!\n");
        } else {
            SERIAL_PORT.write_str("  ISR addresses look valid\n");
        }
    }
}

// ============================================================================
// ISR FUNCTION DECLARATIONS
// ============================================================================

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