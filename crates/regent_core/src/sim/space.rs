//! Where a unit stands and how far apart two are, as the engine measures.
//!
//! ```text
//!  unit                16.16 cells
//!     >> 11            thirty-seconds of a cell  (GPL coords)
//!     / 16             half cells
//!  pad rect            half cells about that     PRIM +0x14..+0x20
//!  distance            squared gap of two rects
//! ```
//!
//! A held unit measures from its holder's half cell. A mobile's half cell
//! is set as it walks, so it is often not its position's (**Game**: 298 of
//! 624 unheld corpus mobiles differ).

use super::movement::plan::{Visit, flood};
use super::units::{PER_HALF, to_pos32};
use crate::base::fixed::Fixed;
use crate::gpl::vm::native::fixed_sqrt;
use crate::sim::placement::Pad;

/// `DistanceBetweenAgents` scales half cells back to coordinates:
/// `<< 4`.
pub const HALF_SHIFT: u32 = 4;

/// A pad rect: `[x0, y0, x1, y1]` in half cells, half-open.
pub type HalfRect = [i32; 4];

/// A 16.16 position as a GPL coordinate: `LocationOf`'s answer.
///
/// ```
/// use regent_core::sim::space::pos32;
///
/// use regent_core::base::fixed::Fixed;
///
/// assert_eq!(pos32(Fixed::from_bits(0x0013_8000)), 624); // 19.5 cells
/// ```
#[must_use]
pub const fn pos32(fixed: Fixed) -> i32 {
    to_pos32(fixed)
}

/// A 16.16 position's half cell: the engine divides
/// [`pos32`] by 16, toward zero. A building's or a scatter object's half
/// cell is always this (**Game**, every corpus record).
#[must_use]
pub const fn half(fixed: Fixed) -> i32 {
    pos32(fixed) / PER_HALF
}

/// A rect placed at a half cell.
#[must_use]
pub const fn placed(rect: HalfRect, at: (i32, i32)) -> HalfRect {
    [
        rect[0] + at.0,
        rect[1] + at.1,
        rect[2] + at.0,
        rect[3] + at.1,
    ]
}

/// A half cell as a rect: rect gap2 without a second unit.
#[must_use]
pub const fn point(at: (i32, i32)) -> HalfRect {
    [at.0, at.1, at.0 + 1, at.1 + 1]
}

/// The squared gap between two placed rects, 0 when they overlap:
/// Rect gap2, in 32 bits.
///
/// ```
/// use regent_core::sim::space::gap2;
///
/// assert_eq!(gap2([0, 0, 2, 2], [2, 0, 3, 1]), 0); // touching
/// assert_eq!(gap2([0, 0, 2, 2], [5, 6, 6, 7]), 9 + 16);
/// ```
#[must_use]
pub fn gap2(a: HalfRect, b: HalfRect) -> i32 {
    let axis = |a0: i32, a1: i32, b0: i32, b1: i32| {
        if b1 <= a0 {
            b1 - a0
        } else if a1 <= b0 {
            b0 - a1
        } else {
            0
        }
    };
    let dx = axis(a[0], a[2], b[0], b[2]);
    let dy = axis(a[1], a[3], b[1], b[3]);

    dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy))
}

/// Whether footprint `a`'s gap to `b` is within `max` half cells:
/// `OrderFollowUnit`'s test, its square a multiply that wraps.
#[must_use]
pub fn within(a: HalfRect, b: HalfRect, max: i32) -> bool {
    gap2(b, a) <= max.wrapping_mul(max)
}

/// `IsAdjacent`, rect adjacent: the first placed
/// rect grown a half cell each way overlaps the second; where the second
/// has a pad mask, placed at `at`, a covered half cell of it lies in the
/// overlap.
///
/// ```
/// use regent_core::sim::space::adjacent;
///
/// assert!(adjacent([0, 0, 1, 1], [1, 0, 2, 1], None, (1, 0)));
/// assert!(!adjacent([0, 0, 1, 1], [2, 0, 3, 1], None, (2, 0)));
/// ```
#[must_use]
pub fn adjacent(a: HalfRect, b: HalfRect, mask: Option<&Pad>, at: (i32, i32)) -> bool {
    let grown = [a[0] - 1, a[1] - 1, a[2] + 1, a[3] + 1];
    let lo = (grown[0].max(b[0]), grown[1].max(b[1]));
    let hi = (grown[2].min(b[2]), grown[3].min(b[3]));
    let covered = |x: i32, y: i32| {
        mask.is_none_or(|pad| {
            let (dx, dy) = (x - at.0 - pad.origin.0, y - at.1 - pad.origin.1);

            dx >= 0 && dy >= 0 && pad.at(dx.cast_unsigned(), dy.cast_unsigned())
        })
    };

    (lo.1..hi.1).any(|y| (lo.0..hi.0).any(|x| covered(x, y)))
}

/// `DistanceBetweenAgents`: the root of [`gap2`], floored
/// (fixed sqrt with no fraction), in coordinates.
#[must_use]
pub const fn distance(gap2: i32) -> i32 {
    fixed_sqrt(gap2.cast_unsigned(), 0).cast_signed() << HALF_SHIFT
}

/// `ListObjects`' range in half cells, squared: a positive range over 16,
/// toward zero, squared in 32 bits; 0 and below as given.
#[must_use]
pub const fn range2(range: i32) -> i32 {
    if range > 0 {
        let r = range / PER_HALF;

        r.wrapping_mul(r)
    } else {
        range
    }
}

/// Place search run from half cell `from` on a grid
/// of `grid` half cells: pop the nearest half cell; the first one
/// `blocked` passes is the answer, a blocked one pushes its unvisited
/// neighbours on the grid. `None` when the heap empties.
///
/// ```
/// use regent_core::sim::space::search;
///
/// // Everything left of x = 6 is blocked: the nearest free half cell.
/// let found = search((5, 5), (10, 10), |(x, _)| x < 6);
///
/// assert_eq!(found, Some((6, 5)));
/// ```
pub fn search(
    from: (i32, i32),
    grid: (i32, i32),
    blocked: impl Fn((i32, i32)) -> bool,
) -> Option<(i32, i32)> {
    let on_grid = |(x, y): (i32, i32)| (0..grid.0).contains(&x) && (0..grid.1).contains(&y);
    let dist2 = |(x, y): (i32, i32)| (x - from.0).pow(2) + (y - from.1).pow(2);

    // Footprint blocked: off the map is blocked.
    flood(grid, from, from, &dist2, &mut |at| {
        if on_grid(at) && !blocked(at) {
            Visit::Take
        } else {
            Visit::Expand
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A max past 46,340 squares past `i32::MAX` and wraps, as the
    /// original's wrapping multiply.
    #[test]
    fn a_follow_distance_squares_as_32_bits() {
        let (a, b) = ([0, 0, 1, 1], [3, 0, 4, 1]);

        assert!(within(a, b, 2));
        assert!(!within(a, b, 1));
        assert!(!within(a, b, 50_000));
    }
}
