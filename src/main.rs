//! Ferrumix — a tiny Unix-like kernel in Rust.
//!
//! Boot flow:
//!
//! ```text
//!   boot.S   32-bit Multiboot2 entry, page tables, long mode
//!   main.rs  console -> memory -> paging -> GDT/TSS -> processes -> VFS
//!            -> interrupts -> syscalls -> run an ELF program in ring 3
//!            -> interactive shell
//! ```

#![no_std]
#![no_main]

mod boot;
mod console;
mod elf;
mod fd;
mod gdt;
mod idt;
mod interrupts;
mod kb_buffer;
mod memory;
mod multiboot;
mod paging;
mod port;
mod process;
mod serial;
mod shell;
mod spinlock;
mod syscall;
mod uaccess;
mod user_program;
mod usermode;
mod vfs;
mod vga;

use core::arch::asm;
use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn kernel_main(magic: u32, mb_info: u32) -> ! {
    serial::init();
    serial::trace(b'1');
    vga::WRITER.lock().clear();
    serial::trace(b'2');

    println!("Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust");
    println!("boot magic: {:#x}, boot info @ {:#x}", magic, mb_info);
    serial::trace(b'4');
    console::init();
    serial::trace(b'5');

    let info = unsafe { multiboot::parse(magic, mb_info as usize) };
    println!(
        "boot protocol: {}, usable RAM: {} MiB in {} region(s)",
        info.protocol,
        info.usable_memory / (1024 * 1024),
        info.region_count
    );

    serial::trace(b'6');
    memory::init(&info);
    serial::trace(b'7');
    paging::init();
    serial::trace(b'8');

    gdt::init();
    serial::trace(b'9');
    println!("GDT + TSS initialised (kernel + ring-3 segments, IST, rsp0)");

    process::init();
    serial::trace(b'a');
    println!(
        "process table: pid {} running, {} entries used",
        process::current_pid(),
        process::process_count()
    );

    vfs::init();
    serial::trace(b'b');

    interrupts::init();
    serial::trace(b'c');
    println!("IDT + PIC + PIT initialised; interrupts enabled");
    syscall::init();
    serial::trace(b'd');

    if let Some(frame) = memory::alloc_frame() {
        let (total, used, free) = memory::stats();
        println!(
            "memory: frame {:#x} allocated, {} total / {} used / {} free",
            frame, total, used, free
        );
        memory::free_frame(frame);
    }

    println!("Ferrumix is alive.");

    // Load the embedded ELF program and run it in ring 3.  This exercises the
    // whole chain: address space creation, ELF parsing, stack mapping, the
    // privilege switch and system calls coming back from user space.
    serial::trace(b'e');
    match usermode::exec("hello", user_program::hello_elf()) {
        Ok(status) => println!("init: ring 3 program finished with status {}", status),
        Err(err) => println!("init: could not run the ring 3 program: {:?}", err),
    }

    serial::trace(b'f');
    shell::run()
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // Report through the lock-free serial writer: the panic may well have
    // happened while the console lock was held, and a deadlock here would
    // hide the message completely.
    use core::fmt::Write;
    let _ = writeln!(serial::RawSerial, "\nKERNEL PANIC: {}", info);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
