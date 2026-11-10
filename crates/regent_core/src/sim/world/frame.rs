//! The frame's step: game frame step's hit point tick and timers,
//! then `GE_Level`'s orders, then what follows them; and the engine
//! dispatches the step makes, for the host to run between passes.
//! **Binary** unless marked.
//!
//! ```text
//!  World::step ── last frame's sight pass (after its scheduler)
//!     │           abode bar, alters, hires, mend, advisors, research,
//!     │           spell timeouts, effectors left
//!     v
//!  ORDR snapshot ── walk: each listed unit's orders (walking); paused at
//!     │             a dispatch until World::resume
//!     v
//!  after the orders ── clones, falls, marks, restyles, plumes, looks,
//!                      sight
//! ```

use super::{Dispatch, Listed, World};
use crate::gpl::ops::Value;

/// Whether the world sees frames: a new game with no log steps them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Frames {
    #[default]
    Unseen,
    Stepped,
}

impl World {
    /// The engine dispatches builtins here made since the last call, in
    /// order: the host runs them where the builtin returns.
    pub fn take_queued(&mut self) -> Vec<Dispatch> {
        std::mem::take(&mut self.dispatches.queued)
    }

    /// The engine dispatches the frame's step made, in order, for a host
    /// with no log to run between passes: the tick's deaths, then each
    /// order's callback, an action's function or a missile's impact.
    ///
    /// **Inferred**: run after the step, not inside it, so an order later
    /// in `GE_Level` does not see what an earlier one's function did.
    ///
    /// Each comes with whether it is a script dispatch call, which
    /// [`World::stands`] decides when it runs.
    pub fn take_between(&mut self) -> Vec<(Dispatch, bool)> {
        let scripted = std::mem::take(&mut self.dispatches.scripted);

        std::mem::take(&mut self.dispatches.between)
            .into_iter()
            .enumerate()
            .map(|(i, d)| (d, scripted.contains(&i)))
            .collect()
    }

    /// Whether a script dispatch call with `args` runs now: the
    /// actor and the target each need their agent, the unit's `AE`
    /// attribute. One killed earlier in the frame's orders,
    /// its agent deleted, leaves the call unmade. **Game**: the no-budget
    /// defeat game, 302,000 ms: champion 11's blow razes Wizards Tower 4,
    /// and ratman 16's blow at it, peaking in the same frame, runs no
    /// `make_attack`.
    #[must_use]
    pub fn stands(&self, args: &[Value]) -> bool {
        args.iter().all(|v| match v {
            Value::Agent(id) => self.agents.alive(*id),
            _ => true,
        })
    }

    /// [`World::dispatch`] of a script dispatch call.
    pub(super) fn dispatch_script(&mut self, function: &str, args: Vec<Value>) {
        if self.dispatches.stepping {
            self.dispatches.scripted.push(self.dispatches.between.len());
        }

        self.dispatch(function, args);
    }

    /// `GE_Level`, after the orders: `UnitDelete` of each piece
    /// `DeleteGamePiece` deleted since, its agent's id an integer
    /// (`GE_Level` defers it), the last deleted first. **Game**, the
    /// probed Barren Waste: two explore flags deleted in one pass, 61 then
    /// 63, have their `UnitDelete`s 63 then 61. For a host with no log,
    /// after [`World::take_between`]'s dispatches. The scatter a clearing
    /// deleted goes first (`super::board::Board::clear_later`).
    pub fn take_deletes(&mut self) -> Vec<Dispatch> {
        self.drain_cleared();

        for id in self.dispatches.deleting.clone() {
            self.count_deleted(id);
            self.count_slain(id);
        }

        // A dead list keeps the drain's order, the last deleted first.
        for id in self.dispatches.deleting.clone().into_iter().rev() {
            self.count_dead(id);
        }

        std::mem::take(&mut self.dispatches.deleting)
            .into_iter()
            .rev()
            .map(|id| {
                (
                    super::UNIT_DELETE.to_owned(),
                    vec![Value::Integer(id.cast_signed())],
                )
            })
            .collect()
    }

    /// `GS_Level` slot 34's drain of the scatter a clearing deleted
    /// (`super::board::Board::clear_later`): after the frame's orders,
    /// before the scheduler. [`World::take_deletes`] runs it; a host
    /// taking `UnitDelete` from its log calls it itself.
    pub fn drain_cleared(&mut self) {
        self.board.drain(self.descriptors.bodies());
    }

    /// The frame's periodic hit point tick, then its orders, before the
    /// scheduler: game frame step runs the tick after the clock
    /// step, `GE_Level` the orders,
    /// then gpl scheduler tick. From the first call on, the world
    /// follows walkers. `gpl` is the GPL RNG engine code draws on.
    pub fn step(&mut self, gpl: Option<&mut u32>) {
        self.lend(gpl, World::frame_step);
    }

    /// [`World::step`] on the lent RNG.
    fn frame_step(&mut self) {
        // The last frame's sight pass, after its scheduler.
        if self.frames == Frames::Stepped {
            self.sight_pass();
        }

        self.frames = Frames::Stepped;
        self.dispatches.between.clear();
        self.dispatches.scripted.clear();
        self.dispatches.stepping = true;
        self.abode_bar();
        self.alter_step();
        self.mend();
        self.advisor_polls();
        self.research_step();
        self.expire();
        self.overlays_left();

        let snapshot = self.dispatches.ordr.clone();

        self.walk_on(snapshot);
    }

    /// The orders from `listed`, then what follows them; paused at a
    /// dispatch, the world steps no more until [`World::resume`], so the
    /// dispatch's own dispatches run inside it, as the engine's do.
    fn walk_on(&mut self, listed: Vec<Listed>) {
        if self.walk(listed) {
            self.after_orders();
        } else {
            self.dispatches.stepping = false;
        }
    }

    /// Whether the frame's orders stopped at an engine dispatch.
    #[must_use]
    pub const fn paused(&self) -> bool {
        self.dispatches.rest.is_some()
    }

    /// The frame's orders on from where an impact paused them, as
    /// [`World::step`] runs them; whether they were paused.
    pub fn resume(&mut self, gpl: Option<&mut u32>) -> bool {
        match self.dispatches.rest.take() {
            Some(rest) => {
                self.dispatches.stepping = true;
                self.lend(gpl, |world| world.walk_on(rest));
                true
            }
            None => false,
        }
    }

    /// What follows the frame's orders.
    fn after_orders(&mut self) {
        self.drop_clones();
        self.fall_step();
        self.marks_step();
        self.restyle_skipped();
        self.plume_step();
        self.relook();
        self.sight();
        self.dispatches.stepping = false;
    }

    /// An engine dispatch the world makes: between passes while the frame
    /// steps, else inside the running builtin.
    pub(super) fn dispatch(&mut self, function: &str, args: Vec<Value>) {
        if self.dispatches.stepping {
            self.dispatches.count += 1;
            self.dispatches.between.push((function.to_owned(), args));
        } else {
            self.dispatches.queued.push((function.to_owned(), args));
        }
    }
}

/// The engine dispatches the world makes, for the host to run, and list
/// ORDR, the units whose orders the frame's step runs.
#[derive(Clone, Debug, Default)]
pub(super) struct Dispatches {
    /// List ORDR: units with an order, in the order they joined.
    ordr: Vec<Listed>,
    /// Engine dispatches a builtin here made, for the host to run.
    queued: Vec<Dispatch>,
    /// Engine dispatches the frame's step made, between passes.
    between: Vec<Dispatch>,
    /// Indices into `between` of script dispatch calls: an
    /// action's function at its peak, a missile's impact. Their agents
    /// must stand when they run.
    scripted: Vec<usize>,
    /// Whether the frame's step runs: an engine dispatch it makes goes to
    /// `between`, else to `queued`.
    stepping: bool,
    /// Agents `DeleteGamePiece` deleted, whose `UnitDelete` the next
    /// `GE_Level` dispatches ([`World::take_deletes`]).
    deleting: Vec<u32>,
    /// Dispatches the frame's orders made, counted.
    count: usize,
    /// The rest of the frame's ORDR snapshot, where an impact paused it.
    rest: Option<Vec<Listed>>,
}

impl Dispatches {
    /// List ORDR, in its order.
    pub(super) fn ordr(&self) -> &[Listed] {
        &self.ordr
    }

    /// `listed` joins the back of ORDR, unless on it.
    pub(super) fn list(&mut self, listed: Listed) {
        if !self.ordr.contains(&listed) {
            self.ordr.push(listed);
        }
    }

    /// A new missile, effector or clone joins the back of ORDR.
    pub(super) fn join(&mut self, listed: Listed) {
        self.ordr.push(listed);
    }

    /// `listed` off ORDR.
    pub(super) fn unlist(&mut self, listed: Listed) {
        self.ordr.retain(|o| *o != listed);
    }

    /// ORDR kept to the entries `keep` holds to.
    pub(super) fn keep(&mut self, keep: impl FnMut(&Listed) -> bool) {
        self.ordr.retain(keep);
    }

    /// An engine dispatch for the host to run where the builtin returns.
    pub(super) fn queue(&mut self, dispatch: Dispatch) {
        self.queued.push(dispatch);
    }

    /// Agent `id` deleted: its `UnitDelete` comes after the orders.
    pub(super) fn delete(&mut self, id: u32) {
        if !self.deleting.contains(&id) {
            self.deleting.push(id);
        }
    }

    /// Whether agent `id` waits for its `UnitDelete`.
    pub(super) fn deleting(&self, id: u32) -> bool {
        self.deleting.contains(&id)
    }

    /// Agent `id`'s `UnitDelete` ran.
    pub(super) fn deleted(&mut self, id: u32) {
        self.deleting.retain(|d| *d != id);
    }

    /// Dispatches the frame's orders made so far.
    pub(super) const fn count(&self) -> usize {
        self.count
    }

    /// A dispatch the host makes itself counted: a recruit's arrival.
    pub(super) const fn counted(&mut self) {
        self.count += 1;
    }

    /// The frame's orders paused, `rest` still to run.
    pub(super) fn pause(&mut self, rest: Vec<Listed>) {
        self.rest = Some(rest);
    }

    /// The queued dispatches from `from` on moved between passes: a death
    /// in the frame's step.
    pub(super) fn requeue(&mut self, from: usize) {
        self.between.extend(self.queued.drain(from..));
    }

    /// How many dispatches wait in the builtin queue.
    pub(super) const fn queued(&self) -> usize {
        self.queued.len()
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Dispatches {
        type Model = m::Dispatches;

        /// The frame's own, empty between frames, where a save is taken,
        /// stay out: `between`, `scripted`, `stepping`, `rest`.
        fn save(&self) -> m::Dispatches {
            debug_assert!(self.between.is_empty() && self.scripted.is_empty());
            debug_assert!(!self.stepping && self.rest.is_none());

            m::Dispatches {
                ordr: self.ordr.save(),
                queued: self.queued.save(),
                deleting: self.deleting.clone(),
                count: self.count.save(),
            }
        }

        fn load(model: m::Dispatches) -> Result<Dispatches, Refused> {
            Ok(Dispatches {
                ordr: Vec::load(model.ordr)?,
                queued: Vec::load(model.queued)?,
                deleting: model.deleting,
                count: usize::load(model.count)?,
                ..Dispatches::default()
            })
        }
    }
}
