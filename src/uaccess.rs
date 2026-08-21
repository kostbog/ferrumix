//! Copying data across the user/kernel boundary.
//!
//! A system call may never dereference a pointer that came from ring 3
//! directly: the address may be unmapped, may point at kernel memory, or may
//! be read-only.  Every buffer is therefore validated with a software page
//! walk first and then copied byte by byte through the identity window, which
//! is exactly what `copy_from_user` / `copy_to_user` do in a real Unix kernel.

use crate::paging;

/// Lowest address a user pointer may have (the first page stays unmapped so
/// that null-pointer dereferences fault).
pub const USER_MIN: u64 = 0x1000;
/// One past the highest canonical user address.
pub const USER_MAX: u64 = 0x0000_8000_0000_0000;

/// Why a user access was rejected.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UserFault {
    /// The range is empty, wraps around or leaves the user half.
    BadRange,
    /// No page table entry for that address.
    NotMapped,
    /// The page is mapped, but only reachable by the kernel.
    NotUser,
    /// The page is mapped read-only and the kernel wanted to write it.
    NotWritable,
}

/// Check that `[addr, addr + len)` is a valid user buffer.
pub fn check_range(root: u64, addr: u64, len: u64, write: bool) -> Result<(), UserFault> {
    if len == 0 {
        return Ok(());
    }
    let end = addr.checked_add(len).ok_or(UserFault::BadRange)?;
    if addr < USER_MIN || end > USER_MAX {
        return Err(UserFault::BadRange);
    }
    let mut page = addr & !(paging::PAGE_SIZE - 1);
    while page < end {
        let (_, flags) = unsafe { paging::entry_in(root, page) }.ok_or(UserFault::NotMapped)?;
        if flags & paging::ENTRY_USER == 0 {
            return Err(UserFault::NotUser);
        }
        if write && flags & paging::ENTRY_WRITABLE == 0 {
            return Err(UserFault::NotWritable);
        }
        page += paging::PAGE_SIZE;
    }
    Ok(())
}

/// Copy `dst.len()` bytes from the user buffer at `src` into kernel memory.
pub fn copy_from_user(root: u64, dst: &mut [u8], src: u64) -> Result<usize, UserFault> {
    check_range(root, src, dst.len() as u64, false)?;
    for (i, slot) in dst.iter_mut().enumerate() {
        let virt = src + i as u64;
        let phys = unsafe { paging::translate_in(root, virt) }.ok_or(UserFault::NotMapped)?;
        *slot = unsafe { core::ptr::read_volatile(phys as *const u8) };
    }
    Ok(dst.len())
}

/// Copy `src` into the user buffer at `dst`.
pub fn copy_to_user(root: u64, dst: u64, src: &[u8]) -> Result<usize, UserFault> {
    check_range(root, dst, src.len() as u64, true)?;
    for (i, byte) in src.iter().enumerate() {
        let virt = dst + i as u64;
        let phys = unsafe { paging::translate_in(root, virt) }.ok_or(UserFault::NotMapped)?;
        unsafe { core::ptr::write_volatile(phys as *mut u8, *byte) };
    }
    Ok(src.len())
}

/// Copy a NUL-terminated string from user space into `buf`.
///
/// Returns the string without its terminator; the buffer must be big enough
/// or `BadRange` is returned.
pub fn copy_str_from_user(root: u64, src: u64, buf: &mut [u8]) -> Result<&str, UserFault> {
    let mut len = 0;
    while len < buf.len() {
        let virt = src + len as u64;
        check_range(root, virt, 1, false)?;
        let phys = unsafe { paging::translate_in(root, virt) }.ok_or(UserFault::NotMapped)?;
        let byte = unsafe { core::ptr::read_volatile(phys as *const u8) };
        if byte == 0 {
            return core::str::from_utf8(&buf[..len]).map_err(|_| UserFault::BadRange);
        }
        buf[len] = byte;
        len += 1;
    }
    Err(UserFault::BadRange)
}

/// Write kernel bytes into an address space that is not necessarily active
/// (used by the ELF loader to fill freshly mapped user pages).
///
/// # Safety
/// The destination range must already be mapped in `root`.
pub unsafe fn write_phys_backed(root: u64, dst: u64, src: &[u8]) -> Result<(), UserFault> {
    for (i, byte) in src.iter().enumerate() {
        let virt = dst + i as u64;
        let phys = paging::translate_in(root, virt).ok_or(UserFault::NotMapped)?;
        core::ptr::write_volatile(phys as *mut u8, *byte);
    }
    Ok(())
}
