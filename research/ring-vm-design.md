# Ring VM design

- **Stack machine**, i64 values, bytecode "RBC1". Jumps are u32 targets; `PUSH8`/`PUSH16` keep small numbers small.
- **Host trait:** the VM knows nothing about screens. The kernel implements `Host` (draw, keys, sound, save);
  tests implement a fake. `beep`/`load`/`save` have default no-op bodies so a minimal host still works.
- **Safety:** every array access is bounds-checked; handles are checked; division by zero is an error;
  the VM polls `should_quit` every 65,536 instructions so Esc always works; memory is capped (see memory-budget).
  A game can only touch its own values: there is no file, network or window access in the language.
- **Compiler** runs on the PC (`xtask`) today. It is no_std-friendly in principle (see open-questions).
- **Parser:** retired words (`ear`, `hear`, `voice`, `window`, ...) give a clear "not part of Ring on RingOS" error.
- **Testing:** `every_game_runs_without_vm_errors` plays each game for 3000 frames with random key presses.
- **Known quirk:** `break` is not supported; games use flags and `heart` conditions.
