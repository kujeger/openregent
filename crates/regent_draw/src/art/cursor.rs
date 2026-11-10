//! The mouse cursor's picture: a `CUR1` slot composed for a hardware
//! cursor. Which slot shows is the library's ([`regent_core::view::map::cursor`]).

use regent_core::content::imagery::{self, Pose};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::error::Result;
use regent_core::data::imag::{Blit, Pass};
use regent_core::data::payload::Palette;
use regent_core::data::tile::Mode;
use regent_core::view::map::cursor::IMAGE;
use regent_core::view::raster::atlas::Sheet;
use regent_core::view::raster::blend;
use regent_core::view::raster::shade::{self, CONTROL_FIRST, LEVELS, Shade};
use regent_core::view::sprite::{Colour, Ink, Sprite};

use crate::soft::{self, Blending};

/// A cursor picture: the composed slot, and the pixel the mouse is at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub sheet: Sheet,
    pub hotspot: (u16, u16),
}

/// Slot `slot` of `CUR1` set `set`: each stream's first frame at its
/// nudge about the anchor. `None` without the image, set, slot or a
/// frame.
///
/// A hardware cursor has nothing under it to darken, so a shadow index
/// draws black at the share soft shadows take off ([`soft::veil`]), a
/// glow white. **Inferred**: the system blends a cursor on encoded bytes.
///
/// # Errors
///
/// If the image or a tile will not parse.
pub fn picture(catalogue: &Catalogue<'_>, set: u32, slot: usize) -> Result<Option<Picture>> {
    let sets = catalogue
        .find(b"IMAG", IMAGE)
        .map(|at| imagery::sets(catalogue, at))
        .transpose()?;
    let pose = sets
        .iter()
        .flatten()
        .find(|a| a.id == set)
        .and_then(|a| a.poses.get(slot));
    let parts = pose.map(|pose| parts(catalogue, pose)).transpose()?;

    Ok(parts.and_then(|p| composed(&p)))
}

/// Each stream's first frame of `pose`, top left from the hotspot.
fn parts(catalogue: &Catalogue<'_>, pose: &Pose) -> Result<Vec<(Sprite, (i32, i32))>> {
    (0..pose.streams)
        .filter_map(|s| pose.blits(s, Pass::First).first().copied().flatten())
        .filter_map(|b| {
            let sprite = Sprite::at(catalogue, b.archive, b.index)?;

            Some(sprite.map(|sprite| {
                let corner = (
                    i32::from(b.nudge.0) - i32::from(sprite.anchor.0),
                    i32::from(b.nudge.1) - i32::from(sprite.anchor.1),
                );

                (sprite, corner)
            }))
        })
        .collect()
}

/// `parts` on one sheet holding the hotspot too. `None` for no part.
fn composed(parts: &[(Sprite, (i32, i32))]) -> Option<Picture> {
    let bounds = parts
        .iter()
        .fold(None, |b: Option<[i32; 4]>, (s, (x, y))| {
            let [x0, y0, x1, y1] = b.unwrap_or([0, 0, 1, 1]);

            Some([
                x0.min(*x),
                y0.min(*y),
                x1.max(x + i32::from(s.width)),
                y1.max(y + i32::from(s.height)),
            ])
        })?;
    let [x0, y0, x1, y1] = bounds;
    let mut sheet = Sheet::blank(
        usize::try_from(x1 - x0).ok()?,
        usize::try_from(y1 - y0).ok()?,
    );

    for (sprite, (x, y)) in parts {
        paint(&mut sheet, sprite, (x - x0, y - y0));
    }

    Some(Picture {
        sheet,
        hotspot: (u16::try_from(-x0).ok()?, u16::try_from(-y0).ok()?),
    })
}

/// Draw `sprite` over `sheet`, top left at `at`, alpha over alpha.
fn paint(sheet: &mut Sheet, sprite: &Sprite, at: (i32, i32)) {
    let width = usize::from(sprite.width);

    for i in 0..sprite.covered.len() {
        let (col, row) = (i % width, i / width);
        let x = usize::try_from(at.0).ok().map(|x| x + col);
        let y = usize::try_from(at.1).ok().map(|y| y + row);

        if let (Some(x), Some(y), Some(ink)) = (x, y, ink(sprite, i)) {
            let to = (y * sheet.width + x) * 4;

            blend_over(&mut sheet.rgba[to..to + 4], ink);
        }
    }
}

/// Straight alpha `src` over the pixel `dst`.
fn blend_over(dst: &mut [u8], src: [u8; 4]) {
    let a = u32::from(src[3]);
    let b = u32::from(dst[3]) * (255 - a) / 255;
    let out = a + b;
    let mix = |s: u8, d: u8| match out {
        0 => 0,
        _ => (u32::from(s) * a + u32::from(d) * b) / out,
    };

    for c in 0..3 {
        dst[c] = u8::try_from(mix(src[c], dst[c])).unwrap_or(u8::MAX);
    }

    dst[3] = u8::try_from(out).unwrap_or(u8::MAX);
}

/// What pixel `i` of `sprite` puts on the cursor; `None` for nothing.
fn ink(sprite: &Sprite, i: usize) -> Option<[u8; 4]> {
    match &sprite.colour {
        _ if !sprite.covered[i] => None,
        Colour::Direct { .. } => match sprite.ink(i, Blit::World) {
            Ink::Colour(rgb) => Some(opaque(rgb.channels())),
            _ => None,
        },
        Colour::Indexed { indices, palette } => indexed(sprite, indices[i], palette),
    }
}

/// An indexed pixel by its tile's draw mode.
fn indexed(sprite: &Sprite, index: u8, palette: &Palette) -> Option<[u8; 4]> {
    let c = palette.colours[usize::from(index)];
    let rgb = [c.r, c.g, c.b];
    let band = index >= CONTROL_FIRST;

    match (sprite.mode, band) {
        _ if index == shade::TRANSPARENT => None,
        (Mode::Translucent | Mode::Other(8), _) => {
            blend::alphas(sprite.mode, sprite.level, palette)
                .map(|a| a[usize::from(index)])
                .filter(|a| *a > 0)
                .map(|a| [c.r, c.g, c.b, a])
        }
        (Mode::Plain, _) | (_, false) => Some(opaque(rgb)),
        // Draw mode 1: all eight darken; mode 7: four darken, four glow.
        (Mode::Shadow | Mode::Band, true) => shade::by_mode(sprite.mode, index).map(mark),
        (Mode::Other(_), true) => None,
    }
}

/// A shade as ink with nothing under it: black at soft's share, white at
/// what it lightens by.
fn mark(shade: Shade) -> [u8; 4] {
    let alpha = |a: u32| u8::try_from(a.min(255)).unwrap_or(u8::MAX);

    match shade {
        Shade::Keep(_) => [0, 0, 0, soft::veil(shade, Blending::Encoded).unwrap_or(0)],
        Shade::Raise(steps) => [255, 255, 255, alpha(steps * 255 / LEVELS)],
    }
}

const fn opaque([r, g, b]: [u8; 3]) -> [u8; 4] {
    [r, g, b, u8::MAX]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shadow_with_nothing_under_it_is_black_at_softs_share() {
        assert_eq!(mark(Shade::Keep(50)), [0, 0, 0, 132]);
        assert_eq!(mark(Shade::Keep(92)), [0, 0, 0, 30]);
        assert_eq!(mark(Shade::Keep(100)), [0, 0, 0, 0]);
        assert_eq!(mark(Shade::Raise(LEVELS)), [255, 255, 255, 255]);
    }

    #[test]
    fn half_over_clear_keeps_its_colour() {
        let mut clear = [0, 0, 0, 0];

        blend_over(&mut clear, [200, 100, 50, 128]);
        assert_eq!(clear, [200, 100, 50, 128]);

        let mut white = [255, 255, 255, 255];

        blend_over(&mut white, [0, 0, 0, 255]);
        assert_eq!(white, [0, 0, 0, 255]);
    }
}
