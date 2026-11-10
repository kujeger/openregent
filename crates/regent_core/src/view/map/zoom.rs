//! The main view's zoom: two levels. **Binary**, `IsoGameView`
//! sets it and reads it:
//!
//! ```text
//!  a new view          1.0
//!  zoom toggle         MainFrameController: 1.0 -> 0.5, anything else -> 1.0
//!  a view restored     0.5, when the save kept it
//! ```

/// The regular zoom: an art pixel a screen pixel (a new view starts there).
pub const REGULAR: f32 = 1.0;

/// Zoomed out: half.
pub const OUT: f32 = 0.5;

/// What zoom toggle makes of zoom `now`.
#[must_use]
pub fn toggled(now: f32) -> f32 {
    #[allow(clippy::float_cmp, reason = "the engine compares with 1.0 exactly")]
    let regular = now == REGULAR;

    if regular { OUT } else { REGULAR }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::float_cmp, reason = "exact binary fractions")]
    fn the_toggle_goes_out_from_regular_and_back_from_anything_else() {
        assert_eq!(toggled(REGULAR), OUT);
        assert_eq!(toggled(OUT), REGULAR);
        assert_eq!(toggled(0.75), REGULAR);
    }
}
