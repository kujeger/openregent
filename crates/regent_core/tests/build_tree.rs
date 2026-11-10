//! Build trees: the shipped five and the `.BTD` format, a new game on
//! one, and the Build Tree Editor's list.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::build_tree::{self, Tree};
use regent_core::view::menu::tree::{self, Rows};

fn shipped() -> Vec<Tree> {
    let library = common::library();

    Tree::shipped(&library.catalogue().expect("catalogue"))
}

fn named(trees: &[Tree], name: &str) -> Tree {
    trees
        .iter()
        .find(|t| t.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("no tree {name}"))
}

/// The writer's format is the records': each shipped tree encodes to
/// its `BTDT` record byte for byte, pairs in word order, its name the
/// record's (`BTD5`'s is `Wiz War`, renamed `Wizard War` by `BTDN`).
#[test]
fn every_shipped_tree_encodes_to_its_record() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let trees = Tree::shipped(&catalogue);
    let records: Vec<&[u8]> = catalogue
        .tagged(&build_tree::RECORDS)
        .map(|(_, r)| r.data)
        .collect();

    let names: Vec<&str> = trees.iter().map(|t| t.name.as_str()).collect();

    assert_eq!(
        names,
        [
            build_tree::DEFAULT,
            "Brute Force",
            "No Towers",
            "One of a Kind",
            "Wizard War"
        ]
    );
    assert_eq!(records.len(), trees.len());

    for (tree, record) in trees.iter().zip(records) {
        let own = Tree {
            name: Tree::parse(record).expect("record").name,
            ..tree.clone()
        };

        assert_eq!(own.encode(), record, "{}", tree.name);
    }

    assert_eq!(
        named(&trees, "Wizard War").file_name(),
        "userWizard War.BTD"
    );
}

/// The editor's list as `ui_freestyle_building_limitations_setup.png`
/// shows it scrolled: twenty rows from `Blacksmith Level 2`, each with or
/// without + - and the count. **Game**: the Gnome Hovel, `ABf1`, hides
/// them, as the upgrades do.
#[test]
fn the_editors_list_is_the_captures() {
    let library = common::library();
    let rows = Rows::of(&library.catalogue().expect("catalogue"));
    let first = rows
        .rows
        .iter()
        .position(|r| r.name == "Blacksmith Level 2")
        .expect("Blacksmith Level 2");
    let shown: Vec<(&str, bool)> = rows.rows[first..first + tree::ROWS]
        .iter()
        .map(|r| (r.name.as_str(), r.counted))
        .collect();

    assert_eq!(
        shown,
        [
            ("Blacksmith Level 2", false),
            ("Blacksmith Level 3", false),
            ("Dwarven Settlement", true),
            ("Elven Bungalow", true),
            ("Embassy", true),
            ("Fairgrounds", true),
            ("Gnome Hovel", false),
            ("Guardhouse", true),
            ("Guardhouse Level 2", false),
            ("Hall Of Champions", true),
            ("Inn", true),
            ("Library", true),
            ("Library Level 2", false),
            ("Magic Bazaar", true),
            ("Magic Bazaar Level 2", false),
            ("Magic Bazaar Level 3", false),
            ("Marketplace", true),
            ("Marketplace Level 2", false),
            ("Marketplace Level 3", false),
            ("Mausoleum", true),
        ]
    );
}

/// SET ALL caps every row at 0, greying each upgrade; REMOVE ALL leaves
/// none.
#[test]
fn set_all_greys_the_upgrades_and_remove_all_clears() {
    let library = common::library();
    let rows = Rows::of(&library.catalogue().expect("catalogue"));
    let mut tree = Tree::default();
    let upgrade = rows
        .rows
        .iter()
        .find(|r| r.name == "Library Level 2")
        .expect("Library Level 2")
        .code;

    assert!(rows.able(&tree, upgrade));

    rows.set_all(&mut tree);

    assert_eq!(tree.caps.len(), rows.rows.len());
    assert!(!rows.able(&tree, upgrade));

    rows.remove_all(&mut tree);

    assert!(tree.caps.is_empty());
    assert!(tree::reserved(&shipped(), "No Towers"));
    assert!(!tree::reserved(&shipped(), "no towers"));
}
