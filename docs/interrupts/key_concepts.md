## GDT (Global Descriptor Table)
The GDT is a data structure used by x86 processors to define memory segments and their properties. It contains segment descriptors that specify:

* Base address and limit (size) of memory segments
* Access rights (read/write/execute permissions)
* Privilege levels (rings 0-3)
* Segment type (code, data, system segments)

Each entry is 8 bytes and describes a segment's characteristics. The processor uses the GDT to translate segment selectors (found in segment registers like CS, DS, SS) into actual memory addresses and enforce protection.
## IDT (Interrupt Descriptor Table)
The IDT maps interrupt and exception vectors to their handler routines. It contains up to 256 entries (0-255), each describing:

* Handler address (where to jump when interrupt occurs)
* Segment selector (which code segment contains the handler)
* Privilege level required to invoke the interrupt
* Gate type (interrupt gate, trap gate, task gate)

When an interrupt occurs (hardware interrupt, software interrupt via int, or CPU exception), the processor looks up the appropriate entry in the IDT and transfers control to the specified handler.
## GPF (General Protection Fault)
A GPF is a CPU exception (interrupt vector 13) that occurs when the processor detects a protection violation, such as:

* Segment limit violations (accessing beyond segment boundaries)
* Privilege violations (ring 3 code trying to execute ring 0 instructions)
* Invalid segment selectors (referencing non-existent GDT entries)
* Write attempts to read-only segments
* Execution of privileged instructions from user mode

When a GPF occurs, the CPU generates interrupt 13, and if no handler is installed or the handler itself causes a fault, it typically results in a system crash or process termination.
These concepts are tightly interconnected - the GDT defines memory protection model, the IDT handles when things go wrong (including GPFs), and GPFs are the CPU's way of enforcing the protection rules you've set up in GDT.