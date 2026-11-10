//! Shadows that do not stack: `[enhancements] shadows = "soft"`, the
//! default; and the factors every path that shades draws with.
//!
//! The engine moves a pixel's HSV value and keeps hue and saturation
//! ([`regent_core::view::raster::shade`]). Scaling the value with hue and
//! saturation kept is scaling all three channels, so each darkening
//! percent is a factor on linear colour ([`DARK`]). Every percent the
//! engine darkens by is one of the `(50, 6)` family's eight: draw mode
//! 1's eight, mode 7's four (every other one), the bevels and the glyph
//! shadow. [`Ramps`] says which family a tile's indices take.
//!
//! One sun casts every shadow, so a pixel in two is as dark as in the
//! darker. Each band pixel writes its coverage to alpha, the blend keeping
//! the least alpha, the most coverage; one pass at the end multiplies
//! colour by one strength, in linear light, so hue is kept and no palette
//! table rounds it:
//!
//! ```text
//!  body          alpha 255: no shadow       (the clear's 0 too)
//!  band, darken  alpha = min(dst, a)        a = 255 − cover × 254
//!  band, glow    colour × factor, in order  mode 7's 251–254, as "blend"
//!  lift          alpha 255: unshaded        a translucent thing ([`lifts`])
//!  resolve       colour × (1 − c (1 − STRENGTH)),  c = (255 − alpha) / 254
//! ```
//!
//! Depth places a shadow: it falls on what is drawn behind its caster,
//! and on what stands on the ground it covers ([`crate::lie`]); not on
//! what stands in front of both.
//!
//! [`STRENGTH`] is the darkest factor, so one shadow darkens by its own
//! factor, as `"blend"` does; the darkening indices are coverages in
//! proportion ([`cover`]).
//!
//! Where nothing is under a picture to darken, a hardware cursor or egui
//! over an unknown background, a shadow is black at the share its factor
//! takes off ([`veil`]).

use regent_core::data::payload::Rgb;
use regent_core::data::tile::Mode;
use regent_core::view::raster::shade::{self, BAND, CONTROL_FIRST, RAMPS, Shade, TRANSPARENT};

/// The factor of each of the `(50, 6)` family's percents, 50 to 92, on
/// linear colour, in thousandths.
///
/// Fitted against [`Shade::over`] by the mean value step over 16-bit
/// colours, as the target encodes sRGB and the resolve presents on the
/// 565 grid; mode 7's four first, mode 1's other four later. All within
/// two of 64 steps over every colour (`the_factors_keep_the_engines_value`).
pub const DARK: [u32; 8] = [198, 246, 322, 379, 468, 550, 658, 752];

/// The factor of each of [`BAND`]'s glows, 251 to 254, in thousandths: a
/// gain capped at twice. Fitted over values 20 to 50 of 64, the lit
/// ground's.
pub const GLOW: [u32; 4] = [1445, 1740, 2000, 2000];

/// Thousandths in one.
pub const ONE: u32 = 1000;

/// What a fully covered pixel's linear colour is multiplied by, in
/// thousandths: the darkest factor.
pub const STRENGTH: u32 = DARK[0];

/// Alpha where no shadow falls: a body's.
pub const CLEAR: u8 = u8::MAX;

/// Alpha of full coverage: the least past the clear colour's 0, which
/// reads as none.
pub const FULL: u8 = 1;

/// Indices from 247 to 254.
pub const INDICES: usize = (TRANSPARENT - CONTROL_FIRST) as usize;

/// The alpha from which a translucent pixel lifts the shadow under it:
/// half drawn.
pub const LIFT_FROM: u8 = 128;

/// Which ramps a tile's indices 247 to 254 take: the engine's blit by
/// draw mode (`shade::by_mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ramps {
    /// Draw mode 7: 247 to 250 darken, 251 to 254 glow. Every mode but 1
    /// takes these.
    #[default]
    Band,
    /// Draw mode 1: all eight darken, the `(50, 6)` family.
    Shadow,
}

impl Ramps {
    /// The ramps of a tile in draw `mode`.
    #[must_use]
    pub const fn of(mode: Mode) -> Ramps {
        match mode {
            Mode::Shadow => Ramps::Shadow,
            _ => Ramps::Band,
        }
    }

    /// What `index` does under these ramps; `None` outside 247 to 254.
    #[must_use]
    pub const fn shade(self, index: u8) -> Option<Shade> {
        match self {
            Ramps::Band => shade::by_mode(Mode::Band, index),
            Ramps::Shadow => shade::by_mode(Mode::Shadow, index),
        }
    }
}

/// `shade`'s factor on linear colour, in thousandths; `None` for a
/// percent off the family or a raise not [`BAND`]'s.
#[must_use]
pub fn factor(shade: Shade) -> Option<u32> {
    let (first, step) = RAMPS[0];

    match shade {
        Shade::Keep(percent) if percent >= first && (percent - first).is_multiple_of(step) => {
            usize::try_from((percent - first) / step)
                .ok()
                .and_then(|n| DARK.get(n).copied())
        }
        Shade::Keep(_) => None,
        Shade::Raise(_) => BAND[BAND.len() - GLOW.len()..]
            .iter()
            .position(|glow| *glow == shade)
            .map(|n| GLOW[n]),
    }
}

/// Each index's factor under `ramps`, 247 to 254, in thousandths; one
/// where none.
#[must_use]
pub fn factors(ramps: Ramps) -> [u32; INDICES] {
    std::array::from_fn(|n| {
        index(n)
            .and_then(|i| ramps.shade(i))
            .and_then(factor)
            .unwrap_or(ONE)
    })
}

/// Each index's alpha under `ramps`, 247 to 254: a darkening's coverage,
/// [`CLEAR`] for a glow.
#[must_use]
pub fn alphas(ramps: Ramps) -> [u8; INDICES] {
    factors(ramps).map(|f| alpha(f).unwrap_or(CLEAR))
}

/// Index `n` past 247.
fn index(n: usize) -> Option<u8> {
    u8::try_from(n).ok().map(|n| CONTROL_FIRST + n)
}

/// How much of a full shadow factor `f` is, in thousandths: its
/// darkening over the darkest's, both in linear light. `None` for a glow.
///
/// ```text
///  50 % 1000   56 % 940   62 % 845   68 % 774   74 % 663   80 % 561   86 % 426   92 % 309
/// ```
#[must_use]
pub const fn cover(f: u32) -> Option<u32> {
    if f < ONE {
        Some((ONE - f) * ONE / (ONE - STRENGTH))
    } else {
        None
    }
}

/// The alpha a band pixel of factor `f` writes; `None` for a glow.
#[must_use]
pub const fn alpha(f: u32) -> Option<u8> {
    match cover(f) {
        #[expect(clippy::cast_possible_truncation, reason = "at most CLEAR")]
        Some(c) => Some(CLEAR - ((c * (CLEAR - FULL) as u32 + ONE / 2) / ONE) as u8),
        None => None,
    }
}

/// The coverage alpha `a` stands for, 0 to 1. The clear's 0 is none.
#[must_use]
pub fn coverage(a: u8) -> f32 {
    if a == 0 {
        0.0
    } else {
        f32::from(CLEAR - a) / f32::from(CLEAR - FULL)
    }
}

/// What linear colour is multiplied by under coverage `c`.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "thousandths")]
pub fn darken(c: f32) -> f32 {
    1.0 - c * (1.0 - STRENGTH as f32 / ONE as f32)
}

/// One pixel as the passes draw it: `under`, linear, then band `indices`
/// of a tile with `ramps` in order, then the resolve. The GPU's
/// arithmetic, for the tests.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "thousandths")]
pub fn over(under: [f32; 3], ramps: Ramps, indices: &[u8]) -> [f32; 3] {
    let factors: Vec<u32> = indices
        .iter()
        .filter_map(|i| ramps.shade(*i))
        .filter_map(factor)
        .collect();
    let least = factors
        .iter()
        .filter_map(|f| alpha(*f))
        .fold(CLEAR, u8::min);

    // Glows multiply in draw order, as "blend"'s.
    let gain: f32 = factors
        .iter()
        .filter(|f| alpha(**f).is_none())
        .map(|f| *f as f32 / ONE as f32)
        .product();

    under.map(|c| c * gain * darken(coverage(least)))
}

/// `shade` drawn over a known colour `under` on the CPU, as one shadow
/// or glow of the GPU's: its factor on linear colour, on the 565 grid as
/// the resolve presents. A shade with no factor leaves it.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "thousandths")]
pub fn shaded(shade: Shade, under: Rgb) -> Rgb {
    let f = factor(shade).map_or(1.0, |f| f as f32 / ONE as f32);

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to a byte"
    )]
    let channel = |v: u8, mask: u8| {
        let out = encode((decode(f32::from(v) / 255.0) * f).min(1.0));

        ((out * 255.0).round() as u8) & mask
    };

    Rgb {
        r: channel(under.r, 0xf8),
        g: channel(under.g, 0xfc),
        b: channel(under.b, 0xf8),
    }
}

/// Where a veil is blended: on linear colour (an sRGB target, as Bevy
/// and egui draw), or on the encoded bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blending {
    Linear,
    Encoded,
}

/// The alpha of the black that stands in for a darkening `shade` where
/// what is under it is unknown: the share its factor takes off, so over
/// anything it darkens as the shadow would. `None` for a glow.
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "thousandths")]
pub fn veil(shade: Shade, blending: Blending) -> Option<u8> {
    let kept = factor(shade)
        .filter(|f| *f < ONE)
        .map(|f| f as f32 / ONE as f32)?;
    let kept = match blending {
        Blending::Linear => kept,
        Blending::Encoded => encode(kept),
    };

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a share of 255"
    )]
    Some(((1.0 - kept) * 255.0).round() as u8)
}

/// Which indices of a translucent picture, alphas `alphas`, lift the
/// shadow under them: 255 for those to lift, 0 for the rest.
///
/// The engine shades a shadow when drawn, so a translucent pixel drawn
/// after blends over the shaded colour; here coverage resolves last,
/// under everything. **Inferred**, an approximation: a pixel at least
/// half drawn shows unshaded, a fainter one shaded with what is under it.
#[must_use]
pub fn lifts(alphas: &[u8; 256]) -> [u8; 256] {
    alphas.map(|a| if a >= LIFT_FROM { u8::MAX } else { 0 })
}

/// sRGB to linear, as the target decodes.
fn decode(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear to sRGB, as the target encodes.
fn encode(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::view::raster::colour::widen_screen;
    use regent_core::view::raster::shade::LEVELS;

    const GROUND: [f32; 3] = [0.3, 0.25, 0.1];

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
    }

    /// The value step, of 64.
    fn value(c: Rgb) -> u32 {
        u32::from(c.r.max(c.g).max(c.b) >> 2)
    }

    /// Mean and worst value steps between the engine's `shade` and
    /// [`shaded`], over colours whose value is in `values`.
    fn off(shade: Shade, values: std::ops::Range<u32>) -> (f64, u32) {
        let offs: Vec<u32> = (0..=u16::MAX)
            .map(widen_screen)
            .filter(|c| values.contains(&value(*c)))
            .map(|c| value(shade.over(c)).abs_diff(value(shaded(shade, c))))
            .collect();

        #[expect(clippy::cast_precision_loss, reason = "a count under 2^16")]
        let mean = f64::from(offs.iter().sum::<u32>()) / offs.len() as f64;

        (mean, offs.into_iter().max().unwrap_or(0))
    }

    /// Every darkening percent keeps the engine's value within two steps
    /// over every colour; glow within a few on the lit ground's values,
    /// worse at the two the gain caps.
    #[test]
    fn the_factors_keep_the_engines_value() {
        for n in 0..8 {
            let shade = Shade::Keep(50 + 6 * n);
            let (mean, worst) = off(shade, 0..LEVELS);

            assert!(mean < 0.6 && worst <= 2, "{shade:?}: {mean} {worst}");
        }

        let glow: Vec<u32> = BAND[4..].iter().map(|s| off(*s, 20..51).1).collect();

        assert_eq!(glow, [3, 5, 9, 13]);
    }

    /// Mode 7's indices take the family's every other factor and the
    /// glows; mode 1's all eight factors, each its own engine percent.
    #[test]
    fn each_mode_takes_its_own_factors() {
        assert_eq!(
            factors(Ramps::Band),
            [198, 322, 468, 658, 1445, 1740, 2000, 2000]
        );
        assert_eq!(factors(Ramps::Shadow), DARK);

        for index in 247..=254 {
            let percent = 50 + 6 * u32::from(index - 247);

            assert_eq!(Ramps::Shadow.shade(index), Some(Shade::Keep(percent)));
            assert_eq!(
                factor(Shade::Keep(percent)),
                Some(DARK[usize::from(index - 247)])
            );
        }

        assert_eq!(Ramps::of(Mode::Shadow), Ramps::Shadow);
        assert_eq!(Ramps::of(Mode::Band), Ramps::Band);
        assert_eq!(Ramps::Band.shade(246), None);
        assert_eq!(Ramps::Shadow.shade(TRANSPARENT), None);
    }

    /// Mode 1's 251 to 254 darken, where mode 7's glow.
    #[test]
    fn mode_ones_upper_half_darkens() {
        let shadow = alphas(Ramps::Shadow);
        let band = alphas(Ramps::Band);

        assert!(shadow.iter().all(|a| *a < CLEAR));
        assert!(shadow.windows(2).all(|w| w[0] < w[1]), "{shadow:?}");
        assert_eq!(band[4..], [CLEAR; 4]);

        for (index, ramps) in [(252, Ramps::Shadow), (252, Ramps::Band)] {
            let out = over(GROUND, ramps, &[index]);
            let darker = out[0] < GROUND[0];

            assert_eq!(darker, ramps == Ramps::Shadow, "{ramps:?}");
        }
    }

    /// Two overlapping shadows darken as the darker alone, in either order.
    #[test]
    fn overlapping_shadows_darken_as_one() {
        let band = |indices: &[u8]| over(GROUND, Ramps::Band, indices);

        assert!(close(band(&[247, 247]), band(&[247])));
        assert!(close(band(&[250, 247]), band(&[247])));
        assert!(close(band(&[247, 250]), band(&[247])));
        assert!(close(band(&[249, 249, 249]), band(&[249])));
        assert!(close(
            over(GROUND, Ramps::Shadow, &[254, 248]),
            over(GROUND, Ramps::Shadow, &[248])
        ));
    }

    /// One shadow darkens as its factor, hue kept: every channel by the
    /// same, in either mode.
    #[test]
    fn one_shadow_darkens_as_its_factor() {
        for ramps in [Ramps::Band, Ramps::Shadow] {
            for (index, f) in (247..=254).zip(factors(ramps)) {
                #[expect(clippy::cast_precision_loss, reason = "thousandths")]
                let f = f as f32 / ONE as f32;
                let out = over(GROUND, ramps, &[index]);

                // Alpha's eight bits round the cover; a glow is exact.
                for (o, g) in out.iter().zip(GROUND) {
                    assert!((o / g - f).abs() < 0.003, "{index}: {} {f}", o / g);
                }
            }
        }
    }

    /// Covers fall from the darkest factor; the darkest is full, its alpha
    /// the least above the clear's.
    #[test]
    fn the_darker_index_covers_more() {
        let covers: Vec<u32> = DARK.iter().filter_map(|f| cover(*f)).collect();

        assert_eq!(covers, [1000, 940, 845, 774, 663, 561, 426, 309]);
        assert_eq!(alpha(STRENGTH), Some(FULL));
        assert_eq!(alpha(ONE), None);
        assert!((coverage(FULL) - 1.0).abs() < 1e-6);
        assert!(coverage(CLEAR) == 0.0 && coverage(0) == 0.0);
    }

    /// Glows brighten in draw order and are shaded by a shadow too.
    #[test]
    fn a_glow_brightens_and_a_shadow_shades_it() {
        let ring = over(GROUND, Ramps::Band, &[252]);

        assert!(ring[0] > GROUND[0]);
        assert!(close(
            over(GROUND, Ramps::Band, &[252, 247]),
            over(ring, Ramps::Band, &[247])
        ));
    }

    /// A veil takes off what the factor does: on linear colour exactly,
    /// on encoded bytes the encoded share. No veil for a glow.
    #[test]
    fn a_veil_takes_off_what_the_factor_does() {
        assert_eq!(veil(Shade::Keep(50), Blending::Linear), Some(205));
        assert_eq!(veil(Shade::Keep(68), Blending::Linear), Some(158));
        assert_eq!(veil(Shade::Keep(50), Blending::Encoded), Some(132));
        assert_eq!(veil(Shade::Raise(9), Blending::Linear), None);
        assert_eq!(veil(Shade::Keep(51), Blending::Linear), None);
    }

    /// A translucent pixel at least half drawn lifts the shadow under it;
    /// a fainter one, or none, keeps it.
    #[test]
    fn a_half_drawn_pixel_lifts_the_shadow_under_it() {
        let mut alphas = [0; 256];

        alphas[1] = 127;
        alphas[2] = 128;
        alphas[3] = 255;

        let lifted = lifts(&alphas);

        assert_eq!(lifted[..5], [0, 0, 255, 255, 0]);
    }
}
