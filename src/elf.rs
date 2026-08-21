//! Minimal ELF64 program loader.
//!
//! Ferrumix loads statically linked `ET_EXEC` x86-64 binaries: the file header
//! is validated, every `PT_LOAD` program header is mapped into the target
//! address space with the permissions requested by the segment (`PF_R`,
//! `PF_W`, `PF_X`), the file contents are copied into the freshly allocated
//! frames and the rest of the segment (`.bss`) is left zeroed.
//!
//! ```text
//!   ELF file                      user address space (per process PML4)
//!   +-----------------+           +-----------------------------+
//!   | Elf64_Ehdr      |           | 0x80_0000_1000 text  r-x u  |
//!   | Elf64_Phdr[]    |  load ->  | ...            data  rw- u  |
//!   | segment data    |           | 0x80_0080_0000 stack rw- u  |
//!   +-----------------+           +-----------------------------+
//! ```
//!
//! The parser reads the headers byte by byte (little endian) so it never
//! depends on the alignment of the image buffer.

// ELF constants are part of the format, not all are needed by the loader.
#![allow(dead_code)]

use crate::paging;
use crate::uaccess;

/// `\x7fELF`
pub const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const EI_VERSION: usize = 6;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 0x3e;

/// Loadable segment.
pub const PT_LOAD: u32 = 1;

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

// Offsets inside Elf64_Ehdr.
const E_TYPE: usize = 16;
const E_MACHINE: usize = 18;
const E_ENTRY: usize = 24;
const E_PHOFF: usize = 32;
const E_PHENTSIZE: usize = 54;
const E_PHNUM: usize = 56;
const EHDR_SIZE: usize = 64;

// Offsets inside Elf64_Phdr.
const P_TYPE: usize = 0;
const P_FLAGS: usize = 4;
const P_OFFSET: usize = 8;
const P_VADDR: usize = 16;
const P_FILESZ: usize = 32;
const P_MEMSZ: usize = 40;
const PHDR_SIZE: usize = 56;

/// Why an image could not be loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ElfError {
    /// The buffer is smaller than an ELF header.
    TooSmall,
    /// Missing `\x7fELF` signature.
    BadMagic,
    /// Not a 64-bit little-endian ELF of the current version.
    BadClass,
    /// Not an `ET_EXEC` executable.
    NotExecutable,
    /// Not compiled for x86-64.
    BadMachine,
    /// The file contains no loadable segment.
    NoSegments,
    /// A program header points outside the file.
    Truncated,
    /// A segment wants to live outside the user address range.
    BadAddress,
    /// Mapping the segment failed.
    Vm(paging::VmError),
    /// Copying the segment contents into the new mapping failed.
    Copy(uaccess::UserFault),
}

/// A program that has been mapped into an address space.
#[derive(Clone, Copy, Debug)]
pub struct LoadedImage {
    /// Virtual address of the first instruction.
    pub entry: u64,
    /// Lowest mapped virtual address.
    pub start: u64,
    /// End of the last segment, page aligned — the initial program break.
    pub brk: u64,
    /// Number of `PT_LOAD` segments processed.
    pub segments: usize,
    /// Number of 4 KiB pages allocated for the image.
    pub pages: u64,
}

fn read_u16(bytes: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([bytes[off], bytes[off + 1]])
}

fn read_u32(bytes: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
}

fn read_u64(bytes: &[u8], off: usize) -> u64 {
    let mut value = 0u64;
    for i in (0..8).rev() {
        value = (value << 8) | bytes[off + i] as u64;
    }
    value
}

/// Header information of an ELF image, without loading it.
#[derive(Clone, Copy, Debug)]
pub struct ElfInfo {
    pub entry: u64,
    pub phnum: usize,
    pub loadable: usize,
    pub size: usize,
}

/// Validate the ELF header and report what the image contains.
pub fn inspect(image: &[u8]) -> Result<ElfInfo, ElfError> {
    if image.len() < EHDR_SIZE {
        return Err(ElfError::TooSmall);
    }
    if image[..4] != ELF_MAGIC {
        return Err(ElfError::BadMagic);
    }
    if image[EI_CLASS] != ELFCLASS64
        || image[EI_DATA] != ELFDATA2LSB
        || image[EI_VERSION] != EV_CURRENT
    {
        return Err(ElfError::BadClass);
    }
    if read_u16(image, E_TYPE) != ET_EXEC {
        return Err(ElfError::NotExecutable);
    }
    if read_u16(image, E_MACHINE) != EM_X86_64 {
        return Err(ElfError::BadMachine);
    }

    let phoff = read_u64(image, E_PHOFF) as usize;
    let phentsize = read_u16(image, E_PHENTSIZE) as usize;
    let phnum = read_u16(image, E_PHNUM) as usize;
    if phentsize < PHDR_SIZE || phoff + phnum * phentsize > image.len() {
        return Err(ElfError::Truncated);
    }

    let mut loadable = 0;
    for i in 0..phnum {
        if read_u32(image, phoff + i * phentsize + P_TYPE) == PT_LOAD {
            loadable += 1;
        }
    }
    if loadable == 0 {
        return Err(ElfError::NoSegments);
    }

    Ok(ElfInfo {
        entry: read_u64(image, E_ENTRY),
        phnum,
        loadable,
        size: image.len(),
    })
}

/// Translate ELF segment flags into page table flags.
fn segment_flags(p_flags: u32) -> u64 {
    let mut flags = paging::ENTRY_PRESENT | paging::ENTRY_USER;
    if p_flags & PF_W != 0 {
        flags |= paging::ENTRY_WRITABLE;
    }
    if p_flags & PF_X == 0 && paging::nx_enabled() {
        flags |= paging::ENTRY_NX;
    }
    flags
}

/// Load every `PT_LOAD` segment of `image` into the address space `root`.
///
/// # Safety
/// `root` must be a valid PML4 reachable through the identity window; the
/// caller owns the address space and is responsible for tearing it down.
pub unsafe fn load(root: u64, image: &[u8]) -> Result<LoadedImage, ElfError> {
    let info = inspect(image)?;
    crate::serial::trace(b'i');
    let phoff = read_u64(image, E_PHOFF) as usize;
    let phentsize = read_u16(image, E_PHENTSIZE) as usize;

    let mut start = u64::MAX;
    let mut brk = 0u64;
    let mut pages_total = 0u64;
    let mut segments = 0usize;

    for i in 0..info.phnum {
        let ph = phoff + i * phentsize;
        if read_u32(image, ph + P_TYPE) != PT_LOAD {
            continue;
        }
        let p_flags = read_u32(image, ph + P_FLAGS);
        let p_offset = read_u64(image, ph + P_OFFSET) as usize;
        let p_vaddr = read_u64(image, ph + P_VADDR);
        let p_filesz = read_u64(image, ph + P_FILESZ) as usize;
        let p_memsz = read_u64(image, ph + P_MEMSZ);

        if p_filesz as u64 > p_memsz || p_offset + p_filesz > image.len() {
            return Err(ElfError::Truncated);
        }
        if p_vaddr < uaccess::USER_MIN || p_vaddr + p_memsz >= uaccess::USER_MAX {
            return Err(ElfError::BadAddress);
        }

        let page_start = p_vaddr & !(paging::PAGE_SIZE - 1);
        let page_end = (p_vaddr + p_memsz + paging::PAGE_SIZE - 1) & !(paging::PAGE_SIZE - 1);
        let pages = (page_end - page_start) / paging::PAGE_SIZE;

        // Fresh zeroed frames: everything the file does not fill (`.bss`)
        // is therefore already zero, exactly as the ABI requires.
        let flags = segment_flags(p_flags) | paging::ENTRY_WRITABLE;
        let mapped = paging::map_alloc(root, page_start, pages, flags).map_err(ElfError::Vm)?;
        pages_total += mapped;
        crate::serial::trace(b'm');

        if p_filesz > 0 {
            let data = &image[p_offset..p_offset + p_filesz];
            uaccess::write_phys_backed(root, p_vaddr, data).map_err(ElfError::Copy)?;
        }

        crate::serial::trace(b'c');
        // Drop the write permission again for read-only segments now that the
        // contents are in place.
        if p_flags & PF_W == 0 {
            let wanted = segment_flags(p_flags);
            for page in 0..pages {
                let virt = page_start + page * paging::PAGE_SIZE;
                if let Ok(frame) = paging::unmap_page(root, virt) {
                    paging::map_page(root, virt, frame, wanted).map_err(ElfError::Vm)?;
                }
            }
        }

        if page_start < start {
            start = page_start;
        }
        if page_end > brk {
            brk = page_end;
        }
        segments += 1;
    }

    if segments == 0 {
        return Err(ElfError::NoSegments);
    }

    Ok(LoadedImage {
        entry: info.entry,
        start,
        brk,
        segments,
        pages: pages_total,
    })
}
