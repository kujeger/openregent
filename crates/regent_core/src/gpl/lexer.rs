//! Turns GPL source into a token stream.
//!
//! Collects errors rather than stopping at the first, so one pass over a
//! directory shows every unrecognised construct.

use logos::Logos;

use super::token::{Keyword, Token};

/// A byte range within the source file.
pub type Span = std::ops::Range<usize>;

/// A value paired with the source range it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    pub node: T,
    pub span: Span,
}

/// A run of source text the lexer could not recognise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub span: Span,
    /// The offending text, truncated for display.
    pub text: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unrecognised input {:?}", self.text)
    }
}

impl std::error::Error for LexError {}

/// The result of lexing one file.
#[derive(Debug, Clone, Default)]
pub struct Lexed<'a> {
    pub tokens: Vec<Spanned<Token<'a>>>,
    /// Errors, at most `MAX_ERRORS` of them.
    pub errors: Vec<LexError>,
    /// Errors beyond the cap, counted but not recorded.
    pub suppressed: usize,
}

impl Lexed<'_> {
    pub const fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Every error found, including those past the cap.
    pub const fn error_count(&self) -> usize {
        self.errors.len() + self.suppressed
    }

    /// Whether the cap was reached, which usually means the input is not GPL.
    pub const fn is_flooded(&self) -> bool {
        self.suppressed > 0
    }
}

/// How much of an unrecognised run to keep for the error message.
const ERROR_TEXT_LIMIT: usize = 32;

/// Most errors recorded for one file.
///
/// Real mistakes stay well under it. A non-GPL file yields about one error per
/// five bytes: worth counting, not printing.
pub const MAX_ERRORS: usize = 20;

/// Tokenise GPL source.
///
/// Identifiers matching a reserved word become [`Token::Kw`] here, so build
/// token streams only through this. The lookup runs after the DFA to keep the
/// keyword set in one table, not in pattern attributes.
pub fn lex(src: &str) -> Lexed<'_> {
    let mut out = Lexed::default();
    let mut lexer = Token::lexer(src);

    while let Some(result) = lexer.next() {
        let span = lexer.span();

        match result {
            Ok(Token::Ident(word)) => {
                let node = match Keyword::lookup(word) {
                    Some(kw) => Token::Kw(kw),
                    None => Token::Ident(word),
                };
                out.tokens.push(Spanned { node, span });
            }

            Ok(node) => out.tokens.push(Spanned { node, span }),

            // Past the cap: count, don't record. Keep lexing so the token
            // stream stays complete.
            Err(()) => super::diag::capped(&mut out.errors, &mut out.suppressed, || LexError {
                span,
                text: lexer.slice().chars().take(ERROR_TEXT_LIMIT).collect(),
            }),
        }
    }

    out
}
