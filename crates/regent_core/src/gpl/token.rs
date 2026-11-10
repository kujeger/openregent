//! The GPL token set.
//!
//! **Corpus**: every pattern is backed by a lexical survey of the 306 `.gpl`
//! files in the Majesty HD modding SDK. See `docs/gpl/language.md`.
//!
//! - Case-insensitive: `TRUE`, `True`, `true` all occur, as do `ThisAGent` and
//!   `THisAgent`. Keywords go through [`Keyword::lookup`], not a pattern each.
//! - No escape sequences: no backslash anywhere in the corpus. A string literal
//!   is `"[^"\r\n]*"`.

use logos::Logos;

/// A reserved word, control or type name; GPL does not distinguish them
/// lexically.
///
/// The set matches the token enum recovered from `Gplbcc.exe`, not the language
/// reference, which lists `agentref`: the compiler has no token for it.
///
/// `and`, `or`, `mod`, `not` are absent. The compiler's `AND`, `OR`, `MOD`,
/// `NOT` name `&&`, `||`, `%`, `!`, like `NOTEQUAL`, `MODEQUAL`, `INSERTER`.
/// Several Workshop mods declare `agent mod;` and compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Keyword {
    // Control and structure.
    Begin,
    Break,
    Continue,
    Declare,
    Define,
    Do,
    Else,
    End,
    Expression,
    False,
    Foreach,
    Function,
    If,
    In,
    Is,
    Prototype,
    Return,
    True,
    While,

    // Type names.
    Agent,
    Boolean,
    Coordinate,
    Float,
    Integer,
    List,
    String,
}

/// Every keyword paired with its lowercase spelling.
///
/// `define` is a compiler token no source uses, shipped or modded. Kept
/// because the language reference lists it.
const KEYWORDS: &[(&str, Keyword)] = &[
    ("begin", Keyword::Begin),
    ("break", Keyword::Break),
    ("continue", Keyword::Continue),
    ("declare", Keyword::Declare),
    ("define", Keyword::Define),
    ("do", Keyword::Do),
    ("else", Keyword::Else),
    ("end", Keyword::End),
    ("expression", Keyword::Expression),
    ("false", Keyword::False),
    ("foreach", Keyword::Foreach),
    ("function", Keyword::Function),
    ("if", Keyword::If),
    ("in", Keyword::In),
    ("is", Keyword::Is),
    ("prototype", Keyword::Prototype),
    ("return", Keyword::Return),
    ("true", Keyword::True),
    ("while", Keyword::While),
    ("agent", Keyword::Agent),
    ("boolean", Keyword::Boolean),
    ("coordinate", Keyword::Coordinate),
    ("float", Keyword::Float),
    ("integer", Keyword::Integer),
    ("list", Keyword::List),
    ("string", Keyword::String),
];

impl Keyword {
    /// Resolve a word to a keyword, ignoring case. `None` for an ordinary
    /// identifier.
    pub fn lookup(word: &str) -> Option<Keyword> {
        KEYWORDS
            .iter()
            .find(|(spelling, _)| spelling.eq_ignore_ascii_case(word))
            .map(|(_, kw)| *kw)
    }

    /// The canonical lowercase spelling, for diagnostics.
    pub fn as_str(self) -> &'static str {
        KEYWORDS
            .iter()
            .find(|(_, kw)| *kw == self)
            .map_or("?", |(spelling, _)| *spelling)
    }

    /// Whether this keyword names a type.
    ///
    /// `function` answers true: it also is a valid member type.
    pub const fn is_type(self) -> bool {
        matches!(
            self,
            Keyword::Agent
                | Keyword::Boolean
                | Keyword::Coordinate
                | Keyword::Float
                | Keyword::Function
                | Keyword::Integer
                | Keyword::List
                | Keyword::String
        )
    }
}

/// A single GPL token, borrowing its text from the source.
///
/// Whitespace and `//` comments are skipped and never appear here.
///
/// `logos` never produces `Kw`: [`super::lex`] rewrites identifiers matching
/// [`Keyword::lookup`], so build streams only through `lex`.
#[derive(Logos, Debug, Clone, Copy, PartialEq)]
#[logos(skip r"[ \t\r\n\x0c]+")]
// A `//` comment runs to end of line; opt out of logos' greedy-repetition lint.
#[logos(skip(r"//[^\r\n]*", allow_greedy = true))]
pub enum Token<'a> {
    /// A reserved word. Synthesised from `Ident` by the driver.
    Kw(Keyword),

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*")]
    Ident(&'a str),

    /// `$Name` — a call or reference to a function, builtin or script-defined.
    #[regex(r"\$[A-Za-z_][A-Za-z0-9_]*", |lex| &lex.slice()[1..])]
    Call(&'a str),

    /// `#Name`, a named constant. Resolved at bytecode load, not compile time,
    /// so it stays a name here.
    #[regex(r"#[A-Za-z_][A-Za-z0-9_]*", |lex| &lex.slice()[1..])]
    Expr(&'a str),

    /// A string literal, quotes removed. Leading and trailing spaces count.
    #[regex(r#""[^"\r\n]*""#, |lex| { let s = lex.slice(); &s[1..s.len() - 1] })]
    Str(&'a str),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    Int(i64),

    /// A float literal, kept as written.
    ///
    /// Forms: `100.0`, `100.`, `.5`. GPL floats are fixed point (2^23 integer
    /// portion, 1/255 precision), so converting via `f64` here would round.
    #[regex(r"[0-9]+\.[0-9]*|\.[0-9]+", |lex| lex.slice())]
    Float(&'a str),

    /// `'s` — the possessive attribute accessor, as in `palace's "gold"`.
    #[token("'s")]
    Poss,

    #[token("=")]
    Assign,
    #[token("==")]
    Eq,
    #[token("!=")]
    Ne,
    #[token("<")]
    Lt,
    #[token(">")]
    Gt,
    #[token("<=")]
    Le,
    #[token(">=")]
    Ge,

    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,

    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,

    #[token("+=")]
    PlusEq,
    #[token("-=")]
    MinusEq,
    #[token("*=")]
    StarEq,
    #[token("/=")]
    SlashEq,
    /// `MODEQUAL` in the compiler's token enum. No shipped source uses it.
    #[token("%=")]
    PercentEq,

    #[token("++")]
    PlusPlus,
    #[token("--")]
    MinusMinus,

    /// `<<` — appends to a list. GPL has no bitwise operators, so no shift.
    #[token("<<")]
    Append,

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token(";")]
    Semi,
}

impl Token<'_> {
    /// A short name for the token's class, for histograms and errors.
    pub fn kind(&self) -> &'static str {
        match self {
            Token::Kw(kw) => kw.as_str(),
            Token::Ident(_) => "ident",
            Token::Call(_) => "call",
            Token::Expr(_) => "expr",
            Token::Str(_) => "string",
            Token::Int(_) => "int",
            Token::Float(_) => "float",
            Token::Poss => "'s",
            Token::Assign => "=",
            Token::Eq => "==",
            Token::Ne => "!=",
            Token::Lt => "<",
            Token::Gt => ">",
            Token::Le => "<=",
            Token::Ge => ">=",
            Token::AndAnd => "&&",
            Token::OrOr => "||",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
            Token::PlusEq => "+=",
            Token::MinusEq => "-=",
            Token::StarEq => "*=",
            Token::SlashEq => "/=",
            Token::PercentEq => "%=",
            Token::PlusPlus => "++",
            Token::MinusMinus => "--",
            Token::Append => "<<",
            Token::LParen => "(",
            Token::RParen => ")",
            Token::Comma => ",",
            Token::Semi => ";",
        }
    }
}
