//! ringcc - compiles Ring source to RingOS bytecode.
//!
//! The lexer and parser are copied from ringc (the Windows compiler), so the language is the
//! same Ring. What differs is the back end: ringc emits x86 machine code, this emits bytecode
//! for the VM. RingOS programs get arcade built-ins (rect, key, frame ...) and lose the things
//! an arcade has no use for (files, network, windows, keyboard text input).
mod lexer;
mod parser;

use parser::{Expr, Stmt};
use ringvm::{op, Func, Program};

pub use ringvm;

pub fn compile(src: &str) -> Result<Program, String> {
    let toks = lexer::lex(src)?;
    let ast = parser::parse_with(toks, src)?;
    let mut g = Gen { code: Vec::new(), strings: Vec::new() };
    let mut funcs = Vec::new();

    for f in &ast.funcs {
        let entry = g.code.len() as u32;
        g.block(&f.body)?;
        // falling off the end returns 0
        g.push_i(0);
        g.code.push(op::RET);
        funcs.push(Func { entry, nparams: f.nparams as u16, nvars: f.nvars as u16 });
    }
    let entry = g.code.len() as u32;
    g.block(&ast.stmts)?;
    g.code.push(op::HALT);
    funcs.push(Func { entry, nparams: 0, nvars: ast.names.len() as u16 });
    let main = (funcs.len() - 1) as u16;
    let prog = Program { funcs, strings: g.strings, code: g.code, main };
    let size = prog.to_bytes().len();
    if size > ringvm::MAX_PROGRAM_BYTES {
        return Err(format!("game is {size} bytes; the limit is {} (16 KB)", ringvm::MAX_PROGRAM_BYTES));
    }
    Ok(prog)
}

struct Gen {
    code: Vec<u8>,
    strings: Vec<Vec<u8>>,
}

type R = Result<(), String>;

impl Gen {
    fn push_i(&mut self, v: i64) {
        if let Ok(b) = i8::try_from(v) {
            self.code.push(op::PUSH8);
            self.code.push(b as u8);
        } else if let Ok(w) = i16::try_from(v) {
            self.code.push(op::PUSH16);
            self.code.extend_from_slice(&w.to_le_bytes());
        } else {
            self.code.push(op::PUSH);
            self.code.extend_from_slice(&v.to_le_bytes());
        }
    }
    fn u16(&mut self, o: u8, v: u16) {
        self.code.push(o);
        self.code.extend_from_slice(&v.to_le_bytes());
    }
    /// emit a jump with a placeholder target; returns where to patch
    fn jump(&mut self, o: u8) -> usize {
        self.code.push(o);
        self.code.extend_from_slice(&[0; 4]);
        self.code.len() - 4
    }
    fn patch(&mut self, at: usize) {
        let here = self.code.len() as u32;
        self.code[at..at + 4].copy_from_slice(&here.to_le_bytes());
    }
    fn jump_to(&mut self, o: u8, target: usize) {
        self.code.push(o);
        self.code.extend_from_slice(&(target as u32).to_le_bytes());
    }
    fn string(&mut self, s: &str) -> Result<u16, String> {
        let b = s.as_bytes().to_vec();
        if b.len() > u16::MAX as usize || self.strings.len() >= u16::MAX as usize {
            return Err("text too long".into());
        }
        if let Some(i) = self.strings.iter().position(|x| *x == b) {
            return Ok(i as u16);
        }
        self.strings.push(b);
        Ok((self.strings.len() - 1) as u16)
    }

    fn block(&mut self, b: &[Stmt]) -> R {
        for s in b {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> R {
        match s {
            Stmt::Print(e) => {
                self.expr(e)?;
                self.code.push(op::PRINT);
            }
            Stmt::PrintStr(t) => {
                let i = self.string(t)?;
                self.u16(op::PRINTS, i);
            }
            Stmt::Assign(slot, e) => {
                self.expr(e)?;
                self.u16(op::STORE, *slot as u16);
            }
            Stmt::If(c, then, other) => {
                self.expr(c)?;
                let jz = self.jump(op::JZ);
                self.block(then)?;
                if other.is_empty() {
                    self.patch(jz);
                } else {
                    let jend = self.jump(op::JMP);
                    self.patch(jz);
                    self.block(other)?;
                    self.patch(jend);
                }
            }
            Stmt::While(c, body) => {
                let top = self.code.len();
                self.expr(c)?;
                let jz = self.jump(op::JZ);
                self.block(body)?;
                self.jump_to(op::JMP, top);
                self.patch(jz);
            }
            Stmt::Return(e) => {
                self.expr(e)?;
                self.code.push(op::RET);
            }
            Stmt::Eval(e) => {
                self.expr(e)?;
                self.code.push(op::POP);
            }
            Stmt::Store(a, i, v) | Stmt::BStore(a, i, v) => {
                self.expr(a)?;
                self.expr(i)?;
                self.expr(v)?;
                self.code.push(if matches!(s, Stmt::Store(..)) { op::STOREIDX } else { op::BSTOREIDX });
            }
            Stmt::Break => return Err("internal: break".into()),
        }
        Ok(())
    }

    fn expr(&mut self, e: &Expr) -> R {
        match e {
            Expr::Num(n) => self.push_i(*n),
            Expr::Var(slot) => self.u16(op::LOAD, *slot as u16),
            Expr::Neg(x) => {
                self.expr(x)?;
                self.code.push(op::NEG);
            }
            Expr::Bin('A', a, b) => {
                // a and b: short-circuit, result is 0 or 1
                self.expr(a)?;
                let jz = self.jump(op::JZ);
                self.expr(b)?;
                self.push_i(0);
                self.code.push(op::NE);
                let jend = self.jump(op::JMP);
                self.patch(jz);
                self.push_i(0);
                self.patch(jend);
            }
            Expr::Bin('O', a, b) => {
                self.expr(a)?;
                let jnz = self.jump(op::JNZ);
                self.expr(b)?;
                self.push_i(0);
                self.code.push(op::NE);
                let jend = self.jump(op::JMP);
                self.patch(jnz);
                self.push_i(1);
                self.patch(jend);
            }
            Expr::Bin(o, a, b) => {
                self.expr(a)?;
                self.expr(b)?;
                self.code.push(match o {
                    '+' => op::ADD,
                    '-' => op::SUB,
                    '*' => op::MUL,
                    '/' => op::DIV,
                    '%' => op::MOD,
                    'q' => op::EQ,
                    'n' => op::NE,
                    '<' => op::LT,
                    '>' => op::GT,
                    'l' => op::LE,
                    'g' => op::GE,
                    c => return Err(format!("internal: operator {c}")),
                });
            }
            Expr::Call(f, args) => {
                for a in args {
                    self.expr(a)?;
                }
                self.u16(op::CALL, *f as u16);
            }
            Expr::Intr(3, args) if matches!(args.get(2), Some(Expr::Lit(_))) => {
                // text(x, y, "literal", c): draw straight from the program's string table.
                // Making a fresh array for the literal on every call would leak one per frame.
                let Expr::Lit(t) = &args[2] else { unreachable!() };
                let i = self.string(t)?;
                self.expr(&args[0])?;
                self.expr(&args[1])?;
                self.expr(&args[3])?;
                self.u16(op::TEXTLIT, i);
            }
            Expr::Intr(14, args) if matches!(args.get(2), Some(Expr::Lit(_))) => {
                // sprite(x, y, "picture", w, scale): drawn straight from the string table, no array
                let Expr::Lit(t) = &args[2] else { unreachable!() };
                let i = self.string(t)?;
                self.expr(&args[0])?;
                self.expr(&args[1])?;
                self.expr(&args[3])?;
                self.expr(&args[4])?;
                self.u16(op::SPRITELIT, i);
            }
            Expr::Intr(id, args) => {
                for a in args {
                    self.expr(a)?;
                }
                self.code.push(op::INTR);
                self.code.push(*id);
            }
            Expr::New(n) => {
                self.expr(n)?;
                self.code.push(op::NEW);
            }
            Expr::NewB(n) => {
                self.expr(n)?;
                self.code.push(op::NEWB);
            }
            Expr::Index(a, i) | Expr::BIndex(a, i) => {
                self.expr(a)?;
                self.expr(i)?;
                self.code.push(if matches!(e, Expr::Index(..)) { op::INDEX } else { op::BINDEX });
            }
            Expr::Size(a) => {
                self.expr(a)?;
                self.code.push(op::SIZE);
            }
            Expr::Lit(s) => {
                let i = self.string(s)?;
                self.u16(op::LIT, i);
            }
            Expr::Free(a) => {
                self.expr(a)?;
                self.code.push(op::FREE);
            }
            Expr::Keep(a) => {
                self.expr(a)?;
                self.code.push(op::KEEP);
            }
            Expr::ScopeOpen => {
                self.code.push(op::SCOPE_OPEN);
            }
            Expr::ScopeClose => {
                self.code.push(op::SCOPE_CLOSE);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringvm::{Exit, Host, Vm};

    /// A fake machine that just records what the game did.
    #[derive(Default)]
    struct Fake {
        out: Vec<String>,
        rects: Vec<(i64, i64, i64, i64, i64)>,
        frames: u32,
        max_frames: u32,
        keys_down: Vec<i64>,
    }
    impl Host for Fake {
        fn clear(&mut self, _: i64) {}
        fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: i64) {
            self.rects.push((x, y, w, h, c));
        }
        fn pixel(&mut self, _: i64, _: i64, _: i64) {}
        fn text(&mut self, _: i64, _: i64, s: &[u8], _: i64) {
            self.out.push(String::from_utf8_lossy(s).into_owned());
        }
        fn num(&mut self, _: i64, _: i64, n: i64, _: i64) {
            self.out.push(n.to_string());
        }
        fn key(&mut self, k: i64) -> bool {
            self.keys_down.contains(&k)
        }
        fn frame(&mut self) -> bool {
            self.frames += 1;
            self.frames < self.max_frames
        }
        fn time(&mut self) -> i64 {
            0
        }
        fn width(&mut self) -> i64 {
            320
        }
        fn height(&mut self) -> i64 {
            200
        }
        fn should_quit(&mut self) -> bool {
            false
        }
        fn print(&mut self, s: &str) {
            self.out.push(s.to_string());
        }
    }

    fn run(src: &str) -> Fake {
        let prog = compile(src).expect("compile");
        // also check the file format round-trips
        let prog = ringvm::Program::from_bytes(&prog.to_bytes()).expect("roundtrip");
        let mut host = Fake { max_frames: 10, ..Default::default() };
        let mut vm = Vm::new(&prog, 1);
        vm.run(&mut host).expect("run");
        host
    }

    #[test]
    fn comments_for_loops_and_consts() {
        let h = run("# a comment line
const STEP 3
const NEG -2
t = 0
for i 0 4   # trailing comment
  # comment inside a block
  t = t + STEP
mouth t
mouth i
mouth NEG
mouth \"a # not a comment\"");
        assert_eq!(h.out, ["12", "4", "-2", "a # not a comment"]);
    }

    #[test]
    fn nested_for_and_zero_runs() {
        let h = run("n = 0
for i 0 3
  for j 0 4
    n = n + 1
for k 5 5
  n = n + 100
mouth n");
        assert_eq!(h.out, ["12"]);
    }

    #[test]
    fn const_cannot_be_assigned() {
        assert!(compile("const A 1
A = 2").is_err());
        assert!(compile("const A x").is_err());
    }

    #[test]
    fn overlap_min_max_abs() {
        let h = run("mouth overlap(0,0,10,10, 5,5,10,10)
mouth overlap(0,0,10,10, 10,0,5,5)
mouth min(3,-4)
mouth max(3,-4)
mouth abs(-9)");
        assert_eq!(h.out, ["1", "0", "-4", "3", "9"]);
    }

    #[test]
    fn tiles_and_palette_run() {
        run("m = bytes 4
m{1} = 1
t = bytes 8
t{4} = 49
tiles(m, 2, t, 2, 1, 0)
palette(3, 10, 20, 30)
shake(4)");
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(run("mouth 1 + 2 * 3\nmouth (1 + 2) * 3\nmouth 7 % 4 - 10 / 3").out, ["7", "9", "0"]);
    }

    #[test]
    fn functions_and_recursion() {
        let h = run("fact n\n  if n <= 1\n    back 1\n  back n * fact(n-1)\n\nadd a b\n  back a + b\n\nmouth fact(10)\nmouth add(2, fact(3))");
        assert_eq!(h.out, ["3628800", "8"]);
    }

    #[test]
    fn loops_and_if_else() {
        let h = run("i = 1\nheart i <= 15\n  if i % 15 == 0\n    mouth \"FizzBuzz\"\n  else if i % 3 == 0\n    mouth \"Fizz\"\n  else if i % 5 == 0\n    mouth \"Buzz\"\n  else\n    mouth i\n  i = i + 1");
        assert_eq!(h.out[..5], ["1", "2", "Fizz", "4", "Buzz"]);
        assert_eq!(h.out[14], "FizzBuzz");
    }

    #[test]
    fn short_circuit_and_or() {
        // `a[5]` would be an array error if evaluated; `and` must not evaluate it
        let h = run("a = new 2\nmouth 0 and a[5] == 0\nmouth 1 or a[5] == 0\nmouth 1 and 2 == 2");
        assert_eq!(h.out, ["0", "1", "1"]);
    }

    #[test]
    fn arrays_and_strings() {
        let h = run("a = new 3\na[1] = 42\nmouth a[1] + size a\ns = \"hi\"\nmouth s{0}\nb = bytes 2\nb{0} = 300\nmouth b{0}");
        assert_eq!(h.out, ["45", "104", "44"]);
    }

    #[test]
    fn arcade_calls_and_frame_loop() {
        let h = run("x = 0\nheart 1 == 1\n  clear(0)\n  rect(x, 5, 10, 10, 3)\n  x = x + 1\n  frame()");
        assert_eq!(h.frames, 10); // frame() returned false (quit) after 10
        assert_eq!(h.rects.len(), 10);
        assert_eq!(h.rects[3], (3, 5, 10, 10, 3));
    }

    #[test]
    fn keys_and_text() {
        let prog = compile("if key(1)\n  text(0, 0, \"right\", 7)\nnum(0, 0, width(), 7)").unwrap();
        let mut host = Fake { keys_down: vec![1], max_frames: 1, ..Default::default() };
        Vm::new(&prog, 1).run(&mut host).unwrap();
        assert_eq!(host.out, ["right", "320"]);
    }

    #[test]
    fn scope_frees_and_keep_survives() {
        let h = run("out = 0\nscope\n  tmp = new 100\n  res = new 3\n  res[0] = 9\n  out = keep res\nmouth out[0]");
        assert_eq!(h.out, ["9"]);
    }

    #[test]
    fn errors_are_errors_not_crashes() {
        let run_err = |src: &str| {
            let prog = compile(src).unwrap();
            let mut host = Fake { max_frames: 1, ..Default::default() };
            Vm::new(&prog, 1).run(&mut host).unwrap_err()
        };
        assert_eq!(run_err("mouth 1 / 0"), ringvm::VmError::DivideByZero);
        assert_eq!(run_err("a = new 2\nmouth a[2]"), ringvm::VmError::BadArray);
        assert_eq!(run_err("a = new 2\nfree a\nmouth a[0]"), ringvm::VmError::BadArray);
        assert_eq!(run_err("f n\n  back f(n + 1)\nmouth f(0)"), ringvm::VmError::StackOverflow);
    }

    #[test]
    fn quit_stops_runaway_loop_via_host() {
        struct Q(u32);
        impl Host for Q {
            fn clear(&mut self, _: i64) {}
            fn rect(&mut self, _: i64, _: i64, _: i64, _: i64, _: i64) {}
            fn pixel(&mut self, _: i64, _: i64, _: i64) {}
            fn text(&mut self, _: i64, _: i64, _: &[u8], _: i64) {}
            fn num(&mut self, _: i64, _: i64, _: i64, _: i64) {}
            fn key(&mut self, _: i64) -> bool {
                false
            }
            fn frame(&mut self) -> bool {
                true
            }
            fn time(&mut self) -> i64 {
                0
            }
            fn width(&mut self) -> i64 {
                0
            }
            fn height(&mut self) -> i64 {
                0
            }
            fn should_quit(&mut self) -> bool {
                self.0 += 1;
                self.0 > 3
            }
            fn print(&mut self, _: &str) {}
        }
        let prog = compile("heart 1 == 1\n  x = 1").unwrap();
        assert_eq!(Vm::new(&prog, 1).run(&mut Q(0)).unwrap(), Exit::Quit);
    }

    #[test]
    fn rejects_unavailable_features() {
        assert!(compile("x = ear").is_err());
        assert!(compile("window \"t\" 1 2").is_err());
        assert!(compile("rect(1, 2)").is_err()); // wrong arity
    }
}

#[cfg(test)]
mod example_programs {
    /// Every program in examples/ and games/ must compile: the docs can't rot unnoticed.
    #[test]
    fn all_examples_compile() {
        for dir in ["../examples", "../games"] {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.extension().map_or(false, |x| x == "ring") {
                    let src = std::fs::read_to_string(&p).unwrap();
                    if let Err(err) = super::compile(&src) {
                        panic!("{}: {err}", p.display());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod limits {
    use ringvm::{Host, Vm, VmError};
    struct Nop;
    impl Host for Nop {
        fn clear(&mut self, _: i64) {}
        fn rect(&mut self, _: i64, _: i64, _: i64, _: i64, _: i64) {}
        fn pixel(&mut self, _: i64, _: i64, _: i64) {}
        fn text(&mut self, _: i64, _: i64, _: &[u8], _: i64) {}
        fn num(&mut self, _: i64, _: i64, _: i64, _: i64) {}
        fn key(&mut self, _: i64) -> bool {
            false
        }
        fn frame(&mut self) -> bool {
            true
        }
        fn time(&mut self) -> i64 {
            0
        }
        fn width(&mut self) -> i64 {
            320
        }
        fn height(&mut self) -> i64 {
            200
        }
        fn should_quit(&mut self) -> bool {
            false
        }
        fn print(&mut self, _: &str) {}
    }
    fn run(src: &str) -> Result<ringvm::Exit, VmError> {
        Vm::new(&super::compile(src).unwrap(), 1).run(&mut Nop)
    }
    #[test]
    fn sixteen_kb_is_a_hard_limit() {
        assert!(run("a = new 2000
").is_ok()); // 16000 bytes
        assert_eq!(run("a = new 2100
").unwrap_err(), VmError::OutOfMemory); // 16800 bytes
        assert!(run("a = bytes 16000
").is_ok());
        assert_eq!(run("a = bytes 20000
").unwrap_err(), VmError::OutOfMemory);
        assert_eq!(run("a = bytes 9000
b = bytes 9000
").unwrap_err(), VmError::OutOfMemory);
        // unbounded recursion hits the same wall instead of eating the machine
        assert_eq!(run("f n
  back f(n + 1)
mouth f(0)
").unwrap_err(), VmError::StackOverflow);
        // a leaking loop (arrays made and never freed) is stopped too
        assert_eq!(run("i = 0
heart i < 100000
  a = new 100
  i = i + 1
").unwrap_err(), VmError::OutOfMemory);
    }
    #[test]
    fn oversized_programs_are_refused() {
        let mut src = String::new();
        for i in 0..3000 {
            src.push_str(&format!("x{i} = {}
", i * 1000 + 100000));
        }
        assert!(super::compile(&src).err().expect("should refuse").contains("limit"));
    }
}

#[cfg(test)]
mod game_smoke {
    use ringvm::{Host, Vm};

    struct Bot {
        frames: u32,
        seed: u32,
        saves: u32,
        beeps: u32,
    }
    impl Host for Bot {
        fn clear(&mut self, _: i64) {}
        fn rect(&mut self, _: i64, _: i64, _: i64, _: i64, _: i64) {}
        fn pixel(&mut self, _: i64, _: i64, _: i64) {}
        fn text(&mut self, _: i64, _: i64, _: &[u8], _: i64) {}
        fn num(&mut self, _: i64, _: i64, _: i64, _: i64) {}
        fn key(&mut self, k: i64) -> bool {
            self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (self.seed >> 16) % 3 == (k as u32 % 3)
        }
        fn frame(&mut self) -> bool {
            self.frames += 1;
            self.frames < 3000
        }
        fn time(&mut self) -> i64 {
            0
        }
        fn width(&mut self) -> i64 {
            320
        }
        fn height(&mut self) -> i64 {
            200
        }
        fn should_quit(&mut self) -> bool {
            false
        }
        fn print(&mut self, _: &str) {}
        fn beep(&mut self, _: i64, _: i64) {
            self.beeps += 1;
        }
        fn save(&mut self, _: i64, _: i64) {
            self.saves += 1;
        }
    }

    #[test]
    fn every_game_runs_without_vm_errors() {
        for e in std::fs::read_dir("../games").unwrap() {
            let p = e.unwrap().path();
            if p.extension().map_or(false, |x| x == "ring") {
                let prog = super::compile(&std::fs::read_to_string(&p).unwrap()).unwrap();
                let mut bot = Bot { frames: 0, seed: 7, saves: 0, beeps: 0 };
                let r = Vm::new(&prog, 42).run(&mut bot);
                assert!(r.is_ok(), "{}: {:?}", p.display(), r);
                println!("{}: frames {} beeps {} saves {}", p.display(), bot.frames, bot.beeps, bot.saves);
            }
        }
    }
}

#[cfg(test)]
mod hardening {
    use ringvm::{op, Func, Host, Program, Vm, VmError};
    struct Nop;
    impl Host for Nop {
        fn clear(&mut self, _: i64) {}
        fn rect(&mut self, _: i64, _: i64, _: i64, _: i64, _: i64) {}
        fn pixel(&mut self, _: i64, _: i64, _: i64) {}
        fn text(&mut self, _: i64, _: i64, _: &[u8], _: i64) {}
        fn num(&mut self, _: i64, _: i64, _: i64, _: i64) {}
        fn key(&mut self, _: i64) -> bool {
            false
        }
        fn frame(&mut self) -> bool {
            true
        }
        fn time(&mut self) -> i64 {
            0
        }
        fn width(&mut self) -> i64 {
            320
        }
        fn height(&mut self) -> i64 {
            200
        }
        fn should_quit(&mut self) -> bool {
            false
        }
        fn print(&mut self, _: &str) {}
    }
    fn run(src: &str) -> Result<ringvm::Exit, VmError> {
        Vm::new(&super::compile(src).unwrap(), 1).run(&mut Nop)
    }
    fn prog(nvars: u16, code: Vec<u8>) -> Program {
        Program { funcs: vec![Func { entry: 0, nparams: 0, nvars }], strings: vec![], code, main: 0 }
    }
    fn load(p: &Program) -> Result<Program, &'static str> {
        Program::from_bytes(&p.to_bytes())
    }

    #[test]
    fn tiny_arrays_cannot_exhaust_the_heap() {
        // a million empty arrays, never freed
        assert_eq!(run("i = 0\nheart i < 1000000\n  a = new 0\n  i = i + 1\n").unwrap_err(), VmError::OutOfMemory);
        // many one-byte arrays: the handle overhead is charged, so this stops long before 16000
        assert_eq!(run("i = 0\nheart i < 16000\n  a = bytes 1\n  i = i + 1\n").unwrap_err(), VmError::OutOfMemory);
        // but arrays that are freed or scoped can be reused forever
        assert!(run("i = 0\nheart i < 5000\n  a = new 10\n  free(a)\n  i = i + 1\n").is_ok());
    }

    #[test]
    fn real_games_pass_the_verifier() {
        for e in std::fs::read_dir("../games").unwrap() {
            let p = e.unwrap().path();
            if p.extension().map_or(false, |x| x == "ring") {
                let c = super::compile(&std::fs::read_to_string(&p).unwrap()).unwrap();
                load(&c).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            }
        }
    }

    #[test]
    fn the_verifier_refuses_bad_programs() {
        assert!(load(&prog(0, vec![op::HALT])).is_ok());
        assert!(load(&prog(65535, vec![op::HALT])).is_err(), "huge frame");
        assert!(load(&prog(0, vec![200])).is_err(), "unknown opcode");
        assert!(load(&prog(0, vec![op::PUSH, 1, 2])).is_err(), "truncated operand");
        assert!(load(&prog(0, vec![op::JMP, 1, 0, 0, 0, op::HALT])).is_err(), "jump into an operand");
        assert!(load(&prog(0, vec![op::JMP, 99, 0, 0, 0, op::HALT])).is_err(), "jump out of the code");
        assert!(load(&prog(1, vec![op::LOAD, 5, 0, op::HALT])).is_err(), "variable out of range");
        assert!(load(&prog(0, vec![op::CALL, 3, 0, op::HALT])).is_err(), "missing function");
        assert!(load(&prog(0, vec![op::LIT, 0, 0, op::HALT])).is_err(), "missing string");
        assert!(load(&prog(0, vec![op::INTR, 99, op::HALT])).is_err(), "missing built-in");
    }
}
