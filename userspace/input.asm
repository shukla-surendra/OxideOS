; input.asm — stdin echo test for OxideOS
;
; Reads characters one at a time via GetChar (syscall 31) and echoes
; them back to stdout with Write (syscall 21 / fd=1).
; Exits when Ctrl+C (byte 0x03) is received.
;
; Syscall ABI (int 0x80):
;   rax = syscall number
;   rdi = arg1,  rsi = arg2,  rdx = arg3
;
; Build: nasm -f bin -o bin/input.bin input.asm

BITS 64
ORG 0x400000

_start:
    ; Print banner
    mov rax, 30          ; Print
    lea rdi, [rel banner]
    mov rsi, banner_len
    int 0x80

.read_loop:
    ; GetChar (31) — returns char in rax, EAGAIN (-6) if empty
    mov rax, 31
    int 0x80

    ; EAGAIN: nothing yet, spin
    cmp rax, -6
    je  .read_loop

    ; Ctrl+C exits
    cmp al, 0x03
    je  .exit

    ; Echo the character
    mov [rel char_buf], al
    mov rax, 21          ; Write
    mov rdi, 1           ; stdout
    lea rsi, [rel char_buf]
    mov rdx, 1
    int 0x80

    jmp .read_loop

.exit:
    mov rax, 30
    lea rdi, [rel bye_msg]
    mov rsi, bye_len
    int 0x80

    xor rdi, rdi
    xor rax, rax         ; Exit
    int 0x80

; ── Data ──────────────────────────────────────────────────────────────────────
banner:     db "stdin echo ready (Ctrl+C to quit)", 0x0A
banner_len: equ $ - banner

bye_msg:    db 0x0A, "bye!", 0x0A
bye_len:    equ $ - bye_msg

char_buf:   db 0
