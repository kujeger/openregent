//! The freestyle pick page's presets and the Random picks they make,
//! against HD's data: the gold constants, the `EVSC` table, and what a
//! Random pick draws from (`regent_core::sim::mapgen::preset`, `freestyle::randoms`).
//!
//! Corpora: `MAJESTY_HD`.

use regent_corpus as common;

use common::read;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::{self, Map};
use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::freestyle::{self, Band, List, ONE_PLAYER, Picks, RANDOM_LAND};
use regent_core::sim::mapgen::custom;
use regent_core::sim::mapgen::preset::{NO_EVENT, Preset};
use regent_core::sim::setup;

/// The freestyle quest, under the install.
const QUEST: &str = "QuestsMX/mx_Freestyle.q";

fn quest() -> Vec<u8> {
    let path = regent_core::content::on_disk(&common::hd().join(QUEST)).expect("on disk");

    read(&path)
}

fn map(bytes: &[u8]) -> Map<'_> {
    rgs::File::parse(bytes)
        .and_then(|f| f.map())
        .expect("quest")
}

#[test]
fn a_presets_gold_is_its_gpl_constant() {
    let images: Vec<Vec<u8>> = common::sdk_images().iter().map(|p| read(p)).collect();
    let program = Program::load(&images).expect("SDK loads");

    for preset in Preset::ALL {
        let value = program
            .constant(preset.gold_constant())
            .unwrap_or_else(|| panic!("{}", preset.gold_constant()));

        assert!(
            matches!(value, Literal::Int(gold) if gold == preset.gold().cast_signed()),
            "{preset:?}: {value:?}"
        );
    }
}

#[test]
fn the_event_table_is_rated_none_first() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let events = setup::event_list(&catalogue);

    assert_eq!(events.len(), 16);
    assert_eq!(events[0], (NO_EVENT, 0));
    assert_eq!(events[3], (FourCc::new(b"EV03"), 45));
    assert_eq!(events[15], (FourCc::new(b"EV15"), 20));

    // Every preset draws from it.
    for preset in Preset::ALL {
        let picked = preset.picks(preset.gold(), 7, &events);

        assert!(events.iter().any(|(k, _)| *k == picked.events[0]));
    }
}

/// Each band offers a Random pick something to draw, every candidate one
/// the dialog lists, and a land Random draws from the lands it lists.
#[test]
fn every_random_pick_draws_from_what_the_dialog_offers() {
    let bytes = quest();
    let map = map(&bytes);
    let offered = |list, players| -> Vec<FourCc> {
        freestyle::offers(&map, list, players)
            .iter()
            .map(|o| o.code)
            .collect()
    };

    // Every count: no Random pick draws from nothing, so the LAN lists
    // offer all three bands as the single player's do.
    for players in ONE_PLAYER..=freestyle::MOST_PLAYERS {
        for list in [List::Settlement, List::Scenario, List::Monsters] {
            for band in Band::ALL {
                let candidates = freestyle::randoms(&map, list, band, players);

                assert!(!candidates.is_empty(), "{players} {list:?} {band:?}");
                assert!(
                    candidates
                        .iter()
                        .all(|c| offered(list, players).contains(c)),
                    "{players} {list:?} {band:?}: {candidates:?}"
                );
            }
        }

        let lands = freestyle::random_lands(&map, players);
        let mut sorted = offered(List::Land, players);

        sorted.sort_by_key(|c| c.word());
        assert_eq!(lands, sorted, "{players}");
    }
}

/// Only the sides the generator makes are taken. Another is refused, not
/// drawn wrong (100: the fractal leaves vertices unset) or panicked on
/// (46,341: the cell count overflows).
#[test]
fn a_side_the_generator_does_not_make_is_refused() {
    let bytes = quest();
    let map = map(&bytes);
    let picks = |side| Picks {
        side: Some(side),
        ..Picks::default()
    };

    for side in [128, 256, 512] {
        assert_eq!(custom::chosen(&picks(side), &map).map(|c| c.side), Ok(side));
    }

    for side in [0, 4, 100, 1024, 46_341] {
        assert!(custom::chosen(&picks(side), &map).is_err(), "{side}");
    }
}

/// A preset's picks make a game: the codes drawn, the side the drawn
/// scenario's, the setup holding the tags.
#[test]
fn a_presets_picks_make_a_choice_and_keep_their_tags() {
    let bytes = quest();
    let map = map(&bytes);

    for preset in Preset::ALL {
        let picked = preset.picks(preset.gold(), 99, &[]);
        let picks = Picks {
            land: Some(picked.land),
            settlement: Some(picked.settlement),
            scenario: Some(picked.scenario),
            monsters: Some(picked.monsters),
            seed: Some(123_456_789),
            side: None,
            players: None,
        };
        let choice = custom::chosen(&picks, &map).expect("choice");
        let scenario = map.units[1]
            .iter()
            .find(|u| u.faction == choice.scenario)
            .expect("drawn scenario");
        let setup = freestyle::tagged(freestyle::setup_of(&choice, picked.gold), picks.asked());

        assert_eq!(choice.side, scenario.side());
        assert_ne!(choice.scenario, preset.band().tag());
        assert_eq!(setup.scenario(), preset.band().tag());
        assert_eq!(setup.settlement(), preset.band().tag());
        assert_eq!(setup.land(), RANDOM_LAND);
        assert_eq!(setup.gold(), preset.gold());
    }
}
