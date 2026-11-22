//! The kernel heap. A first-fit free-list allocator (the `linked_list_allocator` crate):
//! memory handed back with `free` is reused, so a game that allocates every frame can run
//! for days. (The first version was a "bump" allocator that never freed; it leaked.)
pub use linked_list_allocator::LockedHeap as Heap;
