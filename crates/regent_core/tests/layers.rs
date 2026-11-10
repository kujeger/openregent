//! The library's layers stay acyclic: a top-level module names only the
//! modules below it (the order in the crate doc, `src/lib.rs`).
//!
//! Reads every `src/**.rs` and each `crate::<module>` in its code, comments
//! left out: a doc link may point anywhere. A `super::` chain long enough
//! to reach the crate root counts as `crate::`.

use std::path::{Path, PathBuf};

/// Lowest first; each may use only those before it.
const LAYERS: [&str; 7] = ["base", "data", "content", "save", "gpl", "sim", "view"];

/// What a path names below the crate root.
const ROOT: &str = "crate::";

/// One step up a module path.
const SUPER: &str = "super::";

fn sources(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .flat_map(|p| {
            if p.is_dir() {
                sources(&p)
            } else if p.extension().is_some_and(|x| x == "rs") {
                vec![p]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// A line with its `//` comment cut. Rough, but no code here holds `//`
/// in a string beside a crate path.
fn code(line: &str) -> &str {
    line.split_once("//").map_or(line, |(code, _)| code)
}

/// The top-level modules `text` names by `crate::`.
fn named(text: &str) -> Vec<String> {
    text.lines()
        .map(code)
        .flat_map(|l| {
            l.match_indices(ROOT)
                .map(move |(at, _)| &l[at + ROOT.len()..])
        })
        .map(ident)
        .collect()
}

/// The leading identifier of `rest`.
fn ident(rest: &str) -> String {
    rest.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// How deep a file under `src` sits: `sim/world/walking.rs` is
/// `sim::world::walking`, 3; `sim/movement/mod.rs` is `sim::movement`, 2.
fn depth(relative: &Path) -> usize {
    let parts = relative.components().count();

    if relative.ends_with("mod.rs") {
        parts - 1
    } else {
        parts
    }
}

/// The top-level modules `text` names by a `super::` chain reaching the
/// root from a file `depth` modules deep.
///
/// The chain is counted from the file, not from an inline `mod` inside
/// it, so one in `mod tests` may be taken as reaching the root when it
/// stops one short. That only adds names, and no layer has a submodule
/// named after another layer, so a name it adds is never above.
fn ascended(text: &str, depth: usize) -> Vec<String> {
    text.lines()
        .map(code)
        .flat_map(|l| {
            // Each chain once: from its first `super::`, not from inside it.
            l.match_indices(SUPER)
                .filter(move |(at, _)| !l[..*at].ends_with(SUPER))
                .map(move |(at, _)| &l[at..])
        })
        .filter_map(|chain| {
            let rest = chain.trim_start_matches(SUPER);
            let steps = (chain.len() - rest.len()) / SUPER.len();

            (steps >= depth).then_some(rest)
        })
        .flat_map(|rest| match rest.strip_prefix('{') {
            // `super::super::{view, sim::x}`: each item's first name.
            Some(group) => group
                .split_once('}')
                .map_or(group, |(inner, _)| inner)
                .split(',')
                .map(|item| ident(item.trim()))
                .collect(),
            None => vec![ident(rest)],
        })
        .filter(|name| !name.is_empty())
        .collect()
}

fn rank(module: &str) -> Option<usize> {
    LAYERS.iter().position(|l| *l == module)
}

#[test]
fn every_top_level_module_is_a_layer() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let lib = std::fs::read_to_string(src.join("lib.rs")).expect("lib.rs");
    let declared: Vec<&str> = lib
        .lines()
        .filter_map(|l| l.strip_prefix("pub mod "))
        .map(|l| l.trim_end_matches(';'))
        .collect();

    let mut layers = LAYERS.to_vec();

    layers.sort_unstable();
    assert_eq!(declared, layers);
}

#[test]
fn no_module_uses_a_layer_above_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut wrong = Vec::new();

    for file in sources(&src) {
        let relative = file.strip_prefix(&src).expect("under src");
        let top = relative
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .map(|c| c.trim_end_matches(".rs"))
            .expect("a first component");

        // `lib.rs` declares the layers.
        let Some(own) = rank(top) else { continue };
        let text = std::fs::read_to_string(&file).expect("readable");

        let paths = named(&text)
            .into_iter()
            .chain(ascended(&text, depth(relative)));

        for module in paths {
            let above = rank(&module).is_some_and(|r| r > own);

            if above {
                wrong.push(format!("{}: crate::{module}", relative.display()));
            }
        }
    }

    wrong.sort();
    wrong.dedup();
    assert!(
        wrong.is_empty(),
        "uses a layer above:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn a_crate_path_in_a_comment_is_not_a_use() {
    let text = "use crate::data::cam; // see crate::view\n/// [`crate::sim`]";

    assert_eq!(named(text), ["data"]);
}

#[test]
fn a_super_chain_to_the_root_is_a_crate_path() {
    let text = "use super::super::view::x;\nuse super::super::{sim, data::y};\nuse super::z;";

    assert_eq!(ascended(text, 2), ["view", "sim", "data"]);
    assert_eq!(ascended(text, 3).len(), 0);
}

#[test]
fn a_file_is_as_deep_as_its_module_path() {
    assert_eq!(depth(Path::new("sim/world/walking.rs")), 3);
    assert_eq!(depth(Path::new("sim/movement/mod.rs")), 2);
    assert_eq!(depth(Path::new("sim/light.rs")), 2);
}
