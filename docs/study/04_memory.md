# 04 — Memory: Bump Allocator and Page Tables

Memory management is the most abstract part of the OS — but your kernel has
two concrete implementations you can read. Start simple (bump allocator),
then go deeper (page tables).

---

## Two questions memory management answers

1. **"Where can I put this data?"** → allocation (bump allocator, `allocator.rs`)
2. **"Which program owns which bytes?"** → isolation (page tables, `paging_allocator.rs`)

These are separate concerns. You can understand #1 without fully understanding #2.

---

## Part A: The Bump Allocator — `kernel/src/kernel/mem/allocator.rs`

### The idea

A bump allocator is the simplest possible allocator:

```
[  used  |  used  |  used  |          free          ]
0      next                                        end
```

Allocating N bytes just moves `next` forward by N. That's it.

**It never frees memory.** Once something is allocated, it stays allocated forever.
This is fine for the kernel's permanent data structures (the IDT, page tables, etc.)
which are allocated once at boot and never released.

### Read `allocator.rs`

**`BumpAllocator` struct (line 57):** three fields:
- `regions: [MemRegion; MAX_REGIONS]` — usable RAM segments from the bootloader
- `current_region: usize` — which segment we're currently allocating from
- `next: usize` — the current bump pointer

**`init()` (line 75):** called once at boot. Parses the Limine memory map:
- Iterates all memory map entries
- Keeps only `EntryType::Usable` regions (skips firmware, ACPI tables, etc.)
- Skips the first 1MB (legacy devices live there: VGA memory, ROM, etc.)
- Skips the kernel's own image (can't allocate over ourselves!)

**`allocate()` (line 34, inner) / `alloc()` (line 234, GlobalAlloc impl):**
- Round `next` up to the required alignment
- Check that `next + size` fits in current region
- If not, try the next region
- Return `next` as the pointer, advance `next` by `size`

**`GlobalAlloc` impl (line 233):** this is the hook that makes `Box`, `Vec`, `String`
work. When Rust does `Box::new(thing)`, it calls `GlobalAlloc::alloc()` on the
global allocator — which is your `BumpAllocator`.

### What to notice

- `unsafe impl GlobalAlloc` — allocation is inherently unsafe because raw pointers
- The `static ALLOCATOR: BumpAllocator` (line 290) — this is the global; it must be
  available at link time, hence `const fn new()`
- `alloc_error_handler` (line 315) — called when allocation fails; it panics.
  In kernel code, there's no graceful OOM recovery — running out of memory is fatal.

---

## Part B: Virtual Memory and Page Tables — `kernel/src/kernel/mem/paging_allocator.rs`

This is harder. Take it slow.

### The core concept

Every memory address your code uses is a **virtual address**. The CPU's MMU
(Memory Management Unit) translates it to a **physical address** using page tables
before actually accessing RAM.

```
virtual address 0x0000_0000_1234_5678
        ↓  (MMU hardware, using page tables)
physical address 0x0000_0004_ABCD_E678
```

This indirection enables:
- **Isolation**: process A's virtual address 0x1000 maps to different physical memory
  than process B's virtual address 0x1000
- **Protection**: a page can be marked read-only or no-execute in the page table

### Four-level paging (x86-64)

A 64-bit virtual address is split into five fields:

```
bits 63-48: sign extension (ignored)
bits 47-39: PML4 index    (L4 table, pointed to by CR3)
bits 38-30: PDP index     (L3 table)
bits 29-21: PD index      (L2 table)
bits 20-12: PT index      (L1 table)
bits 11-0:  page offset   (byte within the 4KB page)
```

Each table has 512 entries (9 bits = 2^9 = 512). Each entry either:
- Points to the next-level table's physical address (with Present flag set)
- Is empty (translation will fault)

The CPU walks this tree automatically on every memory access, using **CR3** as
the root pointer. Switching CR3 = switching the entire address space.

### Read `paging_allocator.rs`

**`PageTableEntry` (line 51):** 8-byte value with a physical address in bits 12–51
and flags in bits 0–11:
- `PRESENT` (bit 0) — entry is valid
- `WRITABLE` (bit 1) — writes allowed
- `USER` (bit 2) — Ring 3 can access this page
- `NO_EXECUTE` (bit 63) — can't jump to this page

**`PageTable` (line 65):** just `[PageTableEntry; 512]` — exactly 4096 bytes,
one page.

**`PhysicalFrameAllocator` (line ~84):** manages the freelist of physical 4KB frames
not used by the kernel. `allocate_frame()` pops one off; future work can push freed
frames back.

**`PageTableManager::map()` (line 245):** given a virtual address and a physical
frame, walks (or creates) the four-level table and installs the mapping.
This is the function that makes virtual memory work.

**CR3 and process isolation:** in `scheduler.rs`, every task has its own `cr3`
field. On a context switch, `mov cr3, rax` with the new task's CR3 instantly
switches the entire virtual address space. The old process's pages disappear;
the new process's pages appear.

### Key insight

The kernel's own mappings (code, stack, heap) live in the **higher half**
(addresses above `0xFFFF_8000_0000_0000`). User programs live in the **lower half**
(below `0x0000_8000_0000_0000`). Because the higher half mappings are present in
*every* process's page table (they're copied in when a new process CR3 is created),
the kernel can always access its own data even when running in a user process's
address space. This is necessary for handling syscalls.

---

## Questions

1. Why can't the bump allocator free memory? What data structure would you need
   to track free regions?
2. What is `GlobalAlloc`? Why does implementing it make `Box<T>` work?
3. What does changing CR3 actually do? What stays the same, what changes?
4. Why are kernel mappings in the higher half instead of, say, starting at 0?
5. What does the `NO_EXECUTE` flag prevent? Why is it a security feature?
6. What happens if a process tries to write to a page mapped with only `PRESENT | USER`
   (no `WRITABLE`)? Who gets notified and how?

---

## Exercise: Implement a free-list allocator (conceptual)

On paper (or in a separate `.rs` scratch file), design a simple free-list allocator
that *can* free memory:

```rust
struct FreeBlock {
    size: usize,
    next: Option<*mut FreeBlock>,  // linked list
}
```

- How would `alloc(size)` work? (find a free block big enough, split if too large)
- How would `dealloc(ptr, size)` work? (add block back to list, coalesce neighbors)
- What makes this harder than the bump allocator?

You don't need to make it compile — understanding the design is the goal.

---

## Your notes
<!-- Add your own notes here as you study -->
