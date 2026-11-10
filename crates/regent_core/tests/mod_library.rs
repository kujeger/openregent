//! Mods found and composed (`content::mods`, `dataset::Install::*_with`)
//! on the Workshop's 11 items and the SDK's example.
//!
//! Corpora: `MAJESTY_HD`, `MAJESTY_WORKSHOP`.

mod common;

use std::path::{Path, PathBuf};

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::content::mods::{self, Base, Library};
use regent_core::data::deps::Difficulty;
use regent_core::data::fourcc::FourCc;
use regent_core::data::guid::Guid;

/// Adjust Guardhouse, Workshop item 617966615 and the SDK's example.
const GUARDHOUSE: &str = "{D666FCBE-9906-4582-A6A0-DC96F812441E}";

/// Its one image.
const GUARDHOUSE_IMAGE: &str = "AdjustGuardhouse.bcd";

fn workshop() -> Library {
    mods::scan(&[common::workshop()])
}

fn guid(text: &str) -> Guid {
    Guid::parse(text).expect("a GUID")
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 39 mods in 10 manifests, each GUID once; 30 quests in 4 items. One
/// mod lists no XML: Adjust Guardhouse.
#[test]
fn the_workshop_holds_39_mods_and_30_quests() {
    let library = workshop();
    let plain: Vec<String> = library
        .mods
        .iter()
        .filter(|m| !m.has_xml())
        .map(mods::Mod::title)
        .collect();

    assert_eq!(library.mods.len(), 39);
    assert!(library.skipped.is_empty(), "{:?}", library.skipped);
    assert_eq!(library.quests.len(), 30);
    assert_eq!(plain, ["Adjust Guardhouse"]);
}

/// Each mod names its base; Monster Kingdom's six and Decor Buildings
/// are the expansion's, the rest `Any`.
#[test]
fn each_workshop_mod_names_a_base() {
    let library = workshop();
    let count = |base: Base| library.mods.iter().filter(|m| m.base == base).count();

    assert_eq!(count(Base::Any), 32);
    assert_eq!(count(Base::Expansion), 7);
    assert_eq!(count(Base::Majesty), 0);
}

#[test]
fn adjust_guardhouse_is_one_image_for_any_dataset() {
    let library = workshop();
    let found = library.find(guid(GUARDHOUSE)).expect("Adjust Guardhouse");

    assert_eq!(found.title(), "Adjust Guardhouse");
    assert_eq!(found.name.as_deref(), Some("AdjustGuardHouse"));
    assert_eq!(found.base, Base::Any);
    assert_eq!(
        found.load.programs,
        [(None, "Data/AdjustGuardhouse.bcd".to_owned())]
    );
    assert_eq!(found.load.archives.len(), 0);
    assert!(!found.has_xml());
    assert_eq!(name(&found.package), "617966615");
}

/// The SDK's copy has the Workshop item's GUID: scanned first, it is
/// kept and the Workshop's skipped.
#[test]
fn a_guid_found_twice_keeps_the_first() {
    let sdk = common::sdk().join("Adjust Guardhouse Mod");
    let library = mods::scan(&[sdk.clone(), common::workshop()]);
    let found = library.find(guid(GUARDHOUSE)).expect("Adjust Guardhouse");

    assert_eq!(found.package, sdk);
    assert_eq!(library.mods.len(), 39);
    assert_eq!(library.skipped.len(), 1, "{:?}", library.skipped);
}

/// A mod's image goes last in the list, so first in load order: it owns
/// the names it shares with the base (`gpl::load`). The archives and
/// chart are the expansion's.
#[test]
fn a_mods_image_loads_first_over_the_expansion() {
    let library = workshop();
    let found = library.find(guid(GUARDHOUSE)).expect("Adjust Guardhouse");
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let stock = install.files(Datasets::Expansion).expect("stock");
    let modded = install
        .files_with(Datasets::Expansion, &[found])
        .expect("modded");

    assert_eq!(name(&modded.programs[0]), GUARDHOUSE_IMAGE);
    assert_eq!(modded.programs[1..], stock.programs[..]);
    assert_eq!(modded.archives, stock.archives);
    assert_eq!(modded.chart, stock.chart);
}

/// A mod for the expansion is passed over in a base game.
#[test]
fn a_mod_for_the_other_dataset_is_passed_over() {
    let library = workshop();
    let expansion_only: Vec<&mods::Mod> = library
        .mods
        .iter()
        .filter(|m| m.base == Base::Expansion)
        .collect();
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);

    assert_eq!(
        install.files_with(Datasets::Base, &expansion_only).ok(),
        install.files(Datasets::Base).ok()
    );
}

/// A Workshop quest lists `<GPL><Target>` with `\`: Big Map's image is
/// found and loaded first.
#[test]
fn a_workshop_quests_target_image_is_found() {
    let library = workshop();
    let big = library
        .quests
        .iter()
        .find(|q| q.quest.name == "BigMap")
        .expect("Big Map");
    let hd = common::hd();
    let files = Install::new(&hd, Bytecode::Install)
        .quest_with(&big.quest, &[])
        .expect("its files");

    assert_eq!(name(&files.programs[0]), "BigMap.bcd");
    assert!(!big.quest.has_xml());
}

/// Majesty Expanded's copy of Barren Waste, `{31305551-0054-0032-…}`,
/// does not keep the quest's key `QU01`: an id with more than its first
/// word set is keyed by its CRC, here
/// `0xea29f48e`, as `zlib.crc32` of its 16 bytes, inverted.
#[test]
fn a_quests_copy_is_keyed_by_its_ids_crc() {
    let library = workshop();
    let copy = library
        .quests
        .iter()
        .find(|q| q.quest.template.eq_ignore_ascii_case("barren_waste.q"))
        .expect("the copy");

    assert_eq!(copy.quest.key, FourCc(0xea29_f48e_u32.to_le_bytes()));
    assert_eq!(
        copy.quest.id,
        guid("{31305551-0054-0032-0000-000000000000}")
    );
    assert_eq!(name(&copy.package), "2827106524");
}

/// The Wild says `<Difficulty>Master</Difficulty>`.
#[test]
fn a_workshop_quest_names_its_difficulty() {
    let library = workshop();
    let of = |name: &str| {
        library
            .quests
            .iter()
            .find(|q| q.quest.name == name)
            .map(|q| q.quest.difficulty)
    };

    assert_eq!(of("TheWild"), Some(Difficulty::Master));
}

/// Seven quests list XML descriptions or strings: The Wild, and four
/// new and two copied quests of Majesty Expanded.
#[test]
fn some_workshop_quests_list_xml() {
    let library = workshop();
    let mut xml: Vec<String> = library
        .quests
        .iter()
        .filter(|q| q.quest.has_xml())
        .map(|q| q.quest.name.clone())
        .collect();

    xml.sort();
    assert_eq!(
        xml,
        [
            "EastCheswickSiege",
            "MD01",
            "OculusPrime",
            "SEWER_WAR",
            "TheRandomMapME",
            "TheWild",
            "VAMPIRIC_REVENGE"
        ]
    );
}

/// Every path a Workshop mod lists is on disk.
#[test]
fn every_workshop_mod_file_is_on_disk() {
    let library = workshop();
    let missing: Vec<PathBuf> = library
        .mods
        .iter()
        .flat_map(|m| {
            let lists = &m.load;

            lists
                .archives
                .iter()
                .chain(&lists.programs)
                .chain(&lists.descriptions)
                .chain(&lists.strings)
                .map(|(_, p)| {
                    p.split(['/', '\\'])
                        .fold(m.dir.clone(), |at, part| at.join(part))
                })
                .collect::<Vec<_>>()
        })
        .filter(|p| regent_core::content::on_disk(p).is_none())
        .collect();

    assert_eq!(missing, Vec::<PathBuf>::new());
}

/// The extra quests' list names a Workshop quest's item by its first
/// mod: Majesty Expanded's 28 quests, New Game+'s Legendary Heroes; The
/// Wild, an item with no mod, by its folder.
#[test]
fn a_workshop_quest_is_named_by_its_items_first_mod() {
    let library = workshop();
    let mut named: Vec<(String, usize)> = Vec::new();

    for q in &library.quests {
        let source = regent_core::view::menu::extras::source(&library, &q.package);

        match named.iter_mut().find(|(s, _)| *s == source) {
            Some((_, count)) => *count += 1,
            None => named.push((source, 1)),
        }
    }

    assert_eq!(
        named,
        [
            ("New Game+".to_owned(), 1),
            ("Majesty Expanded".to_owned(), 28),
            ("2892764983".to_owned(), 1)
        ]
    );
}
