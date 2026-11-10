//! The ring drawn under a selected unit. **Binary** unless marked.
//!
//! A sprite, not a drawn shape: one of four level images, picked once a
//! level is made, each 13
//! `Stand` slots of growing ellipses. The ring draw draws
//! it from
//! `BuildingRec`, `VehicleRec`, `RewardFlagRec` and
//! `ConstructionDummyRec`; scatter gets none.
//!
//! ```text
//!  R     by default the unit's own rect:
//!        set 8 slot 0 block 0, else set 0xd0 slot 0 block 1,
//!        else set 0x40 slot 0 block 2, else the frame boxes of set 8,
//!        else of set 0x40; screen pixels about the anchor. With none
//!        of these R stays empty, and IsoGenUnit fills it with
//!        the box the piece is drawn in: its tiles, bar and name
//!        (Drawn; the Creature Den)
//!  slot  the first Stand slot whose block 0 is at least as wide as R,
//!        else the last
//!  at    the slot's frame 0, its anchor on the unit's anchor plus R's
//!        centre, x0 + (x1 - x0) / 2
//! ```
//!
//! The same fit `sim::clearing` uses for the ring a placed building
//! clears, on another image. The tiles are indices 251 to 254 only, which
//! the world blit draws as a glow ([`crate::view::raster::shade::Shade::Raise`]).
//!
//! R is the caller's choice ([`Fit`]): the build preview, a
//! `ConstructionDummyRec`, fits its clearing reach; in build mode a
//! standing building its pad.
//!
//! The fill happens at the first draw with R empty, before any
//! selection; R then stays (unit change type empties it again).
//! **Capture** (`ui_creature_den_selected.png`): the den's ring is slot 3,
//! 4 left of and 59 under its anchor, as its tiles, bar and name give.
//! **Inferred**: the box is taken at view zoom 1; at another zoom the
//! engine's would be scaled.
//!
//! The unit under the cursor draws [`HOVER`] when [`highlights`] takes
//! it: in the select and spell modes (`view::map::cursor`).
//!
//! Unsettled: whether the ring draws before or after the unit's body.

pub mod band;

use crate::data::desc::{self, BuildingFlag, CharacterFlag};
use crate::data::error::Result;
use crate::data::fourcc::FourCc;

use super::cursor::Flagged;
use super::{bar, label};
use crate::content::imagery::{self, Animation, FrameSource};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::sim::footprint::{self, ArtRect};
use crate::sim::{clearing, placement};
use crate::view::ScreenRect;
use crate::view::panel::portrait;
use crate::view::sprite::Sprite;

/// The set every ring image draws from: `Stand`.
pub const SET: u32 = 0x08;

/// Which level image a unit's ring takes, by ring draw's test, in
/// its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Look {
    /// `AVn2 flag_brackets`, level image 2: a unit of kind 4,
    /// a reward flag (`GS_Level` makes a `RewardFlagRec`
    /// for kind 4). One slot, four bracket corners; none under the
    /// cursor, as [`highlights`] takes no flag.
    Flag,
    /// `AVn8`, image 3, green: the build preview while its spot is free,
    /// or the unit under the cursor while the
    /// cursor object's is clear.
    Fits,
    /// `AVn9`, image 4, red: the build preview on a blocked spot,
    /// or the unit under the cursor while
    ///  is set.
    Blocked,
    /// `AVn1 selection_ring`, image 1: the selected unit, unit state
    /// bit 0 (a save's `crate::sim::world::piece_state::SELECTED`).
    Selected,
}

/// Why a unit draws a ring: ring draw fits one to a unit's own
/// rect only while selected or under the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Selected,
    Hovered,
}

impl Look {
    /// The look ring draw takes: a reward flag's is [`Look::Flag`]
    /// whatever asks (ring draw, tested first); else the selection's,
    /// or the hover's ([`HOVER`]). **Capture** (`flag_selected.png`): a
    /// selected attack flag in `AVn2`'s four brackets, its one slot's
    /// anchor on the flag's plus its rect's centre.
    #[must_use]
    pub const fn of(flag: Flagged, why: Why) -> Look {
        match (flag, why) {
            (Flagged::Yes, _) => Look::Flag,
            (Flagged::No, Why::Selected) => Look::Selected,
            (Flagged::No, Why::Hovered) => HOVER,
        }
    }

    /// The `IMAG` record.
    #[must_use]
    pub const fn image(self) -> FourCc {
        match self {
            Look::Flag => FourCc(*b"AVn2"),
            Look::Fits => FourCc(*b"AVn8"),
            Look::Blocked => FourCc(*b"AVn9"),
            Look::Selected => FourCc(*b"AVn1"),
        }
    }
}

/// The ring under the unit the cursor is over, while not selected:
/// `AVn8`. The ring draw takes `AVn9` only while the cursor object's
/// flag is set, and both callers of its setter pass 0.
/// A selected unit keeps `AVn1`; a unit
/// draws one ring.
pub const HOVER: Look = Look::Fits;

/// Whether a unit stands, for the cursor's tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stand {
    Standing,
    /// A ruin, or a mobile lying dead (`GenUnit` 3 or more).
    Fallen,
}

/// Whether the cursor's ring takes `unit`: can highlight.
/// A standing building with `CanHighlight`, which only the
/// shrines `CB01` and `CB02` carry; a character with it, which every
/// mobile carries but resource piles, markers and spell effects.
#[must_use]
pub fn highlights(unit: &desc::Unit<'_>, stand: Stand) -> bool {
    match (unit.building(), unit.character()) {
        (Some(b), _) => stand == Stand::Standing && b.has(BuildingFlag::CanHighlight),
        (None, Some(c)) => c.has(CharacterFlag::CanHighlight),
        _ => false,
    }
}

/// A unit's ring: the slot, its picture, and where its anchor goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    pub look: Look,
    /// The `Stand` slot fitted to the unit.
    pub slot: usize,
    /// Frame 0 of that slot.
    pub frame: FrameSource,
    /// Where the ring's tile anchor goes, screen pixels from the unit's
    /// anchor at scale 1.
    pub at: (i32, i32),
    /// The slot's block 0, pixels about the ring's anchor.
    pub block: ArtRect,
}

impl Ring {
    /// The ring's rect, pixels about the unit's anchor: its slot's block
    /// 0 at [`Ring::at`]. A selected or
    /// hovered unit's box takes it (`view::map::hit`).
    #[must_use]
    pub fn rect(&self) -> [i32; 4] {
        let [x0, y0, x1, y1] = self.block.map(i32::from);
        let (x, y) = self.at;

        [x0 + x, y0 + y, x1 + x, y1 + y]
    }

    /// The ring's picture: a world sprite, all glow.
    ///
    /// `None` if the frame is not a present tile.
    ///
    /// # Errors
    ///
    /// If the tile will not decode.
    pub fn sprite(&self, catalogue: &Catalogue<'_>) -> Option<Result<Sprite>> {
        match self.frame {
            FrameSource::Tile { archive, index, .. } => Sprite::at(catalogue, archive, index),
            _ => None,
        }
    }

    /// The ring drawn as a shape, fitted to its picture ([`band`]),
    /// about the unit's anchor. `None` for the flag's brackets, or
    /// where the frame is not a present tile.
    ///
    /// # Errors
    ///
    /// If the tile will not decode.
    pub fn band(&self, catalogue: &Catalogue<'_>) -> Option<Result<band::Band>> {
        self.sprite(catalogue).and_then(|sprite| match sprite {
            Ok(sprite) => band::Band::fit(&sprite, self.look).map(|b| Ok(self.about_unit(b))),
            Err(e) => Some(Err(e)),
        })
    }

    /// `band`, fitted about the tile's anchor, about the unit's.
    #[allow(clippy::cast_precision_loss, reason = "screen pixels")]
    fn about_unit(&self, band: band::Band) -> band::Band {
        let (x, y) = self.at;

        band::Band {
            centre: (band.centre.0 + x as f32, band.centre.1 + y as f32),
            ..band
        }
    }
}

/// Which rect R a ring fits: the caller's method passes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fit {
    /// The unit's own rect, [`rect`]: ring draw's default, for a
    /// selected unit or the one under the cursor.
    Own,
    /// A building's pad in ground pixels, `PRIM` `+0x04`
    /// ([`placement::pixel_rect`]): `BuildingRec`
    /// passes it for a standing building (`GenUnit` under 3) while
    /// build mode is set, so every building rings as one places.
    Pad,
    /// The build preview's: its pad grown by `ConstructData1` half cells,
    /// in ground pixels ([`clearing::reach`], [`clearing::MARGIN`]).
    /// `ConstructionDummyRec` passes it; the preview
    /// is that class's only unit.
    Clearing,
    /// [`Fit::Own`], an empty unit rect filled by the box the piece was
    /// drawn in ([`Drawn`]).
    Drawn(Drawn),
}

/// Whether a piece's hit point bar is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HpBar {
    Shown,
    Hidden,
}

/// The box `IsoGenUnit` fills an empty unit rect
/// with, pixels about the anchor: the piece as drawn, unselected and
/// unhovered. Its tiles with the anchor,
/// grown by what `GSUnit` gives where that has
/// area: the bar's outer rect while shown, then the label's box while
/// `DisplayFlagNames` is set or the unit is a reward flag (unit kind
/// 4), each by rect union.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Drawn {
    /// [`super::hit::bounds`] of the frames shown.
    pub tiles: Option<ScreenRect>,
    /// [`bar::Bar::outer`].
    pub bar: Option<ScreenRect>,
    /// [`label::below`] for a name, [`label::placed`] for a reward.
    pub name: Option<ScreenRect>,
}

/// The text `GSUnit` boxes: a unit's name under its art, or a
/// reward flag's amount over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Words<'t> {
    Name(&'t str),
    Reward(&'t str),
}

impl Drawn {
    /// `unit`'s, drawn as `tiles`, its bar as `bar`, and `words` its
    /// label's text while shown.
    ///
    /// # Errors
    ///
    /// If a tile the pad's rect needs, or the font, will not parse.
    pub fn of(
        catalogue: &Catalogue<'_>,
        unit: &Unit<'_>,
        tiles: Option<ScreenRect>,
        bar: HpBar,
        words: Option<Words<'_>>,
    ) -> Result<Drawn> {
        // The outer rect does not hang on health.
        let bar = match (bar, bar::sized(catalogue, unit)?) {
            (HpBar::Shown, Some((r, body))) => Some(bar::place(r, body, FULL).outer()),
            _ => None,
        };
        let size = match words {
            Some(Words::Name(text) | Words::Reward(text)) => label::measure(catalogue, text)?,
            None => None,
        };
        let name = match (words, size) {
            (Some(Words::Name(_)), Some(size)) => label::below(catalogue, unit, size)?,
            (Some(Words::Reward(_)), Some(size)) => label::placed(catalogue, unit, size)?,
            _ => None,
        };

        Ok(Drawn { tiles, bar, name })
    }

    /// `GSUnit`'s rect: bar and label, taken only with area.
    #[must_use]
    pub fn shown(&self) -> Option<ScreenRect> {
        [self.bar, self.name]
            .into_iter()
            .flatten()
            .fold(None, grown)
            .filter(|r| area(*r))
    }

    /// The rect filled; `None` without area.
    #[must_use]
    pub fn rect(&self) -> Option<ArtRect> {
        let drawn = [self.tiles, self.shown()]
            .into_iter()
            .flatten()
            .fold(None, grown);

        drawn.filter(|r| area(*r)).and_then(narrow)
    }
}

/// Any health: [`bar::Bar::outer`] does not read it.
const FULL: portrait::Health = portrait::Health::of(1, 1);

/// Rect union: `to` grown to cover `r`, or `r` where `to` has no
/// area, whatever `r`'s.
#[allow(
    clippy::unnecessary_wraps,
    reason = "a fold over an empty start, as the engine's zeroed rect"
)]
fn grown(to: Option<ScreenRect>, r: ScreenRect) -> Option<ScreenRect> {
    match to {
        Some(to) if area(to) => Some(super::hit::union(to, r)),
        _ => Some(r),
    }
}

/// Whether `r` has area.
const fn area(r: ScreenRect) -> bool {
    r[0] < r[2] && r[1] < r[3]
}

/// `unit`'s ring under `look`, fitted to its own rect. `None` without the
/// image.
///
/// # Errors
///
/// If the ring image or a tile R needs will not parse.
pub fn ring(catalogue: &Catalogue<'_>, unit: &Unit<'_>, look: Look) -> Result<Option<Ring>> {
    fitted(catalogue, unit, look, Fit::Own)
}

/// `unit`'s ring under `look`, fitted to the rect `by` names. `None`
/// without the image, or for [`Fit::Pad`] and [`Fit::Clearing`] without a
/// rect; [`Fit::Own`] fits an empty unit rect as zero, as the binary
/// before its first draw.
///
/// # Errors
///
/// If the ring image or a tile R needs will not parse.
pub fn fitted(
    catalogue: &Catalogue<'_>,
    unit: &Unit<'_>,
    look: Look,
    by: Fit,
) -> Result<Option<Ring>> {
    let Some(at) = catalogue.find(b"IMAG", look.image()) else {
        return Ok(None);
    };
    let sets = imagery::sets(catalogue, at)?;
    let r = match by {
        // unit rect build leaves an empty rect zeroed.
        Fit::Own => Some(rect(catalogue, unit)?.unwrap_or_default()),
        Fit::Drawn(drawn) => Some(
            rect(catalogue, unit)?
                .or_else(|| drawn.rect())
                .unwrap_or_default(),
        ),
        Fit::Pad => placement::pixel_rect(catalogue, unit)?.and_then(narrow),
        Fit::Clearing => clearing::reach(catalogue, unit, clearing::MARGIN)?.and_then(narrow),
    };

    Ok(r.and_then(|r| fit(&sets, r, look)))
}

/// A ground pixel rect as an [`ArtRect`]; `None` past `i16`.
fn narrow(r: [i32; 4]) -> Option<ArtRect> {
    let [x0, y0, x1, y1] = r.map(i16::try_from);

    Some([x0.ok()?, y0.ok()?, x1.ok()?, y1.ok()?])
}

/// The ring slot for `r` on one image, and its placement.
fn fit(sets: &[Animation], r: ArtRect, look: Look) -> Option<Ring> {
    let slots = &sets.iter().find(|a| a.id == SET)?.poses;
    let width = i32::from(r[2]) - i32::from(r[0]);
    let slot = clearing::ring_slot(slots.iter().map(|p| p.footprint), width)?;
    let pose = slots.get(slot)?;
    let frame = *pose.frames.first()?;
    let [x0, y0, x1, y1] = r.map(i32::from);

    Some(Ring {
        look,
        slot,
        frame,
        at: (x0 + (x1 - x0) / 2, y0 + (y1 - y0) / 2),
        block: pose.footprint,
    })
}

/// The unit's own rect at unit, screen pixels about its anchor:
/// Unit rect build. `None` where every source is empty.
///
/// Not [`footprint::of_unit`]: that asks block 0 of sets 8 and `0xd0`, this
/// block 0, 1 and 2 of sets 8, `0xd0` and `0x40`, and unions set 8 then
/// `0x40`, skipping `0xd0`.
///
/// # Errors
///
/// If a tile the union needs will not parse.
pub fn rect(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> Result<Option<ArtRect>> {
    match BLOCKS
        .iter()
        .find_map(|(set, block)| block_of(unit, *set, *block))
    {
        Some(found) => Ok(Some(found)),
        None => UNIONS.iter().try_fold(None, |found, set| match found {
            Some(r) => Ok(Some(r)),
            None => footprint::union(catalogue, unit, *set),
        }),
    }
}

/// Unit rect build's three slot block asks: set, block, all
/// on slot 0.
const BLOCKS: [(u32, u32); 3] = [(0x08, 0), (0xd0, 1), (0x40, 2)];

/// Its two frame box union asks.
const UNIONS: [u16; 2] = [0x08, 0x40];

/// Block `block` of slot 0 of layer 0 of `set`, when it has area.
///
/// Slot block compares the whole set id: layer 0 only. A block
/// whose mask bit is clear reads as zero, so it fails the test.
fn block_of(unit: &Unit<'_>, set: u32, block: u32) -> Option<ArtRect> {
    let pose = unit
        .animations
        .iter()
        .find(|a| a.id == set)?
        .poses
        .first()?;
    let r = if block == 0 {
        pose.footprint
    } else {
        pose.attachments
            .iter()
            .find(|(bit, _)| *bit == block)
            .map(|(_, r)| *r)?
    };

    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::imagery::Pose;

    fn pose(footprint: ArtRect) -> Pose {
        Pose {
            footprint,
            x: 0,
            y: 0,
            per_stream: 1,
            streams: 1,
            frames: vec![FrameSource::Sentinel(-1)],
            nudges: vec![(0, 0)],
            attachments: vec![],
            effects: vec![],
        }
    }

    fn rings() -> Vec<Animation> {
        vec![Animation {
            id: SET,
            kind: crate::data::imag::SetKind::Stand,
            variant: 0,
            poses: vec![pose([-19, -10, 21, 11]), pose([-41, -28, 40, 23])],
            playback: 0,
            nudge: 0,
            sort: crate::content::imagery::Sort::WithBase,
            passes: vec![crate::data::imag::Pass::First],
        }]
    }

    #[test]
    fn the_first_slot_as_wide_as_the_unit_is_its_ring() {
        let ring = fit(&rings(), [-15, -9, 10, 12], Look::Selected).expect("a ring");

        assert_eq!(ring.slot, 0);
        assert_eq!(ring.at, (-3, 1));
        // Block 0 [-19, -10, 21, 11] at (-3, 1).
        assert_eq!(ring.rect(), [-22, -9, 18, 12]);
    }

    /// A flag's ring is its brackets, selected or not; anyone else's the
    /// selection's or the hover's.
    #[test]
    fn a_reward_flag_rings_in_brackets() {
        assert_eq!(Look::of(Flagged::Yes, Why::Selected), Look::Flag);
        assert_eq!(Look::of(Flagged::Yes, Why::Hovered), Look::Flag);
        assert_eq!(Look::of(Flagged::No, Why::Selected), Look::Selected);
        assert_eq!(Look::of(Flagged::No, Why::Hovered), HOVER);
    }

    /// `GSUnit`'s rect counts only with area: a nameless label
    /// alone adds nothing, beside a bar it reaches down.
    #[test]
    fn a_nameless_label_grows_the_drawn_box_only_beside_a_bar() {
        let drawn = Drawn {
            tiles: Some([-10, -10, 10, 10]),
            bar: None,
            name: Some([0, 5, 0, 20]),
        };

        assert_eq!(drawn.rect(), Some([-10, -10, 10, 10]));

        let barred = Drawn {
            bar: Some([-5, 8, 5, 12]),
            ..drawn
        };

        assert_eq!(barred.rect(), Some([-10, -10, 10, 20]));
    }

    #[test]
    fn a_unit_wider_than_every_slot_takes_the_last() {
        let ring = fit(&rings(), [-100, -7, 100, 0], Look::Selected).expect("a ring");

        assert_eq!(ring.slot, 1);
        assert_eq!(ring.at, (0, -4));
    }
}
