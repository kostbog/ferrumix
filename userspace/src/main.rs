//! A freestanding Ferrumix user program.
//!
//! It talks to the kernel only through the `int 0x80` ABI in `syscall.rs`:
//! prints a line on the console, opens the serial device and writes to it,
//! asks for its pid and exits.  The kernel loads programs like this one with
//! its ELF64 loader and runs them in ring 3 (see `userspace/README.md`).

#![no_std]
#![no_main]

mod syscall;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    syscall::write(1, b"hello from ferrumix userspace\n");

    let fd = syscall::open("/dev/ttyS0\0");
    if fd >= 0 {
        syscall::write(fd as usize, b"...and this line goes to the serial port\n");
        syscall::close(fd as usize);
    }

    let pid = syscall::getpid();
    syscall::exit(if pid > 0 { 0 } else { 1 });
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(101);
}
