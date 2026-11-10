//! The advisor's timed lines: four threads of the local player, polled
//! each frame. **Binary** unless marked.
//!
//! Each machine polled its own human's threads; here every human seat
//! ([`World::humans`]) has its own, polled in seat order, so play is
//! alike on every machine. A line is cued for its seat alone.
//!
//! ```text
//!  game frame step, state 3, after the periodic hit point tick
//!    the local player's four Pseudo_LowPriorityThread, in order:
//!      Thread_LowTreasury      APP:0 under trigger        RA09
//!      Thread_NeedHeroes       no hero in unit list 1     RA11
//!      Thread_ElvesAvailable   ABc1 offered, no ABc       RA02, then off
//!      Thread_DwarvesAvailable ABb1 offered, no ABb       RA03, then off
//!
//!  each: when on, due when now − last > interval (signed);
//!  then last = now. A line is GVS5, played by GE_SoundManager:
//!  unplaced, the local player's.
//! ```
//!
//! The elves' and dwarves' threads are saved (`Pseudo_LowPriorityThread`),
//! so a save shows when each last ran and whether it is on.
//! `ElvesVoice_SetOperative` and `DwarvesVoice_SetOperative` set whether theirs is on;
//! `SetTreasuryValues` the low treasury's interval and trigger.

use std::sync::Arc;

use super::{Answer, World, int_ref};
use crate::base::digest::Map;
use crate::data::deps::Expr;
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::vm::{Fault, Handle, Vm};

use super::offer::Menu;
use super::sound::ADVISOR;

/// `RA09 Advisor_Empty_Treasury`: the low treasury's line.
pub const LOW_TREASURY: FourCc = FourCc::new(b"RA09");

/// `RA11 Advisor_Need_Heroes`.
pub const NEED_HEROES: FourCc = FourCc::new(b"RA11");

/// `RA02 Advisor_Elves_Join`.
pub const ELVES: FourCc = FourCc::new(b"RA02");

/// `RA03 Advisor_Dwarves`.
pub const DWARVES: FourCc = FourCc::new(b"RA03");

/// The building whose offer the elves' thread waits for, and the code
/// prefix whose level it asks (`0x31634241`, `0x634241`).
pub const ELVEN_BUNGALOW: (FourCc, [u8; 3]) = (FourCc::new(b"ABc1"), *b"ABc");

/// The dwarves' (`0x31624241`, `0x624241`).
pub const DWARVEN_SETTLEMENT: (FourCc, [u8; 3]) = (FourCc::new(b"ABb1"), *b"ABb");

/// The GPL constants the threads read at the player's making:
/// 180,000, 1,000, 60,000,
/// 20,000 and 20,000 in `globals.gpl`.
pub const TREASURY_TIMEOUT: &str = "#advisor_timeout_low_treasury";
pub const TREASURY_TRIGGER: &str = "#advisor_trigger_amount_low_treasury";
pub const HEROES_TIMEOUT: &str = "#advisor_timeout_need_heroes";
pub const ELVES_TIMEOUT: &str = "#advisor_timeout_elves_available";
pub const DWARVES_TIMEOUT: &str = "#advisor_timeout_dwarves_available";

/// One thread: last, interval, the clock it was
/// last polled at, on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watch {
    pub last: u32,
    pub interval: i32,
    pub polled: u32,
    pub on: bool,
}

impl Default for Watch {
    /// As made: on, the rest 0.
    fn default() -> Watch {
        Watch {
            last: 0,
            interval: 0,
            polled: 0,
            on: true,
        }
    }
}

impl Watch {
    /// Polled at `now`: whether it runs.
    pub const fn due(&mut self, now: u32) -> bool {
        if self.on {
            self.polled = now;

            let due = self.interval < now.wrapping_sub(self.last).cast_signed();

            if due {
                self.last = now;
            }

            due
        } else {
            false
        }
    }
}

/// One player's four threads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Advisors {
    pub treasury: Watch,
    /// The low treasury's trigger,.
    pub trigger: i32,
    pub heroes: Watch,
    pub elves: Watch,
    pub dwarves: Watch,
}

/// Each human seat's threads, and what they need of the data: without
/// it, none runs.
#[derive(Clone, Debug, Default)]
pub(super) struct Seats {
    /// Shared: each frame's polls hold it while the world changes.
    rules: Option<Arc<Rules>>,
    threads: Map<u32, Advisors>,
}

/// The `BDEP` rules of the elves' and dwarves' buildings.
#[derive(Clone, Debug)]
struct Rules {
    elves: Option<Expr>,
    dwarves: Option<Expr>,
}

impl World {
    /// Run each human seat's advisor threads with `menu`'s rules and
    /// `program`'s intervals, each thread's on word kept.
    #[must_use]
    pub fn with_advisors(mut self, menu: &Menu, program: &Program) -> World {
        let constant = |name: &str| match program.constant(name) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };
        let humans = self.humans();
        let seats = &mut self.advisors;

        seats.rules = Some(Arc::new(Rules {
            elves: menu.rule(ELVEN_BUNGALOW.0).cloned(),
            dwarves: menu.rule(DWARVEN_SETTLEMENT.0).cloned(),
        }));

        for seat in humans {
            let a = seats.threads.entry(seat).or_default();

            a.treasury.interval = constant(TREASURY_TIMEOUT);
            a.trigger = constant(TREASURY_TRIGGER);
            a.heroes.interval = constant(HEROES_TIMEOUT);
            a.elves.interval = constant(ELVES_TIMEOUT);
            a.dwarves.interval = constant(DWARVES_TIMEOUT);
        }

        self
    }

    /// Player `player`'s advisor threads; `None` but for a human seat.
    #[must_use]
    pub fn advisors(&self, player: u32) -> Option<&Advisors> {
        self.advisors.threads.get(&player)
    }

    /// The frame's four polls, in the engine's order, for each human
    /// seat in turn.
    pub(super) fn advisor_polls(&mut self) {
        if let Some(rules) = self.advisors.rules.clone() {
            for seat in self.humans() {
                self.seat_polls(seat, &rules);
            }
        }
    }

    /// Seat `seat`'s four polls.
    fn seat_polls(&mut self, seat: u32, rules: &Rules) {
        let now = self.now;
        let due = |w: &mut World, pick: fn(&mut Advisors) -> &mut Watch| {
            w.advisors
                .threads
                .get_mut(&seat)
                .is_some_and(|a| pick(a).due(now))
        };

        if due(self, |a| &mut a.treasury) && self.low_treasury(seat) {
            self.advisor_says(LOW_TREASURY, seat);
        }

        if due(self, |a| &mut a.heroes) && !self.has_hero(seat) {
            self.advisor_says(NEED_HEROES, seat);
        }

        if due(self, |a| &mut a.elves) {
            self.newcomers(Newcomer::Elves, rules.elves.as_ref(), seat);
        }

        if due(self, |a| &mut a.dwarves) {
            self.newcomers(Newcomer::Dwarves, rules.dwarves.as_ref(), seat);
        }
    }

    /// `Thread_LowTreasury`: `player`'s treasury, `APP:0`, under its
    /// trigger.
    fn low_treasury(&self, player: u32) -> bool {
        let trigger = self.advisors(player).map_or(0, |a| a.trigger);

        self.treasury(player).is_some_and(|gold| gold < trigger)
    }

    /// `Thread_NeedHeroes`: a hero of `player`'s in unit list 1,
    /// lying dead or not. **Inferred**: list 1 is
    /// the world's mobiles.
    fn has_hero(&self, player: u32) -> bool {
        self.pieces.iter().any(|(id, p)| {
            p.owner == player && p.place.is_some_and(|at| at.is_vehicle()) && self.is_hero(*id)
        })
    }

    /// The elves' or dwarves' thread, due: its building offered — its
    /// `BDEP` rule met, not disabled,
    /// under its cap —
    /// turns it off, saying its line where `player` has none of the
    /// prefix (player building level 0).
    fn newcomers(&mut self, who: Newcomer, rule: Option<&Expr>, player: u32) {
        let ((code, prefix), line) = match who {
            Newcomer::Elves => (ELVEN_BUNGALOW, ELVES),
            Newcomer::Dwarves => (DWARVEN_SETTLEMENT, DWARVES),
        };
        let offered =
            self.rule_met(player, rule) && !self.disabled(code) && self.under_cap(player, code);

        if offered {
            if self.building_level(player, prefix) == 0 {
                self.advisor_says(line, player);
            }

            if let Some(a) = self.advisors.threads.get_mut(&player) {
                newcomers_of(a, who).on = false;
            }
        }
    }

    /// `line`'s `GVS5`, unplaced, `player`'s, heard by `player` alone.
    fn advisor_says(&mut self, line: FourCc, player: u32) {
        self.cue_seat(line, ADVISOR, player);
    }

    /// `ElvesVoice_SetOperative(n)`, gpl elves voice set operative, and
    /// `DwarvesVoice_SetOperative(n)`, gpl dwarves voice set operative: the local player's
    /// thread on while `n` is not 0; here every human
    /// seat's, as every machine ran the script.
    pub(super) fn voice_operative(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        who: Newcomer,
    ) -> Result<Answer, Fault> {
        let on = int_ref(vm, args, 1)? != 0;

        for a in self.advisors.threads.values_mut() {
            newcomers_of(a, who).on = on;
        }

        Ok(Answer::Given)
    }

    /// `SetTreasuryValues(interval, trigger)`, gpl set treasury values: the local
    /// player's low treasury thread's; here every
    /// human seat's.
    pub(super) fn treasury_values(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let interval = int_ref(vm, args, 1)?;
        let trigger = int_ref(vm, args, 2)?;

        for a in self.advisors.threads.values_mut() {
            a.treasury.interval = interval;
            a.trigger = trigger;
        }

        Ok(Answer::Given)
    }
}

/// Which newcomers' thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Newcomer {
    Elves,
    Dwarves,
}

/// `who`'s thread of `a`.
const fn newcomers_of(a: &mut Advisors, who: Newcomer) -> &mut Watch {
    match who {
        Newcomer::Elves => &mut a.elves,
        Newcomer::Dwarves => &mut a.dwarves,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Due past its interval, not at it: a 20,000 ms thread polled each
    /// 50 ms frame runs at 20,050, then 40,100 (the saves' 100,250,
    /// 120,300, 240,600 are its multiples).
    #[test]
    fn a_thread_runs_past_its_interval() {
        let mut w = Watch {
            interval: 20_000,
            ..Watch::default()
        };
        let ran: Vec<u32> = (1..=2000)
            .map(|f| f * 50)
            .filter(|now| w.due(*now))
            .take(3)
            .collect();

        assert_eq!(ran, [20_050, 40_100, 60_150]);
        assert_eq!(w.polled, 60_150);
    }

    /// Off, it is not polled: keeps the clock it went off at.
    #[test]
    fn an_off_thread_keeps_its_last_poll() {
        let mut w = Watch {
            interval: 20_000,
            ..Watch::default()
        };

        assert!(!w.due(100));

        w.on = false;

        assert!(!w.due(30_000));
        assert_eq!((w.interval, w.polled, w.on), (20_000, 100, false));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save as m;
    use std::collections::BTreeMap;

    const fn watch(w: Watch) -> m::Watch {
        m::Watch {
            last: w.last,
            interval: w.interval,
            polled: w.polled,
            on: w.on,
        }
    }

    const fn unwatch(w: m::Watch) -> Watch {
        Watch {
            last: w.last,
            interval: w.interval,
            polled: w.polled,
            on: w.on,
        }
    }

    impl Advisors {
        /// The threads' state.
        const fn watches(&self) -> m::Advisors {
            let Advisors {
                treasury,
                trigger,
                heroes,
                elves,
                dwarves,
            } = *self;

            m::Advisors {
                treasury: watch(treasury),
                trigger,
                heroes: watch(heroes),
                elves: watch(elves),
                dwarves: watch(dwarves),
            }
        }

        /// The saved threads' state.
        const fn rewatch(model: m::Advisors) -> Advisors {
            Advisors {
                treasury: unwatch(model.treasury),
                trigger: model.trigger,
                heroes: unwatch(model.heroes),
                elves: unwatch(model.elves),
                dwarves: unwatch(model.dwarves),
            }
        }
    }

    impl Seats {
        /// Each seat's threads' state; the rules are the dataset's.
        pub(in crate::sim::world) fn watches(&self) -> BTreeMap<u32, m::Advisors> {
            self.threads
                .iter()
                .map(|(seat, a)| (*seat, a.watches()))
                .collect()
        }

        /// The saved seats' threads, the rules kept.
        pub(in crate::sim::world) fn rewatch(&mut self, model: BTreeMap<u32, m::Advisors>) {
            self.threads = model
                .into_iter()
                .map(|(seat, a)| (seat, Advisors::rewatch(a)))
                .collect();
        }
    }
}
