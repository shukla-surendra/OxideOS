; countdown.asm — Count down from 10 to 1, then print "Liftoff!"
;
; Syscalls used:
;  400 (Print): rdi = buf ptr, rsi = len
;   35 (Sleep): rdi = milliseconds
;   60 (Exit):  rdi = exit code

bits 64
org 0x400000

    mov  r15, 10            ; counter = 10

.loop:
    cmp  r15, 0
    jle  .liftoff

    ; print counter
    mov  rax, r15
    lea  rdi, [rel nbuf]
    call itoa64

    add  rdi, rsi
    mov  byte [rdi], 10     ; newline
    inc  rsi

    mov  rax, 400
    lea  rdi, [rel nbuf]
    int  0x80

    ; sleep 500 ms
    mov  rax, 35
    mov  rdi, 500
    int  0x80

    dec  r15
    jmp  .loop

.liftoff:
    mov  rax, 400
    lea  rdi, [rel msg_liftoff]
    mov  rsi, msg_liftoff.end - msg_liftoff
    int  0x80

    xor  rdi, rdi
    mov  rax, 60
    int  0x80

; ── itoa64 ───────────────────────────────────────────────────────────────────
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

; ── Data ─────────────────────────────────────────────────────────────────────
msg_liftoff:
    db  "Liftoff!", 10
.end:

nbuf:
    times 24 db 0
