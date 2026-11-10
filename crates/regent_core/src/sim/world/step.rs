//! What a frame's step is told by its caller, and the GPL RNG engine code
//! draws on.
//!
//! ```text
//!  Game     step(Some(&mut vm.rng))
//!             └─ the world holds the state for this call only
//! ```

use super::World;
use crate::gpl::vm::Vm;

/// Whether a builtin's engine code may draw on the VM's GPL RNG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draws {
    /// It draws: play.
    Lent,
    /// No GPL RNG lent: a missile's aim, which needs one, is unknown.
    Kept,
}

/// The world RNG, and the GPL RNG while lent for a call.
#[derive(Clone, Debug, Default)]
pub(super) struct Rngs {
    /// The world RNG's state, rng global: the session word at a new
    /// game's start. In play a lair's pick
    /// draws on it, and every unit made, upgrade and illusion, its slot
    /// ([`World::slot_draw`]). **Inferred**: no other draw on it in play.
    global: u32,
    /// The GPL RNG's state for engine code that draws on it,
    /// only during the call it was lent to
    /// ([`World::lend`]).
    lent: Option<u32>,
}

impl Rngs {
    /// The world RNG at `state`, nothing lent.
    pub(super) const fn seeded(state: u32) -> Rngs {
        Rngs {
            global: state,
            lent: None,
        }
    }

    /// The world RNG reseeded with `state`.
    pub(super) const fn seed(&mut self, state: u32) {
        self.global = state;
    }

    /// The world RNG's state.
    pub(super) const fn state(&self) -> u32 {
        self.global
    }

    /// A draw below `bound` on the world RNG.
    pub(super) const fn draw(&mut self, bound: i32) -> i32 {
        crate::base::rng::draw(&mut self.global, bound)
    }

    /// The GPL RNG lent at `state`; `None`, nothing lent.
    pub(super) const fn lend(&mut self, state: Option<u32>) {
        self.lent = state;
    }

    /// The lent GPL RNG's state, given back.
    pub(super) const fn take_lent(&mut self) -> Option<u32> {
        self.lent.take()
    }

    /// Whether a GPL RNG is lent.
    pub(super) const fn lending(&self) -> bool {
        self.lent.is_some()
    }

    /// A draw below `bound` on the lent GPL RNG; `None` with none lent.
    pub(super) fn lent_draw(&mut self, bound: i32) -> Option<i32> {
        self.lent
            .as_mut()
            .map(|state| crate::base::rng::draw(state, bound))
    }
}

impl World {
    /// Run `f` on the GPL RNG's state `gpl`, written back after: the
    /// world holds it for this call only. `None`: no draws known.
    pub(super) fn lend<R>(&mut self, gpl: Option<&mut u32>, f: impl FnOnce(&mut World) -> R) -> R {
        self.rngs.lend(gpl.as_deref().copied());

        let out = f(self);

        if let (Some(state), Some(to)) = (self.rngs.take_lent(), gpl) {
            *to = state;
        }

        out
    }

    /// A GPL RNG draw by a builtin's native code: on the state lent for
    /// the call, else the VM's own.
    pub(super) const fn native_draw(&mut self, vm: &mut Vm<'_>, bound: i32) -> i32 {
        match self.rngs.lent.as_mut() {
            Some(state) => crate::base::rng::draw(state, bound),
            None => vm.draw(bound),
        }
    }
}
