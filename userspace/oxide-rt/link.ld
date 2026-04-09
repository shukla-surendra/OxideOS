/* Linker script for OxideOS user-space programs.
 *
 * Code is loaded at virtual address 0x400000 (USER_CODE_ADDR).
 * Stack is provided by the kernel at 0x7C0000–0x800000.
 * No libc, no dynamic linker.
 */

OUTPUT_FORMAT(elf64-x86-64)
OUTPUT_ARCH(i386:x86-64)
ENTRY(_start)

SECTIONS {
    . = 0x400000;

    .text : {
        *(.text._start)
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    . = ALIGN(8);
    .data : {
        *(.data*)
    }

    . = ALIGN(8);
    .bss : {
        *(.bss*)
        *(COMMON)
    }

    /* Discard everything the kernel doesn't need */
    /DISCARD/ : {
        *(.eh_frame*)
        *(.note*)
        *(.comment*)
        *(.debug*)
    }
}
