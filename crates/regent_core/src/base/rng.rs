//! The engine's random number generator, as the binary has it.
//!
//! **Binary**, `MajestyHD.exe`. An LCG with a scramble on two
//! words: a seed and a state.
//!
//! ```text
//! rng seed        both words take the seed; 0 takes the clock
//! rng next        state = state * 0x19660d + 0xae135d3
//!                              state = ((state + (state >> 16)) ^ 0x6d25) & 0x7fffffff
//!                              return bound > 1 ? state % bound : 0
//! ```
//!
//! The multiplier is *Numerical Recipes*'; the addend is not.
//!
//! One formula, several streams: the charts' and the world's,
//! GPL's (gpl rng next, [`crate::gpl::vm::Vm::draw`]),
//! each name generator's (`sim::world::naming`), the map generator's
//! (`sim::mapgen::rng`).
//!
//! The particle system's generator in rng next particle (no scramble, inclusive
//! bound) is another: `view::map::particles`.

/// The LCG multiplier, `0x19660d`, as in *Numerical Recipes*.
pub const MULTIPLIER: i32 = 0x0019_660d;

/// The LCG addend, `0xae135d3`. Not *Numerical Recipes*'.
pub const ADDEND: i32 = 0x0ae1_35d3;

/// Applied by xor after folding the state's top half down.
pub const SCRAMBLE: i32 = 0x6d25;

/// Keeps the state non-negative, so the signed remainder acts unsigned.
pub const MASK: i32 = 0x7fff_ffff;

/// The state transition alone.
#[must_use]
pub const fn next(state: i32) -> i32 {
    // The engine's multiply and add wrap.
    let next = state.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND);

    // Fold the top half down, mix, clear the sign.
    (next.wrapping_add(next >> 16) ^ SCRAMBLE) & MASK
}

/// Rng next on `state`: a draw below `bound & 0x7fffffff`, 0 when
/// that is under 2. The state advances either way.
///
/// The bound's sign bit is cleared first, as in the engine: `-1` becomes
/// `0x7fffffff`, `i32::MIN` becomes 0.
///
/// On the GPL stream it is [`crate::gpl::vm::Vm::draw`]; engine code that
/// draws outside a builtin calls this.
pub const fn draw(state: &mut u32, bound: i32) -> i32 {
    let bound = bound & MASK;

    *state = next(state.cast_signed()).cast_unsigned();

    if bound > 1 {
        state.cast_signed() % bound
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state transition, against a hand transcription of rng next.
    #[test]
    fn the_state_transition_is_the_one_the_binary_runs() {
        let step = |state: i32| -> i32 {
            let next = state.wrapping_mul(0x0019_660d).wrapping_add(0x0ae1_35d3);

            (next.wrapping_add(next >> 16) ^ 0x6d25) & 0x7fff_ffff
        };

        let mut state = 1u32;
        let mut want = 1;

        for _ in 0..1_000 {
            want = step(want);
            draw(&mut state, 0);
            assert_eq!(state.cast_signed(), want);
        }
    }

    /// A masked bound of 1 or less yields 0 and still advances the state.
    ///
    /// `i32::MIN` masks to 0, `i32::MIN + 1` to 1; `-1` masks to `0x7fffffff`
    /// and divides. An `abs` would differ.
    #[test]
    fn a_masked_bound_of_one_or_less_yields_zero_and_still_advances() {
        for bound in [i32::MIN, i32::MIN + 1, 0, 1] {
            let mut state = 7;

            assert_eq!(draw(&mut state, bound), 0, "bound {bound}");
            assert_eq!(
                state.cast_signed(),
                next(7),
                "bound {bound} did not advance"
            );
        }

        let mut state = 7;

        assert_ne!(
            draw(&mut state, -1),
            0,
            "-1 masks to 0x7fffffff and divides"
        );
    }

    /// The state stays non-negative.
    #[test]
    fn the_state_stays_non_negative() {
        let mut state = -1;

        for _ in 0..10_000 {
            state = next(state);
            assert!(state >= 0);
        }
    }
}
