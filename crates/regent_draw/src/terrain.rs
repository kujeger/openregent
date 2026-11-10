//! The tileset, decoded once: the map's ground draws from it, and the
//! viewer's tileset browser too.

use std::sync::OnceLock;

use bevy::prelude::*;

use regent_core::content::imagery::FrameSource;
use regent_core::content::terrain::{Terrain, Tileset};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::settings::TILE_PIXELS;
use regent_core::view::Sprite as Frame;

use crate::data::Corpus;

/// Terrain tile size in both editions: the engine's [`TILE_PIXELS`] a side.
/// The art is asserted by `regent_core/tests/terrain.rs`.
pub const CELL: (u16, u16) = (TILE, TILE);

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "32.0, whole and positive"
)]
const TILE: u16 = TILE_PIXELS as u16;

/// The tileset, decoded once: its tiles' art. Not
/// [`regent_core::content::terrain::ground::Ground`], one terrain class per vertex.
#[derive(Resource)]
pub struct TileArt {
    pub tileset: Option<Tileset<'static>>,
    /// Letter to the class's own name, in table order.
    pub classes: Vec<(u8, String)>,
    /// Every tile of the tileset's archive, indexed by tile number.
    tiles: Vec<Option<Frame>>,
    /// Each tile's RGBA ([`Frame::rgba`]), made the first time a
    /// compose asks: the ground composes the same few tiles over and
    /// over.
    colours: Vec<OnceLock<Vec<u8>>>,
}

impl TileArt {
    /// Decode the tileset the corpus holds, if it has one.
    ///
    /// Takes the widest `DTRN`: with `data` and `datamx` both loaded, the
    /// expansion's is a strict superset (same nine bodies and eight blends,
    /// plus snow's two).
    pub fn open(corpus: &Corpus) -> Self {
        let terrain = corpus
            .catalogue
            .tagged(b"DTRN")
            .filter_map(|(at, _)| Terrain::at(&corpus.catalogue, at).ok())
            .max_by_key(|t| t.classes.len());

        let classes = terrain.as_ref().map_or_else(Vec::new, |t| {
            t.classes
                .iter()
                .map(|(letter, link, class)| {
                    let name = class.map_or_else(
                        || link.id.to_string(),
                        |c| String::from_utf8_lossy(c.head.name).into_owned(),
                    );

                    (*letter, name)
                })
                .collect()
        });

        let tileset = terrain.and_then(|t| t.tileset);
        let tiles = tileset
            .as_ref()
            .map_or_else(Vec::new, |t| decode(&corpus.catalogue, t));

        let colours = tiles.iter().map(|_| OnceLock::new()).collect();

        TileArt {
            tileset,
            classes,
            tiles,
            colours,
        }
    }

    pub const fn ready(&self) -> bool {
        self.tileset.is_some()
    }

    /// One line describing the tileset.
    pub fn summary(&self) -> String {
        self.tileset.as_ref().map_or_else(
            || "no tileset in this corpus".to_owned(),
            |t| {
                format!(
                    "tileset {}: {} classes, {} bodies, {} blends, {} tiles decoded",
                    t.id,
                    self.classes.len(),
                    t.bodies.len(),
                    t.blends.len(),
                    self.tiles.iter().filter(|t| t.is_some()).count(),
                )
            },
        )
    }

    /// The class name for a letter, for the panels.
    pub fn name(&self, letter: u8) -> &str {
        self.classes
            .iter()
            .find(|(l, _)| *l == letter)
            .map_or("?", |(_, n)| n.as_str())
    }

    /// The decoded pixels behind a frame reference, if it names a real tile.
    pub fn tile(&self, frame: &FrameSource) -> Option<&Frame> {
        match frame {
            FrameSource::Tile { index, .. } => self.tiles.get(*index)?.as_ref(),
            _ => None,
        }
    }

    /// [`TileArt::tile`] and its RGBA ([`Frame::rgba`]), made once.
    pub fn tile_rgba(&self, frame: &FrameSource) -> Option<(&Frame, &[u8])> {
        let FrameSource::Tile { index, .. } = frame else {
            return None;
        };
        let tile = self.tiles.get(*index)?.as_ref()?;
        let rgba = self.colours.get(*index)?.get_or_init(|| tile.rgba());

        Some((tile, rgba))
    }
}

/// Decode every tile of the tileset's archive, once.
fn decode(catalogue: &Catalogue<'static>, tileset: &Tileset<'static>) -> Vec<Option<Frame>> {
    let held = catalogue.chunk_len(tileset.archive, b"TILE");

    (0..held)
        .map(|i| Frame::at(catalogue, tileset.archive, i).and_then(Result::ok))
        .collect()
}

/// A composed sheet on screen.
#[derive(Component)]
pub struct Drawn;

/// What the map view last put up, so it does not rebuild without cause.
#[derive(Resource, Default)]
pub struct MapShown {
    /// The map view's revision. Here because every view shares [`Drawn`].
    pub mapped: Option<u64>,
    pub status: String,
}

impl MapShown {
    /// Force a rebuild. Called on leaving a subject.
    pub const fn forget(&mut self) {
        self.mapped = None;
    }
}
