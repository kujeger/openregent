//! The game pieces: each by the agent it drives, and the new game's not
//! yet announced. Every change to one goes through here and marks it
//! stale, so the collision grid hears of it.
//!
//! ```text
//!  Pieces::get_mut, insert, remove, ... ──> stale ids ──┐
//!  Units::set_motion, remove (unit) ──────> stale ids ──┤
//!                                                       v
//!  World::regrid: each stale id's grid entry anew ──> Board::regrid
//!  World::sight: each stale id, and each unknown at the last, reveals
//! ```
//!
//! Each stale id is kept twice (`Stale`): the grid and sight each take
//! their own.
//!
//! The engine registers a unit on the grid and unregisters it where it
//! changes; here
//! a change is noted where it is made and the grid brought up to date
//! before it is read (`walking::collide`):
//!
//! ```text
//!  engine: GenUnit  off and on, off     here
//!  made, placed, teleported, unhidden   insert, get_mut
//!  a walker claims its next half cell   get_mut
//!  enters a building, held              get_mut
//!  deleted                              remove
//!  retyped                              get_mut
//!  loaded                               World::restore
//!  followed or not (the world's own)    Units::set_motion
//! ```

use std::collections::BTreeSet;

use crate::base::digest::Map;
use crate::data::attrib::Key;
use crate::data::fourcc::FourCc;

use super::spell::Charge;
use super::{Piece, Place};

/// Pieces by agent, the new game's unannounced, and which changed since
/// the grid last heard.
#[derive(Clone, Debug, Default)]
pub struct Pieces {
    /// Pieces by the agent they drive.
    bound: Map<u32, Piece>,
    /// The new game's pieces, until [`super::NEW_UNIT`] announces each.
    unbound: Map<u32, Piece>,
    /// Agents whose piece changed since [`Pieces::take_stale`], and
    /// since [`Pieces::take_sight`].
    stale: Stale,
}

/// Agents changed, kept three times: for the collision grid, for sight
/// ([`super::World::sight`]) and for the range kept for `ListObjects`
/// ([`super::World::rereach`]), each taken when its reader catches up.
#[derive(Clone, Debug, Default)]
pub(super) struct Stale {
    grid: BTreeSet<u32>,
    sight: BTreeSet<u32>,
    range: BTreeSet<u32>,
}

impl Stale {
    /// Agent `id` changed.
    pub(super) fn insert(&mut self, id: u32) {
        self.grid.insert(id);
        self.sight.insert(id);
        self.range.insert(id);
    }

    /// The agents changed since the range's last call.
    pub(super) fn take_range(&mut self) -> BTreeSet<u32> {
        std::mem::take(&mut self.range)
    }

    /// Agents `ids` to look at again at the next sight, not the grid.
    pub(super) fn sight_again(&mut self, ids: impl IntoIterator<Item = u32>) {
        self.sight.extend(ids);
    }

    /// The agents changed since the grid's last call.
    pub(super) fn take_grid(&mut self) -> BTreeSet<u32> {
        std::mem::take(&mut self.grid)
    }

    /// Whether any agent changed since the grid's last call.
    pub(super) fn any_grid(&self) -> bool {
        !self.grid.is_empty()
    }

    /// The agents changed since the grid's last call, left for it.
    pub(super) const fn grid(&self) -> &BTreeSet<u32> {
        &self.grid
    }

    /// The agents changed since sight's last call.
    pub(super) fn take_sight(&mut self) -> BTreeSet<u32> {
        std::mem::take(&mut self.sight)
    }
}

impl Extend<u32> for Stale {
    fn extend<I: IntoIterator<Item = u32>>(&mut self, ids: I) {
        for id in ids {
            self.insert(id);
        }
    }
}

impl<'a> Extend<&'a u32> for Stale {
    fn extend<I: IntoIterator<Item = &'a u32>>(&mut self, ids: I) {
        self.extend(ids.into_iter().copied());
    }
}

impl FromIterator<u32> for Stale {
    fn from_iter<I: IntoIterator<Item = u32>>(ids: I) -> Stale {
        let mut stale = Stale::default();

        stale.extend(ids);
        stale
    }
}

impl Pieces {
    /// `bound` and `unbound`, every one stale.
    pub(super) fn of(bound: Map<u32, Piece>, unbound: Map<u32, Piece>) -> Pieces {
        let mut pieces = Pieces::default();

        pieces.replace(bound, unbound);
        pieces
    }

    /// Every piece `bound` and `unbound` in place of those before: old
    /// and new stale.
    pub(super) fn replace(&mut self, bound: Map<u32, Piece>, unbound: Map<u32, Piece>) {
        let all = |b: &Map<u32, Piece>, u: &Map<u32, Piece>| {
            b.keys().chain(u.keys()).copied().collect::<Vec<u32>>()
        };

        self.stale.extend(all(&self.bound, &self.unbound));
        self.stale.extend(all(&bound, &unbound));
        self.bound = bound;
        self.unbound = unbound;
    }

    // -----------------------------------------------------------------------
    // Bound pieces

    /// Agent `id`'s piece.
    pub(super) fn get(&self, id: u32) -> Option<&Piece> {
        self.bound.get(&id)
    }

    /// Agent `id`'s piece, to change.
    pub(super) fn get_mut(&mut self, id: u32) -> Option<&mut Piece> {
        self.stale.insert(id);
        self.bound.get_mut(&id)
    }

    /// Agent `id`'s piece's owner.
    pub(super) fn owner(&self, id: u32) -> Option<u32> {
        self.get(id).map(|p| p.owner)
    }

    /// Agent `id`'s piece's object handle.
    pub(super) fn handle(&self, id: u32) -> Option<u32> {
        self.get(id).map(|p| p.handle)
    }

    /// Agent `id`'s piece's descriptor.
    pub(super) fn code(&self, id: u32) -> Option<FourCc> {
        self.get(id).map(|p| p.code)
    }

    /// Attribute `key` of agent `id`'s piece; `None` where unset.
    pub(super) fn attribute(&self, id: u32, key: Key) -> Option<i32> {
        self.get(id).and_then(|p| p.get(key))
    }

    /// Agent `id`'s piece, where `owner`'s and its code starts with
    /// `prefix`: a guild of a kind, say.
    pub(super) fn owned(&self, id: u32, owner: u32, prefix: &[u8]) -> Option<&Piece> {
        self.get(id)
            .filter(|p| p.owner == owner && p.code.0.starts_with(prefix))
    }

    /// Where agent `id`'s piece stands, where known.
    pub(super) fn place(&self, id: u32) -> Option<Place> {
        self.get(id).and_then(|p| p.place)
    }

    /// Where agent `id`'s piece stands, to change: stale even where
    /// unknown, as [`Pieces::get_mut`].
    pub(super) fn place_mut(&mut self, id: u32) -> Option<&mut Place> {
        self.get_mut(id).and_then(|p| p.place.as_mut())
    }

    /// Agent `id`'s piece's spell entries, to change, where known.
    pub(super) fn spells_mut(&mut self, id: u32) -> Option<&mut Vec<Charge>> {
        self.get_mut(id).and_then(|p| p.spells.as_mut())
    }

    /// Whether agent `id` drives a piece.
    pub(super) fn contains(&self, id: u32) -> bool {
        self.bound.contains_key(&id)
    }

    /// Agent `id` drives `piece`: the piece it drove, if any.
    pub(super) fn insert(&mut self, id: u32, piece: Piece) -> Option<Piece> {
        self.stale.insert(id);
        self.bound.insert(id, piece)
    }

    /// Agent `id`'s piece gone.
    pub(super) fn remove(&mut self, id: u32) -> Option<Piece> {
        self.stale.insert(id);
        self.bound.remove(&id)
    }

    /// Every bound piece, by agent, in no order.
    pub(super) fn iter(&self) -> impl Iterator<Item = (&u32, &Piece)> {
        self.bound.iter()
    }

    /// Every agent driving a piece, in no order.
    pub(super) fn keys(&self) -> impl Iterator<Item = &u32> {
        self.bound.keys()
    }

    /// Every bound piece, in no order.
    pub(super) fn values(&self) -> impl Iterator<Item = &Piece> {
        self.bound.values()
    }

    /// The bound pieces, for a save.
    pub(super) const fn bound(&self) -> &Map<u32, Piece> {
        &self.bound
    }

    // -----------------------------------------------------------------------
    // The new game's, not yet announced

    /// Agent `id`'s piece, bound or not.
    pub(super) fn any(&self, id: u32) -> Option<&Piece> {
        self.bound.get(&id).or_else(|| self.unbound.get(&id))
    }

    /// Every piece, bound then unbound, in no order within each.
    pub(super) fn all(&self) -> impl Iterator<Item = (&u32, &Piece)> {
        self.bound.iter().chain(self.unbound.iter())
    }

    /// Every piece, bound then unbound, to change.
    pub(super) fn all_mut(&mut self) -> impl Iterator<Item = &mut Piece> {
        self.stale
            .extend(self.bound.keys().chain(self.unbound.keys()));
        self.bound.values_mut().chain(self.unbound.values_mut())
    }

    /// Whether `id` is a new game's piece not yet announced.
    pub(super) fn is_unbound(&self, id: u32) -> bool {
        self.unbound.contains_key(&id)
    }

    /// The unannounced piece `id`, to change.
    pub(super) fn unbound_mut(&mut self, id: u32) -> Option<&mut Piece> {
        self.stale.insert(id);
        self.unbound.get_mut(&id)
    }

    /// The unannounced piece `id`, taken out.
    pub(super) fn unbind(&mut self, id: u32) -> Option<Piece> {
        self.stale.insert(id);
        self.unbound.remove(&id)
    }

    /// Every unannounced piece, taken out, in no order.
    pub(super) fn drain_unbound(&mut self) -> impl Iterator<Item = (u32, Piece)> {
        self.stale.extend(self.unbound.keys());
        self.unbound.drain()
    }

    /// Every bound piece unannounced again: a new game's, made by its
    /// start, waits for its unit pass.
    pub(super) fn unbind_all(&mut self) {
        self.stale.extend(self.bound.keys());
        self.unbound.extend(self.bound.drain());
    }

    /// The unannounced pieces, for a save.
    pub(super) const fn unbound(&self) -> &Map<u32, Piece> {
        &self.unbound
    }

    // -----------------------------------------------------------------------
    // The grid

    /// The agents whose piece changed since the last call.
    pub(super) fn take_stale(&mut self) -> BTreeSet<u32> {
        self.stale.take_grid()
    }

    /// Whether any piece changed since [`Pieces::take_stale`].
    pub(super) fn any_stale(&self) -> bool {
        self.stale.any_grid()
    }

    /// The agents whose piece changed since [`Pieces::take_stale`],
    /// left for it.
    pub(super) const fn stale(&self) -> &BTreeSet<u32> {
        self.stale.grid()
    }

    // -----------------------------------------------------------------------
    // Sight

    /// The agents whose piece changed since the last call, or were
    /// given back ([`Pieces::sight_again`]).
    pub(super) fn take_sight(&mut self) -> BTreeSet<u32> {
        self.stale.take_sight()
    }

    /// The agents whose piece changed since the last call
    /// ([`super::World::rereach`]).
    pub(super) fn take_range(&mut self) -> BTreeSet<u32> {
        self.stale.take_range()
    }

    /// Agents `ids` in the next [`Pieces::take_sight`], changed or not.
    pub(super) fn sight_again(&mut self, ids: impl IntoIterator<Item = u32>) {
        self.stale.sight_again(ids);
    }
}

/// Agent `id`'s bound piece, as a map indexes.
impl std::ops::Index<&u32> for Pieces {
    type Output = Piece;

    fn index(&self, id: &u32) -> &Piece {
        &self.bound[id]
    }
}
