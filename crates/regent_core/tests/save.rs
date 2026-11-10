//! Hero classes: their stats' keys, primary stats and gold costs.
//!
//! Corpus: `MAJESTY_HD`.

mod common;

use regent_core::data::stat;
use std::collections::{BTreeMap, BTreeSet};

use regent_core::content::unit::Unit;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::stat::Stat;

/// Every stat names a key, so `Stat::key` is infallible.
#[test]
fn every_stat_names_a_key() {
    for (stat, name, index, _) in stat::Stat::ALL {
        assert_eq!(stat.key(), (name, index));
    }
}

/// **Game.** Every hero class's primary stat matches the UI list in
/// `local/game-text/hero_primary_stats.txt`. `APV:47` mirrors the descriptor's `primary_stat`.
///
/// Values are alphabetical — Artifice 0, Intelligence 2, Strength 3, Vitality
/// 4, Will Power 5. No character uses 1.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_hero_classs_primary_stat_is_the_one_the_game_names() {
    let (_src, library) = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let catalogue = library.catalogue().unwrap();

    // By id, not name: `Enemy_Barbarian` is not the hero class.
    let sheet: BTreeMap<&str, &str> = [
        ("AVA1", "Strength"),     // Adept
        ("AVB1", "Vitality"),     // Barbarian
        ("AVC1", "Artifice"),     // Cultist
        ("AVD1", "Intelligence"), // Healer
        ("AVE1", "Will Power"),   // Monk
        ("AVF1", "Intelligence"), // Paladin
        ("AVG1", "Intelligence"), // Priestess
        ("AVH1", "Strength"),     // Ranger
        ("AVJ1", "Artifice"),     // Rogue
        ("AVK1", "Strength"),     // Solarus
        ("AVL1", "Strength"),     // Warrior
        ("AVM1", "Vitality"),     // Warrior of Discord
        ("AVN1", "Intelligence"), // Wizard
        ("AVc1", "Strength"),     // Dwarf
        ("AVd1", "Artifice"),     // Elf
        ("AVe1", "Will Power"),   // Gnome
    ]
    .into_iter()
    .collect();

    let named: BTreeMap<i32, &str> = stat::PRIMARY_STATS
        .iter()
        .map(|(value, _, name)| (*value, *name))
        .collect();

    let mut checked = 0;

    for (id, want) in &sheet {
        let four = regent_core::data::fourcc::FourCc::new(id.as_bytes()[..4].try_into().unwrap());
        let unit = Unit::find(&catalogue, four)
            .unwrap_or_else(|| panic!("no DUNT {id}"))
            .unwrap();

        let character = unit
            .desc
            .character()
            .unwrap_or_else(|| panic!("{id} is no character"));
        let value = character
            .primary_stat()
            .unwrap_or_else(|| panic!("{id} has no primary stat"));

        let got = named
            .get(&i32::try_from(value).unwrap())
            .unwrap_or_else(|| panic!("{id} reads {value}, which names nothing"));

        assert_eq!(
            got,
            want,
            "{id} {} reads {value}",
            String::from_utf8_lossy(unit.desc.head.name)
        );

        checked += 1;
    }

    assert_eq!(checked, 16, "the list is sixteen classes");

    let vitality: Vec<&str> = sheet
        .iter()
        .filter(|(_, stat)| **stat == "Vitality")
        .map(|(id, _)| *id)
        .collect();

    assert_eq!(
        vitality,
        ["AVB1", "AVM1"],
        "Barbarian and Warrior of Discord"
    );

    // No base-game Character uses 1.
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut characters = 0;

    for (at, _) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&catalogue, at) else {
            continue;
        };

        if unit.desc.kind() != UnitKind::Character {
            continue;
        }

        if let Some(value) = unit.desc.character().and_then(|c| c.primary_stat()) {
            characters += 1;
            used.insert(value);
        }
    }

    assert!(characters > 80, "only {characters} characters");
    assert_eq!(used, BTreeSet::from([0, 2, 3, 4, 5]), "1 found a user");
}

/// Costed characters outside the sixteen classes (Gnome Champion, Sage, Enemy
/// Barbarian) are not player classes.
///
/// The Enemy Barbarian has the player Barbarian's five attributes (20, 9, 8,
/// 5, 25) but primary 0, not 4. So 0 is the default; it means Artifice only on
/// a hero.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_costed_characters_outside_the_sixteen_are_not_classes() {
    let (_base_src, base) = load("Data/MajestyDatasetDefinitions.xml", "Data");
    let (_mx_src, expansion) = load("DataMX/MajestyExpansionDatasetDefinitions.xml", "DataMX");
    let (base, expansion) = (base.catalogue().unwrap(), expansion.catalogue().unwrap());

    // Both, for the expansion's Gnome Champion.
    let character = |id: &str| {
        let four = regent_core::data::fourcc::FourCc::new(id.as_bytes()[..4].try_into().unwrap());
        let unit = Unit::find(&base, four)
            .or_else(|| Unit::find(&expansion, four))
            .unwrap_or_else(|| panic!("no DUNT {id}"))
            .unwrap();
        let c = unit.desc.character().expect("a character");

        (
            c.primary_stat(),
            [
                c.vitality(),
                c.artifice(),
                c.will_power(),
                c.intelligence(),
                c.strength(),
            ],
        )
    };

    // Gnome Champion = Gnome: same attributes, primary 5.
    let (champ, champ_stats) = character("AVO1");
    let (gnome, gnome_stats) = character("AVe1");

    assert_eq!(champ, Some(5));
    assert_eq!(champ, gnome);
    assert_eq!(champ_stats, gnome_stats, "the champion is not a Gnome");

    // The Sage is only in `xqd1_maindata.cam` (bonus quests): a Wizard with
    // more hit points, primary 2, `NotHero`. Only its absence is asserted.
    let sage = regent_core::data::fourcc::FourCc::new(b"XVN1");

    assert!(
        Unit::find(&base, sage).is_none() && Unit::find(&expansion, sage).is_none(),
        "the Sage is in a shipped dataset after all"
    );

    // The Enemy Barbarian is only in `mdl1_unittype.cam`.
    let enemy_id = regent_core::data::fourcc::FourCc::new(b"BV02");

    assert!(
        Unit::find(&base, enemy_id).is_none() && Unit::find(&expansion, enemy_id).is_none(),
        "the Enemy Barbarian is in a shipped dataset after all"
    );

    let loose = regent_core::data::catalogue::Library::from_files(&[
        common::hd().join("Data/MDL1_unittype.cam")
    ])
    .expect("the module archive");
    let loose = loose.catalogue().unwrap();
    let enemy = Unit::find(&loose, enemy_id).expect("BV02").unwrap();
    let enemy = enemy.desc.character().expect("a character");

    let (ours, our_stats) = character("AVB1");

    assert_eq!(
        [
            enemy.vitality(),
            enemy.artifice(),
            enemy.will_power(),
            enemy.intelligence(),
            enemy.strength()
        ],
        our_stats,
        "not the same barbarian"
    );

    assert_eq!(ours, Some(4), "the player's Barbarian takes Vitality");
    assert_eq!(
        enemy.primary_stat(),
        Some(0),
        "a unit with a Barbarian's stats reads 0, so 0 is the default"
    );
}

/// A dataset and the archives it names, as `view.rs` loads it.
fn load(definition: &str, dir: &str) -> (Vec<u8>, regent_core::data::catalogue::Library) {
    let owned = std::fs::read(common::hd().join(definition)).expect("a definition");
    let game = common::hd();
    let base = game.join("Data");

    let set = regent_core::content::dataset::parse(std::str::from_utf8(&owned).expect("ASCII"));
    let library = set.library(&game.join(dir), &[("MajestyDataPath", base.as_path())]);

    (owned, library)
}

/// `PrimaryStat` 0 is Artifice only on a character that costs gold; otherwise
/// it means none.
///
/// In the base dataset the gold-costing `Character` descriptors are exactly the
/// sixteen classes, each naming an attribute.
///
/// - Not `NotHero`: the Gnome carries it and is a class.
/// - Not "recruitable": the expansion's `AVO1 Gnomechamp` costs gold but is
///   made by `$ChangeUnitType` in `mx_give_exp.gpl` from a level-8 Gnome; its
///   100 gold is the Gnome's, never charged. Primary Will Power, as the Gnome.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_character_that_costs_gold_is_a_hero_class() {
    let classes: BTreeSet<&str> = [
        "AVA1", "AVB1", "AVC1", "AVD1", "AVE1", "AVF1", "AVG1", "AVH1", "AVJ1", "AVK1", "AVL1",
        "AVM1", "AVN1", "AVc1", "AVd1", "AVe1",
    ]
    .into_iter()
    .collect();

    let paying = |definition: &str, dir: &str| -> BTreeSet<String> {
        let (_src, library) = load(definition, dir);
        let catalogue = library.catalogue().unwrap();
        let mut found = BTreeSet::new();

        for (at, _) in catalogue.tagged(b"DUNT") {
            let Ok(unit) = Unit::at(&catalogue, at) else {
                continue;
            };
            let Some(character) = unit.desc.character() else {
                continue;
            };

            if character.costs_gold() {
                let value = character.primary_stat().expect("a class has a primary");

                assert!(
                    stat::primary(i32::try_from(value).unwrap()).is_some(),
                    "{} reads {value}, which names no attribute",
                    unit.id
                );

                found.insert(unit.id.to_string());
            }
        }

        found
    };

    let base = paying("Data/MajestyDatasetDefinitions.xml", "Data");
    let names: BTreeSet<&str> = base.iter().map(String::as_str).collect();

    assert_eq!(names, classes, "the paying characters are not the classes");

    // The expansion holds only changed records: a subset plus `AVO1`.
    let expansion = paying("DataMX/MajestyExpansionDatasetDefinitions.xml", "DataMX");

    for id in &expansion {
        assert!(
            classes.contains(id.as_str()) || id == "AVO1",
            "{id} costs gold in the expansion and is no known class"
        );
    }

    assert!(expansion.contains("AVO1"), "the Gnomechamp stopped paying");

    let character = |id: &[u8; 4], definition: &str, dir: &str| {
        let (_src, library) = load(definition, dir);
        let catalogue = library.catalogue().unwrap();
        let unit = Unit::find(&catalogue, FourCc::new(id))
            .unwrap_or_else(|| panic!("no DUNT {}", FourCc::new(id)))
            .unwrap();

        let held = unit.desc.character().expect("a character");

        (held.costs_gold(), held.primary_stat(), held.flags())
    };

    let gnome = character(b"AVe1", "Data/MajestyDatasetDefinitions.xml", "Data");
    let champion = character(
        b"AVO1",
        "DataMX/MajestyExpansionDatasetDefinitions.xml",
        "DataMX",
    );

    assert!(gnome.0 && champion.0);
    assert_eq!(
        gnome.1, champion.1,
        "the Gnomechamp is not the Gnome's kind"
    );
    assert_eq!(
        gnome
            .1
            .and_then(|v| stat::primary(i32::try_from(v).unwrap())),
        Some(Stat::WillPower)
    );

    let flagged = 1 << regent_core::data::desc::CharacterFlag::NotHero as u32;

    assert_eq!(gnome.2.map(|f| f & flagged), Some(flagged), "NotHero moved");
}
