//! A building's GS timers: a recruit's and a research's orders, which
//! hold it in ORDR while they run. **Binary** unless marked.
//!
//! ```text
//!  a recruit's decode   GS_OrderProcessor       GS_OrderProcessor:       kind 0x4002,
//!                       the recruit delay, callback 0x2006
//!  a research's decode  GS_OrderProcessor       GS_OrderProcessor:       kind 0x4005,
//!                       its time, callback 0x2009
//!  an enchantment       GS_OrderProcessor:       kind 0x4006, the
//!                       duration, callback 0x200a
//!  a Market Day         GS_OrderProcessor: kind 0x4004, the
//!                       duration, callback 0x2008
//!  a gamble's spin      GS_OrderProcessor: kind 0x4007,
//!                       #gambling_interval, callback 0x200b
//!    each pushed on the building, which joins the back of ORDR
//!  the building's turn  ends it once now - when >= delta
//!                       (OrderStack); the callback runs there:
//!                       recruit done makes the hero,
//!                       research done sets the
//!                       attribute, wiz tower enchant done ends the enchantment,
//!                       market day end ends the Market Day,
//!                       gamble end ends the spin
//! ```
//!
//! **Game**: play3's recruiting guild and the free probe's researching
//! blacksmith stand in their saves' ORDR where these orders put them.

use super::{Listed, World, recruit::Recruit};

/// What a timer's end does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Timer {
    /// Callback `0x2006`, recruit done.
    Recruit(Recruit),
    /// Callback `0x2009`, research done callback.
    Research,
    /// A wizards' tower's enchantment, kind `0x4006`: callback `0x200a`.
    Enchant,
    /// A marketplace's Market Day, kind `0x4004`: callback `0x2008`.
    MarketDay,
    /// An open embassy's recruit timer, kind `0x4009`, ticks left: a run
    /// is `GS_EmbassyRecruitOrder` ([`super::guild::embassy`]).
    Embassy(i32),
    /// A bounty's `Wait` on the player's palace: callback `0x200c`
    /// (hall timer end, [`super::guild::bounty`]).
    Hall(u32),
    /// A Gambling Hall's spin, kind `0x4007`: callback `0x200b`
    /// (gamble end, [`super::guild::gamble`]).
    Gamble,
}

/// A GS timer on a building.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Timed {
    unit: u32,
    timer: Timer,
    /// Pushed at, game ms.
    when: u32,
    /// Runs for, ms.
    delta: u32,
    /// Its counter,: [`Count::Reset`] re-arms at its next run.
    count: Count,
}

/// `GS_BaseDelayedOrder`: a run with it 0 sets it 1 and restarts
/// the time; with it 1, the
/// callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Count {
    Armed,
    Reset,
}

/// Buildings' GS timers, in the order pushed.
#[derive(Clone, Debug, Default)]
pub(super) struct Timers(Vec<Timed>);

impl World {
    /// Push `timer` on agent `unit`'s building for `delta` ms: it joins
    /// the back of ORDR where it is not listed.
    pub(super) fn time(&mut self, unit: u32, timer: Timer, delta: u32) {
        self.pending.timers.0.push(Timed {
            unit,
            timer,
            when: self.now,
            delta,
            count: Count::Armed,
        });

        self.dispatches.list(Listed::Unit(unit));
    }

    /// Agent `unit`'s timers, in its turn: each whose time is up ends,
    /// its callback run. A hero to make stops the orders there, as a
    /// dispatch does, so the caller makes it in its place.
    pub(super) fn run_timers(&mut self, unit: u32) {
        let now = self.now;
        let (due, left): (Vec<Timed>, Vec<Timed>) = self
            .pending
            .timers
            .0
            .iter()
            .partition(|t| t.unit == unit && now.wrapping_sub(t.when) >= t.delta);

        self.pending.timers.0 = left;

        // A reset counter: the time starts again, no callback.
        let (rearmed, due): (Vec<Timed>, Vec<Timed>) =
            due.into_iter().partition(|t| t.count == Count::Reset);

        self.pending
            .timers
            .0
            .extend(rearmed.into_iter().map(|t| Timed {
                when: now,
                count: Count::Armed,
                ..t
            }));

        for t in due {
            match t.timer {
                Timer::Recruit(r) => {
                    self.pending.hires.arrive(r);
                    self.dispatches.counted();
                }
                Timer::Research => self.research_over(unit),
                Timer::Enchant => self.enchant_over(unit),
                Timer::MarketDay => self.market_day_over(unit),
                Timer::Embassy(left) => self.embassy_tick(unit, left),
                Timer::Hall(player) => self.bounty_over(player),
                Timer::Gamble => self.gamble_over(unit),
            }
        }
    }

    /// Agent `unit`'s embassy timer gone (`GenUnit` method `(1, 0x4009)`).
    pub(super) fn untime_embassy(&mut self, unit: u32) {
        self.pending
            .timers
            .0
            .retain(|t| !(t.unit == unit && matches!(t.timer, Timer::Embassy(_))));
    }

    /// Agent `unit`'s enchantment timer, its counter reset:
    /// whether it had one.
    pub(super) fn reset_enchant(&mut self, unit: u32) -> bool {
        self.reset(unit, Timer::Enchant)
    }

    /// Agent `unit`'s Market Day timer, its counter reset:
    /// whether it had one.
    pub(super) fn reset_market_day(&mut self, unit: u32) -> bool {
        self.reset(unit, Timer::MarketDay)
    }

    /// Agent `unit`'s `timer`, its counter reset: whether it had one.
    fn reset(&mut self, unit: u32, timer: Timer) -> bool {
        let found = self
            .pending
            .timers
            .0
            .iter_mut()
            .find(|t| t.unit == unit && t.timer == timer);

        found.map(|t| t.count = Count::Reset).is_some()
    }

    /// Whether agent `unit` holds `timer`.
    pub(super) fn timed(&self, unit: u32, timer: Timer) -> bool {
        self.pending
            .timers
            .0
            .iter()
            .any(|t| t.unit == unit && t.timer == timer)
    }

    /// Whether agent `unit` holds a timer, its piece standing.
    pub(super) fn timing(&self, unit: u32) -> bool {
        self.pieces.contains(unit) && self.pending.timers.0.iter().any(|t| t.unit == unit)
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Timer {
        type Model = m::Timer;

        fn save(&self) -> m::Timer {
            match self {
                Timer::Recruit(r) => m::Timer::Recruit(r.save()),
                Timer::Research => m::Timer::Research,
                Timer::Enchant => m::Timer::Enchant,
                Timer::MarketDay => m::Timer::MarketDay,
                Timer::Embassy(n) => m::Timer::Embassy(*n),
                Timer::Hall(p) => m::Timer::Hall(*p),
                Timer::Gamble => m::Timer::Gamble,
            }
        }

        fn load(model: m::Timer) -> Result<Timer, Refused> {
            Ok(match model {
                m::Timer::Recruit(r) => Timer::Recruit(Recruit::load(r)?),
                m::Timer::Research => Timer::Research,
                m::Timer::Enchant => Timer::Enchant,
                m::Timer::MarketDay => Timer::MarketDay,
                m::Timer::Embassy(n) => Timer::Embassy(n),
                m::Timer::Hall(p) => Timer::Hall(p),
                m::Timer::Gamble => Timer::Gamble,
            })
        }
    }

    impl Saved for Timed {
        type Model = m::Timed;

        fn save(&self) -> m::Timed {
            let Timed {
                unit,
                timer,
                when,
                delta,
                count,
            } = self;

            m::Timed {
                unit: *unit,
                timer: timer.save(),
                when: *when,
                delta: *delta,
                count: match count {
                    Count::Armed => m::Count::Armed,
                    Count::Reset => m::Count::Reset,
                },
            }
        }

        fn load(model: m::Timed) -> Result<Timed, Refused> {
            Ok(Timed {
                unit: model.unit,
                timer: Timer::load(model.timer)?,
                when: model.when,
                delta: model.delta,
                count: match model.count {
                    m::Count::Armed => Count::Armed,
                    m::Count::Reset => Count::Reset,
                },
            })
        }
    }

    impl Saved for Timers {
        type Model = Vec<m::Timed>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Timers, Refused> {
            Vec::load(model).map(Timers)
        }
    }
}
