// The parser: turns a list of tokens into a tree (the AST).
//   Num(1) Sym('+') Num(2) Sym('*') Num(3)   ->   (+ 1 (* 2 3))
// It also gives every variable a numbered slot and rejects names used before they exist.
//
// Functions have no keyword. A line made of a name and parameter names, followed by an
// indented block, defines one:
//     add a b
//       back a + b
// and `add(1, 2)` calls it. `back` hands a value back; falling off the end gives 0.

use crate::lexer::Tok;

#[derive(Debug, Clone)]
pub enum Expr {
    Num(i64),
    Var(usize),                      // the variable in this slot
    GVar(usize),                     // a `global` of the main program, seen from a function
    Neg(Box<Expr>),                  // -x
    Bin(char, Box<Expr>, Box<Expr>), // a + b, a * b, ...
    Call(usize, Vec<Expr>),          // add(1, 2)   (function number, arguments)
    New(Box<Expr>),                  // new n: a fresh zeroed array of n cells (its address)
    Index(Box<Expr>, Box<Expr>),     // a[i]: cell i of array a (checked)
    Lit(String),                     // "text": a bytes array holding the text
    Size(Box<Expr>),                 // size a: the length of an array
    Keep(Box<Expr>),                 // keep a: a stays alive after its scope ends (value a)
    ScopeOpen,                       // made by `scope`: start logging allocations
    ScopeClose,                      // made by `scope`: free everything allocated since ScopeOpen
    Free(Box<Expr>),                 // free a: give an array back (value 0)
    NewB(Box<Expr>),                 // bytes n: a fresh zeroed array of n bytes
    BIndex(Box<Expr>, Box<Expr>),    // a{i}: byte i of a bytes array (checked)
    Intr(u8, Vec<Expr>),             // RingOS arcade built-in: rect(...), key(...), frame() ... (see ringvm::INTRINSICS)
}

// A statement is one line of a program.
#[derive(Debug, Clone)]
pub enum Stmt {
    Print(Expr),                    // mouth 1+2
    PrintStr(String),               // mouth "Hi"
    Assign(usize, Expr),            // x = 5   (slot, value)
    GAssign(usize, Expr),           // the same for a global
    If(Expr, Vec<Stmt>, Vec<Stmt>), // if cond / then-block / else-block (maybe empty)
    While(Expr, Vec<Stmt>),         // heart cond / block   (repeat while cond is not 0)
    Return(Expr),                   // back x
    Eval(Expr),                     // a call on its own line: hello()
    Store(Expr, Expr, Expr),        // a[i] = v   (array, index, value)
    BStore(Expr, Expr, Expr),       // a{i} = v   (stores the low 8 bits)
    Break,                          // leave the innermost loop (only made by the optimizer)
}

pub struct Func {
    pub name: String,
    pub nparams: usize, // the first `nparams` variable slots are the parameters
    pub nvars: usize,   // parameters + local variables
    pub names: Vec<String>,
    pub body: Vec<Stmt>,
}

pub struct Program {
    pub funcs: Vec<Func>,
    pub stmts: Vec<Stmt>,
    pub names: Vec<String>, // names[slot] = the variable's name
}

// How tightly each operator grabs its neighbours. Bigger = tighter.
fn power(op: char) -> u8 {
    match op {
        'O' => 1, // or
        'A' => 2, // and
        'q' | 'n' | '<' | '>' | 'l' | 'g' => 3, // comparisons: == != < > <= >=
        '+' | '-' => 4,
        '*' | '/' | '%' => 5,
        _ => 0, // not a binary operator
    }
}

const KEYWORDS: [&str; 13] = ["mouth", "if", "else", "heart", "back", "new", "bytes", "size", "free", "scope", "for", "const", "global"];
/// Words from older Ring versions (typed input, files, windows, network): refused with a clear message.
const RETIRED: [&str; 21] = ["ear", "hear", "eye", "hand", "voice", "same", "join", "listen", "wait", "recv", "send", "close", "sendfile", "filesize", "window", "button", "label", "event", "settext", "edit", "gettext"];

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    names: Vec<String>,            // every variable slot ever made (in the current function)
    visible: Vec<(String, usize)>, // names usable right now (a block's own names end with it)
    sigs: Vec<(String, usize)>,    // functions known so far: name, number of parameters
    done: Vec<Func>,               // finished functions
    in_func: bool,
    scopes: usize, // how many `scope` blocks we are inside (in the current function)
    consts: Vec<(String, i64)>, // `const NAME 5`: names that stand for a fixed number
    globals: Vec<(String, usize)>, // `global x`: main-program variables that functions can use too
}

impl Parser {
    fn lookup(&self, name: &str) -> Option<usize> {
        self.visible.iter().rev().find(|(n, _)| n == name).map(|(_, s)| *s)
    }

    /// A variable by name: this function's own first, then the shared `global`s.
    fn var(&self, name: &str) -> Option<Expr> {
        self.lookup(name).map(Expr::Var).or_else(|| self.globals.iter().find(|(n, _)| n == name).map(|(_, s)| Expr::GVar(*s)))
    }

    fn peek(&self) -> &Tok {
        &self.toks[self.i].0
    }
    fn peek2(&self) -> &Tok {
        // the token after the next one (Eof if there is none)
        &self.toks[(self.i + 1).min(self.toks.len() - 1)].0
    }
    fn pos(&self) -> usize {
        self.toks[self.i].1
    }
    fn next(&mut self) {
        self.i += 1;
    }

    // expr(min) parses operators whose power is at least `min`.
    fn expr(&mut self, min: u8) -> Result<Expr, String> {
        let mut left = self.atom()?;
        loop {
            let op = match self.peek() {
                Tok::Sym(c) | Tok::Cmp(c) if power(*c) >= min && power(*c) > 0 => *c,
                _ => break,
            };
            self.next();
            // +1 makes operators of equal power group to the left: 1-2-3 = (1-2)-3
            let right = self.expr(power(op) + 1)?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    // atom: the smallest piece: a number, a variable, a call, a negation, or a (group)
    fn atom(&mut self) -> Result<Expr, String> {
        let pos = self.pos();
        match self.peek() {
            Tok::Num(n) => {
                let n = *n;
                self.next();
                Ok(Expr::Num(n))
            }
            Tok::Str(s) => {
                let s = s.clone();
                self.next();
                Ok(Expr::Lit(s))
            }
            Tok::Ident(name) => {
                let name = name.clone();
                self.next();
                if RETIRED.contains(&name.as_str()) {
                    return Err(format!("'{name}' is not part of Ring on RingOS (see SYNTAX.md) at {pos}"));
                }
                if name == "size" {
                    return Ok(Expr::Size(Box::new(self.atom()?)));
                }
                if name == "keep" {
                    return Ok(Expr::Keep(Box::new(self.expr(1)?)));
                }
                if name == "free" {
                    return Ok(Expr::Free(Box::new(self.expr(1)?)));
                }
                if name == "bytes" {
                    return Ok(Expr::NewB(Box::new(self.expr(1)?)));
                }
                if name == "new" {
                    return Ok(Expr::New(Box::new(self.expr(1)?)));
                }
                if *self.peek() == Tok::Sym('(') {
                    return self.call(&name, pos);
                }
                if let Some((_, v)) = self.consts.iter().find(|(n, _)| *n == name) {
                    return Ok(Expr::Num(*v));
                }
                let Some(base) = self.var(&name) else {
                    return Err(format!("'{name}' is not defined at {pos}"));
                };
                if *self.peek() == Tok::Sym('[') {
                    self.next();
                    let i = self.expr(1)?;
                    if *self.peek() != Tok::Sym(']') {
                        return Err(format!("expected ']' at {}", self.pos()));
                    }
                    self.next();
                    return Ok(Expr::Index(Box::new(base), Box::new(i)));
                }
                if *self.peek() == Tok::Sym('{') {
                    self.next();
                    let i = self.expr(1)?;
                    if *self.peek() != Tok::Sym('}') {
                        return Err(format!("expected '}}' at {}", self.pos()));
                    }
                    self.next();
                    return Ok(Expr::BIndex(Box::new(base), Box::new(i)));
                }
                Ok(base)
            }
            Tok::Sym('-') => {
                self.next();
                Ok(Expr::Neg(Box::new(self.atom()?)))
            }
            Tok::Sym('(') => {
                self.next();
                let e = self.expr(1)?;
                if *self.peek() != Tok::Sym(')') {
                    return Err(format!("expected ')' at {}", self.pos()));
                }
                self.next();
                Ok(e)
            }
            t => Err(format!("expected a value at {pos}, found {t:?}")),
        }
    }

    // name(arg, arg ...)   -- the '(' is the next token
    fn call(&mut self, name: &str, pos: usize) -> Result<Expr, String> {
        if let Some(id) = ringvm::INTRINSICS.iter().position(|(n, _)| *n == name) {
            self.next(); // '('
            let mut args = Vec::new();
            if *self.peek() != Tok::Sym(')') {
                loop {
                    args.push(self.expr(1)?);
                    if *self.peek() == Tok::Sym(',') {
                        self.next();
                    } else {
                        break;
                    }
                }
            }
            if *self.peek() != Tok::Sym(')') {
                return Err(format!("expected ')' at {}", self.pos()));
            }
            self.next();
            let want = ringvm::INTRINSICS[id].1;
            if args.len() != want {
                return Err(format!("'{name}' takes {want} arguments, got {} at {pos}", args.len()));
            }
            return Ok(Expr::Intr(id as u8, args));
        }
        let Some(f) = self.sigs.iter().position(|(n, _)| n == name) else {
            return Err(format!("function '{name}' is not defined at {pos}"));
        };
        self.next(); // '('
        let mut args = Vec::new();
        if *self.peek() != Tok::Sym(')') {
            loop {
                args.push(self.expr(1)?);
                if *self.peek() == Tok::Sym(',') {
                    self.next();
                } else {
                    break;
                }
            }
        }
        if *self.peek() != Tok::Sym(')') {
            return Err(format!("expected ')' at {}", self.pos()));
        }
        self.next();
        let want = self.sigs[f].1;
        if args.len() != want {
            return Err(format!("'{name}' takes {want} arguments, got {} at {pos}", args.len()));
        }
        Ok(Expr::Call(f, args))
    }

    // an indented block: Indent, statements, Dedent
    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        if *self.peek() != Tok::Indent {
            return Err(format!("expected an indented block at {}", self.pos()));
        }
        self.next();
        let mark = self.visible.len();
        let mut v = Vec::new();
        while *self.peek() != Tok::Dedent && *self.peek() != Tok::Eof {
            v.push(self.stmt()?);
        }
        self.next(); // the Dedent
        self.visible.truncate(mark); // names made inside the block end with it
        Ok(v)
    }

    // Does a function definition start here?   name  param*  Indent
    fn is_def(&self) -> bool {
        let Tok::Ident(w) = self.peek() else { return false };
        if KEYWORDS.contains(&w.as_str()) {
            return false;
        }
        let mut j = self.i + 1;
        while matches!(self.toks[j].0, Tok::Ident(_)) {
            j += 1;
        }
        self.toks[j].0 == Tok::Indent
    }

    fn def(&mut self) -> Result<(), String> {
        let pos = self.pos();
        let Tok::Ident(name) = self.peek() else { unreachable!() };
        let name = name.clone();
        self.next();
        let mut params: Vec<String> = Vec::new();
        while let Tok::Ident(p) = self.peek() {
            if params.contains(p) {
                return Err(format!("parameter '{p}' appears twice at {}", self.pos()));
            }
            params.push(p.clone());
            self.next();
        }
        // the pre-scan in parse_with already registered it (so functions may call later ones);
        // without a pre-scan, register now so the body can at least call itself
        if self.sigs.len() <= self.done.len() {
            if self.sigs.iter().any(|(n, _)| *n == name) {
                return Err(format!("function '{name}' is already defined at {pos}"));
            }
            self.sigs.push((name.clone(), params.len()));
        }
        // the function gets its own variables: parameters are the first slots
        let saved_names = std::mem::replace(&mut self.names, params.clone());
        let saved_visible = std::mem::replace(&mut self.visible, params.iter().cloned().zip(0..).collect());
        self.in_func = true;
        let body = self.block()?;
        self.in_func = false;
        let names = std::mem::replace(&mut self.names, saved_names);
        self.visible = saved_visible;
        self.done.push(Func { name, nparams: params.len(), nvars: names.len(), names, body });
        Ok(())
    }

    fn stmt(&mut self) -> Result<Stmt, String> {
        let pos = self.pos();
        if *self.peek2() != Tok::Sym('=') && self.is_def() {
            return Err(format!("functions go at the top level, not inside a block (at {pos})"));
        }
        match self.peek() {
            Tok::Ident(w) if w == "mouth" => {
                self.next();
                if let Tok::Str(s) = self.peek() {
                    let s = s.clone();
                    self.next();
                    Ok(Stmt::PrintStr(s))
                } else {
                    Ok(Stmt::Print(self.expr(1)?))
                }
            }
            Tok::Ident(w) if w == "free" || w == "keep" => Ok(Stmt::Eval(self.expr(1)?)),
            Tok::Ident(w) if w == "back" => {
                if !self.in_func {
                    return Err(format!("'back' only works inside a function (at {pos})"));
                }
                if self.scopes > 0 {
                    return Err(format!("'back' cannot be used inside a 'scope' (at {pos}): finish the scope first, and 'keep' what you want to return"));
                }
                self.next();
                Ok(Stmt::Return(self.expr(1)?))
            }
            Tok::Ident(w) if w == "scope" => {
                self.next();
                self.scopes += 1;
                let body = self.block()?;
                self.scopes -= 1;
                // scope = open, the body, close; the `if 1` only packs them into one statement
                let mut all = vec![Stmt::Eval(Expr::ScopeOpen)];
                all.extend(body);
                all.push(Stmt::Eval(Expr::ScopeClose));
                Ok(Stmt::If(Expr::Num(1), all, Vec::new()))
            }
            Tok::Ident(w) if w == "global" => {
                // global name   (top level, before the functions that use it): a variable every function can use
                if self.in_func {
                    return Err(format!("'global' goes at the top level, before the functions (at {pos})"));
                }
                self.next();
                let Tok::Ident(name) = self.peek() else {
                    return Err(format!("'global' needs a name at {pos}"));
                };
                let name = name.clone();
                if KEYWORDS.contains(&name.as_str()) || self.consts.iter().any(|(n, _)| *n == name) {
                    return Err(format!("'{name}' cannot be a global (at {pos})"));
                }
                self.next();
                let slot = match self.lookup(&name) {
                    Some(s) => s,
                    None => {
                        self.names.push(name.clone());
                        self.visible.push((name.clone(), self.names.len() - 1));
                        self.names.len() - 1
                    }
                };
                self.globals.push((name, slot));
                Ok(Stmt::If(Expr::Num(0), Vec::new(), Vec::new()))
            }
            Tok::Ident(w) if w == "const" => {
                // const NAME 42   (or a negative number): a name for a fixed number
                self.next();
                let Tok::Ident(name) = self.peek() else {
                    return Err(format!("'const' needs a name at {pos}"));
                };
                let name = name.clone();
                if KEYWORDS.contains(&name.as_str()) || self.lookup(&name).is_some() || self.consts.iter().any(|(n, _)| *n == name) {
                    return Err(format!("'{name}' is already used at {pos}"));
                }
                self.next();
                let neg = *self.peek() == Tok::Sym('-');
                if neg {
                    self.next();
                }
                let Tok::Num(n) = self.peek() else {
                    return Err(format!("'const {name}' needs a plain number at {pos}"));
                };
                let n = if neg { -*n } else { *n };
                self.next();
                self.consts.push((name, n));
                Ok(Stmt::If(Expr::Num(0), Vec::new(), Vec::new()))
            }
            Tok::Ident(w) if w == "for" => {
                // for i 0 10   runs the block with i = 0, 1 ... 9 (the end is re-read each time round)
                self.next();
                let Tok::Ident(name) = self.peek() else {
                    return Err(format!("'for' needs a variable name at {pos}"));
                };
                let name = name.clone();
                if KEYWORDS.contains(&name.as_str()) || self.consts.iter().any(|(n, _)| *n == name) {
                    return Err(format!("'{name}' cannot be a loop variable (at {pos})"));
                }
                self.next();
                let from = self.expr(1)?;
                let to = self.expr(1)?;
                let slot = match self.lookup(&name) {
                    Some(slot) => slot,
                    None => {
                        self.names.push(name.clone());
                        self.visible.push((name, self.names.len() - 1));
                        self.names.len() - 1
                    }
                };
                let mut body = self.block()?;
                body.push(Stmt::Assign(slot, Expr::Bin('+', Box::new(Expr::Var(slot)), Box::new(Expr::Num(1)))));
                let cond = Expr::Bin('<', Box::new(Expr::Var(slot)), Box::new(to));
                Ok(Stmt::If(Expr::Num(1), vec![Stmt::Assign(slot, from), Stmt::While(cond, body)], Vec::new()))
            }
            Tok::Ident(w) if w == "heart" => {
                self.next();
                let cond = self.expr(1)?;
                Ok(Stmt::While(cond, self.block()?))
            }
            Tok::Ident(w) if w == "if" => {
                self.next();
                let cond = self.expr(1)?;
                let then = self.block()?;
                let mut other = Vec::new();
                if matches!(self.peek(), Tok::Ident(w) if w == "else") {
                    self.next();
                    if matches!(self.peek(), Tok::Ident(w) if w == "if") {
                        other.push(self.stmt()?); // else if ...
                    } else {
                        other = self.block()?;
                    }
                }
                Ok(Stmt::If(cond, then, other))
            }
            Tok::Ident(name) if *self.peek2() == Tok::Sym('=') => {
                let name = name.clone();
                if KEYWORDS.contains(&name.as_str()) || self.consts.iter().any(|(n, _)| *n == name) {
                    return Err(format!("'{name}' is a keyword or a const, it cannot be a variable (at {pos})"));
                }
                self.next();
                self.next(); // skip the name and the '='
                // parse the value BEFORE defining the name, so `x = x+1` needs x to exist
                let value = self.expr(1)?;
                if self.lookup(&name).is_none() {
                    if let Some((_, g)) = self.globals.iter().find(|(n, _)| *n == name) {
                        return Ok(Stmt::GAssign(*g, value));
                    }
                }
                let slot = match self.lookup(&name) {
                    Some(slot) => slot,
                    None => {
                        self.names.push(name.clone());
                        self.visible.push((name, self.names.len() - 1));
                        self.names.len() - 1
                    }
                };
                Ok(Stmt::Assign(slot, value))
            }
            Tok::Ident(name) if *self.peek2() == Tok::Sym('{') => {
                let name = name.clone();
                let Some(base) = self.var(&name) else {
                    return Err(format!("'{name}' is not defined at {pos}"));
                };
                self.next();
                self.next();
                let i = self.expr(1)?;
                if *self.peek() != Tok::Sym('}') {
                    return Err(format!("expected '}}' at {}", self.pos()));
                }
                self.next();
                if *self.peek() != Tok::Sym('=') {
                    return Err(format!("expected '=' after the }} at {}", self.pos()));
                }
                self.next();
                let v = self.expr(1)?;
                Ok(Stmt::BStore(base, i, v))
            }
            Tok::Ident(name) if *self.peek2() == Tok::Sym('[') => {
                let name = name.clone();
                let Some(base) = self.var(&name) else {
                    return Err(format!("'{name}' is not defined at {pos}"));
                };
                self.next();
                self.next();
                let i = self.expr(1)?;
                if *self.peek() != Tok::Sym(']') {
                    return Err(format!("expected ']' at {}", self.pos()));
                }
                self.next();
                if *self.peek() != Tok::Sym('=') {
                    return Err(format!("expected '=' after the ] at {}", self.pos()));
                }
                self.next();
                let v = self.expr(1)?;
                Ok(Stmt::Store(base, i, v))
            }
            Tok::Ident(_) if *self.peek2() == Tok::Sym('(') => Ok(Stmt::Eval(self.expr(1)?)),
            t => Err(format!("expected 'mouth', 'if', 'heart' or a name at {pos}, found {t:?}")),
        }
    }
}

// A program is a list of function definitions and statements.
pub fn parse(toks: Vec<(Tok, usize)>) -> Result<Program, String> {
    parse_with(toks, "")
}

// With the source text, all function names are collected first, so a function can call one
// defined further down (mutual recursion).
pub fn parse_with(toks: Vec<(Tok, usize)>, src: &str) -> Result<Program, String> {
    let mut p = Parser {
        toks,
        i: 0,
        names: Vec::new(),
        visible: Vec::new(),
        sigs: Vec::new(),
        done: Vec::new(),
        in_func: false,
        scopes: 0,
        consts: Vec::new(),
        globals: Vec::new(),
    };
    if !src.is_empty() {
        let mut depth = 0i32;
        for i in 0..p.toks.len() {
            match p.toks[i].0 {
                Tok::Indent => depth += 1,
                Tok::Dedent => depth -= 1,
                _ => {}
            }
            let line_start = {
                let before = &src[..p.toks[i].1.min(src.len())];
                let line = before.rsplit('\n').next().unwrap_or("");
                line.chars().all(|c| c == ' ' || c == '\t')
            };
            if depth == 0 && line_start {
                p.i = i;
                let is_assign = *p.peek2() == Tok::Sym('=');
                if !is_assign && p.is_def() {
                    let Tok::Ident(name) = p.peek() else { continue };
                    let mut n = 0;
                    while matches!(p.toks[i + 1 + n].0, Tok::Ident(_)) {
                        n += 1;
                    }
                    let name = name.clone();
                    if p.sigs.iter().any(|(m, _)| *m == name) {
                        return Err(format!("function '{name}' is already defined at {}", p.toks[i].1));
                    }
                    p.sigs.push((name, n));
                }
            }
        }
        p.i = 0;
    }
    let mut stmts = Vec::new();
    while *p.peek() != Tok::Eof {
        if *p.peek2() != Tok::Sym('=') && p.is_def() {
            p.def()?;
        } else {
            stmts.push(p.stmt()?);
        }
    }
    Ok(Program { funcs: p.done, stmts, names: p.names })
}

