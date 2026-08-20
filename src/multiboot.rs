//! Multiboot information parser (memory map only, so far).
//!
//! Ferrumix advertises both Multiboot 2 and a small Multiboot 1 compatibility
//! header. GRUB can use the Multiboot 2 path, while QEMU's direct `-kernel`
//! loader currently recognises the Multiboot 1 header. Both formats are
//! normalised into the same `Info` structure.

#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    pub base: u64,
    pub len: u64,
    pub ty: u32,
}

pub struct Info {
    pub usable_memory: u64,
    pub regions: [MemoryRegion; 32],
    pub region_count: usize,
}

pub const MULTIBOOT1_MAGIC: u32 = 0x2BAD_B002;
pub const MULTIBOOT2_MAGIC: u32 = 0x36D7_6289;

const TAG_END: u32 = 0;
const TAG_MMAP: u32 = 6;
const EMPTY_REGION: MemoryRegion = MemoryRegion {
    base: 0,
    len: 0,
    ty: 0,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct TagHeader {
    ty: u32,
    size: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MmapEntry {
    base: u64,
    length: u64,
    ty: u32,
    _reserved: u32,
}

/// Parse the bootloader information structure for the supplied boot magic.
///
/// # Safety
/// `addr` must point to the information structure identified by `magic`. The
/// boot trampoline passes both values through without modification.
pub unsafe fn parse(addr: usize, magic: u32) -> Info {
    match magic {
        MULTIBOOT1_MAGIC => parse_multiboot1(addr),
        MULTIBOOT2_MAGIC => parse_multiboot2(addr),
        _ => Info::empty(),
    }
}

unsafe fn parse_multiboot2(addr: usize) -> Info {
    let mut info = Info::empty();
    let total_size = core::ptr::read_unaligned(addr as *const u32) as usize;
    let end_addr = addr.saturating_add(total_size);
    let mut p = addr.saturating_add(8);

    while p.saturating_add(core::mem::size_of::<TagHeader>()) <= end_addr {
        let header = core::ptr::read_unaligned(p as *const TagHeader);
        if header.ty == TAG_END || header.size < 8 {
            break;
        }

        let tag_end = match p.checked_add(header.size as usize) {
            Some(end) if end <= end_addr => end,
            _ => break,
        };
        if header.ty == TAG_MMAP && header.size >= 16 {
            let entry_size = core::ptr::read_unaligned((p + 8) as *const u32) as usize;
            if entry_size >= core::mem::size_of::<MmapEntry>() {
                let mut entry_addr = p + 16;
                while entry_addr.saturating_add(entry_size) <= tag_end {
                    let entry = core::ptr::read_unaligned(entry_addr as *const MmapEntry);
                    info.add_region(entry.base, entry.length, entry.ty);
                    entry_addr += entry_size;
                }
            }
        }

        p = match p.checked_add((header.size as usize + 7) & !7) {
            Some(next) => next,
            None => break,
        };
    }
    info
}

unsafe fn parse_multiboot1(addr: usize) -> Info {
    let mut info = Info::empty();
    let flags = core::ptr::read_unaligned(addr as *const u32);
    // Bit 6 means mmap_length and mmap_addr are present.
    if flags & (1 << 6) == 0 {
        return info;
    }

    let mmap_len = core::ptr::read_unaligned((addr + 44) as *const u32) as usize;
    let mmap_addr = core::ptr::read_unaligned((addr + 48) as *const u32) as usize;
    let mmap_end = mmap_addr.saturating_add(mmap_len);
    let mut p = mmap_addr;

    while p.saturating_add(4) <= mmap_end {
        let size = core::ptr::read_unaligned(p as *const u32) as usize;
        if size < 20 || p.saturating_add(size + 4) > mmap_end {
            break;
        }
        let base_low = core::ptr::read_unaligned((p + 4) as *const u32) as u64;
        let base_high = core::ptr::read_unaligned((p + 8) as *const u32) as u64;
        let len_low = core::ptr::read_unaligned((p + 12) as *const u32) as u64;
        let len_high = core::ptr::read_unaligned((p + 16) as *const u32) as u64;
        let ty = core::ptr::read_unaligned((p + 20) as *const u32);
        info.add_region(base_low | (base_high << 32), len_low | (len_high << 32), ty);
        p += size + 4;
    }
    info
}

impl Info {
    const fn empty() -> Self {
        Info {
            usable_memory: 0,
            regions: [EMPTY_REGION; 32],
            region_count: 0,
        }
    }

    fn add_region(&mut self, base: u64, len: u64, ty: u32) {
        if ty == 1 {
            self.usable_memory = self.usable_memory.saturating_add(len);
        }
        if self.region_count < self.regions.len() {
            self.regions[self.region_count] = MemoryRegion { base, len, ty };
            self.region_count += 1;
        }
    }

    /// Return the populated part of the fixed-size region array.
    pub fn regions_slice(&self) -> &[MemoryRegion] {
        &self.regions[..self.region_count]
    }
}
