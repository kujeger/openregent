//! Facts the engine's GPL traces settled (**Measured**): dispatch order,
//! upgrades, the trace line a nested dispatch prints.
//!
//! Needs `MAJESTY_HD`.

mod common;

use common::read;
use std::path::Path;

use regent_core::data::fourcc::FourCc;
use regent_core::gpl::emit;
use regent_core::gpl::load::{Entry as MapEntry, Program};
use regent_core::gpl::ops::Value;
use regent_core::gpl::runtime::Catcher;
use regent_core::gpl::vm::{Call, Checks, Config, Crash, DebugOut, Fault, Host, ObjId, Shared, Vm};
use regent_core::sim::world::{build::PREVIEW_ID, input_timing::Stage, search::MODIFIERS};

/// `-gpldebug`: the debug checks on, `DebugOut` for marked lines only.
const TRACED: Config = Config {
    checks: Checks::Debug,
    debug_out: DebugOut::Marked,
    ..Config::RELEASE
};

fn text(path: &Path) -> String {
    String::from_utf8_lossy(&read(path)).into_owned()
}

/// An engine dispatch between two passes runs after the next frame's clock
/// step and hit point tick, in
/// `GE_Level` before the scheduler: an action's function from the
/// orders, `UnitDelete` from method's drain of deferred deletes. The
/// player's input, the music's callback, the interface's queries and the
/// build preview's delete come before the step. **Game**: so placed,
/// play3's wrong `LocationOf` answers fall from 33 to 20, play5's from
/// 13,536 to 12,791.
#[test]
fn a_dispatch_between_passes_runs_after_the_clock_step() {
    let stages: Vec<Stage> = [
        ("make_attack", Some(20)),
        ("UnitDelete", Some(15)),
        ("Energy_Blast_Hit", Some(16)),
        ("UnitDelete", Some(PREVIEW_ID)),
        ("NewUnitInit", Some(9)),
        ("FreeStyle_Music_Stopped", Some(3)),
    ]
    .iter()
    .map(|(f, a)| Stage::unheld(f, *a))
    .collect();

    assert_eq!(
        stages,
        [
            Stage::Order,
            Stage::Order,
            Stage::Order,
            Stage::Message,
            Stage::Message,
            Stage::Message
        ]
    );
}

/// A building's `UpgradeTo` is `SECD + 0x08`: what building upgrade
/// turns it into. Play upgrades the palace and the wizards' guild.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn an_upgrade_turns_a_building_into_its_upgrade_to() {
    let bodies = common::bodies();
    let upgrades: Vec<(FourCc, Option<FourCc>)> = [*b"ABJ1", *b"ABY1", *b"ABJ3", *b"ABF1"]
        .iter()
        .map(|c| (FourCc(*c), bodies[&FourCc(*c)].upgrade))
        .collect();

    assert_eq!(
        upgrades,
        [
            (FourCc(*b"ABJ1"), Some(FourCc(*b"ABJ2"))),
            (FourCc(*b"ABY1"), Some(FourCc(*b"ABY2"))),
            (FourCc(*b"ABJ3"), None),
            (FourCc(*b"ABF1"), None),
        ]
    );
}

/// Answers every builtin by dispatching `Callback`, as an engine callback
/// inside a builtin would.
struct Dispatching;

impl Host for Dispatching {
    fn builtin(&mut self, vm: &mut Vm<'_>, _: &Call<'_>) -> Result<(), Fault> {
        vm.dispatch(self, "Callback", &[Value::Integer(7)])
            .map(|_| ())
            .map_err(Fault::Crash)
    }

    fn attribute(&mut self, _: &mut Vm<'_>, _: u32, _: &str) -> Result<ObjId, Fault> {
        Err(Fault::Crash(Crash::NotCallable))
    }

    fn named(&mut self, _: &str) -> Value {
        Value::Integer(0)
    }

    fn log(&mut self, _: &str) {}
}

/// A dispatch from inside a builtin shows as a run prologue before the
/// builtin's `call` line. The Freestyle trace has none, so this log is the
/// VM's own.
#[test]
fn a_dispatch_inside_a_builtin_prints_its_prologue() {
    let source = "function Callback(integer n) is integer\nbegin\n\treturn n * 2;\nend\n\
                  function Outer() is integer\nbegin\n\t$Halt();\n\treturn 3;\nend\n";
    let bytes = emit::compile_source(source, "test.gpl").expect("compiles");
    let program = Program::load(&[bytes]).expect("loads");
    let Some(MapEntry::Script(outer)) = program.entry("Outer") else {
        panic!("Outer is a script");
    };

    let lines = Shared::new(vec!["header".to_owned()]);
    let sink = lines.clone();
    let mut vm = Vm::new(&program, TRACED);

    vm.set_trace(Some(Box::new(move |l: &str| {
        sink.borrow_mut().push(l.to_owned());
    })));
    vm.run(&mut Dispatching, outer, &[], Catcher::Thread)
        .expect("runs");
    vm.set_trace(None);

    let log = lines.borrow().join("\r\n");

    assert!(
        log.contains("\t\t$Callback(), L#1: op: , lhs: , rhs: , result: "),
        "{log}"
    );
}

/// `ListObjects`' modifier switch takes the values the SDK's `LowLevel.gpl`
/// gives the names, in both images.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn list_objects_modifiers_are_the_sdks_constants() {
    let sdk = common::sdk();

    for image in [
        "OriginalQuests/GPL/LowLevel.gpl",
        "OriginalQuests/GPLMx/mx_LowLevel.gpl",
    ] {
        let source = text(&sdk.join(image));
        let defined: Vec<(String, i32)> = source
            .lines()
            .filter_map(|l| {
                let mut words = l.split_whitespace();
                let head = words.next()?;
                let name = words.next()?.strip_prefix('#')?;
                let value = words.next()?.parse().ok()?;

                (head == "expression").then(|| (name.to_ascii_lowercase(), value))
            })
            .collect();

        for (value, name) in (0..).zip(MODIFIERS) {
            let found = defined
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| *v);

            assert_eq!(found, Some(value), "{image}: {name}");
        }
    }
}
