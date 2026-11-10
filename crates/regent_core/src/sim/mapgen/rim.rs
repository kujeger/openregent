//! Slope clamp after the fractal, then the rim pass.
//!
//! **Binary** — gen heightfield, and
//! `Map` reached through `IsoMap`.
//!
//! The clamp is `crate::sim::slope`. Order:
//!
//! 1. Flood the clamp from `(0, 0)` with [`Spread::Everywhere`], covering the
//!    map. `(0, 0)` still holds the sentinel.
//! 2. For every `x`: zero `(x, 0)` and clamp from it, then zero `(x, side)`
//!    and clamp from it.
//!
//! Step 1 leaves `-0.0479431` (`-1.0 + tan`) at `(0, 1)` and `(1, 1)` in every
//! save: the clamp pulls `(0, 0)`'s neighbours to within
//! [`crate::sim::slope::MAX_SLOPE_TAN`] of
//! the sentinel. Step 2 overwrites `(1, 0)`; row 1 is not touched again.
//!
//! Only the two rows are pinned, not the columns, as in the corpus. With
//! step 1's flag, no generated vertex breaks the limit, as in every save.

use crate::sim::slope::{Flood, Slope, Spread};

use crate::base::fixed::{self, Fixed, ONE};
use crate::sim::mapgen::field::Field;
use crate::sim::trig;

/// Pi as set max slope holds it, `0x3243f` = 3.1415863, an immediate at
/// set max slope. Distinct from `trig::PI`, the region chart's `3.14`.
const DEGREES_PI: Fixed = Fixed::from_bits(0x0003_243f);

/// A half turn in degrees, 16.16 — the immediate in set max slope.
const HALF_TURN: Fixed = fixed::from_whole(180);

/// The tangent the camera stores for a slope limit in degrees.
///
/// **Binary** — set max slope. Degrees to radians via
/// `DEGREES_PI` and `HALF_TURN`, table sine and cosine, one divide.
///
/// `MaxSlope` is 45 (max slope; only writer
/// `GEINIFile` case `0x23`, which no shipped `GOPT` carries). 45° maps
/// to table index 2,097,116 in 16.16, just under 32.0, truncated to 31:
/// `tan(43.59°)`, [`crate::sim::slope::MAX_SLOPE_TAN`].
///
/// # Panics
///
/// As `trig::cos`, on an angle of a whole turn.
#[must_use]
pub fn tan_of(degrees: i32) -> Fixed {
    let radians = fixed::div(fixed::mul(degrees * ONE, DEGREES_PI), HALF_TURN);

    fixed::div(trig::sin(radians), trig::cos(radians))
}

/// A generator field, as the slope clamp reads and writes it.
impl Slope for Field {
    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        self.get(x, y)
    }

    fn set(&mut self, x: u32, y: u32, height: Fixed) {
        Field::set(self, x, y, height);
    }
}

/// Clamp the whole map and pin the rim, in the engine's order.
///
/// Run after the fractal and after every zone, never before.
pub fn pin(field: &mut Field, tan: Fixed) {
    let side = field.side();
    let mut flood = Flood::new(field.across());

    // Whole map from the sentinel corner. Leaves the two marks.
    flood.run(field, (0, 0), tan, Spread::Everywhere);

    for x in 0..=side {
        field.set(x, 0, Fixed::ZERO);
        flood.run(field, (x, 0), tan, Spread::WhereItMoved);

        field.set(x, side, Fixed::ZERO);
        flood.run(field, (x, side), tan, Spread::WhereItMoved);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::ONE;
    use crate::sim::mapgen::field::Rect;
    use crate::sim::mapgen::rng::Rng;
    use crate::sim::slope::MAX_SLOPE_TAN;

    /// `MaxSlope` is 45 and its tangent comes from a coarse table, not 1.0.
    ///
    /// **Binary.** 45° and 44° reach index 31; 46° reaches 32, exactly 1.0.
    #[test]
    fn the_slope_limit_is_forty_five_degrees_off_a_coarse_table() {
        #[allow(clippy::cast_possible_truncation, reason = "a whole number of degrees")]
        let degrees = crate::data::settings::MAX_SLOPE as i32;

        assert_eq!(tan_of(degrees), MAX_SLOPE_TAN);
        assert_ne!(tan_of(degrees), ONE);
        assert_eq!(tan_of(44), MAX_SLOPE_TAN);
        assert_eq!(tan_of(46), ONE);
    }

    /// The rim mark's raw word: `-1.0 + tan`.
    const MARK: Fixed = Fixed::from_bits(-3142);

    /// The mark is `-1.0 + tan`, one unit below `tan`.
    #[test]
    fn the_rim_mark_is_the_sentinel_plus_the_slope() {
        assert_eq!(Field::UNSET + MAX_SLOPE_TAN, MARK);
        assert_eq!(MAX_SLOPE_TAN - (Field::UNSET + MAX_SLOPE_TAN), ONE);
    }

    /// The rim pass leaves exactly two marks, at `(0, 1)` and `(1, 1)`, as in
    /// the corpus.
    #[test]
    fn the_corner_clamp_leaves_two_marks_in_row_one() {
        let side = 8;
        let mut field = Field::new(side);

        // Real fractal ground, not an empty grid.
        let mut rng = Rng::new(1);
        crate::sim::mapgen::field::midpoint(
            &mut field,
            &mut rng,
            Rect {
                x0: 0,
                y0: 0,
                x1: side,
                y1: side,
            },
            side,
            10 * ONE,
            Fixed::from_bits(53_416),
        );

        pin(&mut field, MAX_SLOPE_TAN);

        assert_eq!(field.get(0, 1), Some(MARK), "(0, 1)");
        assert_eq!(field.get(1, 1), Some(MARK), "(1, 1)");

        let marks = (0..=side)
            .flat_map(|y| (0..=side).map(move |x| (x, y)))
            .filter(|(x, y)| field.get(*x, *y) == Some(MARK))
            .count();

        assert_eq!(marks, 2, "the mark is not those two vertices alone");
    }

    /// A field with a cliff in it, far from `(0, 0)`.
    fn cliff(side: u32) -> Field {
        let mut field = Field::new(side);

        for y in 0..=side {
            for x in 0..=side {
                field.set(x, y, if x > side / 2 { 8 * ONE } else { Fixed::ZERO });
            }
        }

        field
    }

    /// How many vertices stand further than `tan` from a neighbour.
    fn over(field: &Field, tan: Fixed) -> usize {
        let side = field.side();

        (0..=side)
            .flat_map(|y| (0..=side).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let here = field.get(x, y).expect("a vertex");

                (-1i64..=1)
                    .flat_map(|dy| (-1i64..=1).map(move |dx| (dx, dy)))
                    .filter_map(|(dx, dy)| {
                        let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);

                        u32::try_from(nx).ok().zip(u32::try_from(ny).ok())
                    })
                    .filter_map(|(nx, ny)| field.get(nx, ny))
                    .any(|there| (there - here).abs() > tan)
            })
            .count()
    }

    /// Only the flag-set fill crosses unmoved ground to reach a far cliff.
    #[test]
    fn only_the_forced_fill_crosses_ground_it_does_not_move() {
        let side = 32;
        let mut stopped = cliff(side);
        let mut carried = cliff(side);

        assert!(over(&stopped, MAX_SLOPE_TAN) > 0, "the cliff is not steep");

        Flood::new(stopped.across()).run(&mut stopped, (0, 0), MAX_SLOPE_TAN, Spread::WhereItMoved);
        Flood::new(carried.across()).run(&mut carried, (0, 0), MAX_SLOPE_TAN, Spread::Everywhere);

        assert!(
            over(&stopped, MAX_SLOPE_TAN) > 0,
            "the unforced fill reached a cliff it never touched"
        );
        assert_eq!(
            over(&carried, MAX_SLOPE_TAN),
            0,
            "the forced fill left the map over the limit"
        );
    }

    /// Rows are pinned; columns are not.
    #[test]
    fn only_the_two_rows_are_pinned() {
        let side = 8;
        let mut field = Field::new(side);
        let mut rng = Rng::new(3);

        crate::sim::mapgen::field::midpoint(
            &mut field,
            &mut rng,
            Rect {
                x0: 0,
                y0: 0,
                x1: side,
                y1: side,
            },
            side,
            10 * ONE,
            Fixed::from_bits(53_416),
        );
        pin(&mut field, MAX_SLOPE_TAN);

        for x in 0..=side {
            assert_eq!(field.get(x, 0), Some(Fixed::ZERO), "row 0 at {x}");
            assert_eq!(field.get(x, side), Some(Fixed::ZERO), "row {side} at {x}");
        }

        let column_flat = |x: u32| {
            (0..=side)
                .all(|y| field.get(x, y) == Some(Fixed::ZERO) || field.get(x, y) == Some(MARK))
        };

        assert!(
            !column_flat(0) || !column_flat(side),
            "a column is pinned too"
        );
    }
}
