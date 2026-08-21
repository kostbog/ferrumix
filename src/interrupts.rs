//! Interrupt wiring: exceptions (IDT), the 8259 PIC remap, the PIT timer,
//! keyboard, and the Unix syscall gate (int 0x80).

use crate::idt;
use crate::port;
use core::arch::asm;

core::arch::global_asm!(
    r#"
    .macro PUSH_REGS
        push r15
        push r14
        push r13
        push r12
        push r11
        push r10
        push r9
        push r8
        push rbp
        push rdi
        push rsi
        push rdx
        push rcx
        push rbx
        push rax
    .endm

    .macro POP_REGS
        pop rax
        pop rbx
        pop rcx
        pop rdx
        pop rsi
        pop rdi
        pop rbp
        pop r8
        pop r9
        pop r10
        pop r11
        pop r12
        pop r13
        pop r14
        pop r15
    .endm

    .global default_exception_stub
    .type default_exception_stub, @function
default_exception_stub:
        PUSH_REGS
        mov rdi, [rsp + 15*8]
        call default_exception_handler
        POP_REGS
        iretq

    .global breakpoint_stub
    .type breakpoint_stub, @function
breakpoint_stub:
        PUSH_REGS
        mov rdi, [rsp + 15*8]
        call breakpoint_handler
        POP_REGS
        iretq

    .global double_fault_stub
    .type double_fault_stub, @function
double_fault_stub:
        PUSH_REGS
        mov rdi, [rsp + 15*8 + 8]
        mov rsi, [rsp + 15*8]
        call double_fault_handler
        POP_REGS
        add rsp, 8
        iretq

    .global page_fault_stub
    .type page_fault_stub, @function
page_fault_stub:
        PUSH_REGS
        mov rdi, [rsp + 15*8 + 8]
        mov rsi, [rsp + 15*8]
        call page_fault_handler
        POP_REGS
        add rsp, 8
        iretq

    .global timer_stub
    .type timer_stub, @function
timer_stub:
        PUSH_REGS
        call timer_handler
        POP_REGS
        iretq

    .global keyboard_stub
    .type keyboard_stub, @function
keyboard_stub:
        PUSH_REGS
        call keyboard_handler
        POP_REGS
        iretq
"#
);

extern "C" {
    fn default_exception_stub();
    fn breakpoint_stub();
    fn double_fault_stub();
    fn page_fault_stub();
    fn timer_stub();
    fn keyboard_stub();
}

#[no_mangle]
extern "C" fn default_exception_handler(rip: u64) {
    crate::serial::serial_println!("EXCEPTION @ {:#x} (default handler)", rip);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[no_mangle]
extern "C" fn breakpoint_handler(rip: u64) {
    crate::serial::serial_println!("BREAKPOINT @ {:#x}", rip);
}

#[no_mangle]
extern "C" fn double_fault_handler(rip: u64, code: u64) {
    crate::serial::serial_println!("DOUBLE FAULT (error={}) @ {:#x}", code, rip);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[no_mangle]
extern "C" fn page_fault_handler(rip: u64, code: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
    crate::serial::serial_println!(
        "PAGE FAULT @ {:#x} (cr2={:#x}, err={:#x})",
        rip,
        cr2,
        code
    );
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
extern "C" fn timer_handler() {
    unsafe {
        TICKS += 1;
        if TICKS % 1000 == 0 {
            crate::serial::serial_println!("timer tick {}", TICKS);
        }
    }
    unsafe { port::outb(0x20, 0x20) };
}

#[no_mangle]
extern "C" fn keyboard_handler() {
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
            idt::IDT[i].set_handler(default_exception_stub as usize as u64);
        }
        idt::IDT[3].set_handler(breakpoint_stub as usize as u64);
        idt::IDT[8].set_handler(double_fault_stub as usize as u64);
        idt::IDT[8].ist = 1;
        idt::IDT[14].set_handler(page_fault_stub as usize as u64);

        idt::IDT[32].set_handler(timer_stub as usize as u64);
        idt::IDT[33].set_handler(keyboard_stub as usize as u64);

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
