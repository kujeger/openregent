//! The surface chart: terrain patterns become terrain classes.
//!
//! **Binary** — surface chart build, the fifth chart.
//! Turns each cell's terrain pattern into the `DTPT` class a save stores, in
//! three passes:
//!
//! ```text
//! 1  every cell takes its pattern's entry 0            -- no draws
//! 2  shuffle a w/3 by h/3 grid, and per block:         -- (w/3)*(h/3) draws
//!        pick a cell inside it with two rng_next(3)    -- 2 draws
//!        where that cell's pattern has exactly 2 entries and entry 1 is
//!        not NONE, one distribution_pick               -- 1 draw
//!        a pick that lands on entry 1 blotches         -- 1 + n draws
//! 3  a cell whose class is on none of its four orthogonal neighbours is
//!    flipped to the other class of its pair            -- no draws
//! ```
//!
//! A one-entry `*_All_*` pattern lays one class; a `*_Mostly_*` pattern lays
//! its majority and speckles the minority.
//!
//! # The blotch
//!
//! Surface blotch writes the minority over a cell and up to eight neighbours.
//! The count starts at `k`, from entry 0's `rest[0]`:
//!
//! ```text
//! k = entry0.rest[0] / 5          -- 5 being a range's bound of 20, over 4
//! n = rng_next(8 - k) + k
//! ```
//!
//! Each neighbour is `rng_next(8)`, walking forward past taken ones: no
//! repeats.
//!
//! A cell (centre or neighbour) is written only where all eight of its own
//! neighbours hold the minority or majority class: no speckle on a biome
//! boundary.

use std::collections::BTreeMap;

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::region::{NONE, RING};
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::shuffle::shuffled_points;
use crate::sim::mapgen::terrain;
use crate::sim::mapgen::world::Refusal;

/// The block pass's block width in cells.
///
/// **Binary** — divisions by `0x55555556` in surface chart build; `rng_next(3)` at
/// surface chart build and surface chart build.
pub const BLOCK: u32 = 3;

/// What a pattern entry's `rest[0]` is divided by to start a blotch.
///
/// **Binary** — surface blotch divides by `v / 4`, `v` the upper bound of
/// world entry 2's second range, a literal 20 in world range pair. Divisor 5;
/// shipped `rest[0]` of 0 to 20 gives `k` 0 to 4.
pub const SPREAD_BOUND: u32 = 20;

/// The four orthogonal neighbours, in the order surface has orthogonal takes them.
const ORTHOGONAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// Neighbours a blotch may reach: the whole ring.
const NEIGHBOURS: i32 = 8;

/// One terrain pattern, as the surface chart uses it.
#[derive(Debug, Clone)]
pub struct Ground {
    /// The weighted table the sprinkle draws from.
    pub table: Distribution,
    /// Entry 0's `rest[0]`: a blotch's starting width.
    pub spread: u32,
}

impl Ground {
    /// Where a blotch's neighbour count starts.
    #[must_use]
    pub fn start(&self) -> i32 {
        i32::try_from(self.spread / (SPREAD_BOUND / 4)).unwrap_or(0)
    }
}

/// One terrain class per cell: what a save's `MTER` stores.
#[derive(Debug, Clone)]
pub struct Chart {
    side: i32,
    cells: Vec<FourCc>,
}

impl Chart {
    /// Run all three passes over a finished terrain chart.
    ///
    /// `patterns` maps a terrain pattern's code to its table. A missing code
    /// gives [`NONE`].
    ///
    /// # Errors
    ///
    /// A pattern whose blotch starts past `NEIGHBOURS`: it would look
    /// for a free neighbour forever.
    ///
    /// # Panics
    ///
    /// If the terrain chart's side does not fit in a `u32`.
    pub fn build(
        terrain: &terrain::Chart,
        patterns: &BTreeMap<FourCc, Ground>,
        rng: &mut Rng,
    ) -> Result<Self, Refusal> {
        if let Some((code, ground)) = patterns.iter().find(|(_, g)| g.start() > NEIGHBOURS) {
            return Err(Refusal::Spread(*code, ground.spread));
        }

        let side = terrain.side();
        let across = u32::try_from(side).expect("a side the terrain chart was built with");

        let mut chart = Chart {
            side,
            cells: terrain
                .cells()
                .iter()
                .map(|code| Self::base(patterns, *code))
                .collect(),
        };

        chart.sprinkle(terrain, patterns, across, rng);
        chart.unspeckle(terrain, patterns);
        Ok(chart)
    }

    /// Pass one: entry 0 of a cell's pattern.
    fn base(patterns: &BTreeMap<FourCc, Ground>, code: FourCc) -> FourCc {
        patterns
            .get(&code)
            .and_then(|ground| ground.table.at(0))
            .unwrap_or(NONE)
    }

    /// Pass two: one candidate cell per three-by-three block, and a blotch
    /// where the draw lands on the pattern's minority entry.
    fn sprinkle(
        &mut self,
        terrain: &terrain::Chart,
        patterns: &BTreeMap<FourCc, Ground>,
        across: u32,
        rng: &mut Rng,
    ) {
        for (bx, by) in shuffled_points(rng, across / BLOCK, across / BLOCK) {
            let at = (
                (rng.below(BLOCK.cast_signed()) + (bx * BLOCK).cast_signed()),
                (rng.below(BLOCK.cast_signed()) + (by * BLOCK).cast_signed()),
            );

            let Some(ground) = terrain.at(at).and_then(|code| patterns.get(&code)) else {
                continue;
            };

            // Only two-entry patterns sprinkle, and only they draw.
            if ground.table.len() != 2 {
                continue;
            }

            let Some(minor) = ground.table.at(1).filter(|id| *id != NONE) else {
                continue;
            };

            if ground.table.pick(rng) == Some(minor) {
                self.blotch(terrain, patterns, at, minor, rng);
            }
        }
    }

    /// Write the minority class over a cell and some of its neighbours.
    fn blotch(
        &mut self,
        terrain: &terrain::Chart,
        patterns: &BTreeMap<FourCc, Ground>,
        at: (i32, i32),
        minor: FourCc,
        rng: &mut Rng,
    ) {
        let Some(code) = terrain.at(at) else {
            return;
        };
        let Some(ground) = patterns.get(&code).filter(|g| !g.table.is_empty()) else {
            return;
        };
        let Some(major) = ground.table.at(0) else {
            return;
        };

        if !self.settled(at, minor, major) {
            return;
        }

        self.set(at, minor);

        let start = ground.start();
        let mut taken = [false; RING.len()];
        let count = rng.below(NEIGHBOURS - start) + start;

        for _ in 0..count {
            // A neighbour, or the next free one round the ring.
            let mut which = usize::try_from(rng.below(NEIGHBOURS)).unwrap_or(0) % RING.len();

            while taken[which] {
                which = (which + 1) % RING.len();
            }

            taken[which] = true;

            let beside = (at.0 + RING[which].0, at.1 + RING[which].1);

            // Same pattern, and settled itself.
            if terrain.at(beside) == Some(code) && self.settled(beside, minor, major) {
                self.set(beside, minor);
            }
        }
    }

    /// Whether every neighbour holds one of the pattern's two classes.
    ///
    /// **Binary** — surface settled. Off the map does not count against it.
    fn settled(&self, at: (i32, i32), minor: FourCc, major: FourCc) -> bool {
        RING.iter()
            .filter_map(|(dx, dy)| self.at((at.0 + dx, at.1 + dy)))
            .all(|held| held == minor || held == major)
    }

    /// Pass three: a cell whose class is on none of its four orthogonal
    /// neighbours is flipped to the other class of its pair.
    ///
    /// **Binary** — surface chart build; surface has orthogonal tests the
    /// four neighbours. x outer, y inner, no draws. Removes lone speckles and
    /// fills lone holes.
    fn unspeckle(&mut self, terrain: &terrain::Chart, patterns: &BTreeMap<FourCc, Ground>) {
        for x in 0..self.side {
            for y in 0..self.side {
                let at = (x, y);
                let Some(code) = terrain.at(at) else {
                    continue;
                };
                let major = Self::base(patterns, code);
                let Some(ground) = patterns.get(&code).filter(|g| g.table.len() == 2) else {
                    continue;
                };
                let Some(minor) = ground.table.at(1).filter(|id| *id != NONE) else {
                    continue;
                };
                let Some(held) = self.at(at) else {
                    continue;
                };

                let alone = !ORTHOGONAL
                    .iter()
                    .any(|(dx, dy)| self.at((at.0 + dx, at.1 + dy)) == Some(held));

                if alone {
                    self.set(at, if held == major { minor } else { major });
                }
            }
        }
    }

    /// The class a cell holds, or `None` off the map.
    #[must_use]
    pub fn at(&self, at: (i32, i32)) -> Option<FourCc> {
        let (x, y) = at;

        if (0..self.side).contains(&x) && (0..self.side).contains(&y) {
            self.cells
                .get(usize::try_from(y * self.side + x).ok()?)
                .copied()
        } else {
            None
        }
    }

    /// Write a cell. [`crate::sim::mapgen::paint`] lays the ground through it.
    ///
    /// # Panics
    ///
    /// If the cell is off the map.
    pub fn set(&mut self, at: (i32, i32), code: FourCc) {
        let slot = usize::try_from(at.1 * self.side + at.0).expect("a cell on the map");

        self.cells[slot] = code;
    }

    #[must_use]
    pub const fn side(&self) -> i32 {
        self.side
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAJOR: FourCc = FourCc::new(b"GR01");
    const MINOR: FourCc = FourCc::new(b"GR02");
    const OTHER: FourCc = FourCc::new(b"GR09");

    /// A chart of one class, to write speckle into.
    fn flat(side: i32) -> Chart {
        Chart {
            side,
            cells: vec![MAJOR; usize::try_from(side * side).expect("a small side")],
        }
    }

    /// A blotch lands only where the whole ring holds the pattern's classes.
    #[test]
    fn a_blotch_will_not_settle_against_another_biome() {
        let mut chart = flat(5);

        assert!(chart.settled((2, 2), MINOR, MAJOR));

        chart.set((1, 1), OTHER);
        assert!(!chart.settled((2, 2), MINOR, MAJOR));

        // Off the map does not count: a corner is settled.
        assert!(chart.settled((0, 4), MINOR, MAJOR));
    }

    /// The third pass removes a lone speckle, keeps a pair, and writes as it
    /// walks.
    #[test]
    fn the_last_pass_flips_whatever_stands_alone() {
        let side = 5;
        let patterns = BTreeMap::from([(
            FourCc::new(b"#Pla"),
            Ground {
                table: Distribution::new(vec![(MAJOR, 78), (MINOR, 30)]),
                spread: 20,
            },
        )]);
        let terrain = terrain::Chart::of(side, vec![FourCc::new(b"#Pla"); 25]);

        // A lone minority cell flips.
        let mut chart = flat(side);

        chart.set((2, 2), MINOR);
        chart.unspeckle(&terrain, &patterns);
        assert_eq!(chart.at((2, 2)), Some(MAJOR));

        // A pair survives.
        let mut chart = flat(side);

        chart.set((2, 2), MINOR);
        chart.set((2, 3), MINOR);
        chart.unspeckle(&terrain, &patterns);
        assert_eq!(
            (chart.at((2, 2)), chart.at((2, 3))),
            (Some(MINOR), Some(MINOR))
        );

        // A majority cell ringed by minority stays: x outer, so (1, 2) flips
        // back first and (2, 2) is no longer alone.
        let mut chart = flat(side);

        for at in [(2, 1), (3, 2), (2, 3), (1, 2)] {
            chart.set(at, MINOR);
        }

        chart.unspeckle(&terrain, &patterns);
        assert_eq!(chart.at((2, 2)), Some(MAJOR));
    }

    /// Where `k` starts: the only known use of an entry's four unread words.
    #[test]
    fn a_blotch_starts_as_wide_as_the_entry_says() {
        let of = |spread| Ground {
            table: Distribution::default(),
            spread,
        };

        assert_eq!(of(0).start(), 0);
        assert_eq!(of(8).start(), 1);
        assert_eq!(of(10).start(), 2);
        assert_eq!(of(16).start(), 3);
        assert_eq!(of(20).start(), 4);
    }

    /// A start past the ring (modded data) is refused, not looped on: a
    /// blotch would then look for a ninth free neighbour of eight.
    #[test]
    fn a_blotch_wider_than_the_ring_is_refused() {
        let side = 9;
        let code = FourCc::new(b"#Pla");
        let patterns = BTreeMap::from([(
            code,
            Ground {
                table: Distribution::new(vec![(MAJOR, 1), (MINOR, 1000)]),
                spread: 45,
            },
        )]);
        let terrain = terrain::Chart::of(side, vec![code; 81]);

        let built = Chart::build(&terrain, &patterns, &mut Rng::new(1));
        assert!(built.is_err());
    }
}
