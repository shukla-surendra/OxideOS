# 03 — Keypress Trace: End-to-End

This is a reading exercise. Follow one 'A' keypress from hardware to screen,
opening each file as you go. Don't just skim — understand what each step does
before moving to the next.

---

## Setup: open these files side by side

```
kernel/src/kernel/arch/interrupts.rs
kernel/src/kernel/drivers/keyboard.rs
kernel/src/gui/terminal.rs
kernel/src/gui/notepad.rs
```

---

## The trace

### 1. Hardware fires IRQ1

The PS/2 keyboard controller stores the scancode `0x1E` (the 'A' key, make code)
in its data register at I/O port `0x60`. It then asserts the IRQ1 line.

The PIC sees IRQ1, translates it to interrupt vector 33, and asserts the CPU's
INT pin.

### 2. CPU jumps to IDT[33]

The CPU finishes its current instruction. It pushes RIP, CS, RFLAGS, RSP, SS onto
the stack, then reads `IDT[33]` to get the handler address.

That address points to the `isr33` assembly stub in `interrupts_asm.rs`, which
saves all the general-purpose registers, then calls the Rust handler in
`interrupts.rs`.

**Find in `interrupts.rs`:** the function that handles IRQ1 / keyboard.
Look for a read from port `0x60`.

### 3. Keyboard ISR reads the scancode

```rust
// inside interrupts.rs keyboard handler
let scancode: u8;
asm!("in al, 0x60", out("al") scancode, ...);
keyboard::handle_interrupt();  // or similar call
pic::send_eoi(1);              // must acknowledge to PIC
```

The raw byte `0x1E` is now in Rust-land.

### 4. `keyboard.rs` decodes the scancode

**Open `kernel/src/kernel/drivers/keyboard.rs`, find `process_scancode()`.**

- The raw byte goes into the `pc-keyboard` crate's `Keyboard` state machine via
  `kb.add_byte(scancode)`
- The crate handles multi-byte sequences (extended keys use `0xE0` prefix) and
  produces a `KeyEvent { code: KeyCode::A, state: KeyState::Down }`
- The driver checks modifier keys (Shift → uppercase), then:
  - For printable chars: calls `KEY_CALLBACK(ch)` where `ch = b'a'` or `b'A'`
  - For arrow keys: calls `ARROW_CALLBACK(ArrowKey::Up/Down/...)` 

**Key observation:** the driver itself has no idea what `KEY_CALLBACK` does.
That function pointer is set by whoever registered a callback — in your kernel,
the terminal does this at startup.

### 5. Event enters the queue — `terminal.rs`

**Open `kernel/src/gui/terminal.rs`, find `terminal_key_callback()` and `queue_event()`.**

```rust
unsafe fn terminal_key_callback(ch: u8) {
    queue_event(ch as u16);
}

unsafe fn queue_event(event: u16) {
    // Push to EVENT_QUEUE ring buffer
    EVENT_QUEUE[EVENT_HEAD] = event;
    EVENT_HEAD = (EVENT_HEAD + 1) % EVENT_QUEUE_SIZE;
}
```

The event `b'a' as u16 = 0x61` is now sitting in a static ring buffer.

**Why a ring buffer?** The interrupt fires asynchronously — the main loop might
be in the middle of drawing. The ring buffer decouples the ISR (producer) from
the UI (consumer) without needing locks.

### 6. The main loop polls the queue

In `main.rs`, every frame the focused app calls `pop_key_event()`:

```rust
// in notepad.rs: process_input()
while let Some(ev) = pop_key_event() {
    match ev {
        32..=126 => self.insert_char(ev as u8),
        ...
    }
}
```

For the notepad, `ev = 0x61` ('a') falls into the `32..=126` printable range →
`insert_char(b'a')` → character is added to the buffer → next draw frame shows it.

---

## What you should be able to draw from memory

```
PS/2 port 0x60
  → IRQ1 → PIC → CPU vector 33 → IDT[33] → isr33 (asm stub)
  → interrupts.rs keyboard handler → keyboard::process_scancode()
  → pc-keyboard state machine → KEY_CALLBACK
  → terminal_key_callback() → queue_event() → EVENT_QUEUE[]
  → pop_key_event() → notepad::insert_char() / terminal command
```

Draw this on paper without looking. If you can't, re-read the relevant section.

---

## Rust patterns you'll see

**`static mut` with `unsafe`** — the event queue and keyboard callbacks are global
mutable state. This is `unsafe` because Rust can't prove they're not accessed from
multiple threads simultaneously (and in an OS, an interrupt is essentially a
"thread" that can preempt anything).

**`asm!` macro** — reading port 0x60 requires the `in` x86 instruction, which
has no Rust equivalent. `asm!("in al, 0x60", out("al") scancode)` embeds one
instruction of assembly directly.

**Callbacks via function pointers** — `KEY_CALLBACK: Option<unsafe fn(u8)>`. The
`Option` means "no callback registered yet." The keyboard driver is initialized
before the GUI, so there's a window where interrupts fire but no callback exists —
the event is just dropped.

---

## Questions

1. What happens to the keyboard interrupt if `KEY_CALLBACK` is `None`?
   Is the event lost? Trace the code path.
2. Why must `queue_event()` be `unsafe`? What invariant are you responsible for?
3. If two keys are pressed very fast, can any events be lost? What determines the
   maximum burst the queue can absorb before dropping events?
4. What would happen if you forgot to call `pic::send_eoi(1)` in the keyboard ISR?
5. Why does the terminal use a `u16` for events instead of `u8`?
   (Hint: look at the arrow key event codes like `0x100`.)

---

## Exercise: Add a key logger

Add a new global `static mut LAST_KEY: u8 = 0` in `keyboard.rs` that stores the
last ASCII character pressed. Then add a terminal command `lastkey` that prints it.

This is the smallest possible change that touches the entire path you just traced.
Write it yourself — don't ask Claude to generate it.

---

## Your notes
<!-- Add your own observations as you read through the code -->
