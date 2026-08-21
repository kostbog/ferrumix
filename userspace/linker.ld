/* Linker layout for the illustrative freestanding userspace binary.
 *
 * The kernel does not load this image yet; this script only lets CI verify that
 * the no_std userspace scaffold links as a standalone x86_64-unknown-none ELF.
 */
ENTRY(_start)

SECTIONS
{
    . = 4M;

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
        *(COMMON)
        *(.bss .bss.*)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
        *(.note .note.*)
    }
}
