//! An effector's value drawn as text: `Got_Gold`'s amount. **Binary**,
//! `IsoUnitIMG`, for a frame of reference kind
//! [`imag::TEXT`](crate::data::imag::TEXT):
//!
//! ```text
//!  view zoom not 1.0         nothing
//!  the part's Animate        none: nothing; else its value
//!  sprintf "%d"              in font SM09, colour 0x80ffffff
//!  box                       w x h, its top left -(w / 2), -(h / 2)
//!                            about the anchor plus the frame's nudge
//! ```
//!
//! **Inferred**: the box's height is the font's rows (the engine's
//! text measure is unread), and the colour's top byte draws nothing: white, opaque.

use crate::data::fourcc::FourCc;

/// The font: `0x39304d53` before text set font.
pub const FONT: FourCc = FourCc::new(b"SM09");

/// `value` as `sprintf("%d")` writes it.
#[must_use]
pub fn text(value: i32) -> String {
    format!("{value}")
}

/// The box's top about the anchor plus the nudge, pixels down: a box
/// `height` rows tall has its top at `-(h / 2)`, truncated.
#[must_use]
pub const fn top(height: i32) -> i32 {
    -(height / 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An odd box's top is half a row past centring.
    #[test]
    fn an_odd_boxs_top_is_half_a_row_on() {
        assert_eq!(top(8), -4);
        assert_eq!(top(9), -4);
        assert_eq!(text(-25), "-25");
    }
}
