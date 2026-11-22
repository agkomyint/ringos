//! CPU setup: GDT, TSS, IDT, the legacy interrupt controller (PIC) and timer (PIT).
//!
//! An *interrupt* makes the CPU stop what it's doing, save its place, and jump to a handler
//! registered in the IDT (a 256-entry table: "if event N happens, run this function").
//! Events are *exceptions* (the CPU complains: bad address, divide by zero...) and
//! *IRQs* (hardware says "look at me": timer tick, key pressed).
use alloc::format;
use core::arch::global_asm;
use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use x86_64::instructions::port::Port;
use x86_64::instructions::tables::load_tss;
use x86_64::registers::segmentation::{Segment, CS, DS, ES, SS};
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

use crate::gfx::Screen;

/// Milliseconds since boot (the PIT fires at 1000 Hz).
pub static TICKS: AtomicU64 = AtomicU64::new(0);
/// Which keys are held down, indexed by PS/2 scancode.
pub static KEYS: [AtomicBool; 128] = [const { AtomicBool::new(false) }; 128];
/// Latched presses: set by the interrupt on key-down, cleared when a reader consumes it. A tap
/// shorter than one frame is still seen, because the latch outlives the key being held.
pub static PRESSED: [AtomicBool; 128] = [const { AtomicBool::new(false) }; 128];
pub static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);
/// Set by main so the crash handler can paint the screen.
pub static mut SCREEN: *mut Screen = core::ptr::null_mut();

const PIC_TIMER: u8 = 32; // IRQ0 after remapping
const PIC_KEYBOARD: u8 = 33; // IRQ1

static mut TSS: TaskStateSegment = TaskStateSegment::new();
static mut GDT: GlobalDescriptorTable = GlobalDescriptorTable::new();
static mut IDT: InterruptDescriptorTable = InterruptDescriptorTable::new();
static mut DOUBLE_FAULT_STACK: [u8; 16 * 1024] = [0; 16 * 1024];

pub fn take_pressed(scancode: u8) -> bool {
    PRESSED[(scancode & 0x7f) as usize].swap(false, Ordering::Relaxed)
}

pub fn key_down(scancode: u8) -> bool {
    KEYS[(scancode & 0x7f) as usize].load(Ordering::Relaxed)
}

pub unsafe fn init() {
    // --- GDT/TSS. In 64-bit mode segmentation is mostly vestigial, but the CPU still needs a
    // valid code segment, and the TSS holds "emergency stacks". A double fault (a fault while
    // handling a fault) runs on its own stack so a broken stack can't escalate to a reboot.
    let stack_top = VirtAddr::from_ptr(addr_of!(DOUBLE_FAULT_STACK)) + 16 * 1024u64;
    (*addr_of_mut!(TSS)).interrupt_stack_table[0] = stack_top;
    let gdt = &mut *addr_of_mut!(GDT);
    let code = gdt.append(Descriptor::kernel_code_segment());
    let tss = gdt.append(Descriptor::tss_segment(&*addr_of!(TSS)));
    (*addr_of!(GDT)).load();
    CS::set_reg(code);
    SS::set_reg(SegmentSelector(0));
    DS::set_reg(SegmentSelector(0));
    ES::set_reg(SegmentSelector(0));
    load_tss(tss);

    // --- IDT: register a handler per event.
    let idt = &mut *addr_of_mut!(IDT);
    idt.invalid_opcode.set_handler_addr(VirtAddr::new(isr_invalid_opcode as *const () as u64));
    idt.page_fault.set_handler_addr(VirtAddr::new(isr_page_fault as *const () as u64));
    idt.general_protection_fault.set_handler_addr(VirtAddr::new(isr_gpf as *const () as u64));
    idt.double_fault.set_handler_addr(VirtAddr::new(isr_double_fault as *const () as u64)).set_stack_index(0);
    idt[PIC_TIMER].set_handler_addr(VirtAddr::new(isr_timer as *const () as u64));
    idt[PIC_KEYBOARD].set_handler_addr(VirtAddr::new(isr_keyboard as *const () as u64));
    (*addr_of!(IDT)).load();

    // --- PIC: the chip that funnels hardware IRQs to the CPU. By default it maps IRQs onto
    // vectors 8-15, which collide with CPU exceptions, so we remap them to 32+.
    let mut c1: Port<u8> = Port::new(0x20);
    let mut d1: Port<u8> = Port::new(0x21);
    let mut c2: Port<u8> = Port::new(0xA0);
    let mut d2: Port<u8> = Port::new(0xA1);
    c1.write(0x11);
    c2.write(0x11); // start init sequence
    d1.write(32);
    d2.write(40); // vector offsets
    d1.write(4);
    d2.write(2); // master/slave wiring
    d1.write(1);
    d2.write(1); // 8086 mode
    d1.write(0b1111_1100); // unmask only IRQ0 (timer) and IRQ1 (keyboard)
    d2.write(0xff);

    // --- PIT: the timer chip. Base clock 1,193,182 Hz / divisor = tick rate.
    let divisor: u16 = (1_193_182 / 1000) as u16;
    let mut cmd: Port<u8> = Port::new(0x43);
    let mut ch0: Port<u8> = Port::new(0x40);
    cmd.write(0x36); // channel 0, lo/hi byte, rate generator
    ch0.write((divisor & 0xff) as u8);
    ch0.write((divisor >> 8) as u8);

    x86_64::instructions::interrupts::enable(); // `sti`: start accepting interrupts
}

fn eoi() {
    // "End Of Interrupt": tell the PIC we're done, or it will never send another.
    unsafe { Port::<u8>::new(0x20).write(0x20) };
}


// ---------------------------------------------------------------------------------------
// Entry stubs. When an interrupt fires the CPU pushes a small "frame" on the stack:
//     [rsp] rip   - where we were         (some exceptions push an error code BEFORE this)
//           cs, rflags, rsp, ss
// and jumps to our stub. A stub must preserve every register the interrupted code was using,
// call into Rust, then `iretq` (interrupt return) to resume exactly where it left off.
// This target uses the Microsoft x64 ABI: args in rcx, rdx, r8, r9, plus 32 bytes of "shadow
// space" the caller must reserve. The callee may clobber rax,rcx,rdx,r8-r11, so we save those
// (we save a few extra, harmless).
// The stack must be 16-byte aligned at `call`: entry is rsp = 8 mod 16, 72 saved bytes fix that.
global_asm!(
    r#"
.macro SAVE
    push rax; push rcx; push rdx; push rsi; push rdi; push r8; push r9; push r10; push r11
.endmacro
.macro RESTORE
    pop r11; pop r10; pop r9; pop r8; pop rdi; pop rsi; pop rdx; pop rcx; pop rax
.endmacro

.global isr_timer
isr_timer:
    SAVE
    cld
    sub rsp, 32
    call timer_handler
    add rsp, 32
    RESTORE
    iretq

.global isr_keyboard
isr_keyboard:
    SAVE
    cld
    sub rsp, 32
    call keyboard_handler
    add rsp, 32
    RESTORE
    iretq

// Exceptions never return (we show a crash screen), so no need to restore registers.
// crash_handler(frame: *const u64 [rcx], error_code: u64 [rdx], kind: u64 [r8])
.global isr_invalid_opcode        // no error code
isr_invalid_opcode:
    mov rcx, rsp
    xor edx, edx
    mov r8d, 0
    and rsp, -16
    sub rsp, 32
    call crash_handler
.global isr_page_fault            // CPU pushed an error code
isr_page_fault:
    mov rdx, [rsp]
    lea rcx, [rsp + 8]
    mov r8d, 1
    and rsp, -16
    sub rsp, 32
    call crash_handler
.global isr_gpf
isr_gpf:
    mov rdx, [rsp]
    lea rcx, [rsp + 8]
    mov r8d, 2
    and rsp, -16
    sub rsp, 32
    call crash_handler
.global isr_double_fault
isr_double_fault:
    mov rdx, [rsp]
    lea rcx, [rsp + 8]
    mov r8d, 3
    and rsp, -16
    sub rsp, 32
    call crash_handler
"#
);

extern "C" {
    fn isr_timer();
    fn isr_keyboard();
    fn isr_invalid_opcode();
    fn isr_page_fault();
    fn isr_gpf();
    fn isr_double_fault();
}

#[no_mangle]
extern "C" fn timer_handler() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    eoi();
}

#[no_mangle]
extern "C" fn keyboard_handler() {
    let code: u8 = unsafe { Port::new(0x60).read() };
    // 0xE0 is a prefix for "extended" keys like the arrows; the real code follows.
    if code != 0xE0 {
        let pressed = code & 0x80 == 0; // high bit set = key released
        KEYS[(code & 0x7f) as usize].store(pressed, Ordering::Relaxed);
        if pressed {
            PRESSED[(code & 0x7f) as usize].store(true, Ordering::Relaxed);
        }
        LAST_SCANCODE.store(code, Ordering::Relaxed);
    }
    eoi();
}

/// Paint a red "blue screen of death" and halt. Exceptions are bugs; show where.
#[no_mangle]
extern "C" fn crash_handler(frame: *const u64, error_code: u64, kind: u64) -> ! {
    let rip = unsafe { *frame };
    let (what, info) = match kind {
        0 => ("INVALID OPCODE", 0),
        1 => ("PAGE FAULT (bad memory access)", x86_64::registers::control::Cr2::read_raw()),
        2 => ("GENERAL PROTECTION FAULT", error_code),
        _ => ("DOUBLE FAULT", error_code),
    };
    unsafe {
        if let Some(s) = (*addr_of_mut!(SCREEN)).as_mut() {
            s.clear(0xa00000);
            s.text(32, 32, "RingOS CRASH", 0xffffff, 4);
            s.text(32, 100, what, 0xffffff, 2);
            s.text(32, 140, &format!("RIP   {:#x}", rip), 0xffffff, 2);
            s.text(32, 170, &format!("INFO  {:#x}", info), 0xffffff, 2);
            s.text(32, 200, &format!("kind {} err {:#x} cs {:#x} rsp {:#x}", kind, error_code, *frame.add(1), *frame.add(3)), 0xffffff, 2);
            s.present();
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}

// ---------------------------------------------------------------------------------------
// PC speaker. PIT channel 2 divides the 1.19 MHz clock down to an audible square wave, and
// bits 0-1 of port 0x61 connect it to the speaker (bit 0: gate the timer, bit 1: speaker on).
pub fn speaker_tone(freq: u32) {
    if freq < 20 || freq > 20_000 {
        speaker_off();
        return;
    }
    let div = (1_193_182 / freq).clamp(1, 65535) as u16;
    unsafe {
        let mut cmd: Port<u8> = Port::new(0x43);
        let mut ch2: Port<u8> = Port::new(0x42);
        cmd.write(0xB6); // channel 2, lo/hi byte, square wave
        ch2.write((div & 0xff) as u8);
        ch2.write((div >> 8) as u8);
        let mut gate: Port<u8> = Port::new(0x61);
        let v = gate.read();
        gate.write(v | 0b11);
    }
}

pub fn speaker_off() {
    unsafe {
        let mut gate: Port<u8> = Port::new(0x61);
        let v = gate.read();
        gate.write(v & !0b11);
    }
}
