//! All of the Bevy drawing `openregent` and `regent_viewer` do: GPU work,
//! materials, shaders and the code that puts them up. It draws what it
//! is given (a map: a world's `map::MapSource`); game logic
//! stays in openregent, engine rules in the `regent_core` library.
//!
//! ```text
//!  regent_core
//!     └── regent_draw
//!           both:          data, frame, material, plain, shadow, stage,
//!                          terrain, texture
//!           openregent only: map, art, present, ring, smooth, veil, cull, wind, sun, settle,
//!                          lie, soft (the last three also under map and
//!                          material)
//!           tests:         compile
//!           ├── openregent   (sim, menus, panels, audio, saves, settings)
//!           └── regent_viewer (unit and tileset browsers)
//! ```

pub mod art;
pub mod compile;
pub mod compose;
pub mod cull;
pub mod data;
pub mod frame;
pub mod instance;
pub mod kept;
pub mod lie;
pub mod map;
pub mod material;
pub mod plain;
pub mod present;
pub mod ring;
pub mod settle;
pub mod shadow;
pub mod smooth;
pub mod soft;
mod sort;
pub mod stage;
pub mod sun;
pub mod terrain;
pub mod texture;
pub mod veil;
pub mod wind;

/// Where the corpora are, for the tests that read HD's art.
#[cfg(test)]
use regent_corpus as common;
