//! The freestyle pick page, `SMNU` `APd2` (map number roll,
//! Map number roll): the quest map's compass opens it, a Map Number is
//! rolled, and three buttons start a game on a preset.
//! **Binary**.
//!
//! ```text
//!  APd2 opens ──> clock RNG: Map Number = rng_next(999,999,999)
//!   5000 BEGINNER RANDOM ─┐
//!   5003 ADVANCED RANDOM ─┼─> preset ──> the game's start config
//!   5007 EXPERT RANDOM ───┘
//!   5004 MANUAL SETTINGS ──> MX17, the setup dialog (freestyle dialog)
//!   5005 CANCEL ──> the quest map (AP30)
//! ```
//!
//! A preset writes the band's tag into all three force picks and
//! [`RANDOM_LAND`] into the land, so freestyle setup draws the
//! codes from the Map Number (`crate::sim::freestyle::random`); the gold is a GPL
//! constant; the special events are drawn on a second clock RNG.

use crate::data::fourcc::FourCc;
use crate::sim::freestyle::{Band, RANDOM_LAND, Victory};

use crate::sim::mapgen::rng::Rng;

/// The bound of the Map Number roll and of the preset's first draw:
/// `rng_next(999999999)`.
pub const MAP_NUMBERS: i32 = 999_999_999;

/// A special event left unpicked (event list rated's skip).
pub const NO_EVENT: FourCc = FourCc::new(b"NONE");

/// Out of this a second event's chance is drawn: `rng_next(100)`.
const PERCENT: i32 = 100;

/// The Map Number the page rolls as it opens: a local RNG seeded with the
/// clock (`timeGetTime`), one draw.
#[must_use]
pub const fn map_number(clock: i32) -> i32 {
    Rng::new(clock).below(MAP_NUMBERS)
}

/// One of the page's three presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Beginner,
    Advanced,
    Expert,
}

/// The `EVSC` entries an event is drawn from: ratings `low..=high`, the
/// `NONE` entry only with `none` (event list rated's arguments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rated {
    low: i32,
    high: i32,
    none: Allow,
}

/// Whether a draw may take the `NONE` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Allow {
    None,
    Kept,
}

/// How a preset picks its second event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Second {
    /// When `rng_next(100)` is below this, from the first event's list.
    Chance(i32),
    /// Always, from the first list with these entries appended: the
    /// helper pushes without clearing.
    Appended(Rated),
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Beginner, Preset::Advanced, Preset::Expert];

    /// Its button on `APd2`, as map number roll switches on it.
    #[must_use]
    pub const fn control(self) -> i32 {
        match self {
            Preset::Beginner => 5000,
            Preset::Advanced => 5003,
            Preset::Expert => 5007,
        }
    }

    /// The tag it writes into the three force picks.
    #[must_use]
    pub const fn band(self) -> Band {
        match self {
            Preset::Beginner => Band::Beginner,
            Preset::Advanced => Band::Advanced,
            Preset::Expert => Band::Expert,
        }
    }

    /// The GPL constant its starting gold is (`"#starting_gold_easy"`, `"#starting_gold_normal"`,
    /// `"#starting_gold_hard"`), read once into preset gold beginner and on.
    #[must_use]
    pub const fn gold_constant(self) -> &'static str {
        match self {
            Preset::Beginner => "#starting_gold_easy",
            Preset::Advanced => "#starting_gold_normal",
            Preset::Expert => "#starting_gold_hard",
        }
    }

    /// [`Preset::gold_constant`]'s value in the shipped `globals.gpl`.
    #[must_use]
    pub const fn gold(self) -> u32 {
        match self {
            Preset::Beginner => 20_000,
            Preset::Advanced => 15_000,
            Preset::Expert => 10_000,
        }
    }

    /// The first event's list.
    const fn first(self) -> Rated {
        match self {
            Preset::Beginner => Rated {
                low: 0,
                high: 50,
                none: Allow::Kept,
            },
            Preset::Advanced => Rated {
                low: 0,
                high: 80,
                none: Allow::None,
            },
            Preset::Expert => Rated {
                low: 0,
                high: 100,
                none: Allow::None,
            },
        }
    }

    /// The second event's pick.
    const fn second(self) -> Second {
        match self {
            Preset::Beginner => Second::Chance(50),
            Preset::Advanced => Second::Chance(25),
            Preset::Expert => Second::Appended(Rated {
                low: 45,
                high: 100,
                none: Allow::None,
            }),
        }
    }

    /// What it picks, `gold` its starting gold, on an RNG seeded with
    /// `clock` (`timeGetTime`), from `events`, every `EVSC` entry and its
    /// rating in table order.
    ///
    /// ```text
    ///  seed clock; rng_next(999,999,999), unused
    ///  event 1 = first list[rng_next(len)], NONE if empty
    ///  Beginner, Advanced: rng_next(100) < 50, 25 ──> event 2 from the first list
    ///  Expert: event 2 from the first list + ratings 45-100
    ///  event 2 = list[rng_next(len)]; equal to event 1: rng_next(len - 1),
    ///            skipping its index; a list of one: NONE
    /// ```
    #[must_use]
    pub fn picks(self, gold: u32, clock: i32, events: &[(FourCc, i32)]) -> Picked {
        let mut rng = Rng::new(clock);
        let first = listed(events, self.first());

        rng.below(MAP_NUMBERS);

        let one = drawn(&mut rng, &first);
        let two = match self.second() {
            Second::Chance(chance) => {
                (rng.below(PERCENT) < chance).then(|| other(&mut rng, &first, one))
            }
            Second::Appended(more) => {
                let list = [first, listed(events, more)].concat();

                Some(other(&mut rng, &list, one))
            }
        };
        let tag = self.band().tag();

        Picked {
            settlement: tag,
            scenario: tag,
            monsters: tag,
            land: RANDOM_LAND,
            gold,
            victory: Victory::Eliminate,
            events: [one, two.unwrap_or(NO_EVENT)],
        }
    }
}

/// What a preset sets: the picks a new game is made from, the Map Number
/// apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picked {
    pub settlement: FourCc,
    pub scenario: FourCc,
    pub monsters: FourCc,
    pub land: FourCc,
    pub gold: u32,
    /// `freestyle_cfg_set_victory(0)`.
    pub victory: Victory,
    /// The two special events, `EVSC` keys.
    pub events: [FourCc; 2],
}

/// The keys of `events` `rated` takes, in table order.
fn listed(events: &[(FourCc, i32)], rated: Rated) -> Vec<FourCc> {
    events
        .iter()
        .filter(|(key, _)| *key != NO_EVENT || rated.none == Allow::Kept)
        .filter(|(_, rating)| (rated.low..=rated.high).contains(rating))
        .map(|(key, _)| *key)
        .collect()
}

/// One of `list` on `rng`; [`NO_EVENT`] from an empty list, no draw.
fn drawn(rng: &mut Rng, list: &[FourCc]) -> FourCc {
    match list.len() {
        0 => NO_EVENT,
        len => list[index(rng.below(count(len)), len)],
    }
}

/// One of `list` other than `not`, as preset code pick draws it.
fn other(rng: &mut Rng, list: &[FourCc], not: FourCc) -> FourCc {
    match list.len() {
        0 => NO_EVENT,
        len => {
            let at = index(rng.below(count(len)), len);

            match (list[at] == not, len) {
                (false, _) => list[at],
                (true, 1) => NO_EVENT,
                (true, _) => {
                    let again = index(rng.below(count(len - 1)), len);
                    let skip = if again >= at { again + 1 } else { again };

                    list[skip.min(len - 1)]
                }
            }
        }
    }
}

/// A list's length as a draw's bound.
fn count(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(i32::MAX)
}

/// A draw as an index into a list of `len`.
fn index(draw: i32, len: usize) -> usize {
    usize::try_from(draw)
        .unwrap_or(0)
        .min(len.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `EVSC` as HD ships it (`regent_cli strings`).
    const EVENTS: [(&[u8; 4], i32); 16] = [
        (b"NONE", 0),
        (b"EV01", 60),
        (b"EV02", 39),
        (b"EV03", 45),
        (b"EV04", 60),
        (b"EV05", 70),
        (b"EV06", 39),
        (b"EV07", 95),
        (b"EV08", 70),
        (b"EV09", 70),
        (b"EV10", 39),
        (b"EV11", 75),
        (b"EV12", 85),
        (b"EV13", 80),
        (b"EV14", 70),
        (b"EV15", 20),
    ];

    fn events() -> Vec<(FourCc, i32)> {
        EVENTS.iter().map(|(k, r)| (FourCc::new(k), *r)).collect()
    }

    #[test]
    fn a_preset_writes_its_tag_into_the_forces_and_random_into_the_land() {
        for preset in Preset::ALL {
            let picked = preset.picks(preset.gold(), 1234, &events());
            let tag = preset.band().tag();

            assert_eq!(
                [picked.settlement, picked.scenario, picked.monsters],
                [tag; 3]
            );
            assert_eq!(picked.land, RANDOM_LAND);
            assert_eq!(picked.victory, Victory::Eliminate);
            assert_eq!(picked.gold, preset.gold());
        }
    }

    /// The Expert's appended list holds a code twice, so its reroll can
    /// land on the first event again: the engine's, kept.
    #[test]
    fn a_presets_events_come_from_its_ratings() {
        let events = events();
        let rating = |key: FourCc| events.iter().find(|(k, _)| *k == key).map(|(_, r)| *r);
        let mut twice = 0;

        for clock in 0..200 {
            for preset in Preset::ALL {
                let [one, two] = preset.picks(0, clock, &events).events;
                let Rated { low, high, .. } = preset.first();

                assert!(rating(one).is_some_and(|r| (low..=high).contains(&r)));

                match preset {
                    Preset::Beginner => assert!(one != two || one == NO_EVENT),
                    Preset::Advanced => assert!(one != two && one != NO_EVENT),
                    Preset::Expert => {
                        assert!(one != NO_EVENT && two != NO_EVENT);
                        twice += usize::from(one == two);
                    }
                }
            }
        }

        assert!(twice > 0);
    }

    #[test]
    fn the_expert_second_list_appends_to_the_first() {
        let events = events();
        let first = listed(&events, Preset::Expert.first());
        let Second::Appended(more) = Preset::Expert.second() else {
            panic!("the expert's second event is always drawn");
        };

        assert_eq!(first.len(), 15);
        assert_eq!(listed(&events, more).len(), 11);
    }

    #[test]
    fn the_map_number_is_one_draw_under_a_billion() {
        for clock in [0, 1, 0x0b60_9e2c, i32::MAX] {
            let number = map_number(clock);

            assert!((0..MAP_NUMBERS).contains(&number));
            assert_eq!(number, Rng::new(clock).below(MAP_NUMBERS));
        }
    }
}
