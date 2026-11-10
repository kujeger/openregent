//! GPL's `float`: 22.10 fixed point, `1.0` stored as 1024.
//!
//! Named [`Gfix`] to keep it apart from the simulation's 16.16 fixed point
//! (`sim::movement`, `sim::mapgen`): the two never mix.
//!
//! **Corpus**: `a_float_literal_is_fixed_point_with_a_scale_of_1024`.
//! **Binary**: `GplInteger` widens an integer by `<< 10`,
//! `GplFloat` narrows by `>> 10`; see [`crate::gpl::ops`].
//!
//! ```text
//!   raw word   1536        -512         1
//!   value      1.5         -0.5         1/1024
//! ```

/// A GPL float: the raw 32-bit word. Arithmetic on it is the operators'
/// business ([`crate::gpl::ops`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Gfix(pub i32);

impl Gfix {
    /// Fractional bits.
    pub const FRACTION_BITS: u32 = 10;

    /// 1.0.
    pub const ONE: Gfix = Gfix(1 << Gfix::FRACTION_BITS);

    /// An integer widened, `<< 10` in 32 bits: wraps from 2^21 up.
    ///
    /// ```
    /// use regent_core::base::gfix::Gfix;
    ///
    /// assert_eq!(Gfix::from_int(3), Gfix(3072));
    /// ```
    #[must_use]
    pub const fn from_int(i: i32) -> Gfix {
        Gfix(i.wrapping_shl(Gfix::FRACTION_BITS))
    }

    /// Floored to an integer, `>> 10`.
    ///
    /// ```
    /// use regent_core::base::gfix::Gfix;
    ///
    /// assert_eq!(Gfix(-512).floor(), -1);
    /// ```
    #[must_use]
    pub const fn floor(self) -> i32 {
        self.0 >> Gfix::FRACTION_BITS
    }

    /// The value as a real number, exactly.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        f64::from(self.0) / f64::from(Gfix::ONE.0)
    }
}
