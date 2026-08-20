//! Interrupt wiring: exceptions (IDT), the 8259 PIC remap, the PIT timer,
//! keyboard, and the Unix syscall gate (int 0x80).
//!
//! Rust's `x86-interrupt` ABI is unstable, so small assembly entry stubs save
//! the interrupted register state and call ordinary C-ABI Rust handlers.

use crate::idt;
use crate::port;
use core::arch::{asm, global_asm};
use core::sync::atomic::{AtomicU64, Ordering};

global_asm!(
    r#"
.intel_syntax noprefix

.global exception_entry
exception_entry:
    push rax
    lea rax, [rip + exception_handler]
    jmp interrupt_common

.global breakpoint_entry
breakpoint_entry:
    push rax
    lea rax, [rip + breakpoint_handler]
    jmp interrupt_common

.global double_fault_entry
double_fault_entry:
    push rax
    lea rax, [rip + double_fault_handler]
    jmp interrupt_common

.global page_fault_entry
page_fault_entry:
    push rax
    lea rax, [rip + page_fault_handler]
    jmp interrupt_common

.global timer_entry
timer_entry:
    push rax
    lea rax, [rip + timer_handler]
    jmp interrupt_common

.global keyboard_entry
keyboard_entry:
    push rax
    lea rax, [rip + keyboard_handler]

interrupt_common:
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

    mov r12, rsp
    and rsp, -16
    call rax
    mov rsp, r12

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
    iretq
"#
);

extern "C" {
    fn exception_entry();
    fn breakpoint_entry();
    fn double_fault_entry();
    fn page_fault_entry();
    fn timer_entry();
    fn keyboard_entry();
}

#[no_mangle]
extern "C" fn exception_handler() -> ! {
    crate::serial_println!("CPU EXCEPTION (default handler)");
    halt_forever()
}

#[no_mangle]
extern "C" fn breakpoint_handler() {
    crate::serial_println!("BREAKPOINT");
}

#[no_mangle]
extern "C" fn double_fault_handler() -> ! {
    crate::serial_println!("DOUBLE FAULT");
    halt_forever()
}

#[no_mangle]
extern "C" fn page_fault_handler() -> ! {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
    crate::serial_println!("PAGE FAULT (cr2={:#x})", cr2);
    halt_forever()
}

fn halt_forever() -> ! {
    loop {
        unsafe { asm!("cli; hlt", options(nomem, nostack)) };
    }
}

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Get the current timer tick count.
pub fn get_ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

#[no_mangle]
extern "C" fn timer_handler() {
    let ticks = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if ticks % 1000 == 0 {
        crate::serial_println!("timer tick {}", ticks);
    }
    unsafe { port::outb(0x20, 0x20) };
}

#[no_mangle]
extern "C" fn keyboard_handler() {
    let scancode = unsafe { port::inb(0x60) };
    // The decoder needs both make and break codes to track Shift correctly.
    crate::kb_buffer::on_scancode(scancode);
    unsafe { port::outb(0x20, 0x20) };
}

pub fn init() {
    unsafe {
        for i in 0..32 {
            idt::IDT[i].set_handler(exception_entry as usize as u64);
        }
        idt::IDT[3].set_handler(breakpoint_entry as usize as u64);
        idt::IDT[8].set_handler(double_fault_entry as usize as u64);
        idt::IDT[8].ist = 1;
        idt::IDT[14].set_handler(page_fault_entry as usize as u64);

        idt::IDT[32].set_handler(timer_entry as usize as u64);
        idt::IDT[33].set_handler(keyboard_entry as usize as u64);

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
