//! A map's terrain classes, one per vertex.
//!
//! A save stores ground per cell: two classes and a blend case. Terrain lives
//! on vertices ([`docs/formats/terrain.md`](docs/formats/terrain.md)); the cell
//! record derives from a per-vertex field.
//!
//! [`Ground`] inverts that: each cell gives its corners a class via
//! [`Schema::corners`]; cells sharing a vertex should agree.
//!
//! ```text
//!   cell (x, y)              vertices
//!   pair 0x89, case 13   ->  (x,   y  ) = 9   the far class, slot 13 is TL
//!                            (x+1, y  ) = 8
//!                            (x, y+1) = 8
//!                            (x+1, y+1) = 8
//! ```
//!
//! `variant` and `rotation` are re-rolled per game, so cell records cannot be
//! diffed; the vertex field can, and generators work in it.
//!
//! # The record's limit
//!
//! A cell names two classes. Where three meet (a Dirt strip, parent and child)
//! the cells cannot all be right: 744 of the corpus's 982 clashing vertices.
//! The rest are unexplained (**Measured**).

use crate::content::terrain::grid::{Cell, Terrain};
use std::cmp::Reverse;

use crate::content::terrain::schema::{Corners, Join, Schema};

/// A vertex no cell has set.
const UNKNOWN: u8 = 0xff;

/// Case of a single-class cell. Slot 0's mask does not apply.
const UNIFORM_CASE: u8 = 0;

/// Cells sharing one vertex.
const INCIDENT: usize = 4;

/// `MAP_TEXTURE_CLASSES`: class table width. 10 used in the base game, 11 in
/// the expansion (`docs/formats/terrain.md`).
const CLASSES: usize = 16;

/// A cell's corners as `(dx, dy)`, in [`Corners`] bit order.
const CORNERS: [(u32, u32); INCIDENT] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// One terrain class per vertex, solved from cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ground {
    across: u32,
    classes: Vec<u8>,
}

impl Ground {
    /// Solve a save's cells into a per-vertex field.
    ///
    /// Infallible: a clash is a measurement. Majority of incident cells wins;
    /// ties go to the lowest class, so order does not matter.
    #[must_use]
    pub fn solve(terrain: &Terrain<'_>) -> Self {
        Self::vote(terrain.side(), &|x, y| {
            terrain
                .cell(x, y)
                .map(|cell| (cell.near(), cell.far(), cell.case))
        })
    }

    /// Each cell votes a class for its four corners; majority wins.
    pub(crate) fn vote(side: u32, cell_of: &dyn Fn(u32, u32) -> Option<(u8, u8, u8)>) -> Self {
        let across = side + 1;
        let mut votes = vec![[UNKNOWN; INCIDENT]; (across * across) as usize];

        for y in 0..side {
            for x in 0..side {
                let Some((near, far_class, case)) = cell_of(x, y) else {
                    continue;
                };

                // Slot 0 is uniform (`pair` names one class twice); its mask
                // does not apply.
                let far = match Schema::default().corners(usize::from(case)) {
                    Some((mask, _)) if case != 0 => mask,
                    _ => Corners::NONE,
                };

                for (bit, (dx, dy)) in CORNERS.into_iter().enumerate() {
                    let at = ((y + dy) * across + x + dx) as usize;
                    let class = if far.has(Corners(1 << bit)) {
                        far_class
                    } else {
                        near
                    };

                    // First free slot. Rim vertices have fewer cells, so some
                    // slots stay UNKNOWN.
                    if let Some(slot) = votes[at].iter_mut().find(|v| **v == UNKNOWN) {
                        *slot = class;
                    }
                }
            }
        }

        let field = votes.iter().map(|cast| elect(*cast)).collect();

        Ground {
            across,
            classes: field,
        }
    }

    /// Vertices per side: cells + 1.
    #[must_use]
    pub const fn across(&self) -> u32 {
        self.across
    }

    /// A vertex's class.
    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> Option<u8> {
        if x < self.across && y < self.across {
            self.classes.get((y * self.across + x) as usize).copied()
        } else {
            None
        }
    }

    /// Every class, row-major.
    #[must_use]
    pub fn classes(&self) -> &[u8] {
        &self.classes
    }

    /// Cell records drawing this field: inverse of [`Ground::solve`].
    ///
    /// Where three classes meet the two commonest win (the engine has the same
    /// limit; see module docs).
    ///
    /// `variant` and `rotation` stay zero: the engine re-rolls both per game
    /// (two fresh games on one seed differ on 86% of variants), and picking
    /// one needs the tileset.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        let side = self.across - 1;

        (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .map(|(x, y)| {
                let mut corners = [0u8; INCIDENT];

                for (bit, (dx, dy)) in CORNERS.into_iter().enumerate() {
                    corners[bit] = self.at(x + dx, y + dy).unwrap_or(UNKNOWN);
                }

                cell_of(corners)
            })
            .collect()
    }

    /// `(matching vertices, total)`, or `None` if sizes differ.
    ///
    /// The generator's score. Two fresh games on one setup agree on 98.4%,
    /// which caps any model.
    #[must_use]
    pub fn agreement(&self, other: &Ground) -> Option<(usize, usize)> {
        if self.across == other.across {
            let same = self
                .classes
                .iter()
                .zip(&other.classes)
                .filter(|(a, b)| a == b)
                .count();

            Some((same, self.classes.len()))
        } else {
            None
        }
    }
}

/// The record drawing one cell's four corners.
///
/// Low class first: `near` is the smaller on all 734,113 corpus blend cells,
/// so the case arranges the larger.
fn cell_of(corners: [u8; INCIDENT]) -> Cell {
    let mut tally = [0usize; CLASSES];

    for class in corners {
        if let Some(slot) = tally.get_mut(class as usize) {
            *slot += 1;
        }
    }

    let mut ranked: Vec<usize> = (0..CLASSES).filter(|c| tally[*c] > 0).collect();

    ranked.sort_by_key(|c| (Reverse(tally[*c]), *c));

    let class = |at: Option<&usize>| at.and_then(|c| u8::try_from(*c).ok()).unwrap_or(0);
    let near = class(ranked.first());
    let far = ranked.get(1).map_or(near, |c| class(Some(c)));
    let (near, far) = (near.min(far), near.max(far));
    let mask = corners
        .iter()
        .enumerate()
        .filter(|(_, class)| **class == far)
        .fold(Corners::NONE, |m, (bit, _)| m.with(Corners(1 << bit)));

    let case = Schema::default()
        .slot(mask, Join::Apart)
        .and_then(|slot| u8::try_from(slot).ok())
        .unwrap_or(UNIFORM_CASE);

    Cell {
        pair: (near << 4) | far,
        case,
        variant: 0,
        rotation: 0,
    }
}

/// Majority class, ties to the lowest.
fn elect(cast: [u8; INCIDENT]) -> u8 {
    let mut tally = [0u8; CLASSES];

    // `UNKNOWN` is past the table end, so unset corners drop out.
    for class in cast {
        if let Some(slot) = tally.get_mut(class as usize) {
            *slot += 1;
        }
    }

    tally
        .into_iter()
        .enumerate()
        .filter(|(_, votes)| *votes > 0)
        .max_by_key(|(class, votes)| (*votes, Reverse(*class)))
        .map_or(UNKNOWN, |(class, _)| u8::try_from(class).unwrap_or(UNKNOWN))
}
