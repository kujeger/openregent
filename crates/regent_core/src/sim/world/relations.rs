//! A unit's `HOME` list, relation 101, as `SetParent` and `ClearFlag`
//! write it. **Binary** unless marked.
//!
//! ```text
//!  SetParent(child, parent)  drop the child's HOME members of the parent's
//!                            sort, a reward flag (unit kind 4) or not;
//!                            then parent's ROSTER (100) and child's HOME
//!                            gain each other, once, at the end
//!  ClearFlag(unit)           drop the unit's HOME members that are flags
//! ```
//!
//! So a hero sent to a flag keeps its guild first: `Parent` still names
//! the guild. `GetParent` names it even with a flag first.

use crate::base::digest::Map;

use super::{Answer, Found, NO_AGENT_ID, World, agent_ref};
use crate::gpl::ops::{self, NULL_AGENT, Value};
use crate::gpl::vm::{Fault, Handle, Vm};

/// Member handles by subject handle, in order.
type Lists = Map<u32, Vec<u32>>;

/// Which `HOME` members go: reward flags, or the others.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Flags,
    Units,
}

impl Sort {
    const fn of(flag: bool) -> Self {
        if flag { Self::Flags } else { Self::Units }
    }
}

/// Every unit's `HOME` list, and each reward flag's `ROSTER`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Relations {
    /// Each piece's `HOME` list, relation 101, by handle: members'
    /// handles in order.
    homes: Lists,
    /// Each reward flag's `ROSTER`, relation 100, by handle: who
    /// `SetParent` sent to it, in order. A member its flag left stays
    /// until sent again; [`World::seekers`] reads it against `homes`.
    seeking: Lists,
}

impl Relations {
    /// Unit `handle`'s `HOME` members, in order.
    pub(super) fn home(&self, handle: u32) -> &[u32] {
        self.homes.get(&handle).map_or(&[], Vec::as_slice)
    }

    /// Unit `handle`'s `HOME` list, `members` from now on.
    pub(super) fn set_home(&mut self, handle: u32, members: Vec<u32>) {
        self.homes.insert(handle, members);
    }

    /// `member` gone from unit `handle`'s `HOME` list.
    pub(super) fn leave(&mut self, handle: u32, member: u32) {
        if let Some(list) = self.homes.get_mut(&handle) {
            list.retain(|m| *m != member);
        }
    }

    /// Relation add's other end: `parent` appended to `child`'s
    /// `HOME` unless there.
    fn adopt(&mut self, child: u32, parent: u32) {
        let list = self.homes.entry(child).or_default();

        if !list.contains(&parent) {
            list.push(parent);
        }
    }

    /// Flag `flag`'s roster takes `child` last: sent again, it goes to
    /// the end.
    fn send(&mut self, flag: u32, child: u32) {
        let roster = self.seeking.entry(flag).or_default();

        roster.retain(|c| *c != child);
        roster.push(child);
    }

    /// Flag `flag`'s roster, in the order sent.
    fn sent(&self, flag: u32) -> &[u32] {
        self.seeking.get(&flag).map_or(&[], Vec::as_slice)
    }

    /// Every unit with a `HOME` list, and its members.
    pub(super) fn homes(&self) -> impl Iterator<Item = (u32, &[u32])> {
        self.homes.iter().map(|(h, m)| (*h, m.as_slice()))
    }
}

impl World {
    /// `GetParent(unit)`, gpl get parent: no unit throws `GetParent() unit
    /// is null`; else the first `HOME` member not a reward flag,
    /// its `AE:0`; none leaves the result as
    /// reset, the null agent. `Parent` takes the first member of any sort.
    pub(super) fn get_parent(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let handle = match self.found(id) {
            Found::Gone => Err(Fault::throw("GetParent() unit is null"))?,
            Found::Piece(p) if p.handle != 0 => Some(p.handle),
            Found::Piece(_) | Found::Unsure => None,
        };
        let homes: Vec<u32> = handle
            .map(|h| self.relations.home(h).to_vec())
            .unwrap_or_default();
        let sorts: Option<Vec<bool>> = homes.iter().map(|m| self.is_flag(*m)).collect();
        let guild = sorts.map(|s| homes.iter().zip(s).find(|(_, flag)| !flag).map(|(m, _)| *m));

        match (handle, guild) {
            (Some(_), Some(Some(member))) => {
                let agent = self.holder(member).unwrap_or(NO_AGENT_ID);
                let unbound = self.pieces.get(agent).is_some_and(super::defeat::unbound);
                let ret = vm.arg(args, 0)?;

                ops::assign(
                    vm.object_mut(ret),
                    &Value::Agent(if unbound { NO_AGENT_ID } else { agent }),
                )?;
                Ok(Answer::Given)
            }
            (Some(_), Some(None)) => Ok(Answer::Given),
            _ => Ok(Answer::Unknown),
        }
    }

    /// Whether the piece with handle `handle` is a reward flag, unit
    ///  4; `None` where the world does not know it.
    pub(super) fn is_flag(&self, handle: u32) -> Option<bool> {
        let id = self.holder(handle)?;

        self.any_piece(id)
            .and_then(|p| p.place)
            .map(|p| p.is_reward())
    }

    /// Drop from `handle`'s `HOME` list the members of `sort`: the loop
    /// in gpl set parent. A deleted member has left already
    /// (**Inferred**).
    fn drop_homes(&mut self, handle: u32, sort: Sort) -> Option<()> {
        let members: Vec<u32> = self
            .relations
            .home(handle)
            .iter()
            .copied()
            .filter(|m| self.holder(*m).is_some())
            .collect();
        let sorts: Option<Vec<bool>> = members.iter().map(|m| self.is_flag(*m)).collect();
        let kept = members
            .into_iter()
            .zip(sorts?)
            .filter(|(_, f)| *f != (sort == Sort::Flags))
            .map(|(m, _)| m)
            .collect();

        self.relations.set_home(handle, kept);
        Some(())
    }

    /// `SetParent(child, parent)`, gpl set parent: both units or a throw;
    /// the child's `HOME` members of the parent's sort go; then
    /// relation add with 100, `ROSTER` on the parent, whose other
    /// end appends the parent to the child's `HOME` unless there.
    pub(super) fn set_parent(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let child = agent_ref(vm, args, 1)?;
        let parent = agent_ref(vm, args, 2)?;
        let (child, parent) = match (self.found(child), self.found(parent)) {
            (Found::Gone, _) => Err(Fault::throw("SetParent() Child unit is null"))?,
            (_, Found::Gone) => Err(Fault::throw("SetParent() Parent unit is null"))?,
            (Found::Piece(c), Found::Piece(p)) => (c.handle, p.handle),
            _ => (0, 0),
        };
        let sort = self.is_flag(parent);
        let dropped = match (child, sort) {
            (0, _) | (_, None) => None,
            (_, Some(flag)) => self.drop_homes(child, Sort::of(flag)),
        };

        match dropped {
            Some(()) => {
                self.relations.adopt(child, parent);

                // A flag's roster takes the child last: dropped above,
                // so sent again it goes to the end.
                if sort == Some(true) {
                    self.relations.send(parent, child);
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `ClearFlag(unit)`, gpl clear flag: a unit or a throw; its `HOME`
    /// members that are flags go.
    pub(super) fn clear_flag(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let handle = match self.found(id) {
            Found::Gone => Err(Fault::throw("ClearFlag() unit is null"))?,
            Found::Piece(p) => p.handle,
            Found::Unsure => 0,
        };
        let dropped = match handle {
            0 => None,
            h => self.drop_homes(h, Sort::Flags),
        };

        Ok(dropped.map_or(Answer::Unknown, |()| Answer::Given))
    }

    /// Whether agent `other`'s piece is in agent `id`'s relation 101
    /// (`HOME`) or 100 (`ROSTER`), as minimap colour asks of the
    /// selection. **Inferred**: the roster read as `HOME`'s other end,
    /// which `SetParent` writes both of.
    #[must_use]
    pub fn kin(&self, id: u32, other: u32) -> bool {
        let handle = |a: u32| self.any_piece(a).map(|p| p.handle).filter(|h| *h != 0);
        let lists = |of: u32, member: u32| self.relations.home(of).contains(&member);

        handle(id)
            .zip(handle(other))
            .is_some_and(|(mine, theirs)| lists(mine, theirs) || lists(theirs, mine))
    }

    /// Reward flag `flag`'s seekers, by agent: its relation 100, the units
    /// whose `HOME` holds it, in the order `SetParent` sent them
    /// (relation link appends). The flag panel counts them,
    /// `RewardFlagMemberController` lists them, and the
    /// minimap lights them while the flag is selected ([`World::kin`]).
    /// One sent before the world kept order comes after, by handle.
    #[must_use]
    pub fn seekers(&self, flag: u32) -> Vec<u32> {
        let handle = self.any_piece(flag).map_or(0, |p| p.handle);
        let homed = |child: &u32| handle != 0 && self.relations.home(*child).contains(&handle);
        let sent: Vec<u32> = self
            .relations
            .sent(handle)
            .iter()
            .copied()
            .filter(homed)
            .collect();
        let mut unsent: Vec<u32> = self
            .relations
            .homes()
            .map(|(c, _)| c)
            .filter(|c| homed(c) && !sent.contains(c))
            .collect();

        unsent.sort_unstable();

        sent.into_iter()
            .chain(unsent)
            .filter_map(|c| self.holder(c))
            .collect()
    }

    /// Agent `id`'s `HOME`: relation 101's first member's agent, its
    /// `AE:0`. A deleted member leaves the list (**Inferred**); one
    /// `DeleteAgent` left standing gives agent 0. **Game**: the no-budget
    /// defeat game, 378,500 ms: the razed palace's peasants' `GetPalace`
    /// reads `??#0`.
    fn home(&self, id: u32) -> Option<u32> {
        let handle = self.any_piece(id)?.handle;
        let member = self
            .relations
            .home(handle)
            .iter()
            .find_map(|h| self.holder(*h))?;
        let unbound = self.pieces.get(member).is_some_and(super::defeat::unbound);

        Some(if unbound { NO_AGENT_ID } else { member })
    }

    /// `Parent(agent)`, gpl parent: the unit's relation 101, `HOME`, not
    /// its holder: its first member's `AE:0`; else agent -1, also for a
    /// missing unit.
    pub(super) fn parent(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        // A piece the log announced, handle 0: its relations are not known.
        let parent = match self.found(id) {
            Found::Piece(p) if p.handle == 0 => None,
            Found::Piece(_) => Some(self.home(id).unwrap_or(NULL_AGENT)),
            Found::Unsure => None,
            Found::Gone => Some(NULL_AGENT),
        };

        match parent {
            Some(p) => {
                let ret = vm.arg(args, 0)?;

                ops::assign(vm.object_mut(ret), &Value::Agent(p))?;
                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};

    impl Saved for Relations {
        type Model = m::Relations;

        fn save(&self) -> m::Relations {
            m::Relations {
                homes: self.homes.save(),
                seeking: self.seeking.save(),
            }
        }

        fn load(model: m::Relations) -> std::result::Result<Relations, Refused> {
            Ok(Relations {
                homes: Map::load(model.homes)?,
                seeking: Map::load(model.seeking)?,
            })
        }
    }
}
