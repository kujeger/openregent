//! The unit chart: where inside a placed block each unit stands.
//!
//! **Binary** — unit chart build, the second chart.
//! Walks the force chart's grid column by column; each placed block's record
//! (a set 0 entry of `Freestyle.q`) has its placements put inside it:
//!
//! ```text
//! for x in 0..5:  for y in 0..5:
//!     code = force.at(x, y);  if NONE: nothing
//!     record = the set 0 record that code names
//!     r = its resolution, stepped down to fit the block
//!     grid = the lettering again, at a step of r, centred in the block
//!     for each placement of the record:
//!         free = its letters' cells that are still NONE
//!         one draw where free holds more than one, none otherwise
//! ```
//!
//! Lettering, turn and draw rule are [`crate::sim::mapgen::force`]'s. The turn is copied at
//! Unit chart build.
//!
//! # The grid inside a block
//!
//! A block is `side / 5` cells across. `A` to `Y` sit at a step of `r` cells,
//! spanning `5r`, centred. `r` is the record's resolution
//! (see `resolution`), stepped down where `5r` does not fit: to 5, then 3.
//!
//! # A cell here is a request, not a place
//!
//! **Binary** — place search. Unit chart place
//! creates the agent on the chart cell; `BuildingRec` then floods the
//! half-cell grid to the nearest half cell where its clearing ring is clear,
//! and its call to unit move to in `BuildingRec` writes it back. Units
//! made before it block; scatter does not (`0x1000`), nor ground
//! (half cell blocked's terrain arm cannot fire under `0x1800`).
//!
//! So the residual is the order units are made in, a shuffle on the stream
//! the scatter leaves: two saves with identical charts on two lands place
//! five of six buildings differently. `crate::sim::start` models it, exact on
//! every corpus building.
//!
//! The flood spends no draw.

use crate::data::fourcc::FourCc;
use crate::data::rgs::map::{BLOCKS, Orientation, UnitPattern};
use crate::sim::freestyle::lair_key;

use crate::sim::mapgen::force::Force;
use crate::sim::mapgen::region::NONE;
use crate::sim::mapgen::rng::Rng;

/// The resolution a record falls back to when the block cannot hold its own.
///
/// **Binary** — literals in unit chart build and unit chart build, tried in order.
const FALLBACKS: [i32; 2] = [5, 3];

/// One placed unit's code per cell.
#[derive(Debug, Clone)]
pub struct Units {
    width: i32,
    height: i32,
    cells: Vec<FourCc>,
    /// The template placement whose force block holds each cell: the cell's
    /// second word comes from it.
    sources: Vec<Option<usize>>,
    /// Each cell's lair key, `1000 × template placement + unit`, the unit
    /// its index among its block record's placements; the
    /// engine keeps it, chart word `0x34`, only where the template has a
    /// spawner of that key ([`crate::sim::freestyle::lairs`]).
    keys: Vec<Option<u32>>,
}

impl Units {
    /// Run the chart over a finished force chart.
    ///
    /// `record` is the set 0 lookup for a force-chart code. An unknown code
    /// places nothing, where the engine would fault.
    ///
    /// # Panics
    ///
    /// If the map is larger than an `i32`.
    #[must_use]
    pub fn build<'a>(
        force: &Force,
        (width, height): (u32, u32),
        record: &dyn Fn(FourCc) -> Option<&'a UnitPattern<'a>>,
        rng: &mut Rng,
    ) -> Self {
        let width = i32::try_from(width).expect("a map side");
        let height = i32::try_from(height).expect("a map side");
        let blocks = i32::try_from(BLOCKS).expect("five");

        let cells = usize::try_from(width * height).expect("a map fits in memory");
        let mut chart = Self {
            width,
            height,
            cells: vec![NONE; cells],
            sources: vec![None; cells],
            keys: vec![None; cells],
        };

        let block = (width / blocks, height / blocks);

        // x outer, y inner: unit chart build and unit chart build.
        for x in 0..BLOCKS {
            for y in 0..BLOCKS {
                let code = force.at((x, y));

                let Some(pattern) = (code != NONE).then(|| record(code)).flatten() else {
                    continue;
                };

                let step = resolution(pattern, block.0);
                let at = (i32::try_from(x).unwrap_or(0), i32::try_from(y).unwrap_or(0));
                let origin = (
                    at.0 * block.0 + (block.0 - blocks * step) / 2,
                    at.1 * block.1 + (block.1 - blocks * step) / 2,
                );

                let source = force.source((x, y));

                for (unit, placed) in pattern.placements.iter().enumerate() {
                    let key = source.map(|s| lair_key(s, unit));

                    chart.place(placed, (origin, step), (source, key), force.turn(), rng);
                }
            }
        }

        chart
    }

    /// What a cell holds, or `None` off the map.
    #[must_use]
    pub fn at(&self, (x, y): (i32, i32)) -> Option<FourCc> {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.cells.get(self.slot((x, y))).copied()
        } else {
            None
        }
    }

    /// The template placement whose block a cell lies in, `None` off the
    /// map or where nothing stands.
    #[must_use]
    pub fn source(&self, (x, y): (i32, i32)) -> Option<usize> {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.sources.get(self.slot((x, y))).copied().flatten()
        } else {
            None
        }
    }

    /// Where a cell sits in the row-major array.
    #[allow(
        clippy::cast_sign_loss,
        reason = "every caller has bounded the point against the map first"
    )]
    const fn slot(&self, at: (i32, i32)) -> usize {
        (at.1 * self.width + at.0) as usize
    }

    /// Put one placement on a free cell of its own letters.
    fn place(
        &mut self,
        placed: &crate::data::rgs::map::Placement<'_>,
        (origin, step): ((i32, i32), i32),
        (source, key): (Option<usize>, Option<u32>),
        turn: Orientation,
        rng: &mut Rng,
    ) {
        // block of letter: letter, turn, scale by
        // step, offset into the block.
        let free: Vec<(i32, i32)> = placed
            .blocks()
            .map(|block| turn.turn(block))
            .map(|(bx, by)| {
                (
                    i32::try_from(bx).unwrap_or(0) * step + origin.0,
                    i32::try_from(by).unwrap_or(0) * step + origin.1,
                )
            })
            .filter(|at| self.at(*at) == Some(NONE))
            .collect();

        let taken = match free.len() {
            0 => return,
            1 => 0,
            n => usize::try_from(rng.below(i32::try_from(n).expect("a letter count fits")))
                .expect("a draw below a positive bound"),
        };

        let slot = self.slot(free[taken]);

        self.cells[slot] = placed.id;
        self.sources[slot] = source;
        self.keys[slot] = key;
    }

    /// A cell's lair key, `None` off the map, where nothing stands, or in
    /// a block no template placement owns.
    #[must_use]
    pub fn key(&self, (x, y): (i32, i32)) -> Option<u32> {
        if (0..self.width).contains(&x) && (0..self.height).contains(&y) {
            self.keys.get(self.slot((x, y))).copied().flatten()
        } else {
            None
        }
    }
}

/// A record's resolution, stepped down until `5r` fits across a block.
///
/// **Binary** — unit chart build. The first step applies to 6 or
/// more, the second to 5 or less.
fn resolution(pattern: &UnitPattern<'_>, block: i32) -> i32 {
    // `words[1]`, not `UnitPattern::sizes`. Layout pattern grid
    // reads the word: the word after the ground,
    // i.e. `words[1]`. `sizes` is a later field reading 3 on every freestyle
    // set-0 record, where the engine lays step 5 or 10.
    let mut step = i32::try_from(pattern.words[1]).unwrap_or(FALLBACKS[1]);
    let across = i32::try_from(BLOCKS).expect("five");

    if step >= 6 && block < across * step {
        step = FALLBACKS[0];
    }

    if step <= 5 && block < across * step {
        step = FALLBACKS[1];
    }

    step
}
