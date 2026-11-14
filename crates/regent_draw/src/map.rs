//! A generated map, read from a live world ([`MapSource`]), and drawn.
//!
//! ```text
//!  load     a source as drawn ([`Loaded`])
//!  project  cells to world units and back; picking, the selection marks
//!  redraw   the ground (patch, floor) and the inset, on change
//!  things   every object put up, sorted by screen depth (depth)
//!  cel      a frame's materials, a set's layers ([`cel::Paint`])
//!  reel     the clock frames cycle by
//! ```
//!
//! The ground is composited real tiles; a seam in the image is a seam in
//! the data. The camera picks region (screen plus margin) and resolution: a
//! full 512-cell map at 32 px is 16,384 px square, 1.07 GB RGBA.
//!
//! Each frame shows its set's
//! [`frame_ms`](regent_core::sim::movement::action::frame_ms) of [`reel::Reel`]
//! time.
//!
//! # Inferred
//!
//! - Animation for placed things without a saved set: a preference list,
//!   `Dead` first for ruins.
//! - The opening frame is the source's; characters hold it
//!   (`reel::cycles`).

use regent_core::content::terrain::grid::Terrain;
use std::collections::BTreeMap;

use bevy::prelude::*;

pub mod cel;
pub mod depth;
#[cfg(test)]
mod fixtures;
mod floor;
pub mod load;
mod patch;
pub mod project;
pub mod redraw;
pub mod reel;
pub mod sheet;
mod source;
pub mod things;

pub use source::{Item, MapSource};

use depth::Depth;
use load::Loaded;
use project::Composed;

/// The buildings' chunk tag.
pub const BLDG: &[u8; 4] = b"BLDG";

/// The mobiles' chunk tag.
pub const VHCL: &[u8; 4] = b"VHCL";

/// The scatter's chunk tag.
pub const GRND: &[u8; 4] = b"GRND";

/// The reward flags' chunk tag.
pub const RWRD: &[u8; 4] = b"RWRD";

/// The object chunks, in the order a save writes them.
pub const CHUNKS: [(&[u8; 4], &str); 4] = [
    (BLDG, "buildings"),
    (VHCL, "mobiles"),
    (GRND, "scatter"),
    (RWRD, "rewards"),
];

/// [`CHUNKS`]' mobiles: `VHCL`.
pub const MOBILES: usize = 1;

/// [`CHUNKS`]' scatter: `GRND`.
pub const SCATTER: usize = 2;

/// The saves this process opened, and what is being looked at.
#[derive(Resource)]
pub struct Maps {
    pub loaded: Vec<Loaded>,
    pub at: usize,
    /// What the art layer's ground was last wanted as: its region,
    /// resolution and rise ([`patch`]).
    composed: Option<Composed>,
    /// The art layer's ground, in patches.
    patches: patch::Patches,
    /// The last [`Depth::of`] and the rows it sorted: each drawing asks
    /// it every frame, and the rows seldom change.
    depth: std::sync::Mutex<Option<(Vec<f32>, Depth)>>,
    /// Which object chunks are drawn, in [`CHUNKS`] order.
    pub objects: [bool; 4],
    /// Bumped by anything that changes the picture.
    pub revision: u64,
    /// Every drawn object with art: which chunk, which record, and where,
    /// so a click finds only what is seen.
    ///
    /// Cached with the picture; the marks redraw every frame.
    pub placed: Vec<Placed>,
    /// The thing a click last landed on, if it is still drawn.
    pub selected: Option<Placed>,
    /// How far right of its anchor each drawn building's picture
    /// reaches, world units, by handle: the room beside it.
    pub reach: BTreeMap<u32, f32>,
}

/// A drawn object, named well enough to be read again.
///
/// An index, not the record itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// Which of [`CHUNKS`].
    pub chunk: usize,
    /// Which record of that chunk, in file order.
    pub index: usize,
    /// Where it stands, in cells.
    pub cell: Vec2,
    /// How high, in the ground's units: its record's. See `project::item_height`.
    pub height: f32,
}

impl Maps {
    /// The map view over saves already open.
    pub const fn with(loaded: Vec<Loaded>) -> Self {
        Maps {
            loaded,
            at: 0,
            composed: None,
            patches: patch::Patches::new(),
            depth: std::sync::Mutex::new(None),
            objects: [true; 4],
            revision: 1,
            placed: Vec::new(),
            selected: None,
            reach: BTreeMap::new(),
        }
    }

    pub fn current(&self) -> Option<&Loaded> {
        self.loaded.get(self.at)
    }

    pub fn terrain(&self) -> Option<Terrain<'_>> {
        self.current()?.terrain()
    }

    /// The ground's patches the view wants and not yet made: a few are
    /// made a frame.
    #[must_use]
    pub const fn pending(&self) -> usize {
        self.patches.pending()
    }

    /// One line naming the map drawn, for the status line.
    pub fn summary(&self) -> String {
        match self.current() {
            None => "no map".to_owned(),
            Some(map) => match map.terrain() {
                None => format!("{}: no map", map.name),
                Some(t) => format!("{} — {} cells a side", map.name, t.side()),
            },
        }
    }

    /// Compose the ground again, as after a pan: its save's ground changed.
    pub const fn recompose(&mut self) {
        self.composed = None;
    }
}

/// How much of [`RELIEF`] an object standing on the ground gets: it rides
/// the ground. **Inferred**.
pub const LIFT: f32 = 1.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_named_chunks_are_their_tags() {
        assert_eq!(CHUNKS[MOBILES].0, VHCL);
        assert_eq!(CHUNKS[SCATTER].0, GRND);
    }
}
