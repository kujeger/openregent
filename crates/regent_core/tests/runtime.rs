//! How the engine runs GPL, against the shipped programs.
//!
//! [`regent_core::gpl::runtime`] holds the rules. The call-graph tests settle
//! whether any shipped script can re-enter itself.

mod common;

#[path = "runtime/graph.rs"]
mod graph;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regent_core::data::bcd;
use regent_core::gpl::opcode;
use regent_core::gpl::runtime;

// ---------------------------------------------------------------------------
// The call graph
// ---------------------------------------------------------------------------

/// HD's `.bcd` images: base game and expansion, and the three the expansion
/// loads per quest.
const HD_IMAGES: usize = 9;

/// Functions calling through a value slot rather than a name, all images.
const VALUE_CALLERS: usize = 23;

/// No shipped function reaches itself by calls that name their callee, in any
/// HD image, `opLParen` by `opLParen`. The union of all nine: a cycle in none
/// of them, whatever set is loaded.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_shipped_function_reaches_itself_by_name() {
    let hd = common::hd();
    let mut paths = common::walk(&hd.join("Data"), &["bcd"]);
    paths.extend(common::walk(&hd.join("DataMX"), &["bcd"]));

    assert_eq!(paths.len(), HD_IMAGES);

    let mut calls: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut value_callers = BTreeSet::new();

    for path in &paths {
        let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = regent_core::gpl::bcd::walk(sections.symbols.body).expect("the code walks");

        for d in walked
            .iter()
            .filter(|d| d.records.iter().any(|r| r.opcode == opcode::BODY))
        {
            let me = d.name.to_ascii_lowercase();
            let callees = calls.entry(me.clone()).or_default();

            for r in d.records.iter().filter(|r| r.opcode == opcode::CALL) {
                let callee = &r.values[0];

                if callee.text.is_empty() {
                    value_callers.insert(me.clone());
                } else {
                    callees.insert(callee.text.to_ascii_lowercase());
                }
            }
        }
    }

    // Builtins are leaves.
    let defined: BTreeSet<String> = calls.keys().cloned().collect();
    for callees in calls.values_mut() {
        callees.retain(|c| defined.contains(c));
    }

    assert_eq!(graph::cycles(&calls), Vec::<BTreeSet<String>>::new());
    assert_eq!(value_callers.len(), VALUE_CALLERS);
}

fn quests() -> PathBuf {
    common::sdk().join("OriginalQuests")
}

/// The two programs: the base game's project, the expansion's five.
fn programs() -> [(&'static str, Vec<PathBuf>); 2] {
    let expansion = ["Build", "Data", "Decision", "Task", "MajMisc"]
        .iter()
        .map(|n| quests().join(format!("GPLMx/Path_{n}.gplproj")))
        .collect();

    [
        ("base", vec![quests().join("GPL/path.gplproj")]),
        ("expansion", expansion),
    ]
}

/// Every value call reads an attribute. Resolved, only two are wild: the
/// expansion's `EventScript`, from `$LookupFunction` of a quest's event
/// name, and nothing in the base game.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_value_call_reads_an_attribute() {
    let expected: [(&str, &[&str], &[&str]); 2] = [
        (
            "base",
            &[
                "activescript",
                "deathscript",
                "evaluationscript",
                "igdeathscript",
                "questscript",
                "transferscript",
            ],
            &[],
        ),
        (
            "expansion",
            &[
                "activescript",
                "birthscript",
                "deathscript",
                "evaluationscript",
                "eventscript",
                "igdeathscript",
                "questscript",
                "spawn_function",
                "transferscript",
            ],
            &["eventscript"],
        ),
    ];

    for ((name, projects), (label, attributes, wild)) in programs().into_iter().zip(expected) {
        assert_eq!(name, label);

        let program = graph::Program::load(&projects);
        let called = program.called_attributes();

        assert!(
            program
                .calls
                .values()
                .flatten()
                .all(|c| *c != graph::Callee::Other),
            "{name}: a value call not through an attribute"
        );
        assert_eq!(
            called.keys().map(String::as_str).collect::<Vec<_>>(),
            attributes
        );
        assert_eq!(
            called
                .iter()
                .filter(|(_, w)| **w)
                .map(|(a, _)| a.as_str())
                .collect::<Vec<_>>(),
            wild,
            "{name}"
        );
    }
}

/// With value calls resolved, three cycles remain, each through a value the
/// static model cannot narrow. **Inferred** none is reached in play:
///
/// - `Player_Dead_Cleanup` calls the `IGDeathScript` of lairs only; the
///   deaths calling it are palaces' and outposts'.
/// - `Be_Dumb` is a `StartingScript`; `QuestScript` copies `StartingScript`
///   before `Be_Dumb` is stored there.
/// - `Freestyle` calls `EventScript`, a quest's special event; the model
///   lets `$LookupFunction` name anything, `Freestyle` included.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn recursion_needs_a_value_the_sources_do_not_narrow() {
    let set = |names: &[&str]| {
        names
            .iter()
            .map(|n| (*n).to_owned())
            .collect::<BTreeSet<_>>()
    };

    let expected = [
        vec![
            set(&["palace_death", "player_dead_cleanup"]),
            set(&["be_dumb"]),
        ],
        vec![
            set(&["outpost_death", "palace_death", "player_dead_cleanup"]),
            set(&["be_dumb"]),
            set(&["freestyle"]),
        ],
    ];

    for ((name, projects), want) in programs().into_iter().zip(expected) {
        let program = graph::Program::load(&projects);
        let mut found = graph::cycles(&program.graph());
        let mut want = want;

        found.sort();
        want.sort();

        assert_eq!(found, want, "{name}");
    }
}

/// A run that breaks the frame, as `sim::game` does on a divergence:
/// the scheduler takes no more units.
#[test]
fn a_run_that_stops_the_frame_takes_no_more_units() {
    let mut queues = runtime::Queues::new([2, 3, 4]);
    let mut ran = vec![];

    queues.frame(|_, unit| {
        ran.push(unit);

        if ran.len() == 2 {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    });

    assert_eq!(ran.len(), 2);
}
