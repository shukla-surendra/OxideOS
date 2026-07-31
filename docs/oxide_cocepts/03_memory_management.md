# Chapter 3: Memory Management — Paging

Once the CPU and interrupt systems are configured, the kernel needs the
ability to dynamically allocate memory and to isolate one program's memory
from another's. Both of these rely on **paging**. This document first
explains paging as a general OS concept, then walks through exactly how
OxideOS implements it, file by file.

For the design rationale (*why* these choices, not just *what* they are),
see [`docs/memory.md`](../memory.md). For a slower, exercise-driven walk
through the code, see [`docs/study/04_memory.md`](../study/04_memory.md).

---

## Part 1: Paging as an OS concept

### Virtual vs. physical memory

Physical RAM is a flat array of bytes, addressed 0 to (RAM size − 1). If
programs addressed RAM directly, any bug in one program could read or
overwrite any other program's memory — or the kernel's.

Instead, every program (and the kernel) works with **virtual addresses**.
The CPU's **Memory Management Unit (MMU)** translates each virtual address
into a physical address at the moment of access, using a data structure
called a **page table**. The OS builds and maintains that page table; the
MMU walks it in hardware on every load, store, and instruction fetch.

This indirection buys three things:
- **Isolation** — process A's virtual address `0x1000` and process B's
  virtual address `0x1000` can map to completely different physical
  frames. Neither can see or touch the other's memory.
- **Protection** — each mapping carries permission bits (readable,
  writable, executable, user-accessible), so the same mechanism that
  isolates processes also enforces things like "code pages aren't
  writable" and "the kernel isn't reachable from ring 3."
- **Flexibility** — a process's memory doesn't need to be physically
  contiguous. The OS can scatter its pages anywhere in RAM (or, in systems
  with swap, on disk) while the process still sees one contiguous address
  range.

### Pages and frames

Both virtual and physical memory are divided into fixed-size chunks — on
x86-64, normally **4 KiB**. A chunk of virtual memory is a **page**; the
matching chunk of physical memory is a **frame**. A page table maps pages
to frames; it never maps anything smaller.

### Four-level paging on x86-64

A 64-bit virtual address is split into five fields:

```
bits 63-48: sign extension (must match bit 47, ignored by translation)
bits 47-39: PML4 index  →  L4 table (root, pointed to by CR3)
bits 38-30: PDPT index  →  L3 table
bits 29-21: PD index    →  L2 table
bits 20-12: PT index    →  L1 table
bits 11-0:  page offset →  byte within the 4 KiB page
```

Each table has exactly 512 entries (9 bits of index). The CPU register
**CR3** holds the physical address of the root (L4) table for whichever
address space is currently active. To translate a virtual address, the
MMU:

1. Reads the L4 entry at `CR3 + (bits 47-39) × 8`.
2. If present, follows it to an L3 table and reads the entry at `bits 38-30`.
3. Repeats for L2 (`bits 29-21`) and L1 (`bits 20-12`).
4. The L1 entry ("the leaf") holds the physical frame address; the low 12
   bits of the virtual address are added unchanged as the offset into that
   frame.

Each entry also carries flags in its low bits: **Present** (is this
mapping valid at all), **Writable**, **User** (accessible from ring 3, not
just the kernel), and (in the high bit) **No-Execute**. If any table in
the chain is not present, or the access violates a permission bit, the CPU
raises a **page fault** (`#PF`, vector 14) instead of completing the
access, and reports the faulting address in the **CR2** register.

**Switching address spaces is a single instruction.** Loading a new value
into CR3 makes the CPU walk a completely different table tree for every
subsequent memory access — this is how a context switch between two
processes instantly swaps out the entire mapping without touching a single
byte of either process's memory.

### The TLB

Walking four tables on every memory access would be far too slow, so the
CPU caches recent virtual→physical translations in the **Translation
Lookaside Buffer (TLB)**. When the OS changes a mapping, it must
explicitly invalidate the stale TLB entry (`invlpg <addr>` for one page, or
implicitly on a full CR3 reload) — otherwise the CPU keeps translating
through a mapping that no longer matches the page table.

### What the OS actually does with all this

The hardware only walks tables and raises faults; the *policy* is entirely
software:
- Deciding which physical frames are free and handing them out.
- Building and tearing down page tables as processes are created, grow,
  and exit.
- Reserving a region of every address space for the kernel itself, so
  system calls and interrupt handlers always have a valid, mapped
  environment to run in regardless of which process was interrupted.
- Handling page faults: some are genuine errors (kill the process), others
  are meaningful events the OS uses to defer work — demand paging (map a
  page only when it's first touched), copy-on-write (share a page until
  someone writes to it), guard pages (detect stack overflow), swap
  (evict a page to disk under memory pressure).

The rest of this document is how OxideOS implements each of those pieces.

---

## Part 2: How OxideOS manages paging

All of this lives in one file:
[`kernel/src/kernel/mem/paging_allocator.rs`](../../kernel/src/kernel/mem/paging_allocator.rs).
It has three layers, from the bottom up.

### Layer 1 — `PhysicalFrameAllocator`: which frames are free

A fixed-size bitmap, `[u64; 1024]` (65536 bits = 65536 frames = **256 MB**
of trackable physical RAM). One bit per 4 KiB frame; `1` means used, `0`
means free.

- `init()` reads Limine's memory map, marks every `Usable` region's frames
  free, but skips the first 32 MB unconditionally (protects the kernel
  image and any boot-time structures Limine placed low in memory).
- It then calls `protect_page_table_frames()`, which walks the page table
  tree that Limine itself already built (starting from the current CR3)
  and marks every frame Limine used for an L4/L3/L2/L1 table as used. This
  matters because Limine's own tables live somewhere in usable memory —
  without this step the allocator could hand out a frame that's secretly
  still part of the live kernel page table and silently corrupt it.
- Physical memory past the 256 MB mark is simply never tracked — not
  detected wrong, just permanently invisible to the allocator. (See
  `docs/memory.md` for why this bound was chosen.)
- `allocate_frame()` does a linear scan from `next_frame` for a free bit
  (O(n) worst case — a known, documented limitation).
- Alongside the bitmap sits `refcount: [u16; 65536]`, one counter per
  frame. A normal allocation sets it to 1. `free_frame()` decrements it
  and only actually frees the frame (clears the bit) once it hits 0 — this
  is what lets a single physical frame be safely shared by more than one
  page table (see copy-on-write, below).

### Layer 2 — `PageTableManager`: walking and building the tree

This is the direct software counterpart to the hardware walk described in
Part 1.

- `PageTableEntry` is an 8-byte value: physical address in bits 12–51,
  flags in the low 12 bits and bit 63. `PageTableFlags` defines
  `PRESENT`, `WRITABLE`, `USER`, `NO_EXECUTE`, plus one OxideOS-specific
  bit: `COW` (bit 9). Bits 9–11 are architecturally ignored by the MMU for
  every paging-structure entry, which makes bit 9 safe to repurpose as a
  software-only "this page is copy-on-write" marker.
- `map(virt, phys, flags, frame_alloc)` walks L4 → L3 → L2 → L1 from the
  root table, **allocating a fresh frame from Layer 1 for any
  intermediate table that doesn't exist yet**, and installs the final
  leaf entry pointing at `phys`. It finishes with `invlpg` so the CPU
  doesn't keep using a stale (absent) translation for that address.
- `unmap(virt, frame_alloc)` walks the same path, clears the leaf entry,
  flushes the TLB, and returns the freed frame to Layer 1.
- Physical addresses are read back through the **HHDM** (higher-half
  direct map): Limine maps *all* physical RAM at a fixed offset
  (`0xFFFF_8000_0000_0000`), so the kernel can always dereference
  `phys + HHDM` to read or write a frame's contents directly — including
  the frames that make up other page tables — without needing them mapped
  anywhere else.

### Layer 3 — `PagingAllocator`: the kernel heap

This struct is registered as Rust's `#[global_allocator]`, so every
`Box`, `Vec`, and `String` the kernel code allocates flows through it. It
is deliberately two-tier:

1. **Slab** — at init, 16 MB of virtual address space starting at
   `0xFFFF_FF00_0000_0000` is eagerly mapped page-by-page via Layer 2,
   then handed to `linked_list_allocator::Heap`. Almost all kernel
   allocations (the common case: small/medium, short-lived) are served
   from here in O(1)-ish time, and — unlike a bump allocator — this layer
   actually frees memory on `dealloc`.
2. **Page-granularity fallback** — anything too large for the slab is
   satisfied by mapping fresh pages one at a time (bump-allocating a
   virtual range up to a 64 MB ceiling). On `dealloc`, those pages are
   unmapped and the virtual range is pushed onto a small recycling
   free-list (`FREE_LIST_CAPACITY = 256`) so a later same-size request can
   reuse the address range instead of consuming new virtual space forever.

### The higher half: how the kernel stays mapped everywhere

OxideOS splits the 64-bit address space at the halfway point:
- **Lower half** (L4 indices 0–255) — user-space, unique per process.
- **Higher half** (L4 indices 256–511) — the kernel: its code, its heap,
  the HHDM physical window. Identical in every process's page table.

Because the higher half is copied into every process's L4 table
(`create_user_page_table()` zeroes indices 0–255 and clones 256–511 from
the kernel's own L4), the kernel's code and data — and therefore every
interrupt handler and system call — remain reachable no matter which
process's CR3 is currently loaded. This is what makes it safe to take a
page fault or a timer interrupt while running arbitrary user code.

### Per-process address spaces

- `create_user_page_table()` — allocates one fresh frame for a new L4,
  zeroes the user half, clones the kernel half. Returns the physical
  address to load into CR3.
- `map_user_region_in` / `unmap_user_region_in` — map or unmap pages in a
  *specific* process's table (identified by its CR3) without touching the
  currently-loaded CR3, by temporarily pointing the `PageTableManager` at
  the target table.
- `free_user_page_table()` — on process exit, walks the user half of the
  table, frees every leaf data frame and every intermediate table frame,
  then frees the L4 frame itself. The kernel half is never touched, since
  those entries are shared pointers into the kernel's own tables, not this
  process's to free.

The scheduler stores each task's CR3 (see `docs/scheduler.md`); a context
switch is, at the paging level, nothing more than loading that value into
CR3.

### Copy-on-write `fork()`

`cow_fork_user_page_table()` builds the child's page table without
copying the whole address space up front:

- Intermediate table frames (L3/L2/L1) are always freshly allocated for
  the child, so parent and child can diverge independently afterward.
- Leaf **data** frames are classified by virtual address:
  - Inside an explicit shared-memory range: shared verbatim, no refcount
    change (shm semantics must survive fork unchanged).
  - Inside the stack range: deep-copied immediately, so kernel-side writes
    to the stack (e.g. delivering a signal frame) never land on a
    read-only COW page.
  - Everything else (the common case — heap, data, code): shared. The
    frame's refcount is incremented, and if it was writable, **both** the
    parent's and the child's PTE are rewritten read-only with the `COW`
    bit set — the parent's live mapping is flushed with `invlpg` on the
    spot, since the parent keeps running on the old CR3.

When either process later writes to a COW page, it takes a page fault.
`kernel/src/kernel/arch/x86_64/interrupts.rs` intercepts vector 14
specifically when it's a **user-mode write to a present page**
(`err_code & 0x3 == 0x3`) and hands it to
`try_resolve_cow_fault(cr2)` before falling through to the generic
exception handler:

- If the frame's refcount is `≤ 1` (no other owner left), the fault just
  flips the PTE back to writable in place — no copy needed.
- Otherwise, a new frame is allocated, the page's contents are copied into
  it, the faulting process's PTE is repointed at the new frame (writable,
  COW cleared), and the old frame's refcount is decremented.
- Either way the function returns `true` and the interrupt handler
  **retries the faulting instruction** rather than killing the task.

This is standard COW-fork behavior: `fork()` is usually followed
immediately by `exec()` (shell pipelines, process spawning), so eagerly
copying the whole address space would waste work on memory the child
typically never touches.

### Shared memory

`map_phys_pages_in()` differs from the ordinary mapping path in one way:
instead of allocating a fresh frame for each new mapping, it maps
*specific* physical frames (already allocated once via
`alloc_phys_frames()`) into a target process's table at a chosen virtual
address. This is the primitive shared-memory segments are built on: two
processes each get a mapping to the same underlying frames.

### What's out of scope today

There is no swap device and no demand-paged file-backed `mmap` — every
mapping is backed by anonymous memory that's resident for as long as it's
mapped. A workload that exhausts the 256 MB frame budget gets `ENOMEM`,
not disk-backed paging. See `docs/memory.md` and `docs/plan.md` (Phase
11.3/11.4/11.6) for the reasoning and what's planned next (a free-list
frame allocator for O(1) alloc/free, and file-backed `mmap`).
