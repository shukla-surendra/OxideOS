; OxideOS user program: counter
; Counts 1–9 and prints each digit on its own line.
;
; Demonstrates: loop in user mode, per-iteration syscall, single-digit formatting.
;
; Syscall ABI (int 0x80):
;  400 (Print): rdi = buf ptr, rsi = len
;   60 (Exit):  rdi = exit code

bits 64
org 0x400000

    ; --- print header ---
    mov  rax, 400
    lea  rdi, [rel header]
    mov  rsi, header.end - header
    int  0x80

    ; --- count 1..9 ---
    mov  rbx, 1                 ; counter

.next:
    ; build "N\n" in buf
    lea  rax, [rbx + 0x30]      ; '0' + counter
    mov  [rel buf], al

    ; print 2 bytes: digit + newline
    mov  rax, 400
    lea  rdi, [rel buf]
    mov  rsi, 2
    int  0x80

    inc  rbx
    cmp  rbx, 10
    jl   .next

    ; --- print footer ---
    mov  rax, 400
    lea  rdi, [rel footer]
    mov  rsi, footer.end - footer
    int  0x80

    ; --- exit(0) ---
    xor  rdi, rdi
    mov  rax, 60
    int  0x80

header:
    db  "Counting 1 to 9:", 10
.end:

footer:
    db  "Done!", 10
.end:

buf:
    db  0x30, 10    ; placeholder digit + newline (overwritten at runtime)
