//! Minimal userspace program scaffold for Ferrumix.
//!
//! Right now this is illustrative: it shows the intended shape of a
//! freestanding (`no_std`, `no_main`) userspace binary that talks to the
//! kernel through the `syscall!` ABI. It is not yet compiled or loaded by the
//! kernel — ring-3 task support is the next milestone. See userspace/README.md.

#![no_std]
#![no_main]

mod syscall;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    syscall::write(1, b"hello from ferrumix userspace\n");
    syscall::exit(0);
}
