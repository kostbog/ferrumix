/* boot.S — boot headers + long-mode trampoline for Ferrumix.
 *
 * The image carries two boot descriptions so it can be started either way:
 *
 *   * a Multiboot2 header (0xE85250D6) for GRUB, which enters with
 *     EAX = 0x36D76289 and EBX = pointer to the Multiboot2 information;
 *   * a PVH ELF note (XEN_ELFNOTE_PHYS32_ENTRY), which is what QEMU's
 *     `-kernel` uses for 64-bit ELF images: it enters with EBX = pointer to
 *     an `hvm_start_info` structure (QEMU refuses Multiboot for ELF64).
 *
 * Both protocols enter in 32-bit protected mode with paging disabled, so the
 * same trampoline serves them; `kernel_main` works out which one it was from
 * EAX and from the magic of the structure EBX points at.
 *
 * Our job here:
 *   1. Build 1:1 page tables (identity map the first 1 GiB with 2 MiB pages).
 *   2. Enable PAE, load CR3, set EFER.LME, enable paging  -> long mode.
 *   3. Load a 64-bit GDT and far-jump into 64-bit code.
 *   4. Set up the stack and call kernel_main(magic, boot_info).
 */

.set MB2_MAGIC,    0xe85250d6
.set MB2_ARCH,     0            /* 0 = x86 (32-bit)                        */
.set MB2_LEN,      (mb2_end - mb2_start)
.set MB2_CHECKSUM, (0 - (MB2_MAGIC + MB2_ARCH + MB2_LEN)) & 0xFFFFFFFF

.section .multiboot, "a"
.align 8
mb2_start:
    .long MB2_MAGIC
    .long MB2_ARCH
    .long MB2_LEN
    .long MB2_CHECKSUM
    /* Information request tag (type 1): ask for cmdline(4), mmap(6), fb(8). */
    .align 8
    .long 1
    .long 0
    .long 24
    .long 4
    .long 6
    .long 8
    .align 8
    .long 0                      /* end tag (type 0) */
    .long 8
mb2_end:

/* PVH boot protocol: XEN_ELFNOTE_PHYS32_ENTRY (type 18) tells the loader the
 * 32-bit entry point of the image. */
.section .note.Xen, "a", @note
.align 4
    .long 4                      /* namesz: "Xen\0"                        */
    .long 4                      /* descsz: a 32-bit entry point            */
    .long 18                     /* XEN_ELFNOTE_PHYS32_ENTRY                */
    .asciz "Xen"
    .align 4
    .long _start

.section .text
.align 8
.global _start
.code32
_start:
    /* Preserve boot args (EAX=magic, EBX=mb_info) in callee-saved regs. */
    movl %eax, %ebp
    /* Set up our own stack (identity-mapped region). */
    movl $boot_stack_top, %esp

    /* ---- Zero the three page tables (3 * 4096 bytes = 12288 bytes) ---- */
    movl $pml4_table, %edi
    xorl %eax, %eax
    movl $3072, %ecx
1:
    movl %eax, (%edi)
    addl $4, %edi
    loop 1b

    /* ---- PML4[0] -> PDPT ---- */
    movl $pml4_table, %edi
    movl $pdpt, %eax
    orl  $0x3, %eax              /* present | writable */
    movl %eax, (%edi)

    /* ---- PDPT[0] -> PD ---- */
    movl $pdpt, %edi
    movl $pd_table, %eax
    orl  $0x3, %eax
    movl %eax, (%edi)

    /* ---- PD: 512 * 2 MiB = 1 GiB identity-mapped ---- */
    movl $pd_table, %edi
    movl $0x00000083, %eax       /* present | writable | huge (2 MiB) */
    movl $512, %ecx
2:
    movl %eax, (%edi)
    addl $0x200000, %eax
    addl $8, %edi
    loop 2b

    /* Enable PAE (CR4.PAE = bit 5). */
    movl %cr4, %eax
    orl  $0x20, %eax
    movl %eax, %cr4

    /* Load PML4. */
    movl $pml4_table, %eax
    movl %eax, %cr3

    /* Enable long mode: EFER.LME = bit 8. */
    movl $0xC0000080, %ecx       /* EFER MSR */
    rdmsr
    orl  $0x100, %eax
    wrmsr

    /* Enable paging (CR0.PG = bit 31). */
    movl %cr0, %eax
    orl  $0x80000000, %eax
    movl %eax, %cr0

    /* Load 64-bit GDT and far-jump into 64-bit code segment (selector 0x08). */
    lgdt gdt64_pointer
    ljmp $0x08, $long_mode_entry

.code64
long_mode_entry:
    /* Reload data segment registers for long mode (data selector 0x10). */
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw $0x00, %ax
    movw %ax, %fs
    movw %ax, %gs

    /* Set the 64-bit stack. */
    movq $boot_stack_top, %rsp

    /* Pass arguments to kernel_main(magic, mb_info) via C ABI regs. */
    movl %ebp, %edi              /* 1st arg: multiboot2 magic */
    movl %ebx, %esi              /* 2nd arg: multiboot2 info pointer */

    cld                          /* Rust assumes the direction flag is clear */
    call kernel_main

    /* kernel_main is declared `-> !`; if it ever returns, park the CPU. */
3:
    hlt
    jmp 3b

.section .rodata
.align 8
gdt64:
    .quad 0x0000000000000000     /* 0x00: null */
    .quad 0x00209A0000000000     /* 0x08: 64-bit code (present, ring0, exec) */
    .quad 0x0000920000000000     /* 0x10: 64-bit data (present, ring0, rw) */
gdt64_pointer:
    .word gdt64_end - gdt64 - 1
    .long gdt64
gdt64_end:

.section .bss
.align 4096
pml4_table:
    .skip 4096
pdpt:
    .skip 4096
pd_table:
    .skip 4096
.align 16
boot_stack_bottom:
    .skip 16384
boot_stack_top:
