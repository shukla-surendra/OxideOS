# 02 — Interrupts: From Hardware to Handler

Interrupts are the mechanism by which hardware tells the CPU "something happened —
stop what you're doing and handle it." Understanding interrupts means understanding
how your OS stays responsive without constantly polling every device.

---

## The problem interrupts solve

Without interrupts, the CPU would have to constantly ask:
"Keyboard: did anyone press a key? No? Okay. Timer: did you tick? No? Disk: done yet? ..."

This is called **polling** and it wastes 100% of the CPU. Instead, hardware signals
the CPU via an interrupt line. The CPU finishes its current instruction, saves its
state, and jumps to a handler. When the handler finishes, it restores state and
resumes exactly where it was interrupted.

---

## The hardware chain

```
User presses 'A'
      │
      ▼
PS/2 Keyboard controller (port 0x60) — stores scancode 0x1E
      │
      ▼
8259A PIC (Programmable Interrupt Controller) — translates to IRQ1
      │  asserts interrupt line on CPU
      ▼
x86-64 CPU — looks up vector 33 (0x21) in IDT
      │
      ▼
ISR (isr33 in interrupts.rs) — reads port 0x60, calls keyboard driver
      │
      ▼
keyboard.rs — decodes scancode → KeyEvent → pushes to event queue
      │
      ▼
terminal.rs / notepad.rs — pop_key_event() → handles the key
```

---

## Step 1: The PIC — `kernel/src/kernel/drivers/pic.rs`

The 8259A PIC is a chip that multiplexes up to 15 hardware interrupt lines (IRQs)
into a single interrupt pin on the CPU.

**Why remapping?** The CPU reserves interrupt vectors 0–31 for CPU exceptions
(divide-by-zero, page fault, etc.). If the PIC wasn't remapped, IRQ0 would fire
vector 8 (Double Fault) — a collision. So we remap:

```
IRQ0  (timer)    → vector 32  (0x20)
IRQ1  (keyboard) → vector 33  (0x21)
IRQ8  (RTC)      → vector 40  (0x28)
IRQ12 (mouse)    → vector 44  (0x2C)
```

**Read in `pic.rs`:**
- `PIC1_COMMAND / PIC1_DATA` (lines 6–7) — the I/O port addresses
- `init()` (line 21) — the initialization sequence:
  - ICW1: tell the PIC "initialization sequence incoming"
  - ICW2: set the vector offset (0x20 for master, 0x28 for slave)
  - ICW3: cascade wiring (IRQ2 connects the two PICs)
  - ICW4: 8086 mode
- End-of-interrupt (`send_eoi`) — **you must send this at the end of every ISR**,
  or the PIC won't send any more interrupts

**Key thing to understand:** The PIC doesn't know what your handler does. It just
routes the IRQ number to the right CPU vector. You tell it "I'm done" with EOI.

---

## Step 2: The IDT — `kernel/src/kernel/arch/idt.rs`

The IDT (Interrupt Descriptor Table) is a 256-entry array that the CPU loads via
the `LIDT` instruction. Each entry holds:
- The address of the handler function
- Which code segment selector to use (always kernel selector = 0x08 in 64-bit)
- Flags (0x8E = present, ring 0, interrupt gate)

**Read in `idt.rs`:**
- `IdtEntry` struct (line ~17) — 16 bytes per entry, packed
- `set_handler()` (line 31) — how an address gets installed into an entry
- The `extern "C"` block (lines 69–104) — declarations of the assembly stubs
  (`isr0`..`isr255`); these are defined in `interrupts_asm.rs`
- `init()` (line 122) — installs every handler:
  - Lines 69–84: CPU exception handlers (0–31)
  - Line 168: `IDT[33].set_handler(isr33, ...)` — keyboard at vector 33
  - Line 179: `IDT[44].set_handler(isr44, ...)` — mouse at vector 44

**Key thing to understand:** The IDT is just a lookup table. Index = vector number,
value = function pointer. The CPU does the lookup in hardware.

---

## Step 3: The ISR — `kernel/src/kernel/arch/interrupts.rs`

When interrupt 33 fires, the CPU jumps to `isr33` (the assembly stub), which saves
all registers and calls the Rust handler.

**Read in `interrupts.rs`:**
- Find the keyboard handler (around line 240) — it reads port 0x60 to get the
  scancode, then calls `keyboard::handle_interrupt()`
- Find the timer handler — it increments a global tick counter and calls the
  scheduler's `tick()`
- At the very end of each handler: `pic::send_eoi(irq_number)` — mandatory

**Why assembly stubs?** The CPU doesn't save all registers on interrupt — it only
saves RIP, RSP, RFLAGS, CS, SS. The assembly stub (`interrupts_asm.rs`) saves the
rest (RAX, RBX, RCX, RDX, RSI, RDI, R8–R15) before calling Rust, and restores
them on the way out. Rust's calling convention doesn't guarantee which registers
are preserved across a call.

---

## Step 4: The keyboard driver — `kernel/src/kernel/drivers/keyboard.rs`

The keyboard ISR calls into `keyboard.rs`, which does the real work.

**Read in `keyboard.rs`:**
- `process_scancode()` (line 142) — this is called from the ISR:
  - Reads the raw scancode byte from the PS/2 data port (0x60)
  - Feeds it into the `pc-keyboard` crate's state machine (`kb.add_byte()`)
  - When a complete key event is decoded, checks for modifiers (Shift, Ctrl, Alt)
  - Dispatches to the registered callback: `KEY_CALLBACK` for chars,
    `ARROW_CALLBACK` for arrow keys
- `init()` (near the bottom) — sets up the `pc-keyboard` decoder for
  scancode set 1, US QWERTY layout

**Why a callback instead of writing directly to a queue?**
The keyboard driver is low-level — it doesn't know whether the terminal or the
notepad should receive the key. The GUI layer registers its own callback.

---

## Step 5: The event queue — `kernel/src/gui/terminal.rs`

The terminal registers itself as the keyboard callback. When a key fires:

- `terminal_key_callback()` (line 125) — called for printable chars → calls
  `queue_event(ch as u16)`
- `terminal_arrow_callback()` (line 127) — called for arrow keys → encodes
  as 0x100+ events, calls `queue_event()`
- `queue_event()` (line 72) — pushes to a static ring buffer
  (`EVENT_QUEUE: [u16; 256]`)
- `pop_key_event()` (line 148) — called each frame by the focused app
  (terminal or notepad) to drain the queue

---

## The full path, with function names

```
[hardware: PS/2 port 0x60]
       ↓
interrupts.rs: keyboard ISR reads port 0x60
       ↓
keyboard.rs: process_scancode() → pc-keyboard crate → KEY_CALLBACK / ARROW_CALLBACK
       ↓
terminal.rs: terminal_key_callback() / terminal_arrow_callback()
       ↓
terminal.rs: queue_event()  →  EVENT_QUEUE ring buffer
       ↓
notepad.rs / terminal.rs: pop_key_event()  ← called each draw frame
       ↓
[UI reacts: character inserted, cursor moves, etc.]
```

---

## Questions to answer

1. What would happen if you forgot to send EOI at the end of the keyboard ISR?
2. Why is IRQ1 mapped to vector 33 and not vector 1?
3. What does "interrupt gate" mean in the IDT flags? How does it differ from a "trap gate"?
4. Why does `process_scancode()` use a state machine (the `pc-keyboard` crate)
   instead of a simple lookup table?
5. What is the difference between a hardware interrupt and a CPU exception?
   Give an example of each from the IDT.

---

## Exercise: Add an interrupt counter

Add a global `static mut KEYBOARD_INTERRUPT_COUNT: u64 = 0` to `interrupts.rs`.
Increment it in the keyboard ISR. Then expose it in `procfs.rs` as a virtual file
`/proc/kbd_count` that prints the value. Test by reading the file in the terminal.

This touches: ISR → global state → procfs → VFS → terminal command.

---

## Your notes
<!-- Add your own notes here as you study -->
