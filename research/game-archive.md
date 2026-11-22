# Game archive (`games.pak`)

Goal: many games (the stretch goal is a million) with RAM that does not grow, on the cheapest hardware.

## Why one file
A FAT disk gives every file at least one cluster (4 KB). A million 0.5 KB games as files would waste about 4 GB and make
directory listings crawl. One archive file is exactly as big as its contents.

## Format (`vm/src/pak.rs`, little endian)
- header 16 B: `RPK1`, count u32, 8 reserved bytes
- index: count x 32 B, **sorted by name**: name[20] NUL-padded, offset u32, size u32, 4 reserved bytes
- data: the `.rbc` programs back to back

The index stays on disk. `lower_bound` is a binary search over it: about 20 reads of 32 bytes for a million games.
`Pak::open` and `Pak::entry` refuse a bad magic, a count larger than the file, and entries that point outside the
file; `read_game` refuses anything over 16 KB. The bytecode is still verified by `Program::from_bytes`.
Limits: names up to 20 bytes, pak under 4 GB (u32 offsets), unique names. `xtask` enforces them.

## How the kernel reads it
- It stays inside UEFI boot services (no `ExitBootServices`), so the firmware's own disk drivers (USB, SATA, NVMe...)
  keep working. We ask for byte ranges through `SimpleFileSystem`; `disk.rs` caches one 4 KB block so index reads
  stay cheap.
- Because the firmware's timer and interrupt setup must stay intact for its drivers, the kernel installs no IDT/PIC/PIT
  of its own. Time is the CPU cycle counter calibrated once against `stall(50 ms)`; keys are polled from the PS/2
  controller; the 60 Hz frame wait uses `stall` in 300 us steps and polls between them.
- The menu holds 8 entries in memory. ENTER reads that one game, verifies it, runs it, drops it.

## Menu
UP/DOWN 1, PGUP/PGDN 8, LEFT/RIGHT 100, TAB random game, letters/digits search by name (jumps with `lower_bound`),
BACKSPACE edits the search, ESC clears it.
Start-up showed phantom letters: the PS/2 controller still held bytes from firmware init. `input::flush` drains it
before the menu starts.

## Measurements (VirtualBox EFI, 256 MB, 2026-11)
| games in pak | pak size | RAM shown by the gallery | search + launch |
|---|---|---|---|
| 6 | 4 KiB | 66K | ok |
| 100,006 | 52 MiB | 66K | typed `zz0004` -> jumped to entry 4007, launched ok |
| 1,000,006 | 517 MiB | 66K | typed `zz000000` -> launched ok |

The kernel binary also shrank (77 -> 60 KiB) because the interrupt code is gone.
Test generator: `RING_FAKE_GAMES=N cargo run --release` (copies of the smallest game named `zz0000000...`).
Host tests: `pak::tests` (format, search, corrupt files, and 22 or fewer reads to find one game among a million).

## Not done
- Real hardware (disk drivers and PS/2 polling both untested there).
- Writing to disk at runtime; adding games to an existing pak (today: rebuild the pak with `xtask`).
- Compression (bytecode would likely shrink about 2x); categories; sorting by anything but name.
- A signed pak (anyone who can write the disk can add games; the verifier is the only gate).
