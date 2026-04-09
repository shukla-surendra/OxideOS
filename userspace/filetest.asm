; filetest.asm — Create a file, write to it, read it back, then print contents
;
; Syscalls used:
;   22 (Open):  rdi = path ptr, rsi = path len, rdx = flags  → fd in rax
;   21 (Write): rdi = fd, rsi = buf ptr, rdx = count
;   20 (Read):  rdi = fd, rsi = buf ptr, rdx = count
;   23 (Close): rdi = fd
;   30 (Print): rdi = buf ptr, rsi = len
;    0 (Exit):  rdi = exit code
;
; Open flags (matching RamFs): 0x1 = read, 0x2 = write/create

bits 64
org 0x400000

    ; ── Print banner ─────────────────────────────────────────────────────────
    mov  rax, 30
    lea  rdi, [rel msg_banner]
    mov  rsi, msg_banner.end - msg_banner
    int  0x80

    ; ── Open file for writing (O_WRONLY | O_CREAT = 0x02) ────────────────────
    mov  rax, 22
    lea  rdi, [rel filename]
    mov  rsi, filename.end - filename
    mov  rdx, 2
    int  0x80

    test rax, rax
    js   .open_fail

    mov  r15, rax           ; r15 = write fd

    ; ── Write message to file ────────────────────────────────────────────────
    mov  rax, 21
    mov  rdi, r15
    lea  rsi, [rel file_data]
    mov  rdx, file_data.end - file_data
    int  0x80

    ; ── Close write fd ───────────────────────────────────────────────────────
    mov  rax, 23
    mov  rdi, r15
    int  0x80

    ; ── Open file for reading (O_RDONLY = 0x01) ──────────────────────────────
    mov  rax, 22
    lea  rdi, [rel filename]
    mov  rsi, filename.end - filename
    mov  rdx, 1
    int  0x80

    test rax, rax
    js   .open_fail

    mov  r15, rax           ; r15 = read fd

    ; ── Read file contents into rbuf ─────────────────────────────────────────
    mov  rax, 20
    mov  rdi, r15
    lea  rsi, [rel rbuf]
    mov  rdx, 64
    int  0x80

    test rax, rax
    jle  .read_fail

    mov  r14, rax           ; bytes read

    ; ── Close read fd ────────────────────────────────────────────────────────
    mov  rax, 23
    mov  rdi, r15
    int  0x80

    ; ── Print "Read back: " label ────────────────────────────────────────────
    mov  rax, 30
    lea  rdi, [rel msg_readback]
    mov  rsi, msg_readback.end - msg_readback
    int  0x80

    ; ── Print the bytes we read ──────────────────────────────────────────────
    mov  rax, 30
    lea  rdi, [rel rbuf]
    mov  rsi, r14
    int  0x80

    ; ensure newline after content
    mov  rax, 30
    lea  rdi, [rel newline]
    mov  rsi, 1
    int  0x80

    ; ── Print success ────────────────────────────────────────────────────────
    mov  rax, 30
    lea  rdi, [rel msg_ok]
    mov  rsi, msg_ok.end - msg_ok
    int  0x80

    xor  rdi, rdi
    xor  rax, rax
    int  0x80

.open_fail:
    mov  rax, 30
    lea  rdi, [rel msg_open_fail]
    mov  rsi, msg_open_fail.end - msg_open_fail
    int  0x80
    mov  rdi, 1
    xor  rax, rax
    int  0x80

.read_fail:
    mov  rax, 30
    lea  rdi, [rel msg_read_fail]
    mov  rsi, msg_read_fail.end - msg_read_fail
    int  0x80
    mov  rdi, 1
    xor  rax, rax
    int  0x80

; ── Data ─────────────────────────────────────────────────────────────────────
msg_banner:
    db  "File I/O test", 10
.end:

filename:
    db  "/test.txt"
.end:

file_data:
    db  "Hello from OxideOS filesystem!"
.end:

msg_readback:
    db  "Read back: "
.end:

msg_ok:
    db  "File test passed!", 10
.end:

msg_open_fail:
    db  "Error: open failed", 10
.end:

msg_read_fail:
    db  "Error: read failed", 10
.end:

newline:    db  10

rbuf:
    times 64 db 0
