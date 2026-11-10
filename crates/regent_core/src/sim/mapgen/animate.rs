//! Which picture of itself each placed thing draws.
//!
//! After the eighth chart, chart cells become agents; each draws one picture
//! from the charts' stream.
//!
//! # Where in the stream
//!
//! **Binary** — world materialise, in order:
//!
//! ```text
//! level setup         two setters, no draw
//! player setup        no draw: a Player is not an agent
//! bare placements     the placements the force chart
//!                     skipped -- one draw per agent it creates
//! landscape place     x outer and y inner over every
//!                     cell: the paint, then the scatter, and one
//!                     rng_next(slot count) per object created
//! unit chart place    -- the unit chart,
//!                     so the settlement stands up after this
//! Map                 draws from the paint RNG only
//! ```
//!
//! The draw is `GSUnit` through agent animate:
//! one `rng_next` over the agent's animation slot count, gated
//! only by unit animation absent.
//!
//! The fractal re-seeds per height profile, so this resumes where the last
//! profile's pass ended. [`crate::sim::mapgen::generate_from`] returns that state.
//!
//! Order and bound are from the binary. [`Before`] is a parameter: the draws
//! Bare placement materialise spends depend on the land's bare-number
//! placements (**Measured** against a save).

use std::collections::BTreeMap;

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::mobject::{self, FOOT};
use crate::sim::mapgen::region::NONE;
use crate::sim::mapgen::rng::Rng;

/// How many draws run between the heightfield and the first scatter object.
///
/// Bare placement materialise creates an agent, one draw each, per
/// template placement the force chart skipped (cell 0 a bare number, not a
/// letter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Before(pub u32);

/// The slot every scatter object draws, by the cell it stands on.
///
/// The slot count comes from the descriptor's `IMAG` set via `crate::view`.
/// An unknown code gets slot zero and still spends a draw, so a gap in the art
/// does not shift later objects: a bound under two returns zero but still
/// advances the state. A missing animation is a separate gate,
/// unit animation absent.
#[must_use]
pub fn slots(
    chart: &mobject::Chart,
    side: u32,
    after: &Rng,
    before: Before,
    count: &dyn Fn(FourCc) -> Option<u16>,
) -> BTreeMap<(i32, i32), u16> {
    let mut rng = *after;
    let mut out = BTreeMap::new();

    // Draws for bare placement materialise's agents; slots not kept.
    for _ in 0..before.0 {
        rng.below(1);
    }

    let span = i32::try_from(side).unwrap_or(i32::MAX);

    // Landscape place's walk: x outer (world 0xcc), y inner (world
    // 0xd0). Storage is y outer.
    for x in 0..span {
        for y in 0..span {
            let Some(code) = chart.at((x, y)) else {
                continue;
            };

            if code == NONE || code == FOOT {
                continue;
            }

            let bound = count(code).unwrap_or(0);
            let drawn = rng.below(i32::from(bound));

            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a remainder by a slot count, which is a u16"
            )]
            out.insert((x, y), drawn as u16);
        }
    }

    out
}
