# Open questions

- Real hardware: boot, framebuffer formats, PS/2 vs USB keyboard, PC speaker presence. All untested.
- In-OS editor: needs the compiler to run inside the kernel (no_std + alloc). Is it worth it, or is
  "write on a PC, copy to the USB stick" enough?
- Installing games after the OS is built: needs a way to read the disk at runtime (FAT driver) or a
  loader step at boot that scans more places.
- 16 KB: should the limit also cover the stack/frames budget separately? Today it is one shared budget.
- Colours: 256-colour palette is fixed. Custom palettes per game would cost bytes in the .rbc.
- Controls: no gamepad support yet (USB HID).
- Boot robustness: `unwrap`/`panic!` on unusual framebuffer formats and other firmware failures; panic behaviour after
  ExitBootServices is untested. Interrupt stubs do not save SSE registers (fine while handlers stay trivial).
- Signed games / signed kernel (needed to control which games run on shipped hardware).
