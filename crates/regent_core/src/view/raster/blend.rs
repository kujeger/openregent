//! Translucent blits and a unit's draw effects: what draw modes 5 and 8
//! do with a pixel, and what `SetDrawEffects` does to a unit's drawing:
//! a see-through unit, flag 5 in the word's low byte,
//! draws in mode 5.
//! **Binary** unless marked.
//!
//! ```text
//!  draw params       mode 5: level − 1, 0..8, where set
//!                    mode 8: level − 10, where set
//!  blend tables      table L: d -> d × L / 10, L 0..10
//!  mode 5 blit       translucent: out = src + (dst − src) × L / 10
//!  mode 8 blit       glow: the same, L per index (palette glow levels):
//!                    level + ⌊10 (256 − max(r, g, b)) / 256⌋
//!  both              247..255 draw nothing
//!
//!  IsoGenUnit        the unit's draw effects word:
//!    low byte a mode (0xff the tile's), bits 8..23 its level; the top
//!    byte a colour, 1..3, whose tint palette every tile colour
//!    is taken to the nearest of
//!  IsoUnitIMGPartEntry         a particle (part kind 0x1000) whose
//!                              alpha is not 1.0: mode 8 at
//!                              level trunc((1 - alpha) x 9)
//! ```
//!
//! So a mode 8 pixel shows as much as it is bright: a glow. A unit made
//! see-through draws mode 5 at its level, every tile, shadows dropped. A
//! fading particle glows, whatever its tile's mode.

use crate::data::payload::{PALETTE_COLOURS, Palette, Rgb};
use crate::data::tile::Mode;

/// Translucency steps: level `L` keeps `L` tenths of what is under.
pub const STEPS: i32 = 10;

/// The first index a translucent blit does not draw: a compare with `0xf6` in both.
pub const UNDRAWN: usize = 247;

/// Mode 8's level offset: draw params subtracts 10 from a level
/// that is set.
const MODE_8_OFFSET: i32 = 10;

/// What `IsoUnitIMGPartEntry` scales a particle's missing alpha
/// by: the double at `IsoUnitIMG`.
pub const FADE_STEPS: f64 = 9.0;

/// The palettes `SetDrawEffects`' colours 1 to 3 take, `PALT` records in
/// `interfacedata.cam`: `RED `, `GRAY`, `GRN `.
pub const TINTS: [[u8; 4]; 3] = [*b"RED ", *b"GRAY", *b"GRN "];

/// How many of a tint palette's entries a colour may land on: its
/// lookup is built over `0xf7`.
pub const TINT_RANGE: usize = 247;

/// Bits per channel in a tint's lookup: 16 levels, 4,096 cells
/// (palette lookup build with 4).
const CELL_BITS: u32 = 4;

/// The alpha a translucent pixel at level `steps` draws with: what of
/// itself shows, `(10 − L) / 10` of 255; 0 past 10, 255 below 0.
///
/// ```
/// use regent_core::view::raster::blend::alpha;
///
/// assert_eq!(alpha(0), 255);
/// assert_eq!(alpha(6), 102);
/// assert_eq!(alpha(10), 0);
/// ```
#[must_use]
pub const fn alpha(steps: i32) -> u8 {
    let kept = STEPS - clamp(steps);

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "0 to 255"
    )]
    let alpha = (kept * 255 / STEPS) as u8;

    alpha
}

const fn clamp(steps: i32) -> i32 {
    if steps < 0 {
        0
    } else if steps > STEPS {
        STEPS
    } else {
        steps
    }
}

/// Each index's alpha under a tile's draw mode and level, palette
/// `palette`: `None` for a mode that blits opaque (0, 1, 7 and the
/// unread 2).
#[must_use]
pub fn alphas(mode: Mode, level: u16, palette: &Palette) -> Option<[u8; PALETTE_COLOURS]> {
    let level = i32::from(level);

    match mode {
        Mode::Translucent => {
            let steps = if level == 0 {
                0
            } else {
                (level - 1).clamp(0, 8)
            };

            Some(even(steps))
        }
        Mode::Other(8) => {
            let base = if level == 0 { 0 } else { level - MODE_8_OFFSET };

            Some(glow(base, palette))
        }
        _ => None,
    }
}

/// The mode 8 level a particle of alpha `alpha` draws at:
/// `None` at exactly 1.0, the tile's own
/// mode; else `(1 − alpha) × 9`, truncated. Its flag at
/// particle alpha on is a static 1 no code writes, so always.
#[must_use]
pub fn fade(alpha: f32) -> Option<i32> {
    #[allow(clippy::float_cmp, reason = "the engine's fcomp against 1.0")]
    let whole = alpha == 1.0;

    #[allow(clippy::cast_possible_truncation, reason = "fistp under truncation")]
    let steps = ((1.0 - f64::from(alpha)) * FADE_STEPS) as i32;

    (!whole).then_some(steps)
}

/// Each index's alpha in mode 8 at level `steps` as given, palette
/// `palette`: the level plus each colour's [`dim`], 0 to 10.
#[must_use]
pub fn glow(steps: i32, palette: &Palette) -> [u8; PALETTE_COLOURS] {
    drawn(|i| alpha(steps + dim(palette.colours[i])))
}

/// A see-through unit's alphas at transparency `level` (1 to 9): mode 5
/// with the level as given, every tile.
#[must_use]
pub fn see_through(level: u8) -> [u8; PALETTE_COLOURS] {
    even(i32::from(level))
}

/// Every drawn index at `steps`.
fn even(steps: i32) -> [u8; PALETTE_COLOURS] {
    drawn(|_| alpha(steps))
}

/// `of` each drawn index, 0 from [`UNDRAWN`].
fn drawn(of: impl Fn(usize) -> u8) -> [u8; PALETTE_COLOURS] {
    std::array::from_fn(|i| if i < UNDRAWN { of(i) } else { 0 })
}

/// How many steps a colour adds in mode 8: `⌊10 (256 − m) / 256⌋`, `m`
/// its brightest channel. Exact in the engine's floats: `(1 − m / 256) ×
/// −10`, truncated.
///
/// ```
/// use regent_core::data::payload::Rgb;
/// use regent_core::view::raster::blend::dim;
///
/// assert_eq!(dim(Rgb { r: 255, g: 0, b: 0 }), 0);
/// assert_eq!(dim(Rgb { r: 128, g: 64, b: 0 }), 5);
/// assert_eq!(dim(Rgb { r: 0, g: 0, b: 0 }), 10);
/// ```
#[must_use]
pub fn dim(c: Rgb) -> i32 {
    let brightest = i32::from(c.r.max(c.g).max(c.b));

    STEPS * (256 - brightest) / 256
}

/// `palette` drawn tinted by `tint`: each colour the tint's entry its
/// lookup names. **Inferred**: both palettes 8 bits a
/// channel, as HD's are.
#[must_use]
pub fn tinted(palette: &Palette, tint: &Palette) -> Palette {
    let lookup = lookup(tint);

    Palette {
        colours: palette
            .colours
            .map(|c| tint.colours[usize::from(lookup[cell(c)])]),
    }
}

/// The lookup cell of colour `c`: each channel's top four bits.
fn cell(c: Rgb) -> usize {
    let top = |v: u8| usize::from(v >> CELL_BITS);

    (top(c.r) << (2 * CELL_BITS)) | (top(c.g) << CELL_BITS) | top(c.b)
}

/// A tint's lookup: per cell, the entry of the first
/// [`TINT_RANGE`] nearest its centre, `16 k + 8` a channel, by squared
/// distance; the lowest index on a tie.
fn lookup(tint: &Palette) -> Vec<u8> {
    let side = 1usize << CELL_BITS;
    let centre = |k: usize| i32::try_from(k * side + side / 2).unwrap_or(0);

    (0..side * side * side)
        .map(|at| {
            let (r, g, b) = (
                centre(at >> (2 * CELL_BITS)),
                centre((at >> CELL_BITS) % side),
                centre(at % side),
            );
            let far = |c: &Rgb| {
                let d = |a: u8, b: i32| (i32::from(a) - b).pow(2);

                d(c.r, r) + d(c.g, g) + d(c.b, b)
            };

            tint.colours[..TINT_RANGE]
                .iter()
                .enumerate()
                .min_by_key(|(i, c)| (far(c), *i))
                .map_or(0, |(i, _)| u8::try_from(i).unwrap_or(0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette(of: impl Fn(usize) -> Rgb) -> Palette {
        Palette {
            colours: std::array::from_fn(of),
        }
    }

    const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    /// Mode 5 at level 7 keeps six tenths of what is under; a level of 0
    /// draws opaque; 247 up draws nothing.
    #[test]
    fn mode_5_draws_its_level_less_one() {
        let p = palette(|_| BLACK);
        let at = |level| alphas(Mode::Translucent, level, &p).expect("translucent");

        assert_eq!(at(7)[0], alpha(6));
        assert_eq!(at(0)[0], 255);
        assert_eq!(at(7)[UNDRAWN], 0);
        assert_eq!(alphas(Mode::Band, 7, &p), None);
    }

    /// Mode 8 shows a pixel as bright as it is; a set level adds to it
    /// less ten.
    #[test]
    fn mode_8_shows_a_pixel_as_it_is_bright() {
        let grey = |v| Rgb { r: v, g: v, b: v };
        let p = palette(|i| grey(u8::try_from(i).unwrap_or(0)));
        let at = |level| alphas(Mode::Other(8), level, &p).expect("translucent");

        assert_eq!(at(0)[0], 0);
        assert_eq!(at(0)[128], alpha(5));
        assert_eq!(at(12)[128], alpha(7));
        assert_eq!(at(0)[246], alpha(0));
    }

    /// A particle's alpha draws as mode 8 at nine tenths of what it
    /// lacks, truncated; a whole one draws as its tile.
    #[test]
    fn a_particles_alpha_is_mode_8_at_nine_tenths_of_its_lack() {
        assert_eq!(fade(1.0), None);
        assert_eq!(fade(0.0), Some(9));
        assert_eq!(fade(0.5), Some(4));
        assert_eq!(fade(0.99), Some(0));

        let white = palette(|_| Rgb {
            r: 255,
            g: 255,
            b: 255,
        });

        assert_eq!(glow(4, &white)[0], alpha(4));
        assert_eq!(glow(4, &white)[UNDRAWN], 0);
    }

    /// A colour takes the nearest of the tint's first 247 entries to its
    /// cell's centre; the lowest index on a tie; 247 up never.
    #[test]
    fn a_tint_takes_each_colour_to_its_nearest() {
        let red = Rgb { r: 200, g: 0, b: 0 };
        let tint = palette(|i| match i {
            3 | 5 => red,
            250 => Rgb {
                r: 250,
                g: 10,
                b: 10,
            },
            _ => BLACK,
        });
        let source = palette(|i| {
            if i == 1 {
                Rgb { r: 250, g: 0, b: 0 }
            } else {
                BLACK
            }
        });
        let out = tinted(&source, &tint);

        assert_eq!(out.colours[1], red);
        assert_eq!(out.colours[0], BLACK);
        assert_eq!(lookup(&tint)[cell(Rgb { r: 250, g: 0, b: 0 })], 3);
    }
}
