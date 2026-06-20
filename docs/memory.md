# Memory Management — Design Notes

For the paging/allocator walkthrough, see
[oxide_cocepts/03_memory_management.md](oxide_cocepts/03_memory_management.md)
and [study/04_memory.md](study/04_memory.md). This doc covers the *why*
behind the choices.

## Two separate allocators, on purpose

OxideOS has two independent allocators rather than one:

1. **Physical frame allocator** (`kernel/src/kernel/mem/paging_allocator.rs`)
   — hands out raw 4 KB physical frames, tracked by a bitmap.
2. **Kernel heap allocator** (`kernel/src/kernel/mem/allocator.rs`) — backs
   Rust's `Vec`/`Box`/`String` inside the kernel itself, currently
   `linked_list_allocator` (replacing an earlier bump allocator that never
   freed memory).

These solve different problems: the frame allocator manages *physical*
memory handed out a page at a time (to processes, to the heap allocator
itself for its own backing storage, to page tables), while the heap
allocator manages *sub-page* allocations within whatever virtual range it's
been given. Conflating them would mean every small kernel `Vec` allocation
wastes a full page, or that physical-frame bookkeeping has to understand
byte-granularity allocation — neither is worth it.

## Bitmap frame allocator, capped at 256 MB

The frame allocator is a fixed `[u64; 1024]` bitmap covering exactly
65536 frames (256 MB). Physical memory beyond that is invisible to the
allocator — not detected incorrectly, just never tracked or handed out.

- **Why a bitmap and not a free-list:** a bitmap is trivially correct (one
  bit per frame, no pointer chasing, no use-after-free class of bugs) and
  cheap to scan at boot. The known cost is allocation is O(n) in the worst
  case (linear scan from `next_frame`), which is the explicit motivation
  for Phase 11.6 in `docs/plan.md` (a stack-based free list for O(1)
  alloc/free) — this is recognized debt, not an oversight.
- **Why 256 MB:** it's a fixed, simple bound that comfortably covers a
  QEMU/dev VM without dynamically sizing the bitmap from the Limine memory
  map. The real cost is this is a hard ceiling, not a soft default — boot
  on a host with more RAM than 256 MB doesn't crash, it just leaves the
  rest of physical memory permanently unused.

## Copy-on-write fork: refcounted frames

`fork()` doesn't copy a child's address space immediately. Instead, every
writable user page is marked read-only in both parent and child page
tables, a bit 9 PTE flag marks it COW, and a parallel `[u16; 65536]`
refcount array (indexed by frame number, alongside the bitmap) tracks how
many page tables currently point at that frame.

- The page-fault handler for a COW-marked page checks the refcount: if it's
  ≤ 1 (no other owner), the fault just reclaims write access in place; if
  it's shared, a new frame is allocated, the data copied, the page
  remapped, and the old frame's refcount decremented.
- This is the standard approach because `fork()`+`exec()` is by far the
  common case (shell pipelines, process spawning) — copying the entire
  address space up front would make every `fork()` pay for memory the
  child usually never even touches before calling `exec()`.

## No swap, no file-backed mmap

All memory is anonymous (`brk`/`sbrk`, `mmap` anonymous). There's no swap
device and no demand-paged file-backed `mmap` yet (Phase 11.3/11.4 in
`docs/plan.md`).

- This means physical memory pressure has no fallback — a workload that
  exceeds available frames gets `ENOMEM`, not slow disk-backed paging.
  For a hobby/learning OS targeting VMs with generous RAM this trade is
  fine; it would not be for a real deployment target.
- File-backed `mmap` (mapping an executable's segments lazily instead of
  reading them eagerly at `exec()` time) is the more valuable of the two
  missing pieces and is the explicit prerequisite for dynamic linking
  (Phase 15), since a dynamic loader needs to map `.so` files on demand.
