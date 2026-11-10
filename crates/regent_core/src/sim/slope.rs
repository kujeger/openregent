//! The slope clamp: no two neighbouring vertices further apart than
//! `tan(MaxSlope)`.
//!
//! **Binary** — `Map`, reached through `IsoMap`.
//! Run by the map generator over the whole map
//! and by a building's flatten
//! from each vertex it writes.
//!
//! ```text
//!  queue the start vertex, marked
//!  pop the front; walk its 3x3, x outer, y inner, itself included:
//!    a neighbour more than tan above or below: pulled to exactly tan
//!    unmarked, and moved (or Spread::Everywhere): marked, queued
//!  clear the marks
//! ```
//!
//! Each vertex is queued once per run: one wavefront, not a fixed point.

use std::collections::VecDeque;

use crate::base::fixed::Fixed;

/// `tan(MaxSlope)` as the engine holds it, `0xf3ba` in 16.16: the steepest
/// step the clamp leaves between neighbours.
///
/// **Binary**, two ways. Set max slope computes it
/// into `camera`, read by `IsoMap`; the rim
/// marks a generated map leaves read `-3142 = -0x10000 + 0xf3ba`. Equals
/// [`crate::content::terrain::grid::Terrain::RIM_STEP`].
///
/// Not `tan(45°)`: [`crate::data::settings::MAX_SLOPE`] goes through a
/// coarse sine table to `tan(43.59°)` = 0.9520569
/// (`crate::sim::mapgen::rim::tan_of`).
pub const MAX_SLOPE_TAN: Fixed = Fixed::from_bits(0xf3ba);

/// How far the clamp carries — `Map`'s fifth argument, tested at
/// `Map`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spread {
    /// Flag set: every unmarked neighbour is queued; one run covers the
    /// grid. Gen heightfield passes it.
    Everywhere,
    /// Flag clear: only moved neighbours are queued. The generator's rim
    /// loop and building flatten.
    WhereItMoved,
}

/// Vertex heights the clamp reads and writes.
pub trait Slope {
    /// The height at a vertex, or `None` past the edge.
    fn height(&self, x: u32, y: u32) -> Option<Fixed>;

    /// Write a vertex on the grid.
    fn set(&mut self, x: u32, y: u32, height: Fixed);
}

/// The clamp's queue and marks, reusable across runs.
///
/// **Binary** — queue: the global vector at slope queue begin;
/// mark: bit `0x40000000` of a vertex's flags, cleared at the end of each
/// run.
pub struct Flood {
    across: u32,
    seen: Vec<bool>,
    marked: Vec<usize>,
    queue: VecDeque<(u32, u32)>,
}

impl Flood {
    /// A clamp for a grid `across` vertices a side.
    #[must_use]
    pub fn new(across: u32) -> Self {
        Flood {
            across,
            seen: vec![false; (across * across) as usize],
            marked: Vec::new(),
            queue: VecDeque::new(),
        }
    }

    /// Clamp outward from `(x, y)`, pulling every neighbour to within
    /// `tan`.
    pub fn run(&mut self, ground: &mut dyn Slope, (x, y): (u32, u32), tan: Fixed, spread: Spread) {
        self.queue.clear();
        self.mark(x, y);
        self.queue.push_back((x, y));

        while let Some((cx, cy)) = self.queue.pop_front() {
            // Includes the popped vertex itself, which never moves.
            for nx in cx.saturating_sub(1)..=cx + 1 {
                for ny in cy.saturating_sub(1)..=cy + 1 {
                    let (Some(here), Some(there)) = (ground.height(cx, cy), ground.height(nx, ny))
                    else {
                        continue;
                    };

                    let moved = if there > here.wrapping_add(tan) {
                        ground.set(nx, ny, here.wrapping_add(tan));
                        true
                    } else if there < here.wrapping_sub(tan) {
                        ground.set(nx, ny, here.wrapping_sub(tan));
                        true
                    } else {
                        false
                    };

                    if !self.saw(nx, ny) && (spread == Spread::Everywhere || moved) {
                        self.mark(nx, ny);
                        self.queue.push_back((nx, ny));
                    }
                }
            }
        }

        for i in self.marked.drain(..) {
            self.seen[i] = false;
        }
    }

    fn saw(&self, x: u32, y: u32) -> bool {
        self.seen
            .get((y * self.across + x) as usize)
            .copied()
            .unwrap_or(true)
    }

    fn mark(&mut self, x: u32, y: u32) {
        let i = (y * self.across + x) as usize;

        if let Some(seen) = self.seen.get_mut(i) {
            *seen = true;
            self.marked.push(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::content::terrain::grid::Terrain;

    /// The clamp's step equals the save rim step.
    #[test]
    #[allow(
        clippy::float_cmp,
        reason = "both sides are one number out of one binary"
    )]
    fn the_clamp_steps_by_the_slope_the_rim_was_pinned_with() {
        assert_eq!(
            f64::from(MAX_SLOPE_TAN.to_bits()) / f64::from(Terrain::FIXED),
            f64::from(Terrain::RIM_STEP)
        );
    }
}
