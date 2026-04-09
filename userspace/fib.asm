; fib.asm — Print first 15 Fibonacci numbers
;
; Syscalls used:
;   30 (Print): rdi = buf ptr, rsi = len
;    0 (Exit):  rdi = exit code

bits 64
org 0x400000

    mov  rax, 30
    lea  rdi, [rel hdr]
    mov  rsi, hdr.end - hdr
    int  0x80

    xor  r12, r12           ; a = 0
    mov  r13, 1             ; b = 1
    xor  r15, r15           ; i = 0

.next:
    ; print a
    mov  rax, r12
    lea  rdi, [rel nbuf]
    call itoa64             ; rsi = length written

    ; append newline
    add  rdi, rsi
    mov  byte [rdi], 10
    inc  rsi

    mov  rax, 30
    lea  rdi, [rel nbuf]
    int  0x80

    ; advance: tmp = a+b, a = b, b = tmp
    mov  r14, r12
    add  r14, r13
    mov  r12, r13
    mov  r13, r14

    inc  r15
    cmp  r15, 15
    jl   .next

    xor  rdi, rdi
    xor  rax, rax
    int  0x80

; ── itoa64 ──────────────────────────────────────────────────────────────────
; rax = unsigned integer, rdi = output buf (>=21 bytes)
; Returns: rsi = bytes written (no null terminator)
; Clobbers: rax, rbx, rcx, rdx, r8, r9, r10
itoa64:
    push rbp
    mov  rbp, rsp
    sub  rsp, 32
    and  rsp, ~0xF

    test rax, rax
    jnz  .nonzero
    mov  byte [rdi], '0'
    mov  rsi, 1
    leave
    ret

.nonzero:
    lea  rbx, [rsp]
    xor  rcx, rcx
.digit:
    test rax, rax
    jz   .rev
    xor  rdx, rdx
    mov  r8, 10
    div  r8
    add  dl, '0'
    mov  [rbx + rcx], dl
    inc  rcx
    jmp  .digit
.rev:
    xor  r9, r9
.copy:
    cmp  r9, rcx
    jge  .done
    mov  r10, rcx
    sub  r10, r9
    dec  r10
    movzx eax, byte [rbx + r10]
    mov  [rdi + r9], al
    inc  r9
    jmp  .copy
.done:
    mov  rsi, rcx
    leave
    ret

; ── Data ────────────────────────────────────────────────────────────────────
hdr:
    db  "Fibonacci sequence (first 15):", 10
.end:

nbuf:
    times 24 db 0
