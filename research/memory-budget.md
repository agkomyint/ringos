# Memory budget

**Goal:** a hard limit of 16 KB per game, so many games run on the cheapest, slowest processor.

## The rule (enforced in `vm/src/lib.rs`, tested in `compiler/src/lib.rs::limits`)
- compiled game (.rbc) <= 16 KB (compiler refuses, loader refuses)
- working memory <= 16 KB = live arrays + value stack (8 B/value) + call frames (16 B each)
- word array cell = 8 B; byte array / text cell = 1 B

## Measurements (VirtualBox, 1024x768, 6 games)
| thing | before | after |
|---|---|---|
| kernel image | 103 KiB | 68 KiB (opt-level s, LTO, strip) |
| screen back buffer | W*H*4 (3 MB at 1024x768, 33 MB at 4K) | 64 KB (320x200, 1 byte/pixel) |
| kernel heap claimed | 64 MiB | 512 KiB |
| heap in use at the menu | n/a | 72 KB (64 KB is the canvas) |
| snake.rbc | 2051 B | 1050 B (small-number pushes: 2 B, was 9 B) |
| VM RAM that boots | 256 MB (default) | 80 MB works, 64 MB fails (likely VirtualBox EFI firmware, not verified) |

Heap use was 72K before and after playing a game and returning to the menu: no leak.

## Leaks found and fixed
1. The first heap was a bump allocator that never freed. Now `linked_list_allocator` (free list).
2. Drawing text allocated a `String` per call; numbers a `format!` per call. Now no allocation.
3. `text(x, y, "literal", c)` made a new VM array per call and nothing freed it: a game would hit the
   array cap in minutes. Now the compiler emits `TEXTLIT`, which draws from the string table.
4. Deadlock (self-inflicted): calling `format!` while holding `HEAP.lock()`. Read numbers, drop the lock, then format.

## Ideas not done
- Smaller canvas or fewer colours (changes how games look).
- Keep only bytecode for unlaunched games (already ~1 KB each; not worth it yet).
- Disk access after ExitBootServices (needs a storage driver; all games are read at boot today).

## Hardening pass (review, 2026-11)
A review found that the 16 KB budget did not count per-array overhead, so a game could still exhaust the kernel heap.
- Each live array is now charged 48 bytes of overhead; at most 256 live arrays. Verified: a game making a million
  one-byte arrays now stops with the 16 KB error screen in the VM, the OS carries on.
- `Program::verify` runs at load: function limits (<= 1024 variables), valid opcodes, complete operands, jumps land
  on instructions inside their own function, slots/strings/functions/built-ins exist. Corrupt files are refused.
- The loader checks a file's size before reading it, and all preloaded games share a 192 KB budget.
- `save()` skips unchanged values and writes flash at most 16 times per play; one final write happens when the game ends.
