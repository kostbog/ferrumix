//! C memory primitives emitted by LLVM for freestanding targets.
//!
//! Volatile accesses keep the compiler from recognising these loops and
//! lowering them back into calls to the same symbols.

#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    let mut i = 0;
    while i < count {
        let byte = core::ptr::read_volatile(src.add(i));
        core::ptr::write_volatile(dest.add(i), byte);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, count: usize) -> *mut u8 {
    if (dest as usize) <= (src as usize) {
        memcpy(dest, src, count)
    } else {
        let mut i = count;
        while i != 0 {
            i -= 1;
            let byte = core::ptr::read_volatile(src.add(i));
            core::ptr::write_volatile(dest.add(i), byte);
        }
        dest
    }
}

#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut u8, value: i32, count: usize) -> *mut u8 {
    let mut i = 0;
    while i < count {
        core::ptr::write_volatile(dest.add(i), value as u8);
        i += 1;
    }
    dest
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(left: *const u8, right: *const u8, count: usize) -> i32 {
    let mut i = 0;
    while i < count {
        let a = core::ptr::read_volatile(left.add(i));
        let b = core::ptr::read_volatile(right.add(i));
        if a != b {
            return a as i32 - b as i32;
        }
        i += 1;
    }
    0
}
