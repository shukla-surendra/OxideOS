## 📌 What is a PIC? (Programmable Interrupt Controller)

A PIC (like the Intel 8259A) is a hardware chip that manages hardware interrupts coming from devices (keyboard, mouse, timer, etc.) to the CPU.

It acts as a middleman between hardware and the processor’s interrupt lines.

## 📌 Why do we need a PIC?

CPUs (especially early x86) have a limited number of pins (INTR lines) for handling interrupts. A PIC helps by multiplexing many hardware interrupt sources into one line to the CPU.

It prioritizes interrupts (so, for example, the keyboard doesn’t block the timer).

It lets the CPU know which device interrupted by sending an interrupt vector number.

## 📌 How does the PIC work in OS dev?

Devices raise IRQs (Interrupt Requests). Example: the keyboard raises IRQ1.

The PIC receives the IRQ and decides whether to forward it (depending on priority and masks).

The PIC tells the CPU: “Hey, interrupt number X occurred.”

The CPU pauses what it was doing, looks up the Interrupt Descriptor Table (IDT) entry for that interrupt vector, and jumps to interrupt handler code.

Once OS handles the interrupt, you send an EOI (End Of Interrupt) signal back to the PIC, so it can allow more interrupts.

## 📌 OS dev specifics:

On x86, there are usually two PICs (master + slave) chained together, giving 15 usable IRQ lines.

Master PIC handles IRQ0–IRQ7.

Slave PIC handles IRQ8–IRQ15 (connected via master’s IRQ2).

The PIC’s default mapping conflicts with CPU exceptions (0–31), so in OS dev we usually remap the PIC to use different interrupt vectors (e.g., IRQs start at 32).

## 📌 Example flow (keyboard press):

You press a key → Keyboard sends IRQ1 → PIC → CPU.

PIC signals CPU with vector (e.g., 0x21 after remap).

CPU jumps to handler in the IDT.

handler reads the scan code from port 0x60.

Handler sends EOI to PIC (outb(0x20, 0x20)).

CPU resumes normal execution.

✅ So in short:
The PIC is the interrupt traffic controller in  OS. Without it,  CPU wouldn’t know which device needs attention, or even if an interrupt occurred at all.




## 📌 What is EOI?

EOI stands for End Of Interrupt.

It’s a command  OS sends to the Programmable Interrupt Controller (PIC) after handling an interrupt.

### 📌 Why do we need EOI?

When a device (like the keyboard or timer) triggers an interrupt, the PIC raises an IRQ and forwards it to the CPU.

While that IRQ is being serviced, the PIC won’t send further interrupts of equal or lower priority.

If you don’t send an EOI, the PIC assumes you’re still handling the interrupt → and it won’t let new interrupts of that level through.

This could make system “freeze” after the first interrupt.

## 📌 How do you send an EOI?

On the Intel 8259 PIC (the classic x86 one):

You send the command 0x20 (known as EOI command) to I/O port 0x20 (the master PIC command port).

If the interrupt came from the slave PIC, you must send an EOI to both:

Slave PIC (port 0xA0)

Master PIC (port 0x20)

### 📌 Example (Keyboard Interrupt – IRQ1):

Keyboard triggers IRQ1 → goes to Master PIC.

CPU jumps to interrupt handler.

Handler reads scan code from port 0x60.

Handler sends:
```
mov al, 0x20   ; EOI command
out 0x20, al   ; Send EOI to master PIC
```

Now the PIC is free to deliver more interrupts.

## 📌 Summary

* EOI = “I’m done, you can send me more interrupts.”

* Without it → the PIC blocks further interrupts at that line.

* With it → OS can continue responding to new hardware events.