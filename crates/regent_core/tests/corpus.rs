//! GPL lexer and parser against the 306 source files in the HD SDK.
//!
//! Counts come from an independent survey of 1.5.2.24's corpus, moved to
//! 1.5.2.28's by this parser's own counts. Corpus claims in
//! `docs/gpl/language.md` belong here.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use common::sdk::DOCUMENTED_BUILTINS;
use regent_core::gpl::ast::{AssignOp, ExprKind, Item, StmtKind};
use regent_core::gpl::resolve::Report;
use regent_core::gpl::{Keyword, Token, lex};

/// Expected number of source files. Three use an uppercase `.GPL` extension.
const FILES: usize = 306;

/// Expected token totals by class, excluding comments and whitespace.
const EXPECTED: &[(&str, usize)] = &[
    ("ident", 42572),
    ("keyword", 37891),
    ("(", 26982),
    (")", 26982),
    (";", 22308),
    (",", 21746),
    ("call", 19819),
    ("string", 13742),
    ("int", 9217),
    ("expr", 8914),
    ("=", 7469),
    ("'s", 7001),
    ("==", 2994),
    ("<<", 317),
    ("&&", 220),
    ("||", 131),
    ("float", 62),
    ("%", 4),
    ("/=", 2),
];

const TOTAL_TOKENS: usize = 253_176;

/// Every keyword's usage, counted from the token stream.
///
/// Pinned per keyword, since grepping raw source counts words in comments.
/// Sums to the `keyword` aggregate.
const EXPECTED_KEYWORDS: &[(Keyword, usize)] = &[
    (Keyword::Begin, 6215),
    (Keyword::End, 6215),
    (Keyword::If, 5341),
    (Keyword::Agent, 3145),
    (Keyword::Function, 2298),
    (Keyword::Declare, 2011),
    (Keyword::False, 1954),
    (Keyword::Else, 1727),
    (Keyword::Expression, 1582),
    (Keyword::Integer, 1398),
    (Keyword::Return, 1181),
    (Keyword::List, 1006),
    (Keyword::True, 813),
    (Keyword::Do, 569),
    (Keyword::Foreach, 458),
    (Keyword::In, 458),
    (Keyword::Boolean, 403),
    (Keyword::String, 394),
    (Keyword::Is, 384),
    (Keyword::Coordinate, 131),
    (Keyword::While, 111),
    (Keyword::Float, 53),
    (Keyword::Prototype, 44),
    // Keywords unused by shipped source. Workshop mods use `break`.
    (Keyword::Break, 0),
    (Keyword::Continue, 0),
    (Keyword::Define, 0),
];

fn corpus() -> Vec<(PathBuf, String)> {
    let root = common::sdk();
    common::walk(&root.join("OriginalQuests"), &["gpl"])
        .into_iter()
        .map(|p| {
            let text = std::fs::read(&p).expect("readable source file");
            (p, String::from_utf8_lossy(&text).into_owned())
        })
        .collect()
}

#[test]
fn every_file_lexes_without_error() {
    let files = corpus();

    assert_eq!(files.len(), FILES, "unexpected source file count");

    for (path, src) in &files {
        let lexed = lex(src);

        assert!(
            lexed.is_ok(),
            "{}: {:?}",
            path.display(),
            &lexed.errors[..lexed.errors.len().min(3)]
        );
    }
}

#[test]
fn only_the_garrison_modules_are_entirely_commented_out() {
    let files = corpus();

    // `TaskModules/Characters/Garrison.gpl` is all blank or `//` lines, yet
    // listed in `path.gplproj`. The expansion has its own copy.
    let mut empty: Vec<_> = files
        .iter()
        .filter(|(_, src)| lex(src).tokens.is_empty())
        .map(|(p, _)| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    empty.sort();

    assert_eq!(
        empty,
        ["Garrison.gpl", "mx_Garrison.gpl"],
        "set of token-empty files changed"
    );
}

#[test]
fn token_counts_match_the_survey() {
    let files = corpus();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total = 0;

    for (_, src) in &files {
        for t in lex(src).tokens {
            let bucket = match t.node {
                Token::Kw(_) => "keyword",
                other => other.kind(),
            };
            *counts.entry(bucket).or_default() += 1;
            total += 1;
        }
    }

    for (kind, want) in EXPECTED {
        assert_eq!(
            counts.get(kind).copied().unwrap_or(0),
            *want,
            "token count changed for {kind:?}"
        );
    }

    assert_eq!(total, TOTAL_TOKENS, "total token count changed");
}

#[test]
fn keyword_counts_match_the_token_stream() {
    let files = corpus();
    let mut counts: BTreeMap<Keyword, usize> = BTreeMap::new();

    for (_, src) in &files {
        for t in lex(src).tokens {
            if let Token::Kw(kw) = t.node {
                *counts.entry(kw).or_default() += 1;
            }
        }
    }

    for (kw, want) in EXPECTED_KEYWORDS {
        assert_eq!(
            counts.get(kw).copied().unwrap_or(0),
            *want,
            "keyword count changed for `{}`",
            kw.as_str()
        );
    }

    // Totals agree, so no keyword is missing from the table.
    let pinned: usize = EXPECTED_KEYWORDS.iter().map(|(_, n)| n).sum();
    let counted: usize = counts.values().sum();

    assert_eq!(
        pinned, counted,
        "a keyword is missing from EXPECTED_KEYWORDS"
    );
    assert_eq!(pinned, 37_891, "should equal the aggregate keyword total");
}

#[test]
fn documented_but_unused_keywords_never_appear() {
    let files = corpus();

    // `define` is documented but used nowhere, shipped or modded. `and` is
    // documented but not a keyword (see `operator_words_are_ordinary_identifiers`
    // in `gpl`).
    for (path, src) in &files {
        for t in lex(src).tokens {
            assert!(
                !matches!(t.node, Token::Kw(Keyword::Define)),
                "{} unexpectedly uses {:?}",
                path.display(),
                t.node
            );

            assert_ne!(
                t.node,
                Token::Ident("and"),
                "{} uses `and`; check whether it is an operator or a name",
                path.display()
            );
        }
    }
}

#[test]
fn no_logical_not_or_bitwise_operators_exist() {
    // Every `!` is in `!=`; `&` and `|` come only in pairs.
    //
    // Lexing succeeds everywhere, so bytes outside tokens are whitespace or
    // comments. String literals are skipped.
    let files = corpus();

    for (path, src) in &files {
        for t in lex(src).tokens {
            if matches!(t.node, Token::Str(_)) {
                continue;
            }

            let text = &src[t.span.clone()];

            for (loose, paired) in [("!", "!="), ("&", "&&"), ("|", "||")] {
                assert!(
                    !text.contains(loose) || text == paired,
                    "{}: bare `{loose}` in {text:?} at {:?}",
                    path.display(),
                    t.span
                );
            }

            for bitwise in ['^', '~'] {
                assert!(
                    !text.contains(bitwise),
                    "{}: bitwise `{bitwise}` at {:?}",
                    path.display(),
                    t.span
                );
            }
        }
    }
}

#[test]
fn spans_are_ordered_and_within_bounds() {
    let files = corpus();

    for (path, src) in &files {
        let mut prev_end = 0;

        for t in lex(src).tokens {
            assert!(
                t.span.start >= prev_end,
                "{}: spans out of order at {:?}",
                path.display(),
                t.span
            );
            assert!(
                t.span.end <= src.len(),
                "{}: span past end at {:?}",
                path.display(),
                t.span
            );
            prev_end = t.span.end;
        }
    }
}

// --------------------------------------------------------------------------
// Parser
// --------------------------------------------------------------------------

/// AST node totals, reconciled against the independent survey.
const FUNCTIONS: usize = 1967;
const PROTOTYPES: usize = 44;
const CONSTANTS: usize = 1582;
const DECLARED_NAMES: usize = 5820;

#[test]
fn every_file_parses_without_error() {
    let files = corpus();

    for (path, src) in &files {
        let parsed = regent_core::gpl::parse(src);

        assert!(
            parsed.is_ok(),
            "{}: {:?}",
            path.display(),
            &parsed.errors[..parsed.errors.len().min(3)]
        );
    }
}

#[test]
fn top_level_item_counts_match() {
    let files = corpus();
    let (mut funcs, mut protos, mut consts, mut names) = (0, 0, 0, 0);

    for (_, src) in &files {
        for item in &regent_core::gpl::parse(src).script.items {
            match item {
                Item::Function(f) => {
                    funcs += 1;
                    names += f.decls.iter().map(|d| d.names.len()).sum::<usize>();
                }
                Item::Prototype(p) => {
                    protos += 1;
                    names += p.decls.iter().map(|d| d.names.len()).sum::<usize>();
                }
                Item::Constant(_) => consts += 1,
            }
        }
    }

    assert_eq!(funcs, FUNCTIONS, "function count changed");
    assert_eq!(protos, PROTOTYPES, "prototype count changed");
    assert_eq!(consts, CONSTANTS, "constant count changed");
    // The survey's 4,172 declare lines, expanded by comma arity.
    assert_eq!(names, DECLARED_NAMES, "declared name count changed");
}

#[test]
fn statement_kind_counts_match() {
    let files = corpus();
    let mut c: BTreeMap<&str, usize> = BTreeMap::new();

    for (_, src) in &files {
        regent_core::gpl::parse(src).script.visit_stmts(&mut |s| {
            let k = match &s.kind {
                StmtKind::Expr(_) => "expr",
                StmtKind::Block(_) => "block",
                StmtKind::If { .. } => "if",
                StmtKind::While { .. } => "while",
                StmtKind::DoWhile { .. } => "do-while",
                StmtKind::Foreach { .. } => "foreach",
                StmtKind::Break => "break",
                StmtKind::Continue => "continue",
                StmtKind::Return(Some(_)) => "return-value",
                StmtKind::Return(None) => "return-bare",
            };
            *c.entry(k).or_default() += 1;
        });
    }

    // Survey counts.
    assert_eq!(c["expr"], 16953);
    assert_eq!(c["if"], 5341);
    assert_eq!(c["foreach"], 458);
    assert_eq!(c["while"], 111);
    assert_eq!(c["return-value"], 945);
    assert_eq!(c["return-bare"], 236);

    // 6,215 `begin`/`end` pairs minus the 2,011 definition bodies the AST
    // stores directly.
    assert_eq!(c["block"], 4204);
    assert_eq!(c["block"] + FUNCTIONS + PROTOTYPES, 6215);

    // No shipped source uses these forms.
    assert_eq!(c.get("break"), None);
    assert_eq!(c.get("continue"), None);
    assert_eq!(c.get("do-while"), None);
}

#[test]
fn expression_counts_match_the_token_stream() {
    let files = corpus();
    let (mut func_refs, mut attrs, mut appends, mut assigns, mut postfix) = (0, 0, 0, 0, 0);

    for (_, src) in &files {
        regent_core::gpl::parse(src)
            .script
            .visit_exprs(&mut |e| match &e.kind {
                ExprKind::Func(_) => func_refs += 1,
                ExprKind::Attr { .. } => attrs += 1,
                ExprKind::Postfix { .. } => postfix += 1,
                ExprKind::Assign { op, .. } => match op {
                    AssignOp::Append => appends += 1,
                    AssignOp::Assign => assigns += 1,
                    _ => {}
                },
                _ => {}
            });
    }

    // Each equals its lexer token count: the parser drops and invents nothing.
    assert_eq!(func_refs, 19819, "every `$name` token should reach the AST");
    assert_eq!(attrs, 7001, "one `Attr` node per `'s` token");
    assert_eq!(appends, 317);
    assert_eq!(assigns, 7469);
    assert_eq!(postfix, 113, "80 `++` plus 33 `--`");
}

// --------------------------------------------------------------------------
// Resolution
// --------------------------------------------------------------------------

#[test]
fn every_function_reference_resolves() {
    let files = corpus();
    let mut report = Report::default();

    for (_, src) in &files {
        report.add(&regent_core::gpl::parse(src).script);
    }

    let unresolved = report.unresolved();

    assert!(
        unresolved.is_empty(),
        "unresolved `$name` references: {unresolved:?}"
    );

    let (script, builtin, none) = report.counts();

    assert_eq!(none, 0);
    assert_eq!(script, 6152, "calls to corpus functions");
    assert_eq!(builtin, 13667, "calls to documented builtins");
    assert_eq!(script + builtin, 19819, "should account for every `$name`");

    assert_eq!(report.defined.len(), 1152, "distinct function names");
    // Base and expansion compile separately; the Mx tree redefines most names.
    assert_eq!(report.duplicate_definitions.len(), FUNCTIONS - 1152);
}

#[test]
fn the_builtin_inventory_is_complete() {
    // 162 from the reference index, plus 4 only the Notepad++ template lists.
    // `RemoveAllBuildingLimits` is also in the reference body, not its index.
    assert_eq!(DOCUMENTED_BUILTINS.len(), 166, "builtin inventory changed");

    let files = corpus();
    let mut report = Report::default();

    for (_, src) in &files {
        report.add(&regent_core::gpl::parse(src).script);
    }

    // Unused by the shipped game. The other 141 are what the interpreter must
    // provide.
    let unused = report.unused_builtins();
    assert_eq!(unused.len(), 25);

    // 16 of the 141 run natively in `gpl::vm`; 125 are world builtins.
    let native = regent_core::gpl::builtins::HD
        .iter()
        .filter(|b| b.native.is_some() && !b.name.is_empty() && !unused.contains(&b.name))
        .count();
    assert_eq!(native, 16);
}

// --------------------------------------------------------------------------
// .dat prototype files
// --------------------------------------------------------------------------

#[test]
fn every_dat_file_parses() {
    let root = common::sdk();

    let paths = common::walk(&root, &["dat"]);

    assert_eq!(paths.len(), 15, "unexpected .dat file count");

    let (mut protos, mut values) = (0, 0);

    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let src = String::from_utf8_lossy(&bytes);
        let parsed = regent_core::gpl::dat::parse(&src);

        assert!(parsed.is_ok(), "{}: {:?}", path.display(), parsed.errors);

        protos += parsed.prototypes.len();
        values += parsed
            .prototypes
            .iter()
            .map(|p| p.values.len())
            .sum::<usize>();
    }

    assert_eq!(protos, 417, "prototype count changed");
    assert_eq!(values, 5176, "key/value count changed");
}
