//! Minimal x86 port I/O helpers (no external crates).

use core::arch::asm;

/// Write a byte to an I/O port.
pub unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

/// Read a byte from an I/O port.
pub unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in al, dx", out("al") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// Write a word (16 bits) to an I/O port.
pub unsafe fn outw(port: u16, val: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
}

/// Read a word (16 bits) from an I/O port.
pub unsafe fn inw(port: u16) -> u16 {
    let ret: u16;
    asm!("in ax, dx", out("ax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// Write a double word (32 bits) to an I/O port.
pub unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

/// Read a double word (32 bits) from an I/O port.
pub unsafe fn inl(port: u16) -> u32 {
    let ret: u32;
    asm!("in eax, dx", out("eax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// A short delay used after programming the PIC/PS2 hardware.
pub fn io_wait() {
    unsafe { outb(0x80, 0); }
}

/// Halt the CPU until the next interrupt.
pub unsafe fn hlt() {
    asm!("hlt", options(nomem, nostack, preserves_flags));
}

/// Enable maskable interrupts.
pub unsafe fn sti() {
    // sti/cli modify the IF flag, so we must not claim preserves_flags.
    asm!("sti", options(nomem, nostack));
}

/// Disable maskable interrupts.
pub unsafe fn cli() {
    asm!("cli", options(nomem, nostack));
}
