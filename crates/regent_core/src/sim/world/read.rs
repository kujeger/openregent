//! Read-only accessors the front ends' queries use
//! ([`crate::view::panel::tracking`]). No rule calls them.

use crate::sim::world::piece_state;
use crate::sim::world::{World, bodies::Class};

impl World {
    /// The agent holding piece `id` inside it (unit holder with state
    /// bit [`piece_state::HELD`]); `None` for one standing alone or hung.
    #[must_use]
    pub fn held_in(&self, id: u32) -> Option<u32> {
        let place = self.pieces.get(id)?.place?;

        (place.holder != 0 && place.state & piece_state::HELD != 0)
            .then(|| self.holder(place.holder))
            .flatten()
    }

    /// Whether a sight pass has given piece `id` its words: before one,
    /// [`World::sight_word`] answers 0 for what is not yet looked at.
    #[must_use]
    pub fn sighted(&self, id: u32) -> bool {
        self.units.words(id).is_some()
    }

    /// Whether piece `id` is a building.
    #[must_use]
    pub fn is_building(&self, id: u32) -> bool {
        self.piece_body(id)
            .is_some_and(|b| b.class == Class::Building)
    }

    /// `GenUnit` for a building: its exit, 32nds of a cell, where
    /// the tracking window centres on it.
    #[must_use]
    pub fn exit_of(&self, id: u32) -> Option<(i32, i32)> {
        self.exit(id)
    }
}
