//! The `.bcd` back end, against `Gplbcc.exe`'s own output on disk.
//!
//! Data section:
//!
//! 1. one project compiles byte for byte — the only whole-file oracle;
//! 2. every shipped data section round-trips: 75 files, 3,101 prototypes;
//! 3. base game and expansion `.dat` files compile to their shipped data
//!    sections — 408 prototypes;
//! 4. so do 20 other projects.
//!
//! 3 and 4 start from source, so they pin sort order and the duplicate-key rule.
//!
//! Needs `MAJESTY_WORKSHOP` and `MAJESTY_HD`, from the
//! environment or `.env`. A missing one panics.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regent_core::data::bcd;
use regent_core::data::gpltype;
use regent_core::data::gpltype::TypeCode;
use regent_core::gpl::bcd::{
    CONSTANT_RECORD, Compiled, DataSection, ScopeSection, encode as encode_code, symbol_count, walk,
};
use regent_core::gpl::opcode::Op;
use regent_core::gpl::{dat, emit, proj};

/// The only data-only project shipped, so the only whole-file oracle.
const ORACLE: &str = "Adjust Guardhouse Mod";

/// In HD, the SDK and the eleven Workshop mods.
const BCD_FILES: usize = 75;
const BCD_PROTOTYPES: usize = 3101;
const BCD_VALUES: usize = 43_977;

/// Of 61 projects on disk on GOG, 19 reproduce a shipped data section
/// exactly. Steam's `TempProject.gplproj`, `SpecialItemsExample`'s only
/// project, makes a 20th.
const PROJECTS_MATCHED: usize = 19;

/// Size of one section-2 record, one name in scope. All 19,458 bodies are a
/// whole number of them; the smallest is one.
const DEFINITION_RECORD: usize = 20;

/// Majestic Majesty, the one project whose data section does not reproduce.
///
/// Its shipped `bytecode.bcd` came from newer `.dat` files: prototype names
/// match, 373 of 375 prototypes have our values as a strict subset of theirs,
/// and the eleven extra keys are later features (`AttackPeriod`,
/// `Fire_Attack_Damage_Max`). It bundles a different `Gplbcc.exe` build.
fn drifted_mod() -> PathBuf {
    common::workshop().join("678145904")
}

/// Every `.bcd` in the SDK, the mods and HD.
fn every_bcd() -> Vec<PathBuf> {
    common::bcds(&[common::sdk(), common::workshop(), common::hd()])
}

/// Compile the `.dat` files at `paths` to a data section body.
fn section_bytes(paths: &[PathBuf]) -> Vec<u8> {
    let sources: Vec<String> = paths
        .iter()
        .map(|p| {
            let raw = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            String::from_utf8_lossy(&raw).into_owned()
        })
        .collect();

    let files: Vec<_> = sources
        .iter()
        .zip(paths)
        .map(|(src, p)| {
            let parsed = dat::parse(src);
            assert!(parsed.is_ok(), "{}: {:?}", p.display(), parsed.errors);
            parsed
        })
        .collect();

    emit::data_section(&files).encode()
}

/// Read the data section a shipped `.bcd` carries.
fn shipped_section(path: &Path) -> Vec<u8> {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let program =
        bcd::Container::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let sections = program
        .sections()
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    sections.data.body.to_vec()
}

/// Our output matches `Gplbcc.exe`'s byte for byte.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn guardhouse_mod_compiles_byte_for_byte() {
    let sdk = common::sdk();
    let root = sdk.join(ORACLE);

    let ours = emit::compile_project(&root.join("GPL/AdjustGuardhouse.gplproj"))
        .expect("the project compiles");
    let theirs = std::fs::read(root.join("Data/AdjustGuardhouse.bcd")).expect("shipped output");

    assert_eq!(ours.len(), 465, "output size changed");
    assert_eq!(ours, theirs, "output differs from Gplbcc.exe's");
}

/// A listed source that does not parse fails the project, as
/// `compile_source` fails.
#[test]
fn a_project_with_a_parse_error_fails() {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let dir = scratch.path();

    std::fs::write(dir.join("Broken.gpl"), "function Go() { x = ; }").expect("source");
    std::fs::write(dir.join("Broken.gplproj"), "source=\"Broken.gpl\"\r\n").expect("project");

    let got = emit::compile_project(&dir.join("Broken.gplproj"));

    assert!(
        matches!(got, Err(emit::Error::Parse { .. })),
        "{:?}",
        got.map(|b| b.len())
    );
}

/// Decoding drops nothing: a loss would change length or order.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_data_section_round_trips() {
    let files = every_bcd();

    assert!(!files.is_empty(), "the corpora hold no .bcd files");

    let mut prototypes = 0;
    let mut values = 0;

    for path in &files {
        let body = shipped_section(path);
        let section =
            DataSection::decode(&body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        prototypes += section.prototypes.len();
        values += section
            .prototypes
            .iter()
            .map(|p| p.values.len())
            .sum::<usize>();

        assert_eq!(section.encode(), body, "{}", path.display());
    }

    assert!(
        files.len() >= BCD_FILES,
        "expected at least {BCD_FILES} .bcd files, found {}",
        files.len()
    );
    assert!(
        prototypes >= BCD_PROTOTYPES,
        "prototype count fell to {prototypes}"
    );
    assert!(values >= BCD_VALUES, "value count fell to {values}");
}

/// Base game and expansion `.dat` files compile to their shipped data sections.
/// Pins sort order and the duplicate-key rule, not only the layout.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn shipped_projects_reproduce_their_data_sections() {
    let sdk = common::sdk();
    let quests = sdk.join("OriginalQuests");

    // Project order. It decides which duplicate key comes first.
    let cases: [(&str, &str, &[&str]); 2] = [
        (
            "base game",
            "Data/bytecode.bcd",
            &[
                "GPL/Flags.dat",
                "GPL/Hero_Data.dat",
                "GPL/Monster_Data.dat",
                "GPL/Building_Data.dat",
                "GPL/Misc_Data.dat",
                "GPL/Spell_Data.dat",
            ],
        ),
        (
            "expansion",
            "DataMX/MX_Data.bcd",
            &[
                "GPLMx/mx_Flags.dat",
                "GPLMx/mx_Building_Data.dat",
                "GPLMx/mx_Monster_Data.dat",
                "GPLMx/mx_Spell_Data.dat",
                "GPLMx/mx_Misc_Data.dat",
                "GPLMx/mx_Hero_Data.dat",
            ],
        ),
    ];

    for (label, output, inputs) in cases {
        let paths: Vec<PathBuf> = inputs.iter().map(|p| quests.join(p)).collect();

        assert_eq!(
            section_bytes(&paths),
            shipped_section(&quests.join(output)),
            "{label}: recompiled data section differs from {output}"
        );
    }
}

/// `Hooligan` sets `castingrange` to 250, then 200. The shipped output keeps
/// 250.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn first_of_two_duplicate_keys_wins() {
    let sdk = common::sdk();
    let body = shipped_section(&sdk.join("OriginalQuests/Data/bytecode.bcd"));
    let section = DataSection::decode(&body).expect("data section");

    let hooligan = section
        .prototypes
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("Hooligan"))
        .expect("Hooligan is in the base game");

    let range = hooligan
        .values
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("castingrange"))
        .expect("castingrange is set");

    assert_eq!(range.1, "250");
    assert_eq!(
        hooligan
            .values
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case("castingrange"))
            .count(),
        1,
        "the repeat should have been dropped"
    );
}

/// Every project on disk against every data section on disk. The data section
/// ignores code, so mixed projects count too.
///
/// Paired by matching output: a project does not record where its `.bcd` goes.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_project_reproduces_a_shipped_data_section() {
    let roots = [common::sdk(), common::workshop()];

    let shipped: Vec<Vec<u8>> = every_bcd().iter().map(|p| shipped_section(p)).collect();
    let mut projects: Vec<PathBuf> = roots
        .iter()
        .flat_map(|root| common::walk(root, &["gplproj"]))
        .collect();

    projects.sort();

    let mut matched = 0;
    let mut unmatched = Vec::new();

    for project in &projects {
        let Ok(ours) = emit::project_data_section(project) else {
            continue;
        };

        // Eight bytes is the empty section: no `data=`.
        if ours.len() > 8 {
            if shipped.contains(&ours) {
                matched += 1;
            } else {
                unmatched.push(project.clone());
            }
        }
    }

    let known_drift: Vec<_> = unmatched
        .iter()
        .filter(|p| !p.starts_with(drifted_mod()))
        .collect();

    assert!(
        known_drift.is_empty(),
        "projects that no longer reproduce their output: {known_drift:#?}"
    );
    assert!(
        matched >= PROJECTS_MATCHED,
        "only {matched} of {} projects reproduced a shipped data section",
        projects.len()
    );
}

/// A mixed project's data section decodes and round-trips on its own.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn mixed_projects_keep_a_decodable_data_section() {
    let sdk = common::sdk();

    for (project, output, protos) in [
        ("Example", "Example/Data/WrathOfKrolm.bcd", 5),
        (
            "SpecialItemsExample",
            "SpecialItemsExample/Data/SpecialItemsExample.bcd",
            2,
        ),
    ] {
        let body = shipped_section(&sdk.join(output));
        let section = DataSection::decode(&body).unwrap_or_else(|e| panic!("{project}: {e}"));

        assert_eq!(section.encode(), body, "{project}");
        assert!(
            section.prototypes.len() >= protos,
            "{project}: {} prototypes",
            section.prototypes.len()
        );
    }
}

// --------------------------------------------------------------------------
// Section 2's name tables, read by `gpl::bcd::ScopeSection`.
// --------------------------------------------------------------------------

/// A file's symbol count and its code section bytes. Owned: a `ScopeSection`
/// borrows its buffer.
fn code_of(path: &Path) -> (u32, Vec<u8>) {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let program =
        bcd::Container::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let sections = program
        .sections()
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    let symbols =
        symbol_count(sections.symbols.body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    (symbols, sections.code.body.to_vec())
}

/// Section 2 holds three tables: prototypes, functions, constants. Section 1's
/// count is its distinct names, on every file — `Gplbcc.exe`'s "Serializing
/// symbol table..".
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn section_one_counts_what_section_two_defines() {
    let files = every_bcd();
    let (mut prototypes, mut functions, mut constants) = (0, 0, 0);
    let mut smallest = usize::MAX;

    for path in &files {
        let (symbols, body) = code_of(path);
        let code =
            ScopeSection::decode(&body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        assert_eq!(code.encode(), body, "{}", path.display());

        // Distinct, as GPL ignores case: Majestic Majesty defines
        // `StringInList` twice, one symbol-table name.
        assert_eq!(
            symbols as usize,
            code.distinct_names(),
            "{}: symbol count against definitions",
            path.display()
        );

        for definition in code.definitions() {
            assert_eq!(
                definition.body.len() % DEFINITION_RECORD,
                0,
                "{}: {} is {} bytes",
                path.display(),
                definition.name,
                definition.body.len()
            );
        }

        smallest = smallest.min(
            code.definitions()
                .map(|d| d.body.len())
                .min()
                .unwrap_or(usize::MAX),
        );

        prototypes += code.prototypes.len();
        functions += code.functions.len();
        constants += code.constants.len();
    }

    assert_eq!(files.len(), BCD_FILES, "the .bcd corpus");
    assert_eq!(
        (prototypes, functions, constants),
        (253, 11_473, 7_741),
        "the census of what section 2 defines"
    );
    assert_eq!(
        smallest, DEFINITION_RECORD,
        "the smallest definition body changed"
    );
}

/// The base game's code section holds what its sources define.
///
/// `path.gplproj` lists 141 of the 143 files in `GPL/`. The SDK's
/// `bytecode.bcd` has 825 functions.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_base_game_code_section_matches_its_sources() {
    let sdk = common::sdk();

    let (_, hd_body) = code_of(&sdk.join("OriginalQuests/Data/bytecode.bcd"));

    let hd = ScopeSection::decode(&hd_body).expect("the HD build's code section");

    assert_eq!((hd.prototypes.len(), hd.constants.len()), (20, 649), "HD");
    assert_eq!(hd.functions.len(), 825, "HD function count");

    assert_eq!(
        hd.prototypes[0].name, "AIRoot",
        "the first prototype changed"
    );

    // Against the sources.
    let mut prototypes = 0;
    let mut constants = 0;

    for (_, src) in project_sources(&sdk.join("OriginalQuests/GPL/path.gplproj")) {
        for item in &regent_core::gpl::parse(&src).script.items {
            match item {
                regent_core::gpl::ast::Item::Prototype(_) => prototypes += 1,
                regent_core::gpl::ast::Item::Constant(_) => constants += 1,
                regent_core::gpl::ast::Item::Function(_) => {}
            }
        }
    }

    assert_eq!(
        prototypes,
        hd.prototypes.len(),
        "prototypes, source against image"
    );
    assert_eq!(
        constants,
        hd.constants.len(),
        "constants, source against image"
    );
}

/// The `.gpl` files a project lists, in order, with their text.
fn project_sources(project: &Path) -> Vec<(PathBuf, String)> {
    let text = std::fs::read_to_string(project).expect("a readable project");
    let dir = project.parent().expect("a project directory");

    proj::parse(&text, dir)
        .sources
        .iter()
        .map(|listed| {
            let path = regent_core::content::on_disk(listed)
                .unwrap_or_else(|| panic!("{} is not on disk", listed.display()));
            let bytes = std::fs::read(&path).expect("a readable source file");

            (path, String::from_utf8_lossy(&bytes).into_owned())
        })
        .collect()
}

/// Section 2 is a scope table, not the instruction stream.
///
/// Record count = 1 + names (parameters and `declare`s) declared by this and
/// every earlier definition in the same file. A running total, reset per file.
///
/// So size follows position, not content: `Cultist_tree` has 26 statements and
/// two records, `end_game_script_expert` five and 449.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn section_two_records_count_the_names_in_scope() {
    let quests = common::sdk().join("OriginalQuests");

    // Every project whose output is identified. All but the first (2,050
    // definitions) were not used to fit the rule.
    let projects: [(&str, &str, usize); 6] = [
        ("GPL/path.gplproj", "Data/bytecode.bcd", 1_494),
        ("GPLMx/Path_Build.gplproj", "DataMX/MX_Build.bcd", 279),
        ("GPLMx/Path_Data.gplproj", "DataMX/MX_Data.bcd", 1_088),
        ("GPLMx/Path_Decision.gplproj", "DataMX/MX_Decision.bcd", 103),
        ("GPLMx/Path_Task.gplproj", "DataMX/MX_Task.bcd", 560),
        (
            "../Example/GPL/WrathOfKrolm_Path.gplproj",
            "../Example/Data/WrathOfKrolm.bcd",
            20,
        ),
    ];

    let mut total = 0;

    for (project, output, expected) in projects {
        let (_, body) = code_of(&quests.join(output));
        let scopes = ScopeSection::decode(&body).expect("section 2");

        // Case-folded. A constant is `#Name` in the image, `Name` in the tree.
        let records: BTreeMap<String, usize> = scopes
            .definitions()
            .map(|d| {
                (
                    d.name.to_ascii_lowercase(),
                    d.body.len() / DEFINITION_RECORD,
                )
            })
            .collect();

        let mut checked = 0;

        for (path, src) in project_sources(&quests.join(project)) {
            let script = regent_core::gpl::parse(&src).script;
            let mut in_scope = 0;

            for item in &script.items {
                in_scope += item.declared_names();

                let key = match item {
                    regent_core::gpl::ast::Item::Constant(c) => {
                        format!("#{}", c.name.to_ascii_lowercase())
                    }
                    other => other.name().to_ascii_lowercase(),
                };

                let got = records
                    .get(&key)
                    .unwrap_or_else(|| panic!("{}: {key} is not in {output}", path.display()));

                assert_eq!(
                    *got,
                    in_scope + 1,
                    "{}: {key} has {got} records, {} names are in scope",
                    path.display(),
                    in_scope
                );

                checked += 1;
            }
        }

        assert_eq!(checked, expected, "{project}: definition count changed");
        assert_eq!(
            checked,
            scopes.len(),
            "{project}: image and sources disagree"
        );
        total += checked;
    }

    assert_eq!(
        total, 3_544,
        "definitions covered by the scope rule changed"
    );
}

/// Section 1's constant definitions against their sources: name, file, line,
/// and integer value. The line is a source line, so section 1 carries debug
/// info.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn section_one_records_a_constant_s_file_line_and_value() {
    let quests = common::sdk().join("OriginalQuests");

    let mut all_floats = 0;

    for (project, output, expected) in [
        ("GPL/path.gplproj", "Data/bytecode.bcd", 649),
        ("GPLMx/Path_Data.gplproj", "DataMX/MX_Data.bcd", 862),
    ] {
        let data = std::fs::read(quests.join(output)).expect("the image");
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let scopes = ScopeSection::decode(sections.code.body).expect("section 2");
        let walked = walk(sections.symbols.body).expect("section 1");
        let constants: Vec<&Compiled<'_>> =
            walked.iter().filter(|d| d.name.starts_with('#')).collect();

        // Section 2 gives the count.
        assert_eq!(constants.len(), scopes.constants.len(), "{output}");
        assert_eq!(constants.len(), expected, "{output}: constants");

        // Keyed as the image spells it.
        let mut declared: BTreeMap<String, (String, usize, Option<i64>)> = BTreeMap::new();

        for (path, src) in project_sources(&quests.join(project)) {
            let file = path.file_name().unwrap().to_string_lossy().to_lowercase();

            for item in &regent_core::gpl::parse(&src).script.items {
                if let regent_core::gpl::ast::Item::Constant(c) = item {
                    let (line, _) = regent_core::gpl::diag::location(&src, c.span.start);
                    let value = match c.value.kind {
                        regent_core::gpl::ast::ExprKind::Int(n) => Some(n),
                        _ => None,
                    };

                    declared
                        .entry(format!("#{}", c.name.to_ascii_lowercase()))
                        .or_insert((file.clone(), line, value));
                }
            }
        }

        let (mut values, mut floats) = (0, 0);

        for d in &constants {
            let key = d.name.to_ascii_lowercase();
            let (file, line, value) = declared
                .get(&key)
                .unwrap_or_else(|| panic!("{output}: {} is not in the sources", d.name));

            // Record 0 names the file, Windows spelling: compare the last
            // component, case-folded.
            let head = &d.records[0];
            let got = head.text.rsplit(['\\', '/']).next().unwrap().to_lowercase();

            assert_eq!(&got, file, "{output}: {} source file", d.name);
            assert_eq!(usize::from(head.line), *line, "{output}: {} line", d.name);

            let slot = d.records[CONSTANT_RECORD].values[0];

            #[allow(clippy::cast_possible_wrap)]
            match (TypeCode::try_from(slot.kind), value) {
                (Ok(TypeCode::Integer), Some(want)) => {
                    assert_eq!(i64::from(slot.word as i32), *want, "{output}: {}", d.name);
                    values += 1;
                }
                (Ok(TypeCode::Float), _) => floats += 1,
                (found, _) => panic!("{output}: {} holds {found:?}", d.name),
            }
        }

        assert_eq!(
            values + floats,
            expected,
            "{output}: every constant classified"
        );
        all_floats += floats;
    }

    // Two per edition, all `1.5`. For the value's encoding see
    // `a_float_literal_is_fixed_point_with_a_scale_of_1024`.
    assert_eq!(all_floats, 4, "float constants changed");
}

/// A construct with no specimen stops the build instead of being left out.
///
/// `do while` is in the grammar but no corpus uses it.
#[test]
fn a_definition_with_no_specimen_stops_the_section() {
    let source =
        "function Loop()\ndeclare\n\tinteger n;\nbegin\n\tdo\n\t\tn++;\n\twhile (n < 3);\nend\n";
    let script = regent_core::gpl::parse(source).script;

    assert_eq!(script.items.len(), 1, "the sample stopped parsing");
    assert!(
        emit::script(&script, source, "loop.gpl").is_none(),
        "a do while was emitted from no specimen"
    );

    // A plain `while` emits.
    let plain = "function Loop()\ndeclare\n\tinteger n;\nbegin\n\twhile (n < 3)\n\t\tn++;\nend\n";
    let script = regent_core::gpl::parse(plain).script;

    assert!(emit::script(&script, plain, "loop.gpl").is_some());
}

/// Every type a constant's or prototype's record carries in its first value
/// slot is one the engine names, the field [`types`] numbers.
///
/// Constants and prototypes only. One-based: never zero.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_type_a_record_carries_is_in_the_engines_list() {
    let mut seen: BTreeSet<u8> = BTreeSet::new();
    let mut records = 0usize;

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for d in walked.iter().filter(|d| {
            d.records
                .first()
                .is_none_or(|r| r.opcode != Op::Advance.code())
        }) {
            for r in &d.records {
                seen.insert(r.values[0].kind);
                records += 1;
            }
        }
    }

    assert_eq!(records, RECORDS_OUTSIDE_FUNCTIONS, "records walked");

    for code in &seen {
        assert!(
            gpltype::name(*code).is_some(),
            "a record carries type {code}, which the engine does not name"
        );
    }

    assert!(!seen.contains(&0), "a record carried type 0");

    // Only `reference` is unused.
    let used: BTreeSet<&str> = seen.iter().filter_map(|c| gpltype::name(*c)).collect();

    assert!(!used.contains("reference"), "something now uses reference");
    assert_eq!(used.len(), gpltype::TABLE.len() - 1, "{used:?}");
}

/// Records of the constants and prototypes in every `.bcd` on disk.
const RECORDS_OUTSIDE_FUNCTIONS: usize = 36_819;

// ---------------------------------------------------------------------------
// Section 1's record stream, walked rather than scanned.

/// A file's symbol-section bytes, owned as in [`code_of`].
fn symbols_of(path: &Path) -> Vec<u8> {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let program =
        bcd::Container::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    program
        .sections()
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .symbols
        .body
        .to_vec()
}

/// The record walk consumes section 1 exactly, on every `.bcd` on disk.
///
/// [`walk`] searches for nothing: counts and lengths are stated, so it lands
/// on the last byte or fails.
///
/// **Binary**: shape from `Gplbcc.exe`'s writer game start — three typed
/// value slots, a `u32`, a string, a `u16` line, an operand count, that many
/// `u16` slots, an opcode.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_record_walk_consumes_section_one_exactly() {
    let files = every_bcd();
    let mut definitions = 0;
    let mut records = 0;

    for path in &files {
        let symbols = symbols_of(path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        definitions += walked.len();
        records += walked.iter().map(|d| d.records.len()).sum::<usize>();
    }

    assert_eq!(files.len(), 75, "the .bcd corpus");
    assert_eq!(definitions, 19_466, "definitions walked");
    assert_eq!(records, 1_262_666, "records walked");
}

/// Opcode 38, `opLParen` (token `call`): 7,060 in HD's image, as the parser's
/// in the SDK source.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_walk_counts_the_calls_the_parser_counts() {
    const CALL: u8 = regent_core::gpl::opcode::CALL;

    let symbols = symbols_of(&common::hd().join("Data/bytecode.bcd"));
    let walked = walk(&symbols).expect("the base game walks");

    let calls = walked
        .iter()
        .flat_map(|d| &d.records)
        .filter(|r| r.opcode == CALL)
        .count();

    assert_eq!(walked.len(), 1_494, "definitions in the base game");
    assert_eq!(calls, 7_060, "calls the walk finds");
}

/// `Acid_Bolt_Hit`, the `docs/formats/bytecode.md` specimen, walked. `12 <- (10 11)`
/// then `13 <- (8 9 12)`: the inner result is the outer's last argument.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn acid_bolt_hit_walks_into_the_calls_its_source_makes() {
    let symbols = symbols_of(&common::hd().join("Data/bytecode.bcd"));
    let walked = walk(&symbols).expect("the base game walks");

    let found = walked
        .iter()
        .find(|d| d.name == "Acid_Bolt_Hit")
        .expect("Acid_Bolt_Hit is in the base game");

    // Source file, own name, then parameters.
    assert_eq!(found.records[0].text, r"TaskModules\SubTasks\Spells.gpl");
    assert_eq!(found.records[1].text, "Acid_Bolt_Hit");
    assert_eq!(found.records[4].text, "__Return_value__");
    assert_eq!(found.records[5].text, "thisagent");
    assert_eq!(found.records[6].text, "target");

    let calls: Vec<(&str, &[u16])> = found
        .records
        .iter()
        .filter(|r| r.opcode == regent_core::gpl::opcode::CALL)
        .map(|r| (r.text, r.slots.as_slice()))
        .collect();

    assert_eq!(
        calls,
        [
            ("", &[12, 10, 11][..]),
            ("", &[13, 8, 9, 12][..]),
            ("", &[17, 14, 15, 16][..]),
        ],
        "the two nested calls and the plain one"
    );
}

/// Opcode 39, one past the engine's table, separates a function's declarations
/// from its body.
///
/// `gpl::opcode::TABLE` has 39 entries (0-38), from all three executables. 39
/// appears once per function, never in a constant or prototype.
///
/// It sits at record six to eleven. Before it only the six declarative opcodes
/// below; everything executable follows. Its name is in neither executable nor
/// debugger.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn opcode_thirty_nine_separates_declarations_from_the_body() {
    use regent_core::gpl::opcode;

    /// One past the last named entry.
    const UNNAMED: u8 = 39;

    // The only opcodes a prologue uses, looked up by name.
    let declarative: Vec<u8> = [
        "opNop",
        "opLocalize",
        "opReference",
        "opDeclare",
        "opAdvance",
        "opExit",
    ]
    .iter()
    .map(|want| {
        let at = opcode::TABLE.iter().position(|e| e.name == *want);

        u8::try_from(at.unwrap_or_else(|| panic!("the table lost {want}"))).expect("under 39")
    })
    .collect();

    let path = common::hd().join("Data/bytecode.bcd");
    let data = std::fs::read(&path).expect("the base game");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let scopes = ScopeSection::decode(sections.code.body).expect("the scope table");
    let walked = walk(sections.symbols.body).expect("the base game walks");

    let total: usize = walked
        .iter()
        .flat_map(|d| &d.records)
        .filter(|r| r.opcode == UNNAMED)
        .count();

    assert_eq!(
        usize::from(UNNAMED),
        opcode::TABLE.len(),
        "the table grew, so 39 may now have a name"
    );
    assert_eq!(total, scopes.functions.len(), "one per function");
    assert_eq!(total, 825, "functions in the Linux base game");

    let mut last = 0;

    for definition in &walked {
        let ops: Vec<u8> = definition.records.iter().map(|r| r.opcode).collect();
        let is_function = scopes.functions.iter().any(|d| d.name == definition.name);
        let at = ops.iter().position(|op| *op == UNNAMED);

        // Once per function, nowhere else.
        assert_eq!(
            at.is_some(),
            is_function,
            "{} carries opcode {UNNAMED} and should not, or the reverse",
            definition.name
        );
        assert_eq!(
            at,
            ops.iter().rposition(|op| *op == UNNAMED),
            "{} has more than one body boundary",
            definition.name
        );

        let Some(at) = at else { continue };

        // Nothing executable before it.
        for op in &ops[..at] {
            assert!(
                declarative.contains(op),
                "{} runs opcode {op} before its body starts",
                definition.name
            );
        }

        // A boundary, not a terminator: last only in empty bodies.
        last += usize::from(at + 1 == ops.len());
    }

    assert_eq!(
        last, 5,
        "functions whose body is empty, so the boundary is also the last record"
    );
}

/// Section 1 round-trips on every `.bcd` on disk.
///
/// Catches what an exact walk alone would allow: a field skipped at the right
/// length.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_code_section_round_trips() {
    let files = every_bcd();
    let mut bytes = 0;

    for path in &files {
        let symbols = symbols_of(path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        assert_eq!(encode_code(&walked), symbols, "{}", path.display());
        bytes += symbols.len();
    }

    assert_eq!(files.len(), 75, "the .bcd corpus");
    assert_eq!(bytes, 63_761_000, "bytes of section 1 round-tripped");
}

/// A record's `head` is set on exactly the control-transfer opcodes.
///
/// No opcode carries a `head` only sometimes. Seven always do: `opIf`,
/// `opWhile`, `opContinue`, `opBreak`, the `foreach` cursor pair
/// `opAdvance`/`opRewind`, and `opLocalize` (a parameter index, not a target).
///
/// **Inferred**: `head` is a jump target. Checked by
/// `a_jumps_head_lands_on_a_record`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_records_head_is_set_on_exactly_the_control_transfer_opcodes() {
    use regent_core::gpl::opcode;

    let mut set: BTreeMap<u8, usize> = BTreeMap::new();
    let mut seen: BTreeMap<u8, usize> = BTreeMap::new();

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for record in walked.iter().flat_map(|d| &d.records) {
            *seen.entry(record.opcode).or_default() += 1;

            if record.head != 0 {
                *set.entry(record.opcode).or_default() += 1;
            }
        }
    }

    let mixed: Vec<u8> = seen
        .iter()
        .filter(|(op, n)| matches!(set.get(op), Some(k) if k != *n))
        .map(|(op, _)| *op)
        .collect();

    assert!(
        mixed.is_empty(),
        "opcodes that sometimes carry a head: {mixed:?}"
    );

    let named: Vec<&str> = set
        .keys()
        .map(|op| {
            opcode::TABLE
                .get(usize::from(*op))
                .map_or("(39)", |entry| entry.name)
        })
        .collect();

    assert_eq!(
        named,
        [
            "opLocalize",
            "opAdvance",
            "opRewind",
            "opIf",
            "opWhile",
            "opContinue",
            "opBreak",
        ],
        "the opcodes carrying a head changed"
    );
}

/// A record's opening slot is a header, not a third value.
///
/// Same layout as the two typed slots, but on every record its string, second
/// word and trailing byte are empty. Only `word` (0 or `u32::MAX`) and `tag`
/// (1, 10, 11, 12) vary. 12 is past the eleven type names, so `tag` is not a
/// type.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_records_opening_slot_carries_no_payload() {
    let mut words = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut records = 0;

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for record in walked.iter().flat_map(|d| &d.records) {
            words.insert(record.mark.word);
            tags.insert(record.mark.tag);
            records += 1;
        }
    }

    assert_eq!(records, 1_262_666, "records seen");
    assert_eq!(
        words.into_iter().collect::<Vec<u32>>(),
        [0, u32::MAX],
        "the mark's word took a value between"
    );
    assert_eq!(
        tags.into_iter().collect::<Vec<u8>>(),
        [1, 10, 11, 12],
        "the mark's tag changed"
    );
}

/// The prologue follows from the signature. On every `.bcd`:
///
/// * the first record is `opAdvance` head 4 (function) or `opNop` head 0
///   (constant, prototype);
/// * a function's record 3 has its record count in `head` and no source line;
/// * `opLocalize`'s `head` is the parameter's 1-based position, and its
///   first value has no text.
///
/// So `head`'s meaning is the opcode's: record index or parameter index.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_definitions_prologue_follows_from_its_signature() {
    use regent_core::gpl::opcode;

    /// The `u16` a record carries when it has no source line.
    const NO_LINE: u16 = u16::MAX;

    let index = |want: &str| {
        opcode::TABLE
            .iter()
            .position(|e| e.name == want)
            .and_then(|at| u8::try_from(at).ok())
            .unwrap_or_else(|| panic!("the table lost {want}"))
    };
    let (advance, nop, localize) = (index("opAdvance"), index("opNop"), index("opLocalize"));

    let mut opens = BTreeMap::new();
    let mut counted = 0;
    let mut parameters = 0;

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for definition in &walked {
            let first = &definition.records[0];
            *opens.entry((first.opcode, first.head)).or_insert(0usize) += 1;

            if first.opcode == advance {
                let third = &definition.records[3];

                assert_eq!(
                    third.head as usize,
                    definition.records.len(),
                    "{} does not count its own records",
                    definition.name
                );
                assert_eq!(
                    third.line, NO_LINE,
                    "{} record 3 has a line",
                    definition.name
                );
                counted += 1;
            }

            let mut nth = 0;

            for record in &definition.records {
                if record.opcode == localize {
                    nth += 1;
                    assert_eq!(
                        record.head, nth,
                        "{} numbers its parameters differently",
                        definition.name
                    );
                    // The debug type check prints this text.
                    assert!(
                        record.values[0].text.is_empty(),
                        "{} gives a parameter text",
                        definition.name
                    );
                    parameters += 1;
                }
            }
        }
    }

    assert_eq!(
        opens,
        BTreeMap::from([((advance, 4), 11_472), ((nop, 0), 7_994)]),
        "a definition opened some other way"
    );
    assert_eq!(counted, 11_472, "functions");
    assert_eq!(parameters, 14_743, "parameters numbered");
}

/// A record's index is its expression slot number.
///
/// A call's result slot is always its own record index, so record *i* computes
/// slot *i*.
///
/// Specimen `Fire_Strike_Active`,
/// `if (target != $nullagent()) $Fire_Strike_Damage(target);`:
///
/// ```text
///  [6] opLocalize  target
///  [8] opLParen    slots [8]      $nullagent()  -> slot 8
///  [9] opNotEqual                 slots 6 and 8 -> slot 9
/// [10] opIf        head 13        on slot 9
/// [11] opNop                      slot 6        -> slot 11
/// [12] opLParen    slots [12, 11] $Fire_Strike_Damage(<11>)
/// ```
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_records_index_is_its_expression_slot() {
    let mut named = 0;

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for definition in &walked {
            for (at, record) in definition.records.iter().enumerate() {
                if record.opcode != regent_core::gpl::opcode::CALL {
                    continue;
                }

                let result = record.slots.first().copied().map(usize::from);

                assert_eq!(
                    result,
                    Some(at),
                    "{} record {at} sends its result elsewhere",
                    definition.name
                );
                named += 1;
            }
        }
    }

    assert_eq!(named, 221_978, "calls checked");
}

/// A jump's head is a record index in `0..=records`.
///
/// Backward jumps show these are targets, not counters: a jump back is a loop.
/// Only `opRewind` and `opWhile` make them.
///
/// One past the end is allowed: `Fire_Strike_Active`'s `opIf` heads to 13 of 13
/// records, skipping its body.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_jumps_head_lands_on_a_record() {
    use regent_core::gpl::opcode;

    let jumps: Vec<u8> = [
        "opAdvance",
        "opRewind",
        "opIf",
        "opWhile",
        "opContinue",
        "opBreak",
    ]
    .iter()
    .map(|want| {
        let at = opcode::TABLE.iter().position(|e| e.name == *want);

        u8::try_from(at.unwrap_or_else(|| panic!("the table lost {want}"))).expect("under 39")
    })
    .collect();

    let (mut forward, mut backward) = (0, 0);

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for definition in &walked {
            let end = definition.records.len();

            for (at, record) in definition.records.iter().enumerate() {
                if !jumps.contains(&record.opcode) {
                    continue;
                }

                let target = record.head as usize;

                assert!(
                    target <= end,
                    "{} jumps to {target} of {end} records",
                    definition.name
                );

                if target > at {
                    forward += 1;
                } else {
                    backward += 1;
                }
            }
        }
    }

    assert_eq!(forward, 184_319, "forward jumps");
    assert_eq!(backward, 3_812, "backward jumps, which are loops");
}

/// The prologue matches the signature, name and type, on every function of
/// the SDK's `OriginalQuests`:
///
/// * `opLocalize` records are the parameters, in order;
/// * `opDeclare` records are the `declare` names, in order, multi-name lines
///   flattened;
/// * each carries its type in its first value slot, one-based into
///   [`regent_core::data::gpltype`].
///
/// Uses the SDK's `.bcd`: `SDK/Documentation/GPL Debugger.pdf` says the
/// debugger loads it, and its paths and line numbers match the SDK sources.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_prologue_pairs_with_the_signature_it_came_from() {
    use regent_core::gpl::ast::{Item, Type};
    use regent_core::gpl::opcode;

    let index = |want: &str| {
        opcode::TABLE
            .iter()
            .position(|e| e.name == want)
            .and_then(|at| u8::try_from(at).ok())
            .expect("an opcode")
    };
    let (localize, declare) = (index("opLocalize"), index("opDeclare"));

    // One-based.
    let code = |ty: Type| {
        let want = match ty {
            Type::Integer => "integer",
            Type::Float => "float",
            Type::String => "string",
            Type::List => "list",
            Type::Agent => "agent",
            Type::Boolean => "boolean",
            Type::Coordinate => "coord",
            Type::Function => "function",
        };
        let at = regent_core::data::gpltype::TABLE
            .iter()
            .position(|n| *n == want)
            .expect("a type");

        u8::try_from(at + 1).expect("under 12")
    };

    let quests = common::sdk().join("OriginalQuests");
    let sources = project_sources(&quests.join("GPL/path.gplproj"));
    let trees: Vec<_> = sources
        .iter()
        .map(|(_, text)| regent_core::gpl::parse(text).script)
        .collect();

    let data = std::fs::read(quests.join("Data/bytecode.bcd")).expect("the SDK image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("the SDK image walks");

    let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();
    let (mut functions, mut parameters, mut locals) = (0, 0, 0);

    for script in &trees {
        for item in &script.items {
            let Item::Function(f) = item else { continue };
            let Some(definition) = compiled.get(f.name) else {
                continue;
            };

            let taken = |op: u8| -> Vec<(&str, u8)> {
                definition
                    .records
                    .iter()
                    .filter(|r| r.opcode == op)
                    .map(|r| (r.text, r.values[0].kind))
                    .collect()
            };

            let wanted: Vec<(&str, u8)> = f.params.iter().map(|p| (p.name, code(p.ty))).collect();

            assert_eq!(taken(localize), wanted, "{}'s parameters", f.name);

            // One record per name, even on a multi-name `declare` line.
            let declared: Vec<(&str, u8)> = f
                .decls
                .iter()
                .flat_map(|d| d.names.iter().map(move |(n, _)| (*n, code(d.ty))))
                .collect();

            assert_eq!(taken(declare), declared, "{}'s locals", f.name);

            functions += 1;
            parameters += wanted.len();
            locals += declared.len();
        }
    }

    assert_eq!(functions, 825, "functions paired");
    assert_eq!(parameters, 1_003, "parameters paired");
    assert_eq!(locals, 1_926, "locals paired");
}

/// Every call node of a function body, in post-order. `None` where the callee
/// is not a plain `$Name`, e.g. `(expr)(...)` or `id's "attr"(...)`.
fn call_order<'a>(stmt: &regent_core::gpl::ast::Stmt<'a>, out: &mut Vec<Option<&'a str>>) {
    use regent_core::gpl::ast::StmtKind;

    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Return(Some(e)) => call_order_expr(e, out),
        StmtKind::Block(v) => {
            for s in v {
                call_order(s, out);
            }
        }
        StmtKind::If { cond, then, els } => {
            call_order_expr(cond, out);
            call_order(then, out);

            if let Some(els) = els {
                call_order(els, out);
            }
        }
        StmtKind::While { cond, body } => {
            call_order_expr(cond, out);
            call_order(body, out);
        }
        StmtKind::DoWhile { body, cond } => {
            call_order(body, out);
            call_order_expr(cond, out);
        }
        StmtKind::Foreach { var, iter, body } => {
            call_order_expr(var, out);
            call_order_expr(iter, out);
            call_order(body, out);
        }
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
    }
}

fn call_order_expr<'a>(expr: &regent_core::gpl::ast::Expr<'a>, out: &mut Vec<Option<&'a str>>) {
    use regent_core::gpl::ast::ExprKind;

    match &expr.kind {
        ExprKind::Call { callee, args } => {
            call_order_expr(callee, out);

            for a in args {
                call_order_expr(a, out);
            }

            out.push(match &callee.kind {
                ExprKind::Func(name) => Some(*name),
                _ => None,
            });
        }
        ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
            call_order_expr(operand, out);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Assign { lhs, rhs, .. } => {
            call_order_expr(lhs, out);
            call_order_expr(rhs, out);
        }
        ExprKind::Attr { obj, attr } => {
            call_order_expr(obj, out);
            call_order_expr(attr, out);
        }
        _ => {}
    }
}

/// A function's `opLParen` records name the same callees, in the same order,
/// as a post-order walk of its tree.
///
/// The exception is `Guardian_Birth`'s `($Monster_Birth)(ThisAgent);`, the
/// corpus's one parenthesised callee:
///
/// ```text
/// [10] opNop                    $Monster_Birth by name  -> slot 10
/// [11] opNop                    slot 5 (ThisAgent)      -> slot 11
/// [12] opLParen slots [12, 11]  callee = slot 10, by reference
/// ```
///
/// So the callee is named (type `function`) or a slot reference; see
/// `every_calls_callee_is_a_name_or_a_slot`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_bodys_calls_are_the_trees_calls_in_post_order() {
    use regent_core::gpl::ast::Item;

    let quests = common::sdk().join("OriginalQuests");
    let sources = project_sources(&quests.join("GPL/path.gplproj"));
    let trees: Vec<_> = sources
        .iter()
        .map(|(_, text)| regent_core::gpl::parse(text).script)
        .collect();

    let data = std::fs::read(quests.join("Data/bytecode.bcd")).expect("the SDK image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("the SDK image walks");
    let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

    let (mut agreed, mut calls) = (0, 0);
    let mut differed = Vec::new();

    for script in &trees {
        for item in &script.items {
            let Item::Function(f) = item else { continue };
            let Some(definition) = compiled.get(f.name) else {
                continue;
            };

            let mut wanted = Vec::new();

            for s in &f.body {
                call_order(s, &mut wanted);
            }

            let taken: Vec<&str> = definition
                .records
                .iter()
                .filter(|r| r.opcode == regent_core::gpl::opcode::CALL)
                .map(|r| r.values[0].text)
                .collect();

            let same = wanted.len() == taken.len()
                && wanted
                    .iter()
                    .zip(&taken)
                    .all(|(w, t)| w.is_none_or(|w| w.eq_ignore_ascii_case(t)));

            if same {
                agreed += 1;
                calls += taken.len();
            } else {
                differed.push(f.name);
            }
        }
    }

    assert_eq!(agreed, 824, "functions whose call order matches by name");
    assert_eq!(calls, 7_058, "calls paired");
    assert_eq!(
        differed,
        ["Guardian_Birth"],
        "the indirect call is the only one"
    );

    // The exception resolves through a slot.
    let odd = compiled["Guardian_Birth"];
    let call = odd
        .records
        .iter()
        .rev()
        .find(|r| r.opcode == regent_core::gpl::opcode::CALL)
        .expect("the indirect call");

    assert_eq!(call.values[0].text, "", "the callee is not named");
    assert_eq!(call.values[0].flag, 2, "the callee is not a reference");

    let callee = &odd.records[call.values[0].word as usize];

    assert_eq!(callee.values[0].text, "Monster_Birth");
    assert_eq!(
        regent_core::data::gpltype::name(callee.values[0].kind),
        Some("function"),
        "what the slot holds"
    );
}

/// On every `.bcd`, a call's first value slot is a name typed `function`, or an
/// unnamed slot reference.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_calls_callee_is_a_name_or_a_slot() {
    let (mut named, mut through) = (0, 0);

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for definition in &walked {
            for record in &definition.records {
                if record.opcode != regent_core::gpl::opcode::CALL {
                    continue;
                }

                let callee = &record.values[0];

                if callee.text.is_empty() {
                    assert_eq!(
                        callee.flag, 2,
                        "{}: a call with no callee at all",
                        definition.name
                    );
                    through += 1;
                } else {
                    assert_eq!(
                        regent_core::data::gpltype::name(callee.kind),
                        Some("function"),
                        "{}: a named callee that is not a function",
                        definition.name
                    );
                    named += 1;
                }
            }
        }
    }

    assert_eq!(named, 221_724, "calls naming their callee");
    assert_eq!(through, 254, "calls through a slot");
}

/// The two typed slots are the operands; how many are used is the opcode's
/// arity, fixed per opcode except `opNop` (0 or 1):
///
/// * 2: the twenty binary operators, `opForeach` (variable, iterand),
///   `opReturnValue` (destination first);
/// * 1: `opInc`, `opDec`, `opNegate`, `opIf`, `opWhile`, `opLParen` (callee),
///   `opReturn`, and `opLocalize`, `opDeclare`, `opReference` (declared type);
/// * 0: pure control, and the unnamed 39.
///
/// An operand is a slot reference (`flag` 2, slot in `word`) or an immediate
/// (literal in `word` or `text`, type in `kind`). 56 `opPlus` records have no
/// reference operand: constants on both sides, unfolded.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn an_opcodes_arity_is_how_many_value_slots_it_uses() {
    use regent_core::gpl::opcode;

    /// `unknowntype`, carried by an unused slot.
    const UNUSED: u8 = 11;

    let mut seen: BTreeMap<u8, BTreeSet<usize>> = BTreeMap::new();

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for record in walked.iter().flat_map(|d| &d.records) {
            let used = record
                .values
                .iter()
                .filter(|v| v.flag == 2 || !v.text.is_empty() || v.kind != UNUSED)
                .count();

            seen.entry(record.opcode).or_default().insert(used);
        }
    }

    let name = |op: u8| {
        opcode::TABLE
            .get(usize::from(op))
            .map_or("(39)", |entry| entry.name)
    };

    let varies: Vec<&str> = seen
        .iter()
        .filter(|(_, arities)| arities.len() > 1)
        .map(|(op, _)| name(*op))
        .collect();

    assert_eq!(varies, ["opNop"], "opcodes with no settled arity");

    let mut table: BTreeMap<usize, Vec<&str>> = BTreeMap::new();

    for (op, arities) in &seen {
        if arities.len() == 1 {
            let only = *arities.iter().next().expect("one arity");

            table.entry(only).or_default().push(name(*op));
        }
    }

    for names in table.values_mut() {
        names.sort_unstable();
    }

    assert_eq!(
        table[&0],
        [
            "(39)",
            "opAdvance",
            "opBreak",
            "opContinue",
            "opExit",
            "opRewind"
        ],
        "the nullary opcodes"
    );
    assert_eq!(
        table[&1],
        [
            "opDec",
            "opDeclare",
            "opIf",
            "opInc",
            "opLParen",
            "opLocalize",
            "opNegate",
            "opReference",
            "opReturn",
            "opWhile",
        ],
        "the unary opcodes"
    );
    assert_eq!(
        table[&2],
        [
            "opAnd",
            "opApostrophe_s",
            "opAssignEqual",
            "opDivide",
            "opDivideEqual",
            "opEqual",
            "opForeach",
            "opGreaterThan",
            "opGreaterThanOrEqualTo",
            "opInserter",
            "opLessThan",
            "opLessThanOrEqualTo",
            "opMinus",
            "opMinusEqual",
            "opMod",
            "opNotEqual",
            "opOr",
            "opPlus",
            "opPlusEqual",
            "opReturnValue",
            "opStar",
            "opTimesEqual",
        ],
        "the binary opcodes"
    );
}

/// Marks per opcode, definitions with tag 1, and `opNop`'s split by arity.
struct Marks {
    by_opcode: BTreeMap<u8, BTreeSet<(bool, u8)>>,
    odd: Vec<String>,
    nop_with: usize,
    nop_without: usize,
}

/// Collects [`Marks`] over every `.bcd`.
fn marks_seen() -> Marks {
    use regent_core::gpl::opcode;

    /// `unknowntype`, carried by an unused slot.
    const UNUSED: u8 = 11;

    let mut marks: BTreeMap<u8, BTreeSet<(bool, u8)>> = BTreeMap::new();
    let mut odd: Vec<String> = Vec::new();
    let (mut with, mut without) = (0, 0);

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for definition in &walked {
            for record in &definition.records {
                let mark = (record.mark.word == u32::MAX, record.mark.tag);

                assert_eq!(
                    mark.0,
                    mark.1 == 10,
                    "{}: the mark's word and tag came apart",
                    definition.name
                );

                marks.entry(record.opcode).or_default().insert(mark);

                if record.mark.tag == 1 {
                    odd.push(definition.name.to_owned());
                }

                let is_nop = opcode::TABLE
                    .get(usize::from(record.opcode))
                    .is_some_and(|e| e.name == "opNop");

                if !is_nop {
                    continue;
                }

                // opNop's tag follows its arity, so it is recomputable.
                let used = record
                    .values
                    .iter()
                    .filter(|v| v.flag == 2 || !v.text.is_empty() || v.kind != UNUSED)
                    .count();

                assert_eq!(
                    used,
                    usize::from(record.mark.tag == 10),
                    "{}: opNop's mark does not follow its arity",
                    definition.name
                );

                if used == 1 {
                    with += 1;
                } else {
                    without += 1;
                }
            }
        }
    }

    Marks {
        by_opcode: marks,
        odd,
        nop_with: with,
        nop_without: without,
    }
}

/// The mark follows from the opcode. Word is `u32::MAX` exactly when tag is 10:
///
/// | Mark | Opcodes | Yields |
/// |---|---|---|
/// | `-1`, 10 | six assignment operators, `opInc`, `opDec`, `opLocalize`, `opDeclare` | a place |
/// | `0`, 12 | fourteen computing a fresh value | a temporary |
/// | `0`, 11 | the rest | neither |
///
/// `opNop`: tag 10 with an operand, 11 without.
///
/// One exception, unexplained: `Snake_Pit_Spawn`'s `opPlus` carries tag 1
/// (`integer`) instead of 12, in the three files shipping it. Its operands are
/// `integer` references, like 181,893 other `opPlus` records'.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_records_mark_is_decided_by_its_opcode() {
    use regent_core::gpl::opcode;

    let Marks {
        by_opcode: marks,
        odd,
        nop_with,
        nop_without,
    } = marks_seen();

    let name = |op: u8| {
        opcode::TABLE
            .get(usize::from(op))
            .map_or("(39)", |entry| entry.name)
    };

    let mut settled: BTreeMap<(bool, u8), Vec<&str>> = BTreeMap::new();
    let mut varies: Vec<&str> = Vec::new();

    for (op, seen) in &marks {
        if seen.len() == 1 {
            let only = *seen.iter().next().expect("one mark");

            settled.entry(only).or_default().push(name(*op));
        } else {
            varies.push(name(*op));
        }
    }

    for names in settled.values_mut() {
        names.sort_unstable();
    }

    varies.sort_unstable();

    assert_eq!(varies, ["opNop", "opPlus"], "opcodes with no settled mark");
    assert_eq!(
        odd, ["Snake_Pit_Spawn"; 3],
        "the records carrying a real type"
    );
    assert_eq!(nop_with, 438_362, "opNop with an operand");
    assert_eq!(nop_without, 27_460, "opNop with none");

    assert_eq!(
        settled[&(true, 10)],
        [
            "opAssignEqual",
            "opDec",
            "opDeclare",
            "opDivideEqual",
            "opInc",
            "opInserter",
            "opLocalize",
            "opMinusEqual",
            "opPlusEqual",
            "opTimesEqual",
        ],
        "the opcodes that yield a place"
    );
    assert_eq!(
        settled[&(false, 12)],
        [
            "opAnd",
            "opApostrophe_s",
            "opDivide",
            "opEqual",
            "opGreaterThan",
            "opGreaterThanOrEqualTo",
            "opLessThan",
            "opLessThanOrEqualTo",
            "opMinus",
            "opMod",
            "opNegate",
            "opNotEqual",
            "opOr",
            "opStar",
        ],
        "the opcodes that yield a temporary"
    );
}

/// Every float literal of a function body, in post-order.
fn float_order<'a>(stmt: &regent_core::gpl::ast::Stmt<'a>, out: &mut Vec<&'a str>) {
    use regent_core::gpl::ast::StmtKind;

    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Return(Some(e)) => float_order_expr(e, out),
        StmtKind::Block(v) => {
            for s in v {
                float_order(s, out);
            }
        }
        StmtKind::If { cond, then, els } => {
            float_order_expr(cond, out);
            float_order(then, out);

            if let Some(els) = els {
                float_order(els, out);
            }
        }
        StmtKind::While { cond, body } => {
            float_order_expr(cond, out);
            float_order(body, out);
        }
        StmtKind::DoWhile { body, cond } => {
            float_order(body, out);
            float_order_expr(cond, out);
        }
        StmtKind::Foreach { var, iter, body } => {
            float_order_expr(var, out);
            float_order_expr(iter, out);
            float_order(body, out);
        }
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue => {}
    }
}

fn float_order_expr<'a>(expr: &regent_core::gpl::ast::Expr<'a>, out: &mut Vec<&'a str>) {
    use regent_core::gpl::ast::ExprKind;

    match &expr.kind {
        ExprKind::Float(text) => out.push(text),
        ExprKind::Call { callee, args } => {
            float_order_expr(callee, out);

            for a in args {
                float_order_expr(a, out);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Postfix { operand, .. } => {
            float_order_expr(operand, out);
        }
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Assign { lhs, rhs, .. } => {
            float_order_expr(lhs, out);
            float_order_expr(rhs, out);
        }
        ExprKind::Attr { obj, attr } => {
            float_order_expr(obj, out);
            float_order_expr(attr, out);
        }
        _ => {}
    }
}

/// A float literal sits in a value slot's second word (`extra`), fixed point
/// with ten fractional bits: `100.` is 102400.
///
/// Every function's float literals, post-order, against its float-typed
/// immediates in file order. Corpus-wide, `extra` is set only on floats.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_float_literal_is_fixed_point_with_a_scale_of_1024() {
    use regent_core::gpl::ast::Item;

    /// 1.0 in GPL fixed point.
    const ONE: f64 = 1024.0;

    // Opcodes whose slot holds a declared type, not an operand.
    let declaring: Vec<u8> = ["opLocalize", "opDeclare", "opReference"]
        .iter()
        .map(|want| {
            let at = regent_core::gpl::opcode::TABLE
                .iter()
                .position(|e| e.name == *want)
                .expect("an opcode");

            u8::try_from(at).expect("under 39")
        })
        .collect();

    let quests = common::sdk().join("OriginalQuests");
    let sources = project_sources(&quests.join("GPL/path.gplproj"));
    let trees: Vec<_> = sources
        .iter()
        .map(|(_, text)| regent_core::gpl::parse(text).script)
        .collect();

    let data = std::fs::read(quests.join("Data/bytecode.bcd")).expect("the SDK image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("the SDK image walks");
    let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

    let mut literals = 0;

    for script in &trees {
        for item in &script.items {
            let Item::Function(f) = item else { continue };
            let Some(definition) = compiled.get(f.name) else {
                continue;
            };

            let mut wanted = Vec::new();

            for s in &f.body {
                float_order(s, &mut wanted);
            }

            // Only an immediate on a non-declaring record is a literal;
            // declarations and references are float-typed too.
            let taken: Vec<u32> = definition
                .records
                .iter()
                .filter(|r| !declaring.contains(&r.opcode))
                .flat_map(|r| &r.values)
                .filter(|v| {
                    v.flag != 2 && regent_core::data::gpltype::name(v.kind) == Some("float")
                })
                .map(|v| v.extra)
                .collect();

            let scaled: Vec<u32> = wanted
                .iter()
                .map(|text| {
                    let value: f64 = text.parse().unwrap_or_else(|e| panic!("{text}: {e}"));

                    // Truncated, not rounded: 0.2 stores as 204, not 205.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let fixed = (value * ONE) as u32;

                    fixed
                })
                .collect();

            assert_eq!(taken, scaled, "{}'s float literals", f.name);
            literals += scaled.len();
        }
    }

    assert_eq!(literals, 13, "float literals paired");

    // Every `.bcd`: `extra` only on floats.
    let (mut floats, mut others) = (0, 0);

    for path in every_bcd() {
        let symbols = symbols_of(&path);
        let walked = walk(&symbols).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for value in walked
            .iter()
            .flat_map(|d| &d.records)
            .flat_map(|r| &r.values)
            .filter(|v| v.extra != 0)
        {
            if regent_core::data::gpltype::name(value.kind) == Some("float") {
                floats += 1;
            } else {
                others += 1;
            }
        }
    }

    assert_eq!(others, 0, "a second word on a slot that is not a float");
    assert_eq!(floats, 344, "float literals in the corpus");
}

/// [`regent_core::gpl::emit::prologue`] reproduces the SDK image's prologue
/// records, every field, on all base game functions.
///
/// Given what it cannot derive: the total record count (from the body) and the
/// source file's spelling.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_prologue_emitter_reproduces_the_compilers_records() {
    use regent_core::gpl::ast::Item;
    use regent_core::gpl::emit;

    let quests = common::sdk().join("OriginalQuests");
    let sources = project_sources(&quests.join("GPL/path.gplproj"));

    let data = std::fs::read(quests.join("Data/bytecode.bcd")).expect("the SDK image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("the SDK image walks");
    let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

    let (mut checked, mut records) = (0, 0);

    for (_, text) in &sources {
        let script = regent_core::gpl::parse(text).script;

        for item in &script.items {
            let Item::Function(f) = item else { continue };
            let Some(definition) = compiled.get(f.name) else {
                continue;
            };

            // Spelling from the first record: project layout, not bytecode.
            let file = definition.records[0].text;
            let total = u32::try_from(definition.records.len()).expect("a sane count");
            let ours = emit::prologue(f, text, file, total);

            assert_eq!(
                ours.as_slice(),
                &definition.records[..ours.len()],
                "{}'s prologue",
                f.name
            );

            checked += 1;
            records += ours.len();
        }
    }

    assert_eq!(checked, 825, "functions whose prologue was emitted");
    assert_eq!(records, 7_879, "records emitted");
}

/// [`regent_core::gpl::emit::function`] reproduces every base game function, record
/// for record, given only the source file's spelling.
///
/// Rules the format does not state:
///
/// * a `#Name` constant keeps its sigil and is typed `reference`;
/// * `flag` 2: `word` holds the value — an integer or boolean literal, or a
///   slot number typed `undefined`. Flag-0 integers are declarations;
/// * brackets are kept: a parenthesised leaf gets its own slot;
/// * an operator's record takes its right operand's line: `a &&\n b` puts
///   `opAnd` on `b`'s;
/// * where a bare slot number is needed (call argument, return value,
///   condition), a leaf goes through `opNop` first. An operator's operand
///   names a variable directly.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_emitter_reproduces_every_base_game_function() {
    use regent_core::gpl::ast::Item;
    use regent_core::gpl::emit;

    let quests = common::sdk().join("OriginalQuests");
    let sources = project_sources(&quests.join("GPL/path.gplproj"));

    let data = std::fs::read(quests.join("Data/bytecode.bcd")).expect("the SDK image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("the SDK image walks");
    let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

    let (mut tried, mut records) = (0, 0);
    let mut differed = Vec::new();

    for (_, text) in &sources {
        let script = regent_core::gpl::parse(text).script;

        for item in &script.items {
            let Item::Function(f) = item else { continue };
            let Some(definition) = compiled.get(f.name) else {
                continue;
            };

            let file = definition.records[0].text;
            let Some(ours) = emit::function(f, text, file) else {
                continue;
            };

            tried += 1;
            records += ours.len();

            if ours != definition.records {
                differed.push(f.name);
            }
        }
    }

    assert_eq!(tried, 825, "functions emitted");
    assert_eq!(records, 41_033, "records emitted");
    assert!(differed.is_empty(), "functions that differ: {differed:?}");
}

/// The emitter's rules, fitted on the base game, hold on the other six SDK
/// projects with sources and image: the expansion's five and `WrathOfKrolm`.
/// Only exception: `Snake_Pit_Spawn` (see
/// `a_records_mark_is_decided_by_its_opcode`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_emitter_holds_on_projects_it_was_not_fitted_to() {
    use regent_core::gpl::ast::Item;
    use regent_core::gpl::emit;

    let sdk = common::sdk();
    let cases: [(&str, &str); 6] = [
        (
            "Example/GPL/WrathOfKrolm_Path.gplproj",
            "Example/Data/WrathOfKrolm.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Task.gplproj",
            "OriginalQuests/DataMX/MX_Task.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Data.gplproj",
            "OriginalQuests/DataMX/MX_Data.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Build.gplproj",
            "OriginalQuests/DataMX/MX_Build.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Decision.gplproj",
            "OriginalQuests/DataMX/MX_Decision.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_MajMisc.gplproj",
            "OriginalQuests/DataMX/MX_Compatibility.bcd",
        ),
    ];

    let (mut tried, mut skipped) = (0, 0);
    let mut differed: Vec<String> = Vec::new();

    for (project, image) in cases {
        let sources = project_sources(&sdk.join(project));
        let data = std::fs::read(sdk.join(image)).unwrap_or_else(|e| panic!("{image}: {e}"));
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = walk(sections.symbols.body).unwrap_or_else(|e| panic!("{image}: {e}"));
        let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

        for (_, text) in &sources {
            let script = regent_core::gpl::parse(text).script;

            for item in &script.items {
                let Item::Function(f) = item else { continue };
                let Some(definition) = compiled.get(f.name) else {
                    continue;
                };

                let file = definition.records[0].text;
                let Some(ours) = emit::function(f, text, file) else {
                    skipped += 1;
                    continue;
                };

                tried += 1;

                if ours != definition.records {
                    differed.push(format!("{image}: {}", f.name));
                }
            }
        }
    }

    assert_eq!(tried, 1_158, "functions emitted");
    assert_eq!(skipped, 0, "functions using an unspecified construct");

    // The emitter writes the rule's 12 where `Snake_Pit_Spawn` has 1.
    assert_eq!(
        differed,
        ["OriginalQuests/DataMX/MX_Data.bcd: Snake_Pit_Spawn"],
        "functions that differ"
    );
}

/// `break` and `continue`, from mods; no shipped source has either.
/// `CaravanGuards` has the only `continue`; the only uncommented `break`s are
/// two in Majestic Majesty.
///
/// Both compile alike, not to a jump: `head` is the enclosing `opForeach`'s
/// record, and there is no source line. Fits `docs/gpl/compiler.md` on `foreach`
/// depth tracking.
///
/// One more `opBreak` comes from a `return` inside a `foreach`, which also
/// moves `opReturnValue` past the loop's `opRewind`
/// (`Phantom_Priestess_Assigned_To`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn break_and_continue_name_their_loop() {
    use regent_core::gpl::ast::Item;
    use regent_core::gpl::emit;

    let workshop = common::workshop();
    let cases: [(&str, &str, &str); 2] = [
        ("3554989139", "TempProject.gplproj", "CaravanGuards.bcd"),
        ("3769947406", "GPL/Phantom.gplproj", "GPL/Phantom.bcd"),
    ];

    let (mut tried, mut declined) = (0, 0);

    for (item, project, image) in cases {
        let root = workshop.join(item);
        let sources = project_sources(&root.join(project));
        let data = std::fs::read(root.join(image)).unwrap_or_else(|e| panic!("{image}: {e}"));
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = walk(sections.symbols.body).unwrap_or_else(|e| panic!("{image}: {e}"));
        let compiled: BTreeMap<&str, &_> = walked.iter().map(|d| (d.name, d)).collect();

        for (_, text) in &sources {
            let script = regent_core::gpl::parse(text).script;

            for item in &script.items {
                let Item::Function(f) = item else { continue };
                let Some(definition) = compiled.get(f.name) else {
                    continue;
                };

                let file = definition.records[0].text;
                let Some(ours) = emit::function(f, text, file) else {
                    declined += 1;
                    continue;
                };

                tried += 1;

                assert_eq!(ours, definition.records, "{image}: {}", f.name);
            }
        }
    }

    assert_eq!(tried, 145, "mod functions emitted");
    assert_eq!(declined, 0, "functions using an unspecified construct");
}

/// Majestic Majesty's `defend_building_packet`: a `return` inside a `foreach`
/// whose value is a call, record for record as its image has it. With
/// `Phantom_Priestess_Assigned_To`, the two shipped specimens.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_return_inside_a_foreach_compiles_as_shipped() {
    use regent_core::gpl::ast::Item;

    const NAME: &str = "defend_building_packet";

    let root = drifted_mod().join("GPL");
    let data = std::fs::read(root.join("bytecode.bcd")).expect("Majestic Majesty's image");
    let program = bcd::Container::parse(&data).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("section 1");
    let shipped = walked.iter().find(|d| d.name == NAME).expect("shipped");

    let raw = std::fs::read(root.join("DecisionTrees/Modules/Defend_Home_MM.gpl")).expect("source");
    let text = String::from_utf8_lossy(&raw);
    let script = regent_core::gpl::parse(&text).script;
    let f = script
        .items
        .iter()
        .find_map(|item| match item {
            Item::Function(f) if f.name == NAME => Some(f),
            _ => None,
        })
        .expect("defined");

    let ours = emit::function(f, &text, shipped.records[0].text).expect("emitted");
    let breaks = ours.iter().filter(|r| r.opcode == Op::Break.code()).count();

    assert_eq!(ours, shipped.records);
    assert_eq!(breaks, 1);
}

/// `opBreak` and `opContinue` records over the 75 files, all in Workshop
/// images: `guard_check`'s `continue`; two source `break`s in Majestic
/// Majesty; three `opBreak`s from a `return` inside a `foreach`, in its
/// `defend_building_packet` and in `Phantom_Priestess_Assigned_To`, whose
/// image ships twice.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn only_workshop_images_hold_break_and_continue() {
    use regent_core::gpl::opcode::Op;

    let workshop = common::workshop();
    let (mut breaks, mut continues) = (0, 0);

    for path in every_bcd() {
        let raw = std::fs::read(&path).expect("a .bcd");
        let program = bcd::Container::parse(&raw).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = walk(sections.symbols.body).expect("section 1");

        let codes = walked.iter().flat_map(|d| &d.records).map(|r| r.opcode);
        let (b, c) = codes.fold((0, 0), |(b, c), code| {
            (
                b + usize::from(code == Op::Break.code()),
                c + usize::from(code == Op::Continue.code()),
            )
        });

        assert!(
            b + c == 0 || path.starts_with(&workshop),
            "{}",
            path.display()
        );
        breaks += b;
        continues += c;
    }

    assert_eq!((breaks, continues), (5, 1));
}

/// The source paths a project lists, in order, as written: `lowlevel.gpl` for
/// `LowLevel.gpl`, `rules\construction_rules.gpl` with its backslash.
fn listed_sources(project: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(project).expect("a readable project");

    text.lines()
        .map(str::trim)
        .filter_map(|line| {
            let (key, rest) = line.split_once('=')?;

            key.trim()
                .eq_ignore_ascii_case("source")
                .then(|| rest.trim().trim_matches('"').to_owned())
        })
        .collect()
}

/// The distinct source paths an image records. Build layout, not bytecode:
/// `C:\UGServer\Cyberlore\...` in the SDK example, `\projects\ap\gpl\...` in
/// the shipped game, relative in `OriginalQuests`.
fn recorded_paths(walked: &[regent_core::gpl::bcd::Compiled<'_>]) -> BTreeSet<String> {
    walked
        .iter()
        .filter_map(|d| d.records.first())
        .map(|r| r.text.to_owned())
        .collect()
}

/// The image's spelling of one listed path.
///
/// Matches the whole listed path at a directory boundary: `Defend_Home.gpl`
/// exists twice in the SDK, and `lowlevel.gpl` must not match `xlowlevel.gpl`.
fn spelling_of<'a>(recorded: &'a BTreeSet<String>, listed: &str) -> &'a str {
    let want = listed.to_ascii_lowercase();

    let mut found = recorded.iter().filter(|path| {
        path.to_ascii_lowercase()
            .strip_suffix(&want)
            .is_some_and(|before| before.is_empty() || before.ends_with(['\\', '/']))
    });

    let one = found
        .next()
        .unwrap_or_else(|| panic!("nothing records {listed}"));

    assert_eq!(found.next(), None, "{listed} is recorded twice");

    one
}

/// A whole section 1 from source, record for record, over seven projects:
/// adds entry count, order, constants and prototypes.
///
/// A constant is three `opNop` records — file, `#Name`, value — on the
/// `expression` keyword's line. A prototype is two of those plus one
/// `opDeclare` per member. Neither has a record count or body boundary.
///
/// A prototype's two header records carry the next definition's line (the
/// file's last line, for the last one), so it depends on the file around it.
/// Hence `emit::script`, not a per-item function.
///
/// Sorted by name, case-insensitively, so `#` constants come first.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_whole_code_section_is_built_from_the_sources() {
    use regent_core::gpl::ast::Item;
    use regent_core::gpl::emit::{self, Unit};

    let sdk = common::sdk();
    let (mut entries, mut constants, mut prototypes) = (0, 0, 0);
    let mut differed: Vec<String> = Vec::new();
    let mut identical: Vec<&str> = Vec::new();

    for (project, image) in sdk_projects() {
        let data = std::fs::read(sdk.join(image)).unwrap_or_else(|e| panic!("{image}: {e}"));
        let program = bcd::Container::parse(&data).expect("a program");
        let sections = program.sections().expect("three sections");
        let theirs = walk(sections.symbols.body).unwrap_or_else(|e| panic!("{image}: {e}"));
        let recorded = recorded_paths(&theirs);

        let sources = project_sources(&sdk.join(project));
        let listed = listed_sources(&sdk.join(project));
        assert_eq!(sources.len(), listed.len(), "{project}");

        let files: Vec<&str> = listed
            .iter()
            .map(|path| spelling_of(&recorded, path))
            .collect();

        let texts: Vec<&str> = sources.iter().map(|(_, text)| text.as_str()).collect();
        let scripts: Vec<_> = texts
            .iter()
            .map(|text| regent_core::gpl::parse(text).script)
            .collect();

        let units: Vec<Unit<'_>> = scripts
            .iter()
            .zip(&texts)
            .zip(&files)
            .map(|((script, src), file)| Unit { script, src, file })
            .collect();

        for item in scripts.iter().flat_map(|s| &s.items) {
            match item {
                Item::Constant(_) => constants += 1,
                Item::Prototype(_) => prototypes += 1,
                Item::Function(_) => {}
            }
        }

        let ours = emit::code_section(&units).expect("every definition emits");

        // Names and order first.
        assert_eq!(
            ours.iter().map(|d| d.name).collect::<Vec<_>>(),
            theirs.iter().map(|d| d.name).collect::<Vec<_>>(),
            "{image}: a different set of entries, or a different order"
        );

        for (ours, theirs) in ours.iter().zip(&theirs) {
            entries += 1;

            if ours.records != theirs.records {
                differed.push(format!("{image}: {}", ours.name));
            }
        }

        // And the bytes.
        if encode_code(&ours) == sections.symbols.body {
            identical.push(image);
        }
    }

    assert_eq!(entries, 3_609, "entries compared");
    assert_eq!(constants, 1_582, "constants among them");
    assert_eq!(prototypes, 44, "prototypes among them");

    // See `a_records_mark_is_decided_by_its_opcode`.
    assert_eq!(
        differed,
        ["OriginalQuests/DataMX/MX_Data.bcd: Snake_Pit_Spawn"],
        "entries that differ"
    );

    assert_eq!(identical.len(), 6, "sections that re-encode exactly");
    assert!(
        !identical.contains(&"OriginalQuests/DataMX/MX_Data.bcd"),
        "MX_Data matched, so the Snake_Pit_Spawn exception is gone"
    );
}

/// The seven SDK projects whose compiled image is known.
const fn sdk_projects() -> [(&'static str, &'static str); 7] {
    [
        (
            "OriginalQuests/GPL/path.gplproj",
            "OriginalQuests/Data/bytecode.bcd",
        ),
        (
            "Example/GPL/WrathOfKrolm_Path.gplproj",
            "Example/Data/WrathOfKrolm.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Task.gplproj",
            "OriginalQuests/DataMX/MX_Task.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Data.gplproj",
            "OriginalQuests/DataMX/MX_Data.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Build.gplproj",
            "OriginalQuests/DataMX/MX_Build.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_Decision.gplproj",
            "OriginalQuests/DataMX/MX_Decision.bcd",
        ),
        (
            "OriginalQuests/GPLMx/Path_MajMisc.gplproj",
            "OriginalQuests/DataMX/MX_Compatibility.bcd",
        ),
    ]
}

/// Section 2 bodies decode as a scope: a three-word header and two counted
/// block lists. Header plus counts fill the first 20 bytes:
///
/// ```text
/// [u32 1] [u32 returns] [u32 kind]
/// [u32 n]  ( [u32 20][u32 1][u32 type][u32 0][i32 -1] )*   the declares
/// [u32 m]  ( same )*                                       the parameters
/// ```
///
/// On every `.bcd`: every body consumed exactly; block types only 1-8, the
/// source-level types; `kind` names the definition's table.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_definitions_scope_decodes_and_names_its_own_table() {
    use regent_core::data::gpltype::TypeCode;
    use regent_core::gpl::bcd::Kind;

    let mut bodies = 0;
    let mut payloads: BTreeSet<u8> = BTreeSet::new();
    let mut returns: BTreeSet<u8> = BTreeSet::new();

    for path in every_bcd() {
        let (_, body) = code_of(&path);
        let scopes =
            ScopeSection::decode(&body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        for (want, table) in [
            (Kind::Prototype, &scopes.prototypes),
            (Kind::Function, &scopes.functions),
            (Kind::Constant, &scopes.constants),
        ] {
            for d in table {
                let scope = d
                    .scope()
                    .unwrap_or_else(|e| panic!("{}: {}: {e}", path.display(), d.name));

                assert_eq!(scope.kind, want, "{}: {}", path.display(), d.name);

                // Guards against an off-by-one split.
                assert_eq!(
                    d.body.len(),
                    (scope.len() + 1) * DEFINITION_RECORD,
                    "{}: {}",
                    path.display(),
                    d.name
                );

                assert_eq!(scope.encode(), *d.body, "{}: {}", path.display(), d.name);

                payloads.extend(scope.declares.iter().chain(&scope.parameters));
                returns.insert(scope.returns);
                bodies += 1;
            }
        }
    }

    assert_eq!(bodies, 19_467, "bodies decoded");
    assert_eq!(
        payloads.into_iter().collect::<Vec<u8>>(),
        [1, 2, 3, 4, 5, 6, 7, 8],
        "a declared name has a type outside the source-level eight"
    );

    // 0 without an `is` clause; `unknowntype` for non-functions. None returns
    // 8, `function`.
    assert_eq!(
        returns.into_iter().collect::<Vec<u8>>(),
        [0, 1, 2, 3, 4, 5, 6, 7, TypeCode::Unknown.code()]
    );
}

/// A whole section 2 from source.
///
/// Each entry holds the types (no names) of every `declare`d name and every
/// parameter so far in the file, in order, reset per file.
///
/// Sorted by name in byte order: `Advanced_Attack` before `adept_birth`.
/// Section 1 ignores case.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_whole_scope_section_is_built_from_the_sources() {
    use regent_core::gpl::emit::{self, Unit};

    let sdk = common::sdk();
    let mut matched = Vec::new();

    for (project, image) in sdk_projects() {
        let sources = project_sources(&sdk.join(project));
        let texts: Vec<&str> = sources.iter().map(|(_, text)| text.as_str()).collect();
        let scripts: Vec<_> = texts
            .iter()
            .map(|text| regent_core::gpl::parse(text).script)
            .collect();

        // Section 2 records no paths.
        let units: Vec<Unit<'_>> = scripts
            .iter()
            .zip(&texts)
            .map(|(script, src)| Unit {
                script,
                src,
                file: "",
            })
            .collect();

        let (_, theirs) = code_of(&sdk.join(image));

        if emit::scope_section(&units).encode() == theirs {
            matched.push(image);
        }
    }

    assert_eq!(matched.len(), 7, "sections that re-encode exactly");
}

/// A whole `.bcd` compiled from its project by `emit::compile_project`.
///
/// Five of seven match byte for byte, including the base game's 3.8 MB
/// `bytecode.bcd`. The two that do not:
///
/// * `MX_Data.bcd`: one byte, `Snake_Pit_Spawn`'s mark;
/// * `WrathOfKrolm`: only the recorded source path — the absolute build path
///   against the project's `WrathOfKrolm.gpl`, 66 bytes per definition.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_whole_bcd_is_compiled_from_its_project() {
    /// The example's build path, and the one its project lists.
    const BUILT_WITH: &str =
        r"C:\UGServer\Cyberlore\Majesty\Programming\Runtime\SDK\Example\GPL\WrathOfKrolm.gpl";
    const LISTED: &str = "WrathOfKrolm.gpl";
    const EXAMPLE: &str = "Example/Data/WrathOfKrolm.bcd";
    const EXAMPLE_DEFINITIONS: usize = 20;

    /// Holds the one unexplained record.
    const SNAKE_PIT: &str = "OriginalQuests/DataMX/MX_Data.bcd";

    let sdk = common::sdk();
    let mut identical = Vec::new();

    for (project, image) in sdk_projects() {
        let ours = regent_core::gpl::emit::compile_project(&sdk.join(project))
            .unwrap_or_else(|e| panic!("{project}: {e}"));
        let theirs = std::fs::read(sdk.join(image)).unwrap_or_else(|e| panic!("{image}: {e}"));

        if ours == theirs {
            identical.push(image);
        } else if image == EXAMPLE {
            assert_eq!(
                theirs.len() - ours.len(),
                (BUILT_WITH.len() - LISTED.len()) * EXAMPLE_DEFINITIONS,
                "{image} differs by more than its build path"
            );
        } else {
            // Same length, one byte apart.
            assert_eq!(image, SNAKE_PIT, "a third image differs");
            assert_eq!(ours.len(), theirs.len(), "{image}: a different size");
            assert_eq!(
                ours.iter().zip(&theirs).filter(|(a, b)| a != b).count(),
                1,
                "{image} differs by more than the one record"
            );
        }
    }

    assert_eq!(
        identical.len(),
        5,
        "images reproduced byte for byte: {identical:?}"
    );
}

/// Monster Kingdom (Workshop `3743606613`): its source holds negative
/// constants, `expression #WYRM_HEAL_MOD_TIER1 -2`.
const NEGATIVE_MOD: &str = "3743606613";

/// A negative constant's value record is an `opNegate` of the literal, as
/// a body's `-2` is: `Gplbcc.exe` folds nothing. The engine evaluates the
/// records, so the value is still -2.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_negative_constant_negates_its_literal() {
    use regent_core::gpl::ast::{ExprKind, Item};

    let root = common::workshop().join(NEGATIVE_MOD);
    let image = std::fs::read(root.join("Data/MonsterKingdom.bcd")).expect("shipped");
    let program = bcd::Container::parse(&image).expect("a program");
    let sections = program.sections().expect("three sections");
    let walked = walk(sections.symbols.body).expect("walks");
    let theirs: BTreeMap<String, &Compiled<'_>> = walked
        .iter()
        .map(|d| (d.name.to_ascii_lowercase(), d))
        .collect();

    let src = std::fs::read_to_string(root.join("GPL/MK_Globals.gpl")).expect("source");
    let script = regent_core::gpl::parse(&src).script;
    let negative: Vec<_> = script
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Constant(c) if matches!(c.value.kind, ExprKind::Unary { .. }) => Some(c),
            _ => None,
        })
        .collect();

    assert_eq!(negative.len(), 3, "negative constants");

    for c in negative {
        let shipped = theirs[&format!("#{}", c.name).to_ascii_lowercase()];
        let ours = emit::constant(c, &src, shipped.records[0].text).expect("emits");

        assert_eq!(shipped.records[2].opcode, Op::Negate.code());
        assert_eq!(ours, shipped.records, "#{}", c.name);
    }
}
