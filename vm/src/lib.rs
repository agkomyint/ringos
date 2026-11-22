//! ringvm - the Ring virtual machine: bytecode format, interpreter, and the `Host` trait.
//!
//! Design: the VM knows NOTHING about screens or keyboards. A game asks for things through
//! "intrinsics" (rect, key, frame...) and the VM forwards them to a `Host`. The kernel is one
//! Host; the tests below use a fake one. Swap the Host and the same game runs anywhere.
//!
//! Safety: a game can only touch its own values and arrays. Every array access is checked,
//! recursion depth and memory are capped, and the host is polled so a game stuck in an
//! endless loop can still be quit with Esc. A buggy game ends with a `VmError`, never a crash.
#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------------------
// Bytecode. The VM is a *stack machine*: instructions pop their inputs from a stack of i64
// and push their result. `1 + 2 * 3` becomes: PUSH 1, PUSH 2, PUSH 3, MUL, ADD.
pub mod op {
    pub const PUSH: u8 = 0; // i64
    pub const LOAD: u8 = 1; // u16 slot
    pub const STORE: u8 = 2; // u16 slot
    pub const ADD: u8 = 3;
    pub const SUB: u8 = 4;
    pub const MUL: u8 = 5;
    pub const DIV: u8 = 6;
    pub const MOD: u8 = 7;
    pub const NEG: u8 = 8;
    pub const EQ: u8 = 9;
    pub const NE: u8 = 10;
    pub const LT: u8 = 11;
    pub const GT: u8 = 12;
    pub const LE: u8 = 13;
    pub const GE: u8 = 14;
    pub const JMP: u8 = 15; // u32 target
    pub const JZ: u8 = 16; // u32 target: pop, jump if zero
    pub const JNZ: u8 = 17; // u32 target: pop, jump if not zero
    pub const CALL: u8 = 18; // u16 function number
    pub const RET: u8 = 19;
    pub const POP: u8 = 20;
    pub const PRINT: u8 = 21; // pop a number, print it
    pub const PRINTS: u8 = 22; // u16 string number
    pub const NEW: u8 = 23; // pop n, push a new array of n cells
    pub const NEWB: u8 = 24; // same, but cells hold bytes (stores keep the low 8 bits)
    pub const INDEX: u8 = 25; // pop i, pop array, push cell
    pub const BINDEX: u8 = 26;
    pub const STOREIDX: u8 = 27; // pop value, pop i, pop array
    pub const BSTOREIDX: u8 = 28;
    pub const SIZE: u8 = 29;
    pub const LIT: u8 = 30; // u16 string number: push a new array holding that text
    pub const FREE: u8 = 31;
    pub const SCOPE_OPEN: u8 = 32;
    pub const SCOPE_CLOSE: u8 = 33;
    pub const KEEP: u8 = 34;
    pub const INTR: u8 = 35; // u8 intrinsic number
    pub const HALT: u8 = 36;
    pub const PUSH8: u8 = 38; // i8: small numbers cost 2 bytes instead of 9
    pub const PUSH16: u8 = 39; // i16
    pub const TEXTLIT: u8 = 37; // u16 string number; pop colour, y, x: draw that literal text (no array made)
}

/// The built-in "arcade" functions: (name, number of arguments). The index is the id in INTR.
/// Colours 0-15 are a fixed palette; bigger numbers are raw 0xRRGGBB.
/// key(k): 0 left, 1 right, 2 up, 3 down, 4 button A, 5 button B, 6 start.
pub const INTRINSICS: &[(&str, usize)] = &[
    ("clear", 1),  // clear(color)
    ("rect", 5),   // rect(x, y, w, h, color)
    ("pixel", 3),  // pixel(x, y, color)
    ("text", 4),   // text(x, y, "string", color)
    ("num", 4),    // num(x, y, number, color)
    ("key", 1),    // key(k): 1 while held
    ("frame", 0),  // show what was drawn and wait for the next frame (1/60 s)
    ("time", 0),   // milliseconds since the game started
    ("rnd", 1),    // rnd(n): a random number from 0 to n-1
    ("width", 0),  // screen width in pixels
    ("height", 0), // screen height in pixels
    ("beep", 2),   // beep(freq_hz, ms): a square-wave tone, 0 Hz = silence
    ("load", 1),   // load(slot): a saved number (slot 0-7, per game; 0 if never saved)
    ("save", 2),   // save(slot, n): keep a number across reboots (high scores)
];

pub trait Host {
    fn clear(&mut self, color: i64);
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, color: i64);
    fn pixel(&mut self, x: i64, y: i64, color: i64);
    fn text(&mut self, x: i64, y: i64, s: &[u8], color: i64);
    fn num(&mut self, x: i64, y: i64, n: i64, color: i64);
    fn key(&mut self, k: i64) -> bool;
    /// Present the frame and wait for the next one. `false` means "the player quit".
    fn frame(&mut self) -> bool;
    fn time(&mut self) -> i64;
    fn width(&mut self) -> i64;
    fn height(&mut self) -> i64;
    /// Called every ~65k instructions so a runaway game can still be stopped.
    fn should_quit(&mut self) -> bool;
    /// `mouth` output.
    fn print(&mut self, s: &str);
    /// Sound and storage are optional: a host without them just ignores the calls.
    fn beep(&mut self, _freq: i64, _ms: i64) {}
    fn load(&mut self, _slot: i64) -> i64 {
        0
    }
    fn save(&mut self, _slot: i64, _value: i64) {}
}

// ---------------------------------------------------------------------------------------
// Program file format (.rbc):  "RBC1" | u16 nfuncs | funcs | u16 nstrings | strings | u32 codelen | code
pub struct Func {
    pub entry: u32,
    pub nparams: u16,
    pub nvars: u16,
}

pub struct Program {
    pub funcs: Vec<Func>,
    pub strings: Vec<Vec<u8>>,
    pub code: Vec<u8>,
    /// index into `funcs` of the top-level code
    pub main: u16,
}

impl Program {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut o = Vec::new();
        o.extend_from_slice(b"RBC1");
        o.extend_from_slice(&self.main.to_le_bytes());
        o.extend_from_slice(&(self.funcs.len() as u16).to_le_bytes());
        for f in &self.funcs {
            o.extend_from_slice(&f.entry.to_le_bytes());
            o.extend_from_slice(&f.nparams.to_le_bytes());
            o.extend_from_slice(&f.nvars.to_le_bytes());
        }
        o.extend_from_slice(&(self.strings.len() as u16).to_le_bytes());
        for s in &self.strings {
            o.extend_from_slice(&(s.len() as u16).to_le_bytes());
            o.extend_from_slice(s);
        }
        o.extend_from_slice(&(self.code.len() as u32).to_le_bytes());
        o.extend_from_slice(&self.code);
        o
    }

    pub fn from_bytes(b: &[u8]) -> Result<Program, &'static str> {
        if b.len() > MAX_PROGRAM_BYTES {
            return Err("game is over the 16 KB limit");
        }
        struct R<'a>(&'a [u8], usize);
        impl<'a> R<'a> {
            fn take(&mut self, n: usize) -> Result<&'a [u8], &'static str> {
                let end = self.1.checked_add(n).ok_or("truncated")?;
                let s = self.0.get(self.1..end).ok_or("truncated")?;
                self.1 = end;
                Ok(s)
            }
            fn u16(&mut self) -> Result<u16, &'static str> {
                Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
            }
            fn u32(&mut self) -> Result<u32, &'static str> {
                Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
            }
        }
        let mut r = R(b, 0);
        if r.take(4)? != b"RBC1" {
            return Err("not a Ring program");
        }
        let main = r.u16()?;
        let nf = r.u16()? as usize;
        let mut funcs = Vec::new();
        for _ in 0..nf {
            funcs.push(Func { entry: r.u32()?, nparams: r.u16()?, nvars: r.u16()? });
        }
        let ns = r.u16()? as usize;
        let mut strings = Vec::new();
        for _ in 0..ns {
            let n = r.u16()? as usize;
            strings.push(r.take(n)?.to_vec());
        }
        let cl = r.u32()? as usize;
        let code = r.take(cl)?.to_vec();
        if main as usize >= funcs.len() {
            return Err("bad main");
        }
        Ok(Program { funcs, strings, code, main })
    }
}

// ---------------------------------------------------------------------------------------
#[derive(Debug, PartialEq)]
pub enum VmError {
    DivideByZero,
    BadArray,    // freed, never made, or index out of range
    OutOfMemory, // array too big
    StackOverflow,
    BadProgram, // corrupt bytecode
}

impl VmError {
    pub fn message(&self) -> &'static str {
        match self {
            VmError::DivideByZero => "divided by zero",
            VmError::BadArray => "bad array use (freed, missing, or index out of range)",
            VmError::OutOfMemory => "over the 16 KB memory limit",
            VmError::StackOverflow => "over the 16 KB memory limit (stack)",
            VmError::BadProgram => "corrupt program",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Exit {
    Done,
    Quit,
}

/// THE LIMITS. A Ring game is a few KiB of bytecode plus at most 16 KiB of working memory,
/// so a whole arcade of them fits on a very small machine. Working memory = live arrays
/// (8 bytes per word cell, 1 per byte cell) + value stack (8 per value) + call frames (16 each).
pub const MEM_LIMIT: usize = 16 * 1024;
/// The compiled game file (.rbc) may not be bigger than this either.
pub const MAX_PROGRAM_BYTES: usize = 16 * 1024;
const MAX_ARRAY: i64 = MEM_LIMIT as i64; // sanity cap on one `new n`; the byte budget below does the real work
const MAX_ARRAY_TOTAL: usize = MEM_LIMIT;

/// Array storage. Word arrays use 8 bytes per cell; byte arrays (text, `bytes n`) use 1.
enum Cells {
    W(Vec<i64>),
    B(Vec<u8>),
}

impl Cells {
    fn len(&self) -> usize {
        match self {
            Cells::W(v) => v.len(),
            Cells::B(v) => v.len(),
        }
    }
    fn bytes(&self) -> usize {
        match self {
            Cells::W(v) => v.len() * 8,
            Cells::B(v) => v.len(),
        }
    }
    fn get(&self, i: usize) -> Option<i64> {
        match self {
            Cells::W(v) => v.get(i).copied(),
            Cells::B(v) => v.get(i).map(|&b| b as i64),
        }
    }
    /// Byte arrays keep only the low 8 bits, whichever store instruction is used.
    fn set(&mut self, i: usize, x: i64) -> Option<()> {
        match self {
            Cells::W(v) => *v.get_mut(i)? = x,
            Cells::B(v) => *v.get_mut(i)? = x as u8,
        }
        Some(())
    }
}

struct Frame {
    ret_pc: usize,
    base: usize,
}

pub struct Vm<'p> {
    prog: &'p Program,
    stack: Vec<i64>,
    frames: Vec<Frame>,
    /// handle h (> 0) names arrays[h-1]; None = freed
    arrays: Vec<Option<Cells>>,
    free_handles: Vec<usize>,
    live_cells: usize,
    /// allocation log for `scope`: handles made since each scope opened
    alloc_log: Vec<usize>,
    scope_marks: Vec<usize>,
    pc: usize,
    rng: u64,
}

impl<'p> Vm<'p> {
    pub fn new(prog: &'p Program, seed: u64) -> Vm<'p> {
        Vm {
            prog,
            stack: Vec::new(),
            frames: Vec::new(),
            arrays: Vec::new(),
            free_handles: Vec::new(),
            live_cells: 0,
            alloc_log: Vec::new(),
            scope_marks: Vec::new(),
            pc: 0,
            rng: seed | 1,
        }
    }

    fn next_rand(&mut self) -> u64 {
        // xorshift64: tiny, fast, good enough for games
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// Bytes of working memory in use right now.
    pub fn used(&self) -> usize {
        self.live_cells + self.stack.len() * 8 + self.frames.len() * 16
    }

    fn alloc(&mut self, cells: Cells) -> Result<i64, VmError> {
        if self.used() + cells.bytes() > MAX_ARRAY_TOTAL {
            return Err(VmError::OutOfMemory);
        }
        self.live_cells += cells.bytes();
        let slot = match self.free_handles.pop() {
            Some(s) => {
                self.arrays[s] = Some(cells);
                s
            }
            None => {
                self.arrays.push(Some(cells));
                self.arrays.len() - 1
            }
        };
        if !self.scope_marks.is_empty() {
            self.alloc_log.push(slot);
        }
        Ok(slot as i64 + 1)
    }

    fn release(&mut self, slot: usize) -> bool {
        if let Some(a) = self.arrays.get_mut(slot).and_then(|a| a.take()) {
            self.live_cells -= a.bytes();
            self.free_handles.push(slot);
            true
        } else {
            false
        }
    }

    fn arr(&self, h: i64) -> Result<&Cells, VmError> {
        if h <= 0 {
            return Err(VmError::BadArray);
        }
        self.arrays.get(h as usize - 1).and_then(|a| a.as_ref()).ok_or(VmError::BadArray)
    }

    fn arr_mut(&mut self, h: i64) -> Result<&mut Cells, VmError> {
        if h <= 0 {
            return Err(VmError::BadArray);
        }
        self.arrays.get_mut(h as usize - 1).and_then(|a| a.as_mut()).ok_or(VmError::BadArray)
    }

    fn pop(&mut self) -> Result<i64, VmError> {
        self.stack.pop().ok_or(VmError::BadProgram)
    }

    fn u8(&mut self) -> Result<u8, VmError> {
        let v = *self.prog.code.get(self.pc).ok_or(VmError::BadProgram)?;
        self.pc += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, VmError> {
        let b = self.prog.code.get(self.pc..self.pc + 2).ok_or(VmError::BadProgram)?;
        self.pc += 2;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, VmError> {
        let b = self.prog.code.get(self.pc..self.pc + 4).ok_or(VmError::BadProgram)?;
        self.pc += 4;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i64(&mut self) -> Result<i64, VmError> {
        let b = self.prog.code.get(self.pc..self.pc + 8).ok_or(VmError::BadProgram)?;
        self.pc += 8;
        Ok(i64::from_le_bytes(b.try_into().unwrap()))
    }

    fn slot(&self, s: u16) -> Result<usize, VmError> {
        let base = self.frames.last().ok_or(VmError::BadProgram)?.base;
        Ok(base + s as usize)
    }

    pub fn run(&mut self, host: &mut dyn Host) -> Result<Exit, VmError> {
        // enter main
        let main = &self.prog.funcs[self.prog.main as usize];
        self.stack.resize(main.nvars as usize, 0);
        self.frames.push(Frame { ret_pc: usize::MAX, base: 0 });
        self.pc = main.entry as usize;
        let mut budget: u32 = 0;

        loop {
            budget += 1;
            if budget & 0xffff == 0 && host.should_quit() {
                return Ok(Exit::Quit);
            }
            if self.stack.len() * 8 + self.frames.len() * 16 + self.live_cells > MEM_LIMIT {
                return Err(VmError::StackOverflow);
            }
            let opcode = self.u8()?;
            match opcode {
                op::PUSH => {
                    let v = self.i64()?;
                    self.stack.push(v);
                }
                op::PUSH8 => {
                    let v = self.u8()? as i8 as i64;
                    self.stack.push(v);
                }
                op::PUSH16 => {
                    let v = self.u16()? as i16 as i64;
                    self.stack.push(v);
                }
                op::LOAD => {
                    let s = self.u16()?;
                    let i = self.slot(s)?;
                    let v = *self.stack.get(i).ok_or(VmError::BadProgram)?;
                    self.stack.push(v);
                }
                op::STORE => {
                    let s = self.u16()?;
                    let i = self.slot(s)?;
                    let v = self.pop()?;
                    *self.stack.get_mut(i).ok_or(VmError::BadProgram)? = v;
                }
                op::ADD | op::SUB | op::MUL | op::DIV | op::MOD | op::EQ | op::NE | op::LT | op::GT | op::LE | op::GE => {
                    let b = self.pop()?;
                    let a = self.pop()?;
                    let r = match opcode {
                        op::ADD => a.wrapping_add(b),
                        op::SUB => a.wrapping_sub(b),
                        op::MUL => a.wrapping_mul(b),
                        op::DIV => {
                            if b == 0 {
                                return Err(VmError::DivideByZero);
                            }
                            a.wrapping_div(b)
                        }
                        op::MOD => {
                            if b == 0 {
                                return Err(VmError::DivideByZero);
                            }
                            a.wrapping_rem(b)
                        }
                        op::EQ => (a == b) as i64,
                        op::NE => (a != b) as i64,
                        op::LT => (a < b) as i64,
                        op::GT => (a > b) as i64,
                        op::LE => (a <= b) as i64,
                        _ => (a >= b) as i64,
                    };
                    self.stack.push(r);
                }
                op::NEG => {
                    let a = self.pop()?;
                    self.stack.push(a.wrapping_neg());
                }
                op::JMP => self.pc = self.u32()? as usize,
                op::JZ | op::JNZ => {
                    let t = self.u32()? as usize;
                    let v = self.pop()?;
                    if (v == 0) == (opcode == op::JZ) {
                        self.pc = t;
                    }
                }
                op::CALL => {
                    let f = self.u16()? as usize;
                    let func = self.prog.funcs.get(f).ok_or(VmError::BadProgram)?;
                    let base = self.stack.len().checked_sub(func.nparams as usize).ok_or(VmError::BadProgram)?;
                    self.stack.resize(base + func.nvars as usize, 0);
                    self.frames.push(Frame { ret_pc: self.pc, base });
                    self.pc = func.entry as usize;
                }
                op::RET => {
                    let v = self.pop()?;
                    let fr = self.frames.pop().ok_or(VmError::BadProgram)?;
                    self.stack.truncate(fr.base);
                    self.stack.push(v);
                    self.pc = fr.ret_pc;
                }
                op::POP => {
                    self.pop()?;
                }
                op::PRINT => {
                    let v = self.pop()?;
                    let mut s = String::new();
                    use core::fmt::Write;
                    let _ = write!(s, "{}", v);
                    host.print(&s);
                }
                op::PRINTS => {
                    let i = self.u16()? as usize;
                    let s = self.prog.strings.get(i).ok_or(VmError::BadProgram)?;
                    host.print(&String::from_utf8_lossy_compat(s));
                }
                op::NEW | op::NEWB => {
                    let n = self.pop()?;
                    if !(0..=MAX_ARRAY).contains(&n) {
                        return Err(VmError::OutOfMemory);
                    }
                    // check the budget before allocating so a huge `new` fails cleanly
                    let cells = if opcode == op::NEWB { Cells::B(alloc::vec![0; n as usize]) } else { Cells::W(alloc::vec![0; n as usize]) };
                    let h = self.alloc(cells)?;
                    self.stack.push(h);
                }
                op::LIT => {
                    let i = self.u16()? as usize;
                    let s = self.prog.strings.get(i).ok_or(VmError::BadProgram)?;
                    let h = self.alloc(Cells::B(s.clone()))?;
                    self.stack.push(h);
                }
                op::INDEX | op::BINDEX => {
                    let i = self.pop()?;
                    let h = self.pop()?;
                    let a = self.arr(h)?;
                    if i < 0 {
                        return Err(VmError::BadArray);
                    }
                    let v = a.get(i as usize).ok_or(VmError::BadArray)?;
                    self.stack.push(v);
                }
                op::STOREIDX | op::BSTOREIDX => {
                    let v = self.pop()?;
                    let i = self.pop()?;
                    let h = self.pop()?;
                    let a = self.arr_mut(h)?;
                    if i < 0 {
                        return Err(VmError::BadArray);
                    }
                    a.set(i as usize, if opcode == op::BSTOREIDX { v & 0xff } else { v }).ok_or(VmError::BadArray)?;
                }
                op::SIZE => {
                    let h = self.pop()?;
                    let n = self.arr(h)?.len() as i64;
                    self.stack.push(n);
                }
                op::FREE => {
                    let h = self.pop()?;
                    if h != 0 && !self.release(h as usize - 1) {
                        return Err(VmError::BadArray); // freed twice or never made
                    }
                    self.stack.push(0);
                }
                op::SCOPE_OPEN => {
                    self.scope_marks.push(self.alloc_log.len());
                    self.stack.push(0);
                }
                op::SCOPE_CLOSE => {
                    let mark = self.scope_marks.pop().ok_or(VmError::BadProgram)?;
                    let doomed: Vec<usize> = self.alloc_log.drain(mark..).collect();
                    for slot in doomed {
                        self.release(slot);
                    }
                    self.stack.push(0);
                }
                op::KEEP => {
                    let h = *self.stack.last().ok_or(VmError::BadProgram)?;
                    if h > 0 {
                        let slot = h as usize - 1;
                        if let Some(p) = self.alloc_log.iter().rposition(|&s| s == slot) {
                            self.alloc_log.remove(p);
                            // hand it to the enclosing scope (if any) instead of freeing it
                            self.keep_into_outer(slot);
                        }
                    }
                }
                op::INTR => {
                    let id = self.u8()? as usize;
                    let r = self.intrinsic(id, host)?;
                    match r {
                        Some(v) => self.stack.push(v),
                        None => return Ok(Exit::Quit),
                    }
                }
                op::TEXTLIT => {
                    let i = self.u16()? as usize;
                    let c = self.pop()?;
                    let y = self.pop()?;
                    let x = self.pop()?;
                    let t = self.prog.strings.get(i).ok_or(VmError::BadProgram)?;
                    host.text(x, y, t, c);
                    self.stack.push(0);
                }
                op::HALT => return Ok(Exit::Done),
                _ => return Err(VmError::BadProgram),
            }
        }
    }

    /// `keep`: the array leaves the innermost scope's log and joins the next outer scope's.
    fn keep_into_outer(&mut self, slot: usize) {
        let n = self.scope_marks.len();
        if n >= 2 {
            // insert at the start of the inner scope's segment, which belongs to the outer one
            let inner_start = self.scope_marks[n - 1];
            self.alloc_log.insert(inner_start, slot);
            self.scope_marks[n - 1] += 1;
        }
        // no outer scope: it simply lives until freed (or forever)
    }

    /// Returns Some(result) or None when the player quit during `frame()`.
    fn intrinsic(&mut self, id: usize, host: &mut dyn Host) -> Result<Option<i64>, VmError> {
        let arity = INTRINSICS.get(id).ok_or(VmError::BadProgram)?.1;
        let mut a = [0i64; 5];
        for i in (0..arity).rev() {
            a[i] = self.pop()?;
        }
        Ok(Some(match id {
            0 => {
                host.clear(a[0]);
                0
            }
            1 => {
                host.rect(a[0], a[1], a[2], a[3], a[4]);
                0
            }
            2 => {
                host.pixel(a[0], a[1], a[2]);
                0
            }
            3 => {
                match self.arr(a[2])? {
                    Cells::B(v) => host.text(a[0], a[1], v, a[3]), // no copy
                    Cells::W(v) => {
                        let bytes: Vec<u8> = v.iter().map(|&c| c as u8).collect();
                        host.text(a[0], a[1], &bytes, a[3]);
                    }
                }
                0
            }
            4 => {
                host.num(a[0], a[1], a[2], a[3]);
                0
            }
            5 => host.key(a[0]) as i64,
            6 => {
                if !host.frame() {
                    return Ok(None);
                }
                0
            }
            7 => host.time(),
            8 => {
                if a[0] <= 0 {
                    0
                } else {
                    (self.next_rand() % a[0] as u64) as i64
                }
            }
            9 => host.width(),
            10 => host.height(),
            11 => {
                host.beep(a[0], a[1]);
                0
            }
            12 => host.load(a[0]),
            13 => {
                host.save(a[0], a[1]);
                0
            }
            _ => return Err(VmError::BadProgram),
        }))
    }
}

/// `String::from_utf8_lossy` lives in alloc as `String::from_utf8_lossy` returning Cow; this tiny
/// helper keeps the call site tidy.
trait LossyCompat {
    fn from_utf8_lossy_compat(b: &[u8]) -> String;
}
impl LossyCompat for String {
    fn from_utf8_lossy_compat(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }
}
