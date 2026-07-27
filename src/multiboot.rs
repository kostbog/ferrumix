//! Minimal Multiboot2 information-structure parser (memory map only, so far).
//!
//! The structure is provided by the bootloader at the physical address passed
//! in `EBX` at entry. Because we identity-map the low 1 GiB, that address is
//! also a valid virtual address here.

pub struct Info {
    pub usable_memory: u64,
}

const TAG_END: u32 = 0;
const TAG_MMAP: u32 = 6;

#[repr(C)]
struct TagHeader {
    ty: u32,
    size: u32,
}

#[repr(C)]
struct MmapEntry {
    base: u64,
    length: u64,
    ty: u32,
    _reserved: u32,
}

/// Parse the Multiboot2 tags starting at the given address.
///
/// # Safety
/// `addr` must be a valid pointer to a Multiboot2 information structure. The
/// caller (the boot trampoline) guarantees this.
pub unsafe fn parse(addr: usize) -> Info {
    let mut usable: u64 = 0;
    let mut p = (addr + 8) as *const u8; // skip total_size + reserved

    loop {
        let header = p as *const TagHeader;
        let ty = (*header).ty;
        let size = (*header).size as usize;

        if ty == TAG_END {
            break;
        }

        if ty == TAG_MMAP {
            let esz = *((p as *const u32).add(2)) as usize; // entry_size at p+8
            let mut e = p.add(16); // skip header(8) + entry_size(4) + version(4)
            let end = p.add(size);
            while e.add(esz) <= end {
                let ent = e as *const MmapEntry;
                if (*ent).ty == 1 {
                    // type 1 == available RAM
                    usable += (*ent).length;
                }
                e = e.add(esz);
            }
        }

        // Tags are 8-byte aligned. Advance to the next one.
        p = p.add((size + 7) & !7);
    }

    Info { usable_memory: usable }
}
