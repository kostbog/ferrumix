//! Unix system call interface — `int 0x80` handler.

use core::arch::asm;

pub const SYS_USER_EXIT: u64 = 0;
pub const SYS_USER_WRITE: u64 = 1;
pub const SYS_WRITE: u64 = 1;
pub const SYS_BRK: u64 = 12;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXIT: u64 = 60;

pub const STDOUT: usize = 1;
pub const STDERR: usize = 2;
pub const ENOSYS: u64 = 38;
pub const EINVAL: u64 = 22;
pub const EFAULT: u64 = 14;

#[repr(C)]
pub struct SyscallStack {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

core::arch::global_asm!(
    r#"
    .global syscall_int80_entry
    .type syscall_int80_entry, @function
syscall_int80_entry:
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

    mov rdi, rsp
    call syscall_dispatch

    mov [rsp], rax

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
    iretq
"#
);

extern "C" {
    pub fn syscall_int80_entry();
}

#[no_mangle]
pub extern "C" fn syscall_dispatch(stack: *mut SyscallStack) -> u64 {
    unsafe {
        let s = &*stack;
        let nr = s.rax;
        match nr {
            SYS_USER_WRITE | SYS_WRITE => do_write(s.rdi as usize, s.rsi as *const u8, s.rdx as usize),
            SYS_USER_EXIT | SYS_EXIT => do_exit(s.rdi as usize),
            SYS_GETPID => crate::process::current_pid(),
            SYS_BRK => {
                crate::serial::serial_println!("syscall: brk({:#x}) -> stub 0", s.rdi);
                0
            }
            _ => {
                crate::serial::serial_println!("syscall: unknown nr {} (rip={:#x}) -> ENOSYS", nr, s.rip);
                (-(ENOSYS as i64)) as u64
            }
        }
    }
}

fn do_write(fd: usize, buf_ptr: *const u8, len: usize) -> u64 {
    if buf_ptr.is_null() {
        return (-(EFAULT as i64)) as u64;
    }
    if len == 0 {
        return 0;
    }
    if len > 1024 * 1024 {
        return (-(EINVAL as i64)) as u64;
    }

    if fd != STDOUT && fd != STDERR && fd != 0 {
        crate::serial::serial_println!("syscall: write fd={} len={} -> alias to stdout", fd, len);
    }

    let slice = unsafe { core::slice::from_raw_parts(buf_ptr, len) };

    for &b in slice {
        crate::vga::WRITER.lock().write_byte(b);
        unsafe {
            while (crate::port::inb(0x3F8 + 5) & 0x20) == 0 {
                core::hint::spin_loop();
            }
            crate::port::outb(0x3F8, b);
        }
    }

    len as u64
}

fn do_exit(code: usize) -> u64 {
    crate::serial::serial_println!("syscall: exit({}) pid={}", code, crate::process::current_pid());
    crate::println!("process exit({}) via syscall pid={}", code, crate::process::current_pid());
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

pub fn init() {
    crate::serial::serial_println!(
        "syscall: handler at {:#x}, int 0x80 DPL=3 ready",
        syscall_int80_entry as *const () as u64
    );
}
