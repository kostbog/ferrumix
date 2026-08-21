//! Interrupts, exceptions and the system-call gate.
//!
//! Rust's `x86-interrupt` calling convention is still unstable, so Ferrumix
//! uses hand written assembly stubs instead: one stub per vector pushes a
//! (possibly dummy) error code and the vector number, `isr_common` saves all
//! general purpose registers and calls [`trap_dispatch`] with a pointer to the
//! resulting [`TrapFrame`].
//!
//! ```text
//!   ring 3 --int 0x80--> isr_stub_128 --+
//!   IRQ    --PIC-------> isr_stub_32  --+--> isr_common --> trap_dispatch
//!   fault  ------------> isr_stub_14  --+
//! ```
//!
//! Because the frame contains every register (and the interrupted `cs`), the
//! same structure is used to return values to user space from a system call
//! and to decide whether a fault happened in ring 0 or ring 3.

use crate::idt;
use crate::port;
use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

/// Registers saved by the assembly stubs, in memory order.
#[derive(Debug)]
#[repr(C)]
pub struct TrapFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    /// Interrupt vector, pushed by the stub.
    pub vector: u64,
    /// Error code pushed by the CPU (or zero when there is none).
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl TrapFrame {
    /// True when the trap came from ring 3.
    pub fn from_user(&self) -> bool {
        self.cs & 3 == 3
    }
}

core::arch::global_asm!(
    r#"
.section .text

// Shared tail of every interrupt stub.  On entry the stack holds
// [error code][vector][rip][cs][rflags][rsp][ss].
isr_common:
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    cld
    mov rdi, rsp
    call trap_dispatch

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax

    add rsp, 16          // drop vector + error code
    iretq

// Vectors that do not push an error code get a dummy one so that the frame
// layout is identical for every trap.
.macro ISR_NOERR vec
.global isr_stub_\vec
isr_stub_\vec:
    push 0
    push \vec
    jmp isr_common
.endm

.macro ISR_ERR vec
.global isr_stub_\vec
isr_stub_\vec:
    push \vec
    jmp isr_common
.endm

ISR_NOERR 0
ISR_NOERR 1
ISR_NOERR 2
ISR_NOERR 3
ISR_NOERR 4
ISR_NOERR 5
ISR_NOERR 6
ISR_NOERR 7
ISR_ERR   8
ISR_NOERR 9
ISR_ERR   10
ISR_ERR   11
ISR_ERR   12
ISR_ERR   13
ISR_ERR   14
ISR_NOERR 15
ISR_NOERR 16
ISR_ERR   17
ISR_NOERR 18
ISR_NOERR 19
ISR_NOERR 20
ISR_ERR   21
ISR_NOERR 22
ISR_NOERR 23
ISR_NOERR 24
ISR_NOERR 25
ISR_NOERR 26
ISR_NOERR 27
ISR_NOERR 28
ISR_ERR   29
ISR_ERR   30
ISR_NOERR 31

ISR_NOERR 32
ISR_NOERR 33
ISR_NOERR 34
ISR_NOERR 35
ISR_NOERR 36
ISR_NOERR 37
ISR_NOERR 38
ISR_NOERR 39
ISR_NOERR 40
ISR_NOERR 41
ISR_NOERR 42
ISR_NOERR 43
ISR_NOERR 44
ISR_NOERR 45
ISR_NOERR 46
ISR_NOERR 47
ISR_NOERR 128

.section .rodata
.global isr_stub_table
isr_stub_table:
    .quad isr_stub_0
    .quad isr_stub_1
    .quad isr_stub_2
    .quad isr_stub_3
    .quad isr_stub_4
    .quad isr_stub_5
    .quad isr_stub_6
    .quad isr_stub_7
    .quad isr_stub_8
    .quad isr_stub_9
    .quad isr_stub_10
    .quad isr_stub_11
    .quad isr_stub_12
    .quad isr_stub_13
    .quad isr_stub_14
    .quad isr_stub_15
    .quad isr_stub_16
    .quad isr_stub_17
    .quad isr_stub_18
    .quad isr_stub_19
    .quad isr_stub_20
    .quad isr_stub_21
    .quad isr_stub_22
    .quad isr_stub_23
    .quad isr_stub_24
    .quad isr_stub_25
    .quad isr_stub_26
    .quad isr_stub_27
    .quad isr_stub_28
    .quad isr_stub_29
    .quad isr_stub_30
    .quad isr_stub_31
    .quad isr_stub_32
    .quad isr_stub_33
    .quad isr_stub_34
    .quad isr_stub_35
    .quad isr_stub_36
    .quad isr_stub_37
    .quad isr_stub_38
    .quad isr_stub_39
    .quad isr_stub_40
    .quad isr_stub_41
    .quad isr_stub_42
    .quad isr_stub_43
    .quad isr_stub_44
    .quad isr_stub_45
    .quad isr_stub_46
    .quad isr_stub_47
    .quad isr_stub_128
"#
);

extern "C" {
    #[link_name = "isr_stub_table"]
    static ISR_STUB_TABLE: [u64; 49];
}

/// Index of the `int 0x80` stub inside [`ISR_STUB_TABLE`].
const SYSCALL_STUB_INDEX: usize = 48;
/// Vector the Unix system-call gate lives on.
pub const SYSCALL_VECTOR: u8 = 0x80;

const EXCEPTION_NAMES: [&str; 32] = [
    "divide error",
    "debug",
    "NMI",
    "breakpoint",
    "overflow",
    "bound range exceeded",
    "invalid opcode",
    "device not available",
    "double fault",
    "coprocessor segment overrun",
    "invalid TSS",
    "segment not present",
    "stack segment fault",
    "general protection fault",
    "page fault",
    "reserved",
    "x87 floating point",
    "alignment check",
    "machine check",
    "SIMD floating point",
    "virtualisation",
    "control protection",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "reserved",
    "hypervisor injection",
    "VMM communication",
    "security exception",
    "reserved",
    "reserved",
];

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Number of timer interrupts since boot (the PIT runs at ~100 Hz).
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Entry point of every interrupt and exception.
///
/// # Safety
/// Called from assembly with a pointer to a complete [`TrapFrame`].
#[no_mangle]
pub unsafe extern "C" fn trap_dispatch(frame: *mut TrapFrame) {
    let frame = &mut *frame;
    let vector = frame.vector;
    match vector {
        0x80 => crate::syscall::dispatch(frame),
        32 => {
            TICKS.fetch_add(1, Ordering::Relaxed);
            if TICKS.load(Ordering::Relaxed) % 1000 == 0 {
                crate::serial_println!("timer tick {}", TICKS.load(Ordering::Relaxed));
            }
            end_of_interrupt(32);
        }
        33 => {
            let scancode = port::inb(0x60);
            if scancode & 0x80 == 0 {
                if let Some(ch) = scancode_to_ascii(scancode) {
                    crate::kb_buffer::on_key(ch);
                }
            }
            end_of_interrupt(33);
        }
        32..=47 => end_of_interrupt(vector),
        _ => exception(frame),
    }
}

fn end_of_interrupt(vector: u64) {
    unsafe {
        if vector >= 40 {
            port::outb(0xa0, 0x20);
        }
        port::outb(0x20, 0x20);
    }
}

/// Report a CPU exception; kill the process when it came from ring 3.
fn exception(frame: &mut TrapFrame) -> ! {
    let vector = frame.vector as usize;
    let name = EXCEPTION_NAMES.get(vector).copied().unwrap_or("unknown");
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags)) };

    crate::println!(
        "EXCEPTION {} ({}) at {:#x} cs={:#x} err={:#x}",
        vector,
        name,
        frame.rip,
        frame.cs,
        frame.error_code
    );
    if vector == 14 {
        let err = frame.error_code;
        let cause = if err & 1 != 0 { "protection" } else { "absent" };
        let access = if err & 2 != 0 { "write" } else { "read" };
        let origin = if err & 4 != 0 { "user" } else { "kernel" };
        crate::println!(
            "  page fault: address {:#x} ({}, {}, from {})",
            cr2,
            cause,
            access,
            origin
        );
    }

    if frame.from_user() {
        crate::usermode::abort_current(name);
    }

    crate::println!("kernel fault — halting");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

fn scancode_to_ascii(scancode: u8) -> Option<char> {
    let ch = match scancode & 0x7f {
        0x01 => '1',
        0x02 => '2',
        0x03 => '3',
        0x04 => '4',
        0x05 => '5',
        0x06 => '6',
        0x07 => '7',
        0x08 => '8',
        0x09 => '9',
        0x0a => '0',
        0x0b => '-',
        0x0c => '=',
        0x0d => '\n',
        0x0e => '\u{8}',
        0x0f => '\t',
        0x10 => 'q',
        0x11 => 'w',
        0x12 => 'e',
        0x13 => 'r',
        0x14 => 't',
        0x15 => 'y',
        0x16 => 'u',
        0x17 => 'i',
        0x18 => 'o',
        0x19 => 'p',
        0x1a => '[',
        0x1b => ']',
        0x1c => '\n',
        0x1e => 'a',
        0x1f => 's',
        0x20 => 'd',
        0x21 => 'f',
        0x22 => 'g',
        0x23 => 'h',
        0x24 => 'j',
        0x25 => 'k',
        0x26 => 'l',
        0x27 => ';',
        0x28 => '\'',
        0x29 => '`',
        0x2b => '\\',
        0x2c => 'z',
        0x2d => 'x',
        0x2e => 'c',
        0x2f => 'v',
        0x30 => 'b',
        0x31 => 'n',
        0x32 => 'm',
        0x33 => ',',
        0x34 => '.',
        0x35 => '/',
        0x39 => ' ',
        _ => return None,
    };
    Some(ch)
}

/// Install the IDT, remap the PIC, start the timer and enable interrupts.
pub fn init() {
    unsafe {
        for vector in 0..48usize {
            idt::entry(vector).set_handler(ISR_STUB_TABLE[vector]);
        }
        // The double fault runs on its own IST stack so it survives a broken
        // kernel stack.
        idt::entry(8).ist = 1;
        // The system-call gate must be callable from ring 3.
        let stub = ISR_STUB_TABLE[SYSCALL_STUB_INDEX];
        idt::entry(SYSCALL_VECTOR as usize).set_handler_with_dpl(stub, 3);

        idt::load();
        remap_pic();
        init_pit();

        // Unmask the timer (IRQ0) and the keyboard (IRQ1).
        port::outb(0x21, 0xfc);
        port::outb(0xa1, 0xff);

        asm!("sti", options(nomem, nostack));
    }
}

unsafe fn remap_pic() {
    port::outb(0x20, 0x11);
    port::io_wait();
    port::outb(0xa0, 0x11);
    port::io_wait();
    port::outb(0x21, 0x20);
    port::io_wait();
    port::outb(0xa1, 0x28);
    port::io_wait();
    port::outb(0x21, 0x04);
    port::io_wait();
    port::outb(0xa1, 0x02);
    port::io_wait();
    port::outb(0x21, 0x01);
    port::io_wait();
    port::outb(0xa1, 0x01);
    port::io_wait();
    port::outb(0x21, 0xff);
    port::outb(0xa1, 0xff);
}

unsafe fn init_pit() {
    let divisor: u16 = (1_193_182u32 / 100) as u16;
    port::outb(0x43, 0x36);
    port::outb(0x40, (divisor & 0xff) as u8);
    port::outb(0x40, (divisor >> 8) as u8);
}
