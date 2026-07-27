/* linker.ld — layout for the Ferrumix kernel.
 *
 * The kernel is loaded by a Multiboot2-compliant loader (QEMU's `-kernel`)
 * at its link base address. Because our boot trampoline identity-maps the
 * first 1 GiB of physical memory, virtual addresses equal physical addresses
 * at this stage, so the link address is also where the code runs.
 *
 * The Multiboot2 header MUST be within the first 8 KiB of the loaded image
 * and 64-bit aligned, so we place it first in .text.
 */

ENTRY(_start)

SECTIONS
{
    /* Load base: 1 MiB. Well inside the identity-mapped region. */
    . = 1M;

    .text : {
        KEEP(*(.multiboot2))      /* Multiboot2 header must come first */
        *(.text .text.*)
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
        *(COMMON)
    }

    .bss : ALIGN(4K) {
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note .note.*)
    }
}
