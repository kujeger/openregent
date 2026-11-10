//! The landscape chart: which entry of a zone's scatter each cell drew.
//!
//! **Binary** — landscape chart build, the sixth chart.
//! Shaped like the terrain chart, with two differences:
//!
//! ```text
//! shuffled_point_list(w, h)              -- w * h draws
//! for each point in that order:
//!     zone = region_chart.at(point)
//!     pattern = the zone's landscape pattern, at zone
//!     if the zone or the pattern is unknown: no draw, and the cell keeps
//!                                            the last pick that worked
//!     else:                                  one draw, whatever the table holds
//! ```
//!
//! * It stores the entry's index, not its code:
//!   Distribution pick index
//!   ([`Distribution::pick_index`]).
//! * An empty table still draws; the terrain chart's does not.
//!
//! # A cell that resolves nothing keeps the last one that did
//!
//! Landscape chart build writes the pick to a local; landscape chart build writes the local to
//! the cell whether or not the pick ran. Here the local starts at 0; the
//! engine's is stack garbage, never reached on shipped lands (all their region
//! codes resolve).
//!
//! # What it is for
//!
//! Mobject place looks the index up in the zone's landscape pattern to
//! get an object pattern to scatter from; clump size is that entry's
//! `rest[0]`.
//!
//! # A save does not hold it
//!
//! Its draw count is fixed per cell, so draw-count sweeps cannot see it. Pinned
//! by inversion: a `GRND` record names an object code, a landscape entry an
//! object pattern, that pattern the codes it may place. Where only one entry
//! can make a saved code, the index at that clump's seed is fixed; a
//! one-object clump's seed is the object's cell.
//!
//! Reproduces exactly. `landscape_oracle`: 16,229 pinned cells over 60 saves,
//! all right (chance: 1,992).
//!
//! Pins need the table in file order. Pattern read's entry loop at
//! Pattern read appends each entry unfiltered and unsorted, so file order is
//! index order.

use std::collections::BTreeMap;

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::region;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::shuffle::shuffled_points;

/// One entry index per cell.
#[derive(Debug, Clone)]
pub struct Chart {
    side: i32,
    cells: Vec<usize>,
}

impl Chart {
    /// Run the chart over a finished region map.
    ///
    /// `scatters` maps a region code to its zone's landscape pattern table. A
    /// missing code: no draw, and the cell keeps the last pick.
    ///
    /// # Panics
    ///
    /// If the region chart's side does not fit in a `u32`.
    #[must_use]
    pub fn build(
        region: &region::Chart,
        scatters: &BTreeMap<FourCc, Distribution>,
        rng: &mut Rng,
    ) -> Self {
        let side = region.side();
        let across = u32::try_from(side).expect("a side the region chart was built with");

        let mut chart = Chart {
            side,
            cells: vec![0; across as usize * across as usize],
        };
        let mut last = 0;

        for (x, y) in shuffled_points(rng, across, across) {
            let at = (x.cast_signed(), y.cast_signed());

            if let Some(table) = region.at(at).and_then(|code| scatters.get(&code)) {
                last = table.pick_index(rng);
            }

            chart.cells[(y * across + x) as usize] = last;
        }

        chart
    }

    /// What a cell drew, or `None` off the map.
    #[must_use]
    pub fn at(&self, at: (i32, i32)) -> Option<usize> {
        let (x, y) = at;

        if (0..self.side).contains(&x) && (0..self.side).contains(&y) {
            self.cells
                .get(usize::try_from(y * self.side + x).ok()?)
                .copied()
        } else {
            None
        }
    }
}
