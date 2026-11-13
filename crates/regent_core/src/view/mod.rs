//! What the screen shows: shipped content resolved for drawing, and the
//! simulation's state as the original's interface presents it.
//!
//! The joins themselves are [`data`](crate::data)'s: a unit's stats are in
//! `unittype.cam`, art and palette in `maindata.cam`, sounds in
//! `sounddesc.cam`, waveforms in `soundfx.cam`.
//!
//! ```text
//! DUNT ──ImageIDBase──> IMAG ──frame refs──> TILE ──palette──> SPLT
//!   │                     └── sets and slots: pose, facing, playback order
//!   ├──DefaultSound────> DSND ──phases──> WAVE
//!   ├──SoundOverride───> DACT
//!   ├──MOVE slot───────> DMOV
//!   └──HelpID, DialogID> STRT
//! ```
//!
//! **Corpus** facts that shape the API:
//!
//! - Every join crosses an archive: none of the 583 `DUNT` records names an
//!   `IMAG` in its own archive.
//! - Ids are not unique across archives (18 `IMAG` ids repeat), so a lookup
//!   needs a scope: [`dataset`](crate::content::dataset).
//! - 569 of 583 units resolve. The other 14 name missing art; show them, do
//!   not skip them.
//!
//! # Scope
//!
//! Hands a front end resolved content (unit, animations, frames as pixels)
//! and the interface's rules, and reports holes — a version-1 `TILE`, a
//! unit with no art, a sentinel frame reference — as variants, never
//! dropped records. The engine's own pixel rules, checked against the
//! captures, are [`raster`]; how a front end draws (GPU passes, upscaling,
//! departures) is `regent_draw`'s.
//!
//! Uses [`base`](crate::base), [`data`](crate::data),
//! [`content`](crate::content), [`gpl`](crate::gpl) and
//! [`sim`](crate::sim); of other crates, `serde`.
//!
//! # Groups
//!
//! Split as openregent's folders are:
//!
//! ```text
//!  sprite   a tile resolved against its palette; every group draws them
//!  raster   the engine's pixel rules
//!  map      what the iso view draws over the ground, and its picks
//!  panel    the in-game interface: selection, bars, minimap, tracking
//!  menu     panels and their controls, and the pages around a game
//!  sound    what a cue plays, how loud and where
//!  pack     frames packed into one texture, and trimmed to what they draw
//! ```
//!
//! The terrain model (blend slots, vertex classes) is
//! [`content::terrain`](crate::content::terrain).

pub mod map;
pub mod menu;
pub mod pack;
pub mod panel;
pub mod picture_file;
pub mod raster;
pub mod sound;
pub mod sprite;

pub use raster::atlas::{Backdrop, Sheet};

/// A screen rect, `[x0, y0, x1, y1]`, right and bottom exclusive, as
/// fill rect takes it: bars, labels, picks, the minimap. A panel
/// control's is [`crate::data::smnu::Rect`], `x, y, width, height` as the
/// file holds it; [`crate::data::smnu::Rect::corners`] turns one into this.
pub type ScreenRect = [i32; 4];
pub use raster::shade::Shade;
pub use sprite::{Colour, Ink, PaletteSource, Sprite};
