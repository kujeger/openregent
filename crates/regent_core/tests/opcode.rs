//! The VM's opcode table, against the lexer and the shipped bytecode.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;

use regent_core::gpl::opcode;

/// The lexer, built from the SDK reference and sources, is an independent
/// answer key. The five unspelled opcodes are the compiler's own.
#[test]
fn every_opcode_token_is_one_the_lexer_knows() {
    // Lexed, not compared against a copied token list, so it cannot drift.
    let mut known: BTreeSet<&str> = BTreeSet::new();

    for spelling in [
        "=",
        "*=",
        "/=",
        "%=",
        "+=",
        "-=",
        "<<",
        "||",
        "&&",
        "==",
        "!=",
        "<",
        ">",
        "<=",
        ">=",
        "+",
        "-",
        "*",
        "/",
        "%",
        "'s",
        "++",
        "--",
        "if",
        "foreach",
        "while",
        "continue",
        "break",
        "return",
        "declare",
        "call",
        "declare return",
    ] {
        let lexed = regent_core::gpl::lex(spelling);

        assert!(
            lexed.errors.is_empty(),
            "the lexer rejects {spelling:?}, which the engine's table spells"
        );

        known.insert(spelling);
    }

    let silent: Vec<&str> = opcode::TABLE
        .iter()
        .filter(|op| op.token.is_empty())
        .map(|op| op.name)
        .collect();

    assert_eq!(
        silent,
        ["opNop", "opLocalize", "opAdvance", "opRewind", "opExit"],
        "a different set of opcodes has no spelling"
    );

    for op in &opcode::TABLE {
        if !op.token.is_empty() {
            assert!(
                known.contains(op.token),
                "{} spells {:?}, which the lexer does not know",
                op.name,
                op.token
            );
        }
    }
}

/// Names are unique, so a position can stand for an index.
#[test]
fn no_opcode_is_named_twice() {
    let unique: BTreeSet<&str> = opcode::TABLE.iter().map(|op| op.name).collect();

    assert_eq!(unique.len(), opcode::TABLE.len());
}

/// `gpl::bcd` found `0x26` closes an operand list in compiled files; table
/// entry 38 is `opLParen`, spelled `call`. Two independent routes agree.
#[test]
fn the_call_terminator_is_the_call_opcode() {
    assert_eq!(opcode::CALL, 0x26);
    assert_eq!(opcode::TABLE[usize::from(opcode::CALL)].name, "opLParen");
    assert_eq!(opcode::TABLE[usize::from(opcode::CALL)].token, "call");
}

/// The shipped bytecode never uses the three opcodes the name table lacks:
/// 39 is the highest in all 84 `.bcd` files.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_shipped_bytecode_uses_the_inline_attribute_opcodes() {
    use regent_core::data::bcd;
    use regent_core::gpl::bcd::walk;

    let roots = [common::sdk(), common::workshop(), common::hd()];
    let mut paths: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| common::walk(root, &["bcd"]))
        .collect();

    paths.sort();
    paths.dedup();

    let mut highest = 0;

    for path in &paths {
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = walk(sections.symbols.body).expect("the code walks");

        let top = walked
            .iter()
            .flat_map(|d| &d.records)
            .map(|r| r.opcode)
            .max();
        highest = highest.max(top.unwrap_or_default());
    }

    assert_eq!(paths.len(), 75, "the .bcd files of every edition and mod");
    assert_eq!(highest, opcode::BODY);
}
