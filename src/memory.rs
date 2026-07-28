//! Physical frame allocator — first step toward real Unix memory management.
//!
//! Uses the Multiboot2 memory map to build a list of usable regions and
//! provides a bump allocator with free-list recycling.  The allocator is
//! intentionally simple (single CPU) and protected by our `Spinlock`.
//!
//! Frame size is 4 KiB.  The kernel image (1 MiB .. __kernel_end) and low
//! memory <1 MiB (where BIOS / Multiboot / VGA etc. live) are excluded.

use crate::spinlock::Spinlock;
use crate::multiboot::{Info, MemoryRegion};

pub const FRAME_SIZE: u64 = 4096;

extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

#[derive(Clone, Copy)]
struct Region {
    start: u64,
    end: u64, // exclusive
    next: u64,
}

impl Region {
    const fn empty() -> Self {
        Region {
            start: 0,
            end: 0,
            next: 0,
        }
    }
    fn contains(&self, addr: u64) -> bool {
        addr >= self.start && addr < self.end
    }
    fn has_next(&self) -> bool {
        self.next < self.end
    }
}

pub struct FrameAllocator {
    regions: [Region; 32],
    region_count: usize,
    // Recycled frames (free list) — stack of phys addresses.
    free_list: [u64; 256],
    free_count: usize,
    total_frames: u64,
    used_frames: u64,
}

impl FrameAllocator {
    pub const fn new() -> Self {
        FrameAllocator {
            regions: [Region::empty(); 32],
            region_count: 0,
            free_list: [0; 256],
            free_count: 0,
            total_frames: 0,
            used_frames: 0,
        }
    }

    /// Initialise from Multiboot info.
    pub fn init(&mut self, info: &Info) {
        self.regions = [Region::empty(); 32];
        self.region_count = 0;
        self.total_frames = 0;
        self.used_frames = 0;

        let k_start = unsafe { &__kernel_start as *const u8 as u64 };
        let k_end = unsafe { &__kernel_end as *const u8 as u64 };

        // Round kernel end up to next frame.
        let k_end_aligned = (k_end + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);

        for r in info.regions_slice() {
            if r.ty != 1 {
                continue;
            }
            let mut start = r.base;
            let mut end = r.base + r.len;

            // Align start up, end down to frame boundaries.
            start = (start + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
            end &= !(FRAME_SIZE - 1);
            if start >= end {
                continue;
            }

            // Skip low memory <1 MiB to avoid clobbering BIOS / VGA / boot structures.
            const LOW_MEM_CUTOFF: u64 = 1 * 1024 * 1024;
            if start < LOW_MEM_CUTOFF {
                start = LOW_MEM_CUTOFF;
                start = (start + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
                if start >= end {
                    continue;
                }
            }

            // Cut out kernel image from region if overlapping.
            if start < k_end_aligned && end > k_start {
                if start < k_start {
                    let left_end = k_start & !(FRAME_SIZE - 1);
                    if left_end > start {
                        self.add_region(start, left_end);
                    }
                }
                if end > k_end_aligned {
                    self.add_region(k_end_aligned, end);
                }
            } else {
                self.add_region(start, end);
            }
        }

        for i in 0..self.region_count {
            let r = &self.regions[i];
            if r.end > r.start {
                self.total_frames += (r.end - r.start) / FRAME_SIZE;
            }
        }
    }

    fn add_region(&mut self, start: u64, end: u64) {
        if self.region_count >= self.regions.len() {
            return;
        }
        if start >= end {
            return;
        }
        let aligned_start = (start + FRAME_SIZE - 1) & !(FRAME_SIZE - 1);
        let aligned_end = end & !(FRAME_SIZE - 1);
        if aligned_start >= aligned_end {
            return;
        }
        self.regions[self.region_count] = Region {
            start: aligned_start,
            end: aligned_end,
            next: aligned_start,
        };
        self.region_count += 1;
    }

    pub fn alloc_frame(&mut self) -> Option<u64> {
        if self.free_count > 0 {
            self.free_count -= 1;
            let frame = self.free_list[self.free_count];
            self.used_frames += 1;
            return Some(frame);
        }
        for i in 0..self.region_count {
            let region = &mut self.regions[i];
            if region.has_next() {
                let frame = region.next;
                region.next += FRAME_SIZE;
                self.used_frames += 1;
                return Some(frame);
            }
        }
        None
    }

    pub fn free_frame(&mut self, addr: u64) {
        if addr % FRAME_SIZE != 0 {
            return;
        }
        if self.free_count < self.free_list.len() {
            self.free_list[self.free_count] = addr;
            self.free_count += 1;
            if self.used_frames > 0 {
                self.used_frames -= 1;
            }
        }
    }

    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }
    pub fn used_frames(&self) -> u64 {
        self.used_frames
    }
    pub fn free_frames(&self) -> u64 {
        if self.total_frames >= self.used_frames {
            self.total_frames - self.used_frames + self.free_count as u64
        } else {
            self.free_count as u64
        }
    }
}

static FRAME_ALLOCATOR: Spinlock<FrameAllocator> = Spinlock::new(FrameAllocator::new());

/// Initialise the global frame allocator from Multiboot info.
pub fn init(info: &Info) {
    let mut alloc = FRAME_ALLOCATOR.lock();
    alloc.init(info);
    crate::serial::serial_println!(
        "frame allocator: {} regions, {} total frames ({} MiB), {} free",
        alloc.region_count,
        alloc.total_frames,
        alloc.total_frames * FRAME_SIZE / (1024 * 1024),
        alloc.free_frames()
    );
    for i in 0..alloc.region_count {
        let r = alloc.regions[i];
        crate::serial::serial_println!(
            "  region {}: [{:#x} - {:#x}) {} frames",
            i,
            r.start,
            r.end,
            (r.end - r.start) / FRAME_SIZE
        );
    }
}

pub fn alloc_frame() -> Option<u64> {
    FRAME_ALLOCATOR.lock().alloc_frame()
}

pub fn free_frame(addr: u64) {
    FRAME_ALLOCATOR.lock().free_frame(addr)
}

pub fn stats() -> (u64, u64, u64) {
    let a = FRAME_ALLOCATOR.lock();
    (a.total_frames(), a.used_frames(), a.free_frames())
}
