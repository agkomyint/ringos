# Open questions

- Real hardware: boot, framebuffer formats, PS/2 vs USB keyboard, HD Audio codecs (the driver is only seen working on VirtualBox's STAC9220). All untested.
- In-OS editor: needs the compiler to run inside the kernel (no_std + alloc). Is it worth it, or is
  "write on a PC, copy to the USB stick" enough?
- Writing to disk at runtime (save files, installing games from inside the OS): reads work (game-archive.md); writes
  through the firmware are possible but untested. Appending to the pak would need an index rewrite or an overlay file.
- 16 KB: should the limit also cover the stack/frames budget separately? Today it is one shared budget.
- Colours: 256-colour palette is fixed. Custom palettes per game would cost bytes in the .rbc.
- Controls: no gamepad support yet (USB HID).
- Boot robustness: `unwrap`/`panic!` on unusual framebuffer formats and other firmware failures; the kernel no longer
  installs interrupt handlers (the SSE-in-stubs risk is gone) but polls the PS/2 port, which can race with the
  firmware's own keyboard driver; real hardware may show dropped or duplicated keys. The frame wait is a busy `stall`
  loop, so the CPU never idles (power/heat on real machines).
- Signed games / signed kernel (needed to control which games run on shipped hardware).
