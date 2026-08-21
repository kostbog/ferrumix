//! Multiboot information parsing (memory map), for both boot protocols.
//!
//! Ferrumix can be started by QEMU's `-kernel` (the PVH protocol, because
//! QEMU refuses Multiboot for 64-bit ELF images) or by GRUB (Multiboot2), and
//! every protocol hands over its information in a different structure.
//! [`parse`] dispatches on the boot magic — `EAX` for Multiboot, the magic of
//! the structure itself for PVH — and normalises all of them into one
//! [`Info`].
//!
//! The structure lives in low physical memory, which the boot trampoline
//! identity maps, so the physical address passed in `EBX` is also a valid
//! virtual address here.

/// Magic in EAX when a Multiboot1 loader (QEMU `-kernel`) starts us.
pub const MULTIBOOT1_MAGIC: u32 = 0x2bad_b002;
/// Magic in EAX when a Multiboot2 loader (GRUB) starts us.
pub const MULTIBOOT2_MAGIC: u32 = 0x36d7_6289;
/// Magic at the start of the `hvm_start_info` structure of the PVH protocol.
pub const PVH_MAGIC: u32 = 0x336e_c578;

/// One entry of the firmware memory map.
#[derive(Clone, Copy, Debug)]
pub struct MemoryRegion {
    pub base: u64,
    pub len: u64,
    /// 1 = usable RAM, everything else is reserved in some way.
    pub ty: u32,
}

/// What the kernel keeps from the boot information.
pub struct Info {
    pub usable_memory: u64,
    pub regions: [MemoryRegion; 32],
    pub region_count: usize,
    /// Which boot protocol the information came from ("multiboot1"/"2").
    pub protocol: &'static str,
}

impl Info {
    const fn empty() -> Self {
        Info {
            usable_memory: 0,
            regions: [MemoryRegion {
                base: 0,
                len: 0,
                ty: 0,
            }; 32],
            region_count: 0,
            protocol: "none",
        }
    }

    fn push(&mut self, base: u64, len: u64, ty: u32) {
        if ty == 1 {
            self.usable_memory += len;
        }
        if self.region_count < self.regions.len() {
            self.regions[self.region_count] = MemoryRegion { base, len, ty };
            self.region_count += 1;
        }
    }

    /// The regions actually filled in.
    pub fn regions_slice(&self) -> &[MemoryRegion] {
        &self.regions[..self.region_count]
    }
}

const TAG_END: u32 = 0;
const TAG_MMAP: u32 = 6;

#[repr(C)]
struct TagHeader {
    ty: u32,
    size: u32,
}

/// Parse whichever boot information structure the loader handed over.
///
/// # Safety
/// `addr` must be the pointer the bootloader passed in `EBX`.
pub unsafe fn parse(magic: u32, addr: usize) -> Info {
    let mut info = match magic {
        MULTIBOOT2_MAGIC => parse_multiboot2(addr),
        MULTIBOOT1_MAGIC => parse_multiboot1(addr),
        // PVH does not put a magic in EAX; the structure carries its own.
        _ if addr != 0 && *(addr as *const u32) == PVH_MAGIC => parse_pvh(addr),
        _ => Info::empty(),
    };

    // Without a usable memory map the frame allocator would have nothing to
    // work with; fall back to the 1 MiB .. 32 MiB window that every PC has.
    if info.usable_memory == 0 {
        info.push(1024 * 1024, 31 * 1024 * 1024, 1);
        info.protocol = "fallback";
    }
    info
}

/// Multiboot1: a flat structure, the memory map hangs off `mmap_addr`.
unsafe fn parse_multiboot1(addr: usize) -> Info {
    let mut info = Info::empty();
    info.protocol = "multiboot1";

    let flags = *(addr as *const u32);
    if flags & (1 << 6) == 0 {
        // No memory map, but `mem_lower`/`mem_upper` (in KiB) may be there.
        if flags & 1 != 0 {
            let mem_lower = *((addr + 4) as *const u32) as u64;
            let mem_upper = *((addr + 8) as *const u32) as u64;
            info.push(0, mem_lower * 1024, 1);
            info.push(1024 * 1024, mem_upper * 1024, 1);
        }
        return info;
    }

    let mmap_length = *((addr + 44) as *const u32) as usize;
    let mmap_addr = *((addr + 48) as *const u32) as usize;
    let mut entry = mmap_addr;
    let end = mmap_addr + mmap_length;
    while entry + 4 <= end {
        // Each entry starts with its own size, which does not include the
        // size field itself.
        let size = *(entry as *const u32) as usize;
        let base = *((entry + 4) as *const u64);
        let len = *((entry + 12) as *const u64);
        let ty = *((entry + 20) as *const u32);
        info.push(base, len, ty);
        entry += size + 4;
    }
    info
}

/// PVH: `hvm_start_info` with an e820-style memory map hanging off it.
///
/// ```text
///   struct hvm_start_info {
///     uint32_t magic;            +0
///     uint32_t version;          +4
///     uint32_t flags;            +8
///     uint32_t nr_modules;      +12
///     uint64_t modlist_paddr;   +16
///     uint64_t cmdline_paddr;   +24
///     uint64_t rsdp_paddr;      +32
///     uint64_t memmap_paddr;    +40   (version >= 1)
///     uint32_t memmap_entries;  +48
///   };
/// ```
unsafe fn parse_pvh(addr: usize) -> Info {
    let mut info = Info::empty();
    info.protocol = "pvh";

    let version = *((addr + 4) as *const u32);
    if version < 1 {
        return info;
    }
    let memmap = *((addr + 40) as *const u64) as usize;
    let entries = *((addr + 48) as *const u32) as usize;
    if memmap == 0 {
        return info;
    }
    for i in 0..entries {
        // struct hvm_memmap_table_entry { u64 addr; u64 size; u32 type; u32 _; }
        let entry = memmap + i * 24;
        let base = *(entry as *const u64);
        let len = *((entry + 8) as *const u64);
        let ty = *((entry + 16) as *const u32);
        info.push(base, len, ty);
    }
    info
}

/// Multiboot2: a list of 8-byte aligned tags.
unsafe fn parse_multiboot2(addr: usize) -> Info {
    let mut info = Info::empty();
    info.protocol = "multiboot2";

    let mut tag = (addr + 8) as *const u8; // skip total_size + reserved
    loop {
        let header = tag as *const TagHeader;
        let ty = (*header).ty;
        let size = (*header).size as usize;
        if ty == TAG_END {
            break;
        }
        if ty == TAG_MMAP {
            let entry_size = *((tag as *const u32).add(2)) as usize;
            let mut entry = tag.add(16);
            let end = tag.add(size);
            while entry.add(entry_size) <= end {
                let base = *(entry as *const u64);
                let len = *((entry as usize + 8) as *const u64);
                let ty = *((entry as usize + 16) as *const u32);
                info.push(base, len, ty);
                entry = entry.add(entry_size);
            }
        }
        tag = tag.add((size + 7) & !7);
    }
    info
}
