# Boot, disk image, ISO

- **Firmware:** UEFI x86_64. The kernel is a UEFI application (`x86_64-unknown-uefi`) at `\EFI\BOOT\BOOTX64.EFI`.
- **Boot order in `main.rs`:** memory map -> claim heap -> open `\games.pak` -> grab GOP framebuffer -> calibrate clock -> arcade loop.
  The kernel does NOT call `ExitBootServices`: the firmware's disk drivers stay alive, so games are read from
  disk whenever one is picked (see game-archive.md). No IDT/PIC/PIT of our own; the firmware keeps its own.
- **`target/ringos.img`:** raw GPT disk, one FAT32 EFI System Partition (built by `xtask` with `gpt` + `fatfs`).
  Write it to a USB stick for real hardware.
- **`target/ringos.iso`:** no ISO tool was installed, so `xtask` writes ISO9660 + El Torito itself:
  sectors 16 volume descriptor, 17 boot record, 18 terminator, 19 boot catalog (platform 0xEF = EFI,
  no emulation), 20/21 path tables, 22 root dir, 24.. a 16 MB FAT image that firmware boots like a disk.
  Verified: boots in VirtualBox (EFI) and shows the gallery.
- **Real hardware:** not tested. Needs UEFI with Secure Boot off (the kernel is unsigned). Needs a GOP
  framebuffer in RGB or BGR 32-bit format (otherwise the kernel panics at start).
- **Input:** PS/2 scancodes, polled from port 0x60/0x64 each frame (no IRQ handler). USB keyboards work on real PCs only if the firmware emulates PS/2
  (many do, not all). A USB HID driver would be the real fix.
