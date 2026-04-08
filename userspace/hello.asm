; OxideOS user program: hello
; Flat binary — loaded at USER_CODE_ADDR (0x400000) by the kernel loader.
;
; Syscall ABI (int 0x80):
;   rax = syscall number
;   rdi = arg1,  rsi = arg2,  rdx = arg3
;
; Syscalls used:
;   30 (Print):  rdi = buf ptr, rsi = byte count  → prints to terminal
;    0 (Exit):   rdi = exit code

bits 64
org 0x400000

    ; --- print greeting ---
    mov  rax, 30
    lea  rdi, [rel msg]
    mov  rsi, msg.end - msg
    int  0x80

    ; --- exit(0) ---
    xor  rdi, rdi
    xor  rax, rax
    int  0x80

msg:
    db  "Hello from OxideOS user space!", 10
    db  "Running in ring 3 (unprivileged mode).", 10
.end:
