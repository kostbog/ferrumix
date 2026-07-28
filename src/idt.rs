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

    pub fn set_handler(&mut self, handler: u64) {
        self.gdt_selector = 0x08;
        self.ist = 0;
        self.type_attr = 0x8E;
        self.pointer_low = handler as u16;
        self.pointer_mid = (handler >> 16) as u16;
        self.pointer_high = (handler >> 32) as u32;
    }

    pub fn set_handler_with_dpl(&mut self, handler: u64, dpl: u8) {
        let type_attr = 0x80 | ((dpl & 0x3) << 5) | 0x0E;
        self.gdt_selector = 0x08;
        self.ist = 0;
        self.type_attr = type_attr;
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

pub static mut IDT: [Entry; 256] = [Entry::missing(); 256];

pub unsafe fn load() {
    let desc = Descriptor {
        limit: (core::mem::size_of::<[Entry; 256]>() - 1) as u16,
        base: &IDT as *const _ as u64,
    };
    asm!("lidt ({0})", in(reg) &desc, options(nostack, preserves_flags));
}
