//! Saved numbers (high scores) kept in UEFI non-volatile variables.
//!
//! Why not a disk driver? Writing our own SATA/NVMe/USB driver is a big job and differs per
//! machine. The firmware already knows how to store small named values in flash (NVRAM), the
//! same place it keeps boot settings, and its *runtime services* keep working after we took over.
//! One variable per game, 8 slots of 8 bytes each.
use alloc::format;
use uefi::runtime::{self, VariableAttributes, VariableVendor};
use uefi::{guid, CString16};

const VENDOR: VariableVendor = VariableVendor(guid!("6f1d2c3a-5b7e-4a90-8c21-d4e5f6a7b8c9"));

fn name(game: &str) -> Option<CString16> {
    CString16::try_from(format!("RingOS-{game}").as_str()).ok()
}

pub fn load(game: &str) -> [i64; 8] {
    let mut slots = [0i64; 8];
    let Some(n) = name(game) else { return slots };
    let mut buf = [0u8; 64];
    if let Ok((data, _)) = runtime::get_variable(&n, &VENDOR, &mut buf) {
        for (i, c) in data.chunks_exact(8).take(8).enumerate() {
            slots[i] = i64::from_le_bytes(c.try_into().unwrap());
        }
    }
    slots
}

pub fn save(game: &str, slots: &[i64; 8]) {
    let Some(n) = name(game) else { return };
    let mut buf = [0u8; 64];
    for (i, v) in slots.iter().enumerate() {
        buf[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    let attrs = VariableAttributes::NON_VOLATILE | VariableAttributes::BOOTSERVICE_ACCESS | VariableAttributes::RUNTIME_ACCESS;
    let _ = runtime::set_variable(&n, &VENDOR, attrs, &buf);
}
