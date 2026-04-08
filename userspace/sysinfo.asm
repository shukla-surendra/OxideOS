; OxideOS user program: sysinfo
; Calls GetSystemInfo (syscall 50) and prints uptime + memory figures.
;
; SystemInfo layout (repr C):
;   offset  0: total_memory  u64
;   offset  8: free_memory   u64
;   offset 16: uptime_ms     u64
;   offset 24: process_count u32
;
; Syscall ABI (int 0x80):
;   50 (GetSystemInfo): rdi = *SystemInfo  → fills the struct
;   30 (Print):         rdi = buf ptr, rsi = len
;    0 (Exit):          rdi = exit code
;
; Number formatting: itoa64 subroutine (unsigned decimal, result in scratch_buf).

bits 64
org 0x400000

; ── Entry point ──────────────────────────────────────────────────────────────

    sub  rsp, 32            ; SystemInfo on stack (28 bytes, padded)
    and  rsp, ~0xF          ; 16-byte align

    ; GetSystemInfo(rdi = &info)
    mov  rax, 50
    mov  rdi, rsp
    int  0x80

    ; stash fields before we clobber rsp-relative access
    mov  r12, [rsp + 0]     ; total_memory
    mov  r13, [rsp + 8]     ; free_memory
    mov  r14, [rsp + 16]    ; uptime_ms

    ; --- header ---
    mov  rax, 30
    lea  rdi, [rel hdr]
    mov  rsi, hdr.end - hdr
    int  0x80

    ; --- uptime in seconds ---
    mov  rax, 30
    lea  rdi, [rel lbl_up]
    mov  rsi, lbl_up.end - lbl_up
    int  0x80

    mov  rax, r14
    mov  rbx, 1000
    xor  rdx, rdx
    div  rbx                ; rax = seconds
    lea  rdi, [rel scratch]
    call itoa64             ; rsi = length written
    mov  rax, 30
    lea  rdi, [rel scratch]
    int  0x80               ; rsi already set by itoa64

    mov  rax, 30
    lea  rdi, [rel unit_s]
    mov  rsi, unit_s.end - unit_s
    int  0x80

    ; --- total memory in MB ---
    mov  rax, 30
    lea  rdi, [rel lbl_tot]
    mov  rsi, lbl_tot.end - lbl_tot
    int  0x80

    mov  rax, r12
    shr  rax, 20            ; bytes → MB
    lea  rdi, [rel scratch]
    call itoa64
    mov  rax, 30
    lea  rdi, [rel scratch]
    int  0x80

    mov  rax, 30
    lea  rdi, [rel unit_mb]
    mov  rsi, unit_mb.end - unit_mb
    int  0x80

    ; --- free memory in MB ---
    mov  rax, 30
    lea  rdi, [rel lbl_free]
    mov  rsi, lbl_free.end - lbl_free
    int  0x80

    mov  rax, r13
    shr  rax, 20
    lea  rdi, [rel scratch]
    call itoa64
    mov  rax, 30
    lea  rdi, [rel scratch]
    int  0x80

    mov  rax, 30
    lea  rdi, [rel unit_mb]
    mov  rsi, unit_mb.end - unit_mb
    int  0x80

    ; --- exit ---
    xor  rdi, rdi
    xor  rax, rax
    int  0x80

; ── itoa64 ───────────────────────────────────────────────────────────────────
; Convert unsigned 64-bit integer in rax to decimal ASCII.
; rdi = output buffer (must hold at least 20 bytes).
; Returns: rsi = number of bytes written (not null-terminated).
; Clobbers: rax, rbx, rcx, rdx.
itoa64:
    push rbp
    mov  rbp, rsp
    sub  rsp, 24            ; local reverse buffer (20 digits max)
    and  rsp, ~0xF

    test rax, rax
    jnz  .nonzero
    mov  byte [rdi], '0'
    mov  rsi, 1
    leave
    ret

.nonzero:
    mov  rbx, rsp           ; base of reverse buffer
    xor  rcx, rcx           ; digit count

.digit_loop:
    test rax, rax
    jz   .done_digits
    xor  rdx, rdx
    mov  r8, 10
    div  r8                 ; rax = quot, rdx = digit
    add  dl, '0'
    mov  [rbx + rcx], dl
    inc  rcx
    jmp  .digit_loop

.done_digits:
    ; reverse-copy into rdi
    xor  r9, r9
.copy:
    cmp  r9, rcx
    jge  .itoa_done
    ; src index = (rcx - r9 - 1)
    mov  r10, rcx
    sub  r10, r9
    dec  r10
    mov  al, [rbx + r10]
    mov  [rdi + r9], al
    inc  r9
    jmp  .copy

.itoa_done:
    mov  rsi, rcx
    leave
    ret

; ── Strings ──────────────────────────────────────────────────────────────────
hdr:
    db  "--- OxideOS System Info ---", 10
.end:

lbl_up:
    db  "Uptime:  "
.end:

unit_s:
    db  " s", 10
.end:

lbl_tot:
    db  "Total:   "
.end:

lbl_free:
    db  "Free:    "
.end:

unit_mb:
    db  " MB", 10
.end:

scratch:
    times 20 db 0           ; number formatting scratch space
