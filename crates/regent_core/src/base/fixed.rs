//! 16.16 fixed point, as the engine holds heights and positions, and as
//! saves and charts store them. `sim::units` has the
//! simulation's units on top of it.
//!
//! # The engine's own rounding
//!
//! **Binary** — fixed mul and fixed div. Both compute in 64 bits and narrow, but round differently,
//! which decides the low bit of most generated vertices:
//!
//! ```text
//! fixed_mul   a 64-bit product, shifted right 16
//!             -> ((a as i64 * b as i64) >> 16) as i32      arithmetic, floors
//!
//! fixed_div   a, widened and shifted left 16, divided by b
//!             -> (((a as i64) << 16) / b as i64) as i32    signed division, toward zero
//! ```
//!
//! They differ only on negatives, which occur: the generator's unset
//! sentinel is `-1.0`, and `square_average` averages it in.

use std::fmt;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// 16.16 fixed point, as the engine holds heights and positions.
///
/// A newtype, as [`crate::base::gfix::Gfix`] is, so neither mixes with
/// plain integers or with the other unseen. Raw words (saves, files,
/// digests) cross with [`Fixed::from_bits`] and [`Fixed::to_bits`]; serde
/// and `Debug` see the bare `i32`.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
#[serde(transparent)]
pub struct Fixed(i32);

impl Fixed {
    /// 0.0.
    pub const ZERO: Fixed = Fixed(0);

    /// The least value.
    pub const MIN: Fixed = Fixed(i32::MIN);

    /// The value whose raw word is `bits`.
    ///
    /// ```
    /// use regent_core::base::fixed::{Fixed, ONE};
    ///
    /// assert_eq!(Fixed::from_bits(0x0001_0000), ONE);
    /// ```
    #[must_use]
    pub const fn from_bits(bits: i32) -> Fixed {
        Fixed(bits)
    }

    /// The raw word.
    #[must_use]
    pub const fn to_bits(self) -> i32 {
        self.0
    }

    /// The magnitude, as [`i32::abs`].
    #[must_use]
    pub const fn abs(self) -> Fixed {
        Fixed(self.0.abs())
    }

    /// The mean of `self` and `other`, floored, as [`i32::midpoint`].
    #[must_use]
    pub const fn midpoint(self, other: Fixed) -> Fixed {
        Fixed(self.0.midpoint(other.0))
    }

    /// `self + other`, wrapping as the engine's 32-bit `add` does.
    #[must_use]
    pub const fn wrapping_add(self, other: Fixed) -> Fixed {
        Fixed(self.0.wrapping_add(other.0))
    }

    /// `self - other`, wrapping as the engine's 32-bit `sub` does.
    #[must_use]
    pub const fn wrapping_sub(self, other: Fixed) -> Fixed {
        Fixed(self.0.wrapping_sub(other.0))
    }
}

/// As the bare word, so logs and test messages read as before.
impl fmt::Debug for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl Add for Fixed {
    type Output = Fixed;

    fn add(self, other: Fixed) -> Fixed {
        Fixed(self.0 + other.0)
    }
}

impl AddAssign for Fixed {
    fn add_assign(&mut self, other: Fixed) {
        self.0 += other.0;
    }
}

impl Sub for Fixed {
    type Output = Fixed;

    fn sub(self, other: Fixed) -> Fixed {
        Fixed(self.0 - other.0)
    }
}

impl SubAssign for Fixed {
    fn sub_assign(&mut self, other: Fixed) {
        self.0 -= other.0;
    }
}

impl Neg for Fixed {
    type Output = Fixed;

    fn neg(self) -> Fixed {
        Fixed(-self.0)
    }
}

/// Scaled by a plain count: `3 * ONE` is 3.0.
impl Mul<Fixed> for i32 {
    type Output = Fixed;

    fn mul(self, f: Fixed) -> Fixed {
        Fixed(self * f.0)
    }
}

/// Scaled by a plain count.
impl Mul<i32> for Fixed {
    type Output = Fixed;

    fn mul(self, n: i32) -> Fixed {
        Fixed(self.0 * n)
    }
}

/// Divided by a plain count, truncating: `ONE / 2` is 0.5.
impl Div<i32> for Fixed {
    type Output = Fixed;

    fn div(self, n: i32) -> Fixed {
        Fixed(self.0 / n)
    }
}

/// The fraction bits of [`Fixed`].
pub const FRACTION_BITS: u32 = 16;

/// 1.0 in 16.16.
pub const ONE: Fixed = Fixed(0x0001_0000);

/// [`ONE`] as floats, for converting to and from: 2^16 is exact in both.
#[allow(clippy::cast_precision_loss, reason = "2^16 is exact")]
pub const ONE_F32: f32 = ONE.0 as f32;
#[allow(clippy::cast_lossless, reason = "`f64::from` is not const")]
pub const ONE_F64: f64 = ONE.0 as f64;

/// 16.16 as an `f32`: exact to 24 bits, so for a map's cells and heights.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a map's 16.16 fits 24 bits")]
pub fn to_f32(f: Fixed) -> f32 {
    f.0 as f32 / ONE_F32
}

/// 16.16 as an `f64`: exact.
#[must_use]
pub fn to_f64(f: Fixed) -> f64 {
    f64::from(f.0) / ONE_F64
}

/// Whole number `n` in 16.16: `<< 16`.
///
/// ```
/// use regent_core::base::fixed::{ONE, from_whole};
///
/// assert_eq!(from_whole(3), 3 * ONE);
/// ```
#[must_use]
pub const fn from_whole(n: i32) -> Fixed {
    Fixed(n << FRACTION_BITS)
}

/// `f`'s whole part, flooring: `>> 16`.
///
/// ```
/// use regent_core::base::fixed::{Fixed, ONE, whole};
///
/// assert_eq!(whole(3 * ONE + Fixed::from_bits(1)), 3);
/// assert_eq!(whole(Fixed::from_bits(-1)), -1);
/// ```
#[must_use]
pub const fn whole(f: Fixed) -> i32 {
    f.0 >> FRACTION_BITS
}

/// `a * b`, flooring — the engine's fixed mul.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the narrowing is the instruction: the engine keeps eax and \
              discards the high word, so wrapping here is the behaviour"
)]
pub const fn mul(a: Fixed, b: Fixed) -> Fixed {
    Fixed((((a.0 as i64) * (b.0 as i64)) >> 16) as i32)
}

/// `a / b`, truncating toward zero — the engine's fixed div.
///
/// # Panics
///
/// On a zero divisor, where a 32-bit signed divide faults.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the narrowing is the instruction: the divide leaves the quotient in \
              eax and faults rather than widening"
)]
pub const fn div(a: Fixed, b: Fixed) -> Fixed {
    Fixed((((a.0 as i64) << 16) / (b.0 as i64)) as i32)
}

/// A decimal string, as the engine's own parser reads it.
///
/// **Binary** — `Str2Fixed` in str2fixed, which parses several engine
/// constants at start-up. Whole part: Horner from the left,
/// `whole = fixed_mul(whole, 10.0) + digit`. Fraction: Horner from the right,
/// `frac = fixed_div(digit + frac, 10.0)`.
///
/// Each fraction step truncates ([`div`] truncates), so the result lands at or
/// below the true value. `"3.14"` gives 205,783, the nearest 16.16;
/// `"0.840896"` gives 55,108, nearest 55,109.
///
/// An illegal character yields 0; the engine also logs
/// `Illegal Character in Str2Fixed`.
#[must_use]
pub const fn from_decimal(text: &str) -> Fixed {
    let bytes = text.as_bytes();
    let mut at = 0;
    let negative = match bytes {
        [b'-', ..] => {
            at = 1;
            true
        }
        [b'+', ..] => {
            at = 1;
            false
        }
        _ => false,
    };

    // Whole part. Raw words: a const fn has no operators.
    let ten = 10 * ONE.0;
    let mut whole = 0;

    while at < bytes.len() && bytes[at] != b'.' {
        if !bytes[at].is_ascii_digit() {
            return Fixed::ZERO;
        }

        whole = mul(Fixed(whole), Fixed(ten)).0 + (bytes[at] - b'0') as i32 * ONE.0;
        at += 1;
    }

    // Fraction, last digit first.
    let mut frac = 0;
    let mut back = bytes.len();

    while back > at + 1 {
        back -= 1;

        if !bytes[back].is_ascii_digit() {
            return Fixed::ZERO;
        }

        frac = div(
            Fixed((bytes[back] - b'0') as i32 * ONE.0 + frac),
            Fixed(ten),
        )
        .0;
    }

    if negative {
        Fixed(-(whole + frac))
    } else {
        Fixed(whole + frac)
    }
}

/// A height as a save stores it, in the engine's fixed point.
///
/// A saved `f32` height is a 16.16 value round-tripped through a float.
/// Rounding recovers it exactly; truncating would not.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "the clamp puts the value inside Fixed first, which is what the \
              lint asks for; `as` on an f64 already in range is exact"
)]
pub fn from_float(height: f32) -> Fixed {
    let scaled = (f64::from(height) * ONE_F64).round();

    if scaled.is_nan() {
        Fixed::ZERO
    } else {
        Fixed(scaled.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raw word.
    const fn bits(n: i32) -> Fixed {
        Fixed::from_bits(n)
    }

    /// `whole` floors, so it inverts `from_whole` and rounds negatives
    /// down, as an arithmetic shift does.
    #[test]
    fn a_whole_part_floors_and_inverts_from_whole() {
        for n in [-7, -1, 0, 1, 255] {
            assert_eq!(whole(from_whole(n)), n);
        }

        assert_eq!(from_whole(1), ONE);
        assert_eq!(whole(ONE - bits(1)), 0);
        assert_eq!(whole(-ONE / 2), -1);
    }

    #[test]
    fn a_multiply_is_the_product_shifted_down() {
        assert_eq!(mul(ONE, ONE), ONE);
        assert_eq!(mul(2 * ONE, 3 * ONE), 6 * ONE);
        assert_eq!(mul(ONE / 2, ONE / 2), ONE / 4);
        assert_eq!(mul(10 * ONE, Fixed::ZERO), Fixed::ZERO);
    }

    /// `mul` floors and `div` truncates, so they cannot share one helper.
    #[test]
    fn the_two_round_negatives_differently() {
        // -1/4 of a unit, times a half. Exact, so both agree.
        assert_eq!(mul(-ONE / 4, ONE / 2), -ONE / 8);

        // Off a boundary: mul floors away from zero, div truncates toward it.
        assert_eq!(mul(bits(-1), ONE / 2), bits(-1));
        assert_eq!(div(bits(-1), 2 * ONE), Fixed::ZERO);

        // The square average's case: a negative sum over a count.
        assert_eq!(div(-3 * ONE, 4 * ONE), -(3 * ONE) / 4);
        assert_eq!(div(-2 * ONE - bits(1), 3 * ONE), bits(-43691));
    }

    /// The parser on plain numbers. The engine's own constants are
    /// [`crate::sim::mapgen`]'s `the_engines_own_parser_reaches_its_own_constants`.
    #[test]
    fn the_engines_parser_reads_plain_numbers() {
        assert_eq!(from_decimal("0.5"), ONE / 2);
        assert_eq!(from_decimal("1.000000"), ONE);
        assert_eq!(from_decimal("-2.5"), -(2 * ONE + ONE / 2));
        assert_eq!(from_decimal("7"), 7 * ONE);
        assert_eq!(from_decimal("3.14"), bits(205_783));
    }

    /// The fraction truncates per digit, so it never lands above the real
    /// number.
    #[test]
    fn a_parsed_fraction_never_lands_above_the_real_number() {
        for text in ["0.840896", "0.996540", "0.1", "0.9999", "3.14", "0.333333"] {
            let parsed = from_decimal(text);
            let parsed = parsed.to_bits();
            let real = text.parse::<f64>().expect("a decimal") * ONE_F64;

            assert!(
                f64::from(parsed) <= real && real - f64::from(parsed) < 2.0,
                "{text} parses to {parsed}, against a real {real}"
            );
        }
    }

    #[test]
    fn a_divide_inverts_a_multiply_on_exact_values() {
        for a in [ONE, 3 * ONE, -7 * ONE, 100 * ONE] {
            for b in [ONE, 2 * ONE, 4 * ONE] {
                assert_eq!(mul(div(a, b), b), a, "{a:?} / {b:?} * {b:?}");
            }
        }
    }
}
