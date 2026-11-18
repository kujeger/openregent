//! GPL forms the shipped scripts never use, but mods do: `break`,
//! `continue`, a `return` inside a `foreach`, computed and chained attribute
//! names, `mod` as a name, a negative constant; and an add-on image loaded
//! over a base one.
//!
//! The fixtures are made up, under `tests/fixtures/gpl/`. The record shapes
//! asserted are those [docs/formats/bytecode.md] documents; `gplbcc_diff.rs`
//! holds the same projects against `Gplbcc.exe` itself.

mod common;

use std::path::{Path, PathBuf};

use regent_core::content::dataset;
use regent_core::data::bcd::Container;
use regent_core::data::gpltype::TypeCode;
use regent_core::gpl::ast::{Expr, ExprKind, StmtKind};
use regent_core::gpl::bcd::{CONSTANT_RECORD, Compiled, NO_LINE, Record, walk};
use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::{Entry, Missing, Program};
use regent_core::gpl::opcode::Op;
use regent_core::gpl::ops::Value;
use regent_core::gpl::resolve::Report;
use regent_core::gpl::runtime::Catcher;
use regent_core::gpl::vm::{Checks, Config, Crash, DebugOut, Null, Vm};
use regent_core::gpl::{Token, emit, lex};

use common::log_host::LogHost;

/// The project of the forms mods use.
const FORMS: &str = "forms/forms.gplproj";

/// A base image, with a constant, two functions and a prototype.
const BASE: &str = "base/base.gplproj";

/// An add-on over [`BASE`].
const ADDON: &str = "addon/addon.gplproj";

/// A `#Name` no fixture defines: a host's to answer.
const TEXT_ID: &str = "#ZQ_TEXT_ID";

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gpl")
        .join(path)
}

fn source(path: &str) -> String {
    std::fs::read_to_string(fixture(path)).expect("a fixture")
}

/// A project compiled by `gpl::emit`, as `regent_cli compile` does.
fn image(project: &str) -> Vec<u8> {
    emit::compile_project(&fixture(project)).unwrap_or_else(|e| panic!("{project}: {e}"))
}

/// Section 1 of a compiled image. Leaked: a test's records borrow it.
fn definitions(project: &str) -> Vec<Compiled<'static>> {
    let bytes: &'static [u8] = Box::leak(image(project).into_boxed_slice());
    let program: &'static Container<'static> =
        Box::leak(Box::new(Container::parse(bytes).expect("a program")));
    let sections = program.sections().expect("three sections");

    walk(sections.symbols.body).expect("section 1 walks")
}

fn records(project: &str, name: &str) -> Vec<Record<'static>> {
    definitions(project)
        .into_iter()
        .find(|d| d.name == name)
        .unwrap_or_else(|| panic!("{name}: not compiled"))
        .records
}

fn op(record: &Record<'_>) -> Op {
    Op::from_code(record.opcode).expect("a named opcode")
}

/// Indices of the records of `which`.
fn find(records: &[Record<'_>], which: Op) -> Vec<usize> {
    (0..records.len())
        .filter(|&at| op(&records[at]) == which)
        .collect()
}

/// The record a value slot names, as a record index.
const fn named(record: &Record<'_>, slot: usize) -> usize {
    record.values[slot].word as usize
}

// ---------------------------------------------------------------------------
// The source forms

/// `break` and `continue` parse as their own statements; neither is in a
/// shipped source.
#[test]
fn break_and_continue_parse_as_statements() {
    let src = source("forms/forms.gpl");
    let script = regent_core::gpl::parse(&src).script;
    let (mut breaks, mut continues) = (0, 0);

    script.visit_stmts(&mut |s| match s.kind {
        StmtKind::Break => breaks += 1,
        StmtKind::Continue => continues += 1,
        _ => {}
    });

    assert_eq!((breaks, continues), (1, 1));
}

/// An attribute name is any expression: a variable, `mod's key`, or a
/// bracketed one, `mod's ("ZqTier" + tier)`. Accessors chain:
/// `who's "ZqHome"'s "ZqGold"`.
#[test]
fn attribute_names_are_any_expression() {
    let src = source("forms/forms.gpl");
    let script = regent_core::gpl::parse(&src).script;
    let (mut literal, mut variable, mut computed, mut chained) = (0, 0, 0, 0);

    script.visit_exprs(&mut |e: &Expr<'_>| {
        if let ExprKind::Attr { obj, attr } = &e.kind {
            match attr.kind {
                ExprKind::Str(_) => literal += 1,
                ExprKind::Ident(_) => variable += 1,
                _ => computed += 1,
            }

            if matches!(obj.kind, ExprKind::Attr { .. }) {
                chained += 1;
            }
        }
    });

    assert_eq!((literal, variable, computed, chained), (3, 1, 1, 1));
}

/// Each accessor is one `opApostrophe`: its first slot the object, its
/// second the name, a literal string or the record computing it.
#[test]
fn an_attribute_name_compiles_to_its_operand() {
    let records = records(FORMS, "ZqReach");
    let reads = find(&records, Op::Apostrophe);
    let local = |name: &str| {
        (0..records.len())
            .find(|&at| records[at].text == name)
            .expect("declared")
    };

    assert_eq!(reads.len(), 5, "accessors");

    // `mod's key`: the name is the parameter's slot.
    let by_variable = reads[1];
    assert_eq!(named(&records[by_variable], 0), local("mod"));
    assert_eq!(named(&records[by_variable], 1), local("key"));

    // `mod's ("ZqTier" + tier)`: the name is the `opPlus` before it.
    let by_sum = reads[2];
    assert_eq!(op(&records[named(&records[by_sum], 1)]), Op::Plus);

    // `who's "ZqHome"'s "ZqGold"`: the outer accessor's object is the inner.
    let (inner, outer) = (reads[3], reads[4]);
    assert_eq!(records[inner].values[1].text, "ZqHome");
    assert_eq!(records[outer].values[1].text, "ZqGold");
    assert_eq!(named(&records[outer], 0), inner);
}

/// `mod` lexes as an identifier, so `agent mod;` declares a variable: the
/// compiler's `mod` is its token name for `%`.
#[test]
fn mod_is_a_variable_name() {
    let src = source("forms/forms.gpl");
    let uses = lex(&src)
        .tokens
        .iter()
        .filter(|t| matches!(t.node, Token::Ident(n) if n.eq_ignore_ascii_case("mod")))
        .count();

    let declared = records(FORMS, "ZqReach")
        .iter()
        .filter(|r| op(r) == Op::Declare && r.text == "mod")
        .count();

    assert_eq!(uses, 4);
    assert_eq!(declared, 1);
}

// ---------------------------------------------------------------------------
// `break`, `continue` and `return` in a `foreach`

/// `break` and `continue` compile alike, not to a jump: `head` is the
/// enclosing `opForeach`'s record, and the record has no source line.
#[test]
fn break_and_continue_name_their_loop() {
    let records = records(FORMS, "ZqSum");
    let [foreach] = find(&records, Op::Foreach)[..] else {
        panic!("one loop")
    };

    for jump in [Op::Break, Op::Continue] {
        let [at] = find(&records, jump)[..] else {
            panic!("one {jump:?}")
        };

        assert_eq!(records[at].head as usize, foreach, "{jump:?}");
        assert_eq!(records[at].line, NO_LINE, "{jump:?}");
    }
}

/// A `return value` inside a `foreach`: an `opBreak` naming the loop, the
/// value's records, the loop's `opRewind`; then `opReturnValue`, on the
/// `foreach` line, where the loop's jump out lands. Returns the value's
/// records.
fn returns_past_its_loop(records: &[Record<'_>]) -> Vec<Op> {
    let [brk] = find(records, Op::Break)[..] else {
        panic!("one opBreak")
    };
    let foreach = records[brk].head as usize;
    let rewind = brk + find(&records[brk..], Op::Rewind)[0];
    let ret = rewind + 1;

    assert_eq!(op(&records[foreach]), Op::Foreach);
    assert_eq!(records[rewind].head as usize, foreach, "the loop's rewind");

    // The loop's cursor jumps out onto the return.
    assert_eq!(op(&records[foreach + 1]), Op::Advance);
    assert_eq!(records[foreach + 1].head as usize, ret);

    assert_eq!(op(&records[ret]), Op::ReturnValue);
    assert_eq!(records[ret].line, records[foreach].line);
    assert_eq!(named(&records[ret], 1), rewind - 1, "returns the value");

    records[brk + 1..rewind].iter().map(op).collect()
}

/// Both shapes mods ship: a literal, `return True;`, and a call's value,
/// `return $ZqSum(l);`.
#[test]
fn a_return_inside_a_foreach_compiles_past_its_loop() {
    assert_eq!(returns_past_its_loop(&records(FORMS, "ZqHas")), [Op::Nop]);
    assert_eq!(
        returns_past_its_loop(&records(FORMS, "ZqCalc")),
        [Op::Nop, Op::Call]
    );
}

const RELEASE: Config = Config {
    checks: Checks::Release,
    debug_out: DebugOut::Marked,
    ..Config::RELEASE
};

fn ints(items: &[i32]) -> Value {
    Value::List(items.iter().copied().map(Value::Integer).collect())
}

/// The value's records never run, and a loop that runs out returns too. A
/// literal is made at load, so `ZqHas` returns `True` on every path, the
/// `return False` after the loop dead; a call's slot is empty, so `ZqCalc`'s
/// return crashes.
#[test]
fn a_return_inside_a_foreach_returns_its_unrun_value() {
    let program = Program::load(&[image(FORMS)]).expect("loads");
    let run = |name: &str, items: &[i32]| {
        let Some(Entry::Script(def)) = program.entry(name) else {
            panic!("{name}: not a script")
        };
        let mut vm = Vm::new(&program, RELEASE);

        vm.run(
            &mut LogHost::default(),
            def,
            &[ints(items)],
            Catcher::Thread,
        )
        .map(|r| r.result)
    };

    for items in [&[-5, 1][..], &[-5], &[]] {
        assert_eq!(run("ZqHas", items), Ok(Value::Boolean(true)), "{items:?}");
        assert_eq!(
            run("ZqCalc", items),
            Err(Crash::NullValue(Null::Operand)),
            "{items:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// A negative constant

/// `expression #ZQ_NEG -2`: the value record is an `opNegate` of the
/// literal 2, as a body's `-2` is; nothing is folded. Loaded, the records are
/// evaluated: -2.
#[test]
fn a_negative_constant_negates_its_literal() {
    let records = records(FORMS, "#ZQ_NEG");
    let value = &records[CONSTANT_RECORD];

    assert_eq!(records.len(), CONSTANT_RECORD + 1);
    assert_eq!(op(value), Op::Negate);
    assert_eq!(value.values[0].word, 2);
    assert_eq!(value.values[0].kind, TypeCode::Integer.code());

    let program = Program::load(&[image(FORMS)]).expect("loads");
    assert_eq!(program.constant("#ZQ_NEG"), Some(Literal::Int(-2)));
}

// ---------------------------------------------------------------------------
// An add-on over a base image

fn script_sources(projects: &[&str]) -> Vec<String> {
    projects
        .iter()
        .map(|p| source(&p.replace(".gplproj", ".gpl")))
        .collect()
}

/// An add-on's `$Name`s resolve against its base and the builtins, not
/// alone.
#[test]
fn addon_calls_resolve_against_the_base() {
    let report = |projects: &[&str]| {
        let sources = script_sources(projects);
        let scripts: Vec<_> = sources
            .iter()
            .map(|s| regent_core::gpl::parse(s).script)
            .collect();
        let mut report = Report::default();

        for script in &scripts {
            report.add(script);
        }

        let unresolved: Vec<String> = report
            .unresolved()
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect();

        (unresolved, report.counts())
    };

    // Names fold to lower case, as the engine's map does.
    let (alone, _) = report(&[ADDON]);
    assert_eq!(alone, ["zqtax"]);

    let (unresolved, (script, builtin, none)) = report(&[BASE, ADDON]);
    assert!(unresolved.is_empty(), "{unresolved:?}");
    assert_eq!((script, builtin, none), (3, 1, 0));
}

/// What a program leaves unresolved, as `owner: what`.
fn missing(program: &Program) -> Vec<String> {
    let names = program.names();

    program
        .unresolved()
        .iter()
        .map(|u| {
            let owner = names.get(program.definitions()[u.definition as usize].name);
            let what = match u.missing {
                Missing::Call(n) => format!("call {}", names.get(n)),
                Missing::Function(n) => format!("value {}", names.get(n)),
                Missing::Constant(n) => names.get(n).to_owned(),
            };

            format!("{owner}: {what}")
        })
        .collect()
}

/// An add-on image loads over its base, as a mod's over the expansion: the
/// last listed owns a shared name, and every call and function value
/// resolves across the two.
///
/// A `#Name` no script defines is left to the host, as a mod's text
/// id is. **Inferred**: the engine's third source, descriptor by name,
/// answers it.
#[test]
fn an_addon_image_loads_over_its_base() {
    let order = dataset::load_order(vec![image(BASE), image(ADDON)]);
    let program = Program::load(&order).expect("loads");

    let gold = program.function("ZqGold").expect("defined");
    let at = program
        .definitions()
        .iter()
        .position(|d| std::ptr::eq(d, gold))
        .expect("a definition");

    // Load order is listed order reversed: the add-on is image 0.
    assert_eq!(program.origin()[at], 0, "the add-on's ZqGold");
    assert_eq!(program.shadowed().len(), 1);
    assert_eq!(missing(&program), [format!("ZqPay: {TEXT_ID}")]);

    // Alone, nothing answers the base's names. A call's record holds its
    // callee as a function value too.
    let alone = Program::load(&[image(ADDON)]).expect("loads");
    assert_eq!(
        missing(&alone),
        [
            "ZqGold: #ZQ_GOLD",
            "ZqPay: value ZqTax",
            "ZqPay: #ZQ_TEXT_ID",
            "ZqPay: call ZqTax",
            "ZqPay: value ZqTax",
        ]
    );
}
