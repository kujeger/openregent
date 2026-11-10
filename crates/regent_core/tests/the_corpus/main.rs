//! Tests of the generator's inputs: the quest's map, the chart and the
//! art's footprints. `docs/settled.md` and `docs/formats/maps.md` cite
//! test names.
//!
//! | Module | What it holds |
//! |---|---|
//! | [`whole_generator`] | two fidelity rows: footprints and special sets |
//! | [`landscape_oracle`] | the landscape chart's clump sizes |
//! | [`parting_clump`] | every scatter code has a footprint |

#[path = "../common/mod.rs"]
mod common;

mod fixture;

mod landscape_oracle;
mod parting_clump;
mod whole_generator;
