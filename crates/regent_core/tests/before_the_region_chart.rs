//! What the two charts before the region chart spend, derived.
//!
//! [`MEASURED`]: offsets found by scanning the stream, 1 to 13 draws over
//! twelve scenarios. [`regent_core::sim::mapgen::force`] and [`regent_core::sim::mapgen::unit`] derive
//! them from the quest:
//!
//! ```text
//! 1                                       -- the grid's turn
//! + one per template placement that still had a free block to choose from
//! + one per placement of each block it placed that still had a free cell
//! ```
//!
//! matching every Windows freestyle save.
//!
//! # The settlement draws too
//!
//! Settlement placements precede the scenario's. The corpus's two settlements
//! never have a choice; `Shaa` "Shanty Town" is predicted (**unverified**) to
//! spend four more. See `docs/formats/maps.md` and
//! [`the_settlement_moves_the_offset_where_its_block_has_a_choice`].

mod common;

/// A unit pattern's `words[0]` on a Wandering Monsters entry.
///
/// **Binary** — pattern kind returns a field;
/// freestyle dialog refresh buckets on it: player count for Player
/// Forces, 9 for Enemy Forces, 10 for these.
const MONSTERS: u32 = 10;

/// The code the dialog's "None" item writes.
const NONE: FourCc = FourCc::new(b"NONE");

/// The block letters a placement may name.
///
/// **Binary** — force chart build skips cell 0 below `A` (compare at
/// force chart build) or above `Y`.
const LETTERS: std::ops::RangeInclusive<u8> = b'A'..=b'Y';

use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::Map;
use regent_core::data::rgs::map::{BLOCKS, UnitPattern};
use regent_core::sim::mapgen::rng::Rng;
use regent_core::sim::mapgen::setup;

/// Measured offsets by scenario, the oracle for the derivation.
///
/// **Corpus.** Each the unique zero-score minimum below 2²⁰, the runner-up
/// thousands of draws away. `Cona`, `Skea`, `dliW` on three lands each,
/// `Ruib` on four; twelve saves.
const MEASURED: [(&[u8; 4], u32); 12] = [
    (b"Ench", 1),
    (b"Skea", 1),
    (b"dliW", 1),
    (b"Cona", 2),
    (b"Gorg", 2),
    (b"Ruib", 3),
    (b"Ratx", 3),
    (b"Gobf", 3),
    (b"Wint", 3),
    (b"evaR", 7),
    (b"Shad", 8),
    (b"Gobe", 13),
];

/// Probe walk length. Short: a longer walk on a rho finds coincidences.
const PROBE: u32 = 64;

/// Draws from `seed` to where the charts stopped.
fn draws(seed: i32, ended: Rng) -> Option<u32> {
    let mut probe = Rng::new(seed);

    (0..PROBE).find(|_| {
        let here = probe.state == ended.state;

        if !here {
            probe.step();
        }

        here
    })
}

/// Run both charts on one setup and say what they spent.
///
/// # Panics
///
/// If the charts land beyond [`PROBE`] draws.
fn spent(map: &Map<'_>, settlement: FourCc, scenario: FourCc, seed: i32, side: u32) -> u32 {
    let mut rng = Rng::new(seed);

    let _ = setup::before_the_region_chart(
        map,
        &regent_core::sim::freestyle::Forces {
            settlement,
            scenario,
            monsters: FourCc::new(b"NONE"),
        },
        side,
        &mut rng,
    );

    draws(seed, rng).expect("the charts spend a handful of draws")
}

/// A settlement whose block has a choice moves the offset the scenario sets.
///
/// Derived from the file, **unverified** in game: a Shanty Town start spends
/// four more draws than Wizard Hold, whatever the scenario.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_settlement_moves_the_offset_where_its_block_has_a_choice() {
    /// In the corpus.
    const WIZARD_HOLD: &[u8; 4] = b"Wizb";
    /// Not in the corpus.
    const SHANTY_TOWN: &[u8; 4] = b"Shaa";
    /// What `Shaa`'s four two-lettered placements cost.
    const SHANTY_EXTRA: u32 = 4;

    let map = common::mapgen::map();

    let named = |code: &[u8; 4]| {
        let code = FourCc::new(code);

        let found: &UnitPattern<'_> = map.units[1]
            .iter()
            .find(|u| u.faction == code)
            .unwrap_or_else(|| panic!("{code} is not a dialog entry"));

        (code, found)
    };

    let (hold, hold_entry) = named(WIZARD_HOLD);
    let (shanty, shanty_entry) = named(SHANTY_TOWN);

    // Both single-player, so both offered.
    assert_eq!(hold_entry.players(), Some(1), "Wizard Hold is offered");
    assert_eq!(shanty_entry.players(), Some(1), "Shanty Town is offered");

    for (code, want) in MEASURED {
        let scenario = FourCc::new(code);

        for seed in [1, 2, 3, 4, 5] {
            assert_eq!(spent(map, hold, scenario, seed, 128), want, "{scenario}");
            assert_eq!(
                spent(map, shanty, scenario, seed, 128),
                want + SHANTY_EXTRA,
                "{scenario} from a Shanty Town"
            );
        }
    }
}

/// The Wandering Monsters pick is a third force, and it places no block.
///
/// **Binary** — freestyle setup merges three entries: settlement at
/// freestyle setup, scenario, this (skipped on
/// `NONE`). Config, save setup slot 2, prefs `RGS3`;
/// freestyle dialog refresh fills it from set 1
/// records with `words[0]` = [`MONSTERS`].
///
/// # Free for the two charts on shipped data
///
/// All 22 entries' placements are bare numbers (1, 3, 5, 7), which
/// force chart build skips. Modelled anyway: a lettered mod entry
/// would draw.
///
/// Not inert later: bare placement materialise spends a draw per
/// bare-number placement before the scatter slots (`crate::animate::Before`).
/// No corpus save measures it.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_monsters_pick_is_a_third_force_that_places_no_block() {
    let map = common::mapgen::map();

    let base = regent_core::sim::freestyle::Forces {
        settlement: FourCc::new(b"Fron"),
        scenario: FourCc::new(b"Ruib"),
        monsters: NONE,
    };

    // Every kind-10 entry: the dialog's list.
    let wandering: Vec<&UnitPattern<'_>> = map.units[1]
        .iter()
        .filter(|pattern| pattern.words[0] == MONSTERS)
        .collect();

    assert!(
        wandering.len() > 10,
        "only {} Wandering Monsters entries, so the quest is not the one this measured",
        wandering.len()
    );

    // None names a block.
    let lettered = wandering
        .iter()
        .flat_map(|pattern| pattern.placements.iter())
        .filter(|placed| placed.cells.first().is_some_and(|c| LETTERS.contains(c)))
        .count();
    let total: usize = wandering.iter().map(|p| p.placements.len()).sum();

    println!(
        "  {} entries, {total} placements, {lettered} of them lettered",
        wandering.len()
    );

    assert_eq!(
        lettered, 0,
        "{lettered} Wandering Monsters placements name a block letter, so the third merge \
         is no longer free and the two charts have to be re-measured"
    );

    // Through the charts: stream and blocks identical whatever is picked.
    let run = |picks: &regent_core::sim::freestyle::Forces| {
        let mut rng = Rng::new(1);
        let (force, _units) = setup::before_the_region_chart(map, picks, 128, &mut rng);
        let placed: Vec<FourCc> = (0..BLOCKS)
            .flat_map(|y| (0..BLOCKS).map(move |x| (x, y)))
            .map(|at| force.at(at))
            .collect();

        (rng.draws, placed)
    };

    let without = run(&base);

    for pattern in &wandering {
        assert_eq!(
            run(&regent_core::sim::freestyle::Forces {
                monsters: pattern.faction,
                ..base
            }),
            without,
            "{} changed the force chart, so it must name a block after all",
            pattern.faction
        );
    }

    // An unknown code is a no-op, like `NONE`.
    assert_eq!(
        run(&regent_core::sim::freestyle::Forces {
            monsters: FourCc::new(b"zzzz"),
            ..base
        }),
        without,
        "a code the quest does not hold changed the stream"
    );
}

/// The first draw after the seed turns the placement grid.
///
/// **Binary**, checked against ground. Force chart build
/// draws `rng_next(8)`, stores it;
/// block of letter applies that square symmetry. So
/// orientation depends on the map number alone.
///
/// Check: Winter Behemoths places `Icex` at `X`, one of `EVY`, one of `JPT`.
/// On Tholaria at map number 1 the snow fits orientation 7 best by a clear
/// margin, as drawn.
#[test]
fn the_first_draw_turns_the_placement_grid() {
    /// `rng_next(8)` one step past each seed.
    const TURNS: [(i32, u8); 6] = [(1, 7), (2, 4), (3, 2), (4, 0), (5, 1), (6, 7)];

    for (seed, want) in TURNS {
        let mut rng = Rng::new(seed);

        assert_eq!(
            u8::try_from(rng.below(8)).expect("a turn is under eight"),
            want,
            "map number {seed} turns the grid differently"
        );
    }

    // Map number 1: orientation 7, `A` bottom left, as in the Tholaria
    // saves.
    let turn = regent_core::data::rgs::map::Orientation(7);

    assert_eq!(turn.turn((0, 0)), (0, 4), "A is not bottom left at turn 7");
    assert_eq!(turn.turn((4, 0)), (0, 0), "E is not top left at turn 7");
    assert_eq!(
        turn.turn((3, 4)),
        (4, 1),
        "X did not land where the snow is"
    );
}
