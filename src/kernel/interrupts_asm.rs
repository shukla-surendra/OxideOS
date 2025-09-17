// src/kernel/interrupts_asm.rs
#![no_std]

use core::arch::global_asm;
// ============================================================================
// ASSEMBLY INTERRUPT STUBS - CORRECTED WITH ALIGNMENT CHECK
// ============================================================================

global_asm!(r#"
.section .text
.intel_syntax noprefix

// Minimal test timer handler - no parameters, no frame
.global test_timer_isr
test_timer_isr:
    pushad                    // Save all registers
    call minimal_test_handler // Call simple Rust function (no parameters)
    popad                     // Restore all registers
    mov al, 0x20             // EOI command
    out 0x20, al             // Send to master PIC
    iret                     // Return from interrupt

// Macro for interrupts without error code
.macro ISR_NOERR name num
    .global \name
\name:
    // Check and align stack (ESP must be 4-byte aligned)
    mov eax, esp
    and eax, 3              // Check low 2 bits (ESP % 4)
    jz aligned_noerr_\num   // If zero, already aligned
    push 0                  // Push 4 bytes to align stack
aligned_noerr_\num:
    push 0                  // Dummy error code
    push \num               // Interrupt number
    pushad                  // Save all general purpose registers

    push esp                // Push pointer to frame (now aligned)
    call isr_common_handler // Call Rust handler with frame ptr
    add esp, 4              // Clean up the pushed pointer

    popad                   // Restore registers
    add esp, 8              // Remove int_no and err_code
    test eax, eax           // Check if we pushed for alignment
    jz no_pop_noerr_\num
    add esp, 4              // Remove alignment padding
no_pop_noerr_\num:
    iret                    // Return from interrupt
.endm

// Special macro for ISR3 (breakpoint) with EIP advance
.macro ISR_BREAKPOINT name num
    .global \name
\name:
    // Check and align stack
    mov eax, esp
    and eax, 3              // Check low 2 bits (ESP % 4)
    jz aligned_breakpoint_\num
    push 0                  // Push 4 bytes to align stack
aligned_breakpoint_\num:
    push 0                  // Dummy error code
    push \num               // Interrupt number
    pushad                  // Save all general purpose registers

    // Advance CPU-saved EIP by 1 to skip INT3 (0xCC, 1 byte)
    mov ebx, esp
    add ebx, 40             // Point to the CPU-saved EIP slot
    add dword ptr [ebx], 1  // Advance saved EIP by 1

    push esp                // Push pointer to frame (now aligned)
    call isr_common_handler // Call Rust handler with frame ptr
    add esp, 4              // Clean up the pushed pointer

    popad                   // Restore registers
    add esp, 8              // Remove int_no and err_code
    test eax, eax           // Check if we pushed for alignment
    jz no_pop_breakpoint_\num
    add esp, 4              // Remove alignment padding
no_pop_breakpoint_\num:
    iret                    // Return from interrupt
.endm

// Macro for interrupts with error code
.macro ISR_WITHERR name num
    .global \name
\name:
    // Check and align stack
    mov eax, esp
    and eax, 3              // Check low 2 bits (ESP % 4)
    jz aligned_witherr_\num
    push 0                  // Push 4 bytes to align stack
aligned_witherr_\num:
    push \num               // Interrupt number (error code already on stack)
    pushad                  // Save all general purpose registers
    
    push esp                // Push pointer to frame (now aligned)
    call isr_common_handler // Call Rust handler with frame ptr
    add esp, 4              // Clean up the pushed pointer
    
    popad                   // Restore registers
    add esp, 8              // Remove int_no and err_code
    test eax, eax           // Check if we pushed for alignment
    jz no_pop_witherr_\num
    add esp, 4              // Remove alignment padding
no_pop_witherr_\num:
    iret                    // Return from interrupt
.endm

// CPU Exceptions (0-31)
ISR_NOERR isr0 0      // Divide by zero
ISR_NOERR isr1 1      // Debug
ISR_NOERR isr2 2      // NMI
ISR_BREAKPOINT isr3 3 // Breakpoint (with EIP advance)
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