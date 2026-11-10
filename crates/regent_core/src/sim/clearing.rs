//! What a placed unit deletes around itself. **Binary** unless marked.
//!
//! `GSUnit` runs building clear
//! when its flags hold bit 0: two passes of
//! Building clear pass, each over a mask of half
//! cells about the unit, deleting occupants of the collision grid.
//!
//! ```text
//!  pass 0  mask GenUnit      (0): the unit's own pad (placement::shape)
//!          deletes a building ruined or with no agent, and AVk1-3
//!  pass 1  mask GSUnit      (margin) -> GSUnit:        a selection ring
//!          deletes scatter, the same buildings, and an emptied BBt chest
//!
//!  the ring, GSUnit        in GSUnit:
//!    R = pad rect grown by margin, times 16      GSUnit
//!    art AVn8, set 8: the first slot whose block 0 is at least as wide
//!    as R, else the last                          loop in GSUnit
//!    its point list 0, y unprojected              IsoFootprintRectManager
//!    moved so block 0's centre is R's             GSUnit
//!    rasterised like a pad                        FootprintRectManager
//! ```
//!
//! Who asks, with what margin and flags (`BuildingRec` and
//! `VehicleRec` pass theirs to `GSUnit`):
//!
//! | Caller | Margin | Flags |
//! |---|---|---|
//! | unit chart place, every chart unit | [`MARGIN`] | 1 |
//! | the build menu, `CREATE_BUILDING_MESSAGE` | [`MARGIN`] | 3 |
//! | `SpawnUnit` on a character | — | 8: no clearing |
//!
//! A deletion is deferred: `GE_Level` sets unit
//!  bit 2 and queues the unit, so a pass sees the grid unchanged.
//! Neither pass touches the ground or draws a random number.

use crate::data::fourcc::FourCc;
use crate::data::imag;

use super::placement::{self, Pad};
use super::units::PIXELS_PER_HALF;
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;

/// The margin every clearing caller passes, in half cells: `game.ini`'s
/// `ConstructData1`, default 8, written by construct data ctor. No
/// shipped file sets the key. Unit chart place reads it at
/// unit chart place; the build menu puts it in
/// the message, word `0x44`.
pub const MARGIN: i32 = 8;

/// The ring art: the level's image 3, `AVn8`,
/// taken while the UI placement state placement state is 0.
/// Otherwise image 4, `AVn9`, whose slots differ by a pixel.
/// **Inferred**: the state is 0 when a placement is decoded, since the
/// build menu sends only then.
pub const RING: FourCc = FourCc(*b"AVn8");

/// The ring set: `8` in `GSUnit`. Layer 0 only.
const RING_SET: u32 = imag::SetKind::Stand.id();

/// The set `IsoFootprintRectManager` falls back to for a slot's
/// points.
const RING_FALLBACK: u32 = imag::SetKind::Inactive.id();

/// Codes pass 0 deletes whatever they are: is avk.
pub const RESOURCES: [FourCc; 3] = [FourCc(*b"AVk1"), FourCc(*b"AVk2"), FourCc(*b"AVk3")];

/// The code prefix `GSUnit` tests: `BBt`, the
/// treasure chests, cleared once their hit points are 0.
pub const CHEST: [u8; 3] = *b"BBt";

/// One of building clear pass's two passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pass {
    /// Pass 0, over the unit's own pad.
    Under,
    /// Pass 1, over the ring.
    Around,
}

/// When the clearing runs: engine flag `0x40` is set during
/// world generate charts, and spares buildings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Time {
    Generating,
    Playing,
}

/// What an occupant is, as the passes ask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Unit kind 2, a terrain feature: `GenUnit`.
    Scatter,
    /// Unit kind 0: `GenUnit`.
    Building,
    Other,
}

/// An occupant of the collision grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Occupant {
    pub kind: Kind,
    pub code: FourCc,
    /// Unit state bit 4.
    pub ruined: bool,
    /// It holds an `AE` attribute, an agent (`BuildingRec`,
    /// `BuildingRec`).
    pub agent: bool,
    /// `APU:0`.
    pub hit_points: i32,
}

impl Occupant {
    /// Whether `pass` deletes it: building clear pass's test at
    /// building clear pass for pass 0, `GSUnit` for pass 1.
    #[must_use]
    pub fn cleared(&self, pass: Pass, time: Time) -> bool {
        // method(0), method, not engine flag 0x40.
        let spent =
            self.kind == Kind::Building && (self.ruined || !self.agent) && time == Time::Playing;
        let chest = self.code.0.starts_with(&CHEST) && self.hit_points == 0;

        match pass {
            Pass::Under => spent || RESOURCES.contains(&self.code),
            Pass::Around => self.kind == Kind::Scatter || spent || chest,
        }
    }
}

/// The half cells pass 1 covers about a unit placed with `margin`: the
/// ring `GSUnit` builds. Without the ring art, its fallback, the
/// unit's pad mask; without that, building clear pass's own, the
/// pad rect grown by `margin`, half-open. `None` without any.
///
/// # Errors
///
/// If the art will not parse.
pub fn ring(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    margin: i32,
) -> crate::data::error::Result<Option<Pad>> {
    ring_in(catalogue, unit, margin, ring_art(catalogue)?.as_ref())
}

/// The ring art, parsed: the same for every unit.
///
/// # Errors
///
/// If the art will not parse.
pub fn ring_art<'a>(
    catalogue: &Catalogue<'a>,
) -> crate::data::error::Result<Option<imag::Image<'a>>> {
    catalogue
        .find(b"IMAG", RING)
        .and_then(|at| catalogue.record(b"IMAG", at))
        .map(|r| imag::Image::parse(r.data))
        .transpose()
}

/// [`ring`] with the ring art (`ring_art`) given, to parse it once for
/// many units.
///
/// # Errors
///
/// If the art will not parse.
pub fn ring_in(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    margin: i32,
    image: Option<&imag::Image<'_>>,
) -> crate::data::error::Result<Option<Pad>> {
    // A missing pad rect reads as nothing; **Inferred**: no shipped unit
    // lacks one.
    let pad = placement::pad_rect(catalogue, unit)?;

    let shaped = image
        .zip(pad.map(|r| grow(r, margin)))
        .and_then(|(image, r)| fitted(image, r));

    match (shaped, image) {
        (Some(ring), _) => Ok(Some(ring)),
        (None, Some(_)) => Ok(None),
        (None, None) => Ok(match placement::polygon(unit) {
            Some(points) if points.len() >= 3 => Some(placement::rasterise(&points)),
            _ => pad.map(|[x0, y0, x1, y1]| {
                rect((x0 - margin, y0 - margin), (x1 + margin, y1 + margin))
            }),
        }),
    }
}

/// The rect the ring fits, ground pixels about the unit: the pad rect
/// grown by `margin` half cells, times 16. `GSUnit`;
/// also the build preview's ring
/// ([`crate::view::map::ring::Fit::Clearing`]). `None` without a pad rect.
///
/// # Errors
///
/// If the art will not parse.
pub fn reach(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    margin: i32,
) -> crate::data::error::Result<Option<[i32; 4]>> {
    Ok(placement::pad_rect(catalogue, unit)?.map(|r| grow(r, margin)))
}

/// Pad rect `r` grown by `margin`, in ground pixels.
fn grow([x0, y0, x1, y1]: [i32; 4], margin: i32) -> [i32; 4] {
    [x0 - margin, y0 - margin, x1 + margin, y1 + margin].map(|v| v * PIXELS_PER_HALF)
}

/// The ring slot for a rect `width` pixels wide, of slots whose block 0
/// is each of `blocks`: the first at least as wide, else the last
/// (`GSUnit`, the loop). The selection ring
/// fits so too. `None` without a slot.
pub fn ring_slot(blocks: impl IntoIterator<Item = [i16; 4]>, width: i32) -> Option<usize> {
    let blocks: Vec<[i16; 4]> = blocks.into_iter().collect();
    let wide = |b: &[i16; 4]| i32::from(b[2]) - i32::from(b[0]) >= width;

    blocks.iter().position(wide).or(blocks.len().checked_sub(1))
}

/// The ring slot for `r`, ground pixels about the unit, moved and
/// rasterised.
fn fitted(image: &imag::Image<'_>, r: [i32; 4]) -> Option<Pad> {
    let slots = &image.sets.iter().find(|s| s.id == RING_SET)?.slots;
    let chosen = ring_slot(slots.iter().map(|s| s.footprint), r[2] - r[0])?;
    let b = slots.get(chosen)?.footprint.map(i32::from);
    let points = [RING_SET, RING_FALLBACK].iter().find_map(|id| {
        let set = image.sets.iter().find(|s| s.id == *id)?;
        let list = set.slots.get(chosen)?.point_lists().first()?;

        (!list.is_empty()).then(|| list.clone())
    })?;

    // Block 0 is screen pixels, R ground pixels: the engine mixes them.
    let dx = (r[0] + r[2] - b[0] - b[2]) / 2;
    let dy = (r[1] + r[3] - b[1] - b[3]) / 2;
    let moved: Vec<(i32, i32)> = points
        .into_iter()
        .map(|(x, y)| {
            let (gx, gy) = placement::unproject(x, y);

            (gx + dx, gy + dy)
        })
        .collect();

    Some(placement::rasterise(&moved))
}

/// A full half-open rect of half cells, as a pad.
fn rect(low: (i32, i32), high: (i32, i32)) -> Pad {
    let size = (
        (high.0 - low.0).max(0).cast_unsigned(),
        (high.1 - low.1).max(0).cast_unsigned(),
    );

    Pad {
        origin: low,
        size,
        covered: vec![true; (size.0 * size.1) as usize],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first slot as wide, else the last; none without a slot.
    #[test]
    fn a_ring_takes_the_first_slot_as_wide_else_the_last() {
        let blocks = [[-10, 0, 10, 0], [-20, 0, 20, 0], [-30, 0, 30, 0]];

        assert_eq!(ring_slot(blocks, 20), Some(0));
        assert_eq!(ring_slot(blocks, 21), Some(1));
        assert_eq!(ring_slot(blocks, 99), Some(2));
        assert_eq!(ring_slot([], 1), None);
    }
}
