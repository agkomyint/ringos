#![no_main]
#![no_std]

extern crate alloc;

mod arcade;
mod disk;
mod gfx;
mod heap;
mod input;
mod sound;
mod store;

use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

#[global_allocator]
static HEAP: heap::Heap = heap::Heap::empty();

/// Canvas 64 KB + the one running game (16 KB) + a page of the menu. Games are NOT kept in memory:
/// they are read from \games.pak when picked, so RAM use does not grow with the game count.
const HEAP_BYTES: usize = 512 * 1024;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    let _ = boot::set_watchdog_timer(0, 0, None);

    // ---- 1. Read the memory map: the firmware's list of every RAM region and what it's for.
    let map = boot::memory_map(MemoryType::LOADER_DATA).unwrap();
    let mut free_pages = 0u64;
    let mut total_pages = 0u64;
    let mut best: Option<(u64, u64)> = None; // (start, pages) of largest free region
    for d in map.entries() {
        total_pages += d.page_count;
        if d.ty == MemoryType::CONVENTIONAL {
            free_pages += d.page_count;
            if best.map_or(true, |(_, p)| d.page_count > p) {
                best = Some((d.phys_start, d.page_count));
            }
        }
    }
    uefi::println!("RingOS v{}", env!("CARGO_PKG_VERSION"));
    uefi::println!("RingOS memory map: {} regions", map.entries().len());
    uefi::println!("  total: {} MiB, free: {} MiB", total_pages * 4 / 1024, free_pages * 4 / 1024);
    let (start, pages) = best.expect("no free memory");
    uefi::println!("  largest free block: {:#x}, {} MiB", start, pages * 4 / 1024);
    drop(map);

    // ---- 2. Claim part of it for ourselves (so nobody else gets it).
    let take = (HEAP_BYTES as u64 / 4096).min(pages);
    let heap_ptr = boot::allocate_pages(AllocateType::Address(start), MemoryType::LOADER_DATA, take as usize).unwrap();
    unsafe { HEAP.lock().init(heap_ptr.as_ptr(), (take * 4096) as usize) };
    uefi::println!("  heap: {} KiB claimed", take * 4);

    // ---- 3. Open the game archive. The firmware's disk drivers stay alive for the whole run, so
    // the arcade can read games from disk whenever one is picked.
    let pak = disk::open_pak();
    match &pak {
        Some(p) => uefi::println!("  games.pak: {} games", p.len()),
        None => uefi::println!("  games.pak: not found"),
    }

    // ---- 4. Grab the screen and start the clock. We do NOT exit boot services.
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
    let mut s = gfx::Screen::new(&mut gop);
    core::mem::forget(gop); // keep the framebuffer claim for the life of the machine
    input::init_clock();
    sound::init();
    input::flush();
    arcade::run(&mut s, pak)
}

/// No libc here, so provide the one C function the firmware bindings expect.
#[no_mangle]
pub unsafe extern "C" fn wcslen(mut s: *const u16) -> usize {
    let mut n = 0;
    while *s != 0 {
        s = s.add(1);
        n += 1;
    }
    n
}
