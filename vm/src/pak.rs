//! `games.pak`: every game in ONE file, with a sorted index that stays on disk.
//!
//! Why: a million separate files would waste 4 KB each on a FAT disk and make directory
//! listings crawl. A pak is one file. The OS never loads the index: it reads a few 32-byte
//! entries with a binary search (about 20 small reads for a million games), so RAM use does not
//! grow with the number of games.
//!
//! Layout (little endian):
//!   header  16 B: "RPK1", count u32, reserved u64
//!   index   count x 32 B, sorted by name: name[20] (NUL padded), offset u32, size u32, reserved u32
//!   data    the .rbc programs, back to back
use alloc::vec::Vec;

pub const MAGIC: &[u8; 4] = b"RPK1";
pub const HEADER: u64 = 16;
pub const ENTRY: u64 = 32;
pub const NAME_MAX: usize = 20;

/// Anything that can read bytes at an offset: a file on disk, or memory in tests.
pub trait Reader {
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool;
}

impl Reader for &[u8] {
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool {
        let off = off as usize;
        match self.get(off..off.saturating_add(buf.len())) {
            Some(src) => {
                buf.copy_from_slice(src);
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    name: [u8; NAME_MAX],
    pub offset: u32,
    pub size: u32,
}

impl Entry {
    pub fn name(&self) -> &str {
        let n = self.name.iter().position(|&b| b == 0).unwrap_or(NAME_MAX);
        core::str::from_utf8(&self.name[..n]).unwrap_or("?")
    }
}

pub struct Pak<R: Reader> {
    r: R,
    count: u32,
    total: u64, // size of the file, so entries cannot point past the end
}

impl<R: Reader> Pak<R> {
    /// `total` is the file length. Refuses anything that is not a well-formed pak header.
    pub fn open(mut r: R, total: u64) -> Option<Pak<R>> {
        let mut h = [0u8; 16];
        if total < HEADER || !r.read_at(0, &mut h) || &h[..4] != MAGIC {
            return None;
        }
        let count = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
        if HEADER + count as u64 * ENTRY > total {
            return None;
        }
        Some(Pak { r, count, total })
    }

    pub fn len(&self) -> u32 {
        self.count
    }

    /// Read index entry `i` (one 32-byte disk read). Entries that point outside the file are refused.
    pub fn entry(&mut self, i: u32) -> Option<Entry> {
        if i >= self.count {
            return None;
        }
        let mut b = [0u8; ENTRY as usize];
        if !self.r.read_at(HEADER + i as u64 * ENTRY, &mut b) {
            return None;
        }
        let mut name = [0u8; NAME_MAX];
        name.copy_from_slice(&b[..NAME_MAX]);
        let offset = u32::from_le_bytes([b[20], b[21], b[22], b[23]]);
        let size = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
        let data_start = HEADER + self.count as u64 * ENTRY;
        if (offset as u64) < data_start || offset as u64 + size as u64 > self.total {
            return None;
        }
        Some(Entry { name, offset, size })
    }

    /// Index of the first game whose name is >= `prefix` (so typing letters jumps there).
    /// Binary search: log2(count) disk reads, no memory.
    pub fn lower_bound(&mut self, prefix: &str) -> u32 {
        let (mut lo, mut hi) = (0u32, self.count);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let before = match self.entry(mid) {
                Some(e) => e.name().as_bytes() < prefix.as_bytes(),
                None => false,
            };
            if before {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Exact lookup by name.
    pub fn find(&mut self, name: &str) -> Option<u32> {
        let i = self.lower_bound(name);
        (self.entry(i)?.name() == name).then_some(i)
    }

    /// Read a game's bytecode into memory (only the game being started). Refuses over 16 KB.
    pub fn read_game(&mut self, e: &Entry) -> Option<Vec<u8>> {
        if e.size as usize > crate::MAX_PROGRAM_BYTES {
            return None;
        }
        let mut v = alloc::vec![0u8; e.size as usize];
        self.r.read_at(e.offset as u64, &mut v).then_some(v)
    }
}

/// Build a pak from (name, bytecode) pairs. Used by `xtask` and the tests; the kernel only reads.
/// Names longer than 20 bytes are an error; duplicates are an error.
pub fn build(games: &[(&str, &[u8])]) -> Result<Vec<u8>, &'static str> {
    let mut g: Vec<&(&str, &[u8])> = games.iter().collect();
    g.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    if g.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err("duplicate game name");
    }
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(g.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    let mut offset = HEADER + g.len() as u64 * ENTRY;
    for (name, bytes) in &g {
        if name.is_empty() || name.len() > NAME_MAX || name.bytes().any(|b| b == 0) {
            return Err("bad game name");
        }
        if bytes.len() > crate::MAX_PROGRAM_BYTES {
            return Err("game over 16 KB");
        }
        let mut e = [0u8; ENTRY as usize];
        e[..name.len()].copy_from_slice(name.as_bytes());
        e[20..24].copy_from_slice(&(offset as u32).to_le_bytes());
        e[24..28].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&e);
        offset += bytes.len() as u64;
        if offset > u32::MAX as u64 {
            return Err("pak over 4 GB");
        }
    }
    for (_, bytes) in &g {
        out.extend_from_slice(bytes);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        build(&[("snake", b"SSS"), ("pong", b"PP"), ("breakout", b"BBBB"), ("bounce", b"B")]).unwrap()
    }

    #[test]
    fn finds_games_by_name() {
        let img = sample();
        let mut p = Pak::open(&img[..], img.len() as u64).unwrap();
        assert_eq!(p.len(), 4);
        let i = p.find("pong").unwrap();
        let e = p.entry(i).unwrap();
        assert_eq!(p.read_game(&e).unwrap(), b"PP");
        assert!(p.find("nope").is_none());
        assert_eq!(p.entry(0).unwrap().name(), "bounce"); // sorted
    }

    #[test]
    fn prefix_search_jumps_to_the_first_match() {
        let img = sample();
        let mut p = Pak::open(&img[..], img.len() as u64).unwrap();
        let i = p.lower_bound("br");
        assert_eq!(p.entry(i).unwrap().name(), "breakout");
        assert_eq!(p.lower_bound("zzz"), 4);
        assert_eq!(p.lower_bound(""), 0);
    }

    #[test]
    fn refuses_bad_files() {
        let img = sample();
        assert!(Pak::open(&img[..10], 10).is_none());
        let mut bad = img.clone();
        bad[0] = b'X';
        assert!(Pak::open(&bad[..], bad.len() as u64).is_none());
        // a count that claims more index than the file holds
        let mut big = img.clone();
        big[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Pak::open(&big[..], big.len() as u64).is_none());
        // an entry pointing past the end of the file
        let mut evil = img.clone();
        evil[16 + 24..16 + 28].copy_from_slice(&1_000_000u32.to_le_bytes());
        let mut p = Pak::open(&evil[..], evil.len() as u64).unwrap();
        assert!(p.entry(0).is_none());
        assert!(build(&[("a", b"x"), ("a", b"y")]).is_err());
        assert!(build(&[("this_name_is_way_too_long", b"x")]).is_err());
    }

    #[test]
    fn a_million_games_search_in_about_twenty_reads() {
        struct Counting<'a>(&'a [u8], core::cell::Cell<u32>);
        impl Reader for &Counting<'_> {
            fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool {
                self.1.set(self.1.get() + 1);
                let mut s = self.0;
                s.read_at(off, buf)
            }
        }
        let names: Vec<String> = (0..1_000_000u32).map(|i| format!("g{i:07}")).collect();
        let games: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b"x"[..])).collect();
        let img = build(&games).unwrap();
        let c = Counting(&img, core::cell::Cell::new(0));
        let mut p = Pak::open(&c, img.len() as u64).unwrap();
        c.1.set(0);
        assert_eq!(p.find("g0765432"), Some(765_432));
        assert!(c.1.get() <= 22, "reads: {}", c.1.get());
    }
}
