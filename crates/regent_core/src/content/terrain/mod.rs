//! A tileset, and the terrain descriptor naming its classes.
//!
//! [`Tileset`]: the art alone, an `IMAG` of `MapTexture` sets. [`Terrain`]:
//! a `DTRN` joined with its same-id tileset and each letter's `DTPT`.
//!
//! ```text
//! terrtype.cam                       tilesetdata.cam
//!   DTRN "GRIT" ──same id──────────────> IMAG "GRIT"
//!     └─ TPRT slot 1 = "GR01" ─┐           ├─ MapTextureBtoB   a body
//!                              │           └─ MapTextureAtoB   a blend
//!   DTPT "GR01" Plains_parent <┘
//! ```
//!
//! # A body set is mostly empty
//!
//! Of `MapTextureBtoB`'s 104 frame references, 78 are tile 0 with flag
//! `0x4000`, a null marker. Stream 0 holds plain ground variants; the other
//! three are null except in columns where all four hold distinct tiles.
//!
//! Such a column is one 2x2-cell picture, a quarter per stream. **Corpus**: a
//! cell's `rotation` is the quarter drawn, non-zero nowhere else
//! ([`crate::content::terrain::grid::Cell::rotation`]).
//!
//! They go in [`Body::groups`], apart from the variants. A frame count is not
//! a tile count.
//!
//! # The terrain model
//!
//! A map's ground as classes on vertices, apart from any screen:
//! [`schema`] (what the seventeen blend slots mean), [`board`] (which slot
//! a cell's corners ask for), [`ground`] (a save's cells inverted to one
//! class per vertex), [`compass`] (which way is north).

pub mod board;
pub mod compass;
pub mod grid;
pub mod ground;
pub mod schema;

use std::marker::PhantomData;

use crate::data::desc::{self, Part};
use crate::data::error::{Error, Result};
use crate::data::fourcc::FourCc;
use crate::data::imag::SetKind;

use crate::content::imagery::{self, Animation, FrameSource};
use crate::content::unit::Link;
use crate::data::catalogue::{Catalogue, Located};

/// Flag marking a null frame reference (with tile 0). Three quarters of a body
/// set's references. Why `0x4000` is unknown.
const NULL_FRAME: u16 = 0x4000;

/// One blend-set slot: a corner arrangement ([`crate::content::terrain::schema::Schema`]) and
/// its art.
///
/// Variants are every frame of the slot regardless of stream split:
/// `MapTextureAtoB` slot 3 is four frames in one stream, `MapTextureAtoJ` slot
/// 1 one frame in four streams; both are four interchangeable pictures. Every
/// shipped blend set has 17 slots (`docs/formats/terrain.md`).
#[derive(Debug, Clone)]
pub struct Case {
    pub slot: usize,
    pub variants: Vec<FrameSource>,
}

/// `MapTexture{X}to{Y}`, `X != Y`: one terrain over another.
#[derive(Debug, Clone)]
pub struct Blend {
    pub from: u8,
    pub to: u8,
    /// Record order. 17 on every shipped set, but read, not assumed.
    pub cases: Vec<Case>,
}

/// `MapTexture{X}to{X}`: one class's uniform ground.
#[derive(Debug, Clone)]
pub struct Body {
    pub letter: u8,
    pub frames: u16,
    pub streams: u16,
    /// Stream 0: plain ground variants.
    pub variants: Vec<FrameSource>,
    /// Columns where every stream has a tile: the 2x2 pictures. See module
    /// docs.
    pub groups: Vec<(usize, Vec<FrameSource>)>,
    /// The raw `frames * streams` table, stream-major, nulls included, so a
    /// browser can show the sparsity.
    pub grid: Vec<FrameSource>,
}

/// Which way round a blend was found for an unordered pair. Drawing `BtoA`
/// for `AtoB` inverts the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    Forward,
    Reversed,
}

/// An `IMAG` record read as terrain art.
#[derive(Debug, Clone)]
pub struct Tileset<'a> {
    pub id: FourCc,
    /// The record's archive, where its tile numbers count. May differ from the
    /// descriptor's. See [`crate::content::unit`].
    pub archive: usize,
    pub bodies: Vec<Body>,
    pub blends: Vec<Blend>,
    marker: PhantomData<&'a ()>,
}

impl<'a> Tileset<'a> {
    /// Read the `IMAG` at `at` as terrain art.
    ///
    /// # Errors
    ///
    /// If there is no `IMAG` there, or it will not parse.
    pub fn at(catalogue: &Catalogue<'a>, at: Located) -> Result<Self> {
        let record = catalogue.record(b"IMAG", at).ok_or(Error::BadValue {
            what: "no IMAG record at this location",
            value: at.record as u64,
        })?;

        let mut bodies = Vec::new();
        let mut blends = Vec::new();

        for set in imagery::sets(catalogue, at)? {
            match set.kind {
                SetKind::MapTexture { from, to } if from == to => bodies.push(body(from, &set)),
                SetKind::MapTexture { from, to } => blends.push(blend(from, to, &set)),
                // None shipped.
                _ => {}
            }
        }

        Ok(Tileset {
            id: record.id,
            archive: at.archive,
            bodies,
            blends,
            marker: PhantomData,
        })
    }

    pub fn body(&self, letter: u8) -> Option<&Body> {
        self.bodies.iter().find(|b| b.letter == letter)
    }

    pub fn blend(&self, from: u8, to: u8) -> Option<&Blend> {
        self.blends.iter().find(|b| b.from == from && b.to == to)
    }

    /// The blend between two classes in either direction; the tree stores
    /// one.
    pub fn between(&self, a: u8, b: u8) -> Option<(&Blend, Sense)> {
        self.blend(a, b)
            .map(|x| (x, Sense::Forward))
            .or_else(|| self.blend(b, a).map(|x| (x, Sense::Reversed)))
    }

    /// The frame a map cell draws.
    ///
    /// **Corpus**, as `regent_draw` draws the ground: a blend cell indexes
    /// its case's variant list; a uniform cell names a body-grid column
    /// with `variant` and a stream with `rotation`. **Binary**: the
    /// minimap asks for set `5000 + pair`, slot `case`,
    /// frame `variant`, stream `rotation`.
    #[must_use]
    pub fn picture(&self, cell: grid::Cell) -> Option<FrameSource> {
        if cell.uniform() {
            let body = self.body(cell.near())?;
            let column = usize::from(cell.rotation) * usize::from(body.frames);

            body.grid.get(column + usize::from(cell.variant)).copied()
        } else {
            self.blend(cell.near(), cell.far())?
                .cases
                .get(usize::from(cell.case))?
                .variants
                .get(usize::from(cell.variant))
                .copied()
        }
    }

    /// Every frame named, bodies and blends.
    pub fn frames(&self) -> impl Iterator<Item = FrameSource> + '_ {
        let bodies = self.bodies.iter().flat_map(|b| {
            b.variants
                .iter()
                .copied()
                .chain(b.groups.iter().flat_map(|(_, g)| g.iter().copied()))
        });
        let blends = self
            .blends
            .iter()
            .flat_map(|b| b.cases.iter().flat_map(|c| c.variants.iter().copied()));

        bodies.chain(blends)
    }
}

/// A `DTRN`, its art, and each letter's class. `GR09 Snow` has no body art
/// in the base game; every letter has some in the expansion.
#[derive(Debug, Clone)]
pub struct Terrain<'a> {
    pub id: FourCc,
    pub desc: desc::Terrain<'a>,
    /// The `IMAG` sharing this record's id. A [`Link`]: one instance per
    /// dataset is too few to call it a rule.
    pub art: Link,
    /// Empty when [`Terrain::art`] did not resolve.
    pub tileset: Option<Tileset<'a>>,
    /// Per filled `TPRT` slot: letter, `DTPT` link, record if resolved.
    pub classes: Vec<(u8, Link, Option<desc::TerrainPart<'a>>)>,
}

impl<'a> Terrain<'a> {
    /// Resolve the `DTRN` record at `at`.
    ///
    /// # Errors
    ///
    /// If there is no `DTRN` there, or it will not parse. Unresolved references
    /// are not errors.
    pub fn at(catalogue: &Catalogue<'a>, at: Located) -> Result<Self> {
        let record = catalogue.record(b"DTRN", at).ok_or(Error::BadValue {
            what: "no DTRN record at this location",
            value: at.record as u64,
        })?;

        let part = Part::parse(record.data)?;
        let desc = desc::Terrain::parse(&part)?;

        let art = Link::find(catalogue, *b"IMAG", record.id);
        let tileset = art
            .at
            .map(|found| Tileset::at(catalogue, found))
            .transpose()?;

        let classes = desc
            .classes()
            .map(|(letter, id)| {
                let link = Link::find(catalogue, *b"DTPT", id);
                let class = link
                    .at
                    .and_then(|at| catalogue.record(b"DTPT", at))
                    .and_then(|r| Part::parse(r.data).ok())
                    .and_then(|p| desc::TerrainPart::parse(&p).ok());

                (letter, link, class)
            })
            .collect();

        Ok(Terrain {
            id: record.id,
            desc,
            art,
            tileset,
            classes,
        })
    }

    /// The dataset's first `DTRN`. By tag, not the literal `GRIT`, which is
    /// only observed.
    ///
    /// # Errors
    ///
    /// As [`Terrain::at`].
    pub fn find(catalogue: &Catalogue<'a>) -> Option<Result<Self>> {
        catalogue
            .tagged(b"DTRN")
            .next()
            .map(|(at, _)| Self::at(catalogue, at))
    }
}

/// Split a body set into ground variants and 2x2 groups.
fn body(letter: u8, set: &Animation) -> Body {
    // A body set is one slot; extra slots would be a finding, not merged.
    let slot = set.poses.first();
    let (frames, streams) = slot.map_or((0, 0), |s| (s.per_stream, s.streams));

    let column = |i: u16| -> Vec<FrameSource> {
        (0..streams)
            .filter_map(|s| slot?.stream(s).get(usize::from(i)).copied())
            .collect()
    };

    // A group: every stream has a tile. Elsewhere only stream 0 does.
    let (groups, plain): (Vec<_>, Vec<_>) = (0..frames)
        .map(|i| (usize::from(i), column(i)))
        .partition(|(_, tiles)| tiles.len() == usize::from(streams) && tiles.iter().all(is_drawn));

    Body {
        letter,
        frames,
        streams,
        variants: plain
            .into_iter()
            .filter_map(|(_, tiles)| tiles.first().copied())
            .collect(),
        groups,
        grid: slot.map(|s| s.frames.clone()).unwrap_or_default(),
    }
}

/// Read a blend set's slots as cases.
fn blend(from: u8, to: u8, set: &Animation) -> Blend {
    Blend {
        from,
        to,
        cases: set
            .poses
            .iter()
            .enumerate()
            .map(|(slot, pose)| Case {
                slot,
                variants: pose.frames.clone(),
            })
            .collect(),
    }
}

/// Whether a reference names a tile, not the null marker.
const fn is_drawn(frame: &FrameSource) -> bool {
    match frame {
        FrameSource::Tile { index, flags, .. } => !(*index == 0 && *flags == NULL_FRAME),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::imagery::Pose;
    use crate::data::imag::SetKind;

    fn tile(index: usize) -> FrameSource {
        FrameSource::Tile {
            archive: 0,
            index,
            flags: 0,
        }
    }

    const NULL: FrameSource = FrameSource::Tile {
        archive: 0,
        index: 0,
        flags: NULL_FRAME,
    };

    fn set(kind: SetKind, poses: Vec<Pose>) -> Animation {
        Animation {
            id: 0,
            kind,
            variant: 0,
            poses,
            playback: 0,
            nudge: 0,
            sort: crate::content::imagery::Sort::WithBase,
            passes: vec![crate::data::imag::Pass::First],
        }
    }

    fn pose(per_stream: u16, streams: u16, frames: Vec<FrameSource>) -> Pose {
        Pose {
            footprint: [0; 4],
            x: 0,
            y: 0,
            nudges: vec![(0, 0); frames.len()],
            per_stream,
            streams,
            frames,
            attachments: Vec::new(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn a_body_keeps_stream_zero_and_sets_its_groups_aside() {
        // Two frames, two streams: column 0 plain, column 1 a group.
        // Stream-major: stream 0 is the first two entries.
        let s = set(
            SetKind::MapTexture { from: 1, to: 1 },
            vec![pose(2, 2, vec![tile(10), tile(11), NULL, tile(21)])],
        );

        let b = body(1, &s);

        assert_eq!(b.variants, vec![tile(10)], "only the plain column");
        assert_eq!(b.groups.len(), 1);
        assert_eq!(b.groups[0].0, 1, "the group is column 1");
        assert_eq!(b.groups[0].1, vec![tile(11), tile(21)]);
    }

    #[test]
    fn a_case_keeps_every_variant_however_the_streams_split_them() {
        // The two shipped shapes: 4x1 and 1x4. Both give four pictures.
        let wide = set(
            SetKind::MapTexture { from: 0, to: 1 },
            vec![pose(4, 1, (0..4).map(tile).collect())],
        );
        let tall = set(
            SetKind::MapTexture { from: 0, to: 1 },
            vec![pose(1, 4, (0..4).map(tile).collect())],
        );

        assert_eq!(blend(0, 1, &wide).cases[0].variants.len(), 4);
        assert_eq!(blend(0, 1, &tall).cases[0].variants.len(), 4);
    }

    #[test]
    fn a_blend_is_found_in_either_direction() {
        let tileset = Tileset {
            id: FourCc::new(b"GRIT"),
            archive: 0,
            bodies: Vec::new(),
            blends: vec![Blend {
                from: 0,
                to: 1,
                cases: Vec::new(),
            }],
            marker: PhantomData,
        };

        assert_eq!(tileset.between(0, 1).map(|(_, s)| s), Some(Sense::Forward));
        assert_eq!(tileset.between(1, 0).map(|(_, s)| s), Some(Sense::Reversed));
        assert!(tileset.between(0, 2).is_none());
    }

    #[test]
    fn the_null_marker_is_not_a_tile_but_tile_zero_is() {
        assert!(!is_drawn(&NULL));
        assert!(is_drawn(&tile(0)), "index 0 with no flag is real art");
        assert!(!is_drawn(&FrameSource::Sentinel(-3)));
    }
}
