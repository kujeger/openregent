//! What the integration tests share beyond the corpus lookup
//! ([`regent_corpus`], re-exported here): sim helpers.

// Each test binary reads a different subset of the corpora.
#![allow(dead_code)]

pub mod log_host;
pub mod mapgen;
pub mod sdk;

pub use regent_corpus::*;

/// Every descriptor in HD's archives, as `sim::world` places, measures
/// and names them: `sim::setup::descriptors`.
pub fn descriptors() -> regent_core::sim::world::descriptors::Descriptors {
    let library = library();

    regent_core::sim::setup::descriptors(&library.catalogue().expect("a catalogue"))
}

/// Every descriptor's body in HD's archives ([`descriptors`]).
pub fn bodies() -> regent_core::sim::world::bodies::Bodies {
    descriptors().bodies().clone()
}

/// HD's actions, particle systems, spells and name generators:
/// `sim::setup::lookups`.
pub fn lookups() -> (
    regent_core::sim::world::bodies::Named,
    regent_core::sim::world::naming::Generators,
) {
    let library = library();

    regent_core::sim::setup::lookups(&library.catalogue().expect("a catalogue"))
}
