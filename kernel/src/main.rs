#![no_main]
#![no_std]

extern crate alloc;

mod gfx;
mod heap;

use core::time::Duration;
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

#[global_allocator]
static HEAP: heap::Bump = heap::Bump::empty();

const HEAP_BYTES: usize = 64 * 1024 * 1024;

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
    uefi::println!("RingOS memory map: {} regions", map.entries().len());
    uefi::println!("  total: {} MiB, free: {} MiB", total_pages * 4 / 1024, free_pages * 4 / 1024);
    let (start, pages) = best.expect("no free memory");
    uefi::println!("  largest free block: {:#x}, {} MiB", start, pages * 4 / 1024);
    drop(map);

    // ---- 2. Claim part of it for ourselves (so nobody else gets it).
    let take = (HEAP_BYTES as u64 / 4096).min(pages);
    let heap_ptr = boot::allocate_pages(AllocateType::Address(start), MemoryType::LOADER_DATA, take as usize).unwrap();
    unsafe { HEAP.init(heap_ptr.as_ptr() as usize, (take * 4096) as usize) };
    uefi::println!("  heap: {} MiB claimed. Taking over in 3s...", take * 4 / 1024);
    boot::stall(Duration::from_secs(3));

    // ---- 3. Grab the screen while firmware can still hand it to us.
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
    let mut s = gfx::Screen::new(&mut gop);
    core::mem::forget(gop); // dropping it would call firmware AFTER it's gone

    // ---- 4. Exit boot services: firmware services (print, allocate, stall...) are GONE.
    // From here the machine is ours. The framebuffer memory stays valid.
    let _final_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // ---- 5. Game loop with no firmware help. No stall() now, so we pace by busy-waiting
    // (crude; a real timer comes with interrupts).
    let bg = 0x101030;
    let size = 40usize;
    let (mut x, mut y) = (50i32, 50i32);
    let (mut dx, mut dy) = (4i32, 3i32);
    loop {
        s.clear(bg);
        // a green bar showing how much of the heap we've used, just to prove the allocator
        let (used, free) = HEAP.used_free(heap_ptr.as_ptr() as usize);
        let w = s.width * used / (used + free);
        s.rect(0, 0, w.max(2), 8, 0x40e060);

        x += dx;
        y += dy;
        if x < 0 || x as usize + size >= s.width { dx = -dx; x += dx; }
        if y < 0 || y as usize + size >= s.height { dy = -dy; y += dy; }
        s.rect(x as usize, y as usize, size, size, 0xffa000);
        s.present();
        for _ in 0..2_000_000 { core::hint::spin_loop(); }
    }
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
