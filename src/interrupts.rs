//! Interrupt wiring: exceptions (IDT), the 8259 PIC remap, the PIT timer,
//! keyboard, and the Unix syscall gate (int 0x80).
//!
//! Since `extern "x86-interrupt"` is unstable on stable Rust, we use
//! assembly wrappers that save/restore registers and call Rust handlers.

use crate::idt;
use crate::port;
use core::arch::asm;
use core::arch::global_asm;

#[repr(C)]
pub struct InterruptStackFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// Stack layout after our stub saves registers
#[repr(C)]
struct InterruptRegs {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
}

// Generate assembly stubs for interrupt handlers without error code
global_asm!(
    r#"
.global default_handler_stub
.type default_handler_stub, @function
default_handler_stub:
    push 0
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
    mov rdi, rsp
    call default_handler_impl
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
    add rsp, 8
    iretq
"#
);

global_asm!(
    r#"
.global breakpoint_stub
.type breakpoint_stub, @function
breakpoint_stub:
    push 0
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
    mov rdi, rsp
    call breakpoint_impl
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
    add rsp, 8
    iretq
"#
);

// Generate assembly stubs for interrupt handlers with error code
global_asm!(
    r#"
.global double_fault_stub
.type double_fault_stub, @function
double_fault_stub:
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
    mov rdi, rsp
    call double_fault_impl
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
    add rsp, 8
    iretq
"#
);

global_asm!(
    r#"
.global page_fault_stub
.type page_fault_stub, @function
page_fault_stub:
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
    mov rdi, rsp
    call page_fault_impl
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
    add rsp, 8
    iretq
"#
);

global_asm!(
    r#"
.global timer_handler_stub
.type timer_handler_stub, @function
timer_handler_stub:
    push 0
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
    mov rdi, rsp
    call timer_handler_impl
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
    add rsp, 8
    iretq
"#
);

global_asm!(
    r#"
.global keyboard_handler_stub
.type keyboard_handler_stub, @function
keyboard_handler_stub:
    push 0
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
    mov rdi, rsp
    call keyboard_handler_impl
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
    add rsp, 8
    iretq
"#
);

extern "C" {
    fn default_handler_stub();
    fn breakpoint_stub();
    fn double_fault_stub();
    fn page_fault_stub();
    fn timer_handler_stub();
    fn keyboard_handler_stub();
}

#[no_mangle]
extern "C" fn default_handler_impl(regs: *mut InterruptRegs) {
    unsafe {
        crate::serial::serial_println!("EXCEPTION @ {:#x} (default handler)", (*regs).rip);
    }
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[no_mangle]
extern "C" fn breakpoint_impl(regs: *mut InterruptRegs) {
    unsafe {
        crate::serial::serial_println!("BREAKPOINT @ {:#x}", (*regs).rip);
    }
}

#[no_mangle]
extern "C" fn double_fault_impl(regs: *mut InterruptRegs) {
    unsafe {
        crate::serial::serial_println!("DOUBLE FAULT (error={}) @ {:#x}", (*regs).error_code, (*regs).rip);
    }
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[no_mangle]
extern "C" fn page_fault_impl(regs: *mut InterruptRegs) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
    unsafe {
        crate::serial::serial_println!(
            "PAGE FAULT @ {:#x} (cr2={:#x}, err={:#x})",
            (*regs).rip,
            cr2,
            (*regs).error_code
        );
    }
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

static mut TICKS: u64 = 0;

/// Get the current timer tick count.
pub fn get_ticks() -> u64 {
    unsafe { TICKS }
}

#[no_mangle]
extern "C" fn timer_handler_impl(_regs: *mut InterruptRegs) {
    unsafe {
        TICKS += 1;
        if TICKS % 1000 == 0 {
            crate::serial::serial_println!("timer tick {}", TICKS);
        }
    }
    unsafe { port::outb(0x20, 0x20) };
}

#[no_mangle]
extern "C" fn keyboard_handler_impl(_regs: *mut InterruptRegs) {
    let scan = unsafe { port::inb(0x60) };
    // Only handle key-down events (bit 7 clear)
    if scan & 0x80 == 0 {
        if let Some(c) = scancode_to_ascii(scan) {
            crate::kb_buffer::on_key(c);
        }
    }
    unsafe { port::outb(0x20, 0x20) };
}

fn scancode_to_ascii(s: u8) -> Option<char> {
    let c = match s & 0x7F {
        0x01 => '1',
        0x02 => '2',
        0x03 => '3',
        0x04 => '4',
        0x05 => '5',
        0x06 => '6',
        0x07 => '7',
        0x08 => '8',
        0x09 => '9',
        0x0A => '0',
        0x0B => '-',
        0x0C => '=',
        0x0D => '\n',
        0x0E => '\u{8}',
        0x0F => '\t',
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
        0x1A => '[',
        0x1B => ']',
        0x1C => '\n',
        0x1E => 'a',
        0x1F => 's',
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
        0x2B => '\\',
        0x2C => 'z',
        0x2D => 'x',
        0x2E => 'c',
        0x2F => 'v',
        0x30 => 'b',
        0x31 => 'n',
        0x32 => 'm',
        0x33 => ',',
        0x34 => '.',
        0x35 => '/',
        0x39 => ' ',
        _ => return None,
    };
    Some(c)
}

pub fn init() {
    unsafe {
        for i in 0..32 {
            idt::IDT[i].set_handler(default_handler_stub as usize as u64);
        }
        idt::IDT[3].set_handler(breakpoint_stub as usize as u64);
        idt::IDT[8].set_handler(double_fault_stub as usize as u64);
        idt::IDT[8].ist = 1;
        idt::IDT[14].set_handler(page_fault_stub as usize as u64);

        idt::IDT[32].set_handler(timer_handler_stub as usize as u64);
        idt::IDT[33].set_handler(keyboard_handler_stub as usize as u64);

        idt::IDT[0x80].set_handler_with_dpl(crate::syscall::syscall_int80_entry as usize as u64, 3);

        idt::load();
        remap_pic();
        init_pit();

        port::outb(0x21, 0xFC);
        port::outb(0xA1, 0xFF);

        asm!("sti", options(nomem, nostack));
    }
    crate::syscall::init();
}

unsafe fn remap_pic() {
    port::outb(0x20, 0x11);
    port::io_wait();
    port::outb(0xA0, 0x11);
    port::io_wait();
    port::outb(0x21, 0x20);
    port::io_wait();
    port::outb(0xA1, 0x28);
    port::io_wait();
    port::outb(0x21, 0x04);
    port::io_wait();
    port::outb(0xA1, 0x02);
    port::io_wait();
    port::outb(0x21, 0x01);
    port::io_wait();
    port::outb(0xA1, 0x01);
    port::io_wait();
    port::outb(0x21, 0xFF);
    port::outb(0xA1, 0xFF);
}

unsafe fn init_pit() {
    let divisor: u16 = 1193182 / 100;
    port::outb(0x43, 0x36);
    port::outb(0x40, (divisor & 0xFF) as u8);
    port::outb(0x40, (divisor >> 8) as u8);
}
