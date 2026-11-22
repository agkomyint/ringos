#![no_main]
#![no_std]

extern crate alloc;

mod arcade;
mod gfx;
mod heap;
mod interrupts;
mod store;

use alloc::string::String;
use alloc::vec::Vec;
use core::time::Duration;
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::GraphicsOutput;

#[global_allocator]
static HEAP: heap::Heap = heap::Heap::empty();

/// Canvas 64 KB + one game's 16 KB + the loaded programs (about 1 KB each). 512 KiB leaves room
/// for hundreds of games.
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
    uefi::println!("RingOS memory map: {} regions", map.entries().len());
    uefi::println!("  total: {} MiB, free: {} MiB", total_pages * 4 / 1024, free_pages * 4 / 1024);
    let (start, pages) = best.expect("no free memory");
    uefi::println!("  largest free block: {:#x}, {} MiB", start, pages * 4 / 1024);
    drop(map);

    // ---- 2. Claim part of it for ourselves (so nobody else gets it).
    let take = (HEAP_BYTES as u64 / 4096).min(pages);
    let heap_ptr = boot::allocate_pages(AllocateType::Address(start), MemoryType::LOADER_DATA, take as usize).unwrap();
    unsafe { HEAP.lock().init(heap_ptr.as_ptr(), (take * 4096) as usize) };
    uefi::println!("  heap: {} MiB claimed. Taking over...", take * 4 / 1024);
    boot::stall(Duration::from_secs(1));

    // ---- 2b. Load the preinstalled games from disk (firmware still alive).
    let games = load_games();
    uefi::println!("  games: {} loaded", games.len());

    // ---- 3. Grab the screen while firmware can still hand it to us.
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).unwrap();
    let mut s = gfx::Screen::new(&mut gop);
    core::mem::forget(gop); // dropping it would call firmware AFTER it's gone

    // ---- 4. Exit boot services: firmware services (print, allocate, stall...) are GONE.
    // From here the machine is ours. The framebuffer memory stays valid.
    let _final_map = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // ---- 5. Interrupts: the CPU now reacts to the timer and keyboard.
    unsafe {
        interrupts::SCREEN = &mut s;
        interrupts::init();
    }
    arcade::run(&mut s, games)
}

/// Read every `\games\*.rbc` from the boot disk. Must run before ExitBootServices (firmware does the disk I/O).
fn load_games() -> Vec<arcade::Game> {
    use alloc::string::ToString;
    use uefi::fs::{FileSystem, PathBuf};
    use uefi::{cstr16, CString16};
    let mut games = Vec::new();
    let Ok(sfs) = boot::get_image_file_system(boot::image_handle()) else { return games };
    let mut fs = FileSystem::new(sfs);
    let Ok(dir) = fs.read_dir(cstr16!("\\games")) else { return games };
    let names: Vec<String> = dir.filter_map(|e| e.ok()).filter(|e| e.is_regular_file()).map(|e| e.file_name().to_string()).collect();
    for file in names {
        let Some(stem) = file.strip_suffix(".rbc") else { continue };
        let Ok(full) = CString16::try_from(alloc::format!("\\games\\{file}").as_str()) else { continue };
        let path = PathBuf::from(full);
        let Ok(bytes) = fs.read(&path) else { continue };
        if let Ok(program) = ringvm::Program::from_bytes(&bytes) {
            games.push(arcade::Game { name: stem.to_string(), program, size: bytes.len() });
        }
    }
    games.sort_by(|a, b| a.name.cmp(&b.name));
    games
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
