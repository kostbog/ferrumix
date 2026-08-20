//! Interrupt wiring: exceptions (IDT), the 8259 PIC remap, the PIT timer,
//! keyboard, and the Unix syscall gate (int 0x80).

use crate::idt;
use crate::port;
use core::arch::asm;

#[repr(C)]
pub struct InterruptStackFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

extern "x86-interrupt" fn default_handler(frame: &mut InterruptStackFrame) {
    crate::serial::serial_println!("EXCEPTION @ {:#x} (default handler)", frame.rip);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

extern "x86-interrupt" fn breakpoint(frame: &mut InterruptStackFrame) {
    crate::serial::serial_println!("BREAKPOINT @ {:#x}", frame.rip);
}

extern "x86-interrupt" fn double_fault(frame: &mut InterruptStackFrame, code: u64) {
    crate::serial::serial_println!("DOUBLE FAULT (error={}) @ {:#x}", code, frame.rip);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

extern "x86-interrupt" fn page_fault(frame: &mut InterruptStackFrame, code: u64) {
    let cr2: u64;
    unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
    crate::serial::serial_println!(
        "PAGE FAULT @ {:#x} (cr2={:#x}, err={:#x})",
        frame.rip,
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

extern "x86-interrupt" fn timer_handler(_frame: &mut InterruptStackFrame) {
    unsafe {
        TICKS += 1;
        if TICKS % 1000 == 0 {
            crate::serial::serial_println!("timer tick {}", TICKS);
        }
    }
    unsafe { port::outb(0x20, 0x20) };
}

extern "x86-interrupt" fn keyboard_handler(_frame: &mut InterruptStackFrame) {
    let scancode = unsafe { port::inb(0x60) };
    // The decoder needs both make and break codes to track Shift correctly.
    crate::kb_buffer::on_scancode(scancode);
    unsafe { port::outb(0x20, 0x20) };
}

pub fn init() {
    unsafe {
        for i in 0..32 {
            idt::IDT[i].set_handler(default_handler as usize as u64);
        }
        idt::IDT[3].set_handler(breakpoint as usize as u64);
        idt::IDT[8].set_handler(double_fault as usize as u64);
        idt::IDT[8].ist = 1;
        idt::IDT[14].set_handler(page_fault as usize as u64);

        idt::IDT[32].set_handler(timer_handler as usize as u64);
        idt::IDT[33].set_handler(keyboard_handler as usize as u64);

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
