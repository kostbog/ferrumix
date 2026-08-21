//! Paging and virtual memory.
//!
//! The boot trampoline (`boot.S`) identity-maps the first 1 GiB of physical
//! memory with 2 MiB pages: `PML4[0] -> PDPT[0] -> PD (512 * 2 MiB)`.  Because
//! of that identity window the kernel can reach any page table (they all live
//! in frames below 1 GiB) simply by using the physical address as a pointer.
//!
//! On top of that this module implements the basic paging / virtual memory
//! operations the rest of the kernel needs:
//!
//! * inspecting the tables and translating addresses in software
//!   ([`translate`], [`translate_in`], [`entry_of`], [`dump_walk`])
//! * mapping and unmapping 4 KiB pages ([`map_page`], [`unmap_page`],
//!   [`map_range`], [`map_alloc`], [`unmap_alloc`])
//! * splitting a 2 MiB huge page into a page table when a finer granularity
//!   is required ([`map_page`] does it automatically)
//! * per-process address spaces ([`new_address_space`],
//!   [`destroy_address_space`], [`switch_to`])
//! * TLB maintenance ([`flush`], [`flush_all`]) and the NX bit ([`nx_enabled`])
//!
//! Address space layout used by Ferrumix:
//!
//! ```text
//!   0x0000_0000_0000_0000 .. 0x0000_0000_4000_0000   identity mapped kernel
//!   0x0000_0004_0000_0000                            kernel scratch window
//!   0x0000_0080_0000_0000 ..                         user space (PML4[1])
//! ```

use crate::memory;
use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

/// Size of the pages this kernel maps.
pub const PAGE_SIZE: u64 = 4096;

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

/// Physical address bits of a page table entry.
pub const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
/// Physical address bits of a 2 MiB page directory entry.
pub const ADDR_MASK_2M: u64 = 0x000f_ffff_ffe0_0000;
/// Physical address bits of a 1 GiB page directory pointer entry.
pub const ADDR_MASK_1G: u64 = 0x000f_ffff_c000_0000;

/// Only the first GiB is identity mapped, so only frames below this limit can
/// be dereferenced by the kernel (page tables must live below it).
pub const PHYS_ACCESS_LIMIT: u64 = 1 << 30;

/// A free virtual window the kernel uses for temporary mappings (16 GiB).
pub const KERNEL_SCRATCH_BASE: u64 = 0x0000_0004_0000_0000;

/// Errors returned by the virtual memory operations.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VmError {
    /// The frame allocator is out of physical memory.
    OutOfMemory,
    /// A frame lives outside the identity-mapped window and cannot be touched.
    Unreachable,
    /// A 1 GiB huge page blocks the mapping and cannot be split.
    HugePage,
    /// Nothing is mapped at that address.
    NotMapped,
    /// Something is already mapped at that address.
    AlreadyMapped,
    /// Address or size is not page aligned.
    Misaligned,
}

extern "C" {
    static pml4_table: [u64; 512];
}

/// Raw CR3 value (physical address of the active PML4 plus flags).
pub fn current_pml4_addr() -> u64 {
    let cr3: u64;
    unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) };
    cr3
}

/// Physical address of the active PML4.
pub fn active_root() -> u64 {
    current_pml4_addr() & ADDR_MASK
}

/// Physical address of the PML4 created by the boot trampoline.
pub fn kernel_root() -> u64 {
    unsafe { &pml4_table as *const [u64; 512] as u64 }
}

/// Load a new address space into CR3 (this also flushes the whole TLB).
///
/// # Safety
/// `root` must be the physical address of a valid PML4 that maps the kernel.
pub unsafe fn switch_to(root: u64) {
    asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
}

/// Invalidate a single page in the TLB.
pub fn flush(virt: u64) {
    unsafe { asm!("invlpg [{}]", in(reg) virt, options(nostack, preserves_flags)) };
}

/// Flush the whole TLB by reloading CR3.
pub fn flush_all() {
    let cr3 = current_pml4_addr();
    unsafe { switch_to(cr3) };
}

fn index(virt: u64, level: u32) -> usize {
    ((virt >> (12 + 9 * level)) & 0x1ff) as usize
}

unsafe fn read_entry(table: *mut u64, idx: usize) -> u64 {
    core::ptr::read_volatile(table.add(idx))
}

unsafe fn write_entry(table: *mut u64, idx: usize, value: u64) {
    core::ptr::write_volatile(table.add(idx), value);
}

/// Allocate a zeroed frame usable as a page table.
fn alloc_table() -> Result<u64, VmError> {
    let frame = memory::alloc_frame().ok_or(VmError::OutOfMemory)?;
    if frame >= PHYS_ACCESS_LIMIT {
        memory::free_frame(frame);
        return Err(VmError::Unreachable);
    }
    unsafe { zero_frame(frame) };
    Ok(frame)
}

/// Fill a physical frame with zeroes through the identity window.
///
/// # Safety
/// `phys` must be a free 4 KiB frame below [`PHYS_ACCESS_LIMIT`].
pub unsafe fn zero_frame(phys: u64) {
    let words = phys as *mut u64;
    for i in 0..(PAGE_SIZE as usize / 8) {
        core::ptr::write_volatile(words.add(i), 0);
    }
}

/// Follow (and optionally create) the table referenced by `table[idx]`.
unsafe fn next_table(
    table: *mut u64,
    idx: usize,
    parent_flags: u64,
    create: bool,
) -> Result<*mut u64, VmError> {
    let entry = read_entry(table, idx);
    if entry & ENTRY_PRESENT != 0 {
        if entry & ENTRY_HUGE != 0 {
            return Err(VmError::HugePage);
        }
        // Widen the permissions of the parent entry if the new mapping needs
        // more than the existing one (e.g. a user page below a kernel table).
        let extra = parent_flags & (ENTRY_WRITABLE | ENTRY_USER);
        if entry & extra != extra {
            write_entry(table, idx, entry | extra);
        }
        let phys = entry & ADDR_MASK;
        if phys >= PHYS_ACCESS_LIMIT {
            return Err(VmError::Unreachable);
        }
        return Ok(phys as *mut u64);
    }
    if !create {
        return Err(VmError::NotMapped);
    }
    let frame = alloc_table()?;
    let flags = ENTRY_PRESENT | ENTRY_WRITABLE | (parent_flags & ENTRY_USER);
    write_entry(table, idx, frame | flags);
    Ok(frame as *mut u64)
}

/// Replace a 2 MiB page directory entry with a full page table describing the
/// same 512 4 KiB pages, so that individual pages can be re-mapped.
unsafe fn split_huge_2mib(pd: *mut u64, idx: usize) -> Result<(), VmError> {
    let entry = read_entry(pd, idx);
    let base = entry & ADDR_MASK_2M;
    let flags = entry & (ENTRY_PRESENT | ENTRY_WRITABLE | ENTRY_USER | ENTRY_GLOBAL | ENTRY_NX);
    let table = alloc_table()?;
    let pt = table as *mut u64;
    for i in 0..512u64 {
        write_entry(pt, i as usize, (base + i * PAGE_SIZE) | flags);
    }
    let parent = ENTRY_PRESENT | ENTRY_WRITABLE | (entry & ENTRY_USER);
    write_entry(pd, idx, table | parent);
    flush_all();
    Ok(())
}

/// Map one 4 KiB page in the address space rooted at `root`.
///
/// # Safety
/// `root` must be a valid PML4 reachable through the identity window.
pub unsafe fn map_page(root: u64, virt: u64, phys: u64, flags: u64) -> Result<(), VmError> {
    if virt % PAGE_SIZE != 0 || phys % PAGE_SIZE != 0 {
        return Err(VmError::Misaligned);
    }
    let pml4 = root as *mut u64;
    let pdpt = next_table(pml4, index(virt, 3), flags, true)?;
    let pd = next_table(pdpt, index(virt, 2), flags, true)?;

    // A 2 MiB page in the way is split into 512 small pages.
    let pd_idx = index(virt, 1);
    let pde = read_entry(pd, pd_idx);
    if pde & ENTRY_PRESENT != 0 && pde & ENTRY_HUGE != 0 {
        split_huge_2mib(pd, pd_idx)?;
    }

    let pt = next_table(pd, pd_idx, flags, true)?;
    let pt_idx = index(virt, 0);
    if read_entry(pt, pt_idx) & ENTRY_PRESENT != 0 {
        return Err(VmError::AlreadyMapped);
    }
    write_entry(pt, pt_idx, (phys & ADDR_MASK) | flags | ENTRY_PRESENT);
    flush(virt);
    Ok(())
}

/// Remove a 4 KiB mapping and return the physical frame it pointed at.
///
/// # Safety
/// `root` must be a valid PML4 reachable through the identity window.
pub unsafe fn unmap_page(root: u64, virt: u64) -> Result<u64, VmError> {
    if virt % PAGE_SIZE != 0 {
        return Err(VmError::Misaligned);
    }
    let pml4 = root as *mut u64;
    let pdpt = next_table(pml4, index(virt, 3), 0, false)?;
    let pd = next_table(pdpt, index(virt, 2), 0, false)?;
    let pt = next_table(pd, index(virt, 1), 0, false)?;
    let pt_idx = index(virt, 0);
    let entry = read_entry(pt, pt_idx);
    if entry & ENTRY_PRESENT == 0 {
        return Err(VmError::NotMapped);
    }
    write_entry(pt, pt_idx, 0);
    flush(virt);
    Ok(entry & ADDR_MASK)
}

/// Map `pages` consecutive 4 KiB pages of physical memory.
///
/// # Safety
/// See [`map_page`].
pub unsafe fn map_range(
    root: u64,
    virt: u64,
    phys: u64,
    pages: u64,
    flags: u64,
) -> Result<(), VmError> {
    for i in 0..pages {
        map_page(root, virt + i * PAGE_SIZE, phys + i * PAGE_SIZE, flags)?;
    }
    Ok(())
}

/// Allocate `pages` fresh zeroed frames and map them at `virt`.
///
/// Already mapped pages inside the range are left untouched, which makes it
/// safe to call for overlapping ELF segments.
///
/// # Safety
/// See [`map_page`].
pub unsafe fn map_alloc(root: u64, virt: u64, pages: u64, flags: u64) -> Result<u64, VmError> {
    let mut mapped = 0;
    for i in 0..pages {
        let page = virt + i * PAGE_SIZE;
        if translate_in(root, page).is_some() {
            continue;
        }
        let frame = memory::alloc_frame().ok_or(VmError::OutOfMemory)?;
        if frame >= PHYS_ACCESS_LIMIT {
            memory::free_frame(frame);
            return Err(VmError::Unreachable);
        }
        zero_frame(frame);
        match map_page(root, page, frame, flags) {
            Ok(()) => mapped += 1,
            Err(err) => {
                memory::free_frame(frame);
                return Err(err);
            }
        }
    }
    Ok(mapped)
}

/// Unmap `pages` pages and return their frames to the allocator.
///
/// # Safety
/// The pages must have been mapped by [`map_alloc`].
pub unsafe fn unmap_alloc(root: u64, virt: u64, pages: u64) -> u64 {
    let mut freed = 0;
    for i in 0..pages {
        if let Ok(frame) = unmap_page(root, virt + i * PAGE_SIZE) {
            memory::free_frame(frame);
            freed += 1;
        }
    }
    freed
}

/// Translate a virtual address in the address space rooted at `root`.
///
/// # Safety
/// `root` must be a valid PML4 reachable through the identity window.
pub unsafe fn translate_in(root: u64, virt: u64) -> Option<u64> {
    entry_in(root, virt).map(|(phys, _)| phys)
}

/// Like [`translate_in`], but also returns the flags of the leaf entry.
///
/// # Safety
/// `root` must be a valid PML4 reachable through the identity window.
pub unsafe fn entry_in(root: u64, virt: u64) -> Option<(u64, u64)> {
    let pml4 = root as *mut u64;
    let pml4e = read_entry(pml4, index(virt, 3));
    if pml4e & ENTRY_PRESENT == 0 {
        return None;
    }
    let pdpt = (pml4e & ADDR_MASK) as *mut u64;
    let pdpte = read_entry(pdpt, index(virt, 2));
    if pdpte & ENTRY_PRESENT == 0 {
        return None;
    }
    if pdpte & ENTRY_HUGE != 0 {
        let phys = (pdpte & ADDR_MASK_1G) + (virt & 0x3fff_ffff);
        return Some((phys, pdpte & !ADDR_MASK_1G));
    }
    let pd = (pdpte & ADDR_MASK) as *mut u64;
    let pde = read_entry(pd, index(virt, 1));
    if pde & ENTRY_PRESENT == 0 {
        return None;
    }
    if pde & ENTRY_HUGE != 0 {
        let phys = (pde & ADDR_MASK_2M) + (virt & 0x1f_ffff);
        return Some((phys, pde & !ADDR_MASK_2M));
    }
    let pt = (pde & ADDR_MASK) as *mut u64;
    let pte = read_entry(pt, index(virt, 0));
    if pte & ENTRY_PRESENT == 0 {
        return None;
    }
    Some(((pte & ADDR_MASK) + (virt & 0xfff), pte & !ADDR_MASK))
}

/// Translate a virtual address in the currently active address space.
///
/// # Safety
/// The active page tables must be reachable through the identity window.
pub unsafe fn translate_virt(virt: u64) -> Option<u64> {
    translate_in(active_root(), virt)
}

/// Translate an address in the active address space (safe wrapper).
pub fn translate(virt: u64) -> Option<u64> {
    unsafe { translate_in(active_root(), virt) }
}

/// Leaf entry (physical address + flags) in the active address space.
pub fn entry_of(virt: u64) -> Option<(u64, u64)> {
    unsafe { entry_in(active_root(), virt) }
}

/// Render page-table flags as a short `pwux4` style string.
pub fn flags_string(flags: u64, out: &mut [u8; 8]) -> usize {
    out[0] = if flags & ENTRY_PRESENT != 0 { b'p' } else { b'-' };
    out[1] = if flags & ENTRY_WRITABLE != 0 { b'w' } else { b'r' };
    out[2] = if flags & ENTRY_USER != 0 { b'u' } else { b'k' };
    out[3] = if flags & ENTRY_NX != 0 { b'-' } else { b'x' };
    out[4] = if flags & ENTRY_HUGE != 0 { b'H' } else { b'4' };
    5
}

/// Print every level of the page-table walk for `virt` (shell: `pagemap`).
pub fn dump_walk(root: u64, virt: u64) {
    let names = ["PML4", "PDPT", "PD  ", "PT  "];
    let mut table = root as *mut u64;
    for level in 0..4 {
        let idx = index(virt, 3 - level as u32);
        let entry = unsafe { read_entry(table, idx) };
        let mut buf = [0u8; 8];
        let len = flags_string(entry, &mut buf);
        let flags = core::str::from_utf8(&buf[..len]).unwrap_or("");
        crate::println!("  {}[{:3}] = {:#018x}  {}", names[level], idx, entry, flags);
        if entry & ENTRY_PRESENT == 0 {
            crate::println!("  (not present — walk stops here)");
            return;
        }
        if entry & ENTRY_HUGE != 0 {
            crate::println!("  (huge page — walk stops here)");
            return;
        }
        table = (entry & ADDR_MASK) as *mut u64;
    }
    match unsafe { translate_in(root, virt) } {
        Some(phys) => crate::println!("  {:#x} -> {:#x}", virt, phys),
        None => crate::println!("  {:#x} is not mapped", virt),
    }
}

/// Create a fresh address space that shares the kernel mappings.
///
/// The new PML4 gets a copy of every kernel entry (entry 0, the identity
/// window, plus the higher half) while the user slots stay empty, so each
/// process gets a private user address space.
pub fn new_address_space() -> Result<u64, VmError> {
    let frame = alloc_table()?;
    unsafe {
        let dst = frame as *mut u64;
        let src = active_root() as *mut u64;
        write_entry(dst, 0, read_entry(src, 0));
        for idx in 256..512 {
            write_entry(dst, idx, read_entry(src, idx));
        }
    }
    Ok(frame)
}

/// Free every user page and page table of an address space, then the PML4.
///
/// # Safety
/// `root` must not be the active address space.
pub unsafe fn destroy_address_space(root: u64) -> u64 {
    let mut freed = 0;
    let pml4 = root as *mut u64;
    for i4 in 1..256 {
        let pml4e = read_entry(pml4, i4);
        if pml4e & ENTRY_PRESENT == 0 {
            continue;
        }
        let pdpt = (pml4e & ADDR_MASK) as *mut u64;
        for i3 in 0..512 {
            let pdpte = read_entry(pdpt, i3);
            if pdpte & ENTRY_PRESENT == 0 || pdpte & ENTRY_HUGE != 0 {
                continue;
            }
            let pd = (pdpte & ADDR_MASK) as *mut u64;
            for i2 in 0..512 {
                let pde = read_entry(pd, i2);
                if pde & ENTRY_PRESENT == 0 || pde & ENTRY_HUGE != 0 {
                    continue;
                }
                let pt = (pde & ADDR_MASK) as *mut u64;
                for i1 in 0..512 {
                    let pte = read_entry(pt, i1);
                    if pte & ENTRY_PRESENT != 0 {
                        memory::free_frame(pte & ADDR_MASK);
                        freed += 1;
                    }
                }
                memory::free_frame(pde & ADDR_MASK);
            }
            memory::free_frame(pdpte & ADDR_MASK);
        }
        memory::free_frame(pml4e & ADDR_MASK);
        write_entry(pml4, i4, 0);
    }
    memory::free_frame(root);
    freed
}

static NX_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the CPU supports NX and `EFER.NXE` has been enabled.
pub fn nx_enabled() -> bool {
    NX_ENABLED.load(Ordering::Relaxed)
}

/// Enable the no-execute bit (EFER.NXE) if the CPU advertises it.
fn enable_nx() -> bool {
    let supported = unsafe { core::arch::x86_64::__cpuid(0x8000_0001).edx & (1 << 20) != 0 };
    if !supported {
        return false;
    }
    unsafe {
        let (low, high): (u32, u32);
        asm!("rdmsr", in("ecx") 0xc000_0080u32, out("eax") low, out("edx") high,
             options(nomem, nostack, preserves_flags));
        let value = low | (1 << 11);
        asm!("wrmsr", in("ecx") 0xc000_0080u32, in("eax") value, in("edx") high,
             options(nomem, nostack, preserves_flags));
    }
    NX_ENABLED.store(true, Ordering::Relaxed);
    true
}

/// Map a page, write a marker, read it back and unmap it again — a quick
/// self-test that the mapping code really works on the running machine.
fn self_test() -> bool {
    let root = active_root();
    let frame = match memory::alloc_frame() {
        Some(frame) => frame,
        None => return false,
    };
    let virt = KERNEL_SCRATCH_BASE;
    let flags = ENTRY_PRESENT | ENTRY_WRITABLE;
    let result = unsafe { map_page(root, virt, frame, flags) };
    if let Err(err) = result {
        crate::println!("paging: self-test map failed: {:?}", err);
        memory::free_frame(frame);
        return false;
    }

    let magic: u64 = 0xfe22_0000_c0ff_ee42;
    unsafe { core::ptr::write_volatile(virt as *mut u64, magic) };
    let read_back = unsafe { core::ptr::read_volatile(virt as *mut u64) };
    let seen_by_phys = unsafe { core::ptr::read_volatile(frame as *mut u64) };
    let translated = unsafe { translate_in(root, virt) };

    let unmapped = unsafe { unmap_page(root, virt) };
    memory::free_frame(frame);

    let ok = read_back == magic
        && seen_by_phys == magic
        && translated == Some(frame)
        && unmapped == Ok(frame)
        && unsafe { translate_in(root, virt) }.is_none();
    if ok {
        crate::println!(
            "paging: self-test OK — mapped {:#x} -> {:#x}, wrote/read {:#x}, unmapped",
            virt,
            frame,
            magic
        );
    } else {
        crate::println!("paging: self-test FAILED");
    }
    ok
}

/// Bring up the virtual memory subsystem and report what the tables look like.
pub fn init() {
    let cr3 = current_pml4_addr();
    crate::println!(
        "paging: CR3={:#x}, PML4 @ {:#x} (4 KiB pages)",
        cr3,
        active_root()
    );
    unsafe {
        let pml4 = active_root() as *mut u64;
        let pml4e = read_entry(pml4, 0);
        let pdpt = (pml4e & ADDR_MASK) as *mut u64;
        let pdpte = read_entry(pdpt, 0);
        let pd = (pdpte & ADDR_MASK) as *mut u64;
        crate::serial_println!(
            "paging: PML4[0]={:#x} PDPT[0]={:#x} PD[0]={:#x} PD[1]={:#x}",
            pml4e,
            pdpte,
            read_entry(pd, 0),
            read_entry(pd, 1)
        );
        if let Some(phys) = translate_in(active_root(), 0xb8000) {
            crate::serial_println!("paging: virt 0xb8000 -> phys {:#x} (VGA)", phys);
        }
    }
    crate::println!("paging: identity map of the first 1 GiB active (2 MiB pages)");
    if enable_nx() {
        crate::println!("paging: NX bit enabled (EFER.NXE) — user data pages are non-executable");
    } else {
        crate::println!("paging: NX not available, data pages stay executable");
    }
    self_test();
}
