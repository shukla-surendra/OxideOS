// Replace your timer interrupt code with this absolute minimal version

use core::arch::global_asm;

global_asm!(r#"
    .section .text
    .intel_syntax noprefix

    /* Ultra-minimal timer interrupt - just send EOI and return */
    .global isr32
    .type isr32, @function
isr32:
    /* Save only what we absolutely need */
    push eax
    push edx
    
    /* Send EOI directly to PIC - don't call Rust code */
    mov al, 0x20      /* EOI command */
    mov dx, 0x20      /* Master PIC command port */
    out dx, al        /* Send EOI */
    
    /* Restore and return immediately */
    pop edx
    pop eax
    iret

    /* Keep keyboard handler as is for now */
    .global isr33
    .type isr33, @function
isr33:
    push eax
    push ecx
    push edx
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    push 33
    call isr_dispatch
    add esp, 4
    
    pop ds
    pop edx
    pop ecx
    pop eax
    iret

    /* Keep other handlers unchanged */
    .global isr13
    .type isr13, @function
isr13:
    push eax
    mov eax, [esp+4]
    push eax
    call debug_log_gpf_error_code
    add esp, 4
    pop eax
    
    push eax
    push ecx
    push edx
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    push 13
    call isr_dispatch
    add esp, 4
    
    pop ds
    pop edx
    pop ecx
    pop eax
    add esp, 4
    iret

    .global default_isr
    .type default_isr, @function
default_isr:
    push eax
    push ecx
    push edx
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    push 255
    call isr_dispatch
    add esp, 4
    
    pop ds
    pop edx
    pop ecx
    pop eax
    iret

    .global isr8
    .type isr8, @function
isr8:
    push eax
    push ds
    
    mov ax, 0x10
    mov ds, ax
    
    push 8
    call isr_dispatch
    add esp, 4
    
    cli
halt_here:
    hlt
    jmp halt_here

    .att_syntax
"#);