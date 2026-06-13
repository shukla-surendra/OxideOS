# Intel 8042 — PS/2 Controller & Keyboard

**Source:** `kernel/src/kernel/drivers/keyboard.rs`

---

## What it is

The **Intel 8042** (also called the "keyboard controller" or "KBC") is a microcontroller
that sits between the CPU and PS/2 devices. It manages:
- **Port 1:** PS/2 keyboard
- **Port 2:** PS/2 mouse

When a key is pressed, the keyboard sends a **scancode** to the 8042. The 8042 stores
it in its output buffer and asserts IRQ1. The kernel reads the scancode from the data
port (0x60), interprets it, and dispatches the key event.

```
[Key pressed]
     │
     ▼
PS/2 Keyboard (serial protocol, ~12 KHz clock)
     │  scancode byte
     ▼
Intel 8042 controller
  - Stores byte in output buffer
  - Asserts IRQ1
     │
     ▼
CPU: IRQ1 → vector 33 → keyboard ISR
  - Reads port 0x60 (scancode)
  - Calls keyboard driver
```

---

## I/O Ports

| Port | Name | Direction | Purpose |
|------|------|-----------|---------|
| `0x60` | Data port | Read/Write | Read: scancode from keyboard or mouse. Write: send command/data to keyboard |
| `0x64` | Status (read) | Read | Controller status register |
| `0x64` | Command (write) | Write | Send command to the 8042 controller itself |

**Same address, different direction** — reading `0x64` gives you the status; writing
`0x64` sends a command to the controller (not to the keyboard).

---

## Status Register (port 0x64, read)

```
Bit 0 (OBF — Output Buffer Full):
  1 = data is waiting in port 0x60 for the CPU to read
  0 = no data available

Bit 1 (IBF — Input Buffer Full):
  1 = controller is still processing; don't write yet
  0 = ready to receive a command or data

Bit 5 (AUXB — Auxiliary Output Buffer):
  0 = data in port 0x60 came from keyboard (IRQ1)
  1 = data in port 0x60 came from mouse (IRQ12)
```

In OxideOS `keyboard.rs`:
- The **interrupt handler** checks bit 0 (OBF) before reading — ignores the call
  if no data is ready
- The **polling path** checks bit 5 (AUXB) to avoid consuming mouse data as keyboard
  data: `if (status & 0x20) == 0` — only process if it's keyboard data

```rust
// handle_keyboard_interrupt() — line 94
let status: u8;
asm!("in al, 0x64", out("al") status, ...);  // read status
let scancode: u8;
asm!("in al, 0x60", out("al") scancode, ...); // read data
if (status & 0x01) != 0 {    // OBF must be set
    process_scancode(scancode);
}
```

---

## 8042 Controller Commands (port 0x64, write)

These commands go to the 8042 itself, not the keyboard:

| Command | Hex | What it does |
|---------|-----|--------------|
| Disable keyboard port | `0xAD` | Temporarily disable keyboard to avoid stale data |
| Enable keyboard port | `0xAE` | Re-enable keyboard after configuration |
| Write Controller Command Byte | `0x60` | Next byte written to 0x60 becomes the CCB |
| Reboot CPU | `0xFE` | Pulse the CPU reset line — used for reboot |

OxideOS init sequence (`keyboard.rs init()`, line 408):
```rust
ctrl_cmd(0xAD);       // 1. Disable keyboard port
flush_output_buffer();// 2. Drain stale bytes from buffer
ctrl_cmd(0x60);       // 3. Tell 8042 to expect a CCB byte
ctrl_data(0x47);      // 4. Write CCB: 0x47 = IRQ1 enable + IRQ12 enable + scancode translation
ctrl_cmd(0xAE);       // 5. Re-enable keyboard port
```

---

## Controller Command Byte (CCB) — value `0x47`

After `0x60` (write CCB command), the next byte written to `0x60` configures the
8042's behavior:

```
0x47 = 0b 0100 0111

Bit 0: IRQ1 enable       = 1  → keyboard interrupts enabled
Bit 1: IRQ12 enable      = 1  → mouse interrupts enabled
Bit 2: System flag        = 1  → POST passed (required for normal operation)
Bit 3: (reserved)         = 0
Bit 4: Keyboard clock     = 0  → keyboard clock enabled
Bit 5: Mouse clock        = 0  → mouse clock enabled
Bit 6: Translation        = 1  → convert scancode set 2 → set 1 (legacy compat)
Bit 7: (reserved)         = 0
```

**Translation bit (6):** Modern PS/2 keyboards send **Scancode Set 2**, but the
`pc-keyboard` crate expects **Scancode Set 1** (the original IBM format).
Setting bit 6 makes the 8042 transparently translate Set 2 → Set 1 in hardware.

---

## Scancodes

A scancode is the raw byte the keyboard sends when a key is pressed or released.

**Make code** = key pressed (e.g., pressing 'A' sends `0x1E`)
**Break code** = key released (e.g., releasing 'A' sends `0x9E` = `0x1E | 0x80`)

**Extended keys** (arrows, Insert, Delete, etc.) use a two-byte prefix `0xE0` followed
by the scancode. The `pc-keyboard` crate handles this state machine automatically —
`kb.add_byte(scancode)` returns `Ok(None)` for the `0xE0` prefix byte and `Ok(Some(event))`
when the second byte arrives.

```
'A' pressed → 0x1E
'A' released → 0x9E
Right arrow pressed → 0xE0, 0x4D
Right arrow released → 0xE0, 0xCD
```

---

## The `pc-keyboard` crate

Rather than maintaining a hand-written scancode table, OxideOS delegates decoding
to the `pc-keyboard` crate:

```rust
// keyboard.rs line 18
static mut KB: Option<Keyboard<layouts::Us104Key, ScancodeSet1>> = None;

// keyboard.rs line 149 — in process_scancode()
match kb.add_byte(scancode) {
    Ok(Some(key_event)) => {
        // key_event.code  = KeyCode::A, KeyCode::ArrowUp, etc.
        // key_event.state = KeyState::Down or KeyState::Up
        if let Some(decoded) = kb.process_keyevent(key_event) {
            match decoded {
                DecodedKey::Unicode(c) => dispatch_unicode(c),  // printable chars
                DecodedKey::RawKey(kc) => dispatch_raw_key(kc), // arrows, F-keys, etc.
            }
        }
    }
    Ok(None) => {}  // multi-byte sequence not complete yet
    Err(_)   => {}  // bad scancode, ignore
}
```

---

## Modifier key tracking

The `pc-keyboard` crate in version 0.7 doesn't expose modifier state publicly, so
OxideOS maintains its own shadow copy in `MODS: ModState`:

```rust
struct ModState {
    lshift, rshift: bool,  // left/right shift
    lctrl,  rctrl:  bool,  // left/right ctrl
    lalt,   ralt:   bool,  // left alt / AltGr
    caps, num, scroll: bool, // lock keys
}
```

`update_modifiers()` is called on every key event to keep this in sync.
Public accessors (`is_shift_pressed()`, `is_ctrl_pressed()`, etc.) are used by
`terminal.rs` to generate Shift+Arrow events for the notepad.

---

## LED control

Keyboard LEDs (Caps Lock, Num Lock, Scroll Lock) are controlled by sending a command
to the keyboard via the data port:

1. Write `0xED` to `0x60` (LED command)
2. Wait for ACK (`0xFA`)
3. Write the LED byte:
   - Bit 0: Scroll Lock LED
   - Bit 1: Num Lock LED
   - Bit 2: Caps Lock LED

OxideOS (`keyboard.rs send_led_command()`, line 386):
```rust
ctrl_data(0xED);        // LED command
ctrl_read_fast();       // consume ACK from keyboard
ctrl_data(led);         // LED state byte
ctrl_read_fast();       // consume ACK
```

---

## VirtualBox quirk

VirtualBox incorrectly sets bit 5 (AUXB) of the status register during IRQ1.
This would cause OxideOS's polling path to think keyboard data is actually mouse data
and discard it. The fix in `handle_keyboard_interrupt()`:

> "We are in the keyboard IRQ handler (IRQ1), so any data present is keyboard data
> — even if bit 5 says otherwise."

The interrupt handler only checks OBF (bit 0) and trusts the IRQ source. The polling
path still checks AUXB because it runs outside the IRQ context.

---

## Reboot via 8042

The 8042 has a special ability: it can pulse the CPU's RESET line.
`shutdown.rs reboot()` uses this:

```rust
// Wait for input buffer empty
asm!("in al, 0x64", ...);
if (status & 0x02) == 0 { break; }

// Pulse CPU reset via 8042
outb(0x64, 0xFE);
```

Command `0xFE` tells the 8042 to assert the CPU reset pin for ~6µs — instantly
cold-reboots the machine.

---

## Self-check questions

1. Why is there a separate status port (0x64) instead of just reading 0x60?
2. What happens if you read 0x60 before OBF is set?
3. Why does the 8042 have a "translation" mode (bit 6 of CCB)? What problem does it solve?
4. What is `ctrl_read_fast()` waiting for? What does it discard and why?
5. Why does the interrupt handler ignore bit 5 (AUXB) of the status register,
   but the polling path respects it?
