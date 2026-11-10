//! Effectors: units hung on another, what makes,
//! re-arms, tests and deletes them, and what a drawing reads of them.
//! **Binary** unless marked.
//!
//! ```text
//!  CreateEffector(agent, name, duration, [value], [option])
//!    GenUnit: re-arm one of the name whose Wait is pending, else
//!    a new unit of kind 1, the next handle; a Wait on track 1 unless
//!    infinite or -1
//!  CheckEffector(agent, name)    the name found in the descriptors, or
//!                                throw; no unit: false; else GenUnit:
//!                                a unit of that code attached (relation 1)
//!  DeleteEffector(agent, name)   with both, GenUnit: those deleted
//!  DeleteAllEffectors(agent)     no unit: throw; else every attached one
//!                                deleted (relation list 1)
//!  Wait ends, callback 0x100d    the effector ends: GenUnit, then
//!                                its end animation
//!  GenUnit           once per unit: with a
//!                                holder carrying an AE attribute and a
//!                                descriptor script, the script with the
//!                                holder's agent,
//!                                or the unit's own where its kind is 4: a
//!                                reward flag hung on it
//!  a unit's deletion             each effector deleted with its holder or
//!                                by name
//!
//!  drawn:
//!  GenUnit          an effector takes its holder's part 0
//!                               direction, then GSUnit places it
//!  GenUnit          the holder's attachment point,
//!                               else set 0xd0 (a building) or 8, point 3;
//!                               less the holder's place, y + 1
//!  GSUnit           then stacked by StackPriority
//!  animate                     set 0x50, then 0x40 at its end (callback
//!                              0x100c); else 0x40; else no change, the
//!                              first set; not for a 0 ms Wait
//!  end                         set 0x60 from frame 0; else no change
//!  IsoDrawList                 parts drawn in y order of the unit's
//!                              position, a tie after those listed: an
//!                              overlay's y + 1 draws it after its
//!                              holder, over it
//! ```
//!
//! **Inferred**: a building with no `Hotspot` hangs at set 8's point, not
//! set `0xd0`'s ([`super::missile::Hit`] reads set 8 only).

use super::stack::stack_offset;
use super::{
    Answer, Found, World, agent_ref, bodies::Poses, int_ref, plume::IMPACT_SET, put_bool,
    string_ref,
};
use crate::base::fixed::Fixed;
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::runtime;
use crate::gpl::runtime::MS_PER_FRAME;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::movement::action::{Look, frame_ms};
use crate::sim::world::piece_state;
use std::collections::{BTreeMap, BTreeSet};

/// An effector: a unit hung on another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effector {
    /// The agent it hangs on.
    pub agent: u32,
    /// Its descriptor's `HEAD` name, folded.
    pub name: String,
    pub handle: u32,
    /// Its `Wait` on track 1 (effector wait, callback `0x100d`): pushed
    /// or re-armed at `when`, for `duration` ms; `None` with `infinite` or
    /// a duration of -1, which push none.
    pub when: u32,
    pub duration: Option<u32>,
    /// When it is deleted, game ms, once its `Wait` ends: its end
    /// animation played (effector end, `super::spell::end_at`);
    /// `None` where its art leaves that unknown.
    pub end: Option<u32>,
    /// When its end script runs.
    pub ending: Ending,
    /// Whether its end script ran: `GenUnit` sets unit state bit
    /// `0x800` and runs it once, at its `Wait`'s end or its deletion.
    pub ended: bool,
    /// `CreateEffector`'s integer option: `GenUnit` keeps it on
    /// the `Animate` it pushes on each part keyed 1000, where
    /// `IsoUnitIMG` draws it (`Got_Gold`'s amount). `None` without.
    pub value: Option<i32>,
}

/// Whether an effector's `Wait` is pushed, or it hangs without one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    Timed,
    Infinite,
}

impl Span {
    const fn of(infinite: bool) -> Self {
        if infinite {
            Self::Infinite
        } else {
            Self::Timed
        }
    }
}

/// When an effector's end script, `GenUnit`, runs:
/// Effector end tests its `Wait`'s order bit 8.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ending {
    /// As its `Wait` ends, then its end set plays, callback `0x1005`.
    #[default]
    Wait,
    /// `CreateEffector`'s `after`: its end set plays first, and callback
    /// `0x100e` deletes it and runs the script as the set
    /// ends.
    After,
}

/// What [`World::detach`] takes off a unit: an effector by index, or a
/// reward flag by agent.
#[derive(Clone, Copy, Debug)]
enum Taken {
    Effector(usize),
    Flag(u32),
}

/// `CreateEffector`'s options: flag 8, a death script
/// later; flag `0x10`, no `Wait`.
const AFTER: &str = "after";

const INFINITE: &str = "infinite";

/// A duration that pushes no `Wait` (a compare with -1 in `GenUnit`).
const NO_WAIT: i32 = -1;

impl Effector {
    /// Whether it hangs on its unit at game time `now`: its `Wait`
    /// pending, none, or its end animation still playing. **Inferred**:
    /// gone at its `Wait`'s end where the art leaves the animation unknown.
    #[must_use]
    pub fn hangs(&self, now: u32) -> bool {
        self.duration.is_none() || self.waiting(now) || self.end.is_some_and(|e| now < e)
    }

    /// Whether its `Wait` has not run yet at game time `now`, in a script
    /// after the frame's orders: it runs in the first frame at least
    /// `duration` after `when`, and not in the frame it was pushed.
    #[must_use]
    pub fn waiting(&self, now: u32) -> bool {
        self.duration
            .is_some_and(|d| now == self.when || runtime::elapsed(now, self.when) < d)
    }
}

impl World {
    /// Whether an effector named `name` (folded) hangs on agent `id`
    /// ([`Effector::hangs`]).
    fn attached(&self, id: u32, name: &str) -> bool {
        self.effectors
            .list
            .iter()
            .any(|e| e.agent == id && e.name == name && e.hangs(self.now))
    }

    /// Drop agent `id`'s effectors, all or those named `name` (folded):
    /// Relation unit delete runs each one's end script first, where
    /// it has not run. All of them takes the reward
    /// flags hung on it too, in relation list 1's order, the order each
    /// was hung (**Inferred**: by handle, as each takes the next as it
    /// hangs). **Game**: the tournament probe's ratman champion 37,
    /// petrified at 5786 and flagged at 5992, dies at 6158: its
    /// `DeleteAllEffectors` runs `Petrify_End`, then
    /// `Attack_flag_death_callback`.
    pub(super) fn detach(&mut self, id: u32, name: Option<&str>) {
        let gone = |e: &Effector| e.agent == id && name.is_none_or(|n| e.name == n);
        let mut ending: Vec<(u32, Taken)> = (0..self.effectors.list.len())
            .filter(|i| gone(&self.effectors.list[*i]))
            .map(|i| (self.effectors.list[i].handle, Taken::Effector(i)))
            .collect();

        if name.is_none() {
            ending.extend(self.flags_on(id).map(|(h, f)| (h, Taken::Flag(f))));
            self.detach_clones(id);
        }

        ending.sort_unstable_by_key(|(handle, _)| *handle);

        for (_, hung) in ending {
            match hung {
                Taken::Effector(i) => self.end_script(i),
                Taken::Flag(flag) => self.unhang(flag),
            }
        }

        self.effectors.list.retain(|e| !gone(e));
    }

    /// The reward flags hung on agent `id`'s unit, relation 1: handle
    /// and agent each.
    fn flags_on(&self, id: u32) -> impl Iterator<Item = (u32, u32)> + use<> {
        let holder = self.pieces.handle(id);
        let hung: Vec<(u32, u32)> = self
            .pieces
            .iter()
            .filter(|(_, p)| {
                p.place
                    .as_ref()
                    .is_some_and(|pl| Some(pl.holder) == holder && pl.is_reward())
            })
            .map(|(flag, p)| (p.handle, *flag))
            .collect();

        hung.into_iter()
    }

    /// Reward flag `flag` deleted with its holder's effectors:
    /// deferred, then
    /// `GenUnit` runs its descriptor's script with the flag's own
    /// agent, its kind being 4, nested in the builtin. **Game**, the
    /// probed play: troll 48's death script's `DeleteAllEffectors` runs
    /// `Attack_flag_death_callback(Flag_Attack#49)` (`ARA2`'s
    /// `GPLFunction`).
    fn unhang(&mut self, flag: u32) {
        self.dispatches.delete(flag);

        let function = self
            .body_of(flag)
            .and_then(|b| b.script.as_ref())
            .map(|s| s.function.clone())
            .filter(|f| !f.is_empty());

        if let (Some(f), true) = (function, self.effectors.unhung.insert(flag)) {
            self.dispatch(&f, vec![Value::Agent(flag)]);
        }
    }

    /// Effector `handle`'s `Wait` in the frame's orders: once it has run,
    /// callback `0x100d`, effector end, runs the end script; for
    /// [`Ending::After`] the end set first, its callback `0x100e` the
    /// script in the first pass at its end (**Inferred**: in the
    /// effector's place in the orders).
    pub(super) fn effector_wait(&mut self, handle: u32) {
        let now = self.now;
        let due = self.effectors.list.iter().position(|e| {
            let played = match e.ending {
                Ending::Wait => true,
                Ending::After => e.end.is_none_or(|end| now >= end),
            };

            e.handle == handle && e.duration.is_some() && !e.waiting(now) && played
        });

        if let Some(i) = due {
            self.wait_ends(i);
        }
    }

    /// Effector `i`'s `Wait` ending: its end set's
    /// systems, for a plain one, then its end script.
    pub(super) fn wait_ends(&mut self, i: usize) {
        let e = &self.effectors.list[i];

        if !e.ended && e.ending == Ending::Wait {
            let (handle, agent, name, wait) = (e.handle, e.agent, e.name.clone(), e.duration);

            self.effector_end_vents(handle, agent, &name, wait);
        }

        self.end_script(i);
    }

    /// `GenUnit` on effector `i`: once, its descriptor's script with
    /// its holder's agent, where the holder's agent lives.
    pub(super) fn end_script(&mut self, i: usize) {
        let e = &mut self.effectors.list[i];

        if !e.ended {
            e.ended = true;

            let (agent, name) = (e.agent, e.name.clone());
            let function = self
                .descriptors
                .body_named(&name)
                .and_then(|(_, b)| b.script.as_ref())
                .map(|s| s.function.clone())
                .filter(|f| !f.is_empty());

            if let (Some(f), true) = (function, self.agents.alive(agent)) {
                self.dispatch(&f, vec![Value::Agent(agent)]);
            }
        }
    }

    /// The effectors hanging on units now ([`Effector::hangs`]):
    /// what a save writes as overlay records held by their units.
    pub fn hanging(&self) -> impl Iterator<Item = &Effector> {
        self.effectors.list.iter().filter(|e| e.hangs(self.now))
    }

    /// The codes of the effectors hanging on agent `agent` now, in the
    /// order made: its relation list 1, as a hero's
    /// enchantments list reads it.
    pub fn hung_on(&self, agent: u32) -> impl Iterator<Item = FourCc> + '_ {
        self.hanging()
            .filter(move |e| e.agent == agent)
            .filter_map(|e| Some(self.effectors.hung(e.handle)?.code))
    }

    /// `CheckEffector(agent, name)`, gpl check effector. Unknown where the unit
    /// took an effector the world did not follow.
    pub(super) fn check_effector(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let folded = name.to_ascii_lowercase();

        if !self.descriptors.described(&folded) {
            Err(Fault::throw(format!(
                "CheckEffector( {name} ): couldn't find effector named {name}"
            )))?;
        }

        let on = match self.found(id) {
            Found::Piece(_) => Some(self.attached(id, &folded)),
            Found::Gone => Some(false),
            Found::Unsure => None,
        };

        match on {
            Some(b) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, b).map(|()| Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `SetEffectorDirection(agent, name, n)`, gpl set effector direction: throws for a
    /// name no descriptor has; no unit, nothing; else the first effector
    /// of the name on it turns its layer 0 to direction
    /// `n` (`UnitIMG` method `(0, n)`), read by value. Its slot follows
    /// ([`super::bodies::Facing`]). Returns nothing.
    pub(super) fn effector_direction(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let n = crate::gpl::ops::int_val(vm.object(vm.arg(args, 3)?))?;
        let folded = name.to_ascii_lowercase();

        if !self.descriptors.described(&folded) {
            Err(Fault::throw(format!(
                "SetEffectorDirection( {name} ): couldn't find effector named {name}"
            )))?;
        }

        match self.found(id) {
            Found::Piece(_) => {
                self.turn_effector(id, &folded, n);
                Ok(Answer::Given)
            }
            Found::Gone => Ok(Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// The first effector named `name` (folded) on agent `id`, in the
    /// order hung (**Inferred**: relation 1 appends), turned to `n`.
    fn turn_effector(&mut self, id: u32, name: &str, n: i32) {
        let now = self.now;
        let first = self
            .effectors
            .iter()
            .find(|e| e.agent == id && e.name == name && e.hangs(now))
            .map(|e| e.handle);

        if let Some(h) = first.and_then(|h| self.effectors.hung.get_mut(&h)) {
            h.facing = n;
        }
    }

    /// `DeleteEffector(agent, name)`, gpl delete effector: nothing unless both
    /// the unit and the descriptor are found.
    pub(super) fn delete_effector(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let folded = string_ref(vm, args, 2)?.to_ascii_lowercase();

        match self.found(id) {
            Found::Piece(_) => {
                self.detach(id, Some(&folded));
                Ok(Answer::Given)
            }
            Found::Gone => Ok(Answer::Given),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `DeleteAllEffectors(agent)`, gpl delete all effectors.
    pub(super) fn delete_all_effectors(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                self.detach(id, None);
                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw(format!(
                "DeleteAllEffectors() was given invalid agent#{}",
                id.cast_signed()
            ))),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// `CreateEffector(agent, name, duration, [value], [option])`,
    /// gpl create effector: nothing without a unit; throws when no descriptor has
    /// the name (desc find name, folding case); else
    /// `GenUnit`, or method with a value, which calls
    /// it. `after` sets order bit 8 on the effector's `Wait`, so its end
    /// script runs as its end set ends, not as the `Wait` does
    /// ([`Ending::After`]).
    ///
    /// method re-arms an effector of the name already on the unit whose
    /// `Wait` is still on track 1 (attached find, `method(1, 9)`,
    /// effector rearm: `when` now, the new duration), unless a particle
    /// system. Else a new unit of kind 1 by `GE_Level`, whose handle
    /// is the level's counter that every placed
    /// piece takes. **Game**: 534 goes between the inn day's two spawns,
    /// to the thought bubble.
    pub(super) fn effector(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let duration = int_ref(vm, args, 3)?;
        let option = |word: &str| {
            (4..args.len()).any(|k| {
                matches!(vm.value(args[k]), Some(Value::String(s)) if s.eq_ignore_ascii_case(word))
            })
        };
        let (after, infinite) = (option(AFTER), option(INFINITE));

        // The last integer option is the value.
        let value = (4..args.len()).rev().find_map(|k| match vm.value(args[k]) {
            Some(Value::Integer(n)) => Some(*n),
            _ => None,
        });
        let unit = match self.found(id) {
            Found::Piece(_) => Some(true),
            Found::Unsure => None,
            Found::Gone => Some(false),
        };
        let folded = name.to_ascii_lowercase();
        let described = self.descriptors.described(&folded);

        match unit {
            Some(true) if !described => Err(Fault::throw(format!(
                "CreateEffector() could not make unit of type {name} (could not find description) "
            )))?,
            Some(true) => {
                let ending = if after { Ending::After } else { Ending::Wait };

                self.hang(id, folded, (duration, value), Span::of(infinite), ending);
                Ok(Answer::Given)
            }
            Some(false) => Ok(Answer::Given),
            None => Ok(Answer::Unknown),
        }
    }

    /// Which effector `GenUnit` re-arms on agent `id`, by index:
    /// Attached find takes the first of the name still attached,
    /// oldest first, as relation link appends; re-armed only if
    /// its own `Wait` is pending (`method(1, 9)`), else a new one is made,
    /// though a later one waits. **Game**, the probed play: at 254,100 ms
    /// hero 16's second `thought_bubble_talk` of the frame meets the
    /// 251,600 ms one in its end set and takes a handle.
    fn rearmed(&self, id: u32, name: &str) -> Option<usize> {
        let now = self.now;

        self.effectors
            .list
            .iter()
            .position(|e| e.agent == id && e.name == name && e.hangs(now))
            .filter(|i| {
                let e = &self.effectors.list[*i];

                e.waiting(now) && e.duration != Some(0)
            })
    }

    /// `GenUnit`'s effector on agent `id`: re-armed, or made. A
    /// `Wait` is pushed unless `infinite` or [`NO_WAIT`].
    /// One whose `Wait` is 0 ms is not re-armed, even in its own frame:
    /// that `Wait` ends as it is pushed (`method(1, 9)` then finds none).
    /// **Game**: the dwarves game's two `wounded_large` on agent 56 at
    /// 293,500 ms take two handles.
    fn hang(
        &mut self,
        id: u32,
        name: String,
        (duration, value): (i32, Option<i32>),
        span: Span,
        ending: Ending,
    ) {
        let now = self.now;
        let particle = self.descriptors.named().particles.contains(&name);
        let wait = (span == Span::Timed && duration != NO_WAIT).then_some(duration.cast_unsigned());

        // A particle system's is a plume, never re-armed.
        if particle {
            self.hang_plume(id, &name, wait);
        } else {
            let end = wait.and_then(|ms| self.effector_end(&name, now, ms));

            // A re-arm reads no flags: `infinite`
            // still ends; -1 never does.
            let rearm = (duration != NO_WAIT)
                .then(|| self.effector_end(&name, now, duration.cast_unsigned()))
                .flatten();
            let running = self
                .rearmed(id, &name)
                .and_then(|i| self.effectors.list.get_mut(i));

            if let Some(e) = running {
                e.when = now;
                e.duration = Some(duration.cast_unsigned());
                e.end = rearm;
                e.ending = ending;
                e.value = value.or(e.value);
            } else {
                self.handle += 1;

                // Its systems take the handles after its own.
                let handle = self.handle;

                // `GE_OrderProcessor` joins `ORDR` with the `Wait`.
                if wait.is_some() {
                    self.dispatches.join(super::Listed::Effector(handle));
                }

                // Where it hangs, kept for drawing.
                if let Some(spot) = self.hang_spot(id, &name) {
                    self.effectors.hung.insert(handle, spot);
                }

                self.effector_vents(handle, id, &name, wait);

                // Made, it plays its own `Effector Begin`.
                self.cue(
                    super::sound::From::Agent(id),
                    super::sound::Voice::Unit(name.clone()),
                    super::sound::Phase::Id(super::sound::EFFECTOR_BEGIN),
                    0.0,
                );

                self.effectors.list.push(Effector {
                    agent: id,
                    name,
                    handle,
                    when: now,
                    duration: wait,
                    end,
                    ending,
                    ended: false,
                    value,
                });

                // A `Wait` of 0 ms ends as it is pushed: its end script runs
                // inside the builtin, and no `Wait` is left to re-arm.
                // **Game**, play5: `damage`'s `Damage_Shield_Reflect_Effector`
                // on its attacker runs `Damage_Shield_Hit` nested in
                // `CreateEffector`. **Binary**: rate 0 sets header bit 2
                // So `GenUnit` runs it on push, and
                // `OrderWait` ends in its first run.
                if wait == Some(0) && ending == Ending::Wait {
                    self.wait_ends(self.effectors.list.len() - 1);
                }
            }
        }
    }

    /// Init action peak's overlay: `GenUnit` method `(code, 0, 0, …)`
    /// on actor `id`, a 0 ms effector.
    pub(super) fn hang_overlay(&mut self, id: u32, code: FourCc) {
        let name = self
            .descriptors
            .body(code)
            .map(|b| b.name.to_ascii_lowercase());

        if let (Some(name), true) = (name, self.pieces.contains(id)) {
            self.hang(id, name, (0, None), Span::Timed, Ending::Wait);
        }
    }

    /// `GenUnit` method `(name, ms, 0, 0, 2, 0)` on agent `id`: an effector
    /// of `ms`, as the Hall of Champions' bounty hangs `Champion_Icon`.
    pub(super) fn hang_for(&mut self, id: u32, name: &str, ms: i32) {
        let folded = name.to_ascii_lowercase();

        if self.pieces.contains(id) && self.descriptors.described(&folded) {
            self.hang(id, folded, (ms, None), Span::Timed, Ending::Wait);
        }
    }

    /// When an effector named `name` (folded) whose `Wait` of `ms` starts
    /// at `now` is deleted: effector end plays set `0x60` from frame
    /// 0, else its one set on, then callback `0x1005` deletes it, as a
    /// spell unit's timeout does; `0x100e` for `after` ([`Ending::After`]).
    fn effector_end(&self, name: &str, now: u32, ms: u32) -> Option<u32> {
        let (_, body) = self.descriptors.body_named(name)?;

        super::spell::end_at(body.poses.as_ref()?, now, ms, super::spell::First::Wait)
    }

    /// Each unit an effector left since the last step: its flags placed
    /// again ([`World::restack`]).
    pub(super) fn overlays_left(&mut self) {
        let now = self.now;
        let hanging: BTreeSet<u32> = self
            .effectors
            .iter()
            .filter(|e| e.hangs(now))
            .map(|e| e.handle)
            .collect();
        let gone: Vec<(u32, u32)> = self
            .effectors
            .iter()
            .filter(|e| self.effectors.hanging.contains(&e.handle) && !hanging.contains(&e.handle))
            .map(|e| (e.agent, e.handle))
            .collect();
        let mut left: Vec<u32> = gone.iter().map(|(agent, _)| *agent).collect();

        left.dedup();
        self.effectors.hanging = hanging;

        for target in left {
            let leaving: Vec<u32> = gone
                .iter()
                .filter(|(agent, _)| *agent == target)
                .map(|(_, handle)| *handle)
                .collect();

            self.restack(target);
            self.restack_hung(target, &leaving);
        }

        self.effectors.drop_spent(now);
    }
}

/// Effector animate's first choice, `0x50`.
pub const INTRO_SET: u32 = crate::data::imag::SetKind::Build.id();

/// What callback `0x100c` plays as the intro ends, and
/// effector animate's second choice.
pub const LOOP_SET: u32 = crate::data::imag::SetKind::Special.id();

/// `GenUnit`'s nudge toward the viewer, 16.16 cells: `y + 1`,
/// so the overlay sorts after its holder.
pub const NEARER: Fixed = Fixed::from_bits(1);

/// Where an effector hangs and what it plays, as `GenUnit` made it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hung {
    pub code: FourCc,
    /// Game ms it was made: its first set plays from here. A re-arm
    /// changes only the `Wait`.
    pub made: u32,
    /// The holder's attachment point less its place, y + [`NEARER`],
    /// 16.16 cells: x, y, height.
    pub base: [Fixed; 3],
    /// [`Hung::base`] stacked among the holder's overlays.
    pub spot: [Fixed; 3],
    /// Its holder's direction as made.
    pub facing: i32,
}

/// One effect, as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    /// Its unit handle.
    pub handle: u32,
    pub code: FourCc,
    /// The agent it hangs on; 0 for none.
    pub holder: u32,
    /// 16.16 cells, x, y and height: about its holder while held, else
    /// where it stands.
    pub at: [Fixed; 3],
    /// The direction word its slot is picked by.
    pub facing: i32,
    pub look: Look,
    /// [`Effector::value`]: drawn as text by frames of reference
    /// kind `0x08000000`.
    pub value: Option<i32>,
    /// Its holder is held, so it is too: not drawn. **Corpus**: every
    /// overlay record's state bit 1 is its holder's, 196 of 196.
    pub held: bool,
}

impl World {
    /// Every effect drawn at game time `now`: effectors hanging, in the
    /// order made, then spent missiles in impact order, then message
    /// flags ([`World::messages`]). A particle
    /// system's effector is a [`super::plume::Plume`], not here.
    #[must_use]
    pub fn effects(&self) -> Vec<Effect> {
        let hung = self.effectors.list.iter().filter_map(|e| {
            let h = self.effectors.hung.get(&e.handle)?;
            let look = self.effector_look(e, h)?;
            let poses = self.descriptors.body(h.code)?.poses.as_ref()?;
            let facing = if reset(poses, h, e.duration, waited(e), self.now) {
                0
            } else {
                h.facing
            };

            e.hangs(self.now).then_some(Effect {
                handle: e.handle,
                code: h.code,
                holder: e.agent,
                at: h.spot,
                facing,
                look,
                value: e.value,
                held: self.is_held(e.agent),
            })
        });

        // Every HD impact set has one slot (**Corpus**), so the facing
        // draws nothing; a save's record writes it.
        let spent = self.spent().iter().map(|s| Effect {
            handle: s.handle,
            code: s.code,
            holder: 0,
            at: s.at,
            facing: s.facing,
            look: Look::Plays {
                set: IMPACT_SET,
                since: s.since,
            },
            value: None,
            held: false,
        });

        hung.chain(spent).chain(self.message_effects()).collect()
    }

    /// Whether agent `id`'s piece is held: state bit [`crate::sim::world::piece_state::HELD`].
    pub(super) fn is_held(&self, id: u32) -> bool {
        self.known(id)
            .is_some_and(|p| p.state & piece_state::HELD != 0)
    }

    /// The set spell unit `id` a cast made shows now:
    /// Spell unit animate's set 0x50, then 0x40
    /// (callback `0x2003`, spell unit intro done, an effector's `0x100c`), else
    /// 0x40, from its making; set 0x60 from its `Wait`'s end, the set's
    /// length before its deletion ([`World::expiry`]).
    pub(super) fn spell_look(&self, id: u32) -> Option<Look> {
        let made = self.units.get(id)?.spelled?;
        let body = self.body_of(id)?;
        let (poses, first) = (body.poses.as_ref()?, body.first?);
        let facing = self.facing_word(id);
        let fade = poses
            .get(&IMPACT_SET)
            .zip(self.expiry(id))
            .map(|(p, end)| end.saturating_sub(p.frames_at(facing) * frame_ms(p.playback)));
        let h = Hung {
            code: self.pieces.get(id)?.code,
            made,
            base: [Fixed::ZERO; 3],
            spot: [Fixed::ZERO; 3],
            facing,
        };

        match fade {
            Some(since) if self.now >= since => Some(Look::Plays {
                set: IMPACT_SET,
                since,
            }),
            _ => Some(opening(poses, first, &h, None, self.now)),
        }
    }

    /// The set effector `e` shows now: its intro, loop or first set from
    /// its making, then set `0x60` from its `Wait`'s end.
    fn effector_look(&self, e: &Effector, h: &Hung) -> Option<Look> {
        let poses = self.descriptors.body(h.code)?.poses.as_ref()?;
        let first = self.descriptors.body(h.code)?.first?;
        let ended = waited(e).filter(|t| self.now >= *t);

        match ended {
            Some(since) if poses.contains_key(&IMPACT_SET) => Some(Look::Plays {
                set: IMPACT_SET,
                since,
            }),
            _ => Some(opening(poses, first, h, e.duration, self.now)),
        }
    }

    /// `GenUnit` and `GSUnit` for an effector named `name`
    /// (folded) about to hang on agent `id`, before it joins the
    /// holder's overlays. `None` without either's descriptor.
    pub(super) fn hang_spot(&self, id: u32, name: &str) -> Option<Hung> {
        let (code, own) = self.descriptors.body_named(name)?;
        let facing = self.facing_word(id);
        let holder = self.body_of(id)?;
        let [x, y, z] = holder.hit.offset3_at(own.attach, facing);
        let base = [x, y + NEARER, z];
        let handle = self.pieces.get(id)?.handle;
        let (dx, dy) = stack_offset(own.stack, &self.stacked(handle, id));

        Some(Hung {
            code,
            made: self.now,
            base,
            spot: [x + dx, y + NEARER + dy, z],
            facing,
        })
    }
}

/// When effector `e`'s `Wait` ends, game ms: in the first frame at least
/// its duration on and not its own; a 0 ms one as pushed. `None`
/// without a `Wait`, or one ending past the 32-bit clock: a re-armed -1
/// (effector rearm keeps it as is) never ends.
fn waited(e: &Effector) -> Option<u32> {
    e.duration.and_then(|d| match d {
        0 => Some(e.when),
        d => {
            let frames = u64::from(d.div_ceil(MS_PER_FRAME).max(1));

            u32::try_from(u64::from(e.when) + frames * u64::from(MS_PER_FRAME)).ok()
        }
    })
}

/// What an effector plays from its making, at game time `now`:
/// Effector animate's intro then loop, or its loop, else the art's
/// first set; a 0 ms `Wait` skips the intro and loop.
/// The intro ends after its last frame shows (**Inferred**: as an end
/// set's callback `0x1005` does, [`super::spell::end_at`]).
fn opening(poses: &Poses, first: u32, h: &Hung, wait: Option<u32>, now: u32) -> Look {
    let animates = wait != Some(0);
    let intro = intro_end(poses, h, wait);
    let looped = poses.contains_key(&LOOP_SET);

    match intro {
        Some(end) if now >= end && looped => Look::Plays {
            set: LOOP_SET,
            since: end,
        },
        Some(_) => Look::Plays {
            set: INTRO_SET,
            since: h.made,
        },
        None if animates && looped => Look::Plays {
            set: LOOP_SET,
            since: h.made,
        },
        None => Look::Plays {
            set: first,
            since: h.made,
        },
    }
}

/// When effector `h`'s intro ends, game ms: `None` without one, or for
/// a 0 ms `Wait`, which skips it.
fn intro_end(poses: &Poses, h: &Hung, wait: Option<u32>) -> Option<u32> {
    poses
        .get(&INTRO_SET)
        .filter(|_| wait != Some(0))
        .map(|p| h.made + p.frames_at(h.facing) * frame_ms(p.playback))
}

/// Whether effector `h`'s intro has ended into its loop by `now`, its
/// `Wait` of `wait` ms ending at `ended`: effector intro done
/// sets 0x40 by `UnitIMG` with its reset argument, so its slot
/// is 0 from then on. Not where the `Wait` ended first: its end set
/// took the intro's place (**Inferred**: the callback goes with it).
fn reset(poses: &Poses, h: &Hung, wait: Option<u32>, ended: Option<u32>, now: u32) -> bool {
    let looped = poses.contains_key(&LOOP_SET);

    intro_end(poses, h, wait)
        .is_some_and(|end| looped && now >= end && ended.is_none_or(|t| t >= end))
}

/// The effectors made, and what the world keeps of each beside it.
///
/// ```text
///  Effectors ── list     each Effector, in the order made
///      ├─ hung          where each hangs and what it plays, by handle
///      ├─ hanging       those on their unit at the last step
///      └─ unhung        reward flags whose script ran as their holder's
///                       effectors went: once each
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Effectors {
    list: Vec<Effector>,
    hung: BTreeMap<u32, Hung>,
    hanging: BTreeSet<u32>,
    unhung: BTreeSet<u32>,
}

impl Effectors {
    /// Every effector made and not deleted, in the order made.
    pub(super) fn iter(&self) -> std::slice::Iter<'_, Effector> {
        self.list.iter()
    }

    /// Where the effector of `handle` hangs.
    pub(super) fn hung(&self, handle: u32) -> Option<&Hung> {
        self.hung.get(&handle)
    }

    /// Where the effector of `handle` hangs, to restack.
    pub(super) fn hung_mut(&mut self, handle: u32) -> Option<&mut Hung> {
        self.hung.get_mut(&handle)
    }

    /// Drop those deleted by game time `now`: end script run and end set
    /// played (callback `0x1005` deletes the unit), or detached. Neither
    /// hangs nor runs again, so nothing reads them; kept, a long game's
    /// state grew with every one made (a quest save: 41,806 by day 10).
    fn drop_spent(&mut self, now: u32) {
        self.list.retain(|e| !e.ended || e.hangs(now));

        let kept: BTreeSet<u32> = self.list.iter().map(|e| e.handle).collect();

        self.hung.retain(|handle, _| kept.contains(handle));
    }
}

impl World {
    /// The effectors made and not deleted, in the order made.
    #[must_use]
    pub fn effectors(&self) -> &[Effector] {
        &self.effectors.list
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Effectors {
        type Model = m::Effectors;

        fn save(&self) -> m::Effectors {
            m::Effectors {
                list: self.list.save(),
                hung: self.hung.save(),
                hanging: self.hanging.clone(),
                unhung: self.unhung.clone(),
            }
        }

        fn load(model: m::Effectors) -> Result<Effectors, Refused> {
            Ok(Effectors {
                list: Vec::load(model.list)?,
                hung: BTreeMap::load(model.hung)?,
                hanging: model.hanging,
                unhung: model.unhung,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::fourcc::FourCc;
    use crate::sim::movement::action::Pose;
    use crate::sim::world::bodies::Class;
    use crate::sim::world::kind;

    /// `SetEffectorDirection` turns the first effector of the name on the
    /// unit, not a later one, nor another unit's.
    #[test]
    fn the_first_effector_of_the_name_takes_the_direction() {
        let mut world = World::default();
        let lights = |agent, handle| Effector {
            agent,
            name: "spire_lights_effector".to_owned(),
            handle,
            when: 0,
            duration: None,
            end: None,
            ending: Ending::Wait,
            ended: false,
            value: None,
        };
        let hung = Hung {
            code: FourCc(*b"XR59"),
            made: 0,
            base: [Fixed::ZERO; 3],
            spot: [Fixed::ZERO; 3],
            facing: 0,
        };

        for (agent, handle) in [(7, 100), (7, 101), (8, 102)] {
            world.effectors.list.push(lights(agent, handle));
            world.effectors.hung.insert(handle, hung);
        }

        world.turn_effector(7, "spire_lights_effector", 3);

        let facings: Vec<i32> = [100, 101, 102]
            .iter()
            .map(|h| world.effectors.hung[h].facing)
            .collect();

        assert_eq!(facings, [3, 0, 0]);
    }

    fn pose(frames: u16, playback: u32) -> Pose {
        Pose {
            frames: vec![frames],
            playback,
            keeps: 0,
            cues: vec![None],
        }
    }

    fn hung(made: u32) -> Hung {
        Hung {
            code: FourCc(*b"ARB2"),
            made,
            base: [Fixed::ZERO; 3],
            spot: [Fixed::ZERO; 3],
            facing: 0,
        }
    }

    fn effector(when: u32, duration: Option<u32>) -> Effector {
        Effector {
            agent: 40,
            name: "got_gold".to_owned(),
            handle: 900,
            when,
            duration,
            end: None,
            ending: Ending::Wait,
            ended: false,
            value: None,
        }
    }

    /// A 0 ms `Wait` ends as it is pushed; a 30 ms one in the next frame;
    /// 120 ms in the third.
    #[test]
    fn an_effectors_wait_ends_in_the_first_frame_past_it() {
        assert_eq!(waited(&effector(1000, Some(0))), Some(1000));
        assert_eq!(waited(&effector(1000, Some(30))), Some(1050));
        assert_eq!(waited(&effector(1000, Some(120))), Some(1150));
        assert_eq!(waited(&effector(1000, None)), None);
    }

    /// A re-armed `Wait` of -1 (effector rearm stores it as is) is
    /// due when `now - when` reaches `0xffffffff`:
    /// past the clock, so it never ends.
    #[test]
    fn a_wait_past_the_clock_never_ends() {
        assert_eq!(waited(&effector(1000, Some(u32::MAX))), None);
        assert_eq!(waited(&effector(1000, Some(u32::MAX - 500))), None);
        assert!(effector(1000, Some(u32::MAX)).waiting(1_000_000));
    }

    /// With set 0x50, the intro plays, then 0x40 once its frames have
    /// shown; a 0 ms effector plays its first set.
    #[test]
    fn an_effector_plays_its_intro_then_its_loop() {
        let poses = Poses::from([(INTRO_SET, pose(4, 100)), (LOOP_SET, pose(8, 100))]);
        let at = |now| opening(&poses, LOOP_SET, &hung(1000), Some(5000), now);

        assert_eq!(
            at(1350),
            Look::Plays {
                set: INTRO_SET,
                since: 1000
            }
        );
        assert_eq!(
            at(1400),
            Look::Plays {
                set: LOOP_SET,
                since: 1400
            }
        );
        assert_eq!(
            opening(&poses, 0x60, &hung(1000), Some(0), 1400),
            Look::Plays {
                set: 0x60,
                since: 1000
            }
        );
    }

    /// Its loop after its intro faces slot 0, its holder's facing gone;
    /// not during the intro, nor where its `Wait` ended first. **Game**:
    /// the call to arms probe's four `teleport_effector`s save facing 0
    /// in set 0x40, on heroes facing 0, 28, 4 and 0 as each was made.
    #[test]
    fn an_effectors_loop_after_its_intro_faces_slot_zero() {
        let poses = Poses::from([(INTRO_SET, pose(4, 100)), (LOOP_SET, pose(8, 100))]);
        let h = Hung {
            facing: 28,
            ..hung(1000)
        };

        assert!(!reset(&poses, &h, Some(5000), None, 1350));
        assert!(reset(&poses, &h, Some(5000), None, 1400));
        assert!(reset(&poses, &h, Some(5000), Some(6000), 6100));
        assert!(!reset(&poses, &h, Some(300), Some(1300), 1400));
        assert!(!reset(&poses, &h, Some(0), None, 1400));
    }

    /// The game clock is unsigned and wraps:
    /// a `Wait` pushed after `now`
    /// reads a huge elapsed time, so it has run.
    #[test]
    fn a_wait_pushed_after_now_has_run() {
        let effector = Effector {
            agent: 1,
            name: String::new(),
            handle: 2,
            when: 100,
            duration: Some(50),
            end: None,
            ending: Ending::Wait,
            ended: false,
            value: None,
        };

        assert!(!effector.waiting(40));
    }

    /// A `Wait` pushed in a script runs in the first later frame at least
    /// its duration on: `got_gold`'s 0 ms lasts the frame, a thought
    /// bubble's 2000 ms forty frames; `infinite` never runs one.
    #[test]
    fn an_effector_is_re_armed_only_while_its_wait_is_pending() {
        let effector = |duration| Effector {
            agent: 6,
            name: "got_gold".to_owned(),
            handle: 534,
            when: 1000,
            duration,
            end: None,
            ending: Ending::Wait,
            ended: false,
            value: None,
        };

        assert!(effector(Some(0)).waiting(1000));
        assert!(!effector(Some(0)).waiting(1050));
        assert!(effector(Some(2000)).waiting(2950));
        assert!(!effector(Some(2000)).waiting(3000));
        assert!(!effector(None).waiting(1000));
    }

    /// `GenUnit` asks only the first effector of the name still
    /// attached: one in its end set, `Wait` gone, has a new one made,
    /// though a later one waits. **Game**: the probed play's hero 16 at
    /// 254,100 ms, its 251,600 ms `thought_bubble_talk` ending at 254,200.
    #[test]
    fn only_the_first_attached_effector_of_a_name_is_re_armed() {
        let effector = |handle, when, end| Effector {
            agent: 16,
            name: "thought_bubble_talk".to_owned(),
            handle,
            when,
            duration: Some(2500),
            end: Some(end),
            ending: Ending::Wait,
            ended: false,
            value: None,
        };
        let world = |now| World {
            now,
            effectors: Effectors {
                list: vec![
                    effector(1020, 251_600, 254_200),
                    effector(1040, 254_100, 256_700),
                ],
                ..Effectors::default()
            },
            ..World::default()
        };

        assert_eq!(world(254_100).rearmed(16, "thought_bubble_talk"), None);
        assert_eq!(world(254_200).rearmed(16, "thought_bubble_talk"), Some(1));
        assert_eq!(world(254_100).rearmed(17, "thought_bubble_talk"), None);
        assert_eq!(world(254_000).rearmed(16, "thought_bubble_talk"), Some(0));
    }

    /// `CreateEffector(…, "after")`: its end script waits for its end set,
    /// callback `0x100e`; a plain one's runs as its `Wait` ends. **Game**:
    /// the probed play's `Lightning_Bolt_End` seven frames after its
    /// `Lightning_Bolt_Hit`, the effector's one set of seven.
    #[test]
    fn an_after_effectors_end_script_waits_for_its_end_set() {
        let ended = |ending, now| {
            let mut world = World {
                now,
                ..World::default()
            };

            world.effectors.list.push(Effector {
                agent: 46,
                name: "lightening_bolt_effector".to_owned(),
                handle: 700,
                when: 1000,
                duration: Some(0),
                end: Some(1400),
                ending,
                ended: false,
                value: None,
            });
            world.effector_wait(700);
            world.effectors.list[0].ended
        };

        assert!(ended(Ending::Wait, 1050));
        assert!(!ended(Ending::After, 1050));
        assert!(!ended(Ending::After, 1350));
        assert!(ended(Ending::After, 1400));
    }

    /// An effector keeps its own handle when its first set names a
    /// particle system: `GE_Level` numbers the unit, then
    /// `UnitIMG` makes the system, the next handle. Its 0 ms end set
    /// then plays out on its holder. `energy_blast_effector`'s one set,
    /// `0x60`, ten frames of 50 ms, names `XL08`. It keeps its value.
    #[test]
    fn an_effector_venting_as_made_keeps_its_handle() {
        use crate::sim::movement::action::Pose;
        use crate::sim::world::particles::tests::fountain;
        use crate::sim::world::plume::{IMPACT_SET, Lifespan, Particles, Vent};
        use crate::sim::world::ruin::tests::{body, standing};

        let (blast, spark) = (FourCc(*b"WRa1"), FourCc(*b"XL08"));
        let mut world = World {
            now: 10_000,
            handle: 200,
            ..World::default()
        };
        let mut effector = body(Class::Character, 0, vec![]);
        let mut system = body(Class::Character, 0, vec![]);
        let pose = Pose {
            frames: vec![10],
            playback: 50,
            keeps: 0,
            cues: vec![None],
        };

        effector.name = "energy_blast_effector".to_owned();
        effector.poses = Some(std::iter::once((IMPACT_SET, pose)).collect());
        effector.first = Some(IMPACT_SET);
        effector.vents = std::iter::once((
            IMPACT_SET,
            vec![Vent {
                code: spark,
                offset: [Fixed::ZERO; 2],
            }],
        ))
        .collect();
        system.particles = Some(Particles {
            lifespan: Lifespan {
                emitter: Some(500),
                particle: 500,
            },
            state: 0,
            attach: 0,
            spec: fountain(),
            sprite: super::super::particles::Sprite::default(),
        });
        world.descriptors_mut().add_body(blast, effector);
        world.descriptors_mut().add_body(spark, system);
        world
            .descriptors_mut()
            .add_body(FourCc(*b"BVr1"), body(Class::Character, 0x20, vec![(0, 0)]));
        world
            .pieces
            .insert(12, standing(*b"BVr1", 100, kind::Kind::VEHICLE, (20, 20)));
        world.hang(
            12,
            "energy_blast_effector".to_owned(),
            (0, Some(45)),
            Span::Timed,
            Ending::Wait,
        );

        let shown = |world: &World| world.effects().iter().map(|e| e.handle).collect::<Vec<_>>();

        assert_eq!(
            world.plumes().iter().map(|p| p.handle).collect::<Vec<_>>(),
            [202]
        );
        assert_eq!(shown(&world), [201]);
        assert_eq!(world.effects()[0].value, Some(45));

        world.now = 10_450;
        assert_eq!(shown(&world), [201]);

        world.now = 10_500;
        assert_eq!(shown(&world), Vec::<u32>::new());
    }

    /// An effector hangs on through its end animation: **Game**, the
    /// defeat probe's `CheckEffector` finds champion 12's `Damage_Shield`
    /// 1,300 ms after its `Damage_Shield_End`.
    #[test]
    fn an_ended_effector_hangs_on_through_its_end_animation() {
        let shield = Effector {
            agent: 12,
            name: "damage_shield".to_owned(),
            handle: 900,
            when: 221_150,
            duration: Some(40_000),
            end: Some(262_900),
            ending: Ending::Wait,
            ended: false,
            value: None,
        };

        assert!(shield.hangs(261_100));
        assert!(!shield.waiting(261_200));
        assert!(shield.hangs(262_450));
        assert!(!shield.hangs(262_900));
        assert!(
            Effector {
                duration: None,
                end: None,
                ..shield.clone()
            }
            .hangs(400_000)
        );
        assert!(
            !Effector {
                end: None,
                ..shield
            }
            .hangs(261_200)
        );
    }

    /// Effector rearm reads no flags: an `infinite` re-arm still
    /// gives the `Wait` its duration, so the end set plays after it.
    #[test]
    fn an_infinite_re_arm_still_ends_after_its_wait() {
        use crate::sim::movement::action::Pose;
        use crate::sim::world::ruin::tests::body;

        let mut world = World {
            now: 2000,
            ..World::default()
        };
        let mut fire = body(Class::Character, 0, vec![]);
        let pose = Pose {
            frames: vec![4],
            playback: 100,
            keeps: 0,
            cues: vec![None],
        };

        fire.name = "fire_effector".to_owned();
        fire.poses = Some(std::iter::once((IMPACT_SET, pose)).collect());
        world.descriptors_mut().add_body(FourCc(*b"WRa1"), fire);
        world.effectors.list.push(Effector {
            agent: 12,
            name: "fire_effector".to_owned(),
            handle: 300,
            when: 1000,
            duration: Some(5000),
            end: Some(6400),
            ending: Ending::Wait,
            ended: false,
            value: None,
        });
        world.hang(
            12,
            "fire_effector".to_owned(),
            (3000, None),
            Span::Infinite,
            Ending::Wait,
        );

        // Its end set, 0x60, plays its four frames after the `Wait`.
        let e = &world.effectors.list[0];

        assert_eq!((e.when, e.duration, e.end), (2000, Some(3000), Some(5400)));
    }
}
