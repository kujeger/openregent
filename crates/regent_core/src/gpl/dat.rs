//! Reader for `.dat` prototype-data files.
//!
//! Grammar, recovered from `DataFileParsers` in `Gplbcc.exe`:
//!
//! ```ebnf
//! data_file        : ( Id_block_tag Initializer_list End_block_tag )* ;
//! Id_block_tag     : '[' IDENT ']' ;
//! End_block_tag    : '[' 'end' ']' ;
//! Initializer_list : '{' IDENT ( Associative_pair )* '}' ;
//! Associative_pair : '(' IDENT Basic_type ')' ;
//! Basic_type       : IDENT | STRING | INT | FLOAT | 'true' | 'false' ;
//! ```
//!
//! A `.dat` file declares named instances: the identifier after `{` names a
//! `.gpl` `prototype`; the pairs give member values. No inheritance.
//!
//! Own scanner: `.dat` has `[]`, `{}`, whitespace-separated values, no `;`, no
//! operators, no comments.
//!
//! 15 shipped files hold 417 prototypes. The compiler also admits quoted
//! strings, commas and nested initializer lists, none used by shipped files.
//! Only quoted strings are handled here, untested against real data.

use super::lexer::Span;
use super::parser::ParseError;

/// One `[Name] { Shape (k v)... } [end]` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Prototype<'a> {
    /// The instance name, from `[Name]`.
    pub name: &'a str,
    /// The `prototype` in the `.gpl` sources this instantiates.
    pub shape: &'a str,
    /// Member values, in source order. Five shipped cases repeat a key
    /// (case-insensitively); both are kept.
    /// [`crate::gpl::emit::data_section`] applies first-wins.
    pub values: Vec<(&'a str, Value<'a>)>,
    pub span: Span,
}

/// A member value. Everything is unquoted and space-free in shipped data.
#[derive(Debug, Clone, PartialEq)]
pub enum Value<'a> {
    Bool(bool),
    Int(i64),
    Float(&'a str),
    /// An identifier, enum member, or the name of a GPL function.
    Word(&'a str),
    /// A quoted string. Accepted by the compiler; unused by shipped data.
    Str(&'a str),
}

#[derive(Debug, Clone, Default)]
pub struct DataFile<'a> {
    pub prototypes: Vec<Prototype<'a>>,
    pub errors: Vec<ParseError>,
    pub suppressed: usize,
}

impl DataFile<'_> {
    pub const fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Parse a `.dat` file.
pub fn parse(src: &str) -> DataFile<'_> {
    let mut p = Scanner {
        src,
        pos: 0,
        out: DataFile::default(),
    };

    p.data_file();
    p.out
}

struct Scanner<'a> {
    src: &'a str,
    pos: usize,
    out: DataFile<'a>,
}

impl<'a> Scanner<'a> {
    fn skip_ws(&mut self) {
        self.pos += self.src[self.pos..]
            .find(|c: char| !c.is_ascii_whitespace())
            .unwrap_or(self.src.len() - self.pos);
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_ws();
        self.src[self.pos..].chars().next()
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(c);

        if hit {
            self.pos += c.len_utf8();
        }

        hit
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        super::diag::capped(&mut self.out.errors, &mut self.out.suppressed, || {
            ParseError {
                span,
                message: message.into(),
            }
        });
    }

    /// A run of characters up to whitespace or a delimiter.
    fn word(&mut self) -> Option<(&'a str, Span)> {
        self.skip_ws();
        let start = self.pos;
        let rest = &self.src[start..];
        let len = rest
            .find(|c: char| c.is_ascii_whitespace() || "[](){}".contains(c))
            .unwrap_or(rest.len());

        if len == 0 {
            return None;
        }

        self.pos += len;

        Some((&rest[..len], start..self.pos))
    }

    fn expect(&mut self, c: char, ctx: &str) -> bool {
        if self.eat(c) {
            return true;
        }

        let found = self
            .peek()
            .map_or("end of file".into(), |c| format!("`{c}`"));
        let span = self.pos..self.pos + 1;
        self.error(span, format!("expected `{c}` {ctx}, found {found}"));

        false
    }

    fn data_file(&mut self) {
        while self.peek().is_some() {
            let start = self.pos;

            if !self.prototype() {
                // Skip to the next block so one bad entry does not cascade.
                match self.src[self.pos..].find('[') {
                    Some(n) if self.pos + n > start => self.pos += n,
                    _ => return,
                }
            }

            if self.pos == start {
                return;
            }
        }
    }

    /// `[Name] { Shape (k v)... } [end]`
    fn prototype(&mut self) -> bool {
        let start = self.pos;

        if !self.expect('[', "to open a prototype") {
            return false;
        }

        let Some((name, _)) = self.word() else {
            let span = self.pos..self.pos + 1;
            self.error(span, "expected a prototype name after `[`");
            return false;
        };

        if !self.expect(']', "after a prototype name") || !self.expect('{', "to open a body") {
            return false;
        }

        let Some((shape, _)) = self.word() else {
            let span = self.pos..self.pos + 1;
            self.error(span, "expected a shape name after `{`");
            return false;
        };

        let mut values = Vec::new();

        while self.eat('(') {
            let Some((key, _)) = self.word() else {
                let span = self.pos..self.pos + 1;
                self.error(span, "expected a key after `(`");
                return false;
            };

            let Some(value) = self.value() else {
                let span = self.pos..self.pos + 1;
                self.error(span, format!("expected a value for `{key}`"));
                return false;
            };

            if !self.expect(')', "after a key and value") {
                return false;
            }

            values.push((key, value));
        }

        if !self.expect('}', "to close a body") {
            return false;
        }

        // `[end]`, always lowercase in shipped data.
        if !self.expect('[', "to open the terminator") {
            return false;
        }

        match self.word() {
            Some((w, span)) if w.eq_ignore_ascii_case("end") => {
                if !self.expect(']', "after `end`") {
                    return false;
                }
                let _ = span;
            }
            Some((w, span)) => {
                self.error(span, format!("expected `end`, found `{w}`"));
                return false;
            }
            None => {
                let span = self.pos..self.pos + 1;
                self.error(span, "expected `end`");
                return false;
            }
        }

        self.out.prototypes.push(Prototype {
            name,
            shape,
            values,
            span: start..self.pos,
        });

        true
    }

    fn value(&mut self) -> Option<Value<'a>> {
        if self.peek() == Some('"') {
            self.pos += 1;
            let start = self.pos;
            let len = self.src[start..].find('"')?;
            self.pos = start + len + 1;

            return Some(Value::Str(&self.src[start..start + len]));
        }

        let (w, _) = self.word()?;

        Some(classify(w))
    }
}

/// Values are untyped text; the type follows from the spelling.
fn classify(w: &str) -> Value<'_> {
    if w.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if w.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else if let Ok(n) = w.parse::<i64>() {
        Value::Int(n)
    } else if w.parse::<f64>().is_ok() && w.contains('.') {
        Value::Float(w)
    } else {
        Value::Word(w)
    }
}
