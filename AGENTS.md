# AGENTS.md - RingOS (the single source of truth for Ring)

RingOS is an arcade operating system written in Rust. Ring is its game language. Everything lives here.
Windows is not a target: the old `ringc` compiler is only on the `windows-legacy` branch of github.com/agkomyint/ringlang.

## Layout
- `SYNTAX.md` - the whole language. If it is not there or in `examples/`/`games/`, it does not exist. Do not invent syntax.
- `GOALS.md` - design constraints from the Ring project (small, fast, one obvious way).
- `compiler/` (`ringcc`) - Ring source -> bytecode (lexer, parser, code generator). Runs on the host today.
- `vm/` (`ringvm`) - no_std bytecode VM and the `Host` trait. Sandboxed: bounds-checked arrays, capped recursion/memory.
- `kernel/` - UEFI boot, memory, framebuffer (`gfx`), keyboard polling + cycle-counter clock (`input`), disk reads (`disk`), the gallery and VM host (`arcade`).
- `xtask/` - `cargo run` compiles `games/*.ring` and builds `target/ringos.img` (raw disk: write to a USB stick) and `target/ringos.iso` (El Torito UEFI CD).
- `games/` - preinstalled games, packed by `xtask` into ONE file `\games.pak` (sorted index + bytecode; format in `vm/src/pak.rs`). `RING_FAKE_GAMES=N cargo run --release` adds N filler games for stress tests. `examples/` - small language samples.
- `run.ps1` - builds, then boots the image in VirtualBox (EFI).

## Commands
```powershell
cargo test -p ringcc -p ringvm     # language + VM tests (also compiles every example and game)
cargo run                          # build target\ringos.img
.\run.ps1                          # boot it in VirtualBox
```
Adding a game: drop `games/name.ring`, rebuild. New language features need a VM op + compiler support + a test + SYNTAX.md.

## Memory budget (a design goal, not an accident)
- Ring games: 16 KB compiled + 16 KB working memory, enforced in `vm/src/lib.rs` (`MEM_LIMIT`, `MAX_PROGRAM_BYTES`)
  and tested in `compiler/src/lib.rs` (`limits`). Do not raise these; make things smaller instead.
- Kernel heap is 512 KiB (`HEAP_BYTES`); the gallery shows live heap use. Today: ~72 KB with 6 games
  (64 KB of that is the 320x200 8-bit canvas). Never call `format!`/allocate while holding `HEAP.lock()`.
- No full-resolution back buffer: games draw on the 320x200 canvas and `Screen::present` scales it.
