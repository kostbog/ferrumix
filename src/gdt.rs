//! Global Descriptor Table + Task State Segment for long mode.
//!
//! We keep the flat 64-bit code/data segments set up by the boot trampoline,
//! but reload a richer GDT that also contains a TSS. The TSS carries an
//! Interrupt Stack Table (IST) entry used by the double-fault handler so it
//! always has a valid stack even if the normal kernel stack is corrupted.

use core::arch::asm;

#[repr(C, packed)]
struct GdtDescriptor {
    limit: u16,
    base: u64,
}

#[repr(C, packed)]
struct Tss {
    reserved1: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved2: u32,
    reserved3: u32,
    ist: [u64; 7],
    reserved4: u32,
    reserved5: u32,
    iomap_base: u16,
    reserved6: u16, // pads the TSS to 104 bytes (minimum for 64-bit TSS)
}

// null | code | data | TSS(low) | TSS(high)  -> 5 slots.
static mut GDT: [u64; 5] = [0; 5];
static mut TSS: Tss = Tss {
    reserved1: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    reserved2: 0,
    reserved3: 0,
    ist: [0; 7],
    reserved4: 0,
    reserved5: 0,
    iomap_base: 0,
    reserved6: 0,
};
static mut IST_STACK: [u8; 8192] = [0; 8192];

pub fn init() {
    unsafe {
        GDT[0] = 0;
        GDT[1] = 0x00209A0000000000; // 64-bit code: present, ring0, exec
        GDT[2] = 0x0000920000000000; // 64-bit data: present, ring0, rw

        let tss_addr = &TSS as *const Tss as u64;
        let tss_limit = (core::mem::size_of::<Tss>() - 1) as u64;

        let low: u64 = (tss_limit & 0xFFFF)
            | ((tss_addr & 0xFFFF) << 16)
            | (0x89u64 << 40) // present, DPL0, type=9 (available 64-bit TSS)
            | (((tss_limit >> 16) & 0xF) << 48);
        let high: u64 = (tss_addr >> 32) & 0xFFFFFFFF;

        GDT[3] = low;
        GDT[4] = high;

        // IST#0 stack (also used as the ring0 stack fallback via rsp0).
        let ist_top = (&IST_STACK as *const u8 as u64) + IST_STACK.len() as u64;
        TSS.ist[0] = ist_top;
        TSS.rsp0 = ist_top;

        let desc = GdtDescriptor {
            limit: (core::mem::size_of::<[u64; 5]>() - 1) as u16,
            base: &GDT as *const _ as u64,
        };
        asm!("lgdt ({0})", in(reg) &desc, options(nostack, preserves_flags));
        asm!(
            "mov ds, ax; mov es, ax; mov ss, ax",
            in("ax") 0x10u16,
            options(nostack)
        );
        asm!("ltr {0:x}", in(reg) 0x18u16, options(nostack));
    }
}
