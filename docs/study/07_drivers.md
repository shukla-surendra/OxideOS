# 07 — Drivers: Writing New Hardware Code

A driver is code that translates between hardware's language (registers, port I/O,
interrupts) and the kernel's language (functions, buffers, events). This doc teaches
you how to read a hardware spec and turn it into driver code — using your existing
drivers as models.

---

## How hardware is accessed on x86

There are two mechanisms:

### Port-mapped I/O (PMIO)
x86 has a separate 64K I/O address space accessed with special instructions:
```asm
out dx, al    ; write byte in AL to port DX
in al, dx     ; read byte from port DX into AL
```
In Rust (inline asm):
```rust
unsafe { core::arch::asm!("out dx, al", in("dx") port, in("al") value) }
unsafe { core::arch::asm!("in al, dx", in("dx") port, out("al") value) }
```

Used by: PIC (0x20/0x21), PIT timer (0x40–0x43), PS/2 keyboard (0x60/0x64),
RTC/CMOS (0x70/0x71), PC speaker (0x61), serial UART (0x3F8–0x3FF).

### Memory-mapped I/O (MMIO)
Some devices expose their registers as specific physical memory addresses.
Just read/write those addresses with normal loads/stores — but the CPU must not
cache them (`volatile` reads/writes).

Used by: PCI BARs, framebuffer (the screen), network cards.

---

## Model A: Port-mapped driver — `kernel/src/kernel/drivers/rtc.rs`

The RTC (Real-Time Clock) is a simple device in CMOS. You access it via two ports:
- `0x70` — index register (select which CMOS register to read)
- `0x71` — data register (read/write selected register)

**Read `rtc.rs`:**
- Find the port constants (0x70 / 0x71)
- Find how it reads hours: write the register index to 0x70, then read 0x71
- Find the BCD conversion — hardware may store time as BCD (0x12 = 12, not 18)

**Key pattern:** index + data port pair. This is used by many chips (PIC uses it too).

---

## Model B: Interrupt-driven driver — `kernel/src/kernel/drivers/keyboard.rs`

The PS/2 keyboard doesn't need polling — it fires an interrupt when a key is ready.

**The pattern:**
1. Hardware puts data in register
2. Fires interrupt
3. ISR reads the data (must happen quickly, before the device drops it)
4. Decodes and queues the event
5. Returns from interrupt

**Key constraint:** ISRs must be fast. You cannot allocate memory, take locks,
or do I/O in an ISR. That's why keyboard.rs queues the raw scancode and decodes
it later (or uses a minimal state machine).

---

## Model C: PCI device — `kernel/src/kernel/drivers/net/pci.rs`

PCI devices are discovered by scanning a configuration space:
- Each device has a `(bus, device, function)` address
- Reading config space address `0x00` gives `(vendor_id, device_id)`
- If `vendor_id == 0xFFFF`, no device present
- Otherwise, you can read class code, BAR addresses, interrupt line, etc.

**Read `pci.rs`:**
- Find `pci_read_u32(bus, device, function, offset)`
- Find the scan loop (enumerate bus 0, all 32 device slots, function 0)
- Find how RTL8139 is identified (vendor 0x10EC, device 0x8139)

---

## How to write a new driver: PC Speaker

The PC speaker is controlled by two things:
- **PIT channel 2** (port 0x42/0x43) — generates a square wave at a given frequency
- **Port 0x61** (system control port B) — bit 0 enables PIT channel 2 output,
  bit 1 enables speaker gate

To play a 440Hz tone (musical 'A'):

```
PIT input clock = 1,193,182 Hz
divisor = 1,193,182 / 440 = 2712

1. Write 0xB6 to port 0x43 (set channel 2, square wave, binary mode)
2. Write low byte of divisor to port 0x42
3. Write high byte of divisor to port 0x42
4. Read port 0x61, set bits 0 and 1, write back
```

To stop:
```
Read port 0x61, clear bits 0 and 1, write back
```

**Your exercise:** Implement this as `kernel/src/kernel/drivers/speaker.rs` with two functions:
- `pub unsafe fn beep(frequency_hz: u32)` — start a tone
- `pub unsafe fn stop()` — silence the speaker

Then add a `beep` command to the terminal: `beep 440` plays a tone.

This forces you to:
- Read a hardware specification and translate it to port I/O
- Deal with the "frequency to divisor" calculation in integer arithmetic
- Handle the "no floating point in kernel" constraint (use integer division)

---

## Reading a hardware datasheet

When you write real driver code, you need a spec. For x86 built-in devices,
the relevant documents are:

| Device | Document to find |
|--------|-----------------|
| 8253/8254 PIT | "Intel 8254 Programmable Interval Timer Datasheet" |
| 8259A PIC | "Intel 8259A Programmable Interrupt Controller" |
| PS/2 keyboard | OSDev wiki: "PS/2 Keyboard" |
| ATA/IDE | "ATA-4 specification" or OSDev "ATA PIO Mode" |
| RTL8139 | "RTL8139 Programming Guide" (Realtek, available online) |

OSDev wiki (wiki.osdev.org) is the best starting point for any x86 hardware.

---

## Questions

1. Why must an ISR not block (wait for a lock, sleep, or allocate)? What would happen?
2. What is the difference between polling and interrupt-driven I/O? When would you
   choose each?
3. Why does the PIT require an integer divisor? How do you compute frequency from it?
4. What is a "BAR" in PCI? What does it tell you about a device?
5. If a driver needs to transfer a large buffer (e.g., a network packet), and
   the device uses MMIO, why must the writes be `volatile`?

---

## Exercise: PC Speaker driver

Implement `kernel/src/kernel/drivers/speaker.rs` with `beep(freq)` and `stop()`.

Don't look up solutions — read the description in this doc and try. If you get
stuck, use `ata.rs` or `rtc.rs` as models for how port I/O is done in this codebase.

After implementing, add these two terminal commands:
- `beep <hz>` — plays a tone at the given frequency
- `beepstop` — silences it

---

## Your notes
<!-- Add your own notes here as you study -->
