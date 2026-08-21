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

    __kernel_start = .;
    __kernel_phys_start = .;

    .text : {
        KEEP(*(.multiboot))       /* Multiboot2 header must come first */
        *(.text .text.*)
    }

    .note.Xen : {
        KEEP(*(.note.Xen))        /* PVH entry note — QEMU -kernel uses it */
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }

    .data : ALIGN(4K) {
        *(.data .data.*)
        *(COMMON)
    }

    .bss : ALIGN(4K) {
        __bss_start = .;
        *(COMMON)
        *(.bss .bss.*)
        . = ALIGN(8);
        __bss_end = .;
    }

    __kernel_end = .;
    __kernel_phys_end = .;

    /* DWARF debug information.  These sections are not loaded, but they need
     * explicit (zero) addresses: otherwise lld may hand them file ranges that
     * overlap the loaded sections and refuse to link. */
    .debug_abbrev      0 : { *(.debug_abbrev) }
    .debug_addr        0 : { *(.debug_addr) }
    .debug_aranges     0 : { *(.debug_aranges) }
    .debug_frame       0 : { *(.debug_frame) }
    .debug_info        0 : { *(.debug_info .gnu.linkonce.wi.*) }
    .debug_line        0 : { *(.debug_line .debug_line.*) }
    .debug_line_str    0 : { *(.debug_line_str) }
    .debug_loc         0 : { *(.debug_loc) }
    .debug_loclists    0 : { *(.debug_loclists) }
    .debug_macinfo     0 : { *(.debug_macinfo) }
    .debug_macro       0 : { *(.debug_macro) }
    .debug_pubnames    0 : { *(.debug_pubnames) }
    .debug_pubtypes    0 : { *(.debug_pubtypes) }
    .debug_ranges      0 : { *(.debug_ranges) }
    .debug_rnglists    0 : { *(.debug_rnglists) }
    .debug_str         0 : { *(.debug_str) }
    .debug_str_offsets 0 : { *(.debug_str_offsets) }
    .debug_types       0 : { *(.debug_types) }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
