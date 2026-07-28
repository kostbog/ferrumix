//! Ferrumix — a tiny Unix-like kernel in Rust.
//!
//! Entry flow:
//!   boot.S  -> long mode, then `call kernel_main(magic, mb_info)`
//!   here    -> initialise drivers, memory, process table, syscall gate,
//!             VFS, enable interrupts, idle.

#![no_std]
#![no_main]

mod boot;
mod gdt;
mod idt;
mod interrupts;
mod memory;
mod multiboot;
mod paging;
mod port;
mod process;
mod serial;
mod spinlock;
mod syscall;
mod vfs;
mod vga;

use core::arch::asm;
use core::panic::PanicInfo;

#[no_mangle]
pub extern "C" fn kernel_main(magic: u32, mb_info: u32) -> ! {
    vga::WRITER.lock().clear();
    serial::init();

    println!("Ferrumix 0.1.0 — a tiny Unix-like kernel in Rust");
    println!("boot magic: {:#x}, multiboot info @ {:#x}", magic, mb_info);
    if magic != 0x36d76289 {
        println!("WARNING: unexpected Multiboot2 magic");
    }

    let info = unsafe { multiboot::Info::parse(mb_info as usize) };
    println!(
        "detected usable RAM: {} MiB ({} regions)",
        info.usable_memory / (1024 * 1024),
        info.region_count
    );

    memory::init(&info);
    paging::init();
    gdt::init();
    println!("GDT + TSS initialised (kernel + user segments, IST)");

    process::init();
    println!("process table: pid {} running, {} total", process::current_pid(), process::process_count());

    vfs::init();
    println!("VFS initialised: devfs with null, zero, tty");

    interrupts::init();
    println!("IDT + PIC + PIT initialised; interrupts enabled");
    println!("syscall gate: int 0x80 DPL=3 installed (write, exit, getpid)");

    if let Some(f1) = memory::alloc_frame() {
        println!("memory: allocated frame @ {:#x}", f1);
        let (total, used, free) = memory::stats();
        println!("memory: total {} used {} free {}", total, used, free);
        memory::free_frame(f1);
        println!("memory: freed frame @ {:#x} (free list works)", f1);
    }

    println!("testing syscall via int 0x80 ...");
    unsafe { test_syscall_write(); }

    let pid = unsafe { test_syscall_getpid() };
    println!("syscall getpid() -> {}", pid);

    println!("custom target: x86_64-ferrumix.json ready (build via --target x86_64-ferrumix.json)");

    println!("Ferrumix is alive. (idle loop; timer ticks on the serial line)");
    println!("Unix step complete: ring3 GDT, frame allocator, syscall int 0x80, process table, VFS devfs");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

unsafe fn test_syscall_write() {
    let msg = b"syscall: write via int 0x80 works — hello Unix!\n";
    let ret: u64;
    asm!(
        "int 0x80",
        in("rax") 1u64,
        in("rdi") 1u64,
        in("rsi") msg.as_ptr(),
        in("rdx") msg.len(),
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    crate::serial::serial_println!("test syscall write returned {}", ret);
}

unsafe fn test_syscall_getpid() -> u64 {
    let ret: u64;
    asm!(
        "int 0x80",
        in("rax") 39u64,
        lateout("rax") ret,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    ret
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    serial::serial_println!("KERNEL PANIC: {}", info);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
