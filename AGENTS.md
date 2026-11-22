# AGENTS.md - RingOS (the single source of truth for Ring)

RingOS is an arcade operating system written in Rust. Ring is its game language. Everything lives here.
Windows is not a target: the old `ringc` compiler is only on the `windows-legacy` branch of github.com/agkomyint/ringlang.

## Layout
- `SYNTAX.md` - the whole language. If it is not there or in `examples/`/`games/`, it does not exist. Do not invent syntax.
- `GOALS.md` - design constraints from the Ring project (small, fast, one obvious way).
- `compiler/` (`ringcc`) - Ring source -> bytecode (lexer, parser, code generator). Runs on the host today.
- `vm/` (`ringvm`) - no_std bytecode VM and the `Host` trait. Sandboxed: bounds-checked arrays, capped recursion/memory.
- `kernel/` - UEFI boot, memory, framebuffer (`gfx`), interrupts/keyboard/timer, the gallery and VM host (`arcade`).
- `xtask/` - `cargo run` compiles `games/*.ring` and builds `target/ringos.img` (raw disk: write to a USB stick) and `target/ringos.iso` (El Torito UEFI CD).
- `games/` - preinstalled games (copied to `\games\*.rbc` on the disk). `examples/` - small language samples.
- `run.ps1` - builds, then boots the image in VirtualBox (EFI).

## Commands
```powershell
cargo test -p ringcc -p ringvm     # language + VM tests (also compiles every example and game)
cargo run                          # build target\ringos.img
.\run.ps1                          # boot it in VirtualBox
```
Adding a game: drop `games/name.ring`, rebuild. New language features need a VM op + compiler support + a test + SYNTAX.md.
