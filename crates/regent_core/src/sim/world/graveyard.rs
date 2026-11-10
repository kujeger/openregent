//! `SpawnGraveyard`: the building `be_dead` raises for every
//! `#Graveyard_limit` heroes the palace has lost. **Binary**.
//!
//! ```text
//!  SpawnGraveyard(unit, name, [coord])
//!    no unit          throw "Function $SetAttribute( agent#n ) failed ..."
//!    where            the coord, else the unit's exit (GenUnit)
//!    no descriptor    throw "... (could not find description) "
//!    the owner's dead hero list (player) appended to the level's
//!      hero lists, the player's emptied: GravesIndex its index
//!    a vehicle        throw "SpawnGraveyard() called on a vehicle "
//!    any other kind   throw "... (not a building or vehicle) "
//!    the building is placed (level, desc, 8, where, the unit's owner,
//!      0, flags 4: no price, margin ConstructData1) with APU:0 1 and
//!      APB:8 GravesIndex; NewUnitInit
//!    the return slot is not written
//! ```
//!
//! The lists, as the engine keeps them:
//!
//! ```text
//!  GSUnit, a hero deleted: its name not empty
//!    (name, code, APV:11) appended to its owner's
//!    dead list; GE_Level drains the
//!    deletes last first, so two in one pass go in that order
//!  SpawnGraveyard        that list moved to the level's
//!  GSGL                  the level's lists, each player's: gsgl::Hero
//!  GraveyardController (AP15), the local player's only:
//!    8014 GMTX 185 "%d Heroes buried here", its list's length; 8004
//!    shown, opening AP16. Another's: 8004 hidden, 8014 blank
//!  GraveyardListController (AP16): a row per hero in
//!    list order; detail on, GMTX 221 "%s,\nLevel %d" and the code's icon
//!    (the descriptor's, widget 1002), off the name alone
//! ```
//!
//! No other code reads either list: no resurrection takes a hero off.
//!
//! **Corpus**: `Freestyle Game-15 Days s1 t5`'s graveyard `BBJ1` holds
//! `APB:8` 0 and 1 hit point; its `GSGL` one hero list, the 14 dead the
//! 14-day save's player 0 holds, in that order.

use super::chat::Who;
use super::{Answer, Found, World, agent_ref, bodies::Class, string_ref};
use crate::data::attrib::{EXPERIENCE_LEVEL, HP, Key};
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

/// A dead hero, as a graveyard lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grave {
    /// What spells its name ([`crate::view::panel::portrait::own_name`]).
    pub name: Who,
    /// Its code, unit code.
    pub code: FourCc,
    /// `APV:11` as it died.
    pub level: i32,
}

/// `APB:8`, `GravesIndex`: the hero list a graveyard shows
/// (`0x8425041`, gpl spawn graveyard).
pub const GRAVES_INDEX: Key = Key::new(*b"APB", 8);

/// A graveyard's first hit points: `1` in gpl spawn graveyard, then
/// `0x555041`, `APU:0`.
pub const GRAVE_HP: i32 = 1;

/// `SpawnGraveyard`'s arguments with its return slot when a coordinate
/// is given.
const WITH_COORD: usize = 4;

impl World {
    /// `SpawnGraveyard(unit, name, [coord])`: a building of `name` for
    /// the unit's owner, at the coordinate or the unit's exit, searched
    /// with its clearing ring, 1 hit point, `GravesIndex` the level's
    /// next hero list. Unknown where the world cannot place it.
    pub(super) fn spawn_graveyard(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let owner = match self.found(id) {
            Found::Piece(p) => Some(p.owner),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw(format!(
                "Function $SetAttribute( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            )))?,
        };
        let at = if args.len() >= WITH_COORD {
            match vm.object(vm.arg(args, 3)?) {
                Value::Coord(x, y) => Some((*x, *y)),
                _ => None,
            }
        } else {
            self.exit(id)
        };
        let body = self
            .descriptors
            .body_named(&name)
            .map(|(code, b)| (code, b.clone()));
        let Some((code, body)) = body else {
            Err(Fault::throw(format!(
                "SpawnGraveyard() could not make unit of type {name} (could not find description) "
            )))?
        };

        // The owner's dead list moves before the kind is asked.
        // **Inferred**: an owner the world is unsure of moves none.
        let index = i32::try_from(self.limits.graves.len()).unwrap_or(i32::MAX);
        let dead = owner.map(|o| self.players.bury(o)).unwrap_or_default();

        self.limits.graves.push(dead);

        match body.class {
            Class::Character => Err(Fault::throw("SpawnGraveyard() called on a vehicle "))?,
            Class::General | Class::Projectile | Class::Overlay | Class::Particles => {
                Err(Fault::throw(format!(
                    "SpawnGraveyard() could not make unit of type {name} (not a building or vehicle) "
                )))?;
            }
            Class::Building => {}
        }

        let placed = at
            .filter(|_| !self.unplaced())
            .and_then(|a| self.place_ringed(&body, a));

        match (owner, placed) {
            (Some(owner), Some(at)) => {
                let given = [(HP.packed(), GRAVE_HP), (GRAVES_INDEX.packed(), index)];
                let made = self.raise(vm.program(), (code, &body), owner, &given, at)?;

                self.name(made);
                self.dispatches
                    .queue((super::NEW_UNIT.to_owned(), vec![Value::Agent(made)]));
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }
}

impl World {
    /// `GSUnit`'s hero branch on piece `id` as it is deleted: a
    /// hero whose own name is not empty goes on its owner's dead list.
    /// **Inferred**: a name is empty only with no `AP:1`, number or own
    /// name; an `AP:1` that spells nothing is not looked for.
    pub(super) fn count_dead(&mut self, id: u32) {
        let dead = self
            .pieces
            .get(id)
            .filter(|_| self.is_hero(id))
            .and_then(|p| {
                let who = self.who(id)?;
                let named = who.name.is_some() || who.number.is_some() || who.given.is_some();
                let grave = Grave {
                    code: who.code,
                    // `0xb565041` in `GSUnit`.
                    level: p.get(EXPERIENCE_LEVEL).unwrap_or_default(),
                    name: who,
                };

                named.then_some((p.owner, grave))
            });

        if let Some((owner, grave)) = dead.filter(|(o, _)| self.players.contains(*o)) {
            self.players.died(owner, grave);
        }
    }

    /// Player `player`'s dead heroes not yet in a graveyard.
    #[must_use]
    pub fn dead(&self, player: u32) -> &[Grave] {
        self.players.dead(player)
    }

    /// The level's hero lists, by `GravesIndex`.
    #[must_use]
    pub fn graves(&self) -> &[Vec<Grave>] {
        &self.limits.graves
    }

    /// The heroes graveyard `id` holds, by its `GravesIndex`; `None` for
    /// a unit with none (`-1`, `GraveyardController`'s test) or one
    /// past the lists.
    #[must_use]
    pub fn buried(&self, id: u32) -> Option<&[Grave]> {
        let index = self.pieces.get(id)?.get(GRAVES_INDEX)?;

        self.limits
            .graves
            .get(usize::try_from(index).ok()?)
            .map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dead list and a graveyard's list come back from a save as they
    /// went in.
    #[test]
    fn dead_lists_and_graves_survive_a_save() {
        let named = Grave {
            name: Who {
                code: FourCc(*b"AVB1"),
                name: Some(0x0102_0300),
                table: None,
                number: None,
                given: None,
            },
            code: FourCc(*b"AVB1"),
            level: 3,
        };
        let saved = Grave {
            level: 5,
            ..named.clone()
        };
        let mut world = World {
            players: super::super::players::Players::new(vec![0]),
            ..World::default()
        };

        world.players.died(0, named.clone());
        world.limits.graves.push(vec![saved.clone()]);

        let mut back = World::default();

        back.restore(world.saved()).expect("restored");

        assert_eq!(back.dead(0), [named]);
        assert_eq!(back.graves(), [vec![saved]]);
    }
}

/// To and from the save model.
pub(super) mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};

    impl Saved for Grave {
        type Model = m::Grave;

        fn save(&self) -> m::Grave {
            m::Grave {
                name: self.name.save(),
                code: self.code.save(),
                level: self.level,
            }
        }

        fn load(model: m::Grave) -> Result<Grave, Refused> {
            Ok(Grave {
                name: Who::load(model.name)?,
                code: FourCc::load(model.code)?,
                level: model.level,
            })
        }
    }

    impl Saved for Who {
        type Model = m::Who;

        fn save(&self) -> m::Who {
            m::Who {
                code: self.code.save(),
                name: self.name,
                table: self.table,
                number: self.number,
                given: self.given.save(),
            }
        }

        fn load(model: m::Who) -> Result<Who, Refused> {
            Ok(Who {
                code: FourCc::load(model.code)?,
                name: model.name,
                table: model.table,
                number: model.number,
                given: Option::load(model.given)?,
            })
        }
    }
}
