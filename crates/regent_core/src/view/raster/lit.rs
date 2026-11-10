//! How the iso view colours a ground pixel, and how the frame reaches
//! the screen. **Binary**, `MajestyHD.exe`, unless marked.
//!
//! ```text
//!  corner   IsoGameView            f32(shade × f32(1/255)) + Ambient
//!  pixel    the shader's Gouraud   16.16 fixed point over the cell's cut
//!  level    ((s − 1) >> 12) & 31   k/16 < s ≤ (k+1)/16 gives k
//!  texel    565, each channel      table[level][c] =
//!                                    min(top, trunc(level / 16 × c + 0.5))
//!  frame    16-bit 565, software   CYSoftwareShader_565_AccurateColor
//!  screen   565 to 8888 by shifts  the D3D present, blit to
//! ```
//!
//! Level ground (shade 237, `Ambient` 0.35) is 1.279, level 20: every
//! channel times 1.25, clamped. So the game is brighter than its art.
//! A shade of 2 or more wraps to dark; no shipped setting reaches it.
//!
//! # Where each step is
//!
//! - `IsoGameView` builds each corner's shade: the
//!   vertex byte times shade over, plus the cell's byte 0 over
//!   255 where it is not 0, else the level's `Ambient`. Byte 0
//!   is zeroed by `Map` and mcel read and never filled from
//!   a save (**Inferred**: no writer found, so always the ambient).
//! - soft triangle picks the shader's slot by the
//!   triangle's flags: textured and shaded with a 16-bit texture is
//!   slot 1, whose inner loop indexes three tables by
//!   `((s − 1) >> 6) & 0x7c0`.
//! - soft shader pick takes the `AccurateColor`
//!   shader: `main` passes 2.
//! - light tables build fills the tables: 32
//!   levels, `level / 16`, each factor 1.0, chopped after adding 0.5.
//! - With `UseD3D` the frame is a 16-bit buffer; upload
//!   blits it into an `A8R8G8B8` texture through
//!   Blit to, low bits zero, and present draws
//!   it 1:1, modulated by white. The captures agree: every channel of
//!   every 1920 × 1200 capture has its low bits clear.

use super::atlas::Sheet;
use super::colour::{FIVE, SIX, fields, narrow, widen_screen};
use crate::content::terrain::grid::{Terrain, Vertex};
use crate::data::payload::Rgb;
use crate::data::settings::AMBIENT;
use crate::view::map::cut::{Cut, gouraud};

/// Shade levels a table holds.
pub const LEVELS: usize = 32;

/// The level that leaves a colour as it is: 16 sixteenths.
pub const UNITY: usize = 16;

/// The vertex byte's scale: shade over, a double holding
/// the `f32` nearest 1/255.
const OVER_255: f32 = 1.0 / 255.0;

/// One in 16.16 fixed point.
const ONE: f32 = crate::base::fixed::ONE_F32;

/// The bits of `s − 1` above a level: one sixteenth in 16.16.
const LEVEL_SHIFT: u32 = 12;

/// A corner's shade as the iso view hands it to the shader: the vertex
/// byte over 255 plus `Ambient`, each step an `f32`.
#[must_use]
pub fn intensity(shade: u8) -> f32 {
    f32::from(shade) * OVER_255 + AMBIENT
}

/// A shade in the shader's 16.16: float to fixed, which chops
/// toward zero.
#[allow(
    clippy::cast_possible_truncation,
    reason = "float to fixed chops; a shade is a few units"
)]
#[must_use]
pub fn fixed(intensity: f32) -> i32 {
    (intensity * ONE) as i32
}

/// The table a 16.16 shade picks: `((s − 1) >> 12) & 31`.
///
/// ```
/// use regent_core::view::raster::lit::{fixed, level, UNITY};
///
/// assert_eq!(level(fixed(1.0)), UNITY - 1);
/// assert_eq!(level(fixed(1.01)), UNITY);
/// ```
#[allow(
    clippy::cast_sign_loss,
    reason = "the mask keeps five bits, as the shader's and does"
)]
#[must_use]
pub const fn level(fixed: i32) -> usize {
    ((fixed.wrapping_sub(1) >> LEVEL_SHIFT) as usize) & (LEVELS - 1)
}

/// One table entry: channel `value` of at most `top` at `level`.
///
/// `level / 16` and its product with the value are each stored as an
/// `f32`, then 0.5 is added and the FPU chops.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a level and a channel are small; the product is positive"
)]
#[must_use]
pub fn channel(level: usize, value: u16, top: u16) -> u16 {
    let factor = level as f32 / UNITY as f32;
    let product = factor * f32::from(value);
    let entry = (f64::from(product) + 0.5) as u16;

    entry.min(top)
}

/// A 565 word lit at `level`, as the shader's three tables do it: a
/// level past them wraps, as the shader's mask keeps five bits.
#[must_use]
pub fn lit(word: u16, level: usize) -> u16 {
    let level = level % LEVELS;
    let [r, g, b] = fields(word).map(u16::from);

    (channel(level, r, FIVE) << 11) | (channel(level, g, SIX) << 5) | channel(level, b, FIVE)
}

/// The shader's tables, built once: [`lit`] without the arithmetic.
pub struct Tables {
    five: [[u16; FIVE as usize + 1]; LEVELS],
    six: [[u16; SIX as usize + 1]; LEVELS],
}

impl Tables {
    /// light tables build's tables.
    #[must_use]
    pub fn new() -> Tables {
        let mut tables = Tables {
            five: [[0; FIVE as usize + 1]; LEVELS],
            six: [[0; SIX as usize + 1]; LEVELS],
        };

        for level in 0..LEVELS {
            for (value, entry) in (0..).zip(tables.five[level].iter_mut()) {
                *entry = channel(level, value, FIVE);
            }

            for (value, entry) in (0..).zip(tables.six[level].iter_mut()) {
                *entry = channel(level, value, SIX);
            }
        }

        tables
    }

    /// A 565 word lit at `level`.
    #[must_use]
    pub fn lit(&self, word: u16, level: usize) -> u16 {
        let (five, six) = (&self.five[level % LEVELS], &self.six[level % LEVELS]);
        let [r, g, b] = fields(word);

        (five[usize::from(r)] << 11) | (six[usize::from(g)] << 5) | five[usize::from(b)]
    }
}

impl Default for Tables {
    fn default() -> Tables {
        Tables::new()
    }
}

/// A 565 word as the screen shows it:
/// [`widen_screen`].
///
/// ```
/// use regent_core::data::payload::Rgb;
///
/// let white = Rgb { r: 248, g: 252, b: 248 };
///
/// assert_eq!(regent_core::view::raster::lit::present(0xffff), white);
/// ```
#[must_use]
pub const fn present(word: u16) -> Rgb {
    widen_screen(word)
}

/// Light a sheet of ground, cells `from` to `to` (exclusive) of
/// `terrain` at `step` pixels a cell, from its vertex shades, as the iso
/// view does: each corner's [`intensity`] Gouraud over the cell's two
/// triangles ([`gouraud`], [`Cut`]), its [`level`]'s table on the 565
/// texel, then [`present`]ed. Where the engine steps per screen pixel,
/// this steps per sheet pixel. A corner off the map is level ground.
#[allow(clippy::cast_precision_loss, reason = "a step is at most 32")]
pub fn ground(sheet: &mut Sheet, terrain: &Terrain<'_>, from: [u32; 2], to: [u32; 2], step: u32) {
    let span = [to[0] - from[0], to[1] - from[1]];
    let step = step as usize;
    let tables = Tables::new();
    let flat = intensity(Vertex::FLAT);

    for dy in 0..span[1] {
        for dx in 0..span[0] {
            let corner = |ox, oy| {
                terrain
                    .vertex(from[0] + dx + ox, from[1] + dy + oy)
                    .map_or(flat, |v| intensity(v.shade))
            };

            let quad = [corner(0, 0), corner(1, 0), corner(0, 1), corner(1, 1)];
            let cut = Cut::of(from[0] + dx, from[1] + dy);

            for py in 0..step {
                // Pixel centres, so corners land on corners.
                let v = (py as f32 + 0.5) / step as f32;

                for px in 0..step {
                    let u = (px as f32 + 0.5) / step as f32;
                    let picked = level(fixed(gouraud(quad, cut, u, v)));

                    let x = dx as usize * step + px;
                    let y = dy as usize * step + py;
                    let at = (y * sheet.width + x) * 4;
                    let texel = &mut sheet.rgba[at..at + 3];
                    let word = narrow(Rgb::from([texel[0], texel[1], texel[2]]));

                    texel.copy_from_slice(&present(tables.lit(word, picked)).channels());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::terrain::grid::Cell;

    /// A two-by-two map with a shade per vertex, in reading order over
    /// the 3x3 grid.
    fn tiny_shades(shades: [u8; 9]) -> (Vec<Vertex>, Vec<Cell>) {
        let vertices = shades
            .iter()
            .map(|shade| Vertex {
                height: 0.0,
                shade: *shade,
                flags: 0x80,
            })
            .collect();
        let cell = Cell {
            pair: 0,
            case: 0,
            variant: 0,
            rotation: 0,
        };

        (vertices, vec![cell; 4])
    }

    /// Shade uses [`Cut`]'s two triangles, not bilinear. A saddle
    /// separates the readings; the probed cells differ in parity.
    #[test]
    fn a_cells_shade_is_gouraud_over_the_cut_its_parity_picks() {
        // A saddle in shade.
        let (vertices, cells) = tiny_shades([255, 0, 255, 0, 255, 0, 255, 0, 255]);
        let terrain = Terrain::typed(&vertices, &cells).unwrap();

        let mut sheet = Sheet {
            width: 64,
            height: 64,
            rgba: vec![255; 64 * 64 * 4],
        };

        ground(&mut sheet, &terrain, [0, 0], [2, 2], 32);

        let red = |x: usize, y: usize| sheet.rgba[(y * sheet.width + x) * 4];

        // White through the level a shade picks: what a pixel should read.
        let white = |shade: f32| present(lit(u16::MAX, level(fixed(shade)))).r;
        let (high, low) = (intensity(255), intensity(0));
        let bilinear = |u: f32, v: f32, corners: [f32; 4]| {
            let [tl, tr, bl, br] = corners;

            (tl * (1.0 - u) + tr * u) * (1.0 - v) + (bl * (1.0 - u) + br * u) * v
        };
        let (u, v) = (23.5 / 32.0, 7.5 / 32.0);

        // Cell (0, 0) is even: main diagonal, pixel in the top-right triangle.
        let even = [high, low, low, high];
        let lit_even = white(gouraud(even, Cut::Main, u, v));

        assert_eq!(red(23, 7), lit_even, "the even cell was not Gouraud shaded");
        assert_ne!(lit_even, white(bilinear(u, v, even)));

        // Cell (1, 0) is odd: the anti-diagonal.
        let (u, v) = (7.5 / 32.0, 7.5 / 32.0);
        let odd = [low, high, high, low];
        let anti = white(gouraud(odd, Cut::Anti, u, v));

        assert_eq!(red(32 + 7, 7), anti, "the odd cell took the wrong cut");
        assert_ne!(anti, white(gouraud(odd, Cut::Main, u, v)));
    }

    /// Level ground: shade 237 and the shipped ambient give level 20,
    /// a quarter brighter.
    #[test]
    fn level_ground_is_a_quarter_brighter() {
        assert_eq!(level(fixed(intensity(237))), 20);
        assert_eq!(channel(20, 10, FIVE), 13);
        assert_eq!(channel(20, 24, FIVE), 30);
        assert_eq!(channel(20, 25, FIVE), 31);
    }

    /// Level 16 is the identity; a shade of 2.0 wraps to black.
    #[test]
    fn unity_keeps_a_colour_and_two_wraps() {
        assert!((0..=0xffff).all(|w| lit(w, UNITY) == w));
        assert_eq!(level(fixed(2.0)), LEVELS - 1);
        assert_eq!(level(fixed(2.01)), 0);
    }

    /// The built tables are the arithmetic.
    #[test]
    fn the_tables_are_the_arithmetic() {
        let tables = Tables::new();

        for level in 0..LEVELS {
            assert!((0..=0xffff).all(|w| tables.lit(w, level) == lit(w, level)));
        }
    }

    /// A level past the tables wraps, as the shader's mask does: both
    /// ways agree there too.
    #[test]
    fn a_level_past_the_tables_wraps_both_ways() {
        let tables = Tables::new();

        for level in LEVELS..2 * LEVELS {
            assert!((0..=0xffff).all(|w| lit(w, level) == tables.lit(w, level)));
        }
    }
}
