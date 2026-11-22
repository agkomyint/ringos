//! Build tool: compiles the kernel and packs it into a bootable disk image.
//! `cargo run` => target/ringos.img (raw GPT disk with a FAT32 EFI System Partition).
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::process::Command;

const DISK_MB: u64 = 64;

fn main() {
    // 1. build the kernel as a UEFI application
    let st = Command::new("cargo")
        .args(["build", "-p", "kernel", "--release", "--target", "x86_64-unknown-uefi"])
        .status()
        .expect("cargo");
    assert!(st.success(), "kernel build failed");
    let efi = fs::read("target/x86_64-unknown-uefi/release/kernel.efi").expect("kernel.efi");

    // 1b. compile the preinstalled games (games/*.ring -> bytecode)
    let mut games: Vec<(String, Vec<u8>)> = Vec::new();
    let mut entries: Vec<_> = fs::read_dir("games").expect("games dir").filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.extension().map_or(false, |x| x == "ring") {
            let src = fs::read_to_string(&p).unwrap();
            let prog = ringcc::compile(&src).unwrap_or_else(|err| panic!("{}: {err}", p.display()));
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            println!("compiled {name} ({} bytes)", prog.to_bytes().len());
            games.push((name, prog.to_bytes()));
        }
    }

    // 2. create an empty disk file
    let path = "target/ringos.img";
    let size = DISK_MB * 1024 * 1024;
    let mut file = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path).unwrap();
    file.set_len(size).unwrap();

    // 3. protective MBR + GPT with one EFI System Partition
    let lb = gpt::disk::LogicalBlockSize::Lb512;
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size((size / 512 - 1) as u32);
    mbr.overwrite_lba0(&mut file).unwrap();
    let (start, len);
    {
        let mut disk = gpt::GptConfig::new()
            .writable(true)
            .logical_block_size(lb)
            .create_from_device(&mut file, None)
            .unwrap();
        let id = disk
            .add_partition("EFI", (DISK_MB - 4) * 1024 * 1024, gpt::partition_types::EFI, 0, None)
            .unwrap();
        let p = disk.partitions().get(&id).unwrap();
        start = p.bytes_start(lb).unwrap();
        len = p.bytes_len(lb).unwrap();
        disk.write().unwrap();
    }

    // 4. format the partition FAT32 and copy the kernel to \EFI\BOOT\BOOTX64.EFI
    let slice = fscommon::StreamSlice::new(&mut file, start, start + len).unwrap();
    let buf = fscommon::BufStream::new(slice);
    fatfs::format_volume(&mut { buf }, fatfs::FormatVolumeOptions::new().fat_type(fatfs::FatType::Fat32)).ok();
    // (re-open after format)
    let slice = fscommon::StreamSlice::new(&mut file, start, start + len).unwrap();
    let fs = fatfs::FileSystem::new(fscommon::BufStream::new(slice), fatfs::FsOptions::new()).unwrap();
    let root = fs.root_dir();
    root.create_dir("EFI").unwrap().create_dir("BOOT").unwrap();
    let mut f = root.open_dir("EFI").unwrap().open_dir("BOOT").unwrap().create_file("BOOTX64.EFI").unwrap();
    f.write_all(&efi).unwrap();
    f.flush().unwrap();
    drop(f);
    let gd = root.create_dir("games").unwrap();
    for (name, bytes) in &games {
        let mut f = gd.create_file(&format!("{name}.rbc")).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }
    drop(gd);
    drop(root);
    drop(fs);
    file.seek(SeekFrom::Start(0)).unwrap();
    println!("wrote {path} ({} KiB kernel)", efi.len() / 1024);
}
