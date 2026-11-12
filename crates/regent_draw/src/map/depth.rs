//! Screen depth: the band objects sort in, between the ground and the
//! marks.

use regent_core::base::fixed;
use regent_core::content::imagery::sort_cells;

use super::Maps;

/// The depth of a part at row `y` nudged `px` screen pixels, `rows`
/// every object's row in rank order, `step` a rank's depth: between the
/// ranks either side of where it sorts. Before the first, never below
/// [`BEYOND`]: over few rows half a step would sink under the ground.
pub(super) fn sorted_at(rows: &[f32], y: f32, px: i32, step: f32) -> f32 {
    let sorts = y + fixed::to_f32(sort_cells(px));
    let rank = rows.partition_point(|r| *r <= sorts);

    #[allow(clippy::cast_precision_loss, reason = "tens of thousands")]
    let rank = rank as f32;

    (FURTHEST + (rank - 0.5) * step).max(BEYOND)
}

/// The depth a thing standing on row `y` draws at: between the rows of
/// [`Maps::placed`] either side of it, as `things` ranked them. openregent's
/// pieces draw at it, and a soft shadow at its ground's ([`crate::lie`]).
#[derive(Clone, Debug)]
pub struct Depth {
    rows: Vec<f32>,
    step: f32,
}

impl Depth {
    /// The share of a rank's step a thing between two rows spans: kept off
    /// both, so it ties with neither.
    const SPAN: (f32, f32) = (0.25, 0.75);

    /// How far past an end of the rows, in cells, a piece gets half the
    /// gap there: maps are a few hundred cells across.
    const REACH: f32 = 64.0;

    /// Over [`Maps::placed`]'s rows: sorted again only when they changed
    /// since the last ask, a compare of each row instead of a sort.
    #[must_use]
    pub fn of(maps: &Maps) -> Depth {
        let rows = maps.placed.iter().map(|p| p.cell.y);
        let Ok(mut held) = maps.depth.lock() else {
            return Depth::new(rows.collect());
        };

        match &*held {
            Some((had, depth))
                if had.len() == maps.placed.len()
                    && had
                        .iter()
                        .zip(rows.clone())
                        .all(|(a, b)| a.to_bits() == b.to_bits()) =>
            {
                depth.clone()
            }
            _ => {
                let had: Vec<f32> = rows.collect();
                let depth = Depth::new(had.clone());

                *held = Some((had, depth.clone()));
                depth
            }
        }
    }

    /// Over the placed rows `rows`, in any order.
    #[must_use]
    pub fn new(mut rows: Vec<f32>) -> Depth {
        rows.sort_by(f32::total_cmp);

        #[allow(clippy::cast_precision_loss, reason = "tens of thousands")]
        let step = (NEAREST - FURTHEST) / rows.len().max(1) as f32;

        Depth { rows, step }
    }

    /// Between the ranks either side, in proportion to where `y` falls
    /// between their rows, so things on one stretch sort by row too. Past
    /// the ends, by row as well, squeezed into the gap there: after the
    /// last row towards [`NEAREST`]; before the first, or with no rows,
    /// between `BEYOND` and [`FURTHEST`]. A fixed spot there would tie
    /// every piece in it, and over few rows sink under the ground.
    #[must_use]
    pub fn at(&self, y: f32) -> f32 {
        let rank = self.rows.partition_point(|r| *r <= y);
        let (low, high) = Depth::SPAN;
        let gap = FURTHEST - BEYOND;

        #[allow(clippy::cast_precision_loss, reason = "tens of thousands")]
        let ranked = |share: f32| FURTHEST + (rank as f32 - 1.0 + share) * self.step;

        match (
            rank.checked_sub(1).map(|r| self.rows[r]),
            self.rows.get(rank),
        ) {
            (Some(before), Some(after)) if after > &before => {
                ranked(low + (high - low) * (y - before) / (after - before))
            }
            (Some(last), None) => ranked(low + (high - low) * Depth::squeeze(y - last)),
            (None, Some(first)) => FURTHEST - gap * Depth::squeeze(first - y),

            // Row 0 mid-gap, north of it further back.
            (None, None) => {
                f32::midpoint(BEYOND, FURTHEST) + gap / 2.0 * y / (y.abs() + Depth::REACH)
            }
            _ => ranked(0.5),
        }
    }

    /// Distance `cells` past an end as a share of the gap there, in
    /// `0.0..1.0`: 0 at the end, a half at [`Depth::REACH`].
    fn squeeze(cells: f32) -> f32 {
        cells / (cells + Depth::REACH)
    }
}

/// How much nearer a thing draws than the one before it in a tie
/// ([`untie`]): eight steps of an f32 in the band, four of the depth
/// buffer over the camera's 2,000 units, so the projection keeps them
/// apart.
pub const TIE: f32 = 1.0 / 2048.0;

/// Depths for things at `drawn`, each with its key: in (depth, key)
/// order, each at least [`TIE`] nearer than the one before. Things on
/// one row get one depth, and the `GreaterEqual` test leaves a tie to
/// the draw order, which Bevy does not fix. A lower key draws behind.
/// Only a run of things within `TIE` of each other moves, by
/// `TIE` a member.
#[must_use]
pub fn untie<K: Ord + Copy>(drawn: &[(f32, K)]) -> Vec<f32> {
    let mut order: Vec<(u32, K, usize)> = drawn
        .iter()
        .enumerate()
        .map(|(i, (z, key))| (ordered(*z), *key, i))
        .collect();
    let mut out = vec![0.0; drawn.len()];

    order.sort_unstable();

    // Each at its own depth, or past the one before.
    order
        .into_iter()
        .fold(f32::NEG_INFINITY, |before, (.., i)| {
            let z = drawn[i].0.max(before + TIE);

            out[i] = z;
            z
        });

    out
}

/// `z`'s bits, ordered as the depths: the sign bit flipped, and a
/// negative's other bits too.
const fn ordered(z: f32) -> u32 {
    let bits = z.to_bits();

    if bits >> 31 == 1 {
        !bits
    } else {
        bits | 1 << 31
    }
}

// Orthographic z is linear in depth, so this range keeps thousands of
// objects from quantising into ties. Gizmos draw at zero, above all.

/// Below every object.
pub(super) const GROUND_DEPTH: f32 = -950.0;

/// Far end of the object depth band.
pub const FURTHEST: f32 = -900.0;

/// The furthest a piece past the far end draws: halfway to the ground.
const BEYOND: f32 = f32::midpoint(GROUND_DEPTH, FURTHEST);

/// Near end of the object depth band.
pub const NEAREST: f32 = -100.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::fixtures::{at, holding};

    /// A thing between two placed rows draws between their ranks, and
    /// two on one stretch by row; past either end, outside them all.
    #[test]
    fn a_piece_draws_between_the_rows_either_side_by_row() {
        let depth = Depth::new(vec![4.0, 1.0, 3.0, 2.0]);
        let step = (NEAREST - FURTHEST) / 4.0;
        let (row_2, row_3) = (FURTHEST + step, FURTHEST + 2.0 * step);

        assert!(depth.at(2.2) > row_2 && depth.at(2.8) < row_3);
        assert!(depth.at(2.2) < depth.at(2.8));
        assert!(depth.at(0.0) < FURTHEST);
        assert!(depth.at(9.0) > FURTHEST + 3.0 * step);
    }

    /// With no placed row, or one, a piece still draws above the ground:
    /// a flat scene has no scatter.
    #[test]
    fn a_piece_draws_above_the_ground_over_few_rows() {
        for depth in [Depth::new(vec![]), Depth::new(vec![3.0])] {
            assert!(depth.at(1.0) > GROUND_DEPTH && depth.at(1.0) < FURTHEST);
        }
    }

    /// With no placed row, pieces keep their row order above the ground
    /// and behind the band: a flat scene must not draw them as a tie.
    #[test]
    fn pieces_keep_row_order_over_no_rows() {
        let depth = Depth::new(vec![]);
        let at = [0.0, 0.5, 1.0, 64.0, 68.0, 500.0].map(|y| depth.at(y));

        assert!(at.is_sorted_by(|a, b| a < b), "{at:?}");
        assert!(at.iter().all(|z| *z > GROUND_DEPTH && *z < FURTHEST));
    }

    /// With one placed row, pieces keep their row order either side of
    /// it: behind it above the ground, in front of it inside the band.
    #[test]
    fn pieces_keep_row_order_over_one_row() {
        let depth = Depth::new(vec![64.0]);
        let before = [0.0, 1.0, 60.0, 63.9].map(|y| depth.at(y));
        let after = [64.0, 64.5, 68.0, 500.0].map(|y| depth.at(y));
        let at = [before, after].concat();

        assert!(at.is_sorted_by(|a, b| a < b), "{at:?}");
        assert!(before.iter().all(|z| *z > GROUND_DEPTH && *z < FURTHEST));
        assert!(after.iter().all(|z| *z > FURTHEST && *z < NEAREST));
    }

    /// A part nudged before the first of few rows still draws above the
    /// ground.
    #[test]
    fn a_nudged_part_draws_above_the_ground_over_few_rows() {
        let z = sorted_at(&[64.0], 64.0, -1000, NEAREST - FURTHEST);

        assert!(z > GROUND_DEPTH && z < FURTHEST, "{z}");
    }

    /// Things at one depth draw in key order, `TIE` apart, the lowest
    /// where it was; a thing further than `TIE` keeps its own depth.
    #[test]
    fn a_tie_is_ordered_by_key() {
        let row = -500.0;
        let out = untie(&[(row, 9), (row + 1.0, 1), (row, 4), (row, 7)]);

        assert_eq!(out, vec![row + 2.0 * TIE, row + 1.0, row, row + TIE]);
    }

    /// Across the band, a tie's steps stay distinct in the depth buffer
    /// (the camera's 2,000 units onto 0..1).
    #[test]
    fn a_tie_stays_distinct_in_the_depth_buffer() {
        let buffer = |z: f32| (z + 1000.0) / 2000.0;

        for z in [GROUND_DEPTH, FURTHEST, -500.0, NEAREST] {
            let next = z + TIE;

            assert!(next > z && buffer(next) > buffer(z), "{z}");
        }
    }

    /// The rows sorted once are the rows now: a change to the placed
    /// things, even in place, sorts them again.
    #[test]
    fn the_depth_follows_the_placed_rows() {
        let mut maps = holding(vec![at(0, 0, 0.0, 3.0), at(0, 1, 0.0, 1.0)]);
        let fresh = |maps: &Maps| Depth::new(maps.placed.iter().map(|p| p.cell.y).collect());

        assert_eq!(Depth::of(&maps).rows, fresh(&maps).rows);

        maps.placed[1].cell.y = 5.0;
        assert_eq!(Depth::of(&maps).rows, vec![3.0, 5.0]);

        maps.placed.push(at(0, 2, 0.0, 4.0));
        assert_eq!(Depth::of(&maps).rows, fresh(&maps).rows);
    }
}
