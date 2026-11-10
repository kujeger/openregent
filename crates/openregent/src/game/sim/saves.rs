//! Saves a step takes: asked for by name, or the autosave's.

use super::*;

impl Sim {
    /// The game at this frame's end, as a save holds it: the orders given
    /// since among those to come.
    ///
    /// # Errors
    ///
    /// A game that stopped.
    pub fn snapshot(&self) -> Result<regent_core::save::Snapshot, String> {
        let mut snapshot = match &self.stopped {
            Some(why) => Err(format!("a stopped game does not save: {why}")),
            None => self.game.snapshot().map_err(|e| e.to_string()),
        }?;
        let given = self.given().filter_map(|e| match e {
            Event::Input { before, order } => Some((before, order.to_string())),
            Event::PlayOn { .. } | Event::Surrender { .. } | Event::Digest { .. } => None,
        });

        snapshot.game.inputs.extend(given);
        Ok(snapshot)
    }

    /// Autosave every `days` game days from now on; `None`, never.
    pub const fn set_autosave(&mut self, days: Option<u32>) {
        self.saves.every = days;
    }

    /// The autosaves steps took since the last call.
    pub fn take_autosaves(&mut self) -> Vec<crate::save::auto::Taken> {
        std::mem::take(&mut self.saves.auto)
    }

    /// A save named `name` asked of every peer: taken as the step the
    /// delay ahead leaves the game ([`Sim::take_saves`]).
    pub fn ask_save(&mut self, name: &str) {
        self.peer.ask_save(name);
    }

    /// The saves steps took since the last call.
    pub fn take_saves(&mut self) -> Vec<crate::save::Stepped> {
        std::mem::take(&mut self.saves.asked)
    }

    /// The saves the peers asked for at this step: the game as the step
    /// left it, on every peer.
    pub(super) fn save_asked(&mut self) {
        for (seat, name) in self.peer.take_saves() {
            let game = crate::save::FromSim::of(self);
            let nick = self.peer.nick(seat);

            self.saves.asked.push(crate::save::Stepped {
                seat,
                nick,
                name,
                game,
            });
        }
    }

    /// An autosave due as this step leaves the game, as the original
    /// tests it before the next (`view::menu::autosave`): every peer's at
    /// the same frame, the game being one. None once the game ended.
    pub(super) fn save_due(&mut self) {
        let due = self
            .saves
            .every
            .is_some_and(|days| autosave::due(self.frames(), autosave::frames(days)));
        let ended = self.world().ended(self.local()).is_some();

        if due && !ended {
            let game = crate::save::FromSim::of(self);
            let day = regent_core::view::panel::day::Day::of(self.world().now()).number;

            self.say(chat::Line::Text(autosave::NOTICE.index));
            self.saves.auto.push(crate::save::auto::Taken { day, game });
        }
    }
}

/// Saves steps took, for the interface to write, and when the next is
/// due.
#[derive(Default)]
pub(super) struct Saves {
    /// LAN saves taken at a step.
    asked: Vec<crate::save::Stepped>,
    /// Game days between autosaves, this machine's setting; `None` off.
    every: Option<u32>,
    /// Autosaves taken at a step.
    auto: Vec<crate::save::auto::Taken>,
}
