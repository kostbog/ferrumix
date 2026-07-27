//! Boot trampoline (Multiboot2 entry + long-mode switch).
//!
//! The actual assembly lives in `boot.S`; we pull it into the crate here so it
//! is compiled and linked together with the Rust code. `_start` from that file
//! is the very first instruction executed by the CPU after the bootloader
//! hands over, and it eventually calls `kernel_main` defined in `main.rs`.

core::arch::global_asm!(include_str!("boot.S"));
