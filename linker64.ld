ENTRY(_start)

SECTIONS
{
  . = 1M;

  .multiboot2_header : {
    KEEP(*(.multiboot2_header))
  }

  .text : ALIGN(4K) {
    *(.text .text.*)
  }

  .rodata : ALIGN(4K) {
    *(.rodata .rodata.*)
  }

  .data : ALIGN(4K) {
    *(.data .data.*)
  }

  .bss : ALIGN(4K) {
    *(.bss .bss.*)
    *(COMMON)
  }
}
