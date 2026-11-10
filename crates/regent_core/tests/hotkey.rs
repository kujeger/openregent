//! Hot keys against HD's panels (`regent_core::view::menu::hotkey`).

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::view::menu::hotkey::{self, Able};
use regent_core::view::menu::{self, Layout};

/// The game's panels: HD loads the expansion.
fn layout(id: [u8; 4]) -> Layout {
    let catalogue = common::expansion();
    let text = Text::build(catalogue);

    menu::layout(catalogue, &text, FourCc(id))
        .expect("the panel ships")
        .expect("it parses")
}

/// The control `pressed` presses with every control enabled but `off`.
fn pressed(layout: &Layout, pressed: i32, off: &[i32]) -> Option<i32> {
    let able = |id: Option<i32>| {
        if id.is_some_and(|id| off.contains(&id)) {
            Able::Disabled
        } else {
            Able::Enabled
        }
    };

    hotkey::pick(layout.controls.iter().map(|c| (c, able(c.id))), pressed).and_then(|c| c.id)
}

const R: i32 = b'r' as i32;

/// Every guild's recruit button, 8008, is R; the warriors' guild's
/// lone recruit is 5001, its paladin 8008 P.
#[test]
fn r_recruits_at_a_guild() {
    for guild in [
        b"AP01", b"AP05", b"AP06", b"AP07", b"AP47", b"AP48", b"AP53",
    ] {
        assert_eq!(pressed(&layout(*guild), R, &[]), Some(8008), "{guild:?}");
    }

    let warriors = layout(*b"AP52");

    assert_eq!(pressed(&warriors, R, &[]), Some(5001));
    assert_eq!(pressed(&warriors, i32::from(b'p'), &[]), Some(8008));
    assert_eq!(pressed(&warriors, R, &[5001]), None);
}

/// A guild's other keys: H its heroes, X demolish, ? its tips.
#[test]
fn a_guild_panel_has_heroes_demolish_and_tips_keys() {
    let rangers = layout(*b"AP47");

    assert_eq!(pressed(&rangers, i32::from(b'h'), &[]), Some(8004));
    assert_eq!(pressed(&rangers, i32::from(b'x'), &[]), Some(8002));
    assert_eq!(
        pressed(&rangers, i32::from(b'?') | hotkey::SHIFT, &[]),
        Some(8029)
    );
}

/// The main screen's keys: Escape the options, F1 the palace, F9 names,
/// F10 hit point bars, F11 the zoom, F12 the minimap's textures.
#[test]
fn the_main_screen_holds_escape_f1_and_f9_to_f12() {
    let mut keys: Vec<i32> = layout(*b"APMK")
        .controls
        .iter()
        .filter_map(|c| c.hotkey)
        .collect();

    keys.sort_unstable();

    assert_eq!(
        keys,
        [
            hotkey::ESCAPE,
            hotkey::F1,
            hotkey::F1 + 8,
            hotkey::F1 + 9,
            hotkey::F1 + 10,
            hotkey::F1 + 11
        ]
    );
}

/// Each researcher's research page (`Base_ResearchedBuildingController`'s
/// method): the Blacksmith, the Fairgrounds, the Guardhouse, the Library,
/// the Marketplace, the Trading Post, the Magic Bazaar.
const RESEARCH_PAGES: [&[u8; 4]; 7] = [
    b"AP99", b"APa0", b"APa1", b"APa2", b"APa3", b"APa4", b"MX03",
];

/// Every research's button is its id on a research page, labelled and
/// keyed there: research button sets the price's text, never the
/// button's.
#[test]
fn every_research_has_a_labelled_keyed_button() {
    let pages: Vec<Layout> = RESEARCH_PAGES.iter().map(|p| layout(**p)).collect();

    for item in regent_core::sim::world::research::ITEMS {
        let id = i32::try_from(item.id).expect("an id");
        let button = pages
            .iter()
            .find_map(|p| p.control(id))
            .unwrap_or_else(|| panic!("{id} on a research page"));

        assert!(button.text.is_some(), "{id} labelled");
        assert!(button.hotkey_at().is_some(), "{id} keyed in its label");
    }
}

/// The Blacksmith's rows: W weapons, A armour; the first enabled row in
/// panel order takes the key.
#[test]
fn the_blacksmiths_research_keys_are_w_and_a() {
    let smithy = layout(*b"AP99");
    let (w, a) = (i32::from(b'w'), i32::from(b'a'));

    assert_eq!(
        smithy.control(5000).and_then(|c| c.text.as_deref()),
        Some("LVL 2 Weapons")
    );
    assert_eq!(pressed(&smithy, w, &[]), Some(5000));
    assert_eq!(pressed(&smithy, a, &[]), Some(5001));
    assert_eq!(pressed(&smithy, w, &[5000]), Some(5002));
    assert_eq!(pressed(&smithy, a, &[5001]), Some(5005));
}
