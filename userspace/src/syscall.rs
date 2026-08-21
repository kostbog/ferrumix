//! The Ferrumix user/kernel system-call ABI.
//!
//! Arguments go in `rdi`, `rsi`, `rdx` (the Linux x86-64 convention), the call
//! number in `rax`, and the kernel is entered with `int 0x80`.  The result
//! comes back in `rax`, negative values being errno codes.
//!
//!   0  read(fd, buf, len)   -> bytes read
//!   1  write(fd, buf, len)  -> bytes written
//!   2  open(path, 0, 0)     -> fd
//!   3  close(fd)            -> 0
//!  12  brk(addr)            -> program break
//!  39  getpid()             -> pid
//!  60  exit(status)         -> does not return
//!
//! A faster `syscall`/`sysret` path (MSR_LSTAR) is planned; the software
//! interrupt keeps the ABI identical either way.

use core::arch::asm;

const SYS_READ: usize = 0;
const SYS_WRITE: usize = 1;
const SYS_OPEN: usize = 2;
const SYS_CLOSE: usize = 3;
const SYS_GETPID: usize = 39;
const SYS_EXIT: usize = 60;

unsafe fn syscall3(number: usize, a: usize, b: usize, c: usize) -> isize {
    let ret: isize;
    asm!(
        "int 0x80",
        inlateout("rax") number => ret,
        in("rdi") a,
        in("rsi") b,
        in("rdx") c,
    );
    ret
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    unsafe { syscall3(SYS_READ, fd, buf.as_mut_ptr() as usize, buf.len()) }
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

/// `path` must be NUL terminated, e.g. `"/dev/ttyS0\0"`.
pub fn open(path: &str) -> isize {
    unsafe { syscall3(SYS_OPEN, path.as_ptr() as usize, 0, 0) }
}

pub fn close(fd: usize) -> isize {
    unsafe { syscall3(SYS_CLOSE, fd, 0, 0) }
}

pub fn getpid() -> isize {
    unsafe { syscall3(SYS_GETPID, 0, 0, 0) }
}

pub fn exit(status: usize) -> ! {
    unsafe {
        syscall3(SYS_EXIT, status, 0, 0);
    }
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
