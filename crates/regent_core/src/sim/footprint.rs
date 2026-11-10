//! The rect an object's art says it stands on.
//!
//! **Binary** — `IsoFootprintRectManager`. The generator's
//! scatter pass asks for a rect per object code and places the object only
//! where the rect is free ground; see `crate::sim::mapgen::mobject::Scatter::footprint`.
//!
//! Here, not in the generator: the rect is the first four `i16` of a slot, a
//! fact about the `.cam` art.
//!
//! # The chain
//!
//! ```text
//! set 8 (Stand)     slot 0's own rect, if left < right and top < bottom
//!   else set 0xd0 (Inactive)  the same test, for a building with no Stand
//!     else the union of set 8's frame boxes
//!       else set 0xd0's
//!         else set 0x40 (Special)'s
//! ```
//!
//! Two separate passes. Set `0x40`'s slot rect is never read: the
//! slot rect lookups in `IsoFootprintRectManager` ask for 8 and
//! `0xd0` only; the three frame box union calls after them ask for 8,
//! `0xd0` and `0x40`. So an object with only a `Special` set is measured by its
//! pictures — three of the 79 codes the scatter chart can place.
//!
//! A tile's box is its size about its anchor
//! ([`crate::data::tile::Tile::anchor`]). The union
//! takes a subset of the set's pictures: see `union`.
//!
//! # Pixels, converted by the caller
//!
//! The rect is screen pixels about the anchor. The generator converts to cells
//! (`crate::sim::mapgen::mobject::reach`): camera scales top and bottom, then all
//! divide by 32, both truncating toward zero. Rounding matters: one of the
//! fourteen codes a single land scatters changes cell over a quarter pixel.

use crate::data::error::Result;
use crate::data::fourcc::FourCc;
use crate::data::tile::Tile;

use crate::content::imagery::{Animation, FrameSource};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;

/// Sets whose slot 0 rect is tried, in order.
///
/// **Binary** — the two slot block calls in
/// `IsoFootprintRectManager`. 8 is `Stand`, 0xd0 `Inactive`; most scatter
/// objects carry only `Stand`.
pub const RECTS: [u16; 2] = [0x08, 0xd0];

/// Sets whose frame boxes are unioned when no slot rect fits, in order.
///
/// **Binary** — the three frame box union calls. 0x40 is `Special`, not
/// in [`RECTS`].
pub const UNIONS: [u16; 3] = [0x08, 0xd0, 0x40];

/// Masks the layer byte off a set id.
///
/// **Binary** — set layer at masks both sides, so a union walks every layer.
const LAYERLESS: u32 = 0x00ff_ffff;

/// Frame reference flags that exclude a picture from the union.
///
/// **Binary** — tests in frame box union and frame box union: reference bits 30, 27
/// and 28. Two HD sets change box under them.
const NOT_DRAWN: u16 = 0x4000 | 0x1000 | 0x0800;

/// A rect in pixels about a slot's anchor: left, top, right, bottom.
pub type ArtRect = [i16; 4];

/// Where an object's art says it stands. `None`, as in the engine, means its
/// single cell.
///
/// # Errors
///
/// If a tile the union fallback needs will not parse.
pub fn of_unit(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Result<Option<ArtRect>> {
    match RECTS.iter().find_map(|id| stands(unit, *id)) {
        Some(rect) => Ok(Some(rect)),
        None => UNIONS.iter().try_fold(None, |found, id| match found {
            Some(rect) => Ok(Some(rect)),
            None => union(catalogue, unit, *id),
        }),
    }
}

/// [`of_unit`] by descriptor id.
///
/// # Errors
///
/// As [`of_unit`], and if the `DUNT` will not parse.
pub fn of_code(catalogue: &Catalogue<'_>, code: FourCc) -> Result<Option<ArtRect>> {
    match Unit::find(catalogue, code) {
        None => Ok(None),
        Some(unit) => of_unit(catalogue, &unit?),
    }
}

/// Slot 0's rect on one set, if the engine's test accepts it.
///
/// **Binary** — slot block compares the whole set id:
/// layer 0 only.
fn stands(unit: &Unit<'_>, id: u16) -> Option<ArtRect> {
    let set = unit.animations.iter().find(|a| a.id == u32::from(id))?;
    let rect = set.poses.first()?.footprint;

    // The engine's test: nonzero area.
    (rect[0] < rect[2] && rect[1] < rect[3]).then_some(rect)
}

/// The box one set's pictures fill, if any.
///
/// **Binary** — the engine's frame box union. Unlike a plain slot
/// walk, it:
///
/// * takes only frame 0 of each stream of slot 0 (index `per_stream * stream`),
///   so a 23-frame animation gives one picture. The file packs the counts
///   dword as `(per_stream << 16) | streams`;
/// * adds the frame's nudge, [`crate::data::imag::Frame::nudge`];
/// * walks every layer, offsetting layers above the first by their pose
///   offset.
///
/// Dropping each changes the box on 237, 7 and 15 HD sets respectively. A
/// degenerate box counts as none: the engine's rect union overwrites
/// an empty accumulator.
pub(crate) fn union(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    id: u16,
) -> Result<Option<ArtRect>> {
    let mut found: Option<ArtRect> = None;

    for layer in layers(unit, id) {
        let Some(slot) = layer.poses.first() else {
            continue;
        };

        // Layer 0 draws on the anchor; higher layers (e.g. a building's fires)
        // at their pose offset.
        let (ox, oy) = if layer.variant == 0 {
            (0, 0)
        } else {
            (slot.x, slot.y)
        };

        for stream in 0..slot.streams {
            let at = usize::from(stream) * usize::from(slot.per_stream);
            let (dx, dy) = slot.nudges.get(at).copied().unwrap_or((0, 0));
            let Some(frame) = slot.frames.get(at) else {
                continue;
            };
            let off = (ox.saturating_add(dx), oy.saturating_add(dy));
            let Some(one) = box_of(catalogue, *frame, off)? else {
                continue;
            };

            found = Some(match found {
                Some(all) if all[0] < all[2] && all[1] < all[3] => [
                    all[0].min(one[0]),
                    all[1].min(one[1]),
                    all[2].max(one[2]),
                    all[3].max(one[3]),
                ],
                _ => one,
            });
        }
    }

    Ok(found.filter(|r| r[0] < r[2] && r[1] < r[3]))
}

/// Every layer of one set, in file order.
///
/// **Binary** — set layer at returns the n-th animation whose id matches
/// ignoring the layer byte: file order, not [`Animation::variant`].
fn layers<'a>(unit: &'a Unit<'_>, id: u16) -> impl Iterator<Item = &'a Animation> {
    unit.animations
        .iter()
        .filter(move |a| a.id & LAYERLESS == u32::from(id))
}

/// One frame's tile box about the anchor, shifted by `off`.
fn box_of(
    catalogue: &Catalogue<'_>,
    frame: FrameSource,
    off: (i16, i16),
) -> Result<Option<ArtRect>> {
    let FrameSource::Tile {
        archive,
        index,
        flags,
    } = frame
    else {
        return Ok(None);
    };

    if flags & NOT_DRAWN != 0 {
        return Ok(None);
    }

    let Some(record) = catalogue.positional(archive, b"TILE", index) else {
        return Ok(None);
    };

    // On disk in a game without a screen (`cam::Defer::Pixels`).
    let bytes = record.bytes()?;
    let tile = Tile::parse(&bytes)?;
    let x = off.0.saturating_sub(tile.anchor().0);
    let y = off.1.saturating_sub(tile.anchor().1);

    Ok(Some([
        x,
        y,
        x.saturating_add(tile.width().cast_signed()),
        y.saturating_add(tile.height().cast_signed()),
    ]))
}
