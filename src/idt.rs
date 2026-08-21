//! Interrupt Descriptor Table (64-bit gate descriptors) and the `lidt` loader.

use core::arch::asm;

#[derive(Clone, Copy)]
#[repr(C, packed)]
pub struct Entry {
    pub pointer_low: u16,
    pub gdt_selector: u16,
    pub ist: u8,
    pub type_attr: u8,
    pub pointer_mid: u16,
    pub pointer_high: u32,
    pub reserved: u32,
}

impl Entry {
    pub const fn missing() -> Self {
        Entry {
            pointer_low: 0,
            gdt_selector: 0,
            ist: 0,
            type_attr: 0,
            pointer_mid: 0,
            pointer_high: 0,
            reserved: 0,
        }
    }

    /// Install a ring-0 interrupt gate.
    pub fn set_handler(&mut self, handler: u64) {
        self.set_handler_with_dpl(handler, 0);
    }

    /// Install an interrupt gate callable from privilege level `dpl`.
    pub fn set_handler_with_dpl(&mut self, handler: u64, dpl: u8) {
        self.gdt_selector = crate::gdt::KERNEL_CODE_SELECTOR;
        self.type_attr = 0x80 | ((dpl & 0x3) << 5) | 0x0e;
        self.pointer_low = handler as u16;
        self.pointer_mid = (handler >> 16) as u16;
        self.pointer_high = (handler >> 32) as u32;
    }
}

#[repr(C, packed)]
pub struct Descriptor {
    pub limit: u16,
    pub base: u64,
}

static mut IDT: [Entry; 256] = [Entry::missing(); 256];

/// Mutable access to one IDT entry.
///
/// # Safety
/// The caller must not race with interrupt delivery on the same vector.
pub unsafe fn entry(vector: usize) -> &'static mut Entry {
    let table = core::ptr::addr_of_mut!(IDT);
    &mut (*table)[vector]
}

/// Load the IDT into the CPU.
///
/// # Safety
/// Every entry that can be triggered must have been initialised.
pub unsafe fn load() {
    let table = core::ptr::addr_of!(IDT);
    let descriptor = Descriptor {
        limit: (core::mem::size_of::<[Entry; 256]>() - 1) as u16,
        base: table as u64,
    };
    asm!("lidt [{}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
}
