//! Where pieces are: a piece's place where the world knows it, its half
//! cell and pad rect, its holder, and the board. **Binary** unless
//! marked.

use super::reveal::HOLDERS_MOST;
use super::{Answer, Found, NO_AGENT_ID, Piece, Place, World};
use crate::base::fixed::Fixed;
use crate::data::attrib;
use crate::gpl::ops;
use crate::gpl::ops::{NULL_AGENT, Value};
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::space;
use crate::sim::space::HalfRect;

/// Whether the world follows a piece's orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// Followed: its place is known, its orders run in `World::step`.
    Still,
    /// An order the world does not follow: its place is unknown.
    Walking,
}

impl World {
    /// An id at or past the next the world would give, as the engine's
    /// negative ids for a building being placed (**Game**:
    /// `Marketplace1#-6`), is unsure: the engine made it unseen.
    pub(super) fn found(&self, id: u32) -> Found<'_> {
        let unseen =
            self.pieces.is_unbound(id) || (id >= self.agents.next_id() && id != NULL_AGENT);

        match (self.unit(id), unseen) {
            (Some(piece), _) => Found::Piece(piece),
            (None, true) => Found::Unsure,
            (None, false) => Found::Gone,
        }
    }

    /// Attribute `key` of agent `id`'s piece, 0 where unset or gone.
    #[must_use]
    pub fn word_of(&self, id: u32, key: attrib::Key) -> i32 {
        self.pieces.attribute(id, key).unwrap_or(0)
    }

    /// A bound or unbound piece.
    pub(super) fn any_piece(&self, id: u32) -> Option<&Piece> {
        self.pieces.any(id)
    }

    /// The half cell piece `id` measures from, agent position: its
    /// own, or its holder's while held. Known for a new game's piece that
    /// does not walk.
    pub(super) fn spot(&self, id: u32) -> Option<(i32, i32)> {
        self.spot_in(id, 0)
    }

    /// [`World::spot`], `depth` holders up: none past [`HOLDERS_MOST`].
    fn spot_in(&self, id: u32, depth: usize) -> Option<(i32, i32)> {
        let place = self.known(id).filter(|_| depth < HOLDERS_MOST)?;

        self.spot_at(place, depth)
    }

    /// [`World::spot_in`] from `place`, the piece's known place.
    pub(super) fn spot_at(&self, place: &Place, depth: usize) -> Option<(i32, i32)> {
        if place.holder != 0 && place.state & Place::OWN_SPOT == 0 {
            self.spot_in(self.holder(place.holder)?, depth + 1)
        } else {
            Some(place.standing)
        }
    }

    /// The agent whose piece has handle `handle`, agent by handle:
    /// what an input naming a handle acts on.
    #[must_use]
    pub fn by_handle(&self, handle: u32) -> Option<u32> {
        self.holder(handle)
    }

    /// The agent whose piece has handle `handle`, agent by handle.
    pub(super) fn holder(&self, handle: u32) -> Option<u32> {
        self.pieces
            .all()
            .find(|(id, p)| **id != NO_AGENT_ID && p.handle == handle)
            .map(|(id, _)| *id)
    }

    /// Piece `id`'s pad rect where it stands.
    pub(super) fn footprint(&self, id: u32) -> Option<HalfRect> {
        let shape = self.descriptors.shape(self.any_piece(id)?.code)?;

        Some(space::placed(shape.rect, self.spot(id)?))
    }

    /// Agent `id`'s place as the world has it, for a wrong answer's
    /// detail: position, half cell, pad rect.
    #[must_use]
    pub fn describe(&self, id: u32) -> String {
        format!(
            "#{id} at {:?} half {:?} pad {:?}",
            self.position(id),
            self.spot(id),
            self.footprint(id)
        )
    }

    /// A piece's place when the world knows it: a new game's piece that
    /// does not walk, or one whose orders it follows.
    pub(super) fn known(&self, id: u32) -> Option<&Place> {
        known_of(self.any_piece(id)?, self.units.motion(id))
    }

    /// Piece `id`'s orders, where the world follows them.
    #[must_use]
    pub fn motion(&self, id: u32) -> Option<Motion> {
        self.units.motion(id)
    }

    /// Unit pos as 32nds of a cell, what `LocationOf` answers.
    /// `None` where the place is not known.
    #[must_use]
    pub fn position(&self, id: u32) -> Option<(i32, i32)> {
        self.unit_pos(id)
            .map(|(x, y)| (space::pos32(x), space::pos32(y)))
    }

    /// Unit pos, 16.16: a held piece is at its
    /// holder; a held overlay, descriptor kind 4 or 5, adds its own
    /// position as an offset. The world's held overlays are reward flags
    /// ([`World::flag`]).
    pub(super) fn unit_pos(&self, id: u32) -> Option<(Fixed, Fixed)> {
        self.unit_pos_in(id, 0)
    }

    /// [`World::unit_pos`], `depth` holders up: none past [`HOLDERS_MOST`].
    fn unit_pos_in(&self, id: u32, depth: usize) -> Option<(Fixed, Fixed)> {
        let place = self.known(id).filter(|_| depth < HOLDERS_MOST)?;

        self.unit_pos_at(place, depth)
    }

    /// [`World::unit_pos_in`] from `place`, the piece's known place.
    pub(super) fn unit_pos_at(&self, place: &Place, depth: usize) -> Option<(Fixed, Fixed)> {
        if place.holder != 0 && place.state & Place::OWN_SPOT == 0 {
            let (x, y) = self.unit_pos_in(self.holder(place.holder)?, depth + 1)?;

            if place.is_reward() {
                Some((x + place.fixed.0, y + place.fixed.1))
            } else {
                Some((x, y))
            }
        } else {
            Some(place.fixed)
        }
    }

    /// Whether piece `id`'s attribute list is all known: a new game's is
    /// its save's; one the world made is [`super::bodies::initial`] or
    /// [`super::build::initial`] and what it was given, its lists included.
    pub(super) fn complete(&self, id: u32) -> bool {
        self.any_piece(id).is_some_and(|p| p.handle != 0)
    }

    /// `GetBoardExtents()`: the map's width and height, 32nds.
    pub(super) fn board_extents(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        let (w, h) = self.board.extent();
        let at = Value::Coord(w, h);

        ops::assign(vm.object_mut(ret), &at)?;
        Ok(Answer::Given)
    }
}

/// [`World::known`] of a piece whose orders' `motion` is read already.
pub(super) fn known_of(piece: &Piece, motion: Option<Motion>) -> Option<&Place> {
    match motion {
        Some(Motion::Still) => piece.place.as_ref(),
        Some(Motion::Walking) => None,
        None => piece.fixed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::CROWN;
    use crate::sim::world::kind;

    /// A piece a save has holding itself: its place unknown, not a climb
    /// until the stack runs out.
    #[test]
    fn a_piece_holding_itself_has_no_place() {
        const HANDLE: u32 = 900;

        let mut world = World::default();
        let place = Place {
            kind: kind::Kind::REWARD,
            fixed: (Fixed::ZERO, Fixed::ZERO),
            standing: (0, 0),
            holder: HANDLE,
            state: 0,
        };
        let piece = Piece {
            owner: CROWN,
            handle: HANDLE,
            place: Some(place),
            ..Piece::default()
        };

        world.add_test_piece(1, piece.clone());

        assert_eq!(world.spot(1), None);
        assert_eq!(world.unit_pos(1), None);
        assert_eq!(world.word(1, &piece, 1), None);
    }
}
