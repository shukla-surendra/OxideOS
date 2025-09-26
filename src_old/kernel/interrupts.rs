// src/kernel/interrupts.rs - CORRECTED interrupt and exception handling
#![no_std]

use core::arch::global_asm;
use core::arch::asm;
use crate::kernel::interrupts_asm;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::pic;

// ============================================================================
// GLOBAL STATE
// ============================================================================

pub static mut TIMER_TICKS: u64 = 0;

// ============================================================================
// INTERRUPT FRAME STRUCTURE - CORRECTED
// ============================================================================

#[repr(C)]
pub struct InterruptFrame {
    // Pushed by our assembly stub (pushad) - in FORWARD order (EAX is pushed last)
    pub edi: u32,
    pub esi: u32,
    pub ebp: u32,
    pub esp_dummy: u32,  // ESP value, but not useful
    pub ebx: u32,
    pub edx: u32,
    pub ecx: u32,
    pub eax: u32,        // This is at ESP when we enter our handler
    // Pushed by our stub BEFORE pushad
    pub int_no: u32,
    pub err_code: u32,
    // Pushed by CPU during interrupt BEFORE our stub
    pub eip: u32,
    pub cs: u32,
    pub eflags: u32,
}

// ============================================================================
// MINIMAL TEST HANDLER
// ============================================================================

#[unsafe(no_mangle)]
pub extern "C" fn minimal_test_handler() {
    unsafe {
        SERIAL_PORT.write_str("MIN_HANDLER ");
        // Don't touch anything else, just return
    }
}

// src/kernel/interrupts.rs
// ... (imports and global state unchanged)

// MAIN INTERRUPT HANDLER
#[unsafe(no_mangle)]
pub extern "C" fn isr_common_handler(frame: *mut InterruptFrame) {
    unsafe {
        let int_no = (*frame).int_no;
        let err_code = (*frame).err_code;

        // Debug: Show what we're reading
        if TIMER_TICKS < 5 {
            SERIAL_PORT.write_str("INT#");
            SERIAL_PORT.write_decimal(int_no);
            SERIAL_PORT.write_str(" ERR:");
            SERIAL_PORT.write_hex(err_code);
            SERIAL_PORT.write_str("\n");
        }
        
        // Validate interrupt number
        if int_no > 255 {
            SERIAL_PORT.write_str("INVALID_INT:");
            SERIAL_PORT.write_decimal(int_no);
            SERIAL_PORT.write_str(" HALT");
            asm!("cli");
            loop { asm!("hlt"); }
        }
        
        match int_no {
            0..=31 => {
                // CPU exception
                SERIAL_PORT.write_str("EXC");
                SERIAL_PORT.write_decimal(int_no);
                SERIAL_PORT.write_str(" ");
                handle_cpu_exception_simple(int_no, err_code, (*frame).esp_dummy);
                return;
            },
            32 => {
                // Timer interrupt
                TIMER_TICKS += 1;
                if TIMER_TICKS <= 10 || TIMER_TICKS % 100 == 0 {
                    SERIAL_PORT.write_str("T");
                    SERIAL_PORT.write_decimal(TIMER_TICKS as u32);
                    SERIAL_PORT.write_str(" ");
                }
                if TIMER_TICKS < 5 {
                    SERIAL_PORT.write_str("InterruptFrame: ");
                    SERIAL_PORT.write_str("EDI: 0x"); SERIAL_PORT.write_hex((*frame).edi);
                    SERIAL_PORT.write_str(" ESI: 0x"); SERIAL_PORT.write_hex((*frame).esi);
                    SERIAL_PORT.write_str(" EBP: 0x"); SERIAL_PORT.write_hex((*frame).ebp);
                    SERIAL_PORT.write_str(" ESP: 0x"); SERIAL_PORT.write_hex((*frame).esp_dummy);
                    SERIAL_PORT.write_str(" EBX: 0x"); SERIAL_PORT.write_hex((*frame).ebx);
                    SERIAL_PORT.write_str(" EDX: 0x"); SERIAL_PORT.write_hex((*frame).edx);
                    SERIAL_PORT.write_str(" ECX: 0x"); SERIAL_PORT.write_hex((*frame).ecx);
                    SERIAL_PORT.write_str(" EAX: 0x"); SERIAL_PORT.write_hex((*frame).eax);
                    SERIAL_PORT.write_str("\n");
                }
                pic::send_eoi(0);
            },
            33 => {
                // Keyboard interrupt
                let scancode: u8;
                asm!("in al, 0x60", out("al") scancode);
                SERIAL_PORT.write_str("K");
                SERIAL_PORT.write_hex(scancode as u32);
                SERIAL_PORT.write_str(" ");
                pic::send_eoi(1);
            },
            34..=47 => {
                // Other hardware IRQs
                SERIAL_PORT.write_str("I");
                SERIAL_PORT.write_decimal(int_no);
                SERIAL_PORT.write_str(" ");
                if int_no >= 40 {
                    pic::send_eoi((int_no - 32) as u8);
                } else {
                    pic::send_eoi(0);
                }
            },
            _ => {
                // Software interrupts or spurious
                SERIAL_PORT.write_str("S");
                SERIAL_PORT.write_decimal(int_no);
                SERIAL_PORT.write_str(" ");
            }
        }
    }
}

// SIMPLIFIED CPU EXCEPTION HANDLER
fn handle_cpu_exception_simple(int_no: u32, err_code: u32, esp: u32) -> ! {
    unsafe {
        SERIAL_PORT.write_str("=== CPU EXCEPTION ===\n");
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
        SERIAL_PORT.write_str("\n Error Code: 0x");
        SERIAL_PORT.write_hex(err_code);
        SERIAL_PORT.write_str("\n ESP: 0x");
        SERIAL_PORT.write_hex(esp);
        
        // Corrected EIP read
        let stack = esp as *const u32;
        let eip = *stack.add(2); // EIP is 2 u32s up from esp_dummy (int_no -> err_code -> eip)
        SERIAL_PORT.write_str("\n EIP: 0x");
        SERIAL_PORT.write_hex(eip);
        
        SERIAL_PORT.write_str(" === SYSTEM HALTED ===\n");
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
// DEBUG AND VERIFICATION FUNCTIONS
// ============================================================================

/// Verify that ISR handlers are at valid addresses
pub fn verify_handlers() {
    unsafe {
        SERIAL_PORT.write_str("Verifying ISR addresses:");
        
        // Check a few key handlers
        SERIAL_PORT.write_str("  isr0 (Divide by Zero) at: ");
        SERIAL_PORT.write_hex(isr0 as usize as u32);
        
        SERIAL_PORT.write_str("  isr13 (GPF) at: ");
        SERIAL_PORT.write_hex(isr13 as usize as u32);
        
        SERIAL_PORT.write_str("  isr14 (Page Fault) at: ");
        SERIAL_PORT.write_hex(isr14 as usize as u32);
        
        SERIAL_PORT.write_str("  isr32 (Timer/IRQ0) at: ");
        SERIAL_PORT.write_hex(isr32 as usize as u32);
        
        SERIAL_PORT.write_str("  isr33 (Keyboard/IRQ1) at: ");
        SERIAL_PORT.write_hex(isr33 as usize as u32);
        
        // Verify addresses are in reasonable range
        let timer_addr = isr32 as usize as u32;
        if timer_addr < 0x100000 || timer_addr > 0x1000000 {
            SERIAL_PORT.write_str("  WARNING: ISR addresses may be invalid!");
        } else {
            SERIAL_PORT.write_str("  ISR addresses look valid");
        }
    }
}

// ============================================================================
// ISR FUNCTION DECLARATIONS
// ============================================================================

unsafe extern "C" {
    pub unsafe fn test_timer_isr();
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