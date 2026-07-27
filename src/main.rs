//! Ferrumix — a tiny Unix-like kernel in Rust.
//!
//! Entry flow:
//!   boot.S  -> long mode, then `call kernel_main(magic, mb_info)`
//!   here    -> initialise drivers, enable interrupts, idle.

#![no_std]
#![no_main]

mod boot; // pulls in boot.S (Multiboot2 trampoline)
mod gdt;
mod idt;
mod interrupts;
mod multiboot;
mod port;
mod serial;
mod spinlock;
mod vga;

use core::arch::asm;
use core::panic::PanicInfo;

/// Kernel entry point. Called from `boot.S` in long mode with:
///   magic       = EAX = Multiboot2 magic (0x36d76289)
///   mb_info     = EBX = physical address of the Multiboot2 info struct
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
        "detected usable RAM: {} MiB",
        info.usable_memory / (1024 * 1024)
    );

    gdt::init();
    println!("GDT + TSS initialised");

    interrupts::init();
    println!("IDT + PIC + PIT initialised; interrupts enabled");

    println!("Ferrumix is alive. (idle loop; timer ticks on the serial line)");
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("KERNEL PANIC: {}", info);
    serial::serial_println!("KERNEL PANIC: {}", info);
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}
