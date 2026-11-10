//! The art service: one lookup for the game's pictures, and one policy
//! for making, caching, shelving and replacing them
//! (`docs/openregent/art-service.md`). A shared layer under two front ends:
//!
//! ```text
//!   interface (egui)                     Bevy assets
//!   ui::Art: Want, Place ──> texture     world::Sprites: piece, frame,
//!   xBRZ, shrink, snap, the quest map    variant, icon, cursor ──> handle
//!            │                                      │
//!            └──────────── shared layer ────────────┘
//!     ArtSource (archives' identity), Shelving/Pinned and Split (shelving by
//!     source), Count/busy (pending work), over (override folder),
//!     disk (LZ4 cache), remembered (a miss noted or not)
//! ```
//!
//! [`ui`]: the menus' and panels' pictures as egui textures; see its
//! docs. [`world`]: the game view's sprites and the interface pictures
//! drawn as Bevy images, unscaled. Each keeps its pictures by
//! [`ArtSource`]: ids repeat across archives (base and expansion `AP29`
//! differ). A game's source is shelved with its dataset ([`Split`]),
//! the menus' stay ([`Shelving::Pinned`]).

pub mod binds;
pub mod cursor;
pub mod disk;
pub mod fsr;
pub mod glow;
pub mod gpu;
pub mod levels;
pub mod look;
pub mod over;
pub mod read;
pub mod scale;
pub mod shrink;
pub mod side;
pub mod stack;
pub mod ui;
pub mod world;

use std::collections::{HashMap, HashSet, hash_map};
use std::hash::{BuildHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy_egui::egui::{Pos2, Rect};

pub use look::{Tone, Wrap};
pub use read::{Reach, Read};
pub use ui::*;
pub use world::{Assets2, Sprites};

/// Textures asked for and not yet in, across every [`Art`].
static PENDING: AtomicUsize = AtomicUsize::new(0);

/// Whether a texture is still being made: a screenshot waits for it.
pub fn busy() -> bool {
    PENDING.load(Ordering::Relaxed) > 0
}

/// One pending texture, counted in `PENDING` while it lives.
pub struct Count;

impl Count {
    #[allow(
        clippy::new_without_default,
        reason = "a count is a pending texture, not an empty value"
    )]
    pub fn new() -> Count {
        PENDING.fetch_add(1, Ordering::Relaxed);
        Count
    }
}

impl Drop for Count {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::Relaxed);
    }
}

/// A tile: its archive and index.
pub type Tile = (usize, usize);

/// Where a picture is read from: a catalogue, by its archive files in
/// load order (a record's art depends on every archive before it).
/// The menus' base and expansion archives are two; a game's dataset is
/// one, the same as the menus' where its files are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArtSource(u64);

impl ArtSource {
    /// The catalogue of `paths`.
    #[must_use]
    pub fn of(paths: &[PathBuf]) -> ArtSource {
        let mut hasher = std::hash::DefaultHasher::new();

        paths.hash(&mut hasher);
        ArtSource(hasher.finish())
    }
}

/// Page rectangle `r` on the window: `scale` window points a page pixel,
/// page (0, 0) at `origin`.
fn onto(r: Rect, scale: f32, origin: Pos2) -> Rect {
    let point = |at: Pos2| origin + at.to_vec2() * scale;

    Rect::from_min_max(point(r.min), point(r.max))
}

/// The entries of `map` whose key passes `taken`, moved out.
fn moved<K: Eq + Hash + Copy, V, S: BuildHasher + Default>(
    map: &mut HashMap<K, V, S>,
    taken: impl Fn(&K) -> bool,
) -> HashMap<K, V, S> {
    let keys: Vec<K> = map.keys().filter(|k| taken(k)).copied().collect();

    keys.into_iter()
        .filter_map(|k| map.remove(&k).map(|v| (k, v)))
        .collect()
}

/// `map`'s value for `key`, made by `make` on first use. A picture not
/// read is noted absent only where the reader holds its whole source
/// ([`Reach::Whole`]): another reader of the source may hold it.
fn remembered<K: Eq + Hash, V, S: BuildHasher>(
    map: &mut HashMap<K, Option<V>, S>,
    key: K,
    reach: Reach,
    make: impl FnOnce() -> Option<V>,
) -> Option<&V> {
    match map.entry(key) {
        hash_map::Entry::Occupied(held) => held.into_mut().as_ref(),
        hash_map::Entry::Vacant(free) => match (make(), reach) {
            (None, Reach::Part) => None,
            (read, _) => free.insert(read).as_ref(),
        },
    }
}

/// Whether the shelf takes a source's pictures with its game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shelving {
    /// The menus': they stay.
    Pinned,
    /// A game's alone: shelved with its dataset.
    Shelved,
}

/// The sources the shelf leaves where they are: the menus'.
#[derive(Clone, Debug, Default)]
pub struct Pinned(HashSet<ArtSource>);

impl Pinned {
    /// `sources` pinned.
    #[must_use]
    pub fn of(sources: impl IntoIterator<Item = ArtSource>) -> Pinned {
        Pinned(sources.into_iter().collect())
    }

    /// Whether the shelf takes `source`'s pictures.
    #[must_use]
    pub fn kept(&self, source: ArtSource) -> Shelving {
        if self.0.contains(&source) {
            Shelving::Pinned
        } else {
            Shelving::Shelved
        }
    }

    /// `source`'s entries in `entries` moved out, for the next game on
    /// its dataset; none of a pinned source.
    pub fn take<E: Split + Default>(&self, entries: &mut E, source: ArtSource) -> E {
        match self.kept(source) {
            Shelving::Pinned => E::default(),
            Shelving::Shelved => entries.split(source),
        }
    }
}

/// A front end's pictures, which part by source for the shelf.
pub trait Split {
    /// `source`'s entries, moved out.
    #[must_use]
    fn split(&mut self, source: ArtSource) -> Self;

    /// `other`'s entries added; its own kept where both hold one.
    fn join(&mut self, other: Self);
}
