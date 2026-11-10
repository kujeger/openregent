//! What the world keeps per unit beside its [`super::Piece`]: one record
//! each, by agent, removed with the unit (`super::World::unit_goes`).
//!
//! ```text
//!  Units ── Unit by agent: whether its orders are followed, its order
//!                          tracks, its direction, place jumps, draw effects,
//!                          minimap flag, set played, looping Animate,
//!                          given name, number, spell unit's making;
//!                          what it last revealed, its sight words
//! ```
//!
//! The engine keeps each on the unit itself, so a unit's deletion takes
//! them all (**Inferred** for those no field of the unit is known for).

use std::collections::{BTreeMap, BTreeSet};

use crate::sim::id_table::IdTable;

use super::pieces::Stale;
use super::reveal::{Passed, Sighted, Words};
use super::walking::Tracks;
use super::{
    draw::{Overhead, Tint},
    naming::Given,
    places::Motion,
};

/// One unit's record. Each field `None` or false where not set.
#[derive(Clone, Debug, Default)]
pub(super) struct Unit {
    /// Whether the world follows its orders ([`Motion`]): set through
    /// [`Units::set_motion`], since the collision grid reads it.
    pub(super) motion: Option<Motion>,
    /// Its order tracks, where the world runs them ([`super::walking`]).
    pub(super) tracks: Option<Tracks>,
    /// A new game piece's direction word:
    /// the art slot it draws, which picks its exit ([`super::World::exit`]).
    pub(super) direction: Option<u16>,
    /// Places set outright, not walked to ([`super::draw_view::Shown::jumps`]).
    pub(super) jumps: u32,
    /// Draw effects (`SetDrawEffects`).
    pub(super) tint: Option<Tint>,
    /// Whether `ShowMinimap` put it on the minimap or off.
    pub(super) overhead: Option<Overhead>,
    /// A building's set as its art last played it: what its next set
    /// change stops the systems of ([`super::World::vent`]).
    pub(super) played: Option<u32>,
    /// A building a write found holding track 2 ([`super::World::restyle`]).
    pub(super) skipped: bool,
    /// A building whose track 1 holds a looping `Animate`
    /// ([`super::animate`]).
    pub(super) looping: bool,
    /// The name `SpecifyName` gave ([`super::naming`]).
    pub(super) given: Option<Given>,
    /// A numbered building's number ([`super::numbered`]).
    pub(super) number: Option<i32>,
    /// When a cast made it, a spell unit.
    pub(super) spelled: Option<u32>,
    /// Its half cell, held bit and descriptor when it last revealed about
    /// it ([`super::reveal`]).
    pub(super) sighted: Option<Sighted>,
    /// Its position, held bit and descriptor at the last sight pass: a
    /// change marks it (unit bit 0).
    pub(super) passed: Option<Passed>,
    /// Its sight words, unit, as the last sight pass left them
    /// ([`super::World::sight_pass`]).
    pub(super) words: Option<Words>,
    /// The sight pass when its words last changed: lists 8 to 15 dropped
    /// it and took it again at their tail.
    pub(super) relisted: Option<u32>,
    /// A held reward flag's players, a bit each, whose `VisibleUnitList`
    /// took it when its words last changed: its holder's words then.
    pub(super) flag_list: Option<u8>,
}

/// Every unit's record, by agent, and the order they last entered a
/// building.
#[derive(Clone, Debug, Default)]
pub(super) struct Units {
    records: IdTable<Unit>,
    /// Pieces in the order they last entered a building: relation 2
    /// appends.
    entered: Vec<u32>,
    /// Agents whose motion changed since [`Units::take_stale`]: the
    /// collision grid's to hear of ([`super::pieces`]); and since
    /// [`Units::take_sight`], or whose last reveal went.
    stale: Stale,
}

impl Units {
    /// Agent `id`'s record, where it has one.
    pub(super) fn get(&self, id: u32) -> Option<&Unit> {
        self.records.get(id)
    }

    /// Agent `id`'s record to change, where it has one.
    pub(super) fn get_mut(&mut self, id: u32) -> Option<&mut Unit> {
        self.records.get_mut(id)
    }

    /// Agent `id`'s record, made on first write.
    pub(super) fn entry(&mut self, id: u32) -> &mut Unit {
        self.records.entry(id)
    }

    /// Whether the world follows agent `id`'s orders.
    pub(super) fn motion(&self, id: u32) -> Option<Motion> {
        self.get(id)?.motion
    }

    /// Whether the world follows agent `id`'s orders, still or walking.
    pub(super) fn followed(&self, id: u32) -> bool {
        self.motion(id).is_some()
    }

    /// Agent `id`'s direction word, where a new game gave one.
    pub(super) fn direction(&self, id: u32) -> Option<u16> {
        self.get(id)?.direction
    }

    /// Agent `id`'s sight words, as the last sight pass left them.
    pub(super) fn words(&self, id: u32) -> Option<&Words> {
        self.get(id)?.words.as_ref()
    }

    /// The world follows agent `id`'s orders as `motion` says: what it
    /// did before.
    pub(super) fn set_motion(&mut self, id: u32, motion: Motion) -> Option<Motion> {
        self.stale.insert(id);
        self.entry(id).motion.replace(motion)
    }

    /// The world no longer follows agent `id`'s orders, nor knows them.
    pub(super) fn unfollow(&mut self, id: u32) {
        self.stale.insert(id);

        if let Some(u) = self.get_mut(id) {
            u.motion = None;
        }
    }

    /// The agents whose motion changed, or whose record went, since the
    /// last call.
    pub(super) fn take_stale(&mut self) -> BTreeSet<u32> {
        self.stale.take_grid()
    }

    /// Whether any motion changed since [`Units::take_stale`].
    pub(super) fn any_stale(&self) -> bool {
        self.stale.any_grid()
    }

    /// The agents [`Units::take_stale`] would give, left for it.
    pub(super) const fn stale(&self) -> &BTreeSet<u32> {
        self.stale.grid()
    }

    /// The agents whose motion changed, or whose record went, since the
    /// last call ([`super::World::rereach`]).
    pub(super) fn take_range(&mut self) -> BTreeSet<u32> {
        self.stale.take_range()
    }

    /// The agents whose motion or last reveal changed, or whose record
    /// went, since the last call ([`super::World::sight`]).
    pub(super) fn take_sight(&mut self) -> BTreeSet<u32> {
        self.stale.take_sight()
    }

    /// No unit's last reveal kept: each reveals anew where it stands.
    pub(super) fn unsight_all(&mut self) {
        self.stale.sight_again(self.records.keys());
        self.records.each_mut(|u| u.sighted = None);
    }

    /// Agent `id`'s order tracks, where the world runs them.
    pub(super) fn tracks(&self, id: u32) -> Option<&Tracks> {
        self.get(id)?.tracks.as_ref()
    }

    /// Agent `id`'s order tracks to run.
    pub(super) fn tracks_mut(&mut self, id: u32) -> Option<&mut Tracks> {
        self.get_mut(id)?.tracks.as_mut()
    }

    /// Agent `id`'s order tracks taken off it.
    pub(super) fn take_tracks(&mut self, id: u32) -> Option<Tracks> {
        self.get_mut(id)?.tracks.take()
    }

    /// Agent `id`'s record gone with its unit.
    pub(super) fn remove(&mut self, id: u32) {
        self.stale.insert(id);
        self.records.remove(id);
        self.entered.retain(|e| *e != id);
    }

    /// Agent `id` entered a building: last in the order.
    pub(super) fn enter(&mut self, id: u32) {
        self.entered.retain(|e| *e != id);
        self.entered.push(id);
    }

    /// Where agent `id` stands in the order of entering; `None` if it
    /// has not entered.
    pub(super) fn entry_order(&self, id: u32) -> Option<usize> {
        self.entered.iter().position(|e| *e == id)
    }

    /// Every record, by agent in order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (u32, &Unit)> {
        self.records.iter()
    }
}

impl FromIterator<(u32, Unit)> for Units {
    fn from_iter<I: IntoIterator<Item = (u32, Unit)>>(records: I) -> Units {
        let records: IdTable<Unit> = records.into_iter().collect();

        Units {
            stale: records.keys().collect(),
            records,
            entered: Vec::new(),
        }
    }
}

mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Unit {
        type Model = m::Unit;

        fn save(&self) -> m::Unit {
            m::Unit {
                motion: self.motion.save(),
                tracks: self.tracks.save(),
                direction: self.direction,
                jumps: self.jumps,
                tint: self.tint.save(),
                overhead: self.overhead.save(),
                played: self.played,
                skipped: self.skipped,
                looping: self.looping,
                given: self.given.save(),
                number: self.number,
                spelled: self.spelled,
                sighted: self.sighted.save(),
                passed: self.passed.save(),
                words: self.words,
                relisted: self.relisted,
                flag_list: self.flag_list,
            }
        }

        fn load(model: m::Unit) -> Result<Unit, Refused> {
            Ok(Unit {
                motion: Option::load(model.motion)?,
                tracks: Option::load(model.tracks)?,
                direction: model.direction,
                jumps: model.jumps,
                tint: Option::load(model.tint)?,
                overhead: Option::load(model.overhead)?,
                played: model.played,
                skipped: model.skipped,
                looping: model.looping,
                given: Option::load(model.given)?,
                number: model.number,
                spelled: model.spelled,
                sighted: Option::load(model.sighted)?,
                passed: Option::load(model.passed)?,
                words: model.words,
                relisted: model.relisted,
                flag_list: model.flag_list,
            })
        }
    }

    impl Saved for Units {
        type Model = m::Units;

        fn save(&self) -> m::Units {
            m::Units {
                records: self.records.iter().map(|(id, u)| (id, u.save())).collect(),
                entered: self.entered.clone(),
            }
        }

        fn load(model: m::Units) -> Result<Units, Refused> {
            Ok(Units {
                records: BTreeMap::<u32, Unit>::load(model.records)?
                    .into_iter()
                    .collect(),
                entered: model.entered,
                // A restored board builds its grid whole; its pieces,
                // all new, are looked at by sight.
                stale: Stale::default(),
            })
        }
    }
}
