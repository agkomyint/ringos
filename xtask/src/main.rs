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
    populate(&fs, &efi, &games);
    drop(fs);
    file.seek(SeekFrom::Start(0)).unwrap();
    println!("wrote {path} ({} KiB kernel)", efi.len() / 1024);

    // 5. the same boot files again as a CD/DVD image (for VMs, or burning)
    let mut esp = std::io::Cursor::new(vec![0u8; 16 * 1024 * 1024]);
    fatfs::format_volume(&mut esp, fatfs::FormatVolumeOptions::new()).unwrap();
    {
        let fs = fatfs::FileSystem::new(&mut esp, fatfs::FsOptions::new()).unwrap();
        populate(&fs, &efi, &games);
    }
    let iso = make_iso(&esp.into_inner());
    fs::write("target/ringos.iso", &iso).unwrap();
    println!("wrote target/ringos.iso ({} MiB)", iso.len() / 1024 / 1024);
}

/// Put the kernel at \EFI\BOOT\BOOTX64.EFI (where UEFI looks) and the games in \games.
fn populate<IO: fatfs::ReadWriteSeek>(
    fs: &fatfs::FileSystem<IO>,
    efi: &[u8],
    games: &[(String, Vec<u8>)],
) {
    let root = fs.root_dir();
    root.create_dir("EFI").unwrap().create_dir("BOOT").unwrap();
    let mut f = root.open_dir("EFI").unwrap().open_dir("BOOT").unwrap().create_file("BOOTX64.EFI").unwrap();
    f.write_all(efi).unwrap();
    f.flush().unwrap();
    drop(f);
    let gd = root.create_dir("games").unwrap();
    for (name, bytes) in games {
        let mut f = gd.create_file(&format!("{name}.rbc")).unwrap();
        f.write_all(bytes).unwrap();
        f.flush().unwrap();
    }
}

fn both32(v: u32) -> [u8; 8] {
    let mut o = [0u8; 8];
    o[..4].copy_from_slice(&v.to_le_bytes());
    o[4..].copy_from_slice(&v.to_be_bytes());
    o
}
fn both16(v: u16) -> [u8; 4] {
    let mut o = [0u8; 4];
    o[..2].copy_from_slice(&v.to_le_bytes());
    o[2..].copy_from_slice(&v.to_be_bytes());
    o
}
fn dir_record(lba: u32, len: u32, flags: u8, name: &[u8]) -> Vec<u8> {
    let mut r = vec![0u8; 33 + name.len()];
    if r.len() % 2 == 1 {
        r.push(0);
    }
    r[0] = r.len() as u8;
    r[2..10].copy_from_slice(&both32(lba));
    r[10..18].copy_from_slice(&both32(len));
    r[18..24].copy_from_slice(&[126, 1, 1, 0, 0, 0]); // 2026-01-01 00:00:00
    r[25] = flags;
    r[28..32].copy_from_slice(&both16(1));
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

/// A minimal ISO9660 image with one El Torito UEFI boot entry. Layout (2048-byte sectors):
/// 16 volume descriptor, 17 boot record, 18 terminator, 19 boot catalog, 20/21 path tables,
/// 22 root directory, 24.. the FAT boot image (which firmware boots like a tiny disk).
fn make_iso(esp: &[u8]) -> Vec<u8> {
    const S: usize = 2048;
    let esp_sectors = (esp.len() + S - 1) / S;
    let total = 24 + esp_sectors;
    let mut iso = vec![0u8; total * S];
    let put = |iso: &mut Vec<u8>, sector: usize, data: &[u8]| iso[sector * S..sector * S + data.len()].copy_from_slice(data);

    let root = dir_record(22, S as u32, 2, &[0]);
    let mut pvd = vec![b' '; S];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[7] = 0;
    pvd[8..40].copy_from_slice(&[b' '; 32]);
    pvd[40..40 + 6].copy_from_slice(b"RINGOS");
    pvd[72..80].fill(0);
    pvd[80..88].copy_from_slice(&both32(total as u32));
    pvd[88..120].fill(0);
    pvd[120..124].copy_from_slice(&both16(1));
    pvd[124..128].copy_from_slice(&both16(1));
    pvd[128..132].copy_from_slice(&both16(S as u16));
    pvd[132..140].copy_from_slice(&both32(10));
    pvd[140..144].copy_from_slice(&20u32.to_le_bytes());
    pvd[144..148].fill(0);
    pvd[148..152].copy_from_slice(&21u32.to_be_bytes());
    pvd[152..156].fill(0);
    pvd[156..156 + root.len()].copy_from_slice(&root);
    for off in [813usize, 830] {
        pvd[off..off + 16].copy_from_slice(b"2026010100000000");
        pvd[off + 16] = 0;
    }
    pvd[847..863].copy_from_slice(b"0000000000000000");
    pvd[863] = 0;
    pvd[864..880].copy_from_slice(b"2026010100000000");
    pvd[880] = 0;
    pvd[881] = 1;
    pvd[883..S].fill(0);
    put(&mut iso, 16, &pvd);

    let mut br = vec![0u8; S];
    br[1..6].copy_from_slice(b"CD001");
    br[6] = 1;
    br[7..7 + 23].copy_from_slice(b"EL TORITO SPECIFICATION");
    br[71..75].copy_from_slice(&19u32.to_le_bytes());
    put(&mut iso, 17, &br);

    let mut term = vec![0u8; S];
    term[0] = 255;
    term[1..6].copy_from_slice(b"CD001");
    term[6] = 1;
    put(&mut iso, 18, &term);

    let mut cat = vec![0u8; S];
    cat[0] = 1;
    cat[1] = 0xEF; // platform: EFI
    cat[30] = 0x55;
    cat[31] = 0xAA;
    let sum: u32 = cat[..32].chunks(2).map(|w| u16::from_le_bytes([w[0], w[1]]) as u32).sum();
    let ck = (0x10000 - (sum & 0xffff)) as u16;
    cat[28..30].copy_from_slice(&ck.to_le_bytes());
    cat[32] = 0x88; // bootable
    cat[33] = 0; // no emulation: firmware reads the image as a disk
    cat[38..40].copy_from_slice(&((esp.len() / 512).min(65535) as u16).to_le_bytes());
    cat[40..44].copy_from_slice(&24u32.to_le_bytes());
    put(&mut iso, 19, &cat);

    let mut lpt = vec![0u8; 10];
    lpt[0] = 1;
    lpt[2..6].copy_from_slice(&22u32.to_le_bytes());
    lpt[6..8].copy_from_slice(&1u16.to_le_bytes());
    put(&mut iso, 20, &lpt);
    let mut mpt = vec![0u8; 10];
    mpt[0] = 1;
    mpt[2..6].copy_from_slice(&22u32.to_be_bytes());
    mpt[6..8].copy_from_slice(&1u16.to_be_bytes());
    put(&mut iso, 21, &mpt);

    let mut dir = Vec::new();
    dir.extend(dir_record(22, S as u32, 2, &[0]));
    dir.extend(dir_record(22, S as u32, 2, &[1]));
    dir.extend(dir_record(24, esp.len() as u32, 0, b"ESP.IMG;1"));
    put(&mut iso, 22, &dir);

    put(&mut iso, 24, esp);
    iso
}
