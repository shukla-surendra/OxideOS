# 05 — Processes: What a Task Actually Is

A "process" is just a bundle of state the OS saves and restores to give the illusion
that multiple programs run simultaneously. This doc explains what that state is and
how your scheduler manages it.

---

## What the CPU needs to "run a program"

To execute code, the CPU needs:
1. **Where to fetch instructions** — the instruction pointer (RIP)
2. **A stack** — RSP points to it; function calls push/pop here
3. **Registers** — RAX, RBX, ..., R15 hold working values
4. **Which address space** — CR3 points to the page table root
5. **Privilege level** — CS selector determines ring 0 or ring 3

That's it. A "context switch" = save all of these for the current task, load all of
these for the next task. The CPU doesn't know or care about "processes" — that's
a kernel abstraction built on top of these five things.

---

## Part A: The Task struct — `kernel/src/kernel/proc/scheduler.rs`

**Find `struct Task` (line ~129).**

Key fields:
```rust
pub ctx:        TaskContext,   // all saved registers (see user_mode.rs)
pub cr3:        u64,           // physical address of this task's L4 page table
pub stack:      u64,           // virtual address of stack base
pub state:      TaskState,     // Running / Waiting / Exited / ...
pub output:     [u8; 4096],    // captured stdout buffer
pub name:       [u8; 32],      // task name for display
```

**Find `struct TaskContext` in `kernel/src/kernel/proc/user_mode.rs`.** This is
just a bag of saved register values:
- `rip` — where to resume execution
- `rsp` — stack pointer
- `rflags` — CPU flags (interrupt enable, zero flag, etc.)
- `rax`..`r15` — all general-purpose registers
- `cs`, `ss` — segment selectors (determine ring level)

When the timer interrupt fires and it's time to switch tasks, the scheduler saves
all these into the current task's `TaskContext`, loads the next task's `TaskContext`,
and returns from the interrupt handler into the new task. From the new task's
perspective, nothing happened — it just kept running.

---

## Part B: The Scheduler — `scheduler.rs`

**Find `tick()`.** This is called from the timer ISR every 100ms (at 100Hz).

The logic:
1. Decrement the current task's remaining time slice
2. If slice == 0, pick the next `Running` task (round-robin: just increment index,
   wrap around)
3. Set up the context switch: save current registers (done by the ISR assembly stub),
   load next task's CR3, point RSP to next task's kernel stack
4. When the ISR returns (`IRETQ`), it restores the new task's saved registers
   and resumes at its saved RIP

**Find `spawn()` (line ~384).** This is how a new task is created:
1. Parse the ELF binary to get the entry point address
2. Allocate a new page table (`create_user_page_table()`)
3. Map the user stack into that page table
4. Load ELF segments into the page table
5. Set `ctx.rip = entry_point`, `ctx.rsp = stack_top`, `ctx.cs = USER_CS`
6. Add the task to the scheduler's task array

---

## Part C: ELF Loading — `kernel/src/kernel/proc/elf_loader.rs`

ELF (Executable and Linkable Format) is the binary format used by Linux and this OS.

**The relevant part:** `PT_LOAD` segments. When you compile a program, the linker
groups code and data into segments with specific virtual addresses. The ELF file says:
"put these bytes at virtual address 0x200000 with read+execute permissions."

`elf_loader.rs` parses the ELF header, finds all `PT_LOAD` segments, and maps each
one into the new process's page table at the requested virtual address.

**Read:** find the loop over `program_headers` and see where it calls `map_user_region_in`.

---

## Part D: Getting to Ring 3 — `kernel/src/kernel/proc/user_mode.rs`

After `spawn()` sets up the task context, the scheduler eventually switches to it.
The context has `cs = USER_CS` (a Ring 3 code selector).

When the timer ISR does `IRETQ` to restore this context, the CPU sees the Ring 3
CS selector and automatically:
- Drops to privilege level 3
- Switches to the user stack (RSP3 from TSS if coming from ring 0)
- Restricts access to privileged instructions

The program is now running in user mode. If it does anything privileged (like `in`
port I/O), the CPU triggers a General Protection Fault (vector 13) which the kernel
handles.

---

## Questions

1. Why does each task need its own stack? What would go wrong if tasks shared one?
2. What is a "time slice"? Why 2 timer ticks (20ms) specifically?
3. If a task is in `TaskState::Waiting`, what stops the scheduler from running it?
   What would set it back to `TaskState::Running`?
4. What does `IRETQ` do differently from a normal `ret` instruction?
5. After a context switch to a user task via IRETQ with USER_CS, can that task
   read kernel memory? Why or why not?
6. In `spawn()`, why is `ctx.cs = USER_CS` important? What would happen if you
   used `KERNEL_CS` instead?

---

## Exercise: Add a `ps` command to the terminal

The terminal in `kernel/src/gui/terminal.rs` handles commands. Add a `ps` command
that prints each running task's index, name, and state.

You'll need:
- `kernel/src/kernel/proc/scheduler.rs` — `task_infos()` already exists
- `kernel/src/gui/terminal.rs` — find where commands like `ls` are handled

This is fully manual — write it without asking Claude. The hardest part is
converting the `[u8; 32]` name to a `&str` — figure out how the existing
`name_str()` method works.

---

## Your notes
<!-- Add your own notes here as you study -->
