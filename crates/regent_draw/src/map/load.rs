//! A map as drawn: a world's [`MapSource`].

use regent_core::content::terrain::grid::Terrain;

use super::MapSource;

/// One map drawn: its [`MapSource`].
pub struct Loaded {
    pub name: String,
    /// What is drawn.
    pub source: MapSource,
}

impl Loaded {
    /// A map drawn from `source`, named `name`.
    #[must_use]
    pub const fn drawn(name: String, source: MapSource) -> Self {
        Loaded { name, source }
    }

    /// The map, from its source's arrays: no chunk walk.
    pub fn terrain(&self) -> Option<Terrain<'_>> {
        self.source.terrain()
    }
}
