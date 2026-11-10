//! A panel's change to one unit's attribute: `AlterAttributeDeltaMessage`.
//! **Binary** unless marked.
//!
//! ```text
//!  Reward2Controller, button 0x138b   a flag's APR:1 plus #RewardDelta
//!  TaxCollectorController, 0xb–0xe    APV:48 or APV:49 by
//!                                     ±#Taxman_increment, within bounds
//!    └─ the message: the unit's handle, the key, the change,
//!       and a range
//!  the next step, before the clock step, each queued message in turn:
//!  type 0xc, MessageManager
//!       v = attribute(key) + change
//!       range empty (min = max) or min <= v <= max: GSUnit writes
//!       the attribute, as SetAttribute does
//! ```
//!
//! err.log logs `SendMessage frame s` and no decode line
//! (`GSMessageManager` names only its own cases).
//! The low-level dispatch runs it at once, as
//! a decode runs, between the decodes queued before and after it
//! (**Inferred**: the queue is first in, first out). So a game applies it
//! in its place among the frame's inputs ([`World::alter`]). **Game**: the
//! trace checks applied it in the step of `s + 2`, before the hit point
//! tick, as the message comes before it.
//!
//! An `AlterAttributeMessage` (type 8) sets instead: `MessageManager`
//! hands its attribute map to `GSUnit`, which writes
//! each key in order. The fast bar sends one as the
//! Abode's spell timer runs out (`World::abode_bar`); it lands before the
//! next frame's inputs (**Inferred**: the bar's send, in the step's
//! interface poll, comes before a click taken in the pacing wait).

use super::World;
use super::spell::cast::{ABODE, ABODE_LASTS};
use crate::data::attrib::{Key, SORCERERS_ABODE_SPELL_START_TIME};
use crate::gpl::runtime::MS_PER_FRAME;

/// One `AlterAttributeDeltaMessage`: err.log's send frame, the unit's
/// agent id, the key, the change, and the range (`(0, 0)` for none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Alter {
    pub frame: u32,
    pub agent: u32,
    pub key: Key,
    pub change: i32,
    pub range: (i32, i32),
}

/// One `AlterAttributeMessage`: err.log's send frame, the unit's agent
/// id, and each key it sets with its value, in key order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assign {
    pub frame: u32,
    pub agent: u32,
    pub words: Vec<(i32, i32)>,
}

/// One `AlterAttributeMessage` of one key, as a panel sends it: the
/// unit's agent id, the key and its value. The Gambling Hall's panel
/// sends its wager and its colour so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Set {
    pub agent: u32,
    pub key: Key,
    pub value: i32,
}

/// Frames from a send to the step that applies it: decoded the frame
/// after, applied the frame after that.
pub const SEND_TO_STEP: u32 = 2;

/// Attribute settings still to come.
#[derive(Clone, Debug, Default)]
pub(super) struct Alters {
    /// By frame ([`Assign`]).
    assigns: Vec<Assign>,
}

impl World {
    /// The settings due now: each sent [`SEND_TO_STEP`] frames before, in
    /// order. A game lands them before its frame's decodes, as message
    /// queue run does; [`World::step`] lands any left.
    pub fn alter_step(&mut self) {
        let now = self.now;
        let (set, wait): (Vec<Assign>, Vec<Assign>) =
            std::mem::take(&mut self.pending.alters.assigns)
                .into_iter()
                .partition(|a| (a.frame + SEND_TO_STEP) * MS_PER_FRAME <= now);

        self.pending.alters.assigns = wait;

        for (agent, (at, v)) in set
            .into_iter()
            .flat_map(|a| a.words.into_iter().map(move |w| (a.agent, w)))
        {
            self.write(agent, at, v);
        }
    }

    /// Fast bar abode timer for the local player,
    /// here each human seat's in turn ([`World::humans`]), every frame:
    /// the spell bar is always on
    /// screen (**Game**, the user), before the frame's
    /// messages land: past the Abode's timer, `clock - ASS:0` above
    /// `ASP:0`, a message setting both 0, sent in the frame whose
    /// interface sees this step's clock. Sent again each frame until it
    /// lands, as the engine's is. **Game**: the wizard trace's Gate,
    /// cast at 110,600 ms for 80,000, sends at frames 3812 and 3813, and
    /// the save holds both words 0.
    pub(super) fn abode_bar(&mut self) {
        for seat in self.humans() {
            self.seat_abode_bar(seat);
        }
    }

    /// [`World::abode_bar`] for `seat`.
    fn seat_abode_bar(&mut self, seat: u32) {
        let abode = self.best_building(seat, ABODE);
        let word = |id: u32, k| self.word_of(id, k);
        let now = self.now.cast_signed();
        let spent = abode.filter(|id| {
            let begun = word(*id, SORCERERS_ABODE_SPELL_START_TIME);

            begun != 0 && word(*id, ABODE_LASTS) < now.wrapping_sub(begun)
        });

        if let Some(agent) = spent {
            self.pending.alters.assigns.push(Assign {
                frame: self.now / MS_PER_FRAME - 1,
                agent,
                words: vec![
                    (ABODE_LASTS.packed(), 0),
                    (SORCERERS_ABODE_SPELL_START_TIME.packed(), 0),
                ],
            });
        }
    }

    /// `MessageManager` for `s`, as its decode runs: the key
    /// written. No piece, nothing.
    pub fn assign(&mut self, s: &Set) {
        if self.pieces.contains(s.agent) {
            self.write(s.agent, s.key.packed(), s.value);
        }
    }

    /// A bare piece for agent `id`, for tests of the messages.
    #[cfg(test)]
    pub(crate) fn add_bare_piece(&mut self, id: u32) {
        self.pieces.insert(id, super::Piece::default());
    }

    /// `piece` for agent `id`, for tests of the messages.
    #[cfg(test)]
    pub(crate) fn add_test_piece(&mut self, id: u32, piece: super::Piece) {
        self.pieces.insert(id, piece);
    }

    /// `a` sent, as [`World::abode_bar`] sends one, for tests.
    #[cfg(test)]
    pub(crate) fn assign_later(&mut self, a: Assign) {
        self.pending.alters.assigns.push(a);
    }

    /// `MessageManager` for `a`, as its decode runs. No piece,
    /// nothing.
    pub fn alter(&mut self, a: &Alter) {
        let at = a.key.packed();
        let old = self.pieces.get(a.agent).map(|p| p.word(a.key));

        if let Some(old) = old {
            let (min, max) = a.range;
            let v = old + a.change;

            if min == max || (min..=max).contains(&v) {
                self.write(a.agent, at, v);
            }
        }
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Alters {
        type Model = m::Alters;

        fn save(&self) -> m::Alters {
            m::Alters {
                assigns: self.assigns.save(),
            }
        }

        fn load(model: m::Alters) -> Result<Alters, Refused> {
            Ok(Alters {
                assigns: Vec::load(model.assigns)?,
            })
        }
    }
}
