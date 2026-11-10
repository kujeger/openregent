//! The heightfield, and the midpoint displacement that fills it.
//!
//! **Binary** — gen midpoint and its two averaging
//! helpers. The engine's vertices are `0x18` bytes at map; only the
//! height matters here.
//!
//! # The algorithm
//!
//! ```text
//! midpoint(rect, step, range):
//!     if step == 1: return
//!     half = step / 2
//!     for x over rect by step:                    x outer, y inner
//!         for y over rect by step:
//!             if centre (x+half, y+half) unset:
//!                 it = diamond_average + displace(range)
//!     range *= falloff
//!     for x over rect by step:
//!         for y over rect by step:
//!             for each of top, bottom, left, right:
//!                 if unset: it = square_average + displace(range)
//!     range *= falloff
//!     midpoint(rect, half, range)
//! ```
//!
//! Details that must match:
//!
//! * `gen_diamond_average` skips a sentinel corner but divides
//!   by 4, so it reads as zero. `gen_square_average` does not
//!   skip it and divides by the neighbours taken. At the top level, edge
//!   midpoints average corners still at `-1.0`.
//! * The range falls by the profile's multiplier after each pass: `2^-hurst`
//!   per level.
//! * Displacements are whole units: range in thousandths, draw divided by 1000
//!   in integers. All fractions come from averaging.
//! * A set vertex is skipped (unset is [`Field::UNSET`], no mask). One zone's
//!   pass leaves another's alone, and the corners are never written.

use crate::base::fixed::{self, Fixed, ONE};
use crate::sim::mapgen::rng::Rng;
use crate::sim::placement::Heights;

/// A draw is divided by this before becoming [`Fixed`]: ranges are in
/// thousandths.
pub const PER_UNIT: i32 = 1_000;

/// A rectangle of the grid, inclusive of both corners.
///
/// The engine builds `{0, 0, width, height}` in gen heightfield for every zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// A square grid of heights, `side + 1` vertices across.
#[derive(Debug, Clone)]
pub struct Field {
    side: u32,
    heights: Vec<Fixed>,
}

/// A generator field, read the way the engine reads its vertex array.
///
/// For the library's start ([`crate::sim::mapgen::fresh::Made::start`]) and its
/// placement rules.
impl Heights for Field {
    fn across(&self) -> u32 {
        Field::across(self)
    }

    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        self.get(x, y)
    }
}

impl Field {
    /// What an unwritten vertex holds: `-1.0`.
    ///
    /// **Binary** — a compare with `0xffff0000` in gen midpoint;
    /// map fill height fills the array with it before generation. Real ground
    /// never reaches it.
    pub const UNSET: Fixed = fixed::from_whole(-1);

    /// The ceiling a displaced vertex is clamped to: `1000.0`.
    ///
    /// **Binary** — a compare with `0x3e80000` in gen midpoint; the floor is zero.
    /// Shipped ranges are 2 to 10, so it never binds.
    pub const CEILING: Fixed = fixed::from_whole(1_000);

    /// A field of `side` cells a side, every vertex unset.
    #[must_use]
    pub fn new(side: u32) -> Self {
        let across = (side + 1) as usize;

        Field {
            side,
            heights: vec![Self::UNSET; across * across],
        }
    }

    /// Cells a side. Vertices are one more.
    #[must_use]
    pub const fn side(&self) -> u32 {
        self.side
    }

    /// Vertices a side.
    #[must_use]
    pub const fn across(&self) -> u32 {
        self.side + 1
    }

    fn at(&self, x: u32, y: u32) -> Option<usize> {
        (x <= self.side && y <= self.side).then(|| (y * self.across() + x) as usize)
    }

    /// The height at `(x, y)`, or `None` past the edge.
    #[must_use]
    pub fn get(&self, x: u32, y: u32) -> Option<Fixed> {
        self.at(x, y).map(|i| self.heights[i])
    }

    /// Write a height, ignoring a coordinate past the edge.
    pub fn set(&mut self, x: u32, y: u32, h: Fixed) {
        if let Some(i) = self.at(x, y) {
            self.heights[i] = h;
        }
    }

    /// Whether `(x, y)` holds [`Field::UNSET`]. Off the edge counts as set.
    #[must_use]
    pub fn unset(&self, x: u32, y: u32) -> bool {
        self.get(x, y) == Some(Self::UNSET)
    }

    /// Every height in storage order.
    #[must_use]
    pub fn heights(&self) -> &[Fixed] {
        &self.heights
    }
}

/// The range a level displaces by, in thousandths of a unit.
///
/// **Binary** — `fixed_mul(range, 1000.0) >> 16` in gen midpoint.
#[must_use]
pub fn thousandths(range: Fixed) -> i32 {
    fixed::whole(fixed::mul(range, PER_UNIT * ONE))
}

/// One displacement: a draw, divided by 1000, as whole units.
///
/// **Binary** — gen midpoint. Divide, then shift: never a
/// fraction.
pub const fn displace(rng: &mut Rng, range: i32) -> Fixed {
    fixed::from_whole(rng.below(range) / PER_UNIT)
}

/// Clamp a displaced vertex, as the engine does in gen midpoint.
#[must_use]
pub fn clamp(h: Fixed) -> Fixed {
    if h < Fixed::ZERO {
        Fixed::ZERO
    } else if h > Field::CEILING {
        Field::CEILING
    } else {
        h
    }
}

/// The diamond step: a cell centre, from its four corners.
///
/// Skips sentinel corners but always divides by four.
#[must_use]
pub fn diamond_average(field: &Field, x: u32, y: u32, half: u32) -> Fixed {
    let mut sum = Fixed::ZERO;

    for (cx, cy) in [
        (x - half, y - half),
        (x + half, y - half),
        (x - half, y + half),
        (x + half, y + half),
    ] {
        if let Some(h) = field.get(cx, cy)
            && h != Field::UNSET
        {
            sum = sum.wrapping_add(h);
        }
    }

    fixed::div(sum, 4 * ONE)
}

/// The square step: an edge midpoint, from the neighbours it has.
///
/// Takes the sentinel at face value and divides by the neighbours taken. The
/// guards test the midpoint's own coordinate against 0 and the extent:
/// Gen square average.
#[must_use]
pub fn square_average(field: &Field, x: u32, y: u32, half: u32) -> Fixed {
    let side = field.side();
    let mut sum = Fixed::ZERO;
    let mut count: i32 = 0;

    let take = |sum: &mut Fixed, count: &mut i32, cx: u32, cy: u32| {
        if let Some(h) = field.get(cx, cy) {
            *sum = sum.wrapping_add(h);
            *count += 1;
        }
    };

    if x > 0 {
        take(&mut sum, &mut count, x - half, y);
    }
    if x < side {
        take(&mut sum, &mut count, x + half, y);
    }
    if y > 0 {
        take(&mut sum, &mut count, x, y - half);
    }
    if y < side {
        take(&mut sum, &mut count, x, y + half);
    }

    if count == 0 {
        Fixed::ZERO
    } else {
        fixed::div(sum, count * ONE)
    }
}

/// Fill `rect` by midpoint displacement.
///
/// `range` starts at the profile's scale; `falloff` applies after each pass.
/// The step halves down to 1.
pub fn midpoint(
    field: &mut Field,
    rng: &mut Rng,
    rect: Rect,
    step: u32,
    range: Fixed,
    falloff: Fixed,
) {
    if step <= 1 {
        return;
    }

    let half = step / 2;

    // Diamond pass. x outer, y inner, as the engine draws.
    let bound = thousandths(range);
    let mut x = rect.x0;

    while x < rect.x1 {
        let mut y = rect.y0;

        while y < rect.y1 {
            let (cx, cy) = (x + half, y + half);

            if field.unset(cx, cy) {
                let h = diamond_average(field, cx, cy, half) + displace(rng, bound);
                field.set(cx, cy, clamp(h));
            }

            y += step;
        }

        x += step;
    }

    // Square pass, after the first fall. Four edge midpoints a cell, in
    // call-site order.
    let range = fixed::mul(range, falloff);
    let bound = thousandths(range);
    let mut x = rect.x0;

    while x < rect.x1 {
        let mut y = rect.y0;

        while y < rect.y1 {
            for (mx, my) in [
                (x + half, y),        // top, gen midpoint
                (x + half, y + step), // bottom, gen midpoint
                (x, y + half),        // left, gen midpoint
                (x + step, y + half), // right, gen midpoint
            ] {
                if field.unset(mx, my) {
                    let h = square_average(field, mx, my, half) + displace(rng, bound);
                    field.set(mx, my, clamp(h));
                }
            }

            y += step;
        }

        x += step;
    }

    midpoint(field, rng, rect, half, fixed::mul(range, falloff), falloff);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A range of 1.5 units is 1,500 thousandths.
    #[test]
    fn a_range_is_carried_in_thousandths() {
        assert_eq!(thousandths(ONE), 1_000);
        assert_eq!(thousandths(ONE + ONE / 2), 1_500);
        assert_eq!(thousandths(10 * ONE), 10_000);
    }

    /// Whole units, so a range under one unit displaces by nothing.
    #[test]
    fn a_displacement_is_a_whole_number_of_units() {
        let mut rng = Rng::new(1);

        for _ in 0..1_000 {
            assert_eq!(displace(&mut rng, 999), Fixed::ZERO);
        }

        let mut rng = Rng::new(1);
        let seen: std::collections::BTreeSet<_> = (0..10_000)
            .map(|_| fixed::whole(displace(&mut rng, 2_500)))
            .collect();

        assert_eq!(seen, [0, 1, 2].into_iter().collect(), "a range of 2.5");
    }

    /// The clamp at both ends.
    #[test]
    fn a_vertex_is_clamped_to_the_floor_and_the_ceiling() {
        let tiny = Fixed::from_bits(1);

        assert_eq!(clamp(-tiny), Fixed::ZERO);
        assert_eq!(clamp(ONE), ONE);
        assert_eq!(clamp(Field::CEILING + tiny), Field::CEILING);
    }

    /// Three corners at 4.0 and one unset average to 3.0.
    #[test]
    fn the_diamond_divides_by_four_whatever_it_found() {
        let mut field = Field::new(4);

        for (x, y) in [(0, 0), (4, 0), (0, 4)] {
            field.set(x, y, 4 * ONE);
        }

        assert_eq!(field.get(4, 4), Some(Field::UNSET));
        assert_eq!(diamond_average(&field, 2, 2, 2), 3 * ONE);
    }

    /// The square takes the sentinel at face value, and divides by the count.
    #[test]
    fn the_square_averages_the_sentinel_like_any_other_height() {
        let mut field = Field::new(4);

        field.set(0, 0, 3 * ONE);
        field.set(0, 4, 3 * ONE);
        field.set(2, 2, 3 * ONE);

        // Left edge: three neighbours of 3.0.
        assert_eq!(square_average(&field, 0, 2, 2), 3 * ONE);

        // Top edge: (3.0 + -1.0 + 3.0) / 3, with (4, 0) the sentinel.
        assert_eq!(
            square_average(&field, 2, 0, 2),
            fixed::div(5 * ONE, 3 * ONE)
        );
    }

    /// A set vertex is left alone.
    #[test]
    fn the_fractal_does_not_touch_a_vertex_that_is_set() {
        let mut field = Field::new(4);
        let rect = Rect {
            x0: 0,
            y0: 0,
            x1: 4,
            y1: 4,
        };

        field.set(2, 2, 42 * ONE);

        let mut rng = Rng::new(1);
        midpoint(&mut field, &mut rng, rect, 4, 10 * ONE, ONE / 2);

        assert_eq!(field.get(2, 2), Some(42 * ONE));
    }

    /// The fractal never writes the four corners; the rim pass zeroes them
    /// later.
    #[test]
    fn the_corners_survive_the_whole_fractal() {
        let side = 128;
        let mut field = Field::new(side);
        let rect = Rect {
            x0: 0,
            y0: 0,
            x1: side,
            y1: side,
        };

        let mut rng = Rng::new(1);
        midpoint(
            &mut field,
            &mut rng,
            rect,
            side,
            10 * ONE,
            Fixed::from_bits(53_416),
        );

        for (x, y) in [(0, 0), (side, 0), (0, side), (side, side)] {
            assert!(field.unset(x, y), "corner ({x}, {y}) was written");
        }

        let left = (0..=side)
            .flat_map(|y| (0..=side).map(move |x| (x, y)))
            .filter(|(x, y)| field.unset(*x, *y))
            .count();

        assert_eq!(left, 4, "{left} vertices were never written");
    }
}
