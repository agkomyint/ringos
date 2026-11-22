// The lexer: turns source text into a list of tokens.
//   "1+2"  ->  Num(1)  Sym('+')  Num(2)  Eof

#[derive(Debug, PartialEq)]
pub enum Tok {
    Num(i64),      // 42
    Ident(String), // fib, mouth, x   (keywords are identifiers for now)
    Str(String),   // "Hi"
    Sym(char),     // + - * / % = ( )
    Cmp(char),     // comparison: < > and  q (==)  n (!=)  l (<=)  g (>=)
    Indent,        // a line indented deeper than the one before
    Dedent,        // back out one level
    Eof,           // end of input
}

// Each token remembers where it started (byte position), so errors can point at it.
pub fn lex(src: &str) -> Result<Vec<(Tok, usize)>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut levels = vec![0usize]; // indentation levels currently open
    let (mut at_start, mut col) = (true, 0);

    while i < b.len() {
        let c = b[i];
        let start = i;

        if c == b'\n' {
            at_start = true;
            col = 0;
            i += 1;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' {
            if at_start && c != b'\r' {
                col += 1;
            }
            i += 1;
            continue;
        }
        if c == b'#' {
            // a comment runs to the end of the line; a line with only a comment does not
            // count for indentation (the newline is handled on the next turn)
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if at_start {
            // first token on a line: compare its indentation with the open levels
            at_start = false;
            if col > *levels.last().unwrap() {
                levels.push(col);
                out.push((Tok::Indent, start));
            }
            while col < *levels.last().unwrap() {
                levels.pop();
                if col > *levels.last().unwrap() {
                    return Err(format!("inconsistent indentation at {start}"));
                }
                out.push((Tok::Dedent, start));
            }
        }

        if c.is_ascii_digit() {
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let n = src[start..i]
                .parse()
                .map_err(|_| format!("number too big at {start}"))?;
            out.push((Tok::Num(n), start));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            match &src[start..i] {
                "and" => out.push((Tok::Cmp('A'), start)), // A = and, O = or: the loosest operators
                "or" => out.push((Tok::Cmp('O'), start)),
                w => out.push((Tok::Ident(w.to_string()), start)),
            }
        } else if c == b'"' {
            i += 1;
            let mut text: Vec<u8> = Vec::new();
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' && i + 1 < b.len() && matches!(b[i + 1], b'n' | b't' | b'r' | b'"' | b'\\') {
                    text.push(match b[i + 1] {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        c => c,
                    });
                    i += 2;
                } else {
                    text.push(b[i]);
                    i += 1;
                }
            }
            if i >= b.len() {
                return Err(format!("string opened at {start} is never closed"));
            }
            out.push((Tok::Str(String::from_utf8_lossy(&text).into_owned()), start));
            i += 1; // skip the closing quote
        } else if matches!(c, b'=' | b'<' | b'>' | b'!') {
            let two = i + 1 < b.len() && b[i + 1] == b'=';
            let tok = match (c, two) {
                (b'=', true) => Tok::Cmp('q'),
                (b'=', false) => Tok::Sym('='),
                (b'<', true) => Tok::Cmp('l'),
                (b'<', false) => Tok::Cmp('<'),
                (b'>', true) => Tok::Cmp('g'),
                (b'>', false) => Tok::Cmp('>'),
                (_, true) => Tok::Cmp('n'),
                (_, false) => return Err(format!("unexpected '!' at {start}")),
            };
            out.push((tok, start));
            i += if two { 2 } else { 1 };
        } else if b"+-*/%(),[]{}".contains(&c) {
            out.push((Tok::Sym(c as char), start));
            i += 1;
        } else {
            return Err(format!("unexpected '{}' at {start}", c as char));
        }
    }

    for _ in 1..levels.len() {
        out.push((Tok::Dedent, i)); // close blocks still open at the end
    }
    out.push((Tok::Eof, i));
    Ok(out)
}
