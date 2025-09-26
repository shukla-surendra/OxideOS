// src/kernel/interrupts.rs - Complete 64-bit interrupt handling
use core::arch::asm;
use crate::kernel::serial::SERIAL_PORT;
use crate::kernel::pic;
use super::interrupts_asm;

// ============================================================================
// GLOBAL STATE
// ============================================================================

pub static mut TIMER_TICKS: u64 = 0;

// ============================================================================
// 64-BIT INTERRUPT FRAME STRUCTURE
// ============================================================================

#[repr(C)]
#[derive(Debug)]
pub struct InterruptFrame {
    // Saved by our assembly stub (pushed in reverse order, so r15 is first)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    
    // Pushed by our stub before calling handler
    pub int_no: u64,
    pub err_code: u64,
    
    // Pushed by CPU during interrupt (always present in 64-bit)
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,    // User stack pointer (if privilege change)
    pub ss: u64,     // User stack segment (if privilege change)
}

// ============================================================================
// MAIN INTERRUPT HANDLER - 64-bit version
// ============================================================================

/// Main interrupt dispatcher for 64-bit mode
/// Called from assembly stub with pointer to interrupt frame
#[unsafe(no_mangle)]
pub extern "C" fn isr_common_handler(frame: *mut InterruptFrame) {
    unsafe {
        if frame.is_null() {
            SERIAL_PORT.write_str("ERROR: Null interrupt frame!\n");
            halt_system();
        }

        let int_no = (*frame).int_no;
        let err_code = (*frame).err_code;

        // Validate interrupt number
        if int_no > 255 {
            SERIAL_PORT.write_str("FATAL: Invalid 64-bit interrupt number: ");
            SERIAL_PORT.write_decimal(int_no as u32);
            SERIAL_PORT.write_str("\n");
            halt_system();
        }

        // Debug output for early interrupts (reduced spam)
        if (int_no != 32 && TIMER_TICKS < 5) || (int_no == 32 && TIMER_TICKS < 3) {
            SERIAL_PORT.write_str("[64-INT:");
            SERIAL_PORT.write_decimal(int_no as u32);
            if err_code != 0 {
                SERIAL_PORT.write_str(" ERR:0x");
                SERIAL_PORT.write_hex(err_code as u32);
            }
            SERIAL_PORT.write_str("] ");
        }

        // Dispatch to specific handlers
        match int_no {
            0..=31 => {
                // CPU exceptions
                handle_cpu_exception_64(int_no, err_code, frame);
            },
            32 => {
                // Timer interrupt (IRQ0)
                handle_timer_interrupt();
                pic::send_eoi(0);
            },
            33 => {
                // Keyboard interrupt (IRQ1)
                handle_keyboard_interrupt();
                pic::send_eoi(1);
            },
            34..=47 => {
                // Other hardware IRQs
                handle_hardware_irq(int_no);
                // Send EOI to appropriate PIC
                if int_no >= 40 {
                    pic::send_eoi((int_no - 32) as u8); // Slave PIC
                } else {
                    pic::send_eoi(0); // Master PIC only
                }
            },
            48..=127 => {
                // Reserved/unused
                SERIAL_PORT.write_str("WARN: Reserved interrupt ");
                SERIAL_PORT.write_decimal(int_no as u32);
                SERIAL_PORT.write_str("\n");
            },
            128 => {
                // System call interrupt (int 0x80)
                handle_system_call(frame);
            },
            129..=255 => {
                // Software interrupts or spurious
                SERIAL_PORT.write_str("SW-INT:");
                SERIAL_PORT.write_decimal(int_no as u32);
                SERIAL_PORT.write_str(" ");
            },
            256..=u64::MAX => {
                // Invalid interrupt numbers - should never happen due to earlier validation
                SERIAL_PORT.write_str("FATAL: Invalid interrupt number beyond 255: ");
                SERIAL_PORT.write_decimal(int_no as u32);
                SERIAL_PORT.write_str("\n");
                halt_system();
            }
        }
    }
}

// ============================================================================
// SPECIFIC INTERRUPT HANDLERS
// ============================================================================

/// Handle timer interrupt (IRQ0)
unsafe fn handle_timer_interrupt() {
    TIMER_TICKS += 1;
    
    // Periodic output to show system is alive
    if TIMER_TICKS <= 10 || TIMER_TICKS % 100 == 0 {
        SERIAL_PORT.write_str("T64:");
        SERIAL_PORT.write_decimal(TIMER_TICKS as u32);
        SERIAL_PORT.write_str(" ");
    }
    
    // Detailed debug for first few ticks
    if TIMER_TICKS <= 3 {
        SERIAL_PORT.write_str("(RSP in timer: ");
        let rsp: u64;
        asm!("mov {}, rsp", out(reg) rsp, options(nomem, nostack, preserves_flags));
        SERIAL_PORT.write_hex((rsp >> 32) as u32);
        SERIAL_PORT.write_hex(rsp as u32);
        SERIAL_PORT.write_str(") ");
    }
}

/// Handle keyboard interrupt (IRQ1)
unsafe fn handle_keyboard_interrupt() {
    // Read scancode from keyboard controller
    let scancode: u8;
    asm!("in al, 0x60", out("al") scancode, options(nostack, nomem));
    
    SERIAL_PORT.write_str("K64:0x");
    SERIAL_PORT.write_hex(scancode as u32);
    SERIAL_PORT.write_str(" ");
    
    // Basic key processing (just for demonstration)
    match scancode {
        0x1E => SERIAL_PORT.write_str("(A) "), // A key
        0x30 => SERIAL_PORT.write_str("(B) "), // B key
        0x2E => SERIAL_PORT.write_str("(C) "), // C key
        0x01 => SERIAL_PORT.write_str("(ESC) "), // Escape
        0x1C => SERIAL_PORT.write_str("(ENTER) "), // Enter
        _ => {} // Other keys
    }
}

/// Handle other hardware IRQs
unsafe fn handle_hardware_irq(int_no: u64) {
    let irq_num = int_no - 32;
    SERIAL_PORT.write_str("HW-IRQ:");
    SERIAL_PORT.write_decimal(irq_num as u32);
    SERIAL_PORT.write_str(" ");
    
    // Handle specific IRQs if needed
    match irq_num {
        2 => { /* Cascade - should never happen */ }
        3 => { /* COM2 */ }
        4 => { /* COM1 */ }
        5 => { /* LPT2 */ }
        6 => { /* Floppy */ }
        7 => { /* LPT1 */ }
        8 => { /* RTC */ }
        12 => { /* PS/2 Mouse */ }
        14 => { /* Primary ATA */ }
        15 => { /* Secondary ATA */ }
        _ => { /* Other IRQ */ }
    }
}

/// Handle system call (int 0x80) - basic implementation
unsafe fn handle_system_call(frame: *mut InterruptFrame) {
    // In 64-bit, system call number typically in RAX
    let syscall_num = (*frame).rax;
    let arg1 = (*frame).rdi;
    let arg2 = (*frame).rsi;
    let arg3 = (*frame).rdx;
    
    SERIAL_PORT.write_str("SYSCALL:");
    SERIAL_PORT.write_decimal(syscall_num as u32);
    SERIAL_PORT.write_str(" ");
    
    match syscall_num {
        0 => {
            // Example: sys_write
            SERIAL_PORT.write_str("(write) ");
        },
        1 => {
            // Example: sys_exit
            SERIAL_PORT.write_str("(exit) ");
        },
        _ => {
            SERIAL_PORT.write_str("(unknown) ");
            (*frame).rax = u64::MAX; // Return error
        }
    }
}

// ============================================================================
// CPU EXCEPTION HANDLER - 64-bit version
// ============================================================================

/// Handle CPU exceptions with detailed 64-bit information
fn handle_cpu_exception_64(int_no: u64, err_code: u64, frame: *mut InterruptFrame) -> ! {
    unsafe {
        SERIAL_PORT.write_str("\n=== 64-BIT CPU EXCEPTION ===\n");
        SERIAL_PORT.write_str("Exception #");
        SERIAL_PORT.write_decimal(int_no as u32);
        
        let name = match int_no {
            0 => " (Divide by Zero)",
            1 => " (Debug)",
            2 => " (Non-Maskable Interrupt)",
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
            19 => " (SIMD Floating-Point Exception)",
            20 => " (Virtualization Exception)",
            21 => " (Control Protection Exception)",
            _ => " (Reserved/Unknown)",
        };
        
        SERIAL_PORT.write_str(name);
        SERIAL_PORT.write_str("\n");
        
        // Error code
        SERIAL_PORT.write_str("Error Code: 0x");
        SERIAL_PORT.write_hex((err_code >> 32) as u32);
        SERIAL_PORT.write_hex(err_code as u32);
        SERIAL_PORT.write_str("\n");
        
        // Register dump
        SERIAL_PORT.write_str("RIP: 0x");
        SERIAL_PORT.write_hex(((*frame).rip >> 32) as u32);
        SERIAL_PORT.write_hex((*frame).rip as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("RSP: 0x");
        SERIAL_PORT.write_hex(((*frame).rsp >> 32) as u32);
        SERIAL_PORT.write_hex((*frame).rsp as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("RBP: 0x");
        SERIAL_PORT.write_hex(((*frame).rbp >> 32) as u32);
        SERIAL_PORT.write_hex((*frame).rbp as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("RAX: 0x");
        SERIAL_PORT.write_hex(((*frame).rax >> 32) as u32);
        SERIAL_PORT.write_hex((*frame).rax as u32);
        SERIAL_PORT.write_str("\n");
        
        SERIAL_PORT.write_str("CS: 0x");
        SERIAL_PORT.write_hex((*frame).cs as u32);
        SERIAL_PORT.write_str(" RFLAGS: 0x");
        SERIAL_PORT.write_hex(((*frame).rflags >> 32) as u32);
        SERIAL_PORT.write_hex((*frame).rflags as u32);
        SERIAL_PORT.write_str("\n");
        
        // Special handling for specific exceptions
        match int_no {
            14 => {
                // Page fault - read CR2 for fault address
                let fault_addr: u64;
                asm!("mov {}, cr2", out(reg) fault_addr, options(nomem, nostack, preserves_flags));
                SERIAL_PORT.write_str("Page Fault Address: 0x");
                SERIAL_PORT.write_hex((fault_addr >> 32) as u32);
                SERIAL_PORT.write_hex(fault_addr as u32);
                SERIAL_PORT.write_str("\n");
                
                // Decode error code for page fault
                SERIAL_PORT.write_str("Fault Type: ");
                if err_code & 1 != 0 { SERIAL_PORT.write_str("Protection "); } else { SERIAL_PORT.write_str("Non-present "); }
                if err_code & 2 != 0 { SERIAL_PORT.write_str("Write "); } else { SERIAL_PORT.write_str("Read "); }
                if err_code & 4 != 0 { SERIAL_PORT.write_str("User "); } else { SERIAL_PORT.write_str("Supervisor "); }
                SERIAL_PORT.write_str("\n");
            },
            13 => {
                // General Protection Fault
                if err_code != 0 {
                    SERIAL_PORT.write_str("Selector: 0x");
                    SERIAL_PORT.write_hex(err_code as u32);
                    SERIAL_PORT.write_str("\n");
                }
            },
            _ => {}
        }
        
        SERIAL_PORT.write_str("=== SYSTEM HALTED ===\n");
    }
    
    halt_system();
}

// ============================================================================
// UTILITY FUNCTIONS
// ============================================================================

/// Get current timer ticks (thread-safe read)
pub unsafe fn get_timer_ticks() -> u64 {
    TIMER_TICKS
}

/// Halt the system permanently
#[inline(never)]
fn halt_system() -> ! {
    unsafe {
        asm!("cli");  // Disable interrupts
        loop {
            asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Verify that ISR handlers are at valid addresses for 64-bit
pub fn verify_handlers() {
    unsafe {
        SERIAL_PORT.write_str("Verifying 64-bit ISR addresses:\n");
        
        // Check key handlers
        let addrs = [
            ("isr0 (Divide by Zero)", isr0 as usize as u64),
            ("isr3 (Breakpoint)", isr3 as usize as u64),
            ("isr13 (GPF)", isr13 as usize as u64),
            ("isr14 (Page Fault)", isr14 as usize as u64),
            ("isr32 (Timer)", isr32 as usize as u64),
            ("isr33 (Keyboard)", isr33 as usize as u64),
        ];
        
        for (name, addr) in addrs.iter() {
            SERIAL_PORT.write_str("  ");
            SERIAL_PORT.write_str(name);
            SERIAL_PORT.write_str(" at: 0x");
            SERIAL_PORT.write_hex((addr >> 32) as u32);
            SERIAL_PORT.write_hex(*addr as u32);
            SERIAL_PORT.write_str("\n");
        }
        
        // Verify addresses are in reasonable range for 64-bit kernel
        let first_addr = addrs[0].1;
        if first_addr < 0x100000 || first_addr >= 0x8000_0000_0000_0000 {
            SERIAL_PORT.write_str("  WARNING: 64-bit ISR addresses may be invalid!\n");
        } else {
            SERIAL_PORT.write_str("  64-bit ISR addresses look valid\n");
        }
    }
}

// ============================================================================
// ISR FUNCTION DECLARATIONS - External assembly functions
// ============================================================================

unsafe extern "C" {
    // CPU Exceptions (0-31)
    pub unsafe fn isr0();   pub unsafe fn isr1();   pub unsafe fn isr2();   pub unsafe fn isr3();
    pub unsafe fn isr4();   pub unsafe fn isr5();   pub unsafe fn isr6();   pub unsafe fn isr7();
    pub unsafe fn isr8();   pub unsafe fn isr9();   pub unsafe fn isr10();  pub unsafe fn isr11();
    pub unsafe fn isr12();  pub unsafe fn isr13();  pub unsafe fn isr14();  pub unsafe fn isr15();
    pub unsafe fn isr16();  pub unsafe fn isr17();  pub unsafe fn isr18();  pub unsafe fn isr19();
    pub unsafe fn isr20();  pub unsafe fn isr21();  pub unsafe fn isr22();  pub unsafe fn isr23();
    pub unsafe fn isr24();  pub unsafe fn isr25();  pub unsafe fn isr26();  pub unsafe fn isr27();
    pub unsafe fn isr28();  pub unsafe fn isr29();  pub unsafe fn isr30();  pub unsafe fn isr31();

    // Hardware IRQs (32-47)  
    pub unsafe fn isr32();  pub unsafe fn isr33();  pub unsafe fn isr34();  pub unsafe fn isr35();
    pub unsafe fn isr36();  pub unsafe fn isr37();  pub unsafe fn isr38();  pub unsafe fn isr39();
    pub unsafe fn isr40();  pub unsafe fn isr41();  pub unsafe fn isr42();  pub unsafe fn isr43();
    pub unsafe fn isr44();  pub unsafe fn isr45();  pub unsafe fn isr46();  pub unsafe fn isr47();
    
    // Additional utility functions from assembly
    pub unsafe fn get_rsp() -> u64;
    pub unsafe fn get_rbp() -> u64;
    pub unsafe fn read_cr2() -> u64;
    pub unsafe fn read_cr3() -> u64;
}