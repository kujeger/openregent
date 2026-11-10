//! The force chart: which block of the five by five grid each force takes.
//!
//! **Binary** — force chart build, the first of the
//! eight charts: the first draw after seeding.
//!
//! ```text
//! turn = rng_next(8)                     -- one draw, the grid's orientation
//! for each placement of the template:
//!     if its first cell is not a letter A to Y: skip it, no draw
//!     free = its letters' blocks that are still NONE
//!     if free is empty:      place nothing, no draw
//!     if free holds one:     take it,       no draw
//!     otherwise:             one draw, rng_next(free.len())
//! ```
//!
//! One draw, plus one per placement with a choice. With [`crate::sim::mapgen::unit`], that
//! is all a scenario hands the generator.
//!
//! # The template
//!
//! Freestyle builds the template. Freestyle setup
//! copies the Player Forces entry into `world` and merges Enemy Forces
//! in: settlement placements, then scenario. Both are set 1 of `Freestyle.q`;
//! each placement names a set 0 block record, whose placements feed the unit
//! chart.

use crate::data::fourcc::FourCc;
use crate::data::rgs::map::{BLOCKS, Orientation, Placement};

use crate::sim::mapgen::region::NONE;
use crate::sim::mapgen::rng::Rng;

/// Blocks in the grid.
const SLOTS: usize = (BLOCKS * BLOCKS) as usize;

/// The lettering a placement's cells are read in, and its bounds.
///
/// **Binary** — compares in force chart build and force chart build, on cell 0 only.
/// `Y` is block 25. Anything else is one of the corpus's 73 bare numbers,
/// skipped.
const LETTERS: std::ops::RangeInclusive<u8> = b'A'..=b'Y';

/// A five by five grid of block codes, and the turn its letters read through.
#[derive(Debug, Clone)]
pub struct Force {
    cells: [FourCc; SLOTS],
    /// The template placement that put each block: the chart's int grid,
    /// which force owner of maps to a player.
    sources: [Option<usize>; SLOTS],
    turn: Orientation,
}

impl Force {
    /// Build the chart from a template's placements.
    ///
    /// Takes the first draw: `rng` must be freshly seeded with the map number.
    ///
    /// # Panics
    ///
    /// Never: `rng_next(8)` cannot return a value outside 0 to 7.
    #[must_use]
    pub fn build(placements: &[Placement<'_>], rng: &mut Rng) -> Self {
        let turn = Orientation(u8::try_from(rng.below(8)).expect("a turn is under eight"));

        let mut chart = Self {
            cells: [NONE; SLOTS],
            sources: [None; SLOTS],
            turn,
        };

        for (index, placed) in placements.iter().enumerate() {
            chart.place(index, placed, rng);
        }

        chart
    }

    /// The grid's turn, shared with [`crate::sim::mapgen::unit`].
    #[must_use]
    pub const fn turn(&self) -> Orientation {
        self.turn
    }

    /// What block `(column, row)` holds, [`NONE`] where nothing was placed.
    #[must_use]
    pub const fn at(&self, block: (u32, u32)) -> FourCc {
        self.cells[Self::slot(block)]
    }

    /// Which template placement put block `(column, row)`.
    #[must_use]
    pub const fn source(&self, block: (u32, u32)) -> Option<usize> {
        self.sources[Self::slot(block)]
    }

    /// Every placed block, in reading order.
    pub fn placed(&self) -> impl Iterator<Item = ((u32, u32), FourCc)> + '_ {
        (0..BLOCKS)
            .flat_map(|y| (0..BLOCKS).map(move |x| (x, y)))
            .map(|at| (at, self.at(at)))
            .filter(|(_, code)| *code != NONE)
    }

    /// Put one placement in a free block of its own letters.
    fn place(&mut self, index: usize, placed: &Placement<'_>, rng: &mut Rng) {
        if !placed.cells.first().is_some_and(|c| LETTERS.contains(c)) {
            return;
        }

        // `Placement::blocks` letters, turned onto the map. The engine does
        // both in force block of letter (block of letter's twin), unscaled.
        let free: Vec<(u32, u32)> = placed
            .blocks()
            .map(|block| self.turn.turn(block))
            .filter(|block| self.at(*block) == NONE)
            .collect();

        let taken = match free.len() {
            0 => return,
            1 => 0,
            n => usize::try_from(rng.below(i32::try_from(n).expect("a letter count fits")))
                .expect("a draw below a positive bound"),
        };

        let slot = Self::slot(free[taken]);

        self.cells[slot] = placed.id;
        self.sources[slot] = Some(index);
    }

    /// Where a block sits in the row-major array.
    const fn slot((x, y): (u32, u32)) -> usize {
        (y * BLOCKS + x) as usize
    }
}
