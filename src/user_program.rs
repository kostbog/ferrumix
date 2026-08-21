//! User programs shipped inside the kernel image.
//!
//! Ferrumix has no disk driver yet, so the demo programs are assembled
//! directly into the kernel as complete, valid ELF64 executables: the ELF
//! header, the program header and the machine code are emitted with assembler
//! directives, which means the loader in [`crate::elf`] parses exactly the
//! same bytes it would read from a file.
//!
//! Both programs are position independent (all data is addressed RIP-relative)
//! and talk to the kernel only through `int 0x80`.
//!
//! * `hello` — writes to stdout, opens `/dev/ttyS0`, writes to it, closes it,
//!   asks for its pid, makes an unsupported call (to show `-ENOSYS`) and exits
//!   with status 7.
//! * `crash` — writes a line and then dereferences a null pointer, so the
//!   kernel has to kill it with a page fault.

use crate::usermode::USER_TEXT_BASE;

core::arch::global_asm!(
    r#"
.section .rodata
.balign 16

/* ---------------------------------------------------------------- hello */
.global __hello_elf_start
__hello_elf_start:
hello_ehdr:
    .byte 0x7f, 0x45, 0x4c, 0x46     /* \x7fELF                              */
    .byte 2                          /* EI_CLASS   = ELFCLASS64              */
    .byte 1                          /* EI_DATA    = ELFDATA2LSB             */
    .byte 1                          /* EI_VERSION = EV_CURRENT              */
    .byte 0                          /* EI_OSABI   = System V                */
    .quad 0                          /* padding                              */
    .short 2                         /* e_type     = ET_EXEC                 */
    .short 0x3e                      /* e_machine  = x86-64                  */
    .long 1                          /* e_version                            */
    .quad {USER_BASE} + (hello_code - hello_ehdr)   /* e_entry                 */
    .quad hello_phdr - hello_ehdr    /* e_phoff                              */
    .quad 0                          /* e_shoff                              */
    .long 0                          /* e_flags                              */
    .short 64                        /* e_ehsize                             */
    .short 56                        /* e_phentsize                          */
    .short 1                         /* e_phnum                              */
    .short 64                        /* e_shentsize                          */
    .short 0                         /* e_shnum                              */
    .short 0                         /* e_shstrndx                           */
hello_phdr:
    .long 1                          /* p_type     = PT_LOAD                 */
    .long 5                          /* p_flags    = PF_R | PF_X             */
    .quad 0                          /* p_offset                             */
    .quad {USER_BASE}                /* p_vaddr                              */
    .quad {USER_BASE}                /* p_paddr                              */
    .quad __hello_elf_end - hello_ehdr  /* p_filesz                          */
    .quad __hello_elf_end - hello_ehdr  /* p_memsz                           */
    .quad 0x1000                     /* p_align                              */

hello_code:
    /* write(1, hello_msg, hello_msg_len) */
    movq $1, %rax
    movq $1, %rdi
    leaq hello_msg(%rip), %rsi
    movq $hello_msg_len, %rdx
    int $0x80

    /* fd = open("/dev/ttyS0", 0, 0) */
    movq $2, %rax
    leaq hello_path(%rip), %rdi
    xorq %rsi, %rsi
    xorq %rdx, %rdx
    int $0x80
    movq %rax, %rbx

    /* write(fd, hello_serial, hello_serial_len) */
    movq $1, %rax
    movq %rbx, %rdi
    leaq hello_serial(%rip), %rsi
    movq $hello_serial_len, %rdx
    int $0x80

    /* close(fd) */
    movq $3, %rax
    movq %rbx, %rdi
    int $0x80

    /* getpid() — kept in r12 */
    movq $39, %rax
    int $0x80
    movq %rax, %r12

    /* an unimplemented call must come back as -ENOSYS */
    movq $999, %rax
    int $0x80
    movq %rax, %r13

    /* exit(7) */
    movq $60, %rax
    movq $7, %rdi
    int $0x80
9:  jmp 9b

hello_msg:
    .ascii "hello from ring 3: userspace ELF calling the kernel via int 0x80\n"
    .set hello_msg_len, . - hello_msg
hello_serial:
    .ascii "ring 3 -> /dev/ttyS0: this line was written to the serial port\n"
    .set hello_serial_len, . - hello_serial
hello_path:
    .asciz "/dev/ttyS0"
.global __hello_elf_end
__hello_elf_end:

/* ---------------------------------------------------------------- crash */
.balign 16
.global __crash_elf_start
__crash_elf_start:
crash_ehdr:
    .byte 0x7f, 0x45, 0x4c, 0x46
    .byte 2
    .byte 1
    .byte 1
    .byte 0
    .quad 0
    .short 2
    .short 0x3e
    .long 1
    .quad {USER_BASE} + (crash_code - crash_ehdr)
    .quad crash_phdr - crash_ehdr
    .quad 0
    .long 0
    .short 64
    .short 56
    .short 1
    .short 64
    .short 0
    .short 0
crash_phdr:
    .long 1
    .long 5
    .quad 0
    .quad {USER_BASE}
    .quad {USER_BASE}
    .quad __crash_elf_end - crash_ehdr
    .quad __crash_elf_end - crash_ehdr
    .quad 0x1000

crash_code:
    /* write(1, crash_msg, crash_msg_len) */
    movq $1, %rax
    movq $1, %rdi
    leaq crash_msg(%rip), %rsi
    movq $crash_msg_len, %rdx
    int $0x80

    /* dereference a null pointer: the kernel must kill us */
    xorq %rax, %rax
    movq $42, (%rax)
9:  jmp 9b

crash_msg:
    .ascii "ring 3: about to touch a null pointer on purpose\n"
    .set crash_msg_len, . - crash_msg
.global __crash_elf_end
__crash_elf_end:
"#,
    USER_BASE = const USER_TEXT_BASE,
    options(att_syntax)
);

extern "C" {
    static __hello_elf_start: u8;
    static __hello_elf_end: u8;
    static __crash_elf_start: u8;
    static __crash_elf_end: u8;
}

/// The `hello` demo program as a ready to load ELF image.
pub fn hello_elf() -> &'static [u8] {
    unsafe {
        let start = core::ptr::addr_of!(__hello_elf_start);
        let end = core::ptr::addr_of!(__hello_elf_end);
        core::slice::from_raw_parts(start, end as usize - start as usize)
    }
}

/// The `crash` demo program (dereferences a null pointer).
pub fn crash_elf() -> &'static [u8] {
    unsafe {
        let start = core::ptr::addr_of!(__crash_elf_start);
        let end = core::ptr::addr_of!(__crash_elf_end);
        core::slice::from_raw_parts(start, end as usize - start as usize)
    }
}

/// Every program the kernel can execute, by name.
pub fn find(name: &str) -> Option<(&'static str, &'static [u8])> {
    match name {
        "hello" | "/bin/hello" => Some(("hello", hello_elf())),
        "crash" | "/bin/crash" => Some(("crash", crash_elf())),
        _ => None,
    }
}

/// Names of the built-in programs (for `ls /bin` and `help`).
pub const NAMES: [&str; 2] = ["hello", "crash"];
