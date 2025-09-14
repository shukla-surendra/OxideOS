// Enhanced debugging version of interupts.rs
// This version adds extensive logging to track down the GPF source

use core::arch::global_asm;

global_asm!(r#"
    .section .text
    .intel_syntax noprefix

    /* Enhanced ISR 32 with debugging */
    .global isr32
    .type isr32, @function
isr32:
    cli
    
    /* Save all registers first */
    pushad               
    push ds
    push es
    push fs
    push gs
    
    /* Log entry point */
    push eax
    mov eax, 0xDEAD0001  /* Debug marker for ISR32 entry */
    push eax
    call debug_log_entry
    add esp, 4
    pop eax

    /* Carefully set data segments */
    push eax
    mov ax, 0x10         /* Kernel data segment */
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    pop eax
    
    /* Log after segment setup */
    push eax
    mov eax, 0xDEAD0002  /* Debug marker for segment setup */
    push eax
    call debug_log_entry
    add esp, 4
    pop eax

    /* Call dispatcher */
    mov eax, 32
    push eax
    call isr_dispatch
    add esp, 4
    
    /* Log before restore */
    push eax
    mov eax, 0xDEAD0003  /* Debug marker before restore */
    push eax
    call debug_log_entry
    add esp, 4
    pop eax

    /* Restore segments and registers */
    pop gs
    pop fs
    pop es
    pop ds
    popad
    
    /* Log before iret */
    pushad
    mov eax, 0xDEAD0004  /* Debug marker before iret */
    push eax
    call debug_log_entry
    add esp, 4
    popad
    
    iret

/* ISR 33 (IRQ1 - Keyboard) */
.global isr33
.type isr33, @function
isr33:
    /* Same pattern as isr32 */
    pushad
    push ds
    push es
    push fs
    push gs

    /* Set kernel segments */
    mov ax, 0x18     /* Use same segment as isr32 */
    mov ds, ax
    mov es, ax

    /* Call dispatcher */
    mov eax, 33
    push eax
    call isr_dispatch
    add esp, 4

    /* Restore and return */
    pop gs
    pop fs
    pop es
    pop ds
    popad
    iret
    
    /* Enhanced ISR 13 (GPF) with state capture */
    .global isr13
    .type isr13, @function
isr13:
    cli
    
    /* GPF pushes error code automatically - save it */
    push eax             /* Save EAX first */
    mov eax, [esp+4]     /* Get error code from stack */
    push eax             /* Push error code for logging */
    call debug_log_gpf_error_code
    add esp, 4
    pop eax              /* Restore EAX */
    
    pushad
    push ds
    push es
    push fs
    push gs

    /* Set kernel segments */
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    /* Call dispatcher with vector 13 */
    mov eax, 13
    push eax
    call isr_dispatch
    add esp, 4

    pop gs
    pop fs
    pop es
    pop ds
    popad
    add esp, 4  /* Remove error code */
    iret

    /* Simple default ISR */
    .global default_isr
    .type default_isr, @function
default_isr:
    cli
    pushad
    push ds
    push es
    push fs
    push gs

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov eax, 255
    push eax
    call isr_dispatch
    add esp, 4

    pop gs
    pop fs
    pop es
    pop ds
    popad
    iret

    /* Double fault handler */
    .global isr8
    .type isr8, @function
isr8:
    cli
    /* Double fault has error code */
    pushad
    push ds
    push es  
    push fs
    push gs

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    mov eax, 8
    push eax
    call isr_dispatch
    add esp, 4

    pop gs
    pop fs
    pop es
    pop ds
    popad
    add esp, 4  /* Remove error code */
    iret

    .att_syntax
"#);