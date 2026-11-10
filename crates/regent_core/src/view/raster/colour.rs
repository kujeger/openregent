//! The 16-bit frame's colours: 565 words and 8-bit channels.
//!
//! ```text
//!  word   rrrrrggg gggbbbbb
//!  narrow 8 bits to 5, 6, 5: low bits dropped
//!  widen  replicate: white 255 255 255           art handed on at 8 bits
//!         screen:    white 248 252 248           blit to
//! ```
//!
//! Two widenings, each right for its use:
//!
//! - [`widen_screen`]: what the screen shows of the frame. **Binary**:
//!   the D3D present shifts each channel up, low bits zero
//!   ([`super::lit`]); every capture's channels have their low bits clear.
//!   Use it for a pixel the engine computed on the frame: lit ground, a
//!   shaded pixel, the minimap.
//! - [`widen_replicate`]: bit replication, full scale. **Inferred**: for
//!   art a front end draws at 8 bits (a direct-colour tile); the original
//!   was 16-bit and widened nothing. Narrowing it back gives the word
//!   again ([`super::shade`]'s tables build from it).

use crate::data::payload::Rgb;
use crate::data::tile::Packing;

/// Bits a 565 channel keeps: red and blue.
pub(crate) const FIVE: u16 = 0x1f;

/// Bits green keeps.
pub(crate) const SIX: u16 = 0x3f;

/// Three 8-bit channels to one 565 word, low bits dropped.
///
/// ```
/// use regent_core::data::payload::Rgb;
/// use regent_core::view::raster::colour::narrow;
///
/// assert_eq!(narrow(Rgb { r: 0xff, g: 0xff, b: 0xff }), 0xffff);
/// assert_eq!(narrow(Rgb { r: 0, g: 0, b: 0xdf }), 0x001b);
/// ```
#[must_use]
pub const fn narrow(rgb: Rgb) -> u16 {
    ((rgb.r as u16 >> 3) << 11) | ((rgb.g as u16 >> 2) << 5) | (rgb.b as u16 >> 3)
}

/// A 565 word's three fields: red, green, blue. The one unpacking
/// [`super::lit`] and [`super::shade`] use too.
pub(crate) const fn fields(word: u16) -> [u8; 3] {
    #[allow(clippy::cast_possible_truncation, reason = "masked to six bits")]
    let fields = [
        ((word >> 11) & FIVE) as u8,
        ((word >> 5) & SIX) as u8,
        (word & FIVE) as u8,
    ];

    fields
}

/// A 565 word as the screen shows it: shifted up, low bits zero.
///
/// ```
/// use regent_core::data::payload::Rgb;
/// use regent_core::view::raster::colour::widen_screen;
///
/// assert_eq!(widen_screen(0xffff), Rgb { r: 248, g: 252, b: 248 });
/// ```
#[must_use]
pub const fn widen_screen(word: u16) -> Rgb {
    let [r, g, b] = fields(word);

    Rgb {
        r: r << 3,
        g: g << 2,
        b: b << 3,
    }
}

/// A 565 word to full scale by bit replication: white stays white.
///
/// ```
/// use regent_core::data::payload::Rgb;
/// use regent_core::view::raster::colour::widen_replicate;
///
/// assert_eq!(widen_replicate(0xffff), Rgb { r: 255, g: 255, b: 255 });
/// ```
#[must_use]
pub const fn widen_replicate(word: u16) -> Rgb {
    let [r, g, b] = fields(word);

    Rgb {
        r: (r << 3) | (r >> 2),
        g: (g << 2) | (g >> 4),
        b: (b << 3) | (b >> 2),
    }
}

/// A direct-colour tile's word by [`widen_replicate`]; 555 gives green
/// five bits.
#[must_use]
pub const fn split(word: u16, packing: Packing) -> Rgb {
    match packing {
        Packing::Rgb565 => widen_replicate(word),
        Packing::Rgb555 => Rgb {
            r: widen5(word >> 10),
            g: widen5(word >> 5),
            b: widen5(word),
        },
    }
}

/// Low five bits to eight, full scale.
const fn widen5(channel: u16) -> u8 {
    #[allow(clippy::cast_possible_truncation, reason = "masked to five bits")]
    let five = (channel & FIVE) as u8;

    (five << 3) | (five >> 2)
}

/// An 8-bit colour on the 16-bit frame, as the screen shows it:
/// [`narrow`] then [`widen_screen`]. A palette colour reaches the frame
/// so (palette convert drops the low bits).
#[must_use]
pub const fn quantise(rgb: Rgb) -> Rgb {
    Rgb {
        r: rgb.r & 0xf8,
        g: rgb.g & 0xfc,
        b: rgb.b & 0xf8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every word survives replicate-then-narrow; screen-then-narrow
    /// too, and quantising is the screen's round trip.
    #[test]
    fn both_widenings_narrow_back_to_the_word() {
        for word in 0..=u16::MAX {
            assert_eq!(narrow(widen_replicate(word)), word);
            assert_eq!(narrow(widen_screen(word)), word);
            assert_eq!(quantise(widen_replicate(word)), widen_screen(word));
        }
    }

    #[test]
    fn a_555_word_widens_green_from_five_bits() {
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };

        assert_eq!(split(0x7fff, Packing::Rgb555), white);
        assert_eq!(split(0xffff, Packing::Rgb565), white);
    }
}
