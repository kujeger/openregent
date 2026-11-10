//! Every point of a grid, in a shuffled order, the draws counted.
//!
//! **Binary** — shuffled point list, which
//! [`crate::sim::start::shuffled_points`] holds. Three of the seven charts
//! open with one: the largest block of draws in generation.

use crate::sim::start;

use crate::sim::mapgen::rng::Rng;

/// Every point of a `rows` by `columns` grid, shuffled on `rng`
/// ([`start::shuffled_points`]): `rows * columns` draws.
///
/// # Panics
///
/// If `rows * columns` exceeds the engine's `i32` bound.
#[must_use]
pub fn shuffled_points(rng: &mut Rng, rows: u32, columns: u32) -> Vec<(u32, u32)> {
    let mut state = rng.state.cast_unsigned();
    let points = start::shuffled_points(&mut state, rows, columns);

    rng.state = state.cast_signed();
    rng.draws += rows * columns;
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shuffle draws exactly one number per point.
    ///
    /// Every later draw count depends on this. Checked at three map sizes and
    /// their seed lattices.
    #[test]
    fn a_shuffle_draws_exactly_one_number_per_point() {
        for side in [128u32, 256, 512] {
            for grid in [side, side / 10] {
                let mut rng = Rng::new(1);
                let mut counter = Rng::new(1);

                let points = shuffled_points(&mut rng, grid, grid);

                for _ in 0..grid * grid {
                    counter.step();
                }

                assert_eq!(points.len(), (grid * grid) as usize, "grid {grid}");
                assert_eq!(rng.state, counter.state, "grid {grid} spent other draws");
            }
        }
    }
}
