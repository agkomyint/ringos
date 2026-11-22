# High scores and sound

## Saved numbers: `save(slot, n)` / `load(slot)`
- Stored in a UEFI **non-volatile variable** per game (`RingOS-<game>`, vendor GUID in `kernel/src/store.rs`),
  8 slots x 8 bytes. UEFI runtime services keep working after ExitBootServices, so no disk driver was needed.
- Verified in VirtualBox: a test game saved 42, the VM was powered off and on, the next run loaded 42.
- Caveats: NVRAM is small and some firmwares are slow or limit variable count; a real machine may behave
  differently. A file on the ESP would need a FAT driver in the kernel.

## Sound: `beep(hz, ms)`
- PIT channel 2 square wave divided from 1.193182 MHz, gated to the speaker through port 0x61 bits 0-1.
- The VM ends a beep from the 60 Hz frame loop (`tone_off` tick), and the speaker is silenced when a game exits.
- NOT verified audibly: VirtualBox gave no way to check. Some modern PCs have no PC speaker at all.
  Fallback idea: HD Audio / USB audio (a lot of work), or a tiny buzzer on custom hardware.
