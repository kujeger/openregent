//! An `IMAG` record read as sets, slots and frame locations.
//!
//! All art is `IMAG`: unit animations and tileset terrain alike. Set holds
//! slots, slot holds frames, frame names a tile by position in the `IMAG`'s own
//! archive. So the walk is here, not in [`super::unit`].
//!
//! Resolves locations, not pixels: a frame is an archive and tile number;
//! [`crate::view::sprite::Sprite`] rasterises on demand. A catalogue-wide sweep can
//! then check every reference without rasterising 20,000 tiles.
//!
//! # Slot vocabulary
//!
//! Names are the unit ones. For a `MapTexture` set a [`Pose`] is a blend case
//! (which corners take the other terrain), with no facing or playback order.
//! Renaming would churn four crates; see [`crate::data::imag`].

use crate::base::fixed::{Fixed, ONE_F32};
use crate::data::error::{Error, Result};
use crate::data::imag::{self, Blit, FrameRef, Pass, SetKind};
use crate::data::settings::{ISO_CAMERA_SIN, TILE_PIXELS};

use crate::data::catalogue::{Catalogue, Located};

/// Where one animation frame's pixels are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSource {
    /// A present tile at `index` of `archive`'s `TILE` chunk.
    Tile {
        archive: usize,
        index: usize,
        /// The reference's upper 16 bits. Carried as-is; only the footprint
        /// union reads them (see [`crate::sim::footprint`]).
        flags: u16,
    },
    /// A tile number past the end of that archive's `TILE` chunk.
    Missing { archive: usize, index: usize },
    /// A small negative number of unknown meaning. See [`FrameRef::Sentinel`].
    Sentinel(i32),
    /// Upper bits outside the observed set.
    Unknown(u32),
}

/// Reference bits, as [`FrameSource::Tile`]'s `flags` hold them, under
/// which `IsoGenUnit` blits no tile: hidden ([`imag::HIDDEN`]), or
/// a kind other than a tile ([`imag::KIND`]).
const NO_TILE: u16 = ((imag::HIDDEN | imag::KIND) >> 16) as u16;

impl FrameSource {
    /// The tile `IsoGenUnit` blits for this frame, archive and
    /// index. **Binary**: none for a hidden frame or one
    /// of another kind: `Got_Gold`'s second layer is kind
    /// `0x08000000`, its amount as text.
    #[must_use]
    pub const fn tile(&self) -> Option<(usize, usize)> {
        match *self {
            FrameSource::Tile {
                archive,
                index,
                flags,
            } if flags & NO_TILE == 0 => Some((archive, index)),
            _ => None,
        }
    }

    /// Whether `IsoUnitIMG` draws this frame as text: kind
    /// [`imag::TEXT`], not hidden.
    #[must_use]
    pub const fn is_text(&self) -> bool {
        match *self {
            FrameSource::Tile { flags, .. } => flags & NO_TILE == TEXT,
            _ => false,
        }
    }
}

/// [`imag::TEXT`] as [`FrameSource::Tile`]'s `flags` hold it.
const TEXT: u16 = (imag::TEXT >> 16) as u16;

/// One slot of a set: a pose, facing, variant or blend case.
///
/// A `Walk` set's eight slots are facings; a rock's ten are variants; one
/// record has 32. See `docs/formats/art.md`.
#[derive(Debug, Clone)]
pub struct Pose {
    /// Standing rect in pixels about the anchor
    /// ([`crate::data::imag::Slot::footprint`]). All zero if none.
    pub footprint: [i16; 4],
    /// Offset of this layer from layer 0 of its set. Not a map placement:
    /// scatter matches the game only with it ignored.
    ///
    /// Of HD's 10,024 slots, 1,555 have a non-zero offset; 1,451 are `Die`,
    /// none on layer 0 — it places a building's fires. `Active` layer 1 uses it
    /// for the Rogues' Guild's pipe smoke.
    ///
    /// Apply above layer 0 only. 16 offsets sit on layer 0, none on scatter;
    /// their in-game effect is unchecked.
    ///
    /// Held by `a_pose_offset_places_a_stacked_layer`.
    pub x: i16,
    pub y: i16,
    /// Frames per stream, from the slot's count word (not derived from
    /// [`Pose::frames`]), so a mismatch is checkable.
    pub per_stream: u16,
    /// Parallel streams.
    pub streams: u16,
    /// `per_stream * streams` entries, stream-major, each stream in playback
    /// order.
    pub frames: Vec<FrameSource>,
    /// Per [`Pose::frames`] entry: offset from the anchor on top of the tile's
    /// own ([`crate::data::imag::Frame::nudge`]). Kept out of [`FrameSource`],
    /// which names only a location.
    pub nudges: Vec<(i16, i16)>,
    /// Attachment blocks with their mask bits, two points each
    /// ([`crate::data::imag::Slot::attachment_blocks`]). Vary per slot: the
    /// Ballista Tower's eight `Attack` slots share pictures but differ in
    /// points 4 and 5, a firing position.
    pub attachments: Vec<(u32, [i16; 4])>,
    /// Version 5's effect entries ([`crate::data::imag::Slot::effects`]):
    /// more parts a first-pass part of this slot carries, resolved by
    /// [`effect`].
    pub effects: Vec<imag::Effect>,
}

impl Pose {
    /// One stream's frames, in playback order. Empty past the end.
    pub fn stream(&self, stream: u16) -> &[FrameSource] {
        let width = usize::from(self.per_stream);
        let at = usize::from(stream) * width;

        self.frames.get(at..at + width).unwrap_or(&[])
    }

    /// The frames of one stream drawn as text, frame by frame: each one's
    /// nudge, where the text centres about the anchor;
    /// `None` for a frame not text.
    #[must_use]
    pub fn texts(&self, stream: u16) -> Vec<Option<(i16, i16)>> {
        let at = usize::from(stream) * usize::from(self.per_stream);

        self.stream(stream)
            .iter()
            .enumerate()
            .map(|(i, f)| {
                f.is_text()
                    .then(|| self.nudges.get(at + i).copied().unwrap_or_default())
            })
            .collect()
    }

    /// What a unit's part of `pass` blits of one stream, frame by frame:
    /// the tile and the frame's nudge; `None` for none.
    ///
    /// **Binary**, `IsoGenUnit`: at the part's frame, every stream
    /// is blitted, each at its frame's nudge (`+4`, `+6`)
    /// on top of the layer's pose, where the frame's bit 24 is the
    /// part's pass (`IsoGenUnit`, [`Pass`]).
    #[must_use]
    pub fn blits(&self, stream: u16, pass: Pass) -> Vec<Option<Blitted>> {
        let at = usize::from(stream) * usize::from(self.per_stream);
        let drawn = |f: &FrameSource| match f {
            FrameSource::Tile { flags, .. } if Pass::of(u32::from(*flags) << 16) == pass => {
                f.tile()
            }
            _ => None,
        };

        self.stream(stream)
            .iter()
            .enumerate()
            .map(|(i, f)| {
                drawn(f).map(|(archive, index)| Blitted {
                    archive,
                    index,
                    nudge: self.nudges.get(at + i).copied().unwrap_or_default(),
                })
            })
            .collect()
    }
}

/// A frame a part blits ([`Pose::blits`]): its tile, and the frame's
/// nudge in pixels, right and down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blitted {
    pub archive: usize,
    pub index: usize,
    pub nudge: (i16, i16),
}

/// One set: a unit action, or a terrain pair.
#[derive(Debug, Clone)]
pub struct Animation {
    /// The whole packed set id. [`Animation::kind`] is lossy: the Barbarian's
    /// three `Interface` sets differ only here.
    pub id: u32,
    pub kind: SetKind,
    /// Layer, from the set id's top byte.
    ///
    /// Repeated kinds draw together, lowest first; not alternatives. `ABJ1
    /// Palace1` `Active`: layer 0 building, layer 1 banner. See
    /// [`Unit::layers`](super::unit::Unit::layers).
    pub variant: u8,
    pub poses: Vec<Pose>,
    /// The set's playback word 2, 0 without one ([`imag::Set::playback`]):
    /// its frame time, as `sim::movement::action::frame_ms` reads it.
    pub playback: u32,
    /// Screen pixels its parts sort nearer ([`imag::Set::sort_nudge`]).
    pub nudge: i32,
    /// Whether its parts join the draw list apart from the unit's base.
    pub sort: Sort,
    /// The passes its parts draw, in part order ([`imag::Set::passes`]).
    /// **Inferred**: a set with neither flag (247 in HD: interface art,
    /// some effect and missile sets) as one first-pass part, though the
    /// unit draw would make it none.
    pub passes: Vec<Pass>,
}

/// How a layer's parts join the draw list (`IsoDrawList`,
/// `IsoDrawList`): by set layer flags bit 0 ([`imag::SORTS_OWN`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Drawn with part 0, the unit's base, at its place in the list.
    #[default]
    WithBase,
    /// A node of its own, its y nudged by [`Animation::nudge`]; the
    /// base's draw skips it (part flag `0x10`, `IsoGenUnit`).
    Own,
}

impl Animation {
    /// Screen pixels this layer's parts sort nearer, `base` its stack's
    /// layer 0: its own nudge where it sorts apart, else the base's.
    ///
    /// **Binary**, `IsoDrawList`: part 0 always joins the list, at
    /// its set's nudge; a higher part only with [`Sort::Own`]. So the
    /// Mausoleum's dirt diamond, `Inactive` layer 1 at -1,000, draws
    /// under the building.
    #[must_use]
    pub const fn listed(&self, base: &Animation) -> i32 {
        match self.sort {
            Sort::Own => self.nudge,
            Sort::WithBase => base.nudge,
        }
    }

    /// Which blit draws this set, and so what high palette indices mean.
    ///
    /// **Inferred** from set purpose; nothing on disk states it. `Interface` is
    /// panel art (portrait, panel picture, 25x25 icon); the rest is world. Of
    /// the base dataset's 31,250 referenced tiles, three are in both.
    #[must_use]
    pub const fn blit(&self) -> Blit {
        match self.kind {
            SetKind::Interface => Blit::Interface,
            _ => Blit::World,
        }
    }
}

/// Read an `IMAG` record, resolving frame references to locations.
///
/// # Errors
///
/// If there is no `IMAG` at `at`, or the record will not parse.
pub fn sets(catalogue: &Catalogue<'_>, at: Located) -> Result<Vec<Animation>> {
    let image = image(catalogue, at)?;

    Ok(animations(catalogue, at, &image))
}

/// The `IMAG` record at `at`, parsed.
///
/// # Errors
///
/// If there is no `IMAG` at `at`, or the record will not parse.
pub fn image<'a>(catalogue: &Catalogue<'a>, at: Located) -> Result<imag::Image<'a>> {
    let record = catalogue.record(b"IMAG", at).ok_or(Error::BadValue {
        what: "no IMAG record at this location",
        value: at.record as u64,
    })?;

    imag::Image::parse(record.data)
}

/// The sets of the `image` at `at` ([`sets`]), frame references resolved
/// to locations.
pub fn animations(
    catalogue: &Catalogue<'_>,
    at: Located,
    image: &imag::Image<'_>,
) -> Vec<Animation> {
    // Tile bound, in the IMAG's own archive.
    let tiles = tile_count(catalogue, at.archive);

    image
        .sets
        .iter()
        .map(|set| animation(set, at.archive, tiles))
        .collect()
}

/// The set an effect entry draws, where `UnitIMG` makes a part of
/// it: the entry is used ([`imag::Effect::enabled`]),
/// its `IMAG` and exact set
/// exist, and the set draws a first-pass part (layer flag `0x100`).
/// Its pose is [`imag::Effect::slot`] where the slot is fixed.
#[must_use]
pub fn effect(catalogue: &Catalogue<'_>, effect: &imag::Effect) -> Option<Animation> {
    let at = catalogue.find(b"IMAG", effect.image()?)?;
    let record = catalogue.record(b"IMAG", at)?;
    let image = imag::Image::parse(record.data).ok()?;
    let tiles = tile_count(catalogue, at.archive);

    image
        .sets
        .iter()
        .find(|set| set.id == effect.set())
        .filter(|set| set.layer_flags() & Pass::First.part_flag() != 0)
        .filter(|_| effect.enabled())
        .map(|set| animation(set, at.archive, tiles))
}

/// Tiles in `archive`'s `TILE` chunk: the bound a frame reference
/// resolves within.
fn tile_count(catalogue: &Catalogue<'_>, archive: usize) -> usize {
    catalogue
        .archives
        .get(archive)
        .and_then(|(_, a)| a.chunk_by_tag(b"TILE"))
        .map_or(0, |c| c.records.len())
}

/// One set of an `IMAG` in `archive`, its frames resolved against
/// `tiles`.
fn animation(set: &imag::Set<'_>, archive: usize, tiles: usize) -> Animation {
    Animation {
        id: set.id,
        kind: set.kind(),
        variant: set.variant(),
        poses: set
            .slots
            .iter()
            .map(|slot| Pose {
                footprint: slot.footprint,
                x: slot.x,
                y: slot.y,
                per_stream: slot.frames,
                streams: slot.streams,
                frames: slot
                    .frames()
                    .map(|f| locate(f.target(), archive, tiles))
                    .collect(),
                nudges: slot.frames().map(imag::Frame::nudge).collect(),
                attachments: slot.attachment_blocks().collect(),
                effects: slot.effects().to_vec(),
            })
            .collect(),
        playback: set.playback().map_or(0, |p| p.value),
        nudge: set.sort_nudge(),
        sort: if set.layer_flags() & imag::SORTS_OWN == 0 {
            Sort::WithBase
        } else {
            Sort::Own
        },
        passes: match set.passes().collect::<Vec<_>>() {
            none if none.is_empty() => vec![Pass::First],
            some => some,
        },
    }
}

/// Resolve one frame reference.
const fn locate(target: FrameRef, archive: usize, tiles: usize) -> FrameSource {
    match target {
        FrameRef::Tile { index, flags } => {
            let index = index as usize;

            if index < tiles {
                FrameSource::Tile {
                    archive,
                    index,
                    flags,
                }
            } else {
                FrameSource::Missing { archive, index }
            }
        }
        FrameRef::Sentinel(v) => FrameSource::Sentinel(v),
        FrameRef::Unknown(v) => FrameSource::Unknown(v),
    }
}

/// [`Animation::nudge`] as 16.16 board cells of y at regular zoom:
/// `IsoDrawList` divides it by the camera's `pixel_scale_sin`, 32
/// times sin [`ISO_CAMERA_ANGLE`](crate::data::settings::ISO_CAMERA_ANGLE), then truncates to
/// 16.16.
#[must_use]
pub fn sort_cells(px: i32) -> Fixed {
    #[allow(clippy::cast_precision_loss, reason = "a nudge of a few thousand")]
    let px = px as f32;

    // The `f32` `sin` gave, to the bit (`tests/settings.rs`).
    #[allow(clippy::cast_possible_truncation, reason = "the narrowing checked")]
    let sin = ISO_CAMERA_SIN as f32;

    #[allow(
        clippy::cast_possible_truncation,
        reason = "16.16 truncates, as the engine's"
    )]
    let cells = (px / (TILE_PIXELS * sin) * ONE_F32) as i32;

    Fixed::from_bits(cells)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::whole;

    /// The attack flag's 2,000 pixels sort it 76 cells nearer; a ruin's
    /// -1,000 38 further.
    #[test]
    fn a_sort_nudge_is_pixels_over_the_cameras_sine() {
        assert_eq!(whole(sort_cells(2000)), 76);
        assert_eq!(sort_cells(-1000).to_bits() / 65_536, -38);
        assert_eq!(sort_cells(0), Fixed::ZERO);
    }

    #[test]
    fn a_tile_number_inside_the_chunk_resolves() {
        assert_eq!(
            locate(FrameRef::Tile { index: 3, flags: 0 }, 2, 10),
            FrameSource::Tile {
                archive: 2,
                index: 3,
                flags: 0
            }
        );
    }

    #[test]
    fn a_tile_number_past_the_chunk_is_a_hole_not_a_tile() {
        // Clamping or wrapping would draw wrong art and hide the break.
        assert_eq!(
            locate(
                FrameRef::Tile {
                    index: 10,
                    flags: 0
                },
                2,
                10
            ),
            FrameSource::Missing {
                archive: 2,
                index: 10
            }
        );
    }

    #[test]
    fn an_archive_with_no_tiles_resolves_nothing() {
        assert_eq!(
            locate(FrameRef::Tile { index: 0, flags: 0 }, 0, 0),
            FrameSource::Missing {
                archive: 0,
                index: 0
            }
        );
    }

    fn pose(per_stream: u16, streams: u16) -> Pose {
        let n = usize::from(per_stream) * usize::from(streams);

        Pose {
            footprint: [0; 4],
            x: 0,
            y: 0,
            per_stream,
            streams,
            nudges: vec![(0, 0); n],
            attachments: Vec::new(),
            effects: Vec::new(),
            // Numbered, so a stream's slice is identifiable.
            frames: (0..n)
                .map(|i| FrameSource::Tile {
                    archive: 0,
                    index: i,
                    flags: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn a_stream_is_a_contiguous_run_of_the_frame_table() {
        // Stream-major: the Healer's Cast (nine frames, two streams) has
        // stream 1 at index 9, not interleaved.
        let p = pose(9, 2);

        assert_eq!(p.frames.len(), 18);
        assert_eq!(
            p.stream(0)[0],
            FrameSource::Tile {
                archive: 0,
                index: 0,
                flags: 0
            }
        );
        assert_eq!(
            p.stream(1)[0],
            FrameSource::Tile {
                archive: 0,
                index: 9,
                flags: 0
            }
        );
        assert_eq!(p.stream(0).len(), 9);
        assert_eq!(p.stream(1).len(), 9);
    }

    #[test]
    fn a_stream_past_the_end_is_empty_rather_than_wrapping() {
        assert_eq!(pose(8, 1).stream(1).len(), 0);
        assert_eq!(pose(0, 0).stream(0).len(), 0);
    }

    #[test]
    fn a_sentinel_stays_a_sentinel() {
        assert_eq!(
            locate(FrameRef::Sentinel(-3), 1, 10),
            FrameSource::Sentinel(-3)
        );
    }
}
