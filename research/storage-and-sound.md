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

## HD Audio (`kernel/src/sound.rs`, added after 0.2.0)
The PC speaker could not be heard in VirtualBox and many PCs lack one, so sound now goes through the Intel HD Audio
controller (PCI class 0x0403), which nearly every laptop/PC/VM has.
- PCI scan through ports 0xCF8/0xCFC, controller reset, codec discovery, commands over the CORB/RIRB rings
  (Immediate Command is the fallback; VirtualBox did not answer it).
- Finds the audio function group, then for every wired speaker/line-out/headphone pin searches pins -> mixers/selectors ->
  DAC (depth 4), powers and unmutes the path, sets stream 1, 48 kHz, 16-bit, stereo.
- One output stream plays a 16 KB ring (2 DMA buffer entries). `pump()` runs from the keyboard poll (many times per frame) and keeps
  8 KB (about 43 ms) queued ahead of the hardware's position; if it falls behind it skips forward. No interrupts.
- Voice: one square wave, amplitude 5000/32767 (`beep(hz, ms)` unchanged). If no controller works, the PC speaker is used.
  The gallery shows `HDA` (or `BEEPn`, n = the stage that failed) next to the version.
- Verified in VirtualBox 7.1 (HDA, STAC9220) by having VirtualBox dump the DMA stream
  (`VBoxInternal2/Audio/Debug/Enabled=1`, `PathOut=<dir>`): shooter's `beep(900, 20)` came out as 891 Hz for 21 ms at +-5000.
  Real hardware codecs are untested.
Not done: mixing several sounds at once, volume control, envelopes, sample playback, USB audio, HDMI-only machines.
