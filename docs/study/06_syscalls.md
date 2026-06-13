# 06 — Syscalls: Crossing the Ring Boundary

A syscall is the only legitimate way for a user program to ask the kernel to do
something. Understanding syscalls means understanding the ring boundary in action.

---

## Why syscalls exist

A user program can't just call `keyboard::read()` or `fs::write()` directly.
Those functions live in kernel memory which user programs can't access (their page
table entries don't map it). Even if they could read the address, calling Ring 0
code from Ring 3 would trigger a General Protection Fault.

The CPU provides a controlled escape hatch: the `SYSCALL` instruction (or the older
`int 0x80` software interrupt). This atomically:
1. Saves RIP (return address) in RCX
2. Saves RFLAGS in R11
3. Switches to Ring 0 (sets CS to kernel code selector)
4. Jumps to the address stored in the LSTAR MSR (your kernel's syscall handler)

Now the kernel is running, can access all memory, and can decide whether to grant
the request.

---

## The two mechanisms in this kernel

### Legacy: `int 0x80`

Triggers interrupt vector 128. Handled via the IDT just like hardware interrupts.
Slower but simpler. Some programs may use this.

### Modern: `SYSCALL` instruction

**Find in `kernel/src/kernel/sys/syscall_handler.rs`:**
- `init()` sets three MSRs (Model-Specific Registers):
  - `STAR` — segment selectors for the transition (kernel CS/SS)
  - `LSTAR` — the 64-bit handler address (where SYSCALL jumps to)
  - `SFMASK` — which RFLAGS bits to clear on entry (e.g., clear interrupt flag)

When a user program executes `syscall`, the CPU automatically uses these MSR values.

---

## The dispatch path — `kernel/src/kernel/sys/syscall_core.rs`

**Find `dispatch()` (line ~929).** Every syscall flows through here:

1. User program puts the syscall number in RAX, arguments in RDI, RSI, RDX, R10...
2. Executes `syscall`
3. CPU enters kernel at `LSTAR` handler
4. Handler reads RAX → constructs `SyscallRequest { number, arg1, arg2, ... }`
5. Calls `dispatch(request)` which matches on the syscall number:
   ```rust
   Syscall::Write => sys_write(runtime, fd, buf_ptr, len),
   Syscall::Exit  => sys_exit(runtime, code),
   Syscall::Open  => sys_open(runtime, path_ptr, flags),
   // ... ~35 total
   ```
6. Result goes in RAX
7. `SYSRET` returns to user code

**Find `Syscall` enum** — these are the ~35 syscall numbers.

**Find `sys_write()` (line ~1401)** — the most commonly called syscall:
- Takes a file descriptor, a buffer pointer (user virtual address!), and a length
- Calls `validate_user_range()` to ensure the pointer is actually in user space
  (if not: could be a bug or exploit — reject with EFAULT)
- Writes to the correct destination: stdout → process output buffer, file FDs → FS

---

## The security boundary

`validate_user_range()` (line ~914) is crucial. A user program could pass *any*
pointer as the `buf_ptr` argument to `sys_write`, including a pointer into kernel
memory. Without this check, the kernel would happily copy kernel secrets to the
file. This is called an **arbitrary kernel read** vulnerability.

Always validate:
- That pointer arguments point into user space (not kernel space)
- That the entire range `[ptr, ptr+len)` is within user space
- That lengths are sane (not wrapping around `u64::MAX`)

---

## Adding a new syscall (the exercise)

Adding a syscall touches exactly these files, in this order:

1. **`syscall_core.rs`** — add a new variant to the `Syscall` enum and assign it a number
2. **`syscall_core.rs`** — add a branch in `dispatch()` that calls your handler
3. **`syscall_core.rs`** — implement `sys_yourname()` below the existing ones
4. **Userspace program** — call it with `syscall` instruction and your number in RAX

---

## Questions

1. What is a MSR? Why does setting LSTAR require a special instruction (`wrmsr`)?
2. Why is `SFMASK` configured to clear the interrupt flag on syscall entry?
   What would go wrong if interrupts stayed enabled?
3. What is the difference between SYSCALL/SYSRET and int/IRET for syscalls?
   (Hint: think about how many ring transitions and memory reads each requires)
4. If a user program passes a kernel-space address as a buffer pointer to `sys_write`,
   what prevents the kernel from reading that memory?
5. Why does the syscall handler need to be written in assembly (or have an assembly
   wrapper) rather than a plain Rust `extern "C" fn`?

---

## Exercise: Add `sys_getpid`

Implement a `sys_getpid` syscall that returns the current task's index as its PID.

Steps:
1. In `syscall_core.rs`, add `GetPid = 200` to the `Syscall` enum (pick a free number)
2. In `dispatch()`, add `Syscall::GetPid => { runtime.return_value(current_task_idx as u64); }`
   (look at how other syscalls use `runtime.return_value()`)
3. In a userspace test program, call it:
   ```c
   // or in a Rust userspace program:
   let pid: u64;
   unsafe { core::arch::asm!("mov rax, 200", "syscall", out("rax") pid) };
   ```

You need to figure out how to get the current task index inside dispatch — look at
how `sys_exit` or `sys_write` does it.

---

## Your notes
<!-- Add your own notes here as you study -->
