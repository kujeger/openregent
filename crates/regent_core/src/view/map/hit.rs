//! Which piece a screen point is over: the iso view's unit pick,
//! `IsoGameView`. **Binary** unless marked.
//!
//! ```text
//!  the select mode's cursor, hover and click, a unit spell's aim and an
//!  attack flag's target all ask the unit pick with mask 0x80000030
//!  and reach 4, then a clone's leader (APU:8) or a hung piece's holder
//!
//!        the drawn pieces, front to back (the draw list's tail first)
//!    box     the piece's screen box grown by REACH
//!    picture a tile pixel of its current frame
//!  ─> the first met on its picture             0x38   Hit::Picture
//!     else the last met in its box             0x18   Hit::Box
//! ```
//!
//! Here the list leaves out what [`solid`] refuses, so a click reaches the
//! unit under a spell effect.
//!
//! The draw list holds only seen pieces.
//! **Inferred**: no held one, as none draws. Scatter is skipped (mask
//! bit 2 clear).
//!
//! What a caller takes: the select mode's pointing hand and click want a
//! picture hit or a character ([`super::cursor::over`]); the hover ring
//! any hit can highlight takes; a unit spell any hit; an attack
//! flag a picture hit or a character.
//!
//! The box is the union of the current frame's tiles
//! over every layer and the anchor, then the
//! ring's rect while the piece is selected or hovered,
//! then `GSUnit`'s bar and label rect (`ring::Drawn::shown`).

use crate::data::desc::{self, UnitKind};
use crate::data::error::Result;
use crate::data::imag;
use crate::data::tile::Tile;

use super::cursor::Hit;
use super::tile_hit;
use crate::content::imagery::{self, Animation, FrameSource, Pose};
use crate::data::catalogue::Catalogue;
use crate::view::ScreenRect;

/// How far outside its box a point still meets a piece, screen pixels:
/// `0x04` in pick callback, every caller's.
pub const REACH: i32 = 4;

/// Whether the pick stops at `unit`. Not at a character with no panel: a
/// spell effect, a marker (selection panel of gives 0, so
/// unit select maybe does nothing); nor at a projectile, overlay
/// other than a reward flag, or particle system (pick kind ok
/// refuses them).
///
/// A departure: `IsoGameView` stops at them all, and its callers
/// then refuse them, so a unit beneath is out of reach (`docs/openregent/enhancements.md`).
/// A hung piece is its caller's matter: it picks its holder.
#[must_use]
pub fn solid(unit: &desc::Unit<'_>) -> bool {
    match unit.kind() {
        UnitKind::Character => unit.character().is_some_and(|c| c.dialog_id().is_some()),
        UnitKind::Overlay => unit.engine().category() == Some(REWARD_FLAG),
        UnitKind::Projectile | UnitKind::ParticleSystem => false,
        UnitKind::General | UnitKind::Building => true,
    }
}

/// `Category` on the reward flags, which are overlays and take a click.
const REWARD_FLAG: u32 = 1;

/// One drawn layer: its set, slot and frame (`UnitIMGPartEntry`).
#[derive(Debug, Clone, Copy)]
pub struct Layer<'u> {
    pub set: &'u Animation,
    pub slot: usize,
    pub frame: usize,
}

/// One frame a piece shows: its tile, top left in pixels from the piece's
/// anchor; `None` for a frame of another kind, which always meets.
#[derive(Debug, Clone)]
pub struct Part<'a> {
    pub tile: Option<Tile<'a>>,
    pub corner: (i32, i32),
}

impl<'a> Part<'a> {
    /// Tile `index` of `archive`, top left at `corner`. `None` without it.
    ///
    /// # Errors
    ///
    /// If the tile will not parse.
    pub fn tile(
        catalogue: &Catalogue<'a>,
        (archive, index): (usize, usize),
        corner: (i32, i32),
    ) -> Option<Result<Self>> {
        let record = catalogue.positional(archive, b"TILE", index)?;

        Some(Tile::parse(record.data).map(|tile| Part {
            tile: Some(tile),
            corner,
        }))
    }

    /// Its rect about the anchor; `None` for a frame of another kind.
    #[must_use]
    pub fn rect(&self) -> Option<ScreenRect> {
        let (x, y) = self.corner;

        self.tile
            .as_ref()
            .map(|t| [x, y, x + i32::from(t.width()), y + i32::from(t.height())])
    }

    /// Whether the pick meets it at `at`, pixels from the anchor.
    #[must_use]
    pub fn picks(&self, (x, y): (i32, i32)) -> bool {
        self.tile
            .as_ref()
            .is_none_or(|t| tile_hit::picks(t, x - self.corner.0, y - self.corner.1))
    }
}

/// The frames `layers` show, as unit hit lays them out: layer 0
/// at its slot (wrapped to the set), a higher layer at its own slot but
/// placed by the pose of the base slot scaled to its set's slots; every
/// stream at its frame's nudge less the tile's anchor. Hidden and
/// second-pass frames are skipped (reference bits 30, 24).
///
/// Then each layer's slot effects ([`imagery::effect`]), last entry
/// first, at the entry's offset. An effect part keeps its
/// own frame from 0; this takes the layer's, wrapped to the effect's
/// frames, so it is exact while both play from the set's start and the
/// frame is below both lengths.
///
/// **Inferred**: no part flags; every layer draws in the first pass.
///
/// # Errors
///
/// If a tile will not parse.
pub fn parts<'a>(catalogue: &Catalogue<'a>, layers: &[Layer<'_>]) -> Result<Vec<Part<'a>>> {
    let base = layers
        .first()
        .map(|l| (l.set.poses.len(), wrap(l.slot, l.set)));
    let slot_of = |nth: usize, layer: &Layer<'_>| {
        if nth == 0 {
            wrap(layer.slot, layer.set)
        } else {
            layer.slot
        }
    };

    // Each layer's frame references, at their nudges plus its offset.
    let frames = layers.iter().enumerate().flat_map(|(nth, layer)| {
        let offset = match (nth, base) {
            (1.., Some(base)) => offset(base, layer.set),
            _ => (0, 0),
        };
        let pose = layer
            .set
            .poses
            .get(slot_of(nth, layer))
            .filter(|p| layer.frame < usize::from(p.per_stream));

        pose.into_iter()
            .flat_map(move |p| shown(p, layer.frame, offset))
    });

    // Each layer's slot effects: set, slot, offset, frame.
    let effects: Vec<(Animation, usize, (i32, i32), usize)> = layers
        .iter()
        .enumerate()
        .filter_map(|(nth, layer)| {
            let slot = slot_of(nth, layer);

            layer.set.poses.get(slot).map(|p| (p, slot, layer.frame))
        })
        .flat_map(|(pose, slot, frame)| pose.effects.iter().rev().map(move |e| (e, slot, frame)))
        .filter_map(|(entry, slot, frame)| {
            let set = imagery::effect(catalogue, entry)?;
            let slot = if entry.fixed_slot() {
                entry.slot() as usize
            } else {
                slot
            };

            Some((set, slot, entry.offset(), frame))
        })
        .collect();

    let effect_frames = effects.iter().flat_map(|(set, slot, (x, y), frame)| {
        let pose = set.poses.get(*slot).filter(|p| p.per_stream > 0);

        pose.into_iter().flat_map(move |p| {
            let offset = (i32::from(p.x) + x, i32::from(p.y) + y);

            shown(p, frame % usize::from(p.per_stream), offset)
        })
    });

    frames
        .chain(effect_frames)
        .filter_map(|(frame, nudge)| part(catalogue, frame, nudge))
        .collect()
}

/// Every stream's frame `frame` of `pose`, each at its nudge plus
/// `offset`.
fn shown(
    pose: &Pose,
    frame: usize,
    offset: (i32, i32),
) -> impl Iterator<Item = (FrameSource, (i32, i32))> + '_ {
    (0..usize::from(pose.streams)).filter_map(move |stream| {
        let at = stream * usize::from(pose.per_stream) + frame;
        let nudge = pose.nudges.get(at).copied().unwrap_or_default();
        let nudge = (i32::from(nudge.0) + offset.0, i32::from(nudge.1) + offset.1);

        pose.frames.get(at).map(|f| (*f, nudge))
    })
}

/// Layer 0's slot: past the set's slots, wrapped.
const fn wrap(slot: usize, set: &Animation) -> usize {
    match set.poses.len() {
        0 => slot,
        n => slot % n,
    }
}

/// A higher layer's pose offset: the base slot scaled to `set`'s slots,
/// unit hit; none past them.
fn offset((count, slot): (usize, usize), set: &Animation) -> (i32, i32) {
    let own = set.poses.len();
    let mapped = if count == 0 {
        slot
    } else if count < own {
        own / count * slot
    } else if own != 0 && own < count {
        slot / ((count - 1 + own) / own)
    } else {
        slot
    };

    set.poses
        .get(mapped)
        .map_or((0, 0), |p| (i32::from(p.x), i32::from(p.y)))
}

/// The part one frame reference makes at `nudge`; `None` for none.
fn part<'a>(
    catalogue: &Catalogue<'a>,
    frame: FrameSource,
    nudge: (i32, i32),
) -> Option<Result<Part<'a>>> {
    match frame {
        FrameSource::Tile { flags, .. } if flags & SKIPPED != 0 => None,
        FrameSource::Tile { flags, .. } if flags & KIND != 0 => Some(Ok(Part {
            tile: None,
            corner: nudge,
        })),
        FrameSource::Tile { archive, index, .. } => {
            let record = catalogue.positional(archive, b"TILE", index)?;

            Some(Tile::parse(record.data).map(|tile| {
                let corner = (
                    nudge.0 - i32::from(tile.anchor().0),
                    nudge.1 - i32::from(tile.anchor().1),
                );

                Part {
                    tile: Some(tile),
                    corner,
                }
            }))
        }
        _ => None,
    }
}

/// Reference bits, as [`FrameSource::Tile`]'s `flags` hold them, under
/// which unit hit skips a frame: hidden, second pass.
const SKIPPED: u16 = ((imag::HIDDEN | imag::PASS) >> 16) as u16;

/// [`imag::KIND`] as [`FrameSource::Tile`]'s `flags` hold it: a frame
/// of another kind meets wherever its box does.
const KIND: u16 = (imag::KIND >> 16) as u16;

/// Whether the pick meets any of `parts` at `at`, pixels from the
/// anchor: unit hit.
#[must_use]
pub fn picks(parts: &[Part<'_>], at: (i32, i32)) -> bool {
    parts.iter().any(|p| p.picks(at))
}

/// The box `parts` cover with the anchor, pixels about it;
/// `None` with no tile.
#[must_use]
pub fn bounds(parts: &[Part<'_>]) -> Option<ScreenRect> {
    covering(parts.iter().filter_map(Part::rect))
}

/// The box tile rects `rects` cover with the anchor; `None` for none.
#[must_use]
pub fn covering(rects: impl IntoIterator<Item = ScreenRect>) -> Option<ScreenRect> {
    rects.into_iter().fold(None, |b: Option<ScreenRect>, r| {
        Some(union(b.unwrap_or([0; 4]), r))
    })
}

/// The rect covering both.
#[must_use]
pub fn union(a: ScreenRect, b: ScreenRect) -> ScreenRect {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// A box at view scale `scale`: at 0.5 halved and
/// grown a pixel each way, at 1 as is, else each edge scaled, truncated.
#[must_use]
#[allow(clippy::float_cmp, reason = "the engine compares exactly")]
#[allow(clippy::cast_possible_truncation, reason = "the engine's _ftol")]
#[allow(clippy::cast_precision_loss, reason = "screen pixels")]
pub fn screen_box(rect: ScreenRect, scale: f32) -> ScreenRect {
    let [x0, y0, x1, y1] = rect;

    if scale == 1.0 {
        rect
    } else if scale == HALF {
        [(x0 >> 1) - 1, (y0 >> 1) - 1, (x1 >> 1) + 1, (y1 >> 1) + 1]
    } else {
        rect.map(|v| (v as f32 * scale) as i32)
    }
}

/// The scale `IsoGameView` grows a box at.
const HALF: f32 = 0.5;

/// Whether `at` meets screen box `rect` within [`REACH`].
#[must_use]
pub const fn boxed(rect: ScreenRect, (x, y): (i32, i32)) -> bool {
    let [x0, y0, x1, y1] = rect;

    x0 - REACH <= x && x < x1 + REACH && y0 - REACH <= y && y < y1 + REACH
}

/// The piece the pick takes among `front_to_back`: the first `boxed` and
/// on its `picture`, else the last `boxed`. `IsoGameView` under the
/// mask every caller passes, `0x80000030` in pick callback: bit 31
/// returns the last box hit when no picture is hit; bit 2, scatter too, is
/// clear.
pub fn pick<T: Copy>(
    front_to_back: impl IntoIterator<Item = T>,
    boxed: impl Fn(T) -> bool,
    mut picture: impl FnMut(T) -> bool,
) -> Option<(T, Hit)> {
    // Err stops at a picture hit; Ok carries the last box hit.
    let walked = front_to_back
        .into_iter()
        .filter(|p| boxed(*p))
        .try_fold(None, |_, p| {
            if picture(p) {
                Err((p, Hit::Picture))
            } else {
                Ok(Some((p, Hit::Box)))
            }
        });

    match walked {
        Ok(last) => last,
        Err(hit) => Some(hit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_picture_hit_wins_else_the_last_box() {
        // Front to back: 1 boxed only, 2 boxed on its picture, 3 boxed.
        let boxed = |p: u32| p != 4;
        let on = |pictured: u32| move |p: u32| p == pictured;

        assert_eq!(pick([1, 2, 3, 4], boxed, on(2)), Some((2, Hit::Picture)));
        assert_eq!(pick([1, 2, 3, 4], boxed, on(9)), Some((3, Hit::Box)));
        assert_eq!(pick([4], boxed, on(4)), None);
    }

    #[test]
    fn a_box_reaches_four_pixels_out() {
        let rect = [10, 20, 30, 40];

        assert!(boxed(rect, (6, 16)));
        assert!(!boxed(rect, (5, 20)));
        assert!(boxed(rect, (33, 43)));
        assert!(!boxed(rect, (34, 30)));
    }

    #[test]
    fn a_box_at_half_scale_is_halved_and_grown_a_pixel() {
        assert_eq!(screen_box([10, -5, 31, 40], 0.5), [4, -4, 16, 21]);
        assert_eq!(screen_box([10, -5, 31, 40], 1.0), [10, -5, 31, 40]);
        assert_eq!(screen_box([10, -5, 31, 40], 2.0), [20, -10, 62, 80]);
    }

    #[test]
    fn a_frame_of_another_kind_always_meets() {
        let text = Part {
            tile: None,
            corner: (0, 0),
        };

        assert!(picks(std::slice::from_ref(&text), (500, -500)));
        assert_eq!(bounds(&[text]), None);
    }
}
