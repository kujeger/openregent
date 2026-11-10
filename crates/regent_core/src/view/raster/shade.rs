//! In a world sprite, palette indices 247 to 254 are an operation on the screen.
//! Interface art uses them as colours; [`Blit`] picks the reading.
//!
//! **Binary.** The blitter branches before the palette and rewrites the
//! destination pixel:
//!
//! ```text
//! ax = dest                          ; RGB565
//! ax = forward[ax]                   ; 64K entries -> H:5 S:5 V:6
//! v  = ax & 0x3f
//! ax = (ax & 0xffc0) | ramp[n][v]    ; only the value moves
//! ax = inverse[ax]                   ; 64K entries -> RGB565
//! ```
//!
//! Draw order decides what a shadow lands on.
//!
//! `n` is `(index + 1) & 7`: 247 is ramp 0, 254 ramp 7. [`BAND`] is the eight
//! ramps, [`RAMPS`] their argument pairs. See `docs/formats/art.md`.
//!
//! **Game**: selection ring and level-up star are bright, so [`BAND`]'s upper
//! half is [`Shade::Raise`].
//!
//! # The two tables
//!
//! Built at video init over every 16-bit colour. HD has two builders;
//! `main` passes 0 to shade tables init, which takes
//! builder 1, hsv tables build: hue, saturation and
//! value, each channel over 256 (the 565 word shifted up). Builder 2,
//! Hls tables build (hue, lightness, saturation over 31 and 63, the
//! textbook HLS), is never picked. **Game**: a shadow on snow is
//! grey, as value scaling keeps it; HLS turns it purple. Over
//! `heighty_lowerleft_native.png` the HSV tables explain 100,221 shadow
//! pixels exactly, HLS 35,378 (**Measured**).
//!
//! The blit is two lookups and a byte substitution. [`forward`],
//! [`inverse`] and [`ramps`] build them, for GPU use.
//! `the_two_tables_compose_into_the_operation_they_came_from` checks all
//! 524,288 answers against [`Shade::over`].

use super::colour::{narrow, widen_screen};
use crate::data::imag::Blit;
use crate::data::payload::Rgb;
use crate::data::tile::Mode;

/// First index the blitter treats as an operation.
pub const CONTROL_FIRST: u8 = 247;

/// Index the blitter skips.
///
/// **Inferred**, hardcoded: no blit takes a colour key. The evidence, blits
/// comparing sprite indices only with immediates 247 and 255, is in
/// `local/archive/linux-port.md`. The header field at
/// `+0x10` is not one — see `docs/formats/art.md`.
///
/// Not every blit skips it. In `MajestyHD.exe` version 1 tiles copy it as a
/// colour ([`crate::data::tile::VERSION_OPAQUE`]), and version 3's mode 0
/// blit draws it from the palette ([`crate::data::tile::Mode::Plain`]).
///
/// 428 uncompressed indexed tiles hold it, median 0.2% of the canvas.
/// Tested by `an_uncompressed_indexed_tile_can_have_holes`.
pub const TRANSPARENT: u8 = 255;

/// Lightness steps in the engine's colour space.
pub const LEVELS: u32 = 64;

/// Whether a palette entry is a placeholder rather than a colour.
///
/// **Inferred**. Art tooling fills the band with magenta for the shadow half
/// (`(178, 0, 178)` upward, per the mod's notes) and green for the glow half:
/// red equals blue, green differs.
///
/// At index 247 world sets hold it in 9,750 of 9,935 tiles, `Interface` sets
/// in 130 of 702. Tested by
/// `an_interface_picture_reads_its_colours_from_the_palette`.
#[must_use]
pub const fn marker(colour: Rgb) -> bool {
    colour.r == colour.b && colour.g != colour.r
}

/// Whether `index` is an operation or hole rather than a colour.
///
/// `colour` is its palette entry, used only by [`Blit::Interface`]. [`shade`]
/// says which.
#[must_use]
pub const fn controls(index: u8, colour: Rgb, blit: Blit) -> bool {
    match blit {
        Blit::World => index >= CONTROL_FIRST,
        Blit::Interface => index >= CONTROL_FIRST && marker(colour),
    }
}

/// What a control index does to on-screen lightness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shade {
    /// Keep this percentage of the value. A shadow.
    Keep(u32),
    /// Add this many of the [`LEVELS`] steps, clamped at the top. A glow.
    Raise(u32),
}

impl Shade {
    /// Resulting lightness step.
    #[must_use]
    pub const fn level(self, level: u32) -> u32 {
        match self {
            // Truncating integer division, as the table is built.
            Shade::Keep(percent) => level * percent / 100,
            Shade::Raise(steps) => {
                let raised = level + steps;

                if raised < LEVELS { raised } else { LEVELS - 1 }
            }
        }
    }

    /// Draws this over `under`, on the 16-bit frame: hue and saturation
    /// keep, bar quantisation; the answer as the screen shows it.
    #[allow(clippy::cast_possible_truncation, reason = "a moved value is under 64")]
    #[must_use]
    pub fn over(self, under: Rgb) -> Rgb {
        let packed = to_hsv(narrow(under));
        let moved = (packed & 0xffc0) | self.level(u32::from(packed) & 0x3f) as u16;

        widen_screen(from_hsv(moved))
    }
}

/// Indices 247 to 254, in order.
///
/// Shadow: multiplicative family `(50, 12)`. Glow: additive family `(30, 12)`,
/// scaled by 32, not [`LEVELS`] — `+9` is `30 * 32 / 100`.
pub const BAND: [Shade; 8] = [
    Shade::Keep(50),
    Shade::Keep(62),
    Shade::Keep(74),
    Shade::Keep(86),
    Shade::Raise(9),
    Shade::Raise(13),
    Shade::Raise(17),
    Shade::Raise(21),
];

/// `(first, step)` of each ramp family, in build order.
///
/// **Binary** — one call site sets all four,
/// `MajestyHD.exe:ramps init`. Table *n* uses `first + n * step` percent.
///
/// | Args | Tables | Rule | Where |
/// |---|---|---|---|
/// | `(50, 6)` | 8 | multiplicative | draw mode 1, [`crate::data::tile::Mode::Shadow`] |
/// | `(50, 12)` | 4 | multiplicative | [`BAND`]'s shadow half, draw mode 7 |
/// | `(10, 6)` | 8 | additive | a whole-rectangle brighten |
/// | `(30, 12)` | 4 | additive | [`BAND`]'s glow half |
///
/// The two four-table families are adjacent, so `(index + 1) & 7` reaches
/// both.
pub const RAMPS: [(u32, u32); 4] = [(50, 6), (50, 12), (10, 6), (30, 12)];

/// Entries in [`forward`] and [`inverse`]: every 16-bit word.
pub const TABLE: usize = 1 << 16;

/// Every 16-bit colour's hue, saturation and value, packed
/// `hhhhhsss ssvvvvvv`: HD's forward table. Value is the low six bits.
#[must_use]
pub fn forward() -> Vec<u16> {
    (0..=u16::MAX).map(to_hsv).collect()
}

/// Packed triple to 16-bit colour: HD's inverse table. Every pattern is
/// valid, so dense.
#[must_use]
pub fn inverse() -> Vec<u16> {
    (0..=u16::MAX).map(from_hsv).collect()
}

/// Value substitution per index; row *n* is [`BAND`]'s *n*th.
#[must_use]
pub fn ramps() -> [[u8; LEVELS as usize]; BAND.len()] {
    let mut out = [[0u8; LEVELS as usize]; BAND.len()];

    for (row, shade) in out.iter_mut().zip(BAND) {
        for (level, moved) in row.iter_mut().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a level is under 64, which is a byte many times over"
            )]
            let step = shade.level(level as u32) as u8;

            *moved = step;
        }
    }

    out
}

/// What `index` does, or `None` if it is an ordinary colour or [`TRANSPARENT`].
#[must_use]
pub const fn shade(index: u8) -> Option<Shade> {
    if index < CONTROL_FIRST || index == TRANSPARENT {
        None
    } else {
        Some(BAND[(index - CONTROL_FIRST) as usize])
    }
}

/// What `index` does in a tile of draw `mode`; `None` where it is a
/// colour, as everywhere in mode 0. **Binary**: mode 1 darkens all eight
/// through the `(50, 6)` family, mode 7 is [`BAND`] (`docs/formats/art.md`, the
/// draw mode picks the blit). Modes 5 and 8 draw nothing from 247
/// (`view::raster::blend`); `None` here.
#[must_use]
pub const fn by_mode(mode: Mode, index: u8) -> Option<Shade> {
    match (mode, shade(index)) {
        (Mode::Shadow, Some(_)) => {
            let (first, step) = RAMPS[0];

            Some(Shade::Keep(first + step * (index - CONTROL_FIRST) as u32))
        }
        (Mode::Band, band) => band,
        _ => None,
    }
}

/// Draws a control-band pixel over `under`. `None` outside the eight.
#[must_use]
pub fn apply(index: u8, under: Rgb) -> Option<Rgb> {
    shade(index).map(|shade| shade.over(under))
}

/// One 565 word as hsv tables build packs it: each channel over
/// 256, the value its brightest, saturation its spread over that, hue in
/// sixths; each times 255 and chopped (`_ftol`), then cut to five, five
/// and six bits. Hue and its sixths are stored as `f32` on the way.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "_ftol chops; max and min are the channels themselves"
)]
fn to_hsv(word: u16) -> u16 {
    let [r, g, b] = super::colour::fields(word);
    let r = f64::from(r << 3) / 256.0;
    let g = f64::from(g << 2) / 256.0;
    let b = f64::from(b << 3) / 256.0;
    let high = r.max(g).max(b);
    let low = r.min(g).min(b);
    let value = (high * 255.0) as u32;
    let saturation = if high == 0.0 {
        0
    } else {
        ((high - low) / high * 255.0) as u32
    };
    let hue = if saturation == 0 {
        0
    } else {
        let spread = high - low;
        let sixths = if r == high {
            (g - b) / spread
        } else if g == high {
            (b - r) / spread + 2.0
        } else {
            (r - g) / spread + 4.0
        };
        let turn = f64::from((f64::from(sixths as f32) / 6.0) as f32);

        // Below red wraps: the packing keeps the low byte.
        (turn * 255.0) as i32 as u32
    };

    (((hue & 0xf8) << 8) | ((saturation & 0xf8) << 3) | (value >> 2)) as u16
}

/// A packed triple back to a 565 word, as hsv tables build fills
/// the inverse: each field over 256, the textbook six sectors with `p`,
/// `q`, `t` stored as `f32`, each channel times 255 chopped, low bits
/// dropped.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "_ftol chops; a field over 256 is exact"
)]
fn from_hsv(packed: u16) -> u16 {
    let hue = f32::from((packed >> 11) << 3) / 256.0;
    let saturation = f32::from(((packed >> 6) & 0x1f) << 3) / 256.0;
    let value = f32::from((packed & 0x3f) << 2) / 256.0;
    let byte = |c: f32| (f64::from(c) * 255.0) as u16;

    let [r, g, b] = if saturation == 0.0 {
        [byte(value); 3]
    } else {
        let sixths = hue * 6.0;
        let sector = sixths.floor();
        let slide = sixths - sector;
        let (top, spread, part) = (f64::from(value), f64::from(saturation), f64::from(slide));

        // The textbook's p, q and t: the floor, falling and rising channels.
        let floor = (top * (1.0 - spread)) as f32;
        let falling = (top * (1.0 - part * spread)) as f32;
        let rising = (top * (1.0 - (1.0 - part) * spread)) as f32;

        match sector as u32 {
            0 => [byte(value), byte(rising), byte(floor)],
            1 => [byte(falling), byte(value), byte(floor)],
            2 => [byte(floor), byte(value), byte(rising)],
            3 => [byte(floor), byte(falling), byte(value)],
            4 => [byte(rising), byte(floor), byte(value)],
            _ => [byte(value), byte(floor), byte(falling)],
        }
    };

    ((r & 0xf8) << 8) | ((g & 0xfc) << 3) | (b >> 3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::raster::colour::widen_replicate;

    const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn only_the_eight_control_indices_shade() {
        assert_eq!(shade(246), None);
        assert_eq!(shade(TRANSPARENT), None);
        assert_eq!(shade(0), None);

        assert_eq!(shade(247), Some(Shade::Keep(50)));
        assert_eq!(shade(250), Some(Shade::Keep(86)));
        assert_eq!(shade(251), Some(Shade::Raise(9)));
        assert_eq!(shade(254), Some(Shade::Raise(21)));
    }

    /// [`BAND`] follows from the two ramp families.
    #[test]
    fn the_band_is_what_its_two_ramp_families_generate() {
        let (dark_first, dark_step) = RAMPS[1];
        let (glow_first, glow_step) = RAMPS[3];

        let generated: Vec<Shade> = (0..4)
            .map(|n| Shade::Keep(dark_first + n * dark_step))
            .chain((0..4).map(|n| Shade::Raise((glow_first + n * glow_step) * 32 / 100)))
            .collect();

        assert_eq!(generated, BAND);
    }

    #[test]
    fn a_raise_clamps_at_the_top_step() {
        assert_eq!(Shade::Raise(21).level(40), 61);
        assert_eq!(Shade::Raise(21).level(50), LEVELS - 1);
        assert_eq!(Shade::Keep(50).level(21), 10, "truncating, not rounding");
    }

    /// Grey halves cleanly, and stays grey.
    #[test]
    fn the_deepest_shadow_halves_a_grey() {
        assert_eq!(apply(247, rgb(128, 128, 128)), Some(rgb(56, 56, 56)));
    }

    /// The three tables match [`Shade::over`] for every colour and index.
    #[test]
    fn the_two_tables_compose_into_the_operation_they_came_from() {
        let (forward, inverse, ramps) = (forward(), inverse(), ramps());

        assert_eq!(forward.len(), TABLE);
        assert_eq!(inverse.len(), TABLE);

        for (word, &packed) in forward.iter().enumerate() {
            let packed = usize::from(packed);
            let lightness = packed & 0x3f;

            #[expect(
                clippy::cast_possible_truncation,
                reason = "the table is exactly the u16 range"
            )]
            let colour = widen_replicate(word as u16);

            for (n, shade) in BAND.iter().enumerate() {
                let moved = (packed & 0xffc0) | usize::from(ramps[n][lightness]);
                let want = narrow(shade.over(colour));

                assert_eq!(
                    inverse[moved],
                    want,
                    "colour {word:#06x}, index {}",
                    247 + n
                );
            }
        }
    }

    /// Snow under a shadow stays grey: value scales, hue and saturation
    /// keep. **Game**: `maj_grov_urtat.png`, snow `(224, 220, 224)` under a
    /// tree's shadow is `(104, 104, 104)`.
    #[test]
    fn a_shadow_on_snow_is_grey() {
        assert_eq!(apply(247, rgb(224, 220, 224)), Some(rgb(104, 104, 104)));
    }

    /// Only the value moves.
    #[test]
    fn a_shadow_keeps_the_hue_it_falls_on() {
        let under = narrow(rgb(112, 84, 56));
        let shaded = narrow(apply(247, rgb(112, 84, 56)).expect("247 shades"));
        let (before, after) = (to_hsv(under), to_hsv(shaded));

        assert_eq!(before >> 11, after >> 11, "hue");
        assert!(
            after & 0x3f < (before & 0x3f) * 55 / 100,
            "and it got darker"
        );
    }

    #[test]
    fn a_glow_brightens_and_the_band_is_ordered() {
        let under = rgb(80, 60, 40);
        let value = |c| to_hsv(narrow(c)) & 0x3f;
        let levels: Vec<u16> = (247..=254)
            .map(|i| value(apply(i, under).expect("the band shades")))
            .collect();

        assert!(levels.windows(2).all(|w| w[0] < w[1]), "{levels:?}");
        assert!(levels[0] < value(under), "247 darkens");
        assert!(levels[7] > value(under), "254 lightens");
    }
}
