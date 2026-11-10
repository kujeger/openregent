//! The descriptor tables a game reads: made once from the archives
//! before play, only read after.
//!
//! ```text
//!  sim::setup::descriptors ──> shapes, bodies, every HEAD name ──┐
//!  sim::setup::lookups ──────> named: actions, particles, spells ─┴─> Descriptors
//!                                                                      │
//!  World::with_descriptors, World::fresh <─────────────────────────────┘
//! ```
//!
//! Not saved: a load makes them again from the same dataset
//! ([`World::restore`](super::World::restore)).

use std::collections::BTreeSet;

use super::bodies::{Bodies, Body, BodyNames, Class, Named, body_names};
use super::spell::Spell;
use super::{Shape, Shapes};
use crate::data::fourcc::FourCc;

/// Every descriptor as the world measures, places and names it.
#[derive(Clone, Debug, Default)]
pub struct Descriptors {
    /// Pad rect and sight per descriptor.
    shapes: Shapes,
    /// What placing a piece and blocking one need, by `DUNT` id.
    bodies: Bodies,
    /// The bodies' names: every lookup of a descriptor by name.
    body_names: BodyNames,
    /// Every `DUNT` and `DACT` `HEAD` name, folded: `CreateEffector`'s
    /// lookup.
    described: BTreeSet<String>,
    /// What orders and effectors read of descriptors, by name.
    named: Named,
}

impl Descriptors {
    /// `shapes`, `bodies` indexed by name, and the folded `described`
    /// names; nothing by name yet ([`Descriptors::with_named`]).
    #[must_use]
    pub fn new(shapes: Shapes, bodies: Bodies, described: BTreeSet<String>) -> Descriptors {
        Descriptors {
            shapes,
            body_names: body_names(&bodies),
            bodies,
            described,
            named: Named::default(),
        }
    }

    /// Actions and particle systems by name, too.
    #[must_use]
    pub fn with_named(self, named: Named) -> Descriptors {
        Descriptors { named, ..self }
    }

    /// Descriptor `code`'s pad rect and sight.
    #[must_use]
    pub fn shape(&self, code: FourCc) -> Option<&Shape> {
        self.shapes.get(&code)
    }

    /// Descriptor `code`'s body.
    #[must_use]
    pub fn body(&self, code: FourCc) -> Option<&Body> {
        self.bodies.get(&code)
    }

    /// Every body, by `DUNT` id.
    #[must_use]
    pub const fn bodies(&self) -> &Bodies {
        &self.bodies
    }

    /// The descriptor named `name`, any case ([`body_names`]).
    #[must_use]
    pub fn body_named(&self, name: &str) -> Option<(FourCc, &Body)> {
        let code = self.body_names.get(&name.to_ascii_lowercase())?;

        self.bodies.get(code).map(|b| (*code, b))
    }

    /// Whether any `DUNT` or `DACT` record is named `folded`, folded.
    #[must_use]
    pub fn described(&self, folded: &str) -> bool {
        self.described.contains(folded)
    }

    /// Actions, particle systems, spells and scripts by name.
    #[must_use]
    pub const fn named(&self) -> &Named {
        &self.named
    }

    /// Descriptor `code`'s class.
    #[must_use]
    pub fn class(&self, code: FourCc) -> Option<Class> {
        self.body(code).map(|b| b.class)
    }

    /// The spell of `code`, from the actions given.
    #[must_use]
    pub fn spell(&self, code: FourCc) -> Option<&Spell> {
        self.named.spells.values().find(|s| s.code == code)
    }

    /// The spell named `name`, any case.
    #[must_use]
    pub fn spell_named(&self, name: &str) -> Option<&Spell> {
        self.named.spells.get(&name.to_ascii_lowercase())
    }

    /// The code of the action named `name`, any case: both natives look
    /// it up in the action list (action list, gpl learn spell,
    /// gpl forget spell), not the unit registry. **Corpus**: a learned
    /// `speed_monster` holds the action `A026`, not the effector `XR12`
    /// of the same name.
    #[must_use]
    pub fn action_named(&self, name: &str) -> Option<FourCc> {
        self.spell_named(name).map(|s| s.code)
    }

    /// The code of the descriptor named `name`, any case: an action's,
    /// else a unit's: the unit-type builtins' lookup (`DisableUnitType`
    /// and kin).
    #[must_use]
    pub fn code_named(&self, name: &str) -> Option<FourCc> {
        self.action_named(name)
            .or_else(|| self.body_named(name).map(|(code, _)| code))
    }

    /// Add a body and index its name: for tests that make their own.
    #[cfg(test)]
    pub(super) fn add_body(&mut self, code: FourCc, body: Body) {
        self.bodies.insert(code, body);
        self.body_names = body_names(&self.bodies);
    }

    /// Add a shape: for tests that make their own.
    #[cfg(test)]
    pub(super) fn add_shape(&mut self, code: FourCc, shape: Shape) {
        self.shapes.insert(code, shape);
    }
}
