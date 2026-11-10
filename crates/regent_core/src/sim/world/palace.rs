//! Each player's palace: the handle player palace handle keeps, `SetPalace`
//! and `ListPalaces`, and the one a new game selects. **Binary** unless
//! marked.

use super::{NO_AGENT_ID, World, agent_ref, int_ref, list_of};
use crate::gpl::ops::Value;
use crate::gpl::startup;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::piece_state;
use std::collections::BTreeMap;

impl World {
    /// The agents `Palace_Pre_Birth` runs on: one per owner, in owner
    /// order, its first palace in unit order, else its first outpost
    /// ([`startup::PALACES`]).
    #[must_use]
    pub fn palaces(&self) -> Vec<u32> {
        let mut ids: Vec<&u32> = self.pieces.keys().collect();
        let mut found: BTreeMap<u32, (usize, u32)> = BTreeMap::new();

        ids.sort();

        for id in ids {
            let piece = &self.pieces[id];
            let rank = startup::PALACES
                .iter()
                .position(|p| piece.code.0.starts_with(*p));

            if let Some(rank) = rank {
                let best = found.entry(piece.owner).or_insert((rank, *id));

                if rank < best.0 {
                    *best = (rank, *id);
                }
            }
        }

        found.values().map(|(_, id)| *id).collect()
    }

    /// Player palace of: `player`'s palace, by
    /// agent: the handle it keeps (`SetPalace`), else the piece
    /// `Palace_Pre_Birth` runs on, its first palace in unit order, else its
    /// first outpost ([`startup::PALACES`]). A new game centres its view on
    /// the crown's and selects it.
    #[must_use]
    pub fn player_palace(&self, player: u32) -> Option<u32> {
        self.players
            .palace(player)
            .or_else(|| self.founded(player))
            .and_then(|handle| self.holder(handle))
    }

    /// The handle of `player`'s first palace by handle, else its first
    /// outpost: the piece `Palace_Pre_Birth` runs on ([`World::palaces`]).
    pub(super) fn founded(&self, player: u32) -> Option<u32> {
        self.pieces
            .all()
            .map(|(_, p)| p)
            .filter(|p| p.owner == player)
            .filter_map(|p| {
                startup::PALACES
                    .iter()
                    .position(|code| p.code.0.starts_with(*code))
                    .map(|rank| (rank, p.handle))
            })
            .min()
            .map(|(_, handle)| handle)
    }

    /// The selected piece, by agent: the one with unit state bit 0
    /// ([`crate::sim::world::piece_state::SELECTED`]). Every game start drops a save's;
    /// a new game's is then each
    /// human seat's palace, selected
    /// before frame 1. With no palace nothing is selected.
    ///
    /// The world keeps the bit as the engine did at its start; a player's
    /// later picks are the interface's. Of several, the lowest agent
    /// (**Inferred**: a save holds at most one).
    #[must_use]
    pub fn selected(&self) -> Option<u32> {
        self.pieces
            .all()
            .filter(|(id, _)| **id != NO_AGENT_ID)
            .filter(|(_, p)| {
                p.place
                    .is_some_and(|pl| pl.state & piece_state::SELECTED != 0)
            })
            .map(|(id, _)| *id)
            .min()
    }

    /// `player`'s selected piece, by agent ([`World::selected`]): a new
    /// game's start selection on its machine. Every human seat's palace
    /// is selected; each machine shows its own human's.
    #[must_use]
    pub fn selection(&self, player: u32) -> Option<u32> {
        self.pieces
            .all()
            .filter(|(id, _)| **id != NO_AGENT_ID)
            .filter(|(_, p)| p.owner == player)
            .filter(|(_, p)| {
                p.place
                    .is_some_and(|pl| pl.state & piece_state::SELECTED != 0)
            })
            .map(|(id, _)| *id)
            .min()
    }

    /// `ListPalaces()`, gpl list palaces: for players 0 to 7 in the level, its
    /// palace ([`World::palace_unit`]); those with an agent, appended.
    pub(super) fn list_palaces(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let ret = vm.arg(args, 0)?;
        list_of(vm.object(ret))?;
        let players: Vec<u32> = (0u32..)
            .take(super::TEAMS)
            .filter(|p| self.players.contains(*p))
            .collect();
        let units: Vec<u32> = players
            .into_iter()
            .filter_map(|player| self.palace_unit(player))
            .collect();
        let found: Vec<Value> = units
            .into_iter()
            .filter(|id| {
                self.pieces
                    .get(*id)
                    .is_some_and(|p| !super::defeat::unbound(p))
            })
            .map(Value::Agent)
            .collect();

        if let Value::List(out) = vm.object_mut(ret) {
            out.extend(found);
        }

        Ok(())
    }

    /// Player palace: the unit of the handle player
    ///  keeps, by agent; with none kept, the first building in unit
    /// list 0 the player owns whose code starts `ABJ` or `ABm`, kept.
    /// `SetPalace` moves it. **Inferred**: unit list 0 in handle order.
    pub(super) fn palace_unit(&mut self, player: u32) -> Option<u32> {
        if self.players.palace(player).is_none()
            && let Some(first) = self.first_palace(player)
        {
            self.players.set_palace(player, first);
        }

        self.players.palace(player).and_then(|h| self.holder(h))
    }

    /// The handle of the first `ABJ` or `ABm` building `player` owns.
    fn first_palace(&self, player: u32) -> Option<u32> {
        self.pieces
            .all()
            .map(|(_, p)| p)
            .filter(|p| p.owner == player)
            .filter(|p| p.is_building())
            .filter(|p| {
                crate::gpl::startup::PALACES
                    .iter()
                    .any(|code| p.code.0.starts_with(*code))
            })
            .map(|p| p.handle)
            .min()
    }

    /// `SetPalace(agent, player)`, gpl set palace: the unit's handle becomes
    /// the player's palace. Throws with no
    /// unit, or no such player.
    pub(super) fn set_palace(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<(), Fault> {
        let id = agent_ref(vm, args, 1)?;
        let handle = self
            .unit(id)
            .map(|p| p.handle)
            .ok_or_else(|| Fault::throw("Function $SetPalace: unit is null."))?;
        let player = int_ref(vm, args, 2)?.cast_unsigned();

        if self.players.contains(player) {
            self.players.set_palace(player, handle);
            Ok(())
        } else {
            Err(Fault::throw("Function $SetPalace: player is invalid."))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::Fixed;
    use crate::sim::world::kind;
    use crate::sim::world::{Piece, Place};

    /// Pieces [`several_selected_pieces_give_the_lowest_agent`] marks.
    const MARKED: u32 = 64;

    /// A world whose pieces all carry the selected bit gives the lowest
    /// agent, not the first the map walks to.
    #[test]
    fn several_selected_pieces_give_the_lowest_agent() {
        let mut world = World::default();

        for id in (1..=MARKED).rev() {
            let place = Place {
                kind: kind::Kind::BUILDING,
                fixed: (Fixed::ZERO, Fixed::ZERO),
                standing: (0, 0),
                holder: 0,
                state: piece_state::SELECTED,
            };

            world.pieces.insert(
                id,
                Piece {
                    place: Some(place),
                    ..Piece::default()
                },
            );
        }

        assert_eq!(world.selected(), Some(1));
    }
}
