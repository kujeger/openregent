//! `RandomCoord`: a half cell drawn from a square about a unit or a
//! point. **Binary** unless marked.
//!
//! ```text
//!  centre   a unit's half cell (the agent's position), or a coord / 16
//!  outer    centre ± max / 16, or the board when max < 0
//!  board    3 .. 2 × cells − 3 each way (the board's extents, less 3)
//!  outer ∩ board, empty: a throw
//!  inner    centre ± min / 16 (a point when max < 0); clipped to outer,
//!           not empty: a ring of four bands ([`Area::Ring`])
//!  n = rng_next(cells), inclusive sides; a square: x = x0 + n mod w,
//!           y = y0 + n / w
//!  answer   (x × 16, y × 16)
//! ```
//!
//! Divisions by 16 truncate toward zero (a signed division).

use super::{Answer, Found, World, agent_ref, int_ref};
use crate::gpl::ops::{self, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::units::PER_HALF;

/// Half cells kept clear of the board's edge on each side: 3
/// in gpl random coord.
pub const EDGE: i32 = 3;

/// A rect of half cells, both ends inside: x0, y0, x1, y1.
type Square = [i32; 4];

/// Rect clip: `a` clipped to `b`; whether anything is left, the
/// ends strictly apart.
fn clip(a: &mut Square, b: &Square) -> bool {
    a[0] = a[0].max(b[0]);
    a[1] = a[1].max(b[1]);
    a[2] = a[2].min(b[2]);
    a[3] = a[3].min(b[3]);
    a[0] < a[2] && a[1] < a[3]
}

/// A square of half-width `r` about `c`.
const fn about(c: (i32, i32), r: i32) -> Square {
    [c.0 - r, c.1 - r, c.0 + r, c.1 + r]
}

/// The half cells a `RandomCoord` draw picks from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Area {
    /// A square: `x = x0 + n mod w`, `y = y0 + n / w`.
    Square(Square),
    /// A square less the inner square clipped to it, as four bands:
    ///
    /// ```text
    ///  X0          X1
    ///  +-----------+ Y0   top     rows Y0 .. y0 − 1, whole width
    ///  |    top    |
    ///  |--+-----+--| y0   left    x X0 .. x0 − 1, rows y0 .. y1
    ///  |le|inner|ri|      right   x x1 + 1 .. X1, rows y0 .. y1
    ///  |--+-----+--| y1   bottom  rows y1 + 1 .. Y1, whole width
    ///  |  bottom   |
    ///  +-----------+ Y1   n walks them in that order, each row-major
    /// ```
    Ring(Square, Square),
}

impl Area {
    /// Half cells in it: the draw's bound.
    fn size(&self) -> i32 {
        let cells = |[x0, y0, x1, y1]: Square| (x1 - x0 + 1) * (y1 - y0 + 1);

        match *self {
            Area::Square(outer) => cells(outer),
            Area::Ring(outer, inner) => cells(outer) - cells(inner),
        }
    }

    /// The half cell draw `n` names.
    fn cell(&self, n: i32) -> (i32, i32) {
        let band = |n: i32, x0: i32, x1: i32, y0: i32| {
            let w = x1 - x0 + 1;

            (x0 + n % w, y0 + n / w)
        };

        match *self {
            Area::Square([x0, y0, x1, _]) => band(n, x0, x1, y0),
            Area::Ring([bx0, by0, bx1, _], [ix0, iy0, ix1, iy1]) => {
                let tall = iy1 - iy0 + 1;
                let top = (iy0 - by0) * (bx1 - bx0 + 1);
                let left = (ix0 - bx0) * tall;
                let right = (bx1 - ix1) * tall;

                if n < top {
                    band(n, bx0, bx1, by0)
                } else if n < top + left {
                    band(n - top, bx0, ix0 - 1, iy0)
                } else if n < top + left + right {
                    band(n - top - left, ix1 + 1, bx1, iy0)
                } else {
                    band(n - top - left - right, bx0, bx1, iy1 + 1)
                }
            }
        }
    }
}

/// `RandomCoord`'s area about half cell `c`: the square of `max / 16`
/// clipped to `board`, less the square of `min / 16` where that clips to
/// something inside it. A `max` below 0 takes the board and leaves the
/// inner square a point (gpl random coord jumps past the min's adds), so no
/// ring. `None` where nothing of the board is left: the engine throws.
fn area(c: (i32, i32), (min, max): (i32, i32), board: Square) -> Option<Area> {
    let (mut outer, mut inner) = if max < 0 {
        (board, about(c, 0))
    } else {
        (about(c, max / PER_HALF), about(c, min / PER_HALF))
    };

    match (clip(&mut outer, &board), clip(&mut inner, &outer)) {
        (false, _) => None,
        (true, false) => Some(Area::Square(outer)),
        (true, true) => Some(Area::Ring(outer, inner)),
    }
}

impl World {
    /// `RandomCoord(agent | coord, max)` or `(…, min, max)`. Unknown where
    /// the unit's place is, or the area is empty (the throw's text is the
    /// first argument's, not built here) or a ring of no cells.
    pub(super) fn random_coord(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let radii = match args.len() {
            3 => Some((0, int_ref(vm, args, 2)?)),
            4 => Some((int_ref(vm, args, 2)?, int_ref(vm, args, 3)?)),
            _ => None,
        };
        let centre = match vm.object(vm.arg(args, 1)?) {
            Value::Coord(x, y) => Some((x / PER_HALF, y / PER_HALF)),
            Value::Agent(_) => match self.found(agent_ref(vm, args, 1)?) {
                Found::Piece(_) => self.spot(agent_ref(vm, args, 1)?),
                Found::Gone => Err(Fault::throw(
                    "RandomCoord() tried to access a non-existent game-object",
                ))?,
                Found::Unsure => None,
            },
            _ => Err(Fault::throw(
                "RandomCoord() was given an incorrect type as the first argument",
            ))?,
        };
        let board = [
            EDGE,
            EDGE,
            self.board.side().0 - EDGE,
            self.board.side().1 - EDGE,
        ];
        let area = centre
            .zip(radii)
            .and_then(|(c, (min, max))| area(c, (min, max), board));

        match area.filter(|a| a.size() > 0) {
            Some(area) => {
                let (x, y) = area.cell(self.native_draw(vm, area.size()));
                let at = Value::Coord(x * PER_HALF, y * PER_HALF);
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &at)?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A max below 0 takes the whole board and leaves the inner square
    /// ungrown (gpl random coord jumps past the min's adds): no ring.
    #[test]
    fn a_negative_max_ignores_the_min() {
        let board = [3, 3, 250, 250];

        assert_eq!(
            area((100, 100), (320, -1), board),
            Some(Area::Square(board))
        );
    }

    /// A 5 × 5 square less its middle 3 × 3: sixteen half cells, top row,
    /// left column, right column, bottom row.
    #[test]
    fn a_ring_is_drawn_band_by_band() {
        let ring = area((2, 2), (16, 32), [0, 0, 250, 250]);
        let cells: Vec<(i32, i32)> = (0..16).filter_map(|n| ring.map(|r| r.cell(n))).collect();

        assert_eq!(ring.map(|r| r.size()), Some(16));
        assert_eq!(
            cells,
            [
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (0, 1),
                (0, 2),
                (0, 3),
                (4, 1),
                (4, 2),
                (4, 3),
                (0, 4),
                (1, 4),
                (2, 4),
                (3, 4),
                (4, 4),
            ]
        );
    }

    /// The inner square clipped to the board's edge: no band left of it.
    #[test]
    fn a_ring_at_the_edge_loses_its_clipped_band() {
        let ring = area((4, 10), (16, 48), [3, 3, 250, 250]);

        assert_eq!(ring, Some(Area::Ring([3, 7, 7, 13], [3, 9, 5, 11])));
        assert_eq!(ring.map(|r| r.size()), Some(5 * 7 - 3 * 3));
        assert_eq!(ring.map(|r| r.cell(10)), Some((6, 9)));
    }

    /// A min of under 16 leaves the inner square a point: the whole square.
    #[test]
    fn a_min_under_a_half_cell_is_no_ring() {
        assert_eq!(
            area((10, 10), (15, 32), [3, 3, 250, 250]),
            Some(Area::Square([8, 8, 12, 12]))
        );
    }
}
