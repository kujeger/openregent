//! Fog of war as the iso view draws it. **Binary** unless marked.
//!
//! ```text
//!  ground  IsoGameView,       each cell corner:
//!            explored by the view's mask    shade / 255 + ambient
//!            not                            UNSEEN_SHADE: black
//!          Gouraud over the cell's two triangles, so the edge fades
//!  units   IsoGameView       draws a unit whose seen level is above 0
//!          IsoGenUnit       draws level n of 1 to 99 in palette
//!          (100 − n) / 25, palette 0 at 1 − k / 4 its lightness
//!  clicks  IsoGameView:       a unit at level 0 is no hit
//! ```
//!
//! A unit's level is its sight word for the view's players
//! (`World::seen_level`): 25 a corner of its cell explored, a building's
//! by its rect, its owner's 100. Words come from explored flags alone, and
//! nothing clears a flag, so ground once seen stays clear and every unit on
//! it drawn: the engine has no seen-but-not-now state.

use crate::data::payload::{Palette, Rgb};
use crate::sim::world::reveal::{PER_CORNER, SEEN};

/// The view's mask: `player`'s bit, the local player's.
/// The view keeps its mask; **Inferred**:
/// the local player's bit alone.
#[must_use]
pub const fn mask(player: u32) -> u8 {
    1u8.wrapping_shl(player)
}

/// The shade an unexplored vertex draws at (unseen shade, `0.001f`): so
/// near black that the cell fades to black at that corner.
pub const UNSEEN_SHADE: f32 = 0.001;

/// Palettes a sprite's darkened set holds, its own as palette 0:
/// Prefs registry load sets 4 (`4`).
pub const PALETTES: usize = 4;

/// The darkest palette's lightness, percent: `IsoGenUnit` passes 25.
pub const DARKEST: i32 = 25;

/// Whether the iso view draws a unit at seen level `level`.
#[must_use]
pub const fn drawn(level: i32) -> bool {
    level > 0
}

/// The palette a unit at seen level `level` draws in: `(100 − n) / 25`
/// for n of 1 to 99, else its own, 0.
#[must_use]
pub const fn palette(level: i32) -> usize {
    if level > 0 && level < SEEN {
        ((SEEN - level) / PER_CORNER).unsigned_abs() as usize
    } else {
        0
    }
}

/// How much of its lightness palette `k` keeps: palettes darken steps
/// from [`DARKEST`] percent at the last palette up to palette 1, evenly.
/// With four palettes, 1 − k / 4.
#[must_use]
pub fn lightness(k: usize) -> f64 {
    let last = PALETTES - 1;
    let darkest = f64::from(DARKEST) / 100.0;
    let step = (1.0 - darkest) / f64::from(u8::try_from(last).unwrap_or(1));
    let above = u8::try_from(last.saturating_sub(k)).unwrap_or(0);

    if k == 0 || k > last {
        1.0
    } else {
        darkest + step * f64::from(above)
    }
}

/// Palette `k` of a sprite whose own palette is `own`: each colour's HSL
/// lightness times [`lightness`], channels read over 256 and written back
/// times 256, truncated (palettes darken, rgb to hsl,
/// hsl to rgb). Indices 247 to 254 are shadow operations, so their
/// colours never show.
#[must_use]
pub fn darkened(own: &Palette, k: usize) -> Palette {
    let kept = lightness(k);

    Palette {
        colours: std::array::from_fn(|i| {
            let c = own.colours[i];

            if k == 0 { c } else { darker(c, kept) }
        }),
    }
}

/// One colour with its lightness times `kept`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the engine stores each float as a single"
)]
#[allow(clippy::many_single_char_names, reason = "the colour models' names")]
fn darker(c: Rgb, kept: f64) -> Rgb {
    let unit = |b: u8| (f64::from(b) / 256.0) as f32;
    let (h, s, l) = hsl(unit(c.r), unit(c.g), unit(c.b));
    let (r, g, b) = rgb(h, s, (f64::from(l) * kept) as f32);

    Rgb {
        r: byte(r),
        g: byte(g),
        b: byte(b),
    }
}

/// A channel times 256, truncated, its low byte kept (rounded, then a byte
/// store).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the engine keeps the low byte"
)]
fn byte(v: f32) -> u8 {
    (v * 256.0) as i32 as u8
}

/// Rgb to hsl: hue in degrees, saturation,
/// lightness.
#[allow(clippy::many_single_char_names, reason = "the colour models' names")]
#[allow(clippy::float_cmp, reason = "the engine compares exactly")]
fn hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let most = r.max(g).max(b);
    let least = r.min(g).min(b);
    let l = f32::midpoint(most, least);
    let d = most - least;

    if d == 0.0 {
        (0.0, 0.0, l)
    } else {
        let s = if l > 0.5 {
            d / (2.0 - (most + least))
        } else {
            d / (most + least)
        };
        let sector = if most == r {
            (g - b) / d
        } else if most == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        let h = sector * 60.0;

        (if h < 0.0 { h + 360.0 } else { h }, s, l)
    }
}

/// Hsl to rgb: the six hue sectors, each truncated
/// from hue over 60.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "a sector is 0 to 5"
)]
#[allow(clippy::many_single_char_names, reason = "the colour models' names")]
fn rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let q = if l > 0.5 {
        l + s - l * s
    } else {
        (s + 1.0) * l
    };

    if q == 0.0 {
        (l, l, l)
    } else {
        let p = l + l - q;
        let h = h / 60.0;
        let sector = h as i32;
        let rise = (h - sector as f32) * (q - p);
        let (up, down) = (p + rise, q - rise);

        match sector {
            0 => (q, up, p),
            1 => (down, q, p),
            2 => (p, q, up),
            3 => (p, down, q),
            4 => (up, p, q),
            _ => (q, p, down),
        }
    }
}

/// Each corner of a ground cell drawn at its shade or black: whether
/// vertex flags `flags` hold a player `mask` names (`IsoGameView`,
/// a test of the vertex's `+4` against the mask).
#[must_use]
pub const fn explored(flags: u8, mask: u8) -> bool {
    flags & mask != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The view's mask is the local player's bit: in single play the
    /// crown's, 1.
    #[test]
    fn the_mask_is_the_local_players_bit() {
        use crate::sim::world::{CROWN, new_map::NEUTRAL};

        assert_eq!(mask(CROWN), 1);
        assert_eq!(mask(NEUTRAL), 0x80);
    }

    /// A corner explored adds 25, so a unit seen at one, two or three
    /// corners draws palette 3, 2 or 1; at none it is not drawn, at four
    /// in its own colours.
    #[test]
    fn a_units_seen_level_picks_its_palette() {
        let picks: Vec<(bool, usize)> = [0, 25, 50, 75, 100, 1, 99]
            .iter()
            .map(|n| (drawn(*n), palette(*n)))
            .collect();

        assert_eq!(
            picks,
            [
                (false, 0),
                (true, 3),
                (true, 2),
                (true, 1),
                (true, 0),
                (true, 3),
                (true, 0)
            ]
        );
    }

    #[test]
    #[allow(clippy::float_cmp, reason = "exact binary fractions")]
    fn four_palettes_keep_quarters_of_the_lightness() {
        let kept: Vec<f64> = (0..5).map(lightness).collect();

        assert_eq!(kept, [1.0, 0.75, 0.5, 0.25, 1.0]);
    }

    /// White keeps no hue: lightness 255 / 256 times three quarters,
    /// times 256, is 191.25. A pure red keeps its hue and saturation.
    #[test]
    fn a_darkened_colour_keeps_its_hue() {
        let white = Rgb {
            r: 255,
            g: 255,
            b: 255,
        };
        let red = Rgb { r: 200, g: 0, b: 0 };

        assert_eq!(
            darker(white, lightness(1)),
            Rgb {
                r: 191,
                g: 191,
                b: 191
            }
        );
        assert_eq!(darker(red, lightness(2)), Rgb { r: 100, g: 0, b: 0 });
        assert_eq!(darker(red, lightness(0)), red);
    }

    #[test]
    fn hsl_goes_there_and_back() {
        let colours = [(0.5, 0.25, 0.125), (0.1, 0.6, 0.3), (0.2, 0.3, 0.9)];

        for (r, g, b) in colours {
            let (h, s, l) = hsl(r, g, b);
            let (r2, g2, b2) = rgb(h, s, l);

            assert!(
                (r - r2).abs() + (g - g2).abs() + (b - b2).abs() < 1e-5,
                "{r} {g} {b}"
            );
        }
    }
}
