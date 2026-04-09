; primes.asm — Print all prime numbers up to 100
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

    mov  r15, 2             ; candidate n = 2

.outer:
    cmp  r15, 100
    jg   .done

    ; Check if r15 is prime: try divisors 2..floor(sqrt(n))
    ; We'll just try 2..n/2 for simplicity
    mov  r12, 2             ; divisor d = 2
    mov  r14b, 1            ; is_prime = true

.inner:
    ; if d*d > n, break (n is prime)
    mov  rax, r12
    imul rax, r12
    cmp  rax, r15
    jg   .is_prime

    ; if n mod d == 0, not prime
    mov  rax, r15
    xor  rdx, rdx
    mov  rcx, r12
    div  rcx
    test rdx, rdx
    jz   .not_prime

    inc  r12
    jmp  .inner

.not_prime:
    mov  r14b, 0
    jmp  .next_candidate

.is_prime:
    test r14b, r14b
    jz   .next_candidate

    ; print r15
    mov  rax, r15
    lea  rdi, [rel nbuf]
    call itoa64

    add  rdi, rsi
    mov  byte [rdi], 10
    inc  rsi

    mov  rax, 30
    lea  rdi, [rel nbuf]
    int  0x80

.next_candidate:
    inc  r15
    jmp  .outer

.done:
    xor  rdi, rdi
    xor  rax, rax
    int  0x80

; ── itoa64 ──────────────────────────────────────────────────────────────────
; rax = unsigned integer, rdi = output buf (>=21 bytes)
; Returns: rsi = bytes written
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
    jge  .done2
    mov  r10, rcx
    sub  r10, r9
    dec  r10
    movzx eax, byte [rbx + r10]
    mov  [rdi + r9], al
    inc  r9
    jmp  .copy
.done2:
    mov  rsi, rcx
    leave
    ret

; ── Data ─────────────────────────────────────────────────────────────────────
hdr:
    db  "Primes up to 100:", 10
.end:

nbuf:
    times 24 db 0
