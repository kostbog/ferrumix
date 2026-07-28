//! Global Descriptor Table + Task State Segment for long mode.
//!
//! We keep the flat 64-bit code/data segments set up by the boot trampoline,
//! but reload a richer GDT that also contains:
//!   - user code/data (ring 3) at DPL 3 — first step toward real Unix processes
//!   - TSS with IST stack for double-fault and RSP0 for ring3->ring0 transitions.
//!
//! Layout:
//!   0 null
//!   1 kernel code (0x08)
//!   2 kernel data (0x10)
//!   3 user data   (0x18) DPL3
//!   4 user code   (0x20) DPL3
//!   5 TSS low     (0x28)
//!   6 TSS high    (0x30) — second half of 16-byte TSS descriptor

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
    reserved6: u16,
}

static mut GDT: [u64; 7] = [0; 7];
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
static mut RING0_STACK: [u8; 16384] = [0; 16384];

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18;
pub const USER_CODE_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

pub fn init() {
    unsafe {
        GDT[0] = 0;
        GDT[1] = 0x00209A0000000000;
        GDT[2] = 0x0000920000000000;
        GDT[3] = 0x0000F20000000000;
        GDT[4] = 0x0020FA0000000000;

        let tss_addr = &TSS as *const Tss as u64;
        let tss_limit = (core::mem::size_of::<Tss>() - 1) as u64;

        let low: u64 = (tss_limit & 0xFFFF)
            | ((tss_addr & 0xFFFF) << 16)
            | (0x89u64 << 40)
            | (((tss_limit >> 16) & 0xF) << 48);
        let high: u64 = (tss_addr >> 32) & 0xFFFFFFFF;

        GDT[5] = low;
        GDT[6] = high;

        let ist_top = (&IST_STACK as *const u8 as u64) + IST_STACK.len() as u64;
        let r0_top = (&RING0_STACK as *const u8 as u64) + RING0_STACK.len() as u64;
        TSS.ist[0] = ist_top;
        TSS.rsp0 = r0_top;

        let desc = GdtDescriptor {
            limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
            base: &GDT as *const _ as u64,
        };
        asm!("lgdt ({0})", in(reg) &desc, options(nostack, preserves_flags));
        asm!(
            "mov ds, ax; mov es, ax; mov ss, ax",
            in("ax") KERNEL_DATA_SELECTOR,
            options(nostack)
        );
        asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack));
    }
    crate::serial::serial_println!(
        "GDT: kernel code={:#x} data={:#x}, user code={:#x} data={:#x}, TSS={:#x}",
        KERNEL_CODE_SELECTOR,
        KERNEL_DATA_SELECTOR,
        USER_CODE_SELECTOR,
        USER_DATA_SELECTOR,
        TSS_SELECTOR
    );
}

pub fn set_kernel_stack(rsp0: u64) {
    unsafe {
        TSS.rsp0 = rsp0;
    }
}

pub fn tss_rsp0() -> u64 {
    unsafe { TSS.rsp0 }
}
