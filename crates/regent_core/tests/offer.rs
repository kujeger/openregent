//! What the player may order at a new game's start (`world::offer`): the
//! build menu, the preview's verdict, a guild's recruits, the spells.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::{
    guild::gamble::BetLimits,
    offer::{Menu, by_title},
};

/// Sorted as the list sorts: byte order, case kept.
#[test]
fn the_list_sorts_by_name_with_capitals_first() {
    let mut rows = vec![
        ("Wizard Guild".to_owned(), 1),
        ("blacksmith".to_owned(), 2),
        ("Inn".to_owned(), 3),
        ("Agrela's Temple".to_owned(), 4),
        ("Inn".to_owned(), 5),
    ];

    by_title(&mut rows);

    let order: Vec<_> = rows.iter().map(|(_, id)| *id).collect();

    assert_eq!(order, [4, 3, 5, 1, 2]);
}

/// A warriors' guild produces three heroes, a rangers' guild one.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_guild_produces_what_its_descriptor_lists() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let produces = |code: &[u8; 4]| {
        regent_core::content::unit::Unit::find(&catalogue, FourCc::new(code))
            .and_then(Result::ok)
            .and_then(|u| u.desc.building().map(|b| b.produces()))
            .unwrap_or_default()
    };

    assert_eq!(
        produces(b"ABV1"),
        [b"AVF1", b"AVL1", b"AVM1"].map(FourCc::new)
    );
    assert_eq!(produces(b"ABW1"), [FourCc::new(b"AVH1")]);
    assert_eq!(produces(b"ABC1").len(), 0);
}

/// One Sorcerer's Abode a player: its `BDEP` rule names itself, so the
/// build menu offers none while one stands at any level, even unfinished,
/// and one with a finished Magic Bazaar and no Abode.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_build_menu_offers_one_sorcerers_abode() {
    use regent_core::data::deps::Stand;

    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let menu = Menu::of(&catalogue);
    let rule = menu.rule(FourCc::new(b"ABq1")).expect("the Abode's rule");
    let holds = |stands: &[(&[u8; 4], Stand)]| {
        rule.holds(&|id| {
            stands
                .iter()
                .find(|(code, _)| FourCc::new(code) == id)
                .map_or(Stand::Absent, |(_, s)| *s)
        })
    };
    let bazaar = (b"ABl1", Stand::Finished);

    assert!(holds(&[bazaar]));
    assert!(!holds(&[]));

    for abode in [b"ABq1", b"ABq2", b"ABq3"] {
        assert!(!holds(&[bazaar, (abode, Stand::Placed)]), "{abode:?}");
        assert!(!holds(&[bazaar, (abode, Stand::Finished)]), "{abode:?}");
    }
}

/// The Gambling Hall's bet limits, as the scripts set them.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_gambling_limits_are_the_scripts() {
    let limits = BetLimits::of(common::sdk_leaked());

    assert_eq!(
        limits,
        BetLimits {
            least: 25,
            most: 1000,
            step: 25
        }
    );
}
