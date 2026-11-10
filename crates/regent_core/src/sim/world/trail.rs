//! Speed trails: `TurnOnSpeedTrail`'s clones, each a unit showing where
//! the one before it was a run ago. **Binary** unless marked.
//!
//! ```text
//!  TurnOnSpeedTrail(agent, n, [rate])  no unit: throw; else
//!    NumClones APU:13 = n on the unit (GSUnit), then n times:
//!      GE_Level: a unit of kind 1 (a VehicleRec), the unit's code
//!        and owner, at its place; flags 0xe0: DontBlock, not
//!        live (so no agent, unit live), its own spot; the next handle
//!      relation_add(clone, leader, 1): its holder the leader, the
//!        unit for clone 0, else the clone before
//!      placed at the unit; mode 5 at its level (the clone level);
//!        hidden (GenUnit (0))
//!      OrderFollowClone (GS_OrderProcessor) on track 1, rate the
//!        third argument, run at once (header bit 2); the clone joins ORDR
//!      CloneParentID APU:8 the unit's handle; WhichClone APU:12 its index
//!
//!  OrderFollowClone, each due run:
//!    the leader by handle; its code, if changed (a type change)
//!    the level again; the clone to the leader's place, set,
//!      slot and frame as last recorded; the leader's now recorded
//!    shown where it no longer stands on its leader (pos3 differ)
//!
//!  the clone level: L0 the unit's mode 5 level, else 0;
//!    L0 + (10 - L0) * i / n, clamped 0..10; the unit's colour kept
//!
//!  TurnOffSpeedTrail(agent) the first unit attached
//!    (relation 1) of the unit's own code, deleted (deferred); its
//!    relation 1 members go with it, so the whole trail
//!  HasSpeedTrail(agent) whether one is attached
//! ```
//!
//! No world draw: `GE_Level` calls no `GSUnit`. **Corpus**:
//! warrior saves hold the clones, state `0xe0`, `0xe2` hidden, held by
//! their leaders, after the speed effectors' handles
//! (**Measured**).

use super::{Answer, Found, World, agent_ref, draw_view::Shown, int_ref, put_bool};
use crate::base::fixed::Fixed;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;
use crate::gpl::vm::{Fault, Handle, Vm};

/// `NumClones`, `APU:13`, written on the unit (`0xd555041`,
/// speed trail clones).
pub const NUM_CLONES: Key = Key::new(*b"APU", 13);

/// `CloneParentID`, `APU:8`: the unit's handle on each clone
/// (`0x8555041`, speed trail clones).
pub const CLONE_PARENT: Key = Key::new(*b"APU", 8);

/// The level a trail fades to: a clone's is `L0 + (STEPS - L0) * i / n`
/// (10, clone level).
pub const STEPS: i32 = 10;

/// What a clone follows: unit holder, its holder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leader {
    /// The unit itself, by agent: clone 0's.
    Unit(u32),
    /// The clone before, by handle.
    Clone(u32),
}

/// Where a unit stood and how it looked, as an order records it:
/// `_oldPos`, `_oldSet`, `_oldDir`, `_oldFrame`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    /// unit pos, 16.16 cells.
    pub fixed: (Fixed, Fixed),
    /// Its descriptor, set, slot and frame as the world knows them.
    pub shown: Shown,
    /// The game ms the unit itself looked so: a clone passes on its
    /// leader's.
    pub stamp: u32,
}

/// A speed trail's clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeedClone {
    pub handle: u32,
    /// The unit's agent; `CLONE_PARENT` names it by handle.
    pub unit: u32,
    pub leader: Leader,
    /// `WhichClone`, `APU:12`.
    pub index: u32,
    /// Its code,: its leader's as its order last saw it.
    pub code: FourCc,
    /// Its order's rate, ms; when it last ran, and the wait before the
    /// next (0 as pushed).
    pub rate: u32,
    when: u32,
    delta: u32,
    /// Where and how it stands now.
    pub shows: Seen,
    /// Its leader as its order last recorded it.
    seen: Seen,
    pub shown: Showing,
    /// Its mode 5 level, 0 to 10, and the unit's colour.
    pub level: u8,
    pub colour: Option<u8>,
    /// Deleted, deferred: gone at the next drain.
    pub deleting: bool,
}

/// Unit state bit 1: whether a clone is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Showing {
    Shown,
    Hidden,
}

impl SpeedClone {
    /// Whether its order runs at game time `now`: `OrderStack`.
    const fn due(&self, now: u32) -> bool {
        now.wrapping_sub(self.when) >= self.delta
    }
}

/// Clone `i` of `n`'s level and colour from the unit's draw effects:
/// Clone level. `None` for `n` 0, which sets nothing.
///
/// ```
/// use regent_core::sim::world::{draw::Tint, trail::clone_level};
///
/// assert_eq!(clone_level(None, 1, 4), Some((2, None)));
/// let ghost = Tint { colour: Some(2), transparency: Some(6) };
/// assert_eq!(clone_level(Some(ghost), 1, 2), Some((8, Some(2))));
/// ```
#[must_use]
pub fn clone_level(tint: Option<super::draw::Tint>, i: i32, n: i32) -> Option<(u8, Option<u8>)> {
    let base = tint.and_then(|t| t.transparency).map_or(0, i32::from);
    let colour = tint.and_then(|t| t.colour);

    (n != 0).then(|| {
        let level = (base + (STEPS - base) * i / n).clamp(0, STEPS);

        (u8::try_from(level).unwrap_or_default(), colour)
    })
}

/// Speed trails' clones, in the order made.
#[derive(Clone, Debug, Default)]
pub(super) struct Clones(Vec<SpeedClone>);

impl Clones {
    /// Whether the clone of `handle` stands.
    pub(super) fn has(&self, handle: u32) -> bool {
        self.0.iter().any(|c| c.handle == handle)
    }
}

impl World {
    /// `TurnOnSpeedTrail(agent, n, [rate])`.
    pub(super) fn trail_on(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let n = int_ref(vm, args, 2)?;
        let rate = if args.len() > 3 {
            int_ref(vm, args, 3)?
        } else {
            0
        };

        match self.found(id) {
            Found::Piece(_) => {
                self.make_trail(id, n, rate.cast_unsigned());
                Ok(Answer::Given)
            }
            Found::Unsure => Ok(Answer::Unknown),
            Found::Gone => Err(Fault::throw("TurnOnSpeedTrail(): Invalid unit ")),
        }
    }

    /// `TurnOffSpeedTrail(agent)`.
    pub(super) fn trail_off(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                let first = self.first_clone(id);

                if let Some(c) = first.and_then(|i| self.effects.clones.0.get_mut(i)) {
                    c.deleting = true;
                }

                Ok(Answer::Given)
            }
            Found::Unsure => Ok(Answer::Unknown),
            Found::Gone => Err(Fault::throw("TurnOffSpeedTrail(): Invalid unit ")),
        }
    }

    /// `HasSpeedTrail(agent)`.
    pub(super) fn has_trail(&self, vm: &mut Vm<'_>, args: &[Handle]) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, self.first_clone(id).is_some()).map(|()| Answer::Given)
            }
            Found::Unsure => Ok(Answer::Unknown),
            Found::Gone => Err(Fault::throw("HasSpeedTrail(): Invalid unit ")),
        }
    }

    /// `attached_find(code, 1)` for agent `id`'s own code: the first
    /// clone hung on it, by its place in [`World::clones`].
    fn first_clone(&self, id: u32) -> Option<usize> {
        let code = self.pieces.code(id);

        self.effects
            .clones
            .0
            .iter()
            .position(|c| c.leader == Leader::Unit(id) && Some(c.code) == code)
    }

    /// Speed trail clones: `n` clones of agent `id`'s unit.
    fn make_trail(&mut self, id: u32, n: i32, rate: u32) {
        self.write(id, NUM_CLONES.packed(), n);

        let unit = self.leader_seen(Leader::Unit(id));

        for i in 0..n {
            let leader = match i {
                0 => Leader::Unit(id),
                _ => Leader::Clone(self.handle),
            };

            if let (Some(unit), Some(seen)) = (unit, self.leader_seen(leader)) {
                self.make_clone(id, leader, (i, n), (unit, seen), rate);
            }
        }
    }

    /// Clone `i` of `n` following `leader`: made at the unit, shown there
    /// as it reveals, then hidden; its order run at once.
    fn make_clone(
        &mut self,
        id: u32,
        leader: Leader,
        (i, n): (i32, i32),
        (unit, seen): (Seen, Seen),
        rate: u32,
    ) {
        self.handle += 1;

        let (level, colour) = clone_level(self.tint(id), i, n).unwrap_or_default();
        let owner = self.pieces.owner(id).unwrap_or(0);

        // Placed shown: unit set pos reveals before `method(0)`.
        self.clone_reveal(unit.shown.code, unit.fixed, owner);

        // The order's first run, at once: the leader's place as recorded,
        // which it stands on, so hidden.
        self.effects.clones.0.push(SpeedClone {
            handle: self.handle,
            unit: id,
            leader,
            index: i.cast_unsigned(),
            code: unit.shown.code,
            rate,
            when: self.now,
            delta: rate,
            shows: seen,
            seen,
            shown: Showing::Hidden,
            level,
            colour,
            deleting: false,
        });
        self.dispatches.join(super::Listed::Clone(self.handle));
    }

    /// What `leader` shows now, as an order records it.
    fn leader_seen(&self, leader: Leader) -> Option<Seen> {
        match leader {
            Leader::Unit(id) => Some(Seen {
                fixed: self.unit_pos(id)?,
                shown: self.shown_one(id)?,
                stamp: self.now,
            }),
            Leader::Clone(h) => self
                .effects
                .clones
                .0
                .iter()
                .find(|c| c.handle == h)
                .map(|c| c.shows),
        }
    }

    /// Unit reveal hook for a clone shown at `fixed`: its code's
    /// pad and no `SightRange`, which a clone is not given. **Inferred**:
    /// a pad under two cells across reveals nothing.
    fn clone_reveal(&mut self, code: FourCc, fixed: (Fixed, Fixed), owner: u32) {
        let live = self.fog.sighting() == super::reveal::Sighting::Live;

        let mask = self.players.reveal_mask(owner);

        if let (true, Some((centre, radius))) = (live, self.pad_disc(code, fixed, 0)) {
            self.fog.explored_mut().disc(centre, mask, radius);
        }
    }

    /// Clone `handle`'s order in the frame's orders, when due.
    pub(super) fn follow(&mut self, handle: u32) {
        let now = self.now;
        let at = self
            .effects
            .clones
            .0
            .iter()
            .position(|c| c.handle == handle && c.due(now));
        let leader = at.and_then(|i| self.leader_seen(self.effects.clones.0[i].leader));

        if let (Some(i), Some(seen)) = (at, leader) {
            let c = self.effects.clones.0[i];
            let n = self
                .pieces
                .attribute(c.unit, NUM_CLONES)
                .unwrap_or_default();
            let level = clone_level(self.tint(c.unit), c.index.cast_signed(), n);
            let moved = c.seen.fixed != seen.fixed;
            let owner = self.pieces.owner(c.unit).unwrap_or(0);

            // `method(moved)`: shown off its leader.
            let shown = if moved {
                Showing::Shown
            } else {
                Showing::Hidden
            };

            if let Some((l, colour)) = level {
                let c = &mut self.effects.clones.0[i];

                c.level = l;
                c.colour = colour;
            }

            // Shown at a set, it reveals there; hidden, not.
            if c.shown == Showing::Shown {
                self.clone_reveal(seen.shown.code, c.seen.fixed, owner);
            }

            let c = &mut self.effects.clones.0[i];

            c.code = seen.shown.code;
            c.shows = c.seen;
            c.seen = seen;
            c.shown = shown;
            c.when = now;
            c.delta = c.rate;
        }
    }

    /// `GE_Level`'s drain of deleted clones: each goes with every
    /// clone behind it (relation member drop, at once).
    pub(super) fn drop_clones(&mut self) {
        let mut gone: Vec<u32> = Vec::new();

        for c in &self.effects.clones.0 {
            let behind = matches!(c.leader, Leader::Clone(h) if gone.contains(&h));

            if c.deleting || behind {
                gone.push(c.handle);
            }
        }

        self.forget_clones(&gone);
    }

    /// Agent `id`'s unit deleted: its trails with it, at once.
    pub(super) fn unit_clones_go(&mut self, id: u32) {
        for c in &mut self.effects.clones.0 {
            if c.leader == Leader::Unit(id) {
                c.deleting = true;
            }
        }

        self.drop_clones();
    }

    /// Agent `id`'s attached units deleted:
    /// its trails' first clones, deferred.
    pub(super) fn detach_clones(&mut self, id: u32) {
        for c in &mut self.effects.clones.0 {
            if c.leader == Leader::Unit(id) {
                c.deleting = true;
            }
        }
    }

    fn forget_clones(&mut self, gone: &[u32]) {
        self.effects.clones.0.retain(|c| !gone.contains(&c.handle));
        self.dispatches
            .keep(|l| !matches!(l, super::Listed::Clone(h) if gone.contains(h)));
    }

    /// Every clone standing, in the order made.
    #[must_use]
    pub fn clones(&self) -> &[SpeedClone] {
        &self.effects.clones.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trail fades from the unit's level to 10 over its clones; `n` 0
    /// sets nothing.
    #[test]
    fn a_clones_level_fades_with_its_index() {
        let levels: Vec<u8> = (0..4)
            .filter_map(|i| clone_level(None, i, 4))
            .map(|(l, _)| l)
            .collect();

        assert_eq!(levels, [0, 2, 5, 7]);
        assert_eq!(clone_level(None, 0, 0), None);
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Leader {
        type Model = m::Leader;

        fn save(&self) -> m::Leader {
            match *self {
                Leader::Unit(id) => m::Leader::Unit(id),
                Leader::Clone(h) => m::Leader::Clone(h),
            }
        }

        fn load(model: m::Leader) -> Result<Leader, Refused> {
            Ok(match model {
                m::Leader::Unit(id) => Leader::Unit(id),
                m::Leader::Clone(h) => Leader::Clone(h),
            })
        }
    }

    impl Saved for Seen {
        type Model = m::Seen;

        fn save(&self) -> m::Seen {
            m::Seen {
                fixed: self.fixed.save(),
                shown: self.shown.save(),
                stamp: self.stamp,
            }
        }

        fn load(model: m::Seen) -> Result<Seen, Refused> {
            Ok(Seen {
                fixed: <(Fixed, Fixed)>::load(model.fixed)?,
                shown: Shown::load(model.shown)?,
                stamp: model.stamp,
            })
        }
    }

    impl Saved for Showing {
        type Model = m::Showing;

        fn save(&self) -> m::Showing {
            match self {
                Showing::Shown => m::Showing::Shown,
                Showing::Hidden => m::Showing::Hidden,
            }
        }

        fn load(model: m::Showing) -> Result<Showing, Refused> {
            Ok(match model {
                m::Showing::Shown => Showing::Shown,
                m::Showing::Hidden => Showing::Hidden,
            })
        }
    }

    impl Saved for SpeedClone {
        type Model = m::SpeedClone;

        fn save(&self) -> m::SpeedClone {
            let SpeedClone {
                handle,
                unit,
                leader,
                index,
                code,
                rate,
                when,
                delta,
                shows,
                seen,
                shown,
                level,
                colour,
                deleting,
            } = self;

            m::SpeedClone {
                handle: *handle,
                unit: *unit,
                leader: leader.save(),
                index: *index,
                code: code.save(),
                rate: *rate,
                when: *when,
                delta: *delta,
                shows: shows.save(),
                seen: seen.save(),
                shown: shown.save(),
                level: *level,
                colour: *colour,
                deleting: *deleting,
            }
        }

        fn load(model: m::SpeedClone) -> Result<SpeedClone, Refused> {
            Ok(SpeedClone {
                handle: model.handle,
                unit: model.unit,
                leader: Leader::load(model.leader)?,
                index: model.index,
                code: FourCc::load(model.code)?,
                rate: model.rate,
                when: model.when,
                delta: model.delta,
                shows: Seen::load(model.shows)?,
                seen: Seen::load(model.seen)?,
                shown: Showing::load(model.shown)?,
                level: model.level,
                colour: model.colour,
                deleting: model.deleting,
            })
        }
    }

    impl Saved for Clones {
        type Model = Vec<m::SpeedClone>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Clones, Refused> {
            Vec::load(model).map(Clones)
        }
    }
}
