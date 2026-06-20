# Scheduler — Design Notes

For the process-lifecycle walkthrough, see
[study/05_processes.md](study/05_processes.md). This doc covers the *why*
behind the choices, grounded in `kernel/src/kernel/proc/scheduler.rs`.

## Fixed 8-task array, not a dynamic list

`MAX_TASKS = 8`: the task table is a fixed-size `[Task; 8]` array, not a
heap-allocated list. Creating a 9th process simply fails rather than
growing the table.

- A fixed array means the scheduler's hot path (find the next `Ready` task)
  never allocates and never needs a lock around a resizable structure —
  it's a bounded loop over a known-size array, which is exactly the kind
  of code you want in an interrupt context.
- The cost is an arbitrary, low ceiling on concurrent processes. Raising it
  is a one-line constant change today (no design barrier), but doing it
  "properly" (a growable table) would mean deciding how task-table growth
  interacts with code that currently assumes a fixed array size end to end
  (FD-table-per-task sizing, the round-robin index wraparound, etc.) —
  that audit hasn't been done, so the constant has stayed conservative.

## Strict round-robin, no priority

The next task is simply "the next `Ready` slot after the current one,
wrapping around" — no priority levels, no nice values, no fairness
weighting beyond equal time slices (2 ticks @ 100 Hz = 20 ms each).

- This is the simplest scheduling policy that's still preemptive and fair
  by construction (every runnable task gets an equal share over time). For
  a single-user desktop/dev OS where nothing is fighting for CPU under
  real load, the lack of priority isn't yet a felt limitation.
- It will become one the moment something latency-sensitive (audio,
  input handling under load) needs to preempt a CPU-bound task sooner than
  "wait your turn in the ring" — there's no mechanism for that today.

## Context switches: timer IRQ + explicit voluntary yields

Preemption happens on the timer IRQ when a task's slice expires. Tasks also
yield voluntarily on blocking syscalls — `sleep`, `waitpid`, `msgrcv` — by
setting their own state to `Sleeping`/`Waiting`/`WaitingForMsg` and calling
into the scheduler rather than spinning.

- Voluntary yield on blocking calls matters because otherwise a task
  blocked on `waitpid` would burn its entire timeslice doing nothing until
  the *next* timer tick gave another task a turn — explicit yield hands
  the CPU over immediately instead of wasting up to 20 ms per blocking
  call.

## Single-core only — no per-CPU state

There's exactly one `CURRENT_TASK_IDX` global and no per-CPU scheduler
state of any kind. This isn't an optimization left on the table; it's a
direct consequence of the PIC-based interrupt model (see
[interrupts.md](interrupts.md)) — APIC is the prerequisite for routing
interrupts to multiple cores at all, and SMP (Phase 17) is scoped as its
own phase precisely because moving the scheduler to per-CPU run queues
and locking the shared task table properly is a substantial change, not
an incremental one on top of today's single global index.
