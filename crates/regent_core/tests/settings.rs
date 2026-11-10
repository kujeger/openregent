//! `DATA` records: `GOPT` settings, `BDEP`/`QDEP` dependency tables.
//!
//! Constants in [`regent_core::data::settings`] are checked against their archives.

mod common;

use std::collections::{BTreeMap, BTreeSet};

use regent_core::content::unit::Unit;
use regent_core::data::cam::Archive;
use regent_core::data::catalogue::Library;
use regent_core::data::deps::{self, Difficulty};
use regent_core::data::fourcc::FourCc;
use regent_core::data::settings::{self, Settings};

/// HD's settings archives, base and expansion.
fn shipped() -> Vec<(String, Vec<u8>)> {
    let game = common::hd();
    let mut out = Vec::new();

    for relative in ["Data/miscdata.cam", "DataMX/mx_miscdata.cam"] {
        let path = game.join(relative);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        out.push((relative.to_owned(), bytes));
    }

    out
}

/// One `DATA` record's payload, if the archive carries it.
fn data(bytes: &[u8], id: FourCc) -> Option<&[u8]> {
    let archive = Archive::parse(bytes).expect("a readable archive");

    archive
        .records()
        .find(|(chunk, record)| chunk.tag.as_bytes() == b"DATA" && record.id == id)
        .map(|(_, record)| record.data)
}

/// The `GOPT` payload inside an archive, if it has one.
fn gopt(bytes: &[u8]) -> Option<&[u8]> {
    data(bytes, FourCc::new(b"GOPT"))
}

/// A `DATA` record as text.
fn text_of(bytes: &[u8], id: FourCc) -> String {
    let payload = data(bytes, id).unwrap_or_else(|| panic!("no {id}"));

    String::from_utf8_lossy(payload).into_owned()
}

#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_shipped_settings_hold_the_lighting_constants() {
    let mut checked = 0;

    for (name, bytes) in shipped() {
        let payload = gopt(&bytes).unwrap_or_else(|| panic!("{name} has no GOPT"));
        let settings = Settings::parse(payload).unwrap_or_else(|e| panic!("{name}: {e}"));

        assert_eq!(
            settings.disagreements(),
            vec![],
            "{name} disagrees with regent_core::data::settings"
        );

        // Present, so no disagreements does not mean missing keys.
        for key in [
            "LightX", "LightY", "LightZ", "Contrast", "Ambient", "MaxSlope",
        ] {
            assert!(settings.number(key).is_some(), "{name} has no {key}");
        }

        checked += 1;
    }

    assert_eq!(checked, 2, "both Linux archives carry a GOPT");
}

/// One of few keys with two shipped values. **Binary**: set, the frame loop
/// reseeds the world RNG with frame + 1 each frame (`cases::ENGINE`). HD
/// loads only the expansion's `GOPT`, so HD runs with it off.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_expansion_turns_gpl_pseudorandom_off_and_the_base_game_leaves_it_on() {
    let files = shipped();
    let read = |which: usize| {
        let payload = gopt(&files[which].1).expect("a GOPT");

        Settings::parse(payload)
            .unwrap()
            .get("GplPseudorandom")
            .expect("the key is set in both")
            .to_owned()
    };

    assert_eq!(read(0), "1", "the base game");
    assert_eq!(read(1), "0", "the expansion");
}

/// `BuildTreePath` may reach the leftover `BuildTreeEditor_Controller`.
/// Only the expansion knows and sets it; HD logs
/// it and stores it as a string (`cases::GAME`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn only_the_expansion_configures_a_build_tree_and_a_freestyle_path() {
    let files = shipped();
    let of = |which: usize, key: &str| {
        Settings::parse(gopt(&files[which].1).expect("a GOPT"))
            .unwrap()
            .get(key)
            .map(str::to_owned)
    };

    assert_eq!(of(0, "BuildTreePath"), None, "the base game names none");
    assert_eq!(of(1, "BuildTreePath").as_deref(), Some("BuildTree"));
    assert_eq!(of(1, "FreeStyleIOPath").as_deref(), Some("FreeStyle"));
}

/// Each of the fourteen `LoadCAM` values is a file on disk. Exercises the
/// repeated-key accessor.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_archive_the_settings_load_is_on_disk() {
    let game = common::hd();
    let files = shipped();
    let settings = Settings::parse(gopt(&files[0].1).expect("a GOPT")).unwrap();
    let named: Vec<&str> = settings.every("LoadCAM").collect();

    assert_eq!(named.len(), 14, "the base game loads fourteen archives");

    for archive in named {
        // The disk's case is not the settings' case.
        let found = std::fs::read_dir(game.join("Data"))
            .expect("the data directory")
            .flatten()
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(archive)
            });

        assert!(found, "{archive} is named but absent");
    }
}

/// [`deps::Expr::parse`] balances the stack on every table: every operator
/// finds its operands, one value remains. The Phantoms Haunt mod's `BDEP` is a
/// non-Cyberlore sample of the notation.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_shipped_dependency_table_parses() {
    let counts: Vec<(String, usize, usize)> = tables()
        .iter()
        .map(|(name, bytes)| {
            let rules = deps::buildings(&text_of(bytes, FourCc::new(b"BDEP")))
                .unwrap_or_else(|e| panic!("{name} BDEP: {e}"));
            let conditional = rules.iter().filter(|r| r.requires.is_some()).count();

            (name.clone(), rules.len(), conditional)
        })
        .collect();

    // (rules, conditional). The second catches a lost colon the first misses.
    assert_eq!(
        counts,
        vec![
            ("Data/miscdata.cam".to_owned(), 49, 21),
            ("DataMX/mx_miscdata.cam".to_owned(), 55, 26),
            ("the Phantoms Haunt mod".to_owned(), 56, 27),
        ],
        "the building tables changed shape"
    );

    // `QDEP`, a different notation.
    for (name, bytes) in tables().iter().take(2) {
        let rules = deps::quests(&text_of(bytes, FourCc::new(b"QDEP")))
            .unwrap_or_else(|e| panic!("{name} QDEP: {e}"));

        assert!(rules.len() >= 20, "{name}: only {} quests", rules.len());
    }
}

/// Every id in a rule resolves to a `DUNT`: 160 rules, 60 distinct ids.
///
/// One exception: the mod's `PHG1` guild, declared in `phantom_units.xml` and
/// compiled by HD at load time. The mod ships no `DUNT` archive.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_building_dependency_names_a_building() {
    let archives = [
        common::hd().join("Data/unittype.cam"),
        common::hd().join("DataMX/mx_Unittype.cam"),
    ];

    let library = Library::from_files(&archives).expect("the unit archives");
    let catalogue = library.catalogue().expect("a catalogue");

    let (mut rules, mut named) = (0, BTreeSet::new());
    let mut unresolved = BTreeSet::new();

    for (_name, bytes) in tables() {
        for rule in deps::buildings(&text_of(&bytes, FourCc::new(b"BDEP"))).unwrap() {
            rules += 1;

            let mentioned =
                std::iter::once(rule.id).chain(rule.requires.iter().flat_map(deps::Expr::ids));

            for id in mentioned {
                named.insert(id);

                if Unit::find(&catalogue, id).is_none() {
                    unresolved.insert(id);
                }
            }
        }
    }

    assert_eq!(rules, 160, "the rules read");
    assert_eq!(named.len(), 60, "the distinct ids the three tables name");
    assert_eq!(
        unresolved,
        BTreeSet::from([FourCc::new(b"PHG1")]),
        "a rule names something that is no DUNT"
    );

    // The exception is a real building, not a misread word.
    let xml = common::workshop().join("3769947406/Data/phantom_units.xml");
    let source = std::fs::read_to_string(&xml).unwrap_or_else(|e| panic!("{}: {e}", xml.display()));

    assert!(source.contains("PHG1"), "the mod's own units name no PHG1");
}

/// A quest rule adds a difficulty field and writes `NONE` for no requirement.
/// Every quest an expression names is defined in the same table.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_quest_rule_carries_a_difficulty_and_names_only_quests() {
    for (name, bytes) in tables().iter().take(2) {
        let rules = deps::quests(&text_of(bytes, FourCc::new(b"QDEP"))).unwrap();
        let defined: BTreeSet<FourCc> = rules.iter().map(|r| r.id).collect();

        for rule in &rules {
            for id in rule.requires.iter().flat_map(deps::Expr::ids) {
                assert!(
                    defined.contains(&id),
                    "{name}: {} needs {id}, which the table does not define",
                    rule.id
                );
            }
        }

        // `NONE` is never read as an id.
        assert!(
            !defined.contains(&FourCc::new(b"NONE")),
            "{name}: NONE was read as a quest"
        );
    }
}

/// `MSTR` ([`Difficulty::Master`]) is expansion-only.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_expansion_adds_a_difficulty_the_base_game_never_uses() {
    let files = tables();
    let tiers = |which: usize| -> BTreeSet<Difficulty> {
        deps::quests(&text_of(&files[which].1, FourCc::new(b"QDEP")))
            .unwrap()
            .iter()
            .map(|r| r.difficulty)
            .collect()
    };

    assert_eq!(
        tiers(0),
        BTreeSet::from([Difficulty::Easy, Difficulty::Medium, Difficulty::Hard])
            .iter()
            .copied()
            .collect()
    );

    let expansion = deps::quests(&text_of(&files[1].1, FourCc::new(b"QDEP"))).unwrap();
    let master: Vec<FourCc> = expansion
        .iter()
        .filter(|r| r.difficulty == Difficulty::Master)
        .map(|r| r.id)
        .collect();

    assert_eq!(master, [FourCc::new(b"XQ08"), FourCc::new(b"XQ12")]);
}

/// `BDEP` warns: *"The LAST LINE MUST be left blank, otherwise statue prereqs
/// won't be parsed properly !"* The statue `ABk1` is the last rule. This parser
/// reads the table the same without the line.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dependency_tables_do_not_need_their_blank_last_line() {
    let files = tables();
    let source = text_of(&files[0].1, FourCc::new(b"BDEP"));

    assert!(
        source.ends_with('\n'),
        "the shipped table lost its blank last line"
    );

    let whole = deps::buildings(&source).unwrap();
    let trimmed = deps::buildings(source.trim_end()).unwrap();

    assert_eq!(whole, trimmed);
    assert_eq!(
        whole.last().map(|r| r.id),
        Some(FourCc::new(b"ABk1")),
        "the statue is the rule the warning is about"
    );
}

/// The archives with dependency tables: base, expansion, Phantoms Haunt mod.
fn tables() -> Vec<(String, Vec<u8>)> {
    let game = common::hd();
    let mut out = Vec::new();

    for relative in ["Data/miscdata.cam", "DataMX/mx_miscdata.cam"] {
        let path = game.join(relative);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        out.push((relative.to_owned(), bytes));
    }

    let mods = common::walk(&common::workshop(), &["cam"]);

    let phantom = mods
        .iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("phantom_miscdata.cam"))
        })
        .expect("the Phantoms Haunt mod's own miscdata");

    out.push((
        "the Phantoms Haunt mod".to_owned(),
        std::fs::read(phantom).expect("readable"),
    ));

    out
}

/// [`regent_core::data::settings::ISO_CAMERA_ANGLE`] (55) comes from
/// `MajestyHD.exe`'s static initialiser. It holds only if no `GOPT` overrides
/// `IsoCameraAngle` or `HeightThreshold`.
///
/// Control: `MaxSlope`, the adjacent key, is set everywhere.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_shipped_settings_file_sets_the_camera_pitch() {
    for (name, bytes) in shipped() {
        let settings = Settings::parse(gopt(&bytes).expect("a GOPT")).expect("it parses");

        for key in ["IsoCameraAngle", "HeightThreshold"] {
            assert_eq!(settings.get(key), None, "{name} sets {key} after all");
        }

        assert_eq!(settings.get("MaxSlope"), Some("45"), "{name}: the control");
    }
}

/// `SetScale` arithmetic: `cos(55°)` gives 0.5736 cells per height unit
/// (0.5 was eyeballed from `heighty.GMP`).
#[test]
fn the_camera_pitch_gives_the_ground_its_relief() {
    let radians = f64::from(settings::ISO_CAMERA_ANGLE).to_radians();
    let tile = f64::from(settings::TILE_PIXELS);

    // Default scale 1.
    let depth = radians.sin() * tile;
    let height = radians.cos() * tile;

    assert!(
        (depth - 26.213).abs() < 0.001,
        "sin(55) * 32 changed: {depth}"
    );
    assert!(
        (height - 18.354).abs() < 0.001,
        "cos(55) * 32 changed: {height}"
    );

    // In cells, as the viewer's control uses.
    let cells = height / tile;

    assert!(
        (cells - 0.5736).abs() < 0.0001,
        "cells per height unit: {cells}"
    );
}

/// [`settings::SQUASH`] and [`settings::RELIEF`] are literals (`sin` and
/// `cos` are not `const fn`): recomputed from the camera angle.
#[test]
fn the_projection_constants_are_the_engines_camera_angle() {
    let radians = f64::from(settings::ISO_CAMERA_ANGLE).to_radians();

    assert!(
        (f64::from(settings::SQUASH) - radians.sin()).abs() < 1e-6,
        "SQUASH is not sin(pitch): {}",
        settings::SQUASH
    );
    assert!(
        (f64::from(settings::RELIEF) - radians.cos()).abs() < 1e-6,
        "RELIEF is not cos(pitch): {}",
        settings::RELIEF
    );
}

/// [`settings::ISO_CAMERA_SIN`] is the bits `sin` gave on aarch64 Linux,
/// as `f64` and narrowed to `f32`: the values play used to compute.
#[test]
fn the_camera_sine_is_the_one_play_computed() {
    let radians = f64::from(settings::ISO_CAMERA_ANGLE).to_radians();

    #[allow(clippy::cast_possible_truncation, reason = "the narrowing checked")]
    let narrow = settings::ISO_CAMERA_SIN as f32;

    #[allow(clippy::cast_precision_loss, reason = "an angle in degrees")]
    let sin32 = (settings::ISO_CAMERA_ANGLE as f32).to_radians().sin();

    assert_eq!(settings::ISO_CAMERA_SIN.to_bits(), radians.sin().to_bits());
    assert_eq!(settings::ISO_CAMERA_SIN.to_bits(), 0x3fea_367e_5915_8747);
    assert_eq!(narrow.to_bits(), sin32.to_bits());
}

/// Every edition ships `MaxSlope` 45, whose exact tangent is 1.0. The engine's
/// stored tangent is not: its trig table truncates 45° to `tan(43.59°)` =
/// 0.9520569 (see `settings::MAX_SLOPE`, `regent_core::sim::mapgen::rim::tan_of`).
#[test]
fn the_slope_limit_is_a_tangent_and_it_is_exactly_one() {
    let limit = f64::from(settings::MAX_SLOPE).to_radians().tan();

    assert!(
        (limit - 1.0).abs() < 1e-12,
        "tan(MaxSlope) changed: {limit}"
    );
}

/// Quest names are `STRT QUES` in `gpltext.cam`, keyed by `QDEP` ids.
///
/// Three sources agree on `QU10 : QU02 QU12 && QU15 &&`: the `QDEP` header
/// comment, the string table, and game knowledge (Elven Treachery needs
/// Bell/Book/Candle, the Forsaken Land, Rescue the Prince).
///
/// Version 2 `STRT` has a `u32` key before each string; that key is the join.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_quest_id_is_named_by_the_ques_string_table() {
    let game = common::hd();

    let titles = |relative: &str| -> Vec<(FourCc, String)> {
        let path = game.join(relative);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let archive = Archive::parse(&bytes).expect("a readable archive");

        let (_, record) = archive
            .records()
            .find(|(chunk, record)| {
                chunk.tag.as_bytes() == b"STRT" && record.id == FourCc::new(b"QUES")
            })
            .unwrap_or_else(|| panic!("{relative} has no STRT QUES"));

        let table = regent_core::data::payload::StringTable::parse(record.data).expect("a table");

        (0..table.len())
            .map(|i| {
                (
                    table
                        .key(i)
                        .unwrap_or_else(|| panic!("string {i} is unkeyed")),
                    table.get(i).unwrap_or_default().into_owned(),
                )
            })
            .collect()
    };

    for (misc, strings) in [
        ("Data/miscdata.cam", "Data/gpltext.cam"),
        ("DataMX/mx_miscdata.cam", "DataMX/mx_gpltext.cam"),
    ] {
        let bytes = std::fs::read(game.join(misc)).expect("readable");
        let rules = deps::quests(&text_of(&bytes, FourCc::new(b"QDEP"))).unwrap();
        let names: BTreeMap<FourCc, String> = titles(strings).into_iter().collect();

        let unnamed: Vec<String> = rules
            .iter()
            .filter(|rule| !names.contains_key(&rule.id))
            .map(|rule| rule.id.to_string())
            .collect();

        // HD's expansion has a rule for `MD01` and no title for it.
        let want: &[&str] = if misc.contains("mx_") { &["MD01"] } else { &[] };

        assert_eq!(unnamed, want, "{misc}: rules with no name");
    }

    // Base: same twenty ids in both. Expansion `QUES` has three bonus quests
    // `QDEP` lacks.
    let base: BTreeSet<FourCc> = titles("Data/gpltext.cam")
        .into_iter()
        .map(|(k, _)| k)
        .collect();
    let misc = std::fs::read(game.join("Data/miscdata.cam")).expect("readable");
    let ruled: BTreeSet<FourCc> = deps::quests(&text_of(&misc, FourCc::new(b"QDEP")))
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();

    assert_eq!(base, ruled, "the base game's names and rules are one set");
    assert_eq!(base.len(), 20);

    // Checked against game knowledge.
    let names: BTreeMap<FourCc, String> = titles("Data/gpltext.cam").into_iter().collect();
    let elven = deps::quests(&text_of(&misc, FourCc::new(b"QDEP")))
        .unwrap()
        .into_iter()
        .find(|r| names[&r.id] == "Elven Treachery")
        .expect("Elven Treachery");

    let wants: Vec<&str> = elven
        .requires
        .as_ref()
        .expect("it has prerequisites")
        .ids()
        .iter()
        .map(|id| names[id].as_str())
        .collect();

    assert_eq!(elven.difficulty, Difficulty::Medium);
    assert_eq!(
        wants,
        [
            "The Bell, the Book, and the Candle",
            "The Forsaken Land",
            "Rescue the Prince",
        ]
    );
}

/// Four activity codes read off screen at save time (**Game**).
///
/// Evidence for [`regent_core::data::stat::Stat::Activity`], which resolves
/// against `STRT AITX`'s 177 codes ([`regent_core::data::text::Text::activity`]).
/// "Going home" is 4 for both a wizard and a warrior.
const LABELLED_ACTIVITIES: [(i32, &str); 4] = [
    (4, "Going home"),
    (34, "Resting at home"),
    (36, "Berserking"),
    (45, "Purchasing armor at the Blacksmith"),
];

/// `Stat::Activity` indexes the 177 strings of `STRT AITX` in `gpltext.cam`.
/// The four labelled values were read off screen beside their saves.
///
/// `AITX` is keyed: skipping the four key bytes garbles each string.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_labelled_activities_are_the_ones_the_game_ships() {
    let library = Library::from_files(&[common::hd().join("Data/gpltext.cam")])
        .expect("the GPL text archive");
    let catalogue = library.catalogue().expect("a catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);

    for (code, said) in LABELLED_ACTIVITIES {
        assert_eq!(
            text.activity(code).as_deref().map(str::trim),
            Some(said),
            "activity {code}"
        );
    }

    let table = text.table(regent_core::data::text::ACTIVITY).expect("AITX");

    assert_eq!(table.len(), 177);
    assert!(text.activity(176).is_some(), "the last code");
    assert_eq!(text.activity(177), None, "one past the end");
    assert_eq!(text.activity(-1), None, "a negative code");
}

/// `STRT DPTX` is `BDEP` in English, keyed by the same `DUNT` id
/// (*"ABB1 requires a Dwarven settlement"* beside `ABB1 : ABb1`).
///
/// | `DPTX` says | Means |
/// |---|---|
/// | *"There are no prerequisites for this structure."* | Unconditional |
/// | *"none (not user built)"*, *"none (Can't be built)"* | Never offered |
/// | anything else | The condition |
///
/// Five unbuildable (Palace, Sewers, Brothel, Gambling Hall, General Housing)
/// carry no `BDEP` condition.
///
/// One disagreement: `ABH1 Marketplace1` needs a Palace by rule, "no
/// prerequisites" by prose. Every game has a Palace.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_prose_agrees_with_the_rules_and_names_what_is_never_built() {
    let library = Library::from_files(&[common::hd().join("Data/gpltext.cam")]).expect("gpltext");
    let catalogue = library.catalogue().expect("a catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);

    let bytes = std::fs::read(common::hd().join("Data/miscdata.cam")).expect("readable");
    let rules = deps::buildings(&text_of(&bytes, FourCc::new(b"BDEP"))).unwrap();

    let (mut described, mut plain) = (0, 0);
    let (mut unbuildable, mut disagree) = (BTreeSet::new(), BTreeSet::new());

    for rule in &rules {
        let Some(prose) = text.prerequisite(rule.id) else {
            continue;
        };

        described += 1;

        if prose.contains("not user built") || prose.contains("Can't be built") {
            unbuildable.insert(rule.id);

            assert!(
                rule.requires.is_none(),
                "{}: never offered to the player, yet the rules gate it",
                rule.id
            );

            continue;
        }

        let says_none = prose.contains("no prerequisites");
        plain += usize::from(says_none);

        if says_none != rule.requires.is_none() {
            disagree.insert(rule.id);
        }
    }

    assert!(described > 40, "only {described} rules have prose");
    assert!(plain > 20, "only {plain} rules are unconditional");

    let named = |set: &BTreeSet<FourCc>| -> BTreeSet<String> {
        set.iter().map(|id| id.trimmed().to_owned()).collect()
    };

    assert_eq!(
        named(&unbuildable),
        ["ABJ1", "ABN1", "ABa1", "ABd1", "ABe1"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<String>>(),
        "a different set of buildings is marked unbuildable"
    );

    assert_eq!(
        named(&disagree),
        ["ABH1"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<String>>(),
        "the prose and the rules disagree somewhere new"
    );
}

/// Quest names resolve through the same table the other text does.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_text_resolver_names_a_quest() {
    let library = Library::from_files(&[common::hd().join("Data/gpltext.cam")]).expect("gpltext");
    let catalogue = library.catalogue().expect("a catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);

    assert_eq!(
        text.quest(FourCc::new(b"QU10")).as_deref().map(str::trim),
        Some("Elven Treachery")
    );
}

/// The only markup in any `STRT` is `0x01` plus six hex digits: an RGB colour
/// until the next marker. No other control bytes but `\n` and `\r`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_only_markup_in_the_games_text_is_a_colour() {
    let game = common::hd();
    let library = Library::from_files(&[
        game.join("Data/gpltext.cam"),
        game.join("Data/textdata.cam"),
        game.join("DataMX/mx_gpltext.cam"),
        game.join("DataMX/mx_rgstext.cam"),
        game.join("DataMX/mx_textdata.cam"),
    ])
    .expect("the text archives");

    let catalogue = library.catalogue().expect("a catalogue");
    let (mut marks, mut strings) = (0usize, 0usize);
    let (mut other, mut keys) = (BTreeSet::new(), BTreeSet::new());

    for (_, record) in catalogue.tagged(b"STRT") {
        let Ok(table) = regent_core::data::payload::StringTable::parse(record.data) else {
            continue;
        };

        // `HKTX` holds hotkey codes (`0x1b` is Escape).
        let hotkeys = record.id == FourCc::new(b"HKTX");

        for i in 0..table.len() {
            let Some(body) = table.get(i) else { continue };
            strings += 1;

            for span in regent_core::data::text::spans(&body) {
                marks += usize::from(span.colour.is_some());

                let stray = span
                    .text
                    .chars()
                    .filter(|c| c.is_control() && *c != '\n' && *c != '\r');

                if hotkeys {
                    keys.extend(stray);
                } else {
                    other.extend(stray);
                }
            }
        }
    }

    assert!(strings > 3_000, "only {strings} strings");
    assert!(marks > 400, "only {marks} colour markers");

    // Also catches a malformed marker, left as a stray `0x01`.
    assert_eq!(other, BTreeSet::new(), "an unexplained control character");

    assert_eq!(keys, BTreeSet::new());
}

/// Every `%` in the three editions' `STRT` strings starts `%d`, `%ld` or
/// `%s`: no width, flag or other conversion (`data::text::printf`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_games_text_formats_only_d_ld_and_s() {
    let mut seen = BTreeSet::new();

    for root in [common::hd()] {
        let library = Library::from_files(&common::archives(&root)).expect("the archives");
        let catalogue = library.catalogue().expect("a catalogue");

        for (_, record) in catalogue.tagged(b"STRT") {
            let Ok(table) = regent_core::data::payload::StringTable::parse(record.data) else {
                continue;
            };

            for body in table.iter() {
                for (at, _) in body.match_indices('%') {
                    let after = &body[at + 1..];
                    let conversion = ["ld", "d", "s"].into_iter().find(|c| after.starts_with(c));

                    seen.insert(conversion.unwrap_or(after).to_owned());
                }
            }
        }
    }

    assert_eq!(seen, BTreeSet::from(["d", "ld", "s"].map(String::from)));
}

/// Every key in the `GOPT`s is in a table.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_key_a_shipped_file_sets_is_one_the_engine_knows() {
    let mut counted = Vec::new();

    for (name, bytes) in shipped() {
        let settings = Settings::parse(gopt(&bytes).expect("a GOPT")).expect("it parses");
        let set: BTreeSet<&str> = settings.all().map(|(key, _)| key).collect();

        assert_eq!(settings.unknown_keys(), Vec::<&str>::new(), "{name}");

        counted.push((name, set.len()));
    }

    assert_eq!(
        counted,
        vec![
            ("Data/miscdata.cam".to_owned(), 25),
            ("DataMX/mx_miscdata.cam".to_owned(), 28),
        ]
    );
}

/// No shipped file sets the difficulty bands.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn no_shipped_file_sets_the_difficulty_bands() {
    for (name, bytes) in shipped() {
        let settings = Settings::parse(gopt(&bytes).expect("a GOPT")).expect("it parses");
        let set: Vec<&str> = settings
            .all()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("DifficultyLimit"))
            .collect();

        assert_eq!(set, Vec::<&str>::new(), "{name}");
    }
}
