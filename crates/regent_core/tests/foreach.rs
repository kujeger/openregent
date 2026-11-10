//! A `foreach` cursor under a list that changes. The engine's cursor is a
//! `std::list` node, so a change elsewhere in the
//! list does not move it; `gpl::vm` models that.
//!
//! Needs `MAJESTY_HD`.

mod common;

use common::read;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::gpl::builtins::{self};
use regent_core::gpl::emit;
use regent_core::gpl::exec::{Callee, Definition, Dest, Head, Literal, Operand, Target};
use regent_core::gpl::load::{Entry, Program};
use regent_core::gpl::opcode::Op;
use regent_core::gpl::ops::Value;
use regent_core::gpl::runtime::Catcher;
use regent_core::gpl::vm::{Checks, Config, Crash, DebugOut, Null, Run, Vm};

use common::log_host::LogHost;

// ---------------------------------------------------------------------------
// Running

const RELEASE: Config = Config {
    checks: Checks::Release,
    debug_out: DebugOut::Marked,
    ..Config::RELEASE
};

fn compiled(src: &str) -> Program {
    let bytes = emit::compile_source(src, "test.gpl").expect("compiles");

    Program::load(&[bytes]).expect("loads")
}

/// Run `name` once on a fresh VM, as a thread would.
fn run(program: &Program, name: &str, args: &[Value]) -> Result<Run, Crash> {
    let def = match program.entry(name) {
        Some(Entry::Script(d)) => d,
        other => panic!("{name}: {other:?}"),
    };
    let mut vm = Vm::new(program, RELEASE);

    vm.run(&mut LogHost::default(), def, args, Catcher::Thread)
}

fn ints(items: &[i32]) -> Value {
    Value::List(items.iter().copied().map(Value::Integer).collect())
}

/// `ThisList -= Potential` inside `foreach Potential in ThisList`, as HD's
/// `Cull_Subtypes`. The cursor is past the removed node, so the next element
/// is still visited: every match goes. An index would skip the element after
/// each removal and keep 4.
#[test]
fn removing_the_current_element_skips_nothing() {
    let program = compiled(
        "function Cull(list l) is list\ndeclare\n\tinteger n;\n\
         begin\n\tforeach n in l do\n\t\tif (n > 2)\n\t\t\tl -= n;\n\treturn l;\nend\n",
    );
    let out = run(&program, "Cull", &[ints(&[1, 3, 4, 5, 2])]).expect("runs");

    assert_eq!(out.result, ints(&[1, 2]));
}

/// `RemoveListMember` erases a node before the cursor (gpl list erase at, by
/// position from 1); the loop still visits every element.
#[test]
fn removing_an_element_before_the_cursor_skips_nothing() {
    let program = compiled(
        "function Drain(list l) is integer\ndeclare\n\tinteger n;\n\tinteger sum;\n\
         begin\n\tforeach n in l do\n\t\tbegin\n\t\t\tsum = sum + n;\n\
         \t\t\t$RemoveListMember(l, 1);\n\t\tend\n\treturn sum;\nend\n",
    );
    let out = run(&program, "Drain", &[ints(&[1, 2, 3, 4])]).expect("runs");

    assert_eq!(out.result, Value::Integer(10));
}

/// An element appended while the cursor is on a node is visited. On the last
/// pass the cursor is already the list's end sentinel, so one appended then
/// is not.
#[test]
fn an_element_appended_on_the_last_pass_is_not_visited() {
    let program = compiled(
        "function Grow(list l, integer at) is integer\ndeclare\n\tinteger n;\n\tinteger c;\n\
         begin\n\tforeach n in l do\n\t\tbegin\n\t\t\tc++;\n\
         \t\t\tif (n == at)\n\t\t\t\tl << 9;\n\t\tend\n\treturn c;\nend\n",
    );
    let passes = |at: i32| {
        run(&program, "Grow", &[ints(&[1, 2, 3]), Value::Integer(at)])
            .expect("runs")
            .result
    };

    assert_eq!(passes(1), Value::Integer(4));
    assert_eq!(passes(3), Value::Integer(3));
}

/// `ClearList` releases every value, then frees the nodes in order.
/// A cursor on one reads
/// it at the next pass: its value as released, its old link on. On the
/// last pass the cursor is the sentinel, which survives, and the loop ends.
#[test]
fn clearing_the_list_under_a_cursor_reads_the_freed_nodes() {
    let program = compiled(
        "function Wipe(list l) is list\ndeclare\n\tinteger n;\n\tlist seen;\n\
         begin\n\tforeach n in l do\n\t\tbegin\n\t\t\tseen << n;\n\
         \t\t\t$ClearList(l);\n\t\tend\n\treturn seen;\nend\n",
    );
    let seen = |items: &[i32]| run(&program, "Wipe", &[ints(items)]).map(|r| r.result);

    assert_eq!(seen(&[1, 2, 3]), Ok(ints(&[1, 0, 0])));
    assert_eq!(seen(&[1]), Ok(ints(&[1])));
}

/// `-=` erases every equal node, from the head again after each;
/// with a duplicate next, that is the cursor's own. The
/// next pass reads the freed node: its value released to 0,
/// its link to the node after.
#[test]
fn removing_a_duplicate_under_the_cursor_reads_the_freed_node() {
    let program = compiled(
        "function Cull(list l) is list\ndeclare\n\tinteger n;\n\tlist seen;\n\
         begin\n\tforeach n in l do\n\t\tbegin\n\t\t\tseen << n;\n\
         \t\t\tif (n > 2)\n\t\t\t\tl -= n;\n\t\tend\n\treturn seen;\nend\n",
    );
    let seen = |items: &[i32]| run(&program, "Cull", &[ints(items)]).map(|r| r.result);

    assert_eq!(seen(&[1, 3, 3, 2]), Ok(ints(&[1, 3, 0, 2])));

    // `Release_Guards`' case: two guards, each listed twice, alternating.
    assert_eq!(seen(&[3, 4, 3, 4, 2]), Ok(ints(&[3, 4, 0, 2])));
}

/// A `return` inside a `foreach`, compiled as `Gplbcc.exe` does
/// (`emit::Body::returns`): `opBreak`, the value's records, which never run,
/// and `opReturnValue` where the loop's jump out lands. A literal's object is
/// made at load, so `Phantom_Priestess_Assigned_To`'s shape returns `True`
/// on every path through the loop, empty list included; the `return False`
/// after it never runs. A computed value's slot is empty: the return crashes.
/// **Inferred**: so does Majestic Majesty's `defend_building_packet`, a call's
/// value, whenever it reaches its loop.
#[test]
fn a_return_inside_a_foreach_returns_its_unrun_value() {
    let program = compiled(
        "function Find(list l, integer x) is boolean\ndeclare\n\tinteger n;\n\
         begin\n\tforeach n in l do\n\t\tif (n == x)\n\t\t\treturn True;\n\
         \treturn False;\nend\n\
         function Get(list l, integer x) is integer\ndeclare\n\tinteger n;\n\
         begin\n\tforeach n in l do\n\t\tif (n == x)\n\t\t\treturn n + 1;\n\
         \treturn 7;\nend\n",
    );
    let found = |name: &str, items: &[i32]| {
        run(&program, name, &[ints(items), Value::Integer(2)]).map(|r| r.result)
    };

    for items in [&[1, 2, 3][..], &[5], &[]] {
        assert_eq!(found("Find", items), Ok(Value::Boolean(true)), "{items:?}");
        assert_eq!(
            found("Get", items),
            Err(Crash::NullValue(Null::Operand)),
            "{items:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The shipped scripts

/// HD's base game and expansion, loaded as the engine loads them.
fn hd_program() -> Program {
    let images: Vec<Vec<u8>> = Install::new(&common::hd(), Bytecode::Install)
        .files(Datasets::Expansion)
        .expect("HD's files")
        .programs
        .iter()
        .map(|p| read(p))
        .collect();

    Program::load(&images).expect("HD loads")
}

/// The value a slot names, followed through places: a parameter or local,
/// an attribute by name (any agent), or something else.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Root {
    Local(u32),
    Attribute(String),
    Other(u32),
}

fn root(program: &Program, def: &Definition, slot: u32) -> Root {
    let instr = &def.code[slot as usize];

    match (instr.op, instr.dest, instr.lhs, instr.rhs) {
        (Op::Localize | Op::Declare, ..) => Root::Local(slot),
        (Op::Apostrophe, _, _, Operand::Value(Literal::Str(s))) => {
            Root::Attribute(program.names().get(s).to_lowercase())
        }
        (_, Dest::Alias, Operand::Slot(s), _) if s != slot => root(program, def, s),
        _ => Root::Other(slot),
    }
}

/// The operators that change a list in place.
const fn changes_list(op: Op) -> bool {
    matches!(
        op,
        Op::AssignEqual | Op::PlusEqual | Op::MinusEqual | Op::Inserter
    )
}

/// What a loop's body does to the list it walks, in its own records.
#[derive(Debug, Default, PartialEq, Eq)]
struct Changes {
    /// `list -= variable`: the element just visited.
    removes_current: usize,
    /// Any other list operator on it.
    other_operator: usize,
    /// Passed to a builtin, native or host; a script gets a copy.
    to_builtin: usize,
}

fn changes(program: &Program) -> Changes {
    let mut out = Changes::default();

    for def in program.definitions() {
        for (at, instr) in def.code.iter().enumerate() {
            let (Op::Foreach, Operand::Slot(var), Operand::Slot(list)) =
                (instr.op, instr.lhs, instr.rhs)
            else {
                continue;
            };
            let over = root(program, def, list);
            let variable = root(program, def, var);

            // The body runs to the `opRewind` naming this loop.
            let loop_at = u32::try_from(at).expect("small");
            let end = def.code[at..]
                .iter()
                .position(|i| i.op == Op::Rewind && i.head == Head::Jump(loop_at))
                .map_or(def.code.len(), |e| at + e);

            for b in &def.code[at + 2..end] {
                let on =
                    |o: Operand| matches!(o, Operand::Slot(s) if root(program, def, s) == over);

                if changes_list(b.op) && on(b.lhs) {
                    let current = b.op == Op::MinusEqual
                        && matches!(b.rhs, Operand::Slot(s) if root(program, def, s) == variable);

                    if current {
                        out.removes_current += 1;
                    } else {
                        out.other_operator += 1;
                    }
                }

                let to_builtin = b.call.as_deref().is_some_and(|c| {
                    matches!(c.callee, Callee::Fn(Target::Builtin(x)) if builtins::HD.get(usize::from(x)).is_some())
                        && c.args.iter().any(|&a| root(program, def, a) == over)
                });

                out.to_builtin += usize::from(to_builtin);
            }
        }
    }

    out
}

/// Seven HD loops remove the element just visited from the list they walk:
/// `Cull_Subtypes`, `Cull_Titles`, `Check_Resurrect`,
/// `Loyalty_Mod_Pick_Closest`, `Pick_a_fairground`,
/// `returning_guardian_eval_enemies_simple`, and over an attribute
/// `Dump_Contestants`. 1.5.2.28's `Release_Guards` empties its list before
/// the loop, not inside it. No loop changes its list
/// otherwise in its own body, nor passes it to a builtin. For callees, see
/// **Measured**.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn seven_hd_loops_remove_the_element_they_visit() {
    let found = changes(&hd_program());

    assert_eq!(
        found,
        Changes {
            removes_current: 7,
            other_operator: 0,
            to_builtin: 0,
        }
    );
}
