//! Hit points, damage and death. **Binary** unless marked.
//!
//! The engine keeps no combat rule of its own for an attack: to-hit,
//! damage and armour are the scripts' (`make_attack`, `hit`, `damage` in
//! the SDK's `mx_make_attack.gpl`), each roll a `RandomNumber`. What the
//! engine does is the write:
//!
//! ```text
//!  AdjustAttribute(agent, key, n)
//!    unit gone ─> throw; key unregistered ─> throw
//!    GSUnit writes (key, attribute(key) + n)
//!  SetAttribute(agent, key, v)      GSUnit writes (key, v), the same write
//!
//!  GSUnit's write (BuildingRec, VehicleRec, every GS unit)
//!    store it (GenUnit), tell the player's panels
//!    key APU:0 HitPoints and value < 1:
//!      a building with APB:7 0: dropped from the selection
//!      the unit has AE:0 ─> dispatch Unit_Call_Deathscript(agent) at once
//! ```
//!
//! `Unit_Call_Deathscript` runs the agent's `IGDeathScript`: a hero's
//! `gravestone`, a monster's `monster_gravestone`, which make it `Dead`
//! and later delete it (`DeleteGamePiece`, then `UnitDelete`).

use super::flag::REWARD_COST;
use super::{Answer, Found, World, agent_ref, bodies::Class, int_ref, registered};
use crate::data::attrib::Key;
use crate::data::attrib::{HAS_EFFECT_ILLUSIONARY_HERO, HP, MAX_HP, POISON_COUNTER};
use crate::data::fourcc::FourCc;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::piece_state;

/// What the engine dispatches when a unit's hit points are set below 1:
/// the string `"Unit_Call_Deathscript"`, pushed in `GSUnit`.
pub const DEATH_SCRIPT: &str = "Unit_Call_Deathscript";

/// Hit points below this are death: a compare with 1 in `GSUnit`.
pub const ALIVE_FROM: i32 = 1;

/// One credited death, by victim descriptor and attacker's owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoreKill {
    pub victim: FourCc,
    pub owner: u32,
}

impl World {
    /// `GSUnit`: store `value` under `key` on agent `id`'s piece;
    /// hit points below [`ALIVE_FROM`] on a unit with an agent queue
    /// [`DEATH_SCRIPT`]; `APR:1` moves gold instead of a plain store
    /// ([`World::reward_write`]). **Inferred**: every piece with an agent
    /// holds `AE:0`.
    pub(super) fn write(&mut self, id: u32, key: i32, value: i32) {
        self.write_from(id, key, value, None);
    }

    /// An attribute write with the owner that dealt its damage.
    pub(super) fn write_from(&mut self, id: u32, key: i32, value: i32, owner: Option<u32>) {
        let before = self
            .pieces
            .get(id)
            .and_then(|p| p.attributes.get(&hit_points()).copied())
            .unwrap_or_default();
        let victim = self.pieces.code(id);

        if key == REWARD_COST.packed() {
            self.reward_write(id, value);
        } else if let Some(p) = self.pieces.get_mut(id) {
            p.attributes.insert(key, value);
        }

        if self.body_of(id).is_some_and(|b| b.class == Class::Building) {
            self.restyle(id, super::plume::Force::Unforced);
        }

        // A building at no hit points leaves the selection at once,
        // before its death script; not a palace while a Rage runs.
        let building = self.body_of(id).is_some_and(|b| b.class == Class::Building);
        let raging = self
            .pieces
            .get(id)
            .is_some_and(|p| p.word(super::guild::rage::RAGE_COUNT) != 0);

        if key == hit_points() && value < ALIVE_FROM && building && !raging {
            self.output.deselects.push(id);
        }

        if key == hit_points() && value < ALIVE_FROM && self.agents.alive(id) {
            self.dispatches
                .queue((DEATH_SCRIPT.to_owned(), vec![Value::Agent(id)]));
        }

        if key == hit_points() && before >= ALIVE_FROM && value < ALIVE_FROM {
            if let (Some(victim), Some(owner)) = (victim, owner) {
                self.output.lore_kills.push(LoreKill { victim, owner });
            }

            self.poison_sources.remove(&id);
        }
    }

    /// The pieces whose types a player's lore book may discover.
    #[must_use]
    pub fn lore_pieces(&self) -> Vec<(u32, FourCc, u32)> {
        self.pieces
            .iter()
            .map(|(&id, piece)| (id, piece.code, piece.owner))
            .collect()
    }

    /// The owner of an active attack call, including a reflected shield hit.
    pub(super) fn damage_owner(&self, vm: &Vm<'_>, victim: u32) -> Option<u32> {
        let caller = vm.caller()?;
        let attacker = if caller.eq_ignore_ascii_case("damage") {
            vm.active_arg("damage", "attacker")
        } else if caller.eq_ignore_ascii_case("spelldamage") {
            vm.active_arg("spelldamage", "attacker")
        } else if caller.eq_ignore_ascii_case("Damage_Shield_Hit") {
            vm.active_arg("damage", "defender")
        } else {
            None
        };
        let Value::Agent(id) = attacker? else {
            return None;
        };

        (*id != victim)
            .then_some(*id)
            .and_then(|source| self.pieces.owner(source))
    }

    /// The latest poison application owns later poison ticks.
    pub(super) fn poison_set(&mut self, vm: &Vm<'_>, victim: u32, value: i32) {
        let source = vm
            .active_arg("Poison_begin", "thisagent")
            .and_then(|arg| match arg {
                Value::Agent(id) => self.pieces.owner(*id),
                _ => None,
            });

        if value > 0 {
            if let Some(owner) = source {
                self.poison_sources.insert(victim, owner);
            } else {
                self.poison_sources.remove(&victim);
            }
        } else {
            self.poison_sources.remove(&victim);
        }
    }

    /// `AdjustAttribute(agent, key, amount)`, gpl adjust attribute: the arguments
    /// in order, then the two throws `GetAttribute` has; the key's value
    /// (0 when absent) plus the amount, 32 bits, wrapping, written by
    /// [`World::write`]. The integer result is never written: 0.
    ///
    /// Unknown where `GetAttribute` would be: a spawned piece whose list is
    /// not all known, lacking the key.
    pub(super) fn adjust_attribute(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = int_ref(vm, args, 2)?;
        let amount = int_ref(vm, args, 3)?;
        let piece = self.unit(id).ok_or_else(|| {
            Fault::throw(format!(
                "Function $AdjustAttribute( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            ))
        })?;

        if !registered(at) {
            Err(Fault::throw(format!(
                "Function $AdjustAttribute( agent#{}, #{at:x}, {amount} ) unknown attribute",
                id.cast_signed()
            )))?;
        }

        let unsure = self.units.followed(id) && !self.complete(id);

        match (piece.attributes.get(&at).copied(), unsure) {
            (Some(v), _) => {
                self.write_from(id, at, v.wrapping_add(amount), self.damage_owner(vm, id));
                Ok(Answer::Given)
            }
            (None, false) => {
                self.write_from(id, at, amount, self.damage_owner(vm, id));
                Ok(Answer::Given)
            }
            (None, true) => Ok(Answer::Unknown),
        }
    }

    /// `ClearEngineDeathFlags(unit)`: unit state bit `0x10` off, so it
    /// stands again (`GenUnit` under 3); it still shows `Dead`
    /// until an action plays (`Walker::lay`).
    pub(super) fn raise_dead(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Gone => Err(Fault::throw("ClearEngineDeathFlags() unit is null"))?,
            Found::Unsure => Ok(Answer::Unknown),
            Found::Piece(_) => {
                if let Some(place) = self.pieces.place_mut(id) {
                    place.state &= !piece_state::RUINED;
                }

                Ok(Answer::Given)
            }
        }
    }
}

/// What `MagicalAdjustAttribute` adds to an attribute's key for its
/// magical component: `0x64000000` added, index 100 on in
/// the same bank.
pub const MAGIC_PART: i32 = 0x6400_0000;

impl World {
    /// `MagicalAdjustAttribute(agent, key, amount)`, gpl magical adjust attribute: the
    /// arguments in order; no unit throws; the key and its magical
    /// component ([`MAGIC_PART`]) must both be registered, else it throws;
    /// then the component, then the key, each its value plus the amount
    /// through `GSUnit` ([`World::write`]). The integer result is
    /// never written: 0. Unknown as `AdjustAttribute` is.
    pub(super) fn magical_adjust(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let at = int_ref(vm, args, 2)?;
        let amount = int_ref(vm, args, 3)?;
        let magic = at.wrapping_add(MAGIC_PART);
        let piece = self.unit(id).ok_or_else(|| {
            Fault::throw(format!(
                "Function $MagicalAdjustAttribute( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            ))
        })?;
        let (base, part) = (
            piece.attributes.get(&at).copied(),
            piece.attributes.get(&magic).copied(),
        );

        if !registered(at) {
            Err(Fault::throw(format!(
                "Function $MagicalAdjustAttribute( agent#{}, #{at:x}, {amount} ) unknown attribute",
                id.cast_signed()
            )))?;
        }

        if !registered(magic) {
            Err(Fault::throw(format!(
                "Function $MagicalAdjustAttribute( agent#{}, #{at:x}, {amount} ) unknown attribute (magical component)",
                id.cast_signed()
            )))?;
        }

        let unsure = self.units.followed(id) && !self.complete(id);

        if unsure && (base.is_none() || part.is_none()) {
            Ok(Answer::Unknown)
        } else {
            self.write(id, magic, part.unwrap_or(0).wrapping_add(amount));
            self.write(id, at, base.unwrap_or(0).wrapping_add(amount));
            Ok(Answer::Given)
        }
    }
}

/// The level's periodic hit point tick, set
/// from `#PeriodicHPDeltaPeriod`, `#PoisonInterval` and `#HealingInterval`,
/// saved in `GSGL` (level words 2 to 6).
///
/// ```text
///  game frame step, after the clock step, before the orders
///    periodic hit point tick: clock - last >= period?  last = clock
///      count = (count + 1) mod max(poison, heal), or + 1 if both < 1
///      every mobile, then every building gets (count, poison, heal)
///  VehicleRec
///    APV:27 PoisonCounter > 0: dead, it goes to 0; alive, every poison-th
///      count it falls by 1 and so does APU:0, and nothing else
///    alive, Flags Heals, no APV:46: every (heal + APV:33)-th count,
///      APU:0 one nearer APU:1 MaxHP, from above too
///  BuildingRec        the heal alone, Flags bit 9 Heals
/// ```
///
/// **Game**: from the new game's `[1000, 1, 5, 0, 0]`, a tick each 1000
/// ms gives the day1, play3 and play5 saves' last tick and count
/// (**Measured**).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mend {
    /// ms between ticks.
    pub period: u32,
    pub poison: i32,
    pub heal: i32,
    /// The clock at the last tick.
    pub last: u32,
    pub count: i32,
}

impl Mend {
    /// The tick at clock `now`, if due: the new count.
    pub const fn tick(&mut self, now: u32) -> Option<i32> {
        if now.wrapping_sub(self.last) >= self.period {
            let most = if self.poison > self.heal {
                self.poison
            } else {
                self.heal
            };

            self.last = now;
            self.count = if most < 1 {
                self.count + 1
            } else {
                (self.count + 1) % most
            };

            Some(self.count)
        } else {
            None
        }
    }
}

/// The two unit lists the tick walks, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Mobile,
    Building,
}

/// `APV:33`, `HealingRateModifier`: added to the heal interval.
const HEAL_RATE: i32 = Key::new(*b"APV", 33).packed();

impl World {
    /// The frame's periodic tick ([`Mend`]), before its orders. A death it
    /// causes is the engine's dispatch between passes: the log shows it.
    pub(super) fn mend(&mut self) {
        if let Some(count) = self.level.mending.tick(self.now) {
            let (poison, heal) = (self.level.mending.poison, self.level.mending.heal);

            // Mobiles, then buildings; each list by agent id (**Inferred**:
            // the engine's is a hash map's; only deaths show the order).
            let mut ids: Vec<(Kind, u32)> = self
                .pieces
                .iter()
                .filter_map(|(id, p)| match self.descriptors.body(p.code)?.class {
                    Class::Character => Some((Kind::Mobile, *id)),
                    Class::Building => Some((Kind::Building, *id)),
                    Class::Overlay | Class::Particles | Class::General | Class::Projectile => None,
                })
                .collect();

            ids.sort_unstable();

            for (kind, id) in ids {
                self.tick_unit(id, kind, count, (poison, heal));
            }
        }
    }

    /// `method(count, poison, heal)` on agent `id`'s piece.
    fn tick_unit(&mut self, id: u32, kind: Kind, count: i32, (poison, heal): (i32, i32)) {
        let piece = self.pieces.get(id);
        let heals = piece
            .and_then(|p| self.descriptors.body(p.code))
            .is_some_and(|b| b.heals);
        let get = |k: i32| {
            piece
                .and_then(|p| p.attributes.get(&k).copied())
                .unwrap_or_default()
        };
        let (hp, max, venom) = (
            get(hit_points()),
            get(MAX_HP.packed()),
            get(POISON_COUNTER.packed()),
        );
        let (rate, illusion) = (get(HEAL_RATE), get(HAS_EFFECT_ILLUSIONARY_HERO.packed()));
        let building = kind == Kind::Building;

        let poisoned = !building && venom > 0;
        let every = |n: i32| n < 1 || count % n == 0;

        if poisoned && hp < ALIVE_FROM {
            self.write_between(id, POISON_COUNTER.packed(), 0);
            self.poison_sources.remove(&id);
        } else if poisoned && every(poison) {
            let source = self.poison_sources.get(&id).copied();
            self.write_between(id, POISON_COUNTER.packed(), venom.wrapping_sub(1));
            self.write_between_from(id, hit_points(), hp.wrapping_sub(1), source);

            if venom == 1 {
                self.poison_sources.remove(&id);
            }
        } else if hp >= ALIVE_FROM && heals && (building || illusion == 0) && every(heal + rate) {
            let delta = if max < hp.wrapping_add(1) {
                max.wrapping_sub(hp)
            } else {
                1
            };

            if delta != 0 {
                self.write_between(id, hit_points(), hp.wrapping_add(delta));
            }
        }
    }

    /// [`World::write`] from the frame's step: a death goes to
    /// [`World::take_between`], not the builtin queue.
    fn write_between(&mut self, id: u32, key: i32, value: i32) {
        self.write_between_from(id, key, value, None);
    }

    fn write_between_from(&mut self, id: u32, key: i32, value: i32, owner: Option<u32>) {
        let queued = self.dispatches.queued();

        self.write_from(id, key, value, owner);
        self.dispatches.requeue(queued);
    }
}

/// `PoisonCounter`, `APV:27`.
#[must_use]
pub const fn poison_key() -> i32 {
    POISON_COUNTER.packed()
}

#[cfg(test)]
mod lore_tests {
    use super::*;

    #[test]
    fn death_crossing_counts_once() {
        let mut world = World::default();
        world.add_bare_piece(7);
        let piece = world.pieces.get_mut(7).unwrap();
        piece.code = FourCc(*b"BVG1");
        piece.attributes.insert(hit_points(), 1);

        world.write_from(7, hit_points(), 0, Some(0));
        world.write_from(7, hit_points(), -1, Some(0));

        assert_eq!(
            world.take_output().lore_kills,
            vec![LoreKill {
                victim: FourCc(*b"BVG1"),
                owner: 0,
            }]
        );
        assert_eq!(world.take_output().lore_kills.len(), 0);

        world
            .pieces
            .get_mut(7)
            .unwrap()
            .attributes
            .insert(hit_points(), 1);
        world.write(7, hit_points(), 0);

        assert_eq!(world.take_output().lore_kills.len(), 0);
    }

    #[test]
    fn saved_poison_owner_credits_last_tick() {
        let mut world = World::default();
        world.add_bare_piece(7);
        let piece = world.pieces.get_mut(7).unwrap();
        piece.code = FourCc(*b"BVG1");
        piece.attributes.insert(hit_points(), 1);
        piece.attributes.insert(POISON_COUNTER.packed(), 1);
        world.poison_sources.insert(7, 2);

        let saved = world.saved();
        let mut loaded = World::default();
        loaded.restore(saved).unwrap();
        loaded.tick_unit(7, Kind::Mobile, 0, (1, 0));

        assert_eq!(
            loaded.take_output().lore_kills,
            vec![LoreKill {
                victim: FourCc(*b"BVG1"),
                owner: 2,
            }]
        );
        assert!(loaded.poison_sources.is_empty());
    }
}

#[cfg(test)]
mod deselect_tests {
    use super::*;
    use crate::sim::world::ruin::tests::body;

    /// A world with building `ABW1` as agent 7 and hero `AVa1` as 8.
    fn world() -> World {
        let mut world = World::default();

        for (id, code, class) in [
            (7, *b"ABW1", Class::Building),
            (8, *b"AVa1", Class::Character),
        ] {
            world
                .descriptors_mut()
                .add_body(FourCc(code), body(class, 0x30, vec![(0, 0)]));
            world.add_bare_piece(id);
            let piece = world.pieces.get_mut(id).unwrap();
            piece.code = FourCc(code);
            piece.attributes.insert(hit_points(), 1);
        }

        world
    }

    /// **Binary**: `GSUnit` deselects a building whose hit points
    /// it writes below 1, before
    /// `Unit_Call_Deathscript`; a hero stays selected.
    #[test]
    fn a_building_at_no_hit_points_is_deselected() {
        let mut world = world();

        world.write(8, hit_points(), 0);
        world.write(7, hit_points(), 0);

        assert_eq!(world.take_output().deselects, [7]);
    }

    /// **Binary**: the deselect skips a building with `APB:7` set, the
    /// palace's `RageOfKrolmCount` while a Rage runs.
    #[test]
    fn a_raging_palace_at_no_hit_points_stays_selected() {
        let mut world = world();

        world.write(7, super::super::guild::rage::RAGE_COUNT.packed(), 1);
        world.write(7, hit_points(), 0);

        assert_eq!(world.take_output().deselects, Vec::<u32>::new());
    }
}

/// `APU:0` as scripts hold it.
#[must_use]
pub const fn hit_points() -> i32 {
    HP.packed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tick_counts_modulo_the_longer_interval() {
        let mut mend = Mend {
            period: 1000,
            poison: 1,
            heal: 5,
            last: 0,
            count: 0,
        };

        assert_eq!(mend.tick(950), None);
        assert_eq!(mend.tick(1000), Some(1));
        assert_eq!((2..=5).map(|s| mend.tick(s * 1000)).last(), Some(Some(0)));
    }
}
