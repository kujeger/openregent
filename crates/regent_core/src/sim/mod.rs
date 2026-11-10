//! The game engine, the original's `GE_` and `GS_` layers: the world GPL's
//! builtins act on, and the rules it runs in play.
//!
//! ```text
//!  game ──> host ──> world: pieces, players, ground, sight, orders, missiles
//!   │  frames,  │       ├─ movement, space   walking, paths, distances
//!   │  Input    │       ├─ placement, clearing, footprint, paint, light, slope
//!   │           │       │                    what a building takes and changes
//!   │           │       └─ damage            the set a building shows
//!   │           └─ Live: every builtin by its id; a Fallback where the
//!   │              world gives no answer
//!  mapgen ──> a map from the custom-game dialog's picks or a quest's
//!             seed: the eight charts, heights, paint, scatter
//!  start ──> the charts stood up: lairs, scatter slots, the start's units
//!  setup ──> the world a game starts on
//!  scene ──> a world set up by hand: map, placed units, clock
//!  units: 16.16, 32nds, half cells, ground pixels, camera scales
//!  trig: the sine table the executable ships
//! ```
//!
//! Uses [`crate::base`], [`crate::data`], [`crate::content`],
//! [`crate::save`] and [`crate::gpl`]. [`game`] runs on [`host::Live`].

pub mod attr_objs;
mod chunks;
pub mod clearing;
pub mod damage;
pub mod footprint;
pub mod freestyle;
pub mod game;
pub mod host;
mod id_table;
pub mod light;
pub mod mapgen;
pub mod movement;
pub mod paint;
pub mod placement;
pub mod scene;
pub mod setup;
pub mod slope;
pub mod space;
pub mod start;
pub mod trig;
pub mod units;
pub mod world;
