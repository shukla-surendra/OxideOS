# Step 1: `int 0x80` Syscall Dispatch

## Goal

Build the first syscall milestone around a path the kernel can actually support today.

That means:

- installing a software interrupt entry in the IDT,
- decoding syscall numbers consistently,
- validating user pointers before dereferencing them,
- returning results through `RAX`,
- and testing the dispatch logic outside QEMU.

## Why Start With `int 0x80`?

`syscall/sysret` is fast, but it is not just "one instruction and done".

On x86_64, a production-ready `syscall/sysret` implementation also needs:

- correct STAR/LSTAR/FM`ASK` setup,
- a trusted kernel stack for each task,
- user/kernel GDT layout that matches the MSR configuration,
- a TSS or equivalent stack-switching strategy,
- and a real user-mode execution environment.

OxideOS does not have that full user-mode foundation yet. So the first honest milestone is `int 0x80`, which is slower but much easier to reason about while the kernel is still growing.

## ABI Used In Step 1

OxideOS currently uses this register convention for the interrupt-based syscall path:

- `RAX`: syscall number
- `RDI`: arg1
- `RSI`: arg2
- `RDX`: arg3
- `R10`: arg4
- `R8`: arg5
- return value in `RAX`

This keeps the dispatch shape close to the eventual x86_64 fast-path calling convention without requiring the fast path yet.

## What Changed

### 1. IDT wiring

Vector `128` is now installed as a dedicated software interrupt gate instead of falling through to the default ISR.

Important detail:

- the gate uses DPL 3 (`0xEE`) so future ring 3 code is allowed to invoke it.

### 2. Shared syscall core

The main dispatch rules now live in:

- `kernel/src/kernel/syscall_core.rs`

This file is intentionally written against `core` only, so the same logic can run in:

- the real kernel, and
- host-side tests.

### 3. Kernel adapter

The kernel-specific serial/timer/halt behavior now lives in:

- `kernel/src/kernel/syscall.rs`

That file implements a `SyscallRuntime` adapter and forwards requests into the shared core.

### 4. Boot smoke tests

The kernel runs a small boot-time syscall smoke test for non-pointer syscalls:

- `getpid`
- `gettime`
- invalid syscall handling

Pointer-carrying syscalls are tested on the host instead, because kernel pointers in the higher half should fail user-pointer validation by design.

## Pointer Validation Theory

Step 1 uses a very small rule set:

- user pointers must be at least `0x1000`,
- user pointers must not cross into the kernel higher-half base `0xFFFF800000000000`,
- and pointer arithmetic must not overflow.

This is not a full virtual memory policy yet. It is just the minimum boundary enforcement needed before the kernel reads or writes memory supplied by a caller.

## Current Supported Behavior

Implemented behavior:

- `getpid`
- `gettime`
- `sleep`
- `write`
- `print`
- `get_system_info`

Stubbed behavior:

- `fork`
- `wait`
- `mmap`
- `munmap`
- `brk`
- `read`
- `open`
- `close`
- `getchar`

For unsupported syscalls, the dispatcher returns `ENOSYS`.

## Testing Strategy

Host-side tests live in:

- `kernel/tests/syscall_core.rs`

These tests verify:

- syscall decoding,
- invalid syscall handling,
- pointer validation,
- `write` fd validation,
- output copying into the runtime sink,
- `sleep` deadline calculation,
- and `get_system_info` copy-out behavior.

Suggested command:

```bash
make -C kernel test-syscalls
```

Why this uses a dedicated host harness instead of `cargo test`:

- the OxideOS package is still centered around a `no_std` kernel binary,
- standard Cargo test flows try to pull that binary into a panic-unwind based host test run,
- and that is a separate build-system problem from syscall logic itself.

The dedicated harness lets us test syscall dispatch rules immediately while keeping the kernel crate structure intact.

## Limits of Step 1

This is still not a user-space-capable kernel. Missing pieces include:

- ring 3 tasks,
- address-space ownership,
- safe copy-from-user/copy-to-user helpers tied to page tables,
- per-task kernel stacks,
- and end-to-end QEMU tests from actual user code.

## Next Step

Step 2 should focus on the boundary around syscalls, not on adding more syscall numbers immediately.

The next valuable work is:

1. define a real user-mode execution plan,
2. add per-task kernel stack ownership,
3. introduce copy helpers that are aware of mapped user memory,
4. then decide whether `syscall/sysret` becomes a supported fast path.
