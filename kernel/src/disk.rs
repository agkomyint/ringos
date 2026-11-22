//! Reading `\games.pak` from the boot disk while the OS runs. The firmware's drivers do the
//! actual disk work (USB, SATA, NVMe...); we only ask for byte ranges. A one-block cache keeps
//! the many tiny index reads from becoming many disk reads.
use ringvm::pak::{Pak, Reader};
use uefi::boot;
use uefi::cstr16;
use uefi::proto::media::file::{File, FileAttribute, FileMode, RegularFile};

const BLOCK: u64 = 4096;

pub struct DiskReader {
    file: RegularFile,
    block_at: u64,
    block: [u8; BLOCK as usize],
    have: usize, // valid bytes in `block`
}

impl Reader for DiskReader {
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool {
        let mut done = 0;
        while done < buf.len() {
            let pos = off + done as u64;
            let base = pos / BLOCK * BLOCK;
            if self.have == 0 || self.block_at != base {
                if self.file.set_position(base).is_err() {
                    return false;
                }
                self.have = 0;
                match self.file.read(&mut self.block) {
                    Ok(n) if n > 0 => {
                        self.have = n;
                        self.block_at = base;
                    }
                    _ => return false,
                }
            }
            let start = (pos - base) as usize;
            if start >= self.have {
                return false;
            }
            let n = (self.have - start).min(buf.len() - done);
            buf[done..done + n].copy_from_slice(&self.block[start..start + n]);
            done += n;
        }
        true
    }
}

/// Open `\games.pak` on the disk we booted from.
pub fn open_pak() -> Option<Pak<DiskReader>> {
    let mut sfs = boot::get_image_file_system(boot::image_handle()).ok()?;
    let mut root = sfs.open_volume().ok()?;
    let handle = root.open(cstr16!("games.pak"), FileMode::Read, FileAttribute::empty()).ok()?;
    let mut file = handle.into_regular_file()?;
    file.set_position(RegularFile::END_OF_FILE).ok()?;
    let total = file.get_position().ok()?;
    file.set_position(0).ok()?;
    Pak::open(DiskReader { file, block_at: 0, block: [0; BLOCK as usize], have: 0 }, total)
}
