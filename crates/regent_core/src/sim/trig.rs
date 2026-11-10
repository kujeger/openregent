//! The engine's sine and cosine, out of one 256-entry table.
//!
//! **Binary** — sin and cos,
//! read by sin lookup and
//! Cos lookup. Both tables are
//! `trunc(f(i * 2pi / 256) * 65536)`; cosine is sine shifted 64 entries, so
//! only sine is held.
//!
//! A lookup scales a 16.16 angle by [`STEPS_PER_TURN`] and folds the index into
//! `0 ..= 256` (see [`fold`]). Index 256 is past the sine table: the cosine
//! table's first entry.

use crate::base::fixed::{self, Fixed, ONE};

/// Table entries per turn.
pub const TURN: i32 = 256;

/// Sine table entry `index`, `0.. 256`: sin's word.
#[must_use]
pub const fn sine(index: usize) -> Fixed {
    Fixed::from_bits(SINE[index])
}

/// Cosine table entry `index`, `0.. 256`: cos's word, the
/// sine table a quarter turn on.
#[must_use]
pub const fn cosine(index: usize) -> Fixed {
    sine((index + SINE.len() / 4) % SINE.len())
}

/// Pi, as the engine holds it.
///
/// **Binary** — built at start-up at a static initialiser by handing `"3.14"` to
/// `Str2Fixed` in str2fixed, stored at pi. Only reader: the
/// region chart's frontier ring, which falls a hundredth of a radian short of
/// closing.
pub const PI: Fixed = fixed::from_decimal("3.14");

/// Angle-to-index multiplier.
///
/// **Binary** — immediate `0x28be60` in both lookups: `256 / 2pi` in 16.16,
/// with real pi, not [`PI`].
pub const STEPS_PER_TURN: Fixed = Fixed::from_bits(0x0028_be60);

/// `sin(angle)`, the engine's way.
///
/// A full turn folds to index 256; the engine reads past the sine table into
/// the cosine one, whose first word is 1.0. Returned, not refused.
#[must_use]
pub fn sin(angle: Fixed) -> Fixed {
    let at = fold(angle);

    // Index 256: 1.0. The generator never asks for it.
    SINE.get(at).copied().map_or(ONE, Fixed::from_bits)
}

/// `cos(angle)`: the sine table a quarter turn on.
///
/// # Panics
///
/// On exactly a whole turn, where the engine reads past its cosine table. That
/// word is junk (`0x00000001` in `MajestyHD.exe`), so this refuses.
#[must_use]
pub fn cos(angle: Fixed) -> Fixed {
    let at = fold(angle);

    assert!(at < SINE.len(), "a whole turn reads past the cosine table");

    cosine(at)
}

/// Where a 16.16 angle lands in the table, in `0 ..= 256`.
///
/// **Binary** — same fold in both lookups. Subtracts whole turns by shift, not
/// remainder, so an exact positive turn lands on 256.
#[must_use]
#[allow(
    clippy::cast_sign_loss,
    reason = "the fold's own arithmetic puts the result in 0 ..= 256 first"
)]
pub const fn fold(angle: Fixed) -> usize {
    let mut at = fixed::whole(fixed::mul(angle, STEPS_PER_TURN));

    if at > TURN {
        // `(at - 0x101) >> 8 << 8`, then `at += -0x100 - that`.
        at -= TURN + (((at - TURN - 1) >> 8) << 8);
    }

    if at < 0 {
        at += (((-1 - at) >> 8) << 8) + TURN;
    }

    at as usize
}

/// `trunc(sin(i * 2pi / 256) * 65536)`, the table the executable ships:
/// raw 16.16 words.
static SINE: [i32; 256] = [
    0, 1608, 3215, 4821, 6423, 8022, 9616, 11204, 12785, 14359, 15923, 17479, 19024, 20557, 22078,
    23586, 25079, 26557, 28020, 29465, 30893, 32302, 33692, 35061, 36409, 37736, 39039, 40319,
    41575, 42806, 44011, 45189, 46340, 47464, 48558, 49624, 50660, 51665, 52639, 53581, 54491,
    55368, 56212, 57022, 57797, 58538, 59243, 59913, 60547, 61144, 61705, 62228, 62714, 63162,
    63571, 63943, 64276, 64571, 64826, 65043, 65220, 65358, 65457, 65516, 65536, 65516, 65457,
    65358, 65220, 65043, 64826, 64571, 64276, 63943, 63571, 63162, 62714, 62228, 61705, 61144,
    60547, 59913, 59243, 58538, 57797, 57022, 56212, 55368, 54491, 53581, 52639, 51665, 50660,
    49624, 48558, 47464, 46340, 45189, 44011, 42806, 41575, 40319, 39039, 37736, 36409, 35061,
    33692, 32302, 30893, 29465, 28020, 26557, 25079, 23586, 22078, 20557, 19024, 17479, 15923,
    14359, 12785, 11204, 9616, 8022, 6423, 4821, 3215, 1608, 0, -1608, -3215, -4821, -6423, -8022,
    -9616, -11204, -12785, -14359, -15923, -17479, -19024, -20557, -22078, -23586, -25079, -26557,
    -28020, -29465, -30893, -32302, -33692, -35061, -36409, -37736, -39039, -40319, -41575, -42806,
    -44011, -45189, -46340, -47464, -48558, -49624, -50660, -51665, -52639, -53581, -54491, -55368,
    -56212, -57022, -57797, -58538, -59243, -59913, -60547, -61144, -61705, -62228, -62714, -63162,
    -63571, -63943, -64276, -64571, -64826, -65043, -65220, -65358, -65457, -65516, -65536, -65516,
    -65457, -65358, -65220, -65043, -64826, -64571, -64276, -63943, -63571, -63162, -62714, -62228,
    -61705, -61144, -60547, -59913, -59243, -58538, -57797, -57022, -56212, -55368, -54491, -53581,
    -52639, -51665, -50660, -49624, -48558, -47464, -46340, -45189, -44011, -42806, -41575, -40319,
    -39039, -37736, -36409, -35061, -33692, -32302, -30893, -29465, -28020, -26557, -25079, -23586,
    -22078, -20557, -19024, -17479, -15923, -14359, -12785, -11204, -9616, -8022, -6423, -4821,
    -3215, -1608,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A whole turn in 16.16, nearest unit. Not `2 * PI`.
    const TURN_16_16: Fixed = Fixed::from_bits(411_775);

    /// Every entry is reachable, and a lookup is the table at the folded index.
    #[test]
    fn a_lookup_is_the_table_at_the_folded_index() {
        for at in 0..SINE.len() {
            // One unit past exact: the fold truncates.
            let step = i32::try_from(at).expect("an index under 256");
            let angle = fixed::div(step * ONE, STEPS_PER_TURN) + Fixed::from_bits(1);

            assert_eq!(fold(angle), at, "step {at}");
            assert_eq!(sin(angle), sine(at), "sin at step {at}");
            assert_eq!(cos(angle), cosine(at), "cos {at}");
        }
    }

    /// Quarter turns: the table is a sine, its shift a cosine.
    #[test]
    fn the_table_is_a_sine_and_its_quarter_turn_a_cosine() {
        let tiny = Fixed::from_bits(1);
        let quarter = TURN_16_16 / 4 + tiny;
        let zero = Fixed::ZERO;

        assert_eq!((sin(tiny), cos(tiny)), (zero, ONE));
        assert_eq!((sin(quarter), cos(quarter)), (ONE, zero));
        assert_eq!((sin(2 * quarter), cos(2 * quarter)), (zero, -ONE));
        assert_eq!((sin(3 * quarter), cos(3 * quarter)), (-ONE, zero));
    }

    /// An exact turn folds one past the table; `sin` reads what the engine
    /// reads, the cosine table's first word.
    #[test]
    fn a_whole_turn_folds_one_past_the_table() {
        assert_eq!(fold(TURN_16_16), SINE.len());
        assert_eq!(sin(TURN_16_16), ONE);
        assert_eq!(fold(Fixed::ZERO), 0);
    }

    /// The frontier ring never reaches index 256 at any radius, so `cos` may
    /// refuse it.
    #[test]
    fn the_frontier_ring_never_asks_for_a_whole_turn() {
        for radius in 0..512 {
            let count = (radius + 1) * 7;
            let step = fixed::div(2 * PI, count * ONE);

            assert!(
                fold(step * (count - 1)) < SINE.len(),
                "radius {radius} closed the ring"
            );
        }
    }
}
