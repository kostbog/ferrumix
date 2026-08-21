//! The Unix system call interface (`int 0x80`).
//!
//! A user program puts the call number in `rax` and the arguments in
//! `rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9` (the Linux x86-64 convention) and
//! executes `int 0x80`.  The trap stub in [`crate::interrupts`] saves the
//! registers, [`dispatch`] runs the call and the result — or a negative errno
//! — is written back into the saved `rax`.
//!
//! | nr | name   | arguments                   | returns                     |
//! |----|--------|-----------------------------|-----------------------------|
//! | 0  | read   | fd, buf, len                | bytes read                  |
//! | 1  | write  | fd, buf, len                | bytes written               |
//! | 2  | open   | path, flags, mode           | fd                          |
//! | 3  | close  | fd                          | 0                           |
//! | 12 | brk    | addr                        | new program break           |
//! | 39 | getpid | —                           | pid                         |
//! | 60 | exit   | status                      | does not return             |
//!
//! Pointers coming from ring 3 are never dereferenced directly: every buffer
//! goes through [`crate::uaccess`], which walks the process page tables and
//! rejects addresses that are unmapped, kernel-only or read-only.

use crate::console;
use crate::fd::{self, FileKind};
use crate::interrupts::TrapFrame;
use crate::paging;
use crate::uaccess;
use crate::usermode;

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_BRK: u64 = 12;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXIT: u64 = 60;

pub const STDIN: usize = 0;
pub const STDOUT: usize = 1;
pub const STDERR: usize = 2;

pub const EPERM: i64 = 1;
pub const ENOENT: i64 = 2;
pub const EBADF: i64 = 9;
pub const EFAULT: i64 = 14;
pub const EINVAL: i64 = 22;
pub const EMFILE: i64 = 24;
pub const ENOSYS: i64 = 38;

/// Largest buffer a single `read`/`write` may transfer.
const MAX_IO: usize = 1 << 20;
/// Size of the bounce buffer used to copy user data in chunks.
const CHUNK: usize = 256;

/// Who made the call: which address space its pointers live in, and whether
/// they need the full user-space validation.
#[derive(Clone, Copy)]
pub struct Caller {
    /// Page table root the pointers must be resolved in.
    pub root: u64,
    /// True when the call came from ring 3.
    pub user: bool,
}

impl Caller {
    fn of(frame: &TrapFrame) -> Self {
        if frame.came_from_user() {
            Caller {
                root: usermode::current_root(),
                user: true,
            }
        } else {
            Caller {
                root: paging::active_root(),
                user: false,
            }
        }
    }

    /// Copy `buf.len()` bytes from the caller's memory.
    fn read(&self, addr: u64, buf: &mut [u8]) -> Result<(), uaccess::UserFault> {
        if self.user {
            uaccess::copy_from_user(self.root, buf, addr)?;
        } else {
            // A ring-0 caller (the boot self-test) hands over kernel pointers.
            unsafe {
                core::ptr::copy_nonoverlapping(addr as *const u8, buf.as_mut_ptr(), buf.len())
            };
        }
        Ok(())
    }

    /// Copy bytes into the caller's memory.
    fn write(&self, addr: u64, data: &[u8]) -> Result<(), uaccess::UserFault> {
        if self.user {
            uaccess::copy_to_user(self.root, addr, data)?;
        } else {
            unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), addr as *mut u8, data.len()) };
        }
        Ok(())
    }

    /// Read a NUL terminated string from the caller's memory.
    fn read_str<'a>(&self, addr: u64, buf: &'a mut [u8]) -> Result<&'a str, uaccess::UserFault> {
        if self.user {
            return uaccess::copy_str_from_user(self.root, addr, buf);
        }
        for i in 0..buf.len() {
            let byte = unsafe { core::ptr::read_volatile((addr + i as u64) as *const u8) };
            if byte == 0 {
                return core::str::from_utf8(&buf[..i]).map_err(|_| uaccess::UserFault::BadRange);
            }
            buf[i] = byte;
        }
        Err(uaccess::UserFault::BadRange)
    }
}

/// Handle one system call and store the result in `frame.rax`.
pub fn dispatch(frame: &mut TrapFrame) {
    let number = frame.rax;
    let caller = Caller::of(frame);

    let result = match number {
        SYS_READ => sys_read(caller, frame.rdi as usize, frame.rsi, frame.rdx as usize),
        SYS_WRITE => sys_write(caller, frame.rdi as usize, frame.rsi, frame.rdx as usize),
        SYS_OPEN => sys_open(caller, frame.rdi),
        SYS_CLOSE => sys_close(frame.rdi as usize),
        SYS_BRK => usermode::brk(frame.rdi) as i64,
        SYS_GETPID => crate::process::current_pid() as i64,
        SYS_EXIT => usermode::exit_current(frame.rdi as i64),
        _ => {
            crate::serial_println!(
                "syscall: unknown call {} from {:#x} -> ENOSYS",
                number,
                frame.rip
            );
            -ENOSYS
        }
    };

    frame.rax = result as u64;
}

fn fault_to_errno(fault: uaccess::UserFault) -> i64 {
    match fault {
        uaccess::UserFault::BadRange => -EFAULT,
        uaccess::UserFault::NotMapped => -EFAULT,
        uaccess::UserFault::NotUser => -EPERM,
        uaccess::UserFault::NotWritable => -EFAULT,
    }
}

/// `write(fd, buf, len)` — text output to the screen and/or the serial port.
fn sys_write(caller: Caller, fd: usize, buf: u64, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if len > MAX_IO {
        return -EINVAL;
    }

    let kind = match fd::get(fd) {
        FileKind::Closed if fd <= STDERR => FileKind::Console,
        FileKind::Closed => return -EBADF,
        other => other,
    };
    let target = match kind {
        FileKind::Null => return len as i64, // /dev/null swallows everything
        FileKind::Zero => return len as i64,
        other => match other.write_target() {
            Some(target) => target,
            None => return -EBADF,
        },
    };

    let mut done = 0usize;
    let mut chunk = [0u8; CHUNK];
    while done < len {
        let take = core::cmp::min(CHUNK, len - done);
        let slice = &mut chunk[..take];
        if let Err(fault) = caller.read(buf + done as u64, slice) {
            return if done > 0 {
                done as i64
            } else {
                fault_to_errno(fault)
            };
        }
        console::write_bytes_to(target, slice);
        done += take;
    }
    done as i64
}

/// `read(fd, buf, len)` — line oriented input from the keyboard.
fn sys_read(caller: Caller, fd: usize, buf: u64, len: usize) -> i64 {
    if len == 0 {
        return 0;
    }
    if len > MAX_IO {
        return -EINVAL;
    }

    let kind = match fd::get(fd) {
        FileKind::Closed if fd == STDIN => FileKind::Stdin,
        FileKind::Closed => return -EBADF,
        other => other,
    };

    match kind {
        FileKind::Null => 0,
        FileKind::Zero => {
            let zeros = [0u8; CHUNK];
            let mut done = 0usize;
            while done < len {
                let take = core::cmp::min(CHUNK, len - done);
                if let Err(fault) = caller.write(buf + done as u64, &zeros[..take]) {
                    return fault_to_errno(fault);
                }
                done += take;
            }
            done as i64
        }
        FileKind::Stdin | FileKind::Console | FileKind::Screen | FileKind::Serial => {
            let mut line = [0u8; CHUNK];
            let want = core::cmp::min(len, CHUNK);
            let mut done = 0usize;
            while done < want {
                match crate::kb_buffer::try_read_char() {
                    Some(byte) => {
                        line[done] = byte;
                        done += 1;
                        if byte == b'\n' {
                            break;
                        }
                    }
                    None => {
                        if done > 0 {
                            break;
                        }
                        // Block until the keyboard interrupt delivers a key.
                        unsafe {
                            core::arch::asm!("sti", "hlt", options(nomem, nostack));
                        }
                    }
                }
            }
            match caller.write(buf, &line[..done]) {
                Ok(()) => done as i64,
                Err(fault) => fault_to_errno(fault),
            }
        }
        FileKind::Closed => -EBADF,
    }
}

/// `open(path, flags, mode)` — resolve a devfs path into a descriptor.
fn sys_open(caller: Caller, path_ptr: u64) -> i64 {
    let mut buf = [0u8; 64];
    let path = match caller.read_str(path_ptr, &mut buf) {
        Ok(path) => path,
        Err(fault) => return fault_to_errno(fault),
    };
    match fd::open(path) {
        Some(descriptor) => descriptor as i64,
        None => {
            if fd::open_count() >= fd::MAX_FD {
                -EMFILE
            } else {
                -ENOENT
            }
        }
    }
}

/// `close(fd)`.
fn sys_close(fd: usize) -> i64 {
    if fd::close(fd) {
        0
    } else {
        -EBADF
    }
}

/// Run a `write` system call from ring 0 to prove the gate works on boot.
pub fn self_test() {
    let message = b"syscall: int 0x80 self-test from ring 0 works\n";
    let written: u64;
    unsafe {
        core::arch::asm!(
            "int 0x80",
            inlateout("rax") SYS_WRITE => written,
            in("rdi") STDOUT,
            in("rsi") message.as_ptr(),
            in("rdx") message.len(),
        );
    }
    crate::serial_println!("syscall: self-test write returned {}", written as i64);
}

/// Announce the system-call gate on boot.
pub fn init() {
    crate::println!(
        "syscall: int 0x80 gate installed (DPL 3) — read, write, open, close, brk, getpid, exit"
    );
    self_test();
}
