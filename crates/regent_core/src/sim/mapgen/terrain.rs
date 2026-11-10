//! The terrain chart, and the base pass of the surface chart after it.
//!
//! **Binary** — terrain chart build, the fourth chart.
//!
//! A zone names a surface pattern, whose entries are terrain patterns, whose
//! entries are `DTPT` classes: two picks deep. Each chart is named for the
//! layer it reads.
//!
//! ```text
//! shuffled_point_list(w, h)              -- w * h draws
//! for each point in that order:
//!     zone = region_chart.at(point)
//!     pattern = the zone's surface pattern
//!     if the zone or the pattern is unknown: leave the cell alone, no draw
//!     else if the pattern has no entries:    write NONE, no draw
//!     else:                                  one draw, write what it picked
//! ```
//!
//! A cell holds a terrain pattern's code.
//!
//! # The base class
//!
//! Surface chart build turns those into classes in three
//! passes, all in [`crate::sim::mapgen::surface`]. The first spends no draws: each cell
//! takes entry 0 of its terrain pattern, or `NONE` if missing or empty.

use std::collections::BTreeMap;

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::region;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::shuffle::shuffled_points;

/// An unwritten cell.
pub use crate::sim::mapgen::region::NONE;

/// One terrain-pattern code per cell.
#[derive(Debug, Clone)]
pub struct Chart {
    side: i32,
    cells: Vec<FourCc>,
}

impl Chart {
    /// Run the chart over a finished region map.
    ///
    /// `surfaces` maps a region code to its surface pattern's table. A missing
    /// code leaves the cell alone; an empty table writes [`NONE`]. Neither
    /// draws.
    ///
    /// # Panics
    ///
    /// If the region map's side does not fit in a `u32`.
    #[must_use]
    pub fn build(
        region: &region::Chart,
        surfaces: &BTreeMap<FourCc, Distribution>,
        rng: &mut Rng,
    ) -> Self {
        let side = region.side();
        let across = u32::try_from(side).expect("a side the region chart was built with");
        let mut cells = vec![NONE; (across as usize) * (across as usize)];

        for (x, y) in shuffled_points(rng, across, across) {
            let at = (x.cast_signed(), y.cast_signed());
            let Some(zone) = region.at(at) else {
                continue;
            };
            let Some(pattern) = surfaces.get(&zone) else {
                continue;
            };

            cells[(y * across + x) as usize] = if pattern.is_empty() {
                NONE
            } else {
                pattern.pick(rng).unwrap_or(NONE)
            };
        }

        Chart { side, cells }
    }

    /// A chart from known cells.
    #[must_use]
    pub const fn of(side: i32, cells: Vec<FourCc>) -> Self {
        Chart { side, cells }
    }

    #[must_use]
    pub const fn side(&self) -> i32 {
        self.side
    }

    /// The terrain pattern a cell drew, or `None` off the map.
    #[must_use]
    pub fn at(&self, at: (i32, i32)) -> Option<FourCc> {
        let (x, y) = at;

        if (0..self.side).contains(&x) && (0..self.side).contains(&y) {
            let slot = y * self.side + x;

            self.cells.get(usize::try_from(slot).ok()?).copied()
        } else {
            None
        }
    }

    #[must_use]
    pub fn cells(&self) -> &[FourCc] {
        &self.cells
    }
}
