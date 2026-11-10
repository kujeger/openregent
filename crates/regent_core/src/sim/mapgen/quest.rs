//! A quest's new game: its own `.q` is the recipe, not the dialog's picks.
//!
//! ```text
//! Q header   code ─────> the land: the region the region chart grows
//!              pattern ──> the template: that set 1 entry, alone
//!              word 1 ───> the map number: 0, so the clock
//!              size ─────> the side
//! Q slots    each live one a player, its gold APP:0
//! Q trailer  words 0, 1: the victory index and its goal
//! ```
//!
//! **Binary**: quest start runs
//! World generate charts on the quest's world as
//! Quest file read left it; freestyle setup never runs.
//! The template is the set 1 entry the header's pattern names,
//! the land its code, the map number the
//! header's second word, 0 in all 36 shipped `.q`: so
//! Rng seed takes clock seed, and every play is a new map.
//! **Corpus**: three Day of Reckoning starts, three maps.

use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::data::rgs::chart::Chart as Patterns;
use crate::sim::freestyle::Setup;
use crate::sim::freestyle::{self, Choice};
use crate::sim::world::{
    GOAL_SLOT, VICTORY_SLOT,
    new_map::{NEUTRAL, Seat},
};

use crate::sim::start::Recipe;

use crate::sim::mapgen::custom::Art;
use crate::sim::mapgen::fresh::{self, Made};

/// The force picks a quest leaves empty: its template is one entry.
const NONE: FourCc = FourCc::new(b"NONE");

/// Player slots before slot 7: the kingdoms.
const KINGDOMS: u32 = 7;

/// A slot entry's `active` word when live.
const LIVE: u32 = 1;

/// A quest's generator input: its land, its template, `seed` and its
/// size. `seed` stands in for the clock's word.
#[must_use]
pub const fn choice(map: &Map<'_>, seed: i32) -> Choice {
    Choice {
        land: map.code,
        settlement: map.pattern,
        scenario: NONE,
        monsters: NONE,
        seed,
        side: map.size.0,
    }
}

/// The players player setup makes from the quest's slots: each live
/// one of the seven kingdoms with its gold, then slot 7, always, unaligned
/// and with none.
#[must_use]
pub fn seats(map: &Map<'_>) -> Vec<Seat> {
    let live = |n: u32| {
        map.slots
            .get(usize::try_from(n).unwrap_or(usize::MAX))
            .filter(|s| s.active == LIVE)
            .map(|s| Seat {
                player: n,
                gold: s.gold.cast_signed(),
            })
    };
    let neutral = Seat {
        player: NEUTRAL,
        gold: 0,
    };

    (0..KINGDOMS)
        .filter_map(live)
        .chain(std::iter::once(neutral))
        .collect()
}

/// The setup words the world reads of a quest: the victory index and its
/// goal, the trailer's first two words; the rest as a
/// freestyle setup would hold them. A quest's save writes no setup block.
#[must_use]
pub fn setup(map: &Map<'_>, seed: i32) -> Setup {
    let crown = map.slots.first().map_or(0, |s| s.gold);
    let mut setup = freestyle::setup_of(&choice(map, seed), crown);

    setup.slots[VICTORY_SLOT] = map.trailer[0];
    setup.slots[GOAL_SLOT] = map.trailer[1];
    setup
}

/// Generate the quest's map from `seed` and stand up its start, measured
/// by `art`.
///
/// # Errors
///
/// A quest whose land is not one of its regions.
pub fn generate(
    map: &Map<'_>,
    patterns: &Patterns<'_>,
    seed: i32,
    art: &Art,
) -> Result<Made, String> {
    fresh::generate_with(
        map,
        patterns,
        (choice(map, seed), Recipe::Quest),
        (setup(map, seed), seats(map)),
        art,
    )
}
