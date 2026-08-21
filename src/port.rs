//! Minimal x86 port I/O helpers (no external crates).

use core::arch::asm;

/// Write a byte to an I/O port.
///
/// # Safety
/// Caller must ensure the port exists and that the write is valid for the
/// current hardware state.
pub unsafe fn outb(port: u16, val: u8) {
    asm!("out dx, al", in("dx") port, in("al") val, options(nomem, nostack, preserves_flags));
}

/// Read a byte from an I/O port.
///
/// # Safety
/// Caller must ensure the port exists and that reading it has no invalid side effects.
pub unsafe fn inb(port: u16) -> u8 {
    let ret: u8;
    asm!("in al, dx", out("al") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// Write a word (16 bits) to an I/O port.
///
/// # Safety
/// Caller must ensure the port accepts 16-bit writes in the current hardware state.
pub unsafe fn outw(port: u16, val: u16) {
    asm!("out dx, ax", in("dx") port, in("ax") val, options(nomem, nostack, preserves_flags));
}

/// Read a word (16 bits) from an I/O port.
///
/// # Safety
/// Caller must ensure the port accepts 16-bit reads in the current hardware state.
pub unsafe fn inw(port: u16) -> u16 {
    let ret: u16;
    asm!("in ax, dx", out("ax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// Write a double word (32 bits) to an I/O port.
///
/// # Safety
/// Caller must ensure the port accepts 32-bit writes in the current hardware state.
pub unsafe fn outl(port: u16, val: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") val, options(nomem, nostack, preserves_flags));
}

/// Read a double word (32 bits) from an I/O port.
///
/// # Safety
/// Caller must ensure the port accepts 32-bit reads in the current hardware state.
pub unsafe fn inl(port: u16) -> u32 {
    let ret: u32;
    asm!("in eax, dx", out("eax") ret, in("dx") port, options(nomem, nostack, preserves_flags));
    ret
}

/// A short delay used after programming the PIC/PS2 hardware.
pub fn io_wait() {
    unsafe {
        outb(0x80, 0);
    }
}

/// Halt the CPU until the next interrupt.
///
/// # Safety
/// Caller must ensure halting is valid at this point and that wake-up
/// interrupts are configured if needed.
pub unsafe fn hlt() {
    asm!("hlt", options(nomem, nostack, preserves_flags));
}

/// Enable maskable interrupts.
///
/// # Safety
/// Caller must ensure all interrupt handlers and descriptor tables are fully initialized.
pub unsafe fn sti() {
    // sti/cli modify the IF flag, so we must not claim preserves_flags.
    asm!("sti", options(nomem, nostack));
}

/// Disable maskable interrupts.
///
/// # Safety
/// Caller must restore the previous interrupt state when required by the
/// surrounding code.
pub unsafe fn cli() {
    asm!("cli", options(nomem, nostack));
}

/// Disable interrupts and return whether they were previously enabled.
///
/// Uses `pushf`+`cli`+`pop` to atomically:
///   1. Save original RFLAGS (with original IF state)
///   2. Disable interrupts
///   3. Load the saved flags to check IF
///
/// x86 does not reorder instructions within a single CPU.
///
/// # Safety
/// Caller must pair this with `popcli` on the same CPU and avoid blocking while
/// interrupts are disabled.
///
/// We use `inlateout(reg) 0u64` to tell the compiler a register is used as
/// scratch (the value 0 is arbitrary — it just ensures the compiler picks a
/// general-purpose register, not RSP).  The `pop` overwrites it with the
/// saved RFLAGS value.
pub unsafe fn pushcli() -> bool {
    let flags: u64;
    asm!(
        "pushf",
        "cli",
        "pop {f}",
        f = inlateout(reg) 0u64 => flags,
    );
    flags & (1 << 9) != 0
}

/// Re-enable interrupts if they were enabled before the matching pushcli.
///
/// # Safety
/// `was_enabled` must be the value returned by the matching `pushcli` on this CPU.
pub unsafe fn popcli(was_enabled: bool) {
    if was_enabled {
        asm!("sti", options(nomem, nostack));
    }
}
