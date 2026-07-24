// src/kernel/interrupts_asm.rs - Corrected global_asm! syntax
use core::arch::global_asm;

global_asm!(
r#"
.intel_syntax noprefix

.extern isr_common_handler

# ============================================================================
# COMMON INTERRUPT STUB - 64-bit version
# ============================================================================

.globl isr_common_stub
isr_common_stub:
    # Save all general-purpose registers (64-bit)
    push rax
    push rbx
    push rcx
    push rdx
    push rbp
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    # Set up stack frame for calling convention
    mov rdi, rsp                # Pass pointer to interrupt frame as first argument
    
    # Align stack to 16-byte boundary (required for 64-bit calling convention)
    mov rbp, rsp
    and rsp, -16
    
    # Call the high-level handler
    call isr_common_handler
    
    # Restore stack pointer
    mov rsp, rbp
    
    # Restore all registers
    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rbp
    pop rdx
    pop rcx
    pop rbx
    pop rax
    
    # Remove interrupt number and error code from stack
    add rsp, 16
    
    # Return from interrupt
    iretq

# ============================================================================
# EXCEPTION HANDLERS (0-31) - Manually expanded since no macros
# ============================================================================

# Exceptions that don't push error codes
.globl isr0
isr0:
    push 0        # Push dummy error code
    push 0        # Push interrupt number
    jmp isr_common_stub

.globl isr1
isr1:
    push 0
    push 1
    jmp isr_common_stub

.globl isr2
isr2:
    push 0
    push 2
    jmp isr_common_stub

.globl isr3
isr3:
    push 0
    push 3
    jmp isr_common_stub

.globl isr4
isr4:
    push 0
    push 4
    jmp isr_common_stub

.globl isr5
isr5:
    push 0
    push 5
    jmp isr_common_stub

.globl isr6
isr6:
    push 0
    push 6
    jmp isr_common_stub

.globl isr7
isr7:
    push 0
    push 7
    jmp isr_common_stub

# Double fault - pushes error code
.globl isr8
isr8:
    push 8        # Push interrupt number (error code already pushed by CPU)
    jmp isr_common_stub

.globl isr9
isr9:
    push 0
    push 9
    jmp isr_common_stub

# These push error codes
.globl isr10
isr10:
    push 10
    jmp isr_common_stub

.globl isr11
isr11:
    push 11
    jmp isr_common_stub

.globl isr12
isr12:
    push 12
    jmp isr_common_stub

.globl isr13
isr13:
    push 13
    jmp isr_common_stub

.globl isr14
isr14:
    push 14
    jmp isr_common_stub

.globl isr15
isr15:
    push 0
    push 15
    jmp isr_common_stub

.globl isr16
isr16:
    push 0
    push 16
    jmp isr_common_stub

# Alignment check - pushes error code
.globl isr17
isr17:
    push 17
    jmp isr_common_stub

.globl isr18
isr18:
    push 0
    push 18
    jmp isr_common_stub

.globl isr19
isr19:
    push 0
    push 19
    jmp isr_common_stub

.globl isr20
isr20:
    push 0
    push 20
    jmp isr_common_stub

# Control protection - pushes error code
.globl isr21
isr21:
    push 21
    jmp isr_common_stub

.globl isr22
isr22:
    push 0
    push 22
    jmp isr_common_stub

.globl isr23
isr23:
    push 0
    push 23
    jmp isr_common_stub

.globl isr24
isr24:
    push 0
    push 24
    jmp isr_common_stub

.globl isr25
isr25:
    push 0
    push 25
    jmp isr_common_stub

.globl isr26
isr26:
    push 0
    push 26
    jmp isr_common_stub

.globl isr27
isr27:
    push 0
    push 27
    jmp isr_common_stub

.globl isr28
isr28:
    push 0
    push 28
    jmp isr_common_stub

# VMM communication - pushes error code
.globl isr29
isr29:
    push 29
    jmp isr_common_stub

# Security exception - pushes error code
.globl isr30
isr30:
    push 30
    jmp isr_common_stub

.globl isr31
isr31:
    push 0
    push 31
    jmp isr_common_stub

# ============================================================================
# HARDWARE IRQ HANDLERS (32-47) - None push error codes
# ============================================================================

.globl isr32
isr32:
    push 0
    push 32
    jmp isr_common_stub

.globl isr33
isr33:
    push 0
    push 33
    jmp isr_common_stub

.globl isr34
isr34:
    push 0
    push 34
    jmp isr_common_stub

.globl isr35
isr35:
    push 0
    push 35
    jmp isr_common_stub

.globl isr36
isr36:
    push 0
    push 36
    jmp isr_common_stub

.globl isr37
isr37:
    push 0
    push 37
    jmp isr_common_stub

.globl isr38
isr38:
    push 0
    push 38
    jmp isr_common_stub

.globl isr39
isr39:
    push 0
    push 39
    jmp isr_common_stub

.globl isr40
isr40:
    push 0
    push 40
    jmp isr_common_stub

.globl isr41
isr41:
    push 0
    push 41
    jmp isr_common_stub

.globl isr42
isr42:
    push 0
    push 42
    jmp isr_common_stub

.globl isr43
isr43:
    push 0
    push 43
    jmp isr_common_stub

.globl isr44
isr44:
    push 0
    push 44
    jmp isr_common_stub

.globl isr45
isr45:
    push 0
    push 45
    jmp isr_common_stub

.globl isr46
isr46:
    push 0
    push 46
    jmp isr_common_stub

.globl isr47
isr47:
    push 0
    push 47
    jmp isr_common_stub

# ============================================================================
# SYSTEM CALL HANDLER
# ============================================================================

.globl isr128
isr128:
    push 0        # Push dummy error code
    push 128      # Push interrupt number (0x80)
    jmp isr_common_stub

# ============================================================================
# UTILITY FUNCTIONS
# ============================================================================

.globl get_rsp
get_rsp:
    mov rax, rsp
    ret

.globl get_rbp
get_rbp:
    mov rax, rbp
    ret

.globl read_cr2
read_cr2:
    mov rax, cr2
    ret

.globl read_cr3
read_cr3:
    mov rax, cr3
    ret

.att_syntax prefix
"#
);