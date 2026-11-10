//! A unit's label over the map: a reward flag's reward. **Binary**
//! unless marked.
//!
//! `GSUnit` draws it at view zoom 1.0 alone, for a
//! unit of kind 4 (a reward flag), or any unit while `DisplayFlagNames`
//! (display flags bit 0) is set. Its text object, unit, is made
//! once by method; the flag's:
//!
//! ```text
//!  text   sprintf "%d" of APR:1, RewardCost: the reward, raises in
//!  font   fnt7, an IMAG of glyphs; colour 0x80ffffff
//!  flags  5: centred (bit 0), a drop shadow (bit 2)
//!  box    w x h, w the advances, h a line plus the shadow
//!         x0 = -(w / 2), y1 = R.y0, R the pad's pixel rect
//!         (PRIM +0x04, placement::pixel_rect), about the screen place
//! ```
//!
//! `GSUnit` clears the made bit (unit bit 0)
//! when it writes `APR:1`, so a raise remakes the text.
//!
//! The shadow: the text draw turns the font context's
//! shadow on; `CYFontImage` blits each glyph first
//! at the context's offset, 1, 1, in blit mode 3, then
//! itself. Mode 3 moves each pixel's HSV value through
//! Ramps shadow8 table 3: [`SHADE`].
//!
//! **Capture** (`local/shots/flag_selected.png`): an attack flag's
//! 500 with its glyphs and shadow where this puts them, `AVn2`'s
//! brackets round it.
//!
//! Any other unit's label is its name,
//! the same font, colour and shadow, under its hit point bar:
//!
//! ```text
//!  text   a building (GenUnit      (0)): its own name (AP:1
//!         spelled or numbered), else its HEAD title (desc);
//!         else its name, "%s (%d)" with APV:11 when 1 or more
//!  box    x0 = (R.x0 + R.x1) / 2 - w / 2, y1 = R.y1 + 12
//!  remade when APV:11 is written (GSUnit)
//! ```
//!
//! A building shows it only standing (`BuildingRec`:
//! `GenUnit` under 3); the build preview never
//! (`ConstructionDummyRec` is empty). Nameless scatter, effects and
//! message flags draw an empty text. **Capture** (`maj_mayhew.png`):
//! five buildings' names under their bars.

use crate::data::error::Result;
use crate::data::fourcc::FourCc;

use super::bar::Body;
use crate::content::imagery::{self, FrameSource};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::payload::Rgb;
use crate::sim::placement;
use crate::view::menu::font::{Font, Metrics};
use crate::view::raster::shade::{RAMPS, Shade};
use crate::view::sprite::Sprite;

/// The font: `0x37746e66` before text set font.
pub const FONT: FourCc = FourCc(*b"fnt7");

/// A glyph's pixel as the capture shows it colour text set colour
/// sets, `0x80ffffff`: `0xe0` a
/// channel, not `0xff`. **Capture**; **Inferred**: the engine's glyph
/// tint, as `menu_main.png`'s blue hot keys show `0xe0` for `0xff`.
pub const INK: Rgb = Rgb {
    r: 0xe0,
    g: 0xe0,
    b: 0xe0,
};

/// Where the shadow copy goes, right and down: the font context's
///  and a field.
pub const SHADOW: (i32, i32) = (1, 1);

/// The shadow's shade: blit mode 3 reads table 3 of the first ramp
/// family, `50 + 3 × 6` percent of the value kept.
pub const SHADE: Shade = Shade::Keep(RAMPS[0].0 + SHADE_TABLE * RAMPS[0].1);

/// The ramp table blit mode 3 reads: table 3 of
/// ramps shadow8.
const SHADE_TABLE: u32 = 3;

/// The reward's text: `sprintf("%d")` of `APR:1`, 0 where unset.
#[must_use]
pub fn text(reward: i32) -> String {
    format!("{reward}")
}

/// A label's box about the unit's screen place, pixels right and down:
/// `width` by `height`, centred, its bottom on `rect`'s top.
#[must_use]
pub const fn place(width: i32, height: i32, rect: [i32; 4]) -> [i32; 4] {
    let x0 = -(width / 2);
    let y1 = rect[1];

    [x0, y1 - height, x0 + width, y1]
}

/// How far a name's box ends under the pixel rect's bottom: `0x0c`
/// added in `GSUnit`.
pub const NAME_DROP: i32 = 12;

/// A unit's name label, as `GSUnit` writes it: a building's
/// `own` name, else its `title`; anyone else's `own` name, then
/// `" (level)"` (`"%s (%d)"`) from level 1. Empty where
/// it draws nothing.
#[must_use]
pub fn name(body: Body, own: Option<&str>, title: &str, level: i32) -> String {
    let own = own.unwrap_or_default();

    match body {
        Body::Building if own.is_empty() => title.to_owned(),
        Body::Other if level >= 1 => format!("{own} ({level})"),
        Body::Building | Body::Other => own.to_owned(),
    }
}

/// Whether a unit's name shows: a building only standing, not a ruin;
/// a character lying dead too (`VehicleRec`
/// asks nothing).
#[must_use]
pub const fn standing(body: Body, ruined: bool) -> bool {
    !(matches!(body, Body::Building) && ruined)
}

/// A name's box about the unit's screen place, pixels right and down:
/// `width` by `height`, centred on `rect`, its bottom [`NAME_DROP`] under
/// `rect`'s.
#[must_use]
#[allow(
    clippy::manual_midpoint,
    reason = "the engine's halving truncates toward zero, spelled out"
)]
pub const fn under(width: i32, height: i32, rect: [i32; 4]) -> [i32; 4] {
    let x0 = (rect[0] + rect[2]) / 2 - width / 2;
    let y1 = rect[3] + NAME_DROP;

    [x0, y1 - height, x0 + width, y1]
}

/// A unit's name box for a text `width` by `height`, about its screen
/// place. `None` without art.
///
/// # Errors
///
/// If a tile the pad's rect needs will not parse.
pub fn below(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    (width, height): (i32, i32),
) -> Result<Option<[i32; 4]>> {
    Ok(placement::pixel_rect(catalogue, unit)?.map(|r| under(width, height, r)))
}

/// A unit's label box for a text `width` by `height`, about its screen
/// place. `None` without art.
///
/// # Errors
///
/// If a tile the pad's rect needs will not parse.
pub fn placed(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    (width, height): (i32, i32),
) -> Result<Option<[i32; 4]>> {
    Ok(placement::pixel_rect(catalogue, unit)?.map(|r| place(width, height, r)))
}

/// One pixel of a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pixel {
    Clear,
    /// Under the shadow: [`SHADE`] of what is beneath.
    Shadow,
    /// A glyph's: [`INK`].
    Ink,
}

/// A label's picture, row-major from the box's top left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lettering {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<Pixel>,
}

/// A label's height for a font of `metrics`: a line, and the shadow's
/// drop.
#[must_use]
pub const fn height(metrics: Metrics) -> i32 {
    metrics.ascent + metrics.descent + SHADOW.1
}

/// `text`'s box in [`FONT`]: [`Font::width`] × [`height`].
/// `None` without the font.
///
/// # Errors
///
/// If the font will not parse.
pub fn measure(catalogue: &Catalogue<'_>, text: &str) -> Result<Option<(i32, i32)>> {
    let font = match catalogue.find(b"IMAG", FONT) {
        Some(at) => imagery::sets(catalogue, at)?.first().map(Font::of_set),
        None => None,
    };

    Ok(font.map(|f| (f.width(text), height(f.metrics()))))
}

/// `text` in [`FONT`], its box [`Font::width`] × [`height`]: each
/// glyph's shadow, then the glyph, its anchor on the pen at the
/// baseline, `ascent` down. `None` without the font.
///
/// # Errors
///
/// If the font or a glyph will not parse.
pub fn letter(catalogue: &Catalogue<'_>, text: &str) -> Result<Option<Lettering>> {
    let Some(at) = catalogue.find(b"IMAG", FONT) else {
        return Ok(None);
    };
    let sets = imagery::sets(catalogue, at)?;
    let Some(font) = sets.first() else {
        return Ok(None);
    };
    let measures = Font::of_set(font);
    let metrics = measures.metrics();
    let (w, h) = (measures.width(text), height(metrics));
    let size = |n: i32| usize::try_from(n).unwrap_or(0);
    let mut out = Lettering {
        width: size(w),
        height: size(h),
        pixels: vec![Pixel::Clear; size(w) * size(h)],
    };
    let mut glyphs = Vec::new();
    let mut pen = 0;

    for c in text.bytes() {
        let tile = font
            .poses
            .get(usize::from(c))
            .and_then(|p| p.frames.first());

        if let Some(FrameSource::Tile { archive, index, .. }) = tile
            && let Some(sprite) = Sprite::at(catalogue, *archive, *index)
        {
            glyphs.push((pen, sprite?));
        }

        pen += measures.step(c, metrics);
    }

    // Glyph by glyph, its shadow first.
    for (pen, sprite) in &glyphs {
        for (ink, (dx, dy)) in [(Pixel::Shadow, SHADOW), (Pixel::Ink, (0, 0))] {
            stamp(&mut out, sprite, (pen + dx, metrics.ascent + dy), ink);
        }
    }

    Ok(Some(out))
}

/// `sprite`'s covered pixels as `ink`, its anchor at `at`.
fn stamp(out: &mut Lettering, sprite: &Sprite, at: (i32, i32), ink: Pixel) {
    let w = usize::from(sprite.width);
    let (ax, ay) = (i32::from(sprite.anchor.0), i32::from(sprite.anchor.1));

    for (i, _) in sprite.covered.iter().enumerate().filter(|(_, c)| **c) {
        let (x, y) = (i % w, i / w);
        let x = i32::try_from(x).unwrap_or(i32::MAX) + at.0 - ax;
        let y = i32::try_from(y).unwrap_or(i32::MAX) + at.1 - ay;
        let inside = usize::try_from(x)
            .ok()
            .zip(usize::try_from(y).ok())
            .filter(|(x, y)| *x < out.width && *y < out.height);

        if let Some((x, y)) = inside {
            out.pixels[y * out.width + x] = ink;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Centred on the screen place, its bottom on the rect's top; an odd
    /// width leans right, as `-(w / 2)` truncates.
    #[test]
    fn a_label_sits_centred_on_its_rects_top() {
        assert_eq!(place(21, 11, [-18, -23, 17, 28]), [-10, -34, 11, -23]);
        assert_eq!(place(20, 11, [0, 5, 0, 9]), [-10, -6, 10, 5]);
    }

    /// The reward reads as `%d`; the shadow keeps 68 %.
    #[test]
    fn the_reward_is_its_number_and_the_shadow_keeps_68_percent() {
        assert_eq!(text(0), "0");
        assert_eq!(SHADE, Shade::Keep(68));
    }

    /// A building's name, else its title, never a level; anyone else's
    /// name, its level from 1, nothing without either.
    #[test]
    fn a_name_is_a_buildings_title_or_a_characters_level() {
        assert_eq!(name(Body::Building, None, "Palace", 0), "Palace");
        assert_eq!(name(Body::Building, Some(""), "Palace", 0), "Palace");
        assert_eq!(
            name(Body::Building, Some("Library #1"), "Library", 3),
            "Library #1"
        );
        assert_eq!(name(Body::Other, Some("Aric"), "Warrior", 2), "Aric (2)");
        assert_eq!(name(Body::Other, Some("Aric"), "Warrior", 0), "Aric");
        assert_eq!(name(Body::Other, None, "Tree", 0), "");
        assert_eq!(name(Body::Other, None, "Rat", 1), " (1)");
    }

    /// A ruin shows no name; a character lying dead does.
    #[test]
    fn a_ruin_shows_no_name() {
        assert!(standing(Body::Building, false));
        assert!(!standing(Body::Building, true));
        assert!(standing(Body::Other, true));
    }

    /// Centred on the rect, its bottom 12 under the rect's; an odd width
    /// leans right.
    #[test]
    fn a_name_sits_centred_twelve_under_its_rect() {
        assert_eq!(under(21, 11, [-18, -23, 17, 28]), [-10, 29, 11, 40]);
        assert_eq!(under(20, 11, [-40, -60, 40, 20]), [-10, 21, 10, 32]);
    }
}
