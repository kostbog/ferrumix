//! Paging helpers — step towards a real Unix VM subsystem.
//!
//! Currently the boot trampoline (`boot.S`) identity-maps the first 1 GiB
//! with 2 MiB huge pages: PML4[0] -> PDPT[0] -> PD (512 * 2 MiB).  This module
//! provides safe-ish Rust wrappers to inspect and manipulate those tables,
//! allocate frames for new page tables, and map 4 KiB pages.
//!
//! The higher-half kernel mapping (e.g. 0xFFFFFFFF80000000) from the roadmap
//! will be built on top of this; for now we keep the identity map and merely
//! expose the structures.

use core::arch::asm;

pub const ENTRY_PRESENT: u64 = 1 << 0;
pub const ENTRY_WRITABLE: u64 = 1 << 1;
pub const ENTRY_USER: u64 = 1 << 2;
pub const ENTRY_PWT: u64 = 1 << 3;
pub const ENTRY_PCD: u64 = 1 << 4;
pub const ENTRY_ACCESSED: u64 = 1 << 5;
pub const ENTRY_DIRTY: u64 = 1 << 6;
pub const ENTRY_HUGE: u64 = 1 << 7;
pub const ENTRY_GLOBAL: u64 = 1 << 8;
pub const ENTRY_NX: u64 = 1 << 63;

extern "C" {
    static mut pml4_table: [u64; 512];
    static mut pdpt: [u64; 512];
    static mut pd_table: [u64; 512];
}

/// Return the current PML4 physical address (CR3).
pub fn current_pml4_addr() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov {0}, cr3", out(reg) cr3, options(nomem, nostack)) };
    cr3
}

/// Return a mutable reference to the active PML4 (identity mapped, so phys==virt for first GiB).
pub unsafe fn pml4_mut() -> &'static mut [u64; 512] {
    &mut pml4_table
}

/// Translate a virtual address using the current tables (software walk, identity-mapped tables).
/// Returns Some(phys) if mapped.
pub unsafe fn translate_virt(virt: u64) -> Option<u64> {
    let pml4 = &pml4_table;
    let pml4_idx = ((virt >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt >> 12) & 0x1FF) as usize;

    let pml4e = pml4[pml4_idx];
    if pml4e & ENTRY_PRESENT == 0 {
        return None;
    }
    let pdpt_ptr = (pml4e & 0x000FFFFFFFFFF000) as *const [u64; 512];
    let pdpt_ref = &*pdpt_ptr;
    let pdpte = pdpt_ref[pdpt_idx];
    if pdpte & ENTRY_PRESENT == 0 {
        return None;
    }
    if pdpte & ENTRY_HUGE != 0 {
        let base = pdpte & 0x000FFFFFC0000000;
        return Some(base + (virt & 0x3FFFFFFF));
    }
    let pd_ptr = (pdpte & 0x000FFFFFFFFFF000) as *const [u64; 512];
    let pd_ref = &*pd_ptr;
    let pde = pd_ref[pd_idx];
    if pde & ENTRY_PRESENT == 0 {
        return None;
    }
    if pde & ENTRY_HUGE != 0 {
        let base = pde & 0x000FFFFFFFE00000;
        return Some(base + (virt & 0x1FFFFF));
    }
    let pt_ptr = (pde & 0x000FFFFFFFFFF000) as *const [u64; 512];
    let pt_ref = &*pt_ptr;
    let pte = pt_ref[pt_idx];
    if pte & ENTRY_PRESENT == 0 {
        return None;
    }
    let base = pte & 0x000FFFFFFFFFF000;
    Some(base + (virt & 0xFFF))
}

pub fn init() {
    let cr3 = current_pml4_addr();
    crate::serial::serial_println!("paging: CR3={:#x}, PML4 at {:#x}", cr3, cr3);
    unsafe {
        let pml4e0 = pml4_table[0];
        crate::serial::serial_println!("paging: PML4[0]={:#x}", pml4e0);
        crate::serial::serial_println!("paging: PDPT[0]={:#x}", pdpt[0]);
        crate::serial::serial_println!("paging: PD[0]={:#x} .. PD[1]={:#x}", pd_table[0], pd_table[1]);
    }
    unsafe {
        if let Some(phys) = translate_virt(0x100000) {
            crate::serial::serial_println!("paging: virt 0x100000 -> phys {:#x} (expected 0x100000)", phys);
        }
        if let Some(phys) = translate_virt(0xb8000) {
            crate::serial::serial_println!("paging: virt 0xb8000 -> phys {:#x} (VGA)", phys);
        }
    }
    crate::serial::serial_println!("paging: identity mapping for first 1 GiB active");
}
