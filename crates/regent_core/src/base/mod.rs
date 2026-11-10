//! Engine-wide primitives: no format and no game knowledge, used by every
//! layer above. Uses nothing else in the crate.
//!
//! - [`rng`]: the engine's random number generator.
//! - [`digest`]: hashing that is the same on every run and machine:
//!   fixed-order maps and a digest of any serde value.
//! - [`switch`]: an opt-in debugging switch, set by the environment.
//! - [`fixed`]: the engine's 16.16 fixed point and its rounding.
//! - [`gfix`]: GPL's 22.10 `float`.

pub mod digest;
pub mod fixed;
pub mod gfix;
pub mod rng;
pub mod switch;
