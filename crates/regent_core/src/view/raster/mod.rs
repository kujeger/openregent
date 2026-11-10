//! The engine's software renderer, pixel for pixel: its 16-bit frame,
//! the control band, lighting, translucency, and a CPU sheet to draw
//! them on. Each rule is **Binary** and checked against the captures
//! (`tests/{lit,blend,label,bar}.rs`); how a front end gets the same
//! result on a GPU is `regent_draw`'s.
//!
//! ```text
//!  colour   565 words: narrow, and the two widenings
//!  shade    indices 247-254: the HSV ramps
//!  stack    shadows over shadows, in draw order
//!  lit      the iso view's Gouraud tables, the present
//!  blend    modes 5 and 8, see-through units, tints
//!  atlas    a sheet sprites draw on in engine order
//! ```

pub mod atlas;
pub mod blend;
pub mod colour;
pub mod lit;
pub mod shade;
pub mod stack;
