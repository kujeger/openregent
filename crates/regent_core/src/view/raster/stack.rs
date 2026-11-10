//! Shadows stack: where two overlap, the second darkens the first again.
//!
//! **Binary**: the run blits (mode 7 blit runs band, mode 1 blit runs shadow)
//! read the screen word under each control-band pixel, take it through
//! Shade forward, the ramp and shade inverse, and write it
//! back, sprite by sprite in draw order. Nothing marks a pixel as shaded,
//! so each overlapping shadow applies its ramp to what the last one left.
//! [`over`] is that: [`shade::apply`] once per mark, in turn.
//!
//! **Game**: in `heighty_lowerleft_native.png` the overlap of two trees'
//! shadows is the lit ground through two ramps, one after the other
//! (**Measured**).
//!
//! `regent_draw` draws shadows by factors fitted to this (`regent_draw::soft`);
//! its last drawing of the stack itself is `local/archive/shadow-marks.md`.

use super::shade;
use crate::data::payload::Rgb;

/// `under` with each of `indices` drawn over it in turn, as the screen
/// shows it. Indices outside the band draw nothing.
#[must_use]
pub fn over(indices: &[u8], under: Rgb) -> Rgb {
    indices
        .iter()
        .fold(under, |rgb, &index| shade::apply(index, rgb).unwrap_or(rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second shadow darkens the first: 247 keeps half, twice less.
    #[test]
    fn a_shadow_over_a_shadow_is_darker() {
        let rgb = |v| Rgb {
            r: v,
            g: v + 4,
            b: v,
        };
        let grey = Rgb {
            r: 128,
            g: 128,
            b: 128,
        };
        let once = over(&[247], grey);

        assert_eq!(
            once,
            Rgb {
                r: 56,
                g: 56,
                b: 56
            }
        );
        assert_eq!(over(&[247, 247], grey), rgb(16));
        assert_eq!(over(&[247, 247], grey), over(&[247], once));
    }
}
