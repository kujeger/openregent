//! The engine's random number generator, counting its draws.
//!
//! The formula is [`crate::base::rng`]'s; this adds the seed and a draw
//! count the generator's measurements need.
//!
//! # The stream is a rho
//!
//! The mask drops bit 31, so the transition is not a permutation: two states
//! can map to one. A seed's orbit is a tail into a cycle, both near the
//! `sqrt(pi * 2^31 / 8)` of a random function: tens of thousands of draws
//! ([maps.md](../../../../../docs/formats/maps.md#the-rngs-orbits)).

use crate::base::rng::{self, next as next_state};

/// The engine's generator, seed and state.
///
/// `i32` because the engine's arithmetic is signed: multiply, shift and divide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rng {
    /// The seed. The engine stores it and never reads it.
    pub seed: i32,
    pub state: i32,
    /// How many times [`Rng::step`] has run.
    ///
    /// Not in the engine. A count cannot be recovered from a state (the stream
    /// is a rho). Generators with equal state but different counts compare
    /// unequal.
    pub draws: u32,
}

impl Rng {
    /// Seed it, as rng seed does.
    ///
    /// The engine's seed zero takes the clock; not offered.
    #[must_use]
    pub const fn new(seed: i32) -> Self {
        Rng {
            seed,
            state: seed,
            draws: 0,
        }
    }

    /// Advance, and return the new state.
    ///
    /// Split from [`Rng::below`], which hides the state for bounds of 1 or less.
    pub const fn step(&mut self) -> i32 {
        self.state = next_state(self.state);
        self.draws += 1;

        self.state
    }

    /// A number in `0 .. bound`, and 0 when the masked bound is 1 or less.
    ///
    /// The bound's sign bit is cleared first, as in the engine: `-1` becomes
    /// `0x7fffffff`, `i32::MIN` becomes 0.
    pub const fn below(&mut self, bound: i32) -> i32 {
        let mut state = self.state.cast_unsigned();
        let drawn = rng::draw(&mut state, bound);

        self.state = state.cast_signed();
        self.draws += 1;

        drawn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seeding sets both words.
    #[test]
    fn a_fresh_generator_has_not_moved_yet() {
        let rng = Rng::new(12_345);

        assert_eq!((rng.seed, rng.state), (12_345, 12_345));
    }

    /// [`Rng::below`] masks its bound as the engine does, and a bound of 1
    /// or less still spends a draw.
    #[test]
    fn a_masked_bound_of_one_or_less_yields_zero_and_still_advances() {
        for bound in [i32::MIN, i32::MIN + 1, 0, 1] {
            let mut rng = Rng::new(7);
            let mut twin = Rng::new(7);

            assert_eq!(rng.below(bound), 0, "bound {bound}");
            assert_eq!(rng.state, twin.step(), "bound {bound} did not advance");
        }

        let mut rng = Rng::new(7);
        assert_ne!(rng.below(-1), 0, "-1 masks to 0x7fffffff and divides");
    }
}
