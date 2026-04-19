; spinner.asm — Animate a spinning cursor for ~3 seconds then exit
;
; Syscalls used:
;  400 (Print): rdi = buf ptr, rsi = len
;   35 (Sleep): rdi = milliseconds
;   60 (Exit):  rdi = exit code
;
; Terminal trick: print frame char, then '\r' to return to column 0,
; so next frame overwrites the previous one.

bits 64
org 0x400000

    mov  rax, 400
    lea  rdi, [rel msg_start]
    mov  rsi, msg_start.end - msg_start
    int  0x80

    xor  r15, r15           ; iteration counter

.loop:
    cmp  r15, 30            ; ~3 seconds at 100 ms/frame
    jge  .done

    ; pick frame char: index = r15 & 3
    mov  rcx, r15
    and  rcx, 3
    lea  rax, [rel frames]
    movzx eax, byte [rax + rcx]
    mov  [rel frame_buf], al

    mov  rax, 400
    lea  rdi, [rel frame_buf]
    mov  rsi, 3             ; char + space + '\r'
    int  0x80

    ; sleep 100 ms
    mov  rax, 35
    mov  rdi, 100
    int  0x80

    inc  r15
    jmp  .loop

.done:
    ; print newline to leave cursor on clean line
    mov  rax, 400
    lea  rdi, [rel newline]
    mov  rsi, 1
    int  0x80

    mov  rax, 400
    lea  rdi, [rel msg_done]
    mov  rsi, msg_done.end - msg_done
    int  0x80

    xor  rdi, rdi
    mov  rax, 60
    int  0x80

; ── Data ─────────────────────────────────────────────────────────────────────
msg_start:
    db  "Spinning: "
.end:

frames:     db  '-', '\', '|', '/'   ; four animation frames

frame_buf:  db  0, ' ', 13           ; char, space, carriage-return (3 bytes)

newline:    db  10

msg_done:
    db  "Done!", 10
.end:
