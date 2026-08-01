//! Spinlocks for kernel use.
//!
//! Two variants:
//!   - `Spinlock<T>`: basic spinlock for single-CPU use
//!   - `IntSpinlock<T>`: spinlock that disables interrupts while held,
//!     preventing deadlocks when the interrupt handler also needs the lock.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

// ── Basic spinlock ──────────────────────────────────────────────────────

pub struct Spinlock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Spinlock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        SpinlockGuard { lock: self }
    }
}

pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

// ── Interrupt-safe spinlock ─────────────────────────────────────────────

pub struct IntSpinlock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for IntSpinlock<T> {}

impl<T> IntSpinlock<T> {
    pub const fn new(data: T) -> Self {
        IntSpinlock {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> IntSpinlockGuard<T> {
        // Disable interrupts before acquiring the lock.
        let was_enabled = unsafe { crate::port::pushcli() };
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        IntSpinlockGuard {
            lock: self,
            was_enabled,
        }
    }
}

pub struct IntSpinlockGuard<'a, T> {
    lock: &'a IntSpinlock<T>,
    was_enabled: bool,
}

impl<'a, T> Deref for IntSpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for IntSpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for IntSpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        // Re-enable interrupts if they were enabled before we acquired the lock.
        unsafe { crate::port::popcli(self.was_enabled); }
    }
}
