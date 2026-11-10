//! Fails on what the public copy must not hold, in every tracked file:
//! executable addresses, rizin names, the private function labels
//! (`read.*`), vtable slots, disassembly and decompiler output
//! (docs/status/publication-plan.md). Memory offsets of engine structs
//! have no pattern: keep them out by hand, in `local/`.
//!
//! `just addresses` prints the count, by pattern and by file.
//! Needs no game data.

use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What the check does with a hit.
enum Mode {
    /// Print the count, and pass.
    #[expect(dead_code, reason = "for a survey; tracked files are redacted")]
    Count,
    /// Fail, naming each hit.
    Fail,
}

/// Tracked files are redacted; the unredacted notes are in `local/`.
const MODE: Mode = Mode::Fail;

/// This file: its examples would count.
const SELF: &str = "crates/regent_core/tests/addresses.rs";

/// Private, never committed.
const PRIVATE: &str = "local/";

/// x86 mnemonics, for disassembly.
const MNEMONICS: &str = "mov|movzx|movsx|lea|push|pop|call|cmp|jmp|test|xor|and|or|add|sub|\
                         shl|shr|sar|imul|inc|dec";

/// 32-bit x86 registers and their byte halves.
const REGISTERS: &str = "e[a-d]x|e[sd]i|e[sb]p|[a-d][lh]";

/// A kind of hit.
struct Pattern {
    name: &'static str,
    regex: Regex,
}

fn patterns() -> Vec<Pattern> {
    let disassembly = format!(
        // `mov eax, 3`; `test byte [x]`; `[esp + 0x10]`; rizin's `var_a4h`
        // and `str.CreateAgent`.
        r"(?i:\b(?:{MNEMONICS})\s+(?:{REGISTERS})\b|\b(?:{MNEMONICS})\s+(?:dword|word|byte)\s+(?:ptr\s+)?\[|\[\s*(?:{REGISTERS})\s*[+-]|\b(?:arg|var)_[0-9a-f]+h\b)|\bstr\.[A-Z]\w*"
    );

    [
        // `0x00431300`, `0x007c5454`: the executable's image, code to data.
        ("executable address", r"(?i)\b0x00[4-7][0-9a-f]{5}\b".to_owned()),
        // `fcn.00431300`: rizin's name for a function.
        ("rizin function", r"\bfcn\.[0-9a-fA-F]{8}\b".to_owned()),
        ("disassembly", disassembly),
        // `read.player_setup`: a label from `local/names/`. Not a call on
        // a Rust value named `read`: `read.chunk_by_tag(…)`.
        (
            "function label",
            r"`read\.[a-z_][a-z0-9_]*`|\bread\.[a-z]+_[a-z0-9_]+(?:[^(\w]|$)".to_owned(),
        ),
        // `GenUnit::vf30`, `vf3`: a vtable slot.
        ("vtable slot", r"\bvf\d+\b".to_owned()),
        // `FUN_00431300`, `uVar3`, `local_1c`, `undefined4`, `unaff_EBX`,
        // `CONCAT31`: decompiler output.
        (
            "decompiler",
            r"\b(?:FUN|DAT|LAB|PTR)_[0-9a-fA-F]{6,8}\b|\b[a-z]{1,3}Var\d+\b|\blocal_[0-9a-f]*\d[0-9a-f]*\b|\bundefined[1248]\b|\b(?:unaff|extraout|in)_[A-Z]{2,3}\b|\bCONCAT\d{2}\b"
                .to_owned(),
        ),
    ]
    .into_iter()
    .map(|(name, re)| Pattern {
        name,
        regex: Regex::new(&re).expect("a pattern"),
    })
    .collect()
}

/// One match: where, which pattern, what text.
struct Hit {
    file: String,
    line: usize,
    pattern: usize,
    text: String,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Tracked files, relative to the repo, `local/` and this file left out.
fn tracked() -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo())
        .args(["ls-files", "-z"])
        .output()
        .expect("git runs");

    assert!(
        out.status.success(),
        "git ls-files: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    String::from_utf8(out.stdout)
        .expect("UTF-8 paths")
        .split('\0')
        .filter(|p| !p.is_empty() && !p.starts_with(PRIVATE) && *p != SELF)
        .map(str::to_owned)
        .collect()
}

/// Every hit in `text`, file `file`.
fn scan(patterns: &[Pattern], file: &str, text: &str) -> Vec<Hit> {
    text.lines()
        .enumerate()
        .flat_map(|(n, line)| {
            patterns.iter().enumerate().flat_map(move |(p, pattern)| {
                pattern.regex.find_iter(line).map(move |m| Hit {
                    file: file.to_owned(),
                    line: n + 1,
                    pattern: p,
                    text: m.as_str().to_owned(),
                })
            })
        })
        .collect()
}

/// Totals by pattern, then each file by its total, most first:
///
/// ```text
/// executable address   2290 in 293 files
///   41  crates/x.rs  (executable address 30, disassembly 11)
/// ```
fn report(patterns: &[Pattern], hits: &[Hit]) -> String {
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();

    for hit in hits {
        by_file
            .entry(&hit.file)
            .or_insert_with(|| vec![0; patterns.len()])[hit.pattern] += 1;
    }

    let totals = patterns.iter().enumerate().map(|(p, pattern)| {
        let count = hits.iter().filter(|h| h.pattern == p).count();
        let files = by_file.values().filter(|c| c[p] > 0).count();

        format!("{:<20}{count:>6} in {files} files", pattern.name)
    });
    let totals: Vec<String> = totals.collect();

    let mut files: Vec<(&str, Vec<usize>)> = by_file.into_iter().collect();

    files.sort_by_key(|(_, counts)| std::cmp::Reverse(counts.iter().sum::<usize>()));

    let lines = files.into_iter().map(|(file, counts)| {
        let parts: Vec<String> = patterns
            .iter()
            .zip(&counts)
            .filter(|(_, c)| **c > 0)
            .map(|(pattern, c)| format!("{} {c}", pattern.name))
            .collect();

        format!(
            "{:>6}  {file}  ({})",
            counts.iter().sum::<usize>(),
            parts.join(", ")
        )
    });

    totals
        .into_iter()
        .chain(lines)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn tracked_files_hold_no_addresses() {
    let patterns = patterns();
    let root = repo();

    // A file gone from the work tree, or not text, has nothing to read.
    let hits: Vec<Hit> = tracked()
        .iter()
        .filter_map(|f| {
            std::fs::read_to_string(root.join(f))
                .ok()
                .map(|text| scan(&patterns, f, &text))
        })
        .flatten()
        .collect();

    match MODE {
        Mode::Count => println!("{}", report(&patterns, &hits)),
        Mode::Fail => assert!(
            hits.is_empty(),
            "not for the public copy:\n{}",
            hits.iter()
                .map(|h| format!(
                    "{}:{}: {} `{}`",
                    h.file, h.line, patterns[h.pattern].name, h.text
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

/// Each pattern finds its examples, and leaves look-alikes alone: Bink's
/// tables, Rust fields, prose.
#[test]
fn each_pattern_finds_its_examples() {
    let patterns = patterns();
    let found = |line: &str| -> Vec<&str> {
        scan(&patterns, "x", line)
            .iter()
            .map(|h| patterns[h.pattern].name)
            .collect()
    };

    let cases = [
        ("at `0x00431306`", vec!["executable address"]),
        ("at 0x0052A380", vec!["executable address"]),
        ("the clock at `0x007c5454`", vec!["executable address"]),
        ("`fcn.00431300`", vec!["rizin function"]),
        ("`mov eax, [ecx+0x44]`", vec!["disassembly", "disassembly"]),
        ("lea  ecx, [esp + 0x10]", vec!["disassembly", "disassembly"]),
        (
            "cmp dword [var_a4h], 0x100",
            vec!["disassembly", "disassembly"],
        ),
        ("push str.CreateAgent", vec!["disassembly"]),
        ("push ebx", vec!["disassembly"]),
        ("FUN_00496e90 (uVar3, local_1c)", vec!["decompiler"; 3]),
        ("undefined4 unaff_EBX CONCAT31", vec!["decompiler"; 3]),
        ("0x160000, 0x4347AC, 0x00000001", vec![]),
        ("obj.kind, str.len(), local_add", vec![]),
        ("a word and byte order", vec![]),
        ("tail word [`CHEATS`], a 20-byte [`Playback`]", vec![]),
        ("`reference`, `undefined` and", vec![]),
        ("set by `read.player_setup`", vec!["function label"]),
        ("read.game_end_step step 1", vec!["function label"]),
        ("`GenUnit::vf30` and `vf3`", vec!["vtable slot"; 2]),
        ("read.chunk_by_tag(b\"TILE\"), read.tiles.len()", vec![]),
    ];

    for (line, expected) in cases {
        assert_eq!(found(line), expected, "{line}");
    }
}
