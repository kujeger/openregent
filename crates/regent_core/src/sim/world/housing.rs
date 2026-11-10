//! Units in buildings: `Hide`'s walk in or teleport, the hold itself, and
//! the builtins that read who holds whom. **Binary** unless marked.
//!
//! ```text
//!  Hide(unit, building) ──> FollowPath to its entrance (walking)
//!                            ── at the end ──> held:
//!                               holder = building,
//!                               state HELD, bit 7 clear
//!  InsideBuilding, IsHidden   the holder names a live unit
//!  GetBuildingContainer(unit) its holder's agent; not held:
//!                             the return untouched
//!  GetContainerAgent(unit)    its holder's AE:0, else -1
//!  GetEntranceLoc(unit)       GenUnit's exit
//! ```

use super::{Answer, Found, Place, World, agent_ref, int_ref, places::Motion, put_bool};
use crate::data::attrib::Key;
use crate::gpl::ops;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::piece_state;

/// `#TeleportInsideDestination`, `Hide`'s third argument for entering at
/// once: bank `GPL`, index 1.
pub const TELEPORT: i32 = Key::new(*b"GPL", 1).packed();

/// The agent `GetContainerAgent` gives a unit no one holds:
/// `gpl_agentref_ctor(-1)` in gpl get container agent.
pub const UNHELD: u32 = u32::MAX;

impl World {
    /// unit enter now: relation 2, `INSIDE`, makes the building the
    /// holder; the unit is held, measured from its holder (bit 7 clear).
    /// Its own position stays (**Game**: the day1 save's three hold their
    /// spawn points).
    pub(super) fn enter(&mut self, id: u32, building: u32) {
        let handle = self.pieces.handle(building);

        if let (Some(h), Some(place)) = (handle, self.pieces.place_mut(id)) {
            place.holder = h;
            place.state = (place.state | piece_state::HELD) & !Place::OWN_SPOT;
        }

        // Relation 2 appends: the building's contents in entry order.
        self.units.enter(id);

        self.units.set_motion(id, Motion::Still);
        self.jumped(id);

        // Unit enter now: the building's `GSUnit`.
        self.restyle(building, super::plume::Force::Forced);
    }

    /// `Hide(unit, building, [flag])`, gpl hide: throws unless both
    /// have units; then `GE_OrderProcessor`: nothing
    /// for a unit already held; `#TeleportInsideDestination` enters at
    /// once; else a walk in ([`World::walk_in`]).
    ///
    /// **Game**, the day1 probe: a unit spawned at its spawner's exit and
    /// told to enter it is moving that frame and inside the next.
    pub(super) fn hide(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let unit = agent_ref(vm, args, 1)?;
        let building = agent_ref(vm, args, 2)?;
        let teleport = args.len() > 3 && int_ref(vm, args, 3)? == TELEPORT;
        let missing = || {
            Fault::throw(format!(
                "Unit (agent#{}) could not enter building (agent#{})",
                unit.cast_signed(),
                building.cast_signed()
            ))
        };

        let both = match (self.found(unit), self.found(building)) {
            (Found::Gone, _) | (_, Found::Gone) => Err(missing())?,
            (Found::Piece(_), Found::Piece(_)) => true,
            _ => false,
        };
        let held = self.known(unit).filter(|_| both).map(|p| p.holder != 0);

        match held {
            None => Ok(Answer::Unknown),
            Some(true) => Ok(Answer::Given),
            Some(false) if teleport => {
                self.enter(unit, building);
                Ok(Answer::Given)
            }
            Some(false) => {
                self.walk_in(unit, building);
                Ok(Answer::Given)
            }
        }
    }

    /// Whether piece `id` is held by a live piece: what `InsideBuilding`
    /// answers. `None` where its place is not known.
    #[must_use]
    pub fn inside_building(&self, id: u32) -> Option<bool> {
        self.known(id)
            .map(|p| p.holder != 0 && self.holder(p.holder).is_some())
    }

    /// `InsideBuilding` and `IsHidden`, one
    /// test: unit holder, names a live unit. Both throw
    /// `InsideBuilding`'s message when the unit is gone.
    pub(super) fn inside(
        &self,
        vm: &mut Vm<'_>,
        call: &crate::gpl::vm::host::Call<'_>,
    ) -> std::result::Result<Answer, Fault> {
        let args = call.args;
        let id = agent_ref(vm, args, 1)?;
        let inside = match self.found(id) {
            Found::Piece(_) => self.inside_building(id),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(format!(
                "Function $InsideBuilding( agent#{} ) failed: unit no longer exists.",
                id.cast_signed()
            )))?,
        };

        match inside {
            Some(b) => {
                let ret = vm.arg(args, 0)?;

                put_bool(vm, ret, b)?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `GetBuildingContainer(unit)`: the agent holding it; not held, the
    /// return keeps what it held.
    pub(super) fn container(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Gone => Err(Fault::throw("GetBuildingContainer() unit is null"))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(p) => {
                let holder = p.place.map(|at| at.holder).filter(|h| *h != 0);

                match holder.map(|h| self.holder(h)) {
                    Some(Some(agent)) => {
                        let ret = vm.arg(args, 0)?;

                        ops::assign(vm.object_mut(ret), &Value::Agent(agent))?;
                        Ok(Answer::Given)
                    }
                    Some(None) => Ok(Answer::Unknown),
                    None => Ok(Answer::Given),
                }
            }
        }
    }

    /// `GetContainerAgent(unit)`, gpl get container agent: the holder's `AE:0`
    /// agent, else agent -1. No null test: with no
    /// unit the engine reads through a null pointer, unknown.
    pub(super) fn container_agent(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let holder = match self.found(id) {
            Found::Piece(p) => match p.place.map_or(0, |at| at.holder) {
                0 => Some(UNHELD),
                h => self.holder(h),
            },
            Found::Gone | Found::Unsure => None,
        };

        match holder {
            Some(agent) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &Value::Agent(agent))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `GetEntranceLoc(unit)`, gpl get entrance loc: unit or a throw; its
    /// `GenUnit`, the exit, which is where `Hide` enters
    /// (`GenUnit`, the same point): a held unit's holder's.
    pub(super) fn entrance(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = match self.found(id) {
            Found::Piece(_) => self.exit(id),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("Function $GetEntranceLoc: unit is null."))?,
        };

        match at {
            Some((x, y)) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &Value::Coord(x, y))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}
