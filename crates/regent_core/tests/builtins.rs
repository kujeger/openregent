//! GPL builtin registrations, against the SDK's documentation and the
//! shipped and Workshop scripts.

mod common;

use std::collections::BTreeMap;

use common::sdk::DOCUMENTED_BUILTINS;
use regent_core::gpl::builtins::{self, BuiltinId};
use regent_core::gpl::vm::native;
use regent_core::sim::world::Checked;

/// Every documented builtin is registered but `ReRandom`, and nothing else is
/// but the empty name, an alias of `NullFunction`.
#[test]
fn every_documented_builtin_but_one_is_registered() {
    let registered: Vec<String> = builtins::HD
        .iter()
        .filter(|b| !b.name.is_empty())
        .map(|b| b.name.to_ascii_lowercase())
        .collect();

    let mut documented: Vec<String> = DOCUMENTED_BUILTINS
        .iter()
        .filter(|n| !builtins::UNREGISTERED.contains(n))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    documented.sort();

    assert_eq!(registered, documented);

    // The library's list is the SDK's.
    let mut library: Vec<String> = builtins::documented()
        .map(str::to_ascii_lowercase)
        .collect();
    let mut sdk: Vec<String> = DOCUMENTED_BUILTINS
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();
    library.sort();
    sdk.sort();
    assert_eq!(library, sdk);

    let null = builtins::find("NullFunction").unwrap();
    assert_eq!(builtins::find("").unwrap().native, null.native);
    assert_eq!(builtins::find("ReRandom"), None);
}

/// Calls in the shipped and Workshop corpora that pass an argument their
/// builtin never reads. `CastSpell`'s is an empty string, in 63 calls.
const PASS_MORE: [&str; 4] = [
    "CastSpell",
    "LocationOf",
    "SetThreadInterval",
    "TeleportToPoint",
];

/// Every corpus call passes the arguments its builtin requires; a few pass
/// more than it reads: [`PASS_MORE`].
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn corpus_calls_pass_what_the_builtins_read() {
    let calls = call_counts();

    let short: Vec<(&str, usize)> = builtins::HD
        .iter()
        .flat_map(|b| {
            let counts = calls.get(&b.name.to_ascii_lowercase());
            counts
                .into_iter()
                .flat_map(|c| c.keys())
                .filter(|n| !b.args.accepts(**n))
                .map(|n| (b.name, *n))
                .collect::<Vec<_>>()
        })
        .collect();

    let more: Vec<&str> = builtins::HD
        .iter()
        .filter(|b| {
            let most = b.args.most().unwrap_or(usize::MAX);
            calls
                .get(&b.name.to_ascii_lowercase())
                .is_some_and(|c| c.keys().any(|n| *n > most))
        })
        .map(|b| b.name)
        .collect();

    assert_eq!(short, []);
    assert_eq!(more, PASS_MORE);
}

/// Per builtin, folded, how many calls pass each argument count: shipped
/// quests and Workshop mods together.
fn call_counts() -> BTreeMap<String, BTreeMap<usize, usize>> {
    use regent_core::gpl::ast::ExprKind;

    let sources = common::walk(&common::sdk().join("OriginalQuests"), &["gpl"])
        .into_iter()
        .chain(common::walk(&common::workshop(), &["gpl"]));

    let mut out: BTreeMap<String, BTreeMap<usize, usize>> = BTreeMap::new();

    for path in sources {
        let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();

        regent_core::gpl::parse(&text).script.visit_exprs(&mut |e| {
            if let ExprKind::Call { callee, args } = &e.kind
                && let ExprKind::Func(name) = &callee.kind
                && builtins::find(name).is_some()
            {
                *out.entry(name.to_ascii_lowercase())
                    .or_default()
                    .entry(args.len())
                    .or_default() += 1;
            }
        });
    }

    out
}

/// Every builtin the VM does not run itself has the world's answer
/// ([`Checked`]): the live host never falls back for want of one.
#[test]
fn every_builtin_has_an_answer() {
    let unanswered: Vec<&str> = BuiltinId::all()
        .filter(|b| !native::runs(*b) && Checked::of_id(*b).is_none())
        .map(|b| b.get().name)
        .collect();

    assert_eq!(unanswered, Vec::<&str>::new());
}
