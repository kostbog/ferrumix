//! Global Descriptor Table + Task State Segment for long mode.
//!
//! Besides the flat 64-bit kernel segments the GDT contains ring-3 code and
//! data descriptors and a TSS.  Two fields of the TSS matter for user mode:
//!
//! * `rsp0` — the kernel stack the CPU switches to when a ring-3 thread traps
//!   into the kernel (system calls, IRQs, faults),
//! * `ist[0]` — a known-good stack for the double fault handler.
//!
//! Layout:
//!
//! ```text
//!   0x00 null
//!   0x08 kernel code (ring 0)
//!   0x10 kernel data (ring 0)
//!   0x18 user data   (ring 3)
//!   0x20 user code   (ring 3)
//!   0x28 TSS (16 byte descriptor, occupies slots 5 and 6)
//! ```

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
static mut RING0_STACK: [u8; 32768] = [0; 32768];

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18;
pub const USER_CODE_SELECTOR: u16 = 0x20;
pub const TSS_SELECTOR: u16 = 0x28;

/// Selector values as loaded when entering ring 3 (RPL = 3).
pub const USER_CODE_SELECTOR_RPL3: u16 = USER_CODE_SELECTOR | 3;
pub const USER_DATA_SELECTOR_RPL3: u16 = USER_DATA_SELECTOR | 3;

pub fn init() {
    unsafe {
        let gdt = core::ptr::addr_of_mut!(GDT);
        (*gdt)[0] = 0;
        (*gdt)[1] = 0x0020_9a00_0000_0000; // kernel code
        (*gdt)[2] = 0x0000_9200_0000_0000; // kernel data
        (*gdt)[3] = 0x0000_f200_0000_0000; // user data   (DPL 3)
        (*gdt)[4] = 0x0020_fa00_0000_0000; // user code   (DPL 3)

        let tss = core::ptr::addr_of_mut!(TSS);
        let tss_addr = tss as u64;
        let tss_limit = (core::mem::size_of::<Tss>() - 1) as u64;
        let low: u64 = (tss_limit & 0xffff)
            | ((tss_addr & 0xffff) << 16)
            | (((tss_addr >> 16) & 0xff) << 32)
            | (0x89u64 << 40)
            | (((tss_limit >> 16) & 0xf) << 48)
            | (((tss_addr >> 24) & 0xff) << 56);
        let high: u64 = (tss_addr >> 32) & 0xffff_ffff;
        (*gdt)[5] = low;
        (*gdt)[6] = high;

        let ist_top = core::ptr::addr_of!(IST_STACK) as u64 + 8192;
        (*tss).ist[0] = ist_top;
        (*tss).rsp0 = kernel_stack_top();

        let descriptor = GdtDescriptor {
            limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
            base: gdt as u64,
        };
        asm!("lgdt [{}]", in(reg) &descriptor, options(readonly, nostack, preserves_flags));
        asm!(
            "mov ds, ax",
            "mov es, ax",
            "mov ss, ax",
            in("ax") KERNEL_DATA_SELECTOR,
            options(nostack, preserves_flags)
        );
        asm!("ltr {0:x}", in(reg) TSS_SELECTOR, options(nostack, preserves_flags));
    }
    crate::serial_println!(
        "GDT: kernel {:#x}/{:#x}, user {:#x}/{:#x} (DPL3), TSS {:#x} rsp0={:#x}",
        KERNEL_CODE_SELECTOR,
        KERNEL_DATA_SELECTOR,
        USER_CODE_SELECTOR,
        USER_DATA_SELECTOR,
        TSS_SELECTOR,
        tss_rsp0()
    );
}

/// Top of the stack the CPU switches to on a ring3 -> ring0 transition.
pub fn kernel_stack_top() -> u64 {
    unsafe { core::ptr::addr_of!(RING0_STACK) as u64 + 32768 }
}

/// Point `TSS.rsp0` at a different kernel stack (used when scheduling).
pub fn set_kernel_stack(rsp0: u64) {
    unsafe {
        let tss = core::ptr::addr_of_mut!(TSS);
        (*tss).rsp0 = rsp0;
    }
}

/// Current `TSS.rsp0`.
pub fn tss_rsp0() -> u64 {
    unsafe {
        let tss = core::ptr::addr_of!(TSS);
        (*tss).rsp0
    }
}
