# 00 — Rust for OS Readers

You know operating systems concepts (we've already covered paging, and the
`docs/study/01`–`07` series covers interrupts, memory, processes, syscalls,
drivers). This document is the missing piece: **the Rust itself.** Every
example below is real code from this repository, not a textbook snippet —
so once you're through this, the rest of the codebase reads as "OS logic
wearing Rust syntax" instead of two unfamiliar things at once.

Read this once, top to bottom, with the referenced files open. Then start
`01_mental_model.md` — you'll recognize the syntax and can focus purely on
the OS ideas.

---

## 0. The one-sentence mental model

Rust's entire personality comes from one rule: **every value has exactly
one owner, and the compiler tracks that at compile time — not at runtime,
not with a garbage collector.** No GC, no `malloc`/`free` pairs to get
wrong, no null pointers by default, no exceptions. Instead: ownership,
borrowing, and two enum types (`Option`, `Result`) do the jobs that other
languages hand to the runtime.

An OS kernel is one of the least forgiving places to get memory management
wrong — there's no OS underneath *you* to catch the fault. That's why
OxideOS is written in Rust instead of C: most of C's classic kernel bugs
(use-after-free, double-free, null derefs, buffer overruns) are caught at
`cargo build` time rather than at 3am in a VM.

---

## 1. Freestanding Rust: `#![no_std]`, `#![no_main]`

Open `kernel/src/main.rs:17-19`:

```rust
#![no_std]
#![no_main]
#![cfg_attr(target_arch = "x86_64", feature(abi_x86_interrupt))]
```

Normal Rust programs link against the **standard library** (`std`), which
assumes an OS underneath them — files, threads, heap allocation via the
system allocator, a `main()` the OS calls into after setting up a
runtime. A kernel *is* the thing that would normally provide all of that,
so none of it exists yet when this code starts running.

- `#![no_std]` — don't link `std`. You still get **`core`** (the
  OS-independent subset: types, traits, `Option`/`Result`, slices,
  `match`, iterators — everything that doesn't need an allocator or an
  OS) and, since this kernel does want heap types, `alloc` is opted back
  in explicitly at `main.rs:37`:
  ```rust
  extern crate alloc;
  ```
  That single line is what makes `Vec`, `Box`, `String` available
  throughout the kernel — but only once *something* has told Rust how to
  actually get memory, which is the `#[global_allocator]` you already
  read about in the paging-allocator conversation
  (`kernel/src/kernel/mem/paging_allocator.rs:596-597`).
- `#![no_main]` — skip Rust's normal startup (the bit that would call
  `fn main()` after setting up `argv`, stack guards, etc. — none of which
  exist yet). Instead the kernel defines its own entry point by hand:
  ```rust
  #[unsafe(no_mangle)]
  unsafe extern "C" fn kmain() -> ! { ... }        // main.rs:136
  ```
  `#[unsafe(no_mangle)]` stops the compiler from renaming `kmain` to
  something compiler-internal (Rust normally mangles symbol names so
  overloaded/generic functions don't collide) — the bootloader (Limine)
  needs to jump to a symbol literally named `kmain`. `extern "C"` picks
  the C calling convention so the jump-from-assembly handoff has a
  well-defined ABI. The return type `!` ("never") is Rust's way of saying
  *this function does not return* — fitting, since a kernel entry point
  either loops forever or halts the machine.

---

## 2. Ownership and borrowing, in code you've already seen

You don't `free()` in Rust; you don't garbage-collect either. Every value
has one **owner** (a variable). When the owner goes out of scope, the
value is dropped. You can temporarily **borrow** a value with `&` (shared,
read-only, any number of them at once) or `&mut` (exclusive, only one at a
time, and not alongside any `&`). The compiler enforces this at compile
time — this is the "borrow checker," and it's the thing people mean when
they say Rust "fights you" until you internalize it.

Why it matters *here*: `&mut` exclusivity is what makes data races
impossible to compile, and it's why so much of a kernel — which is all
about shared mutable state (page tables, the scheduler, device registers)
— ends up needing `unsafe` (§7) to get around the borrow checker in
places where *you*, not the compiler, know the aliasing is safe.

A concrete example — `PageTableManager::map` in
`paging_allocator.rs:286-291`:

```rust
unsafe fn map(
    &mut self,
    virt:        u64,
    phys:        u64,
    flags:       PageTableFlags,
    frame_alloc: &mut PhysicalFrameAllocator,
) -> Result<(), &'static str> {
```

`&mut self` — this method needs exclusive access to the `PageTableManager`
it's called on (it's about to mutate the table tree). `frame_alloc: &mut
PhysicalFrameAllocator` — same idea, borrowed from the caller rather than
owned, because the frame allocator is a long-lived singleton that many
functions need to mutate over time, not something `map` should consume.

---

## 3. Structs and enums: how OxideOS models data

### Structs — a bundle of named fields

`Task` in `kernel/src/kernel/proc/scheduler.rs:129` is the struct behind
every running process — 30+ fields, one process-control-block:

```rust
pub struct Task {
    pub state:      TaskState,
    pub ctx:        TaskContext,
    pub name:       [u8; 16],
    pub cr3:        u64,
    pub pid:        u8,
    pub fd_table:   FdTable,
    pub signal_handlers: [u64; NSIG],
    // ... (30+ fields total)
}
```

Nothing exotic — this is a C `struct` with the same layout intuition.
`[u8; 16]` is a fixed-size array (not a `Vec` — no heap allocation, size
known at compile time, which matters a lot in kernel code where you often
can't assume a working allocator yet).

### Enums — not what C calls an enum

C enums are named integers. Rust enums are **tagged unions**: each variant
can carry its own different data, and the compiler forces you to handle
every variant (§4). This is the single biggest "aha" moment for people
coming from C — once it clicks, half of Rust's safety story clicks with
it.

`TaskState` in `scheduler.rs:35-42` is the cleanest example in the whole
codebase:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaskState {
    Empty,
    Ready,
    Running,
    Sleeping(u64),           // wake at this tick
    Waiting(u8),             // waiting for child with this PID to die
    WaitingForMsg(u32, u64), // blocking msgrcv: (queue_id, user msg_out ptr)
    Dead(i64),               // exit code (pages already freed)
}
```

A task isn't just "sleeping" — it's "sleeping, and here's the tick at
which to wake it," carried *inside* the enum value itself, with no
separate "is this field meaningful right now" bookkeeping. In C you'd
model this as a status int plus a union plus a comment explaining which
union field is valid for which status — and nothing stops that comment
from going stale. Here, the compiler makes it structurally impossible to
read `wake_tick` while the state is `Running`.

`Syscall` (`kernel/src/kernel/sys/syscall_core.rs:20-24`) is the other
common enum shape — no payload data, just named values, but pinned to
specific integer representations so they line up with Linux's syscall
numbers for musl-binary compatibility:

```rust
#[repr(u64)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    Read          = 0,
    Write         = 1,
    Open          = 2,
    Close         = 3,
    // ...
}
```

`#[repr(u64)]` pins the memory layout to a plain `u64` (by default Rust
doesn't promise you *any* particular layout for an enum — it picks
whatever's efficient). Converting a raw number *into* the enum is done
explicitly rather than by casting, via a `From` impl
(`syscall_core.rs:322-345`, an example of a **trait**, covered in §5):

```rust
impl From<u64> for Syscall {
    fn from(num: u64) -> Self {
        match num {
            0 => Self::Read,
            1 => Self::Write,
            // ...
        }
    }
}
```

---

## 4. `Option`, `Result`, and pattern matching: no null, no exceptions

Two enums from `core` show up on almost every page of this codebase:

```rust
enum Option<T> { Some(T), None }
enum Result<T, E> { Ok(T), Err(E) }
```

`Option<T>` replaces null pointers: "maybe a value, maybe not," and the
compiler won't let you use the inner value without first checking which
case you're in. `Result<T, E>` replaces exceptions: "succeeded with a
value, or failed with an error," returned like any other value, never
thrown/caught.

You've already seen both in the paging allocator:

```rust
fn allocate_frame(&mut self) -> Option<u64> { ... }   // paging_allocator.rs:217

unsafe fn map(&mut self, ...) -> Result<(), &'static str> { ... }  // :286
```

`allocate_frame` returns `Option<u64>` — either `Some(physical_address)`
or `None` if physical memory is exhausted. There's no sentinel "return 0
for failure" the way you might in C, because `0` is a perfectly valid
physical address and conflating "no frame" with "frame zero" is exactly
the kind of bug this type exists to prevent. `map` returns `Result<(),
&'static str>` — success carries no data (`()`, the empty tuple, "unit" —
Rust's spelling of "nothing"), failure carries a `&'static str` error
message.

### Getting the value out: `match`, `if let`, `let else`, `?`

**`match`** is exhaustive — miss a variant and it's a compile error, which
is exactly what you want for something as consequential as "did the
memory allocation succeed":

```rust
let phys = match inner.frame_allocator.allocate_frame() {
    Some(a) => a,
    None => {
        SERIAL_PORT.write_str("PAGING ALLOCATOR: Out of frames!\n");
        return None;
    }
};
```
(`paging_allocator.rs:513-519`, lightly trimmed)

**`if let`** is `match` for when you only care about one variant:

```rust
if let TaskState::Dead(code) = self { Some(code) } else { None }
```
(`scheduler.rs:47`, inside `TaskState::exit_code`)

**`let else`** — "bind this, or bail out right here" — is newer Rust
syntax, and reads the cleanest of all three for early-return guard
clauses. From `main.rs:56, 65`:

```rust
let Some(resp) = memory_map.get_response() else { return 0 };
// ... resp is a plain MemoryMapResponse from here on, not an Option ...
let Some((base, len)) = best else { return 0 };
```

**`?`** propagates an `Err`/`None` up to the caller instead of handling it
locally — "if this failed, make *my* return value reflect that failure
too, and stop here." From `paging_allocator.rs:307`:

```rust
let t = frame_alloc.allocate_frame().ok_or("OOM: L3 table")?;
```
`.ok_or("OOM: L3 table")` first turns the `Option<u64>` into a
`Result<u64, &str>` (supplying the error to use if it was `None`), then
`?` either unwraps the `Ok` value into `t` or immediately returns
`Err("OOM: L3 table")` from the enclosing function (which is why every
function using `?` this way is itself declared to return a `Result`).

---

## 5. Traits: shared behavior without inheritance

Rust has no class inheritance. Instead, a **trait** declares a set of
methods; any type can `impl` that trait to promise it supports them. It's
closer to a Java/Go interface than a C++ base class — no shared state, no
diamond problem, and crucially, traits can be implemented for types you
don't own (like implementing a trait from an external crate for your own
struct).

The clearest example is `SyscallRuntime`
(`kernel/src/kernel/sys/syscall_core.rs:523`) — it's the seam between
*mechanism* (parsing/dispatching a syscall number) and *policy*
(actually running the kernel):

```rust
pub trait SyscallRuntime {
    fn trace(&mut self, _syscall: Syscall) {}
    fn current_pid(&self) -> u64 { 1 }
    fn current_ticks(&self) -> u64;
    fn write_console(&mut self, bytes: &[u8]);
    fn sleep_until_tick(&mut self, target_tick: u64);
    fn exit(&mut self, code: i32) -> !;
    // ...
}
```

Notice two things:
- Some methods have a body right in the trait (`fn trace(&mut self, ...)
  {}` — a default no-op) — implementors only need to override the ones
  where the default isn't right. Others (`fn current_ticks(&self) ->
  u64;`) have no body — every implementor *must* provide one.
- `KernelRuntime` provides the real implementation
  (`kernel/src/kernel/sys/syscall.rs:14`: `impl SyscallRuntime for
  KernelRuntime`). The syscall-dispatch code in `syscall_core.rs` is
  written entirely against the trait, not against `KernelRuntime`
  directly — which is what lets `run_boot_self_tests()` (referenced from
  `main.rs:151`) exercise the *same* dispatch logic against a fake/test
  runtime without booting real hardware.

You'll also see traits used the conventional "plug into someone else's
interface" way — `kernel/src/kernel/drivers/net/stack.rs:58-60` implements
`smoltcp`'s `Device` trait for OxideOS's own NIC wrapper, which is what
lets an off-the-shelf TCP/IP stack crate drive OxideOS's real network
card without smoltcp knowing anything about this OS.

---

## 6. Generics and lifetimes

### Generics — write the logic once, for any type

`fn with_interrupts_disabled<T>(f: impl FnOnce() -> T) -> T`
(`kernel/src/gui/terminal.rs:58`) works for a closure that returns
*anything* — a `u32`, a `String`, `()` — because `T` is a placeholder
filled in at each call site, not a concrete type baked into the function.
This is the same idea as C++ templates or Java generics, resolved at
compile time (monomorphization: the compiler generates a separate copy of
the function per concrete `T` actually used, so there's no runtime cost).

`alloc_for_type<T>()` (`kernel/src/kernel/mem/allocator.rs:350`) is a
kernel-flavored use: allocate exactly `size_of::<T>()` bytes, correctly
aligned for `T`, however many bytes that turns out to be for whatever
type the caller asks for.

### Lifetimes — how long is this borrow valid?

A `&T` or `&mut T` reference can't outlive the value it points at — the
compiler checks this too, and **lifetimes** are the syntax for naming
"how long," usually only needed when the relationship isn't obvious from
context alone. `OxideBackend<'a>`
(`kernel/src/gui/oxide_backend.rs:101-102`):

```rust
pub struct OxideBackend<'a> {
    gfx: &'a Graphics,
    // ...
}
```

says "a `OxideBackend` holds a borrowed `&Graphics`, and the borrow is
tagged `'a`" — which forces every `OxideBackend` value to not outlive the
`Graphics` it was built from. You don't invent a lifetime value; `'a` is
just a name the compiler uses to check that the struct's borrow and the
`Graphics` it points into stay in the right order. Most code you'll read
in this repo never needs an explicit lifetime at all — the compiler infers
them — they only show up when a struct (not just a function) stores a
reference, as here.

---

## 7. `unsafe`: the part that makes a kernel possible at all

Safe Rust's guarantees (§2) come from *disallowing* certain things: raw
pointer dereferencing, calling into non-Rust code, touching mutable
global state from multiple places, reinterpreting one type as another.
Every one of those is something a kernel has to do routinely — reading a
byte from a hardware I/O port isn't optional. `unsafe` is the keyword that
tells the compiler "I've checked this by hand; stop enforcing this
particular rule here."

`unsafe` does **not** turn off the borrow checker or type checking — only
a handful of specific operations become legal:
- Dereferencing a raw pointer (`*const T` / `*mut T`, as opposed to `&T`)
- Calling an `unsafe fn` (a function whose author is telling you *you*
  must uphold some invariant they can't check for you)
- Reading or writing a `static mut` (see §9)
- Inline assembly (`core::arch::asm!`)
- Implementing certain traits (`unsafe impl Sync`, `unsafe impl
  GlobalAlloc`) whose contract the compiler can't verify

The whole point of the pattern used throughout this kernel is to build a
**small unsafe core, wrapped in a safe(r) public surface** — do the unsafe
thing once, in one audited place, then expose an interface that's hard to
misuse. `PhysicalFrameAllocator::allocate_frame`
(`paging_allocator.rs:217-230`) is entirely safe Rust internally — bitmap
math on plain integers — even though it lives in a file full of `unsafe`,
because touching a bitmap in memory you already own isn't actually
unsafe. Compare `PageTableManager::map`, which *is* `unsafe fn` end to
end, because it dereferences raw physical-address pointers
(`self.get_table(...)`) and executes `invlpg` inline assembly
(`paging_allocator.rs:340`) — operations the compiler has no way to check
are valid, so the function's signature itself carries the warning:
calling this wrong can corrupt memory, and it's on the caller to have
checked.

`main.rs` is the highest concentration of `unsafe` in the whole codebase,
because boot is nothing *but* operations the compiler can't verify (poking
hardware before anything is initialized):

```rust
unsafe { SERIAL_PORT.init(); }
unsafe { boot_init::init_interrupt_system(); }
```

each individually-scoped `unsafe { }` block is a deliberate style choice
here — it marks, line by line, exactly which operation required trusting
the programmer, rather than one giant `unsafe fn kmain` where a reader
can't tell which of the 300 lines actually needed it.

---

## 8. Closures and iterators

A **closure** is an anonymous function that can capture variables from
its surrounding scope — `|x| x + 1` is a closure taking one argument.
`FnOnce`/`Fn`/`FnMut` (seen in §6's `with_interrupts_disabled`) are traits
describing *how* a closure is allowed to use its captures (consume once,
read-only many times, or mutate).

Rust favors **iterator chains** over hand-rolled index loops — from
`main.rs:60`:

```rust
if best.map(|(_, len)| entry.length > len).unwrap_or(true) {
```

`best` is an `Option<(u64, u64)>`. `.map(closure)` runs the closure *only*
if `best` is `Some`, transforming the inner value and leaving `None`
untouched — here, turning "the current best region" into "is this new
entry bigger than it." `.unwrap_or(true)` then supplies `true` if `best`
was `None` (nothing to compare against yet, so this entry automatically
wins). Read as English: "if there's no best region yet, or this one is
bigger than the current best." No manual `if let`/`else` needed for what
would otherwise be a three-way branch.

`panic.rs:521` shows the other extremely common shape,
`.iter().enumerate()`:

```rust
for (i, (name, val)) in regs.iter().enumerate() {
```

`.iter()` produces references to each element without consuming the
collection (so `regs` is still usable afterward); `.enumerate()` pairs
each item with its index. This one line replaces what would be a manual
index variable, bounds-checked array access, and increment in C.

---

## 9. Global mutable state: `static`, `static mut`, and why it's special-cased

Safe Rust normally refuses to let you have a mutable variable that
multiple parts of the program can reach and modify at once — that's
exactly the shared-mutable-state hazard the ownership model exists to
rule out. A kernel can't avoid it: the scheduler's task table, the frame
allocator's bitmap, the window manager — these *are* global mutable
state, reachable from interrupt handlers, syscalls, and the main loop all
at once.

```rust
pub static mut SCHED: Scheduler = Scheduler::new();      // scheduler.rs:238
pub static mut WINDOW_MANAGER: gui::window_manager::WindowManager =
    gui::window_manager::WindowManager::new();             // main.rs:123-124
```

Every read or write of a `static mut` requires an `unsafe` block — the
compiler is telling you "I can't verify no one else is touching this at
the same time; that's now your job." In a single-threaded, cooperatively-
scheduled kernel like this one (no SMP yet), the actual hazard is
narrower — reentrancy from an interrupt firing mid-access, not a second
CPU — but the type system doesn't know that distinction, so the
`unsafe` marker is required regardless.

(Contrast this with `PagingAllocator` in `paging_allocator.rs:401-405`,
which wraps its mutable state in `UnsafeCell` and manually asserts `unsafe
impl Sync` — a more deliberate version of the same trade-off, used because
it also has to satisfy `#[global_allocator]`'s requirement that the
allocator type be usable from a plain `static`, not `static mut`.)

---

## 10. Macros: `macro_rules!` and `#[derive(...)]`

Two different macro mechanisms show up constantly:

**`#[derive(...)]`** generates trait implementations for a type
mechanically, instead of you writing boilerplate by hand:

```rust
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TaskState { ... }
```

`Clone`/`Copy` make the value copyable by simple bit-copy (cheap — fine
for small enums/structs like this one, not fine for anything owning a
heap allocation); `PartialEq` gives you `==`; `Debug` gives you `{:?}`
formatting for logging. Each is a trait (§5) — `derive` just writes the
`impl` for you instead of you typing it.

**`macro_rules!`** defines your own macro by pattern — `serial_println!`
(`kernel/src/kernel/drivers/serial.rs:201`) and `kernel_panic!`
(`kernel/src/panic.rs:591`) are how this kernel gets `println!`-style
convenience without `std`'s `println!` (which assumes an OS stdout that
doesn't exist here) — they expand, at compile time, into calls to
`SERIAL_PORT.write_str(...)`/`write_fmt(...)`.

---

## 11. How the files fit together: `mod`, `pub`, `use`

Rust's module tree usually mirrors the directory tree. `kernel/src/kernel/mod.rs`
is the "table of contents" for everything under `kernel/src/kernel/`:

```rust
pub mod drivers;  // -> kernel/drivers/mod.rs (and everything drivers/ declares)
pub mod arch;
#[cfg(target_arch = "x86_64")]
pub mod mem;
```

`pub mod drivers;` says "there's a submodule named `drivers`, defined in
`drivers/mod.rs` (or `drivers.rs`), and it's visible to code outside this
module." Anything declared `pub` inside a module is reachable from
outside it via a path like `crate::kernel::drivers::serial::SERIAL_PORT`
— `crate` means "the root of this compilation unit" (here, `main.rs`).

The `#[cfg(target_arch = "x86_64")]` you'll see scattered everywhere
(including right above `pub mod mem;`) is a **compile-time** conditional
— on an `aarch64` build, that line doesn't exist at all as far as the
compiler is concerned. This is how one source tree produces two very
different kernels (x86_64 and aarch64) without runtime `if`s checking the
architecture — the wrong-architecture code is never even compiled in, let
alone executed.

`kernel/mod.rs` also re-exports everything at a flat path for
convenience:

```rust
pub use drivers::serial;
```

lets other files write `crate::kernel::serial::SERIAL_PORT` instead of
the deeper `crate::kernel::drivers::serial::SERIAL_PORT` — a `pub use` is
just "also make this name available at this shorter path," it doesn't
move or copy any code.

---

## How to read the rest of this codebase

You now have every Rust construct that recurs constantly:
`no_std`/`no_main`, ownership/borrowing, structs, data-carrying enums,
`Option`/`Result`/`match`/`?`, traits, generics, lifetimes, `unsafe`,
closures/iterators, `static mut`, and the two macro systems. Nothing past
this point introduces genuinely new *language* mechanics — it's the same
dozen tools recombined.

Suggested order from here:

1. **`01_mental_model.md`** — the layered architecture (you already have
   the OS-concepts version of this from our paging discussion).
2. **`02_interrupts.md` → `07_drivers.md`**, in order — each names exact
   files and line numbers. When you hit a construct you don't recognize,
   it's probably not here: check `docs/oxide_cocepts/` for the matching
   *OS-concept* chapter, or come back to this document's table of
   contents.
3. Keep a second terminal open and actually run
   `grep -n "fn function_name" path/to/file.rs` as you go — line numbers
   in every doc here drift as the code evolves; function names don't.

The one habit worth building deliberately: every time you hit an `unsafe`
block, stop and ask "what invariant is this function relying on that the
compiler can't check?" That question — asked over and over — is most of
what it means to actually read systems Rust.
