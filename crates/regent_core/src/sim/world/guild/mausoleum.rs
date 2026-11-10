//! The Mausoleum: a dead hero interred, and raised for gold. **Binary**
//! and SDK unless marked; no trace holds a Mausoleum.
//!
//! ```text
//!  death    gravestone: the grave, be_dead its grave time later
//!  be_dead  Check_Mausoleum (GPLMx Mausoleum.gpl): not a Monk, Healer
//!           or Paladin, and a finished Mausoleum of the owner's with
//!           fewer than #Mausoleum_Limit "occupants": CancelGuildMembership,
//!           Hide #TeleportInsideDestination, "Type" Dead, its thread
//!           killed, listed; else the grave goes as before
//!  panel    Mausoleum_Controller: 8014 GMTX 185 of relation 2's count;
//!           0x1f44 opens MX05 (owner local, built, not empty)
//!           MausoleumP2_Controller: relation 2 listed
//!           (5000); a row's price Mausoleum_Resurrect_Cost(hero), an
//!           interface run, against APP:0 enables 0x138b, which sends GUILD_ACTION 0x15:
//!           the Mausoleum's handle, the hero's, the price
//!  decode   guild action apply, case 0x15: the hero with an agent,
//!           its owner's APP:0 at least the price: taken, then
//!           Mausoleum_Resurrect_Begin(hero) dispatched; the Mausoleum
//!           is not read
//! ```
//!
//! The scripts do the rest: `Mausoleum_Resurrect_Begin` un-hides the hero
//! with an `XR11` effector whose end runs `Mausoleum_Resurrect_Finish`.

use crate::data::attrib::FIRST_STAGE_BUILT;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::{Answer, Found, World, agent_ref};

/// What the decode dispatches: the string `"Mausoleum_Resurrect_Begin"`.
pub const RESURRECT_BEGIN: &str = "Mausoleum_Resurrect_Begin";

/// What the panel asks a row's price of: the string `"Mausoleum_Resurrect_Cost"`.
pub const RESURRECT_COST: &str = "Mausoleum_Resurrect_Cost";

/// The scripts' capacity: `#Mausoleum_Limit`, 10 (`mx_Globals.gpl`).
pub const MAUSOLEUM_LIMIT: &str = "#Mausoleum_Limit";

/// The Mausoleum's code.
pub const MAUSOLEUM: [u8; 4] = *b"ABp1";

/// `GUILD_ACTION` `0x15`: message words `0x0c` to `0x14`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resurrect {
    /// By agent; sent, never read.
    pub mausoleum: u32,
    /// By agent.
    pub hero: u32,
    /// The panel's `Mausoleum_Resurrect_Cost`.
    pub price: i32,
}

impl World {
    /// Building `id`'s contents, relation 2, in entry order: a
    /// Mausoleum's interred.
    #[must_use]
    pub fn interred(&self, id: u32) -> Vec<u32> {
        self.pieces
            .get(id)
            .and_then(|p| self.occupants(p.handle))
            .unwrap_or_default()
    }

    /// What Mausoleum `id`'s panel lists to raise: its interred, where
    /// its button `0x1f44` opens the list:
    /// `owner`'s, `APB:1` set, not empty. Else none.
    #[must_use]
    pub fn mausoleum_offer(&self, owner: u32, id: u32) -> Vec<u32> {
        let open = self.pieces.get(id).is_some_and(|p| {
            let built = p.get(FIRST_STAGE_BUILT);

            p.owner == owner && p.code.0 == MAUSOLEUM && built.is_some_and(|b| b != 0)
        });

        if open { self.interred(id) } else { Vec::new() }
    }

    /// Case `0x15`'s test and payment: the hero has an agent and its
    /// owner's treasury holds the price, which goes. True when
    /// [`RESURRECT_BEGIN`] is to be dispatched.
    pub fn resurrect(&mut self, r: &Resurrect) -> bool {
        let owner = self
            .agents
            .alive(r.hero)
            .then(|| self.pieces.owner(r.hero))
            .flatten();

        owner.is_some_and(|o| self.spend(o, r.price))
    }

    /// `CancelGuildMembership(unit)`, gpl cancel guild membership: a unit or a throw;
    /// its first `HOME` member not a reward flag drops
    /// it from its roster, relation 100, so the home
    /// leaves its `HOME` too (**Inferred**: the drop is mutual, as
    /// relation member drop is for 100 and 101).
    pub(in crate::sim::world) fn cancel_guild(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let handle = match self.found(id) {
            Found::Gone => Err(Fault::throw("CancelGuildMembership() unit is null"))?,
            Found::Piece(p) => Some(p.handle),
            Found::Unsure => None,
        };
        let homes: Vec<u32> = handle
            .map(|h| self.relations.home(h).to_vec())
            .unwrap_or_default();
        // The walk stops at the first member not a flag: one past it is
        // never asked, known or not.
        let guild = homes
            .iter()
            .map(|m| self.is_flag(*m).map(|flag| (*m, flag)))
            .find(|sort| sort.is_none_or(|(_, flag)| !flag))
            .map_or(Some(None), |sort| sort.map(|(m, _)| Some(m)));

        match (handle, guild) {
            (Some(h), Some(g)) => {
                if let Some(g) = g {
                    self.relations.leave(h, g);
                }

                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::Fixed;
    use crate::gpl::builtins;
    use crate::gpl::load::Program;
    use crate::gpl::ops::Value;
    use crate::gpl::vm::Config;
    use crate::gpl::vm::host::Call;
    use crate::sim::world::kind;
    use crate::sim::world::{Agent, Piece, Place};

    const ELF: u32 = 242;
    const ELF_HANDLE: u32 = 7276;
    const LODGE: u32 = 243;
    const LODGE_HANDLE: u32 = 7278;
    /// A handle no piece has.
    const GONE: u32 = 14_567;

    fn placed(handle: u32, kind: kind::Kind) -> Piece {
        Piece {
            handle,
            place: Some(Place {
                kind,
                fixed: (Fixed::ZERO, Fixed::ZERO),
                standing: (0, 0),
                holder: 0,
                state: 0,
            }),
            ..Piece::default()
        }
    }

    /// Run builtin `name` on `args`.
    fn run(world: &mut World, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Answer {
        let builtin = builtins::id(name).expect("a builtin");
        let handles: Vec<Handle> = std::iter::once(Value::Null)
            .chain(args.iter().cloned())
            .map(|v| {
                let o = vm.alloc(v);
                vm.top_handle(Some(o))
            })
            .collect();
        let call = Call {
            builtin,
            args: &handles,
        };

        world
            .run(
                vm,
                &call,
                &|_, _, _| None,
                crate::sim::world::step::Draws::Kept,
            )
            .expect("an answer")
    }

    /// The serpents game's elf: its `HOME` its lodge, then a handle the
    /// world has no piece for. Home first guild stops at the
    /// lodge, so what follows it is never asked.
    #[test]
    fn cancel_guild_membership_stops_at_the_first_guild() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let mut world = World::default();

        world
            .pieces
            .insert(ELF, placed(ELF_HANDLE, kind::Kind::VEHICLE));
        world
            .pieces
            .insert(LODGE, placed(LODGE_HANDLE, kind::Kind::BUILDING));
        world.agents.insert(ELF, Agent::default());
        world.agents.insert(LODGE, Agent::default());
        world
            .relations
            .set_home(ELF_HANDLE, vec![LODGE_HANDLE, GONE]);

        let answer = run(
            &mut world,
            &mut vm,
            "CancelGuildMembership",
            &[Value::Agent(ELF)],
        );

        assert_eq!(answer, Answer::Given);
        assert_eq!(world.relations.home(ELF_HANDLE), [GONE]);
    }

    /// **Game** (the user): an interred hero is no longer listed among
    /// its guild's members. `Check_Mausoleum` cancels the membership,
    /// and the roster is read off the members' `HOME` lists.
    #[test]
    fn an_interred_hero_leaves_its_guilds_roster() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let mut vm = Vm::new(&program, Config::default());
        let mut world = World::default();

        world
            .pieces
            .insert(ELF, placed(ELF_HANDLE, kind::Kind::VEHICLE));
        world
            .pieces
            .insert(LODGE, placed(LODGE_HANDLE, kind::Kind::BUILDING));
        world.agents.insert(ELF, Agent::default());
        world.agents.insert(LODGE, Agent::default());
        world.relations.set_home(ELF_HANDLE, vec![LODGE_HANDLE]);
        assert_eq!(world.roster(LODGE), vec![(ELF, false)]);

        run(
            &mut world,
            &mut vm,
            "CancelGuildMembership",
            &[Value::Agent(ELF)],
        );

        assert_eq!(world.roster(LODGE), Vec::new());
    }
}
