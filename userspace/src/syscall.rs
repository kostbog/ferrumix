//! Planned userspace <-> kernel system-call ABI.
//!
//! SYSCALL NUMBERS (draft):
//!   0  exit(code: usize)                              -> !
//!   1  write(fd: usize, buf: *const u8, len: usize)   -> usize
//!
//! For now this uses the legacy `int 0x80` software-interrupt convention. Once
//! ring-3 tasks exist, it will switch to the faster `syscall`/`sysenter`
//! instructions with arguments in the standard registers (RDI, RSI, RDX, ...).

use core::arch::asm;

pub fn exit(code: usize) -> ! {
    unsafe {
        asm!(
            "int 0x80",
            in("rax") 0,     // syscall number
            in("rdi") code,
            options(nomem, noreturn)
        );
    }
    loop {}
}

pub fn write(fd: usize, buf: &[u8]) -> usize {
    let ret: usize;
    unsafe {
        asm!(
            "int 0x80",
            in("rax") 1,             // syscall number
            in("rdi") fd,
            in("rsi") buf.as_ptr(),
            in("rdx") buf.len(),
            lateout("rax") ret,
            options(nomem)
        );
    }
    ret
}
