//! A freestyle setup, and everything it does before the region chart.
//!
//! The dialog's four picks: land, settlement, scenario, Wandering Monsters. A
//! save writes all four (`crate::sim::freestyle::Setup`). The last three reach
//! the charts here; the region chart grows the land.
//!
//! **Binary** — freestyle setup builds the template at
//! `world` ([`crate::sim::freestyle::template`]);
//! World generate charts builds the force and unit charts from it.
//!
//! # The third pick is `NONE` in the whole corpus
//!
//! All 74 freestyle setups hold `NONE` in slot 2. Modelled anyway: a player can
//! set it (15 of the quest's 22 entries are offered in one-player games).
//!
//! "Random (Beginner)" picks draw from the generator seeded with the map
//! number, but world generate charts re-seeds before any chart, so they
//! need no model. The resolved code is not written back: such a game saves
//! `BEGI`, `ADVA` or `EXPE`. No corpus save does.

use crate::data::fourcc::FourCc;
use crate::data::rgs::Map;
use crate::data::rgs::map::BLOCKS;
use crate::sim::freestyle::{Forces, template};

use crate::sim::mapgen::force::Force;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::unit::Units;

/// Run the force and unit charts, leaving `rng` where the region chart starts.
///
/// `rng` must be freshly seeded with the map number.
#[must_use]
pub fn before_the_region_chart<'a>(
    map: &'a Map<'a>,
    picks: &Forces,
    side: u32,
    rng: &mut Rng,
) -> (Force, Units) {
    let placements = template(map, picks);
    let force = Force::build(&placements, rng);
    let block = |code: FourCc| map.units[0].iter().find(|u| u.faction == code);
    let units = Units::build(&force, (side, side), &block, rng);

    (force, units)
}

/// Every placed block that stamps its own ground, and what it stamps.
///
/// **Binary** — gates of region seed place's second phase at
/// region seed place:
/// [`UnitPattern::stamps`](crate::data::rgs::map::UnitPattern::stamps). Column-by-column walk, the order
/// codes reach the zone table.
#[must_use]
pub fn stamps<'a>(map: &'a Map<'a>, force: &Force) -> Vec<((u32, u32), FourCc)> {
    let mut out = Vec::new();

    for x in 0..BLOCKS {
        for y in 0..BLOCKS {
            let code = force.at((x, y));
            let Some(record) = map.units[0].iter().find(|u| u.faction == code) else {
                continue;
            };

            // `ground`, not `code`: region seed place reads the ground.
            // `code` repeats the record's code and would stamp `Icex` where
            // the engine stamps `$Dta`.
            if record.stamps() {
                out.push(((x, y), record.ground));
            }
        }
    }

    out
}
