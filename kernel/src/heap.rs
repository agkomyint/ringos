//! The simplest possible allocator: a "bump" allocator.
//! Keep a pointer into a big free region; to allocate, round it up to the alignment,
//! hand out that address, and move the pointer forward. `free` does nothing.
//! Fast and tiny; fine for a kernel that allocates a few long-lived things.
//! (Later we can swap in a smarter one without touching callers.)
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

pub struct Bump {
    next: UnsafeCell<usize>,
    end: UnsafeCell<usize>,
}

// We are single-threaded for now, so sharing this static is fine.
unsafe impl Sync for Bump {}

impl Bump {
    pub const fn empty() -> Bump {
        Bump { next: UnsafeCell::new(0), end: UnsafeCell::new(0) }
    }
    /// Give the allocator its memory. Call once, before the first allocation.
    pub unsafe fn init(&self, start: usize, size: usize) {
        *self.next.get() = start;
        *self.end.get() = start + size;
    }
    pub fn used_free(&self, start: usize) -> (usize, usize) {
        unsafe { (*self.next.get() - start, *self.end.get() - *self.next.get()) }
    }
}

unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let next = *self.next.get();
        let start = (next + l.align() - 1) & !(l.align() - 1); // round up to alignment
        let new = start + l.size();
        if new > *self.end.get() {
            return core::ptr::null_mut(); // out of memory
        }
        *self.next.get() = new;
        start as *mut u8
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}
