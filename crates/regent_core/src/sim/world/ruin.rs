//! A building's fall: from its last hit point to rubble, and the rubble
//! gone. **Binary** unless marked.
//!
//! ```text
//!  APU:0 below 1        Unit_Call_Deathscript, nested in the write
//!    building_death     type "Dead", occupants out, construction lists
//!      PerformAction(Become_Rubble)   Crumble on track 2, cProc 0x2000
//!      DeleteAgent      UnitDelete dispatched, AE:0 written 0; the unit
//!                       stands, agentless
//!  Crumble's cue or last frame        the dying is done
//!    the unit is ruined; deselected; APB:2 CurrentStageBuilt 0
//!    GS_OrderProcessor: a 0x4003 order, #RubbleCleanupPeriod ms
//!  Crumble's end        GSUnit: the building's damage set, Dead
//!  the order's end      callback 0x2007: the unit deleted
//! ```
//!
//! The palace's `Palace_Death` does the same. **Game**, the defeat save:
//! the guild and the library stand as ruins, `AE:0` 0, `APB:2` 0, `Dead`
//! at frame 0, each with its order: 60,000 ms from 340,450 and 348,550,
//! the save at 375,500. The tower that fell before them is gone.

use crate::data::attrib::Key;
use crate::sim::world::piece_state;
use std::collections::BTreeMap;

use super::{AGENT_ID, World};
use crate::base::digest::Set;
use crate::data::attrib::{
    CURRENT_EVENT, CURRENT_STAGE_BUILT, MAX_HP, SORCERERS_ABODE_SPELL_START_TIME,
};
use crate::gpl::ops::Value;
use crate::sim::damage::{self, Condition};
use crate::sim::movement::action::Pose;

/// `#RubbleCleanupPeriod` in `globals.gpl`: how long a ruin stands, ms.
/// The engine reads the constant once;
/// **Inferred**: no image changes it.
pub const RUBBLE_MS: u32 = 60_000;

/// The constant [`RUBBLE_MS`] is read from.
pub const RUBBLE_CONSTANT: &str = "#RubbleCleanupPeriod";

/// `APB:11`, `ForceBuildingState`.
const FORCE: i32 = Key::new(*b"APB", 11).packed();

/// A building's `Become_Rubble`: its `Crumble` on track 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fall {
    /// Game ms the `InitAction` was pushed.
    pub start: u32,
    /// Game ms of its peak, the cue or last frame: the ruin.
    peak: u32,
    /// Game ms it ends.
    end: u32,
    /// Whether the peak has run.
    fallen: bool,
}

/// The set a building shows and since when, game ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picked {
    pub set: u16,
    pub since: u32,
}

/// What [`World`] keeps of falls and ruins.
#[derive(Clone, Debug, Default)]
pub struct Ruins {
    /// Buildings falling, by agent.
    falls: BTreeMap<u32, Fall>,
    /// Each ruin's rubble order start, game ms, by agent.
    rubble: BTreeMap<u32, u32>,
    /// Agents `DeleteAgent` took whose units stay.
    orphans: std::collections::BTreeSet<u32>,
    /// Each building's set as last picked.
    looks: BTreeMap<u32, Picked>,
    /// Every collapse, in order: agent and game ms.
    collapses: Vec<(u32, u32)>,
}

impl World {
    /// `PerformAction` of a `cProc` [`super::walking::DYING_DONE`] action on piece
    /// `id`, which has no tracks: a building. Its `InitAction` pushed now
    /// plays `pose`; with no art, nothing falls (**Inferred**).
    pub(super) fn fall(&mut self, id: u32, pose: Option<Pose>) {
        if let Some(pose) = pose {
            let facing = self.facing_word(id);
            let now = self.now;
            let fall = Fall {
                start: now,
                peak: now.wrapping_add(pose.peak_ms(facing)),
                end: now.wrapping_add(pose.init_ms(facing)),
                fallen: false,
            };

            self.ruins.falls.insert(id, fall);
            self.crumble_vents(id);
            self.fall_step();
        }
    }

    /// Falls whose peak has come, rubble whose order ends: in the frame's
    /// orders. **Inferred**: after the units' tracks.
    pub(super) fn fall_step(&mut self) {
        let now = self.now;
        let peaked: Vec<u32> = self
            .ruins
            .falls
            .iter()
            .filter(|(_, f)| !f.fallen && now >= f.peak)
            .map(|(id, _)| *id)
            .collect();

        for id in peaked {
            if let Some(f) = self.ruins.falls.get_mut(&id) {
                f.fallen = true;
            }

            self.collapse(id);
        }

        let ended: Vec<u32> = self
            .ruins
            .falls
            .iter()
            .filter(|(_, f)| f.fallen && now >= f.end)
            .map(|(id, _)| *id)
            .collect();

        self.ruins.falls.retain(|_, f| !f.fallen || now < f.end);

        // `GSUnit` as `Crumble` ends: `Dead`, forced.
        for id in ended {
            self.restyle(id, super::plume::Force::Forced);
        }

        let gone: Vec<u32> = self
            .ruins
            .rubble
            .iter()
            .filter(|(_, start)| now.wrapping_sub(**start) >= RUBBLE_MS)
            .map(|(id, _)| *id)
            .collect();

        for id in gone {
            self.ruins.rubble.remove(&id);
            self.clear_rubble(id);
        }
    }

    /// Unit dying done on building `id`: unit lie down's
    /// ruined bit, relation 1 units gone and place search, as a dying
    /// hero's ([`World::lie_in_place`]): a unit on its pad moves the ruin;
    /// deselected; `CurrentStageBuilt` 0; the rubble order from now.
    pub(super) fn collapse(&mut self, id: u32) {
        // `GS_OrderProcessor` counts a ruin as its rubble order is
        // first made.
        if let Some(owner) = self
            .pieces
            .owner(id)
            .filter(|_| !self.ruins.rubble.contains_key(&id))
        {
            self.count_destroyed(owner);
        }

        if let Some(place) = self.pieces.place_mut(id) {
            place.state |= piece_state::RUINED;
        }

        self.detach(id, None);
        self.lie_in_place(id);
        self.output.deselects.push(id);
        self.write(id, CURRENT_STAGE_BUILT.packed(), 0);
        self.ruins.rubble.insert(id, self.now);
        self.ruins.collapses.push((id, self.now));
    }

    /// Callback `0x2007`: the ruin deleted.
    fn clear_rubble(&mut self, id: u32) {
        self.poison_sources.remove(&id);
        // `GSUnit`'s counts. A building (`GSUnit` method `(0)`,
        // `GSUnit`) takes none, only event `0x2435358`, a panel refresh.
        self.count_deleted(id);
        self.count_slain(id);
        self.inside_go(id);

        self.pieces.remove(id);
        self.units.unfollow(id);
        self.units.take_tracks(id);
        self.ruins.looks.remove(&id);
        self.dispatches.unlist(super::Listed::Unit(id));
    }

    /// Unit dying done, cProc `0x2000`, at the peak of an action
    /// on tracks: a building collapses, as its fall off tracks does;
    /// any other unit lies down.
    pub(super) fn dying_done(&mut self, id: u32) {
        let building = self
            .body_of(id)
            .is_some_and(|b| b.class == super::bodies::Class::Building);

        if building {
            self.collapse(id);
        } else {
            self.lie_down(id);
        }
    }

    /// The units inside piece `id` go with it: `GenUnit` drops
    /// each `INSIDE` member, its holder
    /// cleared, its `UnitDelete` at once. A tax collector asleep in a
    /// building is on no `occupants` list, so `building_death` leaves it in.
    /// **Inferred**: members by agent, as they entered.
    fn inside_go(&mut self, id: u32) {
        let handle = self.pieces.get(id).map_or(0, |p| p.handle);
        let inside: Vec<u32> = self
            .pieces
            .iter()
            .filter(|(_, p)| handle != 0 && p.place.is_some_and(|pl| pl.holder == handle))
            .map(|(i, _)| *i)
            .collect();

        for member in inside {
            if let Some(place) = self.pieces.place_mut(member) {
                place.holder = 0;
                place.state &= !piece_state::HELD;
            }

            // One already queued for deletion ( bit 2) is left to it.
            match (self.dispatches.deleting(member), self.agents.alive(member)) {
                (true, _) => {}
                (false, true) => {
                    self.dispatch(
                        super::UNIT_DELETE,
                        vec![Value::Integer(member.cast_signed())],
                    );
                }
                (false, false) => self.unit_goes(member),
            }
        }
    }

    /// Building `id` deleted by a placement's clearing
    /// ([`super::building_ground`]): its fall and rubble order go with it.
    pub(super) fn sweep(&mut self, id: u32) {
        self.ruins.falls.remove(&id);
        self.ruins.rubble.remove(&id);
        self.clear_rubble(id);
    }

    /// `DeleteAgent` of agent `id`: `AE:0`
    /// written 0; its `UnitDelete` then ends the agent, not the piece.
    pub(super) fn orphan(&mut self, id: u32) {
        if self.pieces.contains(id) {
            self.ruins.orphans.insert(id);
            self.write(id, AGENT_ID.packed(), 0);
        }
    }

    /// Whether [`World::orphan`] took agent `id`'s piece from it, once.
    pub(super) fn orphaned(&mut self, id: u32) -> bool {
        self.ruins.orphans.remove(&id)
    }

    /// Whether building `id` is falling now: its `Crumble` holds track 2.
    pub(super) fn falling(&self, id: u32) -> Option<Fall> {
        self.ruins.falls.get(&id).copied()
    }

    /// Each building's set, picked again where it changed: after every
    /// frame. **Inferred**: once a frame stands for each attribute write.
    pub(super) fn relook(&mut self) {
        let now = self.now;

        // `World::has_inside` for every building: the holders gathered
        // once, not every piece scanned a building.
        let holders: Set<u32> = self
            .pieces
            .values()
            .filter_map(|p| p.place.map(|pl| pl.holder))
            .collect();
        let occupied = |handle: u32| handle != 0 && holders.contains(&handle);
        let picks: Vec<(u32, u16)> = self
            .pieces
            .keys()
            .filter_map(|id| Some((*id, self.building_set_by(*id, &occupied)?)))
            .collect();

        for (id, set) in picks {
            let changed = self.ruins.looks.get(&id).is_none_or(|l| l.set != set);

            if changed {
                self.ruins.looks.insert(id, Picked { set, since: now });
            }
        }
    }

    /// Building `id`'s set as building damage set picks it now;
    /// its `Crumble` while it falls. `None` for any other piece, or one
    /// with no art.
    pub(super) fn building_set(&self, id: u32) -> Option<u16> {
        self.building_set_by(id, &|h| self.has_inside(h))
    }

    /// [`World::building_set`], `occupied` its [`World::has_inside`].
    fn building_set_by(&self, id: u32, occupied: &dyn Fn(u32) -> bool) -> Option<u16> {
        let piece = self.pieces.get(id)?;
        let body = self.descriptors.body(piece.code)?;
        body.poses.as_ref()?;

        if body.class != super::bodies::Class::Building {
            None
        } else if let Some(f) = self.falling(id) {
            (self.now < f.end).then_some(damage::CRUMBLE)
        } else {
            self.damage_pick_by(id, occupied)
                .or_else(|| self.ruins.looks.get(&id).map(|l| l.set))
        }
    }

    /// The set building damage set puts on building `id` by its
    /// condition; `None` where it keeps the one shown.
    pub(super) fn damage_pick(&self, id: u32) -> Option<u16> {
        self.damage_pick_by(id, &|h| self.has_inside(h))
    }

    /// [`World::damage_pick`], `occupied` its [`World::has_inside`].
    fn damage_pick_by(&self, id: u32, occupied: &dyn Fn(u32) -> bool) -> Option<u16> {
        let piece = self.pieces.get(id)?;
        let body = self.descriptors.body(piece.code)?;
        let poses = body.poses.as_ref()?;
        let get = |k: i32| piece.attributes.get(&k).copied().unwrap_or_default();
        let condition = Condition {
            hp: get(super::combat::hit_points()),
            max: get(MAX_HP.packed()),
            built: get(CURRENT_STAGE_BUILT.packed()) != 0,
            base: body.base_hp,
            event: get(CURRENT_EVENT.packed()),
            spell: get(SORCERERS_ABODE_SPELL_START_TIME.packed()) != 0,
            force: get(FORCE),
            occupied: occupied(piece.handle),
        };

        damage::set(&condition, |s| poses.contains_key(&u32::from(s)))
    }

    /// Whether a piece is inside the building of handle `handle`.
    fn has_inside(&self, handle: u32) -> bool {
        handle != 0
            && self
                .pieces
                .values()
                .any(|p| p.place.is_some_and(|pl| pl.holder == handle))
    }

    /// Building `id`'s set and since when, as last picked.
    #[must_use]
    pub fn look(&self, id: u32) -> Option<Picked> {
        self.ruins.looks.get(&id).copied()
    }

    /// When ruin `id` is deleted, game ms.
    #[must_use]
    pub fn rubble_end(&self, id: u32) -> Option<u32> {
        self.ruins.rubble.get(&id).map(|s| s + RUBBLE_MS)
    }

    /// Every building's collapse so far, in order: agent and game ms, the
    /// start of its rubble order.
    #[must_use]
    pub fn collapses(&self) -> &[(u32, u32)] {
        &self.ruins.collapses
    }

    /// Ruins by agent, each with its rubble order's start.
    #[must_use]
    pub const fn rubble(&self) -> &BTreeMap<u32, u32> {
        &self.ruins.rubble
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::{
        Piece, Place,
        bodies::{Body, Class, Extras, Facing},
        places::Motion,
    };
    use super::*;
    use crate::data::fourcc::FourCc;
    use crate::sim::movement::plan::{Blocking, Occupant};
    use crate::sim::placement::Edge;
    use crate::sim::world::kind;

    /// A body of `cells` about its half cell on collision `layer`.
    pub(in crate::sim::world) fn body(class: Class, layer: u32, cells: Vec<(i32, i32)>) -> Body {
        Body {
            name: String::new(),
            class,
            layer,
            cells,
            edge: Edge::Mask,
            exits: vec![],
            facing: Facing::Turns,
            attributes: vec![],
            lists: Extras::Empty,
            spells: vec![],
            heals: false,
            not_hero: false,
            item: false,
            guild: false,
            income: 0,
            gait: None,
            mask: None,
            ring: None,
            lays: super::super::building_ground::Lays::Nothing,
            cost: 0,
            multiplier: 0,
            delay: 0,
            names: None,
            dialog: None,
            tally: None,
            numbered: None,
            poses: None,
            made_in: None,
            slots: 0,
            placing: super::super::bodies::Placing::Placed,
            recorded: None,
            upgrade: None,
            upgraded: None,
            base_hp: 0,
            hit: super::super::missile::Hit::default(),
            launch: super::super::missile::Launch::default(),
            script: None,
            attach: 0,
            stack: 0,
            span: (0, 0),
            overhead: super::super::draw::Overhead::Shown,
            vents: super::super::plume::Vents::default(),
            animated: super::super::animate::Animated::default(),
            first: None,
            particles: None,
            state: 0,
            spread: super::super::missile::Spread::default(),
            sight: super::super::Sight::Everyone,
        }
    }

    /// A piece of `code` standing on half cell `at`.
    pub(in crate::sim::world) fn standing(
        code: [u8; 4],
        handle: u32,
        kind: kind::Kind,
        at: (i32, i32),
    ) -> Piece {
        let fixed = (
            crate::sim::units::from_pos32(at.0 * crate::sim::units::PER_HALF),
            crate::sim::units::from_pos32(at.1 * crate::sim::units::PER_HALF),
        );

        Piece {
            handle,
            code: FourCc(code),
            place: Some(Place {
                kind,
                fixed,
                standing: at,
                holder: 0,
                state: 0,
            }),
            ..Piece::default()
        }
    }

    /// A collapse runs unit lie down on the building, as a dying
    /// hero's peak does: a place search from its half cell, every unit
    /// but itself in the way (place test not self, flags 0), so
    /// a unit on its pad moves the ruin. **Game**: the united winter
    /// game's `Temple_Dauros1` stood at (256, 3168); its ruin is at
    /// (304, 3168). With its pad clear, the ruin keeps its spot, as the
    /// defeat saves' guild and library do.
    #[test]
    fn a_collapse_searches_the_ruins_place() {
        let temple = *b"ABP1";
        let dragon = *b"BVm1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };

        world.descriptors_mut().add_body(
            FourCc(temple),
            body(Class::Building, 0x30, vec![(-1, 0), (0, 0), (1, 0)]),
        );
        world
            .descriptors_mut()
            .add_body(FourCc(dragon), body(Class::Character, 0x20, vec![(0, 0)]));
        world
            .pieces
            .insert(12, standing(temple, 100, kind::Kind::BUILDING, (20, 20)));
        world.units.set_motion(12, Motion::Still);

        world.collapse(12);
        assert_eq!(world.position(12), Some((320, 320)));

        world
            .pieces
            .insert(28, standing(dragon, 101, kind::Kind::VEHICLE, (19, 21)));
        world.units.set_motion(28, Motion::Still);
        world
            .pieces
            .insert(13, standing(temple, 102, kind::Kind::BUILDING, (20, 21)));
        world.units.set_motion(13, Motion::Still);

        world.collapse(13);
        assert_ne!(world.position(13), Some((320, 336)));
        assert!(world.position(13).is_some());
    }

    /// **Binary**: a ruin's delete counts nothing for its owner.
    /// `GSUnit` counts a hero, henchman or monster only on a mobile;
    /// a building (`GSUnit` method `(0)`, `GSUnit`)
    /// sends event `0x2435358` alone. Its ruin was counted at the
    /// collapse.
    #[test]
    fn a_ruins_delete_counts_nothing() {
        let temple = *b"ABP1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };

        world
            .descriptors_mut()
            .add_body(FourCc(temple), body(Class::Building, 0x30, vec![(0, 0)]));
        world
            .pieces
            .insert(12, standing(temple, 100, kind::Kind::BUILDING, (20, 20)));
        world.units.set_motion(12, Motion::Still);

        world.collapse(12);
        let collapsed = world.ledger(0);

        world.clear_rubble(12);

        assert_eq!(collapsed.destroyed, 1);
        assert_eq!(world.ledger(0), collapsed);
        assert!(world.players.slain(0).is_none_or(BTreeMap::is_empty));
    }

    /// A building that acted before it fell has tracks, so its
    /// `Become_Rubble` peaks there; unit dying done collapses it
    /// all the same: ruined, its rubble order started. Without it, an
    /// openregent play's fallen guardhouse stood as a ruin till a
    /// placement cleared it.
    #[test]
    fn a_building_with_tracks_falls_to_rubble() {
        let guardhouse = *b"ABE2";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };

        world.descriptors_mut().add_body(
            FourCc(guardhouse),
            body(Class::Building, 0x30, vec![(0, 0)]),
        );
        world.pieces.insert(
            12,
            standing(guardhouse, 100, kind::Kind::BUILDING, (20, 20)),
        );
        world.units.set_motion(12, Motion::Still);

        world.dying_done(12);

        assert_eq!(world.rubble_end(12), Some(RUBBLE_MS));
        assert!(world.pieces.place(12).is_some_and(|p| p.ruined()));

        // Unit dying done deselects the ruin (`GenUnit` method `(0, 1)`).
        assert_eq!(world.take_output().deselects, [12]);
    }

    /// **Binary**: a deleted unit takes the units inside it.
    /// `GenUnit` frees its relation lists, and
    /// relation member drop clears each type 1 to 99 member's
    /// holder and deletes it at once: `INSIDE`, type 2,
    /// too: a tax collector inside a guardhouse that fell, on no
    /// `occupants` list so not let out, goes with the ruin.
    #[test]
    fn a_ruins_delete_takes_the_units_inside() {
        let guardhouse = *b"ABE2";
        let collector = *b"AVh1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };
        let mut inside = standing(collector, 101, kind::Kind::VEHICLE, (20, 20));

        if let Some(place) = inside.place.as_mut() {
            place.holder = 100;
            place.state |= piece_state::HELD;
        }

        world.descriptors_mut().add_body(
            FourCc(guardhouse),
            body(Class::Building, 0x30, vec![(0, 0)]),
        );
        world.pieces.insert(
            12,
            standing(guardhouse, 100, kind::Kind::BUILDING, (20, 20)),
        );
        world.pieces.insert(51, inside);
        world.agents.insert(51, super::super::Agent::default());
        world.units.set_motion(12, Motion::Still);

        world.collapse(12);
        world.clear_rubble(12);

        assert_eq!(
            world.take_queued(),
            [(
                super::super::UNIT_DELETE.to_owned(),
                vec![crate::gpl::ops::Value::Integer(51)]
            )]
        );
        assert_eq!(world.pieces.place(51).map(|p| p.holder), Some(0));
    }

    /// `GSUnit` calls a building clearable when `BuildingRec`
    /// holds: ruined, or `AE:0` 0, as `DeleteAgent` leaves it. So a
    /// trampler walks through a fallen building before its `Crumble`
    /// peaks. **Game**: the united winter game's ice dragon 28 steps
    /// south-east over `Temple_Dauros1`'s pad at 131,650 ms, 1,500 ms
    /// after its `DeleteAgent` and before its ruin.
    #[test]
    fn an_agentless_building_lets_a_trampler_through() {
        let temple = *b"ABP1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };
        let mut piece = standing(temple, 100, kind::Kind::BUILDING, (20, 20));
        let dragon = Occupant {
            handle: 101,
            info: 0x20,
            state: 0,
            vehicle: true,
            building: false,
            chest: false,
            clearable: false,
            tramples: true,
            dont_block: false,
        };
        let blocks = |world: &mut World| {
            world.regrid();
            world.assert_grid();
            world
                .collision()
                .blocked((20, 20), &dragon, Blocking::Still)
        };

        piece.attributes.insert(AGENT_ID.packed(), 12);
        world
            .descriptors_mut()
            .add_body(FourCc(temple), body(Class::Building, 0x30, vec![(0, 0)]));
        world.pieces.insert(12, piece);
        world.units.set_motion(12, Motion::Still);
        assert!(blocks(&mut world));

        world.orphan(12);
        assert!(!blocks(&mut world));
    }

    /// Cell blocker first asks blocker default with `0x400`
    /// alone, which a ruined unit never answers: a ruin is no walker's
    /// blocker, so a walker meeting a walker beside one waits. **Game**:
    /// the united winter game's ice dragon 30 waits at (256, 3264) from
    /// 133,950 ms, `Temple_Dauros1`'s ruin on its pad rect's first rows.
    #[test]
    fn a_ruin_is_no_walkers_blocker() {
        let temple = *b"ABP1";
        let dragon = *b"BVm1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };

        world
            .descriptors_mut()
            .add_body(FourCc(temple), body(Class::Building, 0x30, vec![(0, 0)]));
        world
            .descriptors_mut()
            .add_body(FourCc(dragon), body(Class::Character, 0x20, vec![(0, 0)]));
        world.descriptors_mut().add_shape(
            FourCc(dragon),
            super::super::Shape {
                rect: [0, 0, 1, 1],
                bounds: [0, 0, 32, 32],
                sight: super::super::Sight::Everyone,
                exposure: super::super::Exposure::Point,
            },
        );
        world
            .pieces
            .insert(12, standing(temple, 100, kind::Kind::BUILDING, (20, 20)));
        world
            .pieces
            .insert(28, standing(dragon, 101, kind::Kind::VEHICLE, (19, 20)));
        world.units.set_motion(12, Motion::Still);
        world.units.set_motion(28, Motion::Still);
        let blocker = |world: &World| {
            let stand = world.standing();

            world.blocker(&stand, World::reach(&stand), 28, (20, 20))
        };

        assert_eq!(blocker(&world), Some(12));

        world.collapse(12);
        assert_eq!(world.position(12), Some((320, 320)));
        assert_eq!(blocker(&world), None);
    }

    /// **Binary**: `GenUnit` is 3 or more for a building that is
    /// ruined or, by `BuildingRec`, agentless; pick kind ok
    /// refuses either. A lying dead mobile is fallen by its ruined bit.
    #[test]
    fn an_agentless_or_ruined_building_is_fallen() {
        let guild = *b"ABW1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };
        let mut piece = standing(guild, 100, kind::Kind::BUILDING, (20, 20));

        piece.attributes.insert(AGENT_ID.packed(), 12);
        world
            .descriptors_mut()
            .add_body(FourCc(guild), body(Class::Building, 0x30, vec![(0, 0)]));
        world.pieces.insert(12, piece);
        world.units.set_motion(12, Motion::Still);
        assert!(!world.fallen(12));

        world.orphan(12);
        assert!(world.fallen(12));

        world.write(12, AGENT_ID.packed(), 12);
        world.collapse(12);
        assert!(world.fallen(12));
    }

    /// **Binary**: recruit done makes its hero only while the guild
    /// stands (`GenUnit` under 3). A guild that fell mid-recruit,
    /// agentless from its `DeleteAgent` or ruined, makes none; the price
    /// stays paid.
    #[test]
    fn a_fallen_guild_recruits_nothing() {
        let guild = *b"ABW1";
        let hero = *b"AVa1";
        let mut world = World {
            board: super::super::board::Board::sized((64, 64)),
            ..World::default()
        };
        let mut piece = standing(guild, 100, kind::Kind::BUILDING, (20, 20));
        let recruit = super::super::recruit::Recruit {
            guild: 12,
            code: FourCc(hero),
        };
        let program = crate::gpl::load::Program::default();

        piece.attributes.insert(AGENT_ID.packed(), 12);
        world
            .descriptors_mut()
            .add_body(FourCc(guild), body(Class::Building, 0x30, vec![(0, 0)]));
        world
            .descriptors_mut()
            .add_body(FourCc(hero), body(Class::Character, 0x20, vec![(0, 0)]));
        world.pieces.insert(12, piece);
        world.units.set_motion(12, Motion::Still);

        world.orphan(12);
        assert_eq!(world.recruit(&program, &recruit).ok(), Some(None));

        world.collapse(12);
        assert_eq!(world.recruit(&program, &recruit).ok(), Some(None));
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Fall {
        type Model = m::Fall;

        fn save(&self) -> m::Fall {
            let Fall {
                start,
                peak,
                end,
                fallen,
            } = *self;

            m::Fall {
                start,
                peak,
                end,
                fallen,
            }
        }

        fn load(model: m::Fall) -> Result<Fall, Refused> {
            Ok(Fall {
                start: model.start,
                peak: model.peak,
                end: model.end,
                fallen: model.fallen,
            })
        }
    }

    impl Saved for Picked {
        type Model = m::Picked;

        fn save(&self) -> m::Picked {
            m::Picked {
                set: self.set,
                since: self.since,
            }
        }

        fn load(model: m::Picked) -> Result<Picked, Refused> {
            Ok(Picked {
                set: model.set,
                since: model.since,
            })
        }
    }

    impl Saved for Ruins {
        type Model = m::Ruins;

        fn save(&self) -> m::Ruins {
            let Ruins {
                falls,
                rubble,
                orphans,
                looks,
                collapses,
            } = self;

            m::Ruins {
                falls: falls.save(),
                rubble: rubble.clone(),
                orphans: orphans.clone(),
                looks: looks.save(),
                collapses: collapses.clone(),
            }
        }

        fn load(model: m::Ruins) -> Result<Ruins, Refused> {
            Ok(Ruins {
                falls: BTreeMap::load(model.falls)?,
                rubble: model.rubble,
                orphans: model.orphans,
                looks: BTreeMap::load(model.looks)?,
                collapses: model.collapses,
            })
        }
    }
}
