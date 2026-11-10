//! Spell icon table, against shipped art.
//!
//! [`regent_core::view::panel::icons`] comes from a g++ build's comparison
//! chain; that evidence is in `local/archive/linux-port.md`.

mod common;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::fourcc::FourCc;
use regent_core::view::panel::icons;

/// Every icon exists in `INTn`.
#[test]
fn the_spellbook_holds_every_icon_the_switch_picks() {
    let library = Library::from_files(&[common::hd().join("Data/interfacedata.cam")])
        .expect("the interface archive");
    let catalogue = library.catalogue().expect("a catalogue");

    let at = catalogue
        .find(b"IMAG", icons::SPELLBOOK)
        .expect("no INTn record");
    let sets = regent_core::content::imagery::sets(&catalogue, at).expect("INTn parses");

    let held: Vec<u16> = sets
        .iter()
        .map(|set| regent_core::data::imag::set_number(set.id))
        .collect();

    // Exactly 1001 to 1024, in order.
    assert_eq!(held, (1001..=1024).collect::<Vec<u16>>(), "INTn changed");

    for (spell, icon) in icons::SPELL_ICONS {
        assert!(
            held.contains(&icon),
            "{spell} asks INTn for {icon}, which it has not got"
        );
    }

    assert!(held.contains(&icons::SPELL_ICON_FALLBACK));
}

/// Every switch spell is a `DUNT` or a `DACT`. Of the seven seen in a save's
/// `SECD`, five are `DUNT` overlays and two `DACT` actions.
#[test]
fn every_spell_the_switch_names_is_a_shipped_descriptor() {
    let game = common::hd();
    let owned = std::fs::read(common::hd().join("Data/MajestyDatasetDefinitions.xml"))
        .expect("a definition");
    let set = regent_core::content::dataset::parse(std::str::from_utf8(&owned).expect("ASCII"));
    let base = game.join("Data");
    let library = set.library(&base, &[("MajestyDataPath", base.as_path())]);
    let catalogue = library.catalogue().expect("a catalogue");

    let described = |id: FourCc, catalogue: &Catalogue<'_>| {
        Unit::find(catalogue, id).is_some() || catalogue.find(b"DACT", id).is_some()
    };

    let missing: Vec<String> = icons::SPELL_ICONS
        .iter()
        .map(|(spell, _)| *spell)
        .filter(|id| !described(*id, &catalogue))
        .map(|id| id.to_string())
        .collect();

    // The switch names one spell the base dataset lacks.
    assert_eq!(missing, vec!["MRd1"], "the shipped spells changed");
}

/// Workshop mod Phantom's Haunt: its `PHM1Phantom` hero clones the stock
/// Priestess, swapping art but keeping set ids, as a fixed table requires.
#[test]
fn a_workshop_hero_uses_the_stock_interface_ids() {
    use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT};

    let mod_art = common::workshop().join("3769947406/Data/phantom_maindata.cam");

    if !mod_art.exists() {
        eprintln!("no Phantom's Haunt in the workshop corpus; skipped");
        return;
    }

    let library = Library::from_files(&[mod_art]).expect("the mod's art");
    let catalogue = library.catalogue().expect("a catalogue");
    let at = catalogue
        .find(b"IMAG", FourCc::new(b"PHM1"))
        .expect("no PHM1 record");
    let sets = regent_core::content::imagery::sets(&catalogue, at).expect("PHM1 parses");

    let held = |want: u16| {
        sets.iter()
            .find(|set| regent_core::data::imag::set_number(set.id) == want)
            .and_then(|set| match set.poses[0].stream(0)[0] {
                regent_core::content::imagery::FrameSource::Tile { archive, index, .. } => {
                    regent_core::view::Sprite::at(&catalogue, archive, index)
                }
                _ => None,
            })
            .and_then(Result::ok)
            .map(|sprite| (sprite.width, sprite.height))
    };

    // Stock hero ids and sizes.
    assert_eq!(held(INTERFACE_PORTRAIT), Some((100, 100)), "portrait");
    assert_eq!(held(INTERFACE_PANEL), Some((200, 245)), "panel picture");
    assert_eq!(held(INTERFACE_ICON), Some((25, 25)), "list icon");
}
