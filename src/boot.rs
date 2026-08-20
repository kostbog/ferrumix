//! Boot trampoline (Multiboot2 entry + long-mode switch).
//!
//! `build.rs` assembles `boot.S` as a mixed `.code32`/`.code64` object and
//! passes it to the linker. Keeping the trampoline in a separate assembler
//! input is required because Rust's x86_64 `global_asm!` rejects instructions
//! that execute before long mode even when they are inside a `.code32` block.
