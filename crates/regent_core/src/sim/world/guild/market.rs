//! The marketplace's Market Day. **Binary** unless marked.
//!
//! ```text
//!  panel AP31, control 8200 MARKET DAY (hot key M), bar 8201, text 8826
//!    MarketplaceController:       APB:1 0 hides all three; APB:22 set:
//!      8201 a bar of now - APB:22 over #Market_Day_Duration, 8826
//!      "Market Day in progress", 8200 hidden; else 8200 shown while
//!      APB:37 (the research). No price
//!         GUILD_ACTION 10, the marketplace's handle
//!  decode GS_OrderProcessor: a 0x4004 order running has its
//!    counter zeroed; else, with an agent: APB:22 the clock,
//!    DoMarketDay(market) at once, then a timed order of
//!    #Market_Day_Duration, kind 0x4004, callback 0x2008
//!  end   market day end: ended normally: APB:22 0, EndMarketDay
//! ```
//!
//! `DoMarketDay` is the effect: `(duration / revenue_time) ×
//! revenue_amount`, ¾ of it plus a draw of the last ¼, plus
//! `RandomNumber(#market_day_bonus)` × level, by `Give_Gold`; the revenue
//! thread suspended, title and subtype `Closed`, occupants released.
//! `EndMarketDay` plays `Select`, resumes the thread and reopens.
//! **Game**: the market probe's, pressed in frame 1407, ends in 2307,
//! 45,000 ms on.

use crate::data::attrib::{FIRST_STAGE_BUILT, Key};
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::sim::world::World;
use crate::sim::world::research;
use crate::sim::world::timed::Timer;

/// What `GS_OrderProcessor` dispatches: the string `"DoMarketDay"`.
pub const MARKET_DAY: &str = "DoMarketDay";

/// What the timed order's callback `0x2008` dispatches, `"EndMarketDay"`.
pub const MARKET_DAY_END: &str = "EndMarketDay";

/// The timed order's length, read once into market day order ms (and by the panel
/// into market day duration).
pub const MARKET_DAY_DURATION: &str = "#Market_Day_Duration";

/// `APB:22`, `MarketTime`: the clock at the decode, 0 when over.
const MARKET_TIME: Key = Key::new(*b"APB", 22);

/// `APB:37`, `ResearchMarketDay`.
const RESEARCHED: Key = research::attribute(research::MARKET_DAY);

/// The Market Day controls as `MarketplaceController` sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// 8200 shown: the research done, no Market Day running.
    Ready,
    /// 8201 and 8826 shown: `elapsed` of `duration` game ms.
    Running { elapsed: i32, duration: i32 },
}

impl World {
    /// The Market Day controls of `owner`'s building `id`: `None` before
    /// its first stage, without the research, or for another's piece.
    #[must_use]
    pub fn market_day(&self, program: &Program, owner: u32, id: u32) -> Option<Market> {
        let piece = self.pieces.get(id).filter(|p| p.owner == owner)?;
        let word = |k| piece.word(k);
        let began = word(MARKET_TIME);

        match (word(FIRST_STAGE_BUILT), began, word(RESEARCHED)) {
            (0, ..) | (_, 0, 0) => None,
            (_, 0, _) => Some(Market::Ready),
            _ => Some(Market::Running {
                elapsed: self.now.cast_signed().wrapping_sub(began),
                duration: duration(program).cast_signed(),
            }),
        }
    }

    /// `GS_OrderProcessor` on agent `id`: whether it dispatches
    /// [`MARKET_DAY`]. A Market Day running runs a whole duration more
    /// from its end instead; no agent, nothing.
    pub fn market_day_now(&mut self, id: u32) -> bool {
        !self.reset_market_day(id) && self.agent(id).is_some()
    }

    /// Before [`MARKET_DAY`]'s dispatch: `APB:22` the clock.
    pub(in crate::sim::world) fn market_day_begins(&mut self, id: u32) {
        let now = self.now.cast_signed();

        self.write(id, MARKET_TIME.packed(), now);
    }

    /// After it: the timed order pushed.
    pub(in crate::sim::world) fn market_day_arm(&mut self, program: &Program, id: u32) {
        self.time(id, Timer::MarketDay, duration(program));
    }

    /// Callback `0x2008` on agent `id`, ended
    /// normally, in its ORDR turn: `APB:22` 0, then [`MARKET_DAY_END`].
    pub(in crate::sim::world) fn market_day_over(&mut self, id: u32) {
        self.write(id, MARKET_TIME.packed(), 0);
        self.dispatch(MARKET_DAY_END, vec![Value::Agent(id)]);
    }
}

/// [`MARKET_DAY_DURATION`], ms.
fn duration(program: &Program) -> u32 {
    match program.constant(MARKET_DAY_DURATION) {
        Some(Literal::Int(v)) => v.cast_unsigned(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKET: u32 = 7;

    fn names(world: &mut World) -> Vec<String> {
        world.take_queued().into_iter().map(|(n, _)| n).collect()
    }

    /// Its time is the clock at the decode; its end, a duration on, zeroes
    /// it and dispatches `EndMarketDay`.
    #[test]
    fn a_market_day_ends_its_duration_on() {
        let mut world = World::default();

        world
            .pieces
            .insert(MARKET, crate::sim::world::Piece::default());
        world.now = 70_350;
        world.market_day_begins(MARKET);
        world.time(MARKET, Timer::MarketDay, 45_000);

        let time = |w: &World| w.pieces[&MARKET].get(MARKET_TIME);

        assert_eq!(time(&world), Some(70_350));

        world.now = 115_300;
        world.run_timers(MARKET);
        assert_eq!(names(&mut world).len(), 0);

        world.now = 115_350;
        world.run_timers(MARKET);
        assert_eq!(time(&world), Some(0));
        assert_eq!(names(&mut world), [MARKET_DAY_END]);
    }

    /// Pressed while one runs, nothing is dispatched: its counter is
    /// zeroed, so it runs a whole duration more from its end.
    #[test]
    fn a_market_day_pressed_while_one_runs_runs_on() {
        let mut world = World::default();

        world
            .pieces
            .insert(MARKET, crate::sim::world::Piece::default());
        world.time(MARKET, Timer::MarketDay, 45_000);

        assert!(!world.market_day_now(MARKET));

        world.now = 45_000;
        world.run_timers(MARKET);
        assert_eq!(names(&mut world).len(), 0);

        world.now = 90_000;
        world.run_timers(MARKET);
        assert_eq!(names(&mut world), [MARKET_DAY_END]);
    }
}
