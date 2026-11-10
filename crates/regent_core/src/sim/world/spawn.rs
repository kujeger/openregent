//! Pieces the world makes: `SpawnUnit`, and a new mobile as
//! Agent create placed makes it. **Binary** per builtin; what is
//! **Inferred** or **Game** says so.
//!
//! ```text
//!  SpawnUnit(spawner, name) ── exit point: the spawner's art, set 0xd0,
//!     │                         block 2's second point (GenUnit)
//!     │  place search from there (place search, flags 0)
//!     │  new agent, HOME = spawner, NewUnitInit queued for the host
//!     v
//!  Still: its place known, its orders run (walking)
//! ```

use super::lairs::{NONE, OFF_MAP, artifice, pick, total};
use super::{
    AGENT_ID, Answer, Found, Piece, Place, World, agent_ref,
    bodies::{Body, Class},
    pairs,
    places::Motion,
    placing::Blockers,
    spell::Charge,
    string_ref,
};
use crate::data::attrib::{self, CURRENT_STAGE_BUILT, FIRST_STAGE_BUILT, HP};
use crate::data::fourcc::FourCc;
use crate::gpl::ops;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::space;
use crate::sim::units::from_pos32;
use crate::sim::world::kind;
use std::collections::BTreeMap;

/// `SpawnUnit`'s options, gpl spawn unit on.
const OVERRIDE: &str = "override";

/// `clear` ors 8 into the create flags (gpl spawn unit, from 4), which
/// neither agent create placed, agent create placed b nor
/// either placing tests: they read bits 0 to 2. So it places as without.
const CLEAR: &str = "clear";

const MAX_HP: &str = "maxhp";

/// Descriptors `VehicleRec` places with its own test,
/// [`Blockers::Anyone`]: `AVk1` to `AVk3`.
const OWN_TEST: [[u8; 4]; 3] = [*b"AVk1", *b"AVk2", *b"AVk3"];

/// A spawned building's hit points and stages: with
/// `maxhp` its `MaxHP` and both stages built, else 1 hit point.
fn stage(body: &Body, options: &Options) -> impl Iterator<Item = (i32, i32)> + use<> {
    let (hp, max) = (HP.packed(), attrib::MAX_HP.packed());
    let full = body
        .attributes
        .iter()
        .find(|(k, _)| *k == max)
        .map_or(0, |(_, v)| *v);

    let list = if options.max_hp {
        vec![
            (hp, full),
            (FIRST_STAGE_BUILT.packed(), 1),
            (CURRENT_STAGE_BUILT.packed(), 1),
        ]
    } else {
        vec![(hp, super::build::FIRST_HP)]
    };

    list.into_iter()
}

/// `SpawnUnit`'s arguments after the name, by type.
#[derive(Clone, Debug, Default)]
struct Options {
    /// A coordinate: where, 32nds of a cell.
    at: Option<(i32, i32)>,
    /// An integer: the owner.
    owner: Option<u32>,
    /// `maxhp`: a building made whole.
    max_hp: bool,
    /// `override`: use the named descriptor instead of a lair's pick.
    override_lair: bool,
    /// A list's key and value pairs.
    attributes: Vec<(i32, i32)>,
}

impl Options {
    fn read(vm: &Vm<'_>, args: &[Handle]) -> std::result::Result<Options, Fault> {
        let mut options = Options::default();

        for k in 3..args.len() {
            match vm.object(vm.arg(args, k)?) {
                Value::Coord(x, y) => options.at = Some((*x, *y)),
                Value::Integer(n) => options.owner = Some(n.cast_unsigned()),
                Value::String(s) if s.eq_ignore_ascii_case(MAX_HP) => options.max_hp = true,
                // `clear` ors 8 into the create flags, which no placement
                // reads ([`CLEAR`]).
                Value::String(s) if s.eq_ignore_ascii_case(OVERRIDE) => {
                    options.override_lair = true;
                }
                Value::String(s) if s.eq_ignore_ascii_case(CLEAR) => {}
                Value::List(items) if items.len() % 2 == 1 => Err(Fault::throw(
                    "SpawnUnit() attribute list parameter contained an uneven #of entries",
                ))?,
                Value::List(items) => options.attributes.extend(pairs(items)),
                _ => {}
            }
        }

        Ok(options)
    }
}

impl World {
    /// `SpawnUnit(spawner, name, [coord], [owner], [option], [list])`,
    /// gpl spawn unit, for a mobile:
    ///
    /// - the descriptor by `HEAD` name, else a throw;
    /// - a coordinate argument is where, else the spawner's exit;
    /// - agent create placed makes it there and
    ///   `VehicleRec` runs the place search;
    /// - owner the integer argument, else the spawner's; the list's pairs
    ///   are its attributes; its `HOME` is the spawner (relation 100)
    ///   where the spawner's owner is its own;
    /// - the next agent id and handle; `NewUnitInit` runs before it
    ///   returns.
    ///
    /// A building (agent create placed b, place flags 1, margin
    /// `ConstructData1`): the search from there tests its ring, not its pad
    /// ([`World::place_ringed`]); `maxhp` makes it whole, else 1 hit point;
    /// levelled and cleared as a player's; no `HOME`. **Game**: play3's
    /// house lands where its save has it.
    ///
    /// Every spawner has lair data (lair data, a
    /// default on a miss): with entries it picks the unit on a world draw,
    /// else the script's name stands.
    ///
    /// An off-map lair's unit enters at a point on its edge
    /// ([`World::edge_point`]).
    ///
    /// Unknown where a piece the world cannot place could block.
    pub(super) fn spawn(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let ret = vm.arg(args, 0)?;
        let me = agent_ref(vm, args, 1)?;
        let name = string_ref(vm, args, 2)?;
        let spawner = match self.found(me) {
            Found::Piece(p) => Some(p.clone()),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("SpawnUnit() Spawning unit is null"))?,
        };
        let mut options = Options::read(vm, args)?;
        let entries = spawner
            .as_ref()
            .and_then(|p| self.lairs.entries(p.handle))
            .cloned();
        // lair pick: an entry on a world draw, `NONE` with no entries.
        let picked = entries
            .filter(|e| !e.is_empty())
            .and_then(|e| {
                let drawn = self.rngs.draw(total(&e));

                pick(&e, drawn).copied()
            })
            .filter(|entry| entry.code != NONE)
            .filter(|_| !options.override_lair);
        let body = match picked {
            Some(entry) => self
                .descriptors
                .body(entry.code)
                .map(|b| (entry.code, b.clone())),
            None => self
                .descriptors
                .body_named(&name)
                .map(|(code, b)| (code, b.clone())),
        };
        let edge = self.position(me).filter(|(x, _)| OFF_MAP.contains(x));

        // Spawn edge: an off-map lair's unit enters by its edge, a
        // GPL draw along it; a lair that picked nothing throws.
        if let Some((x, _)) = edge {
            if picked.is_none() {
                let lair = spawner.as_ref().map_or(String::new(), |p| p.name.clone());

                Err(Fault::throw(format!(
                    "SpawnUnit() could not find lair data for {lair}"
                )))?;
            }

            options.at = Some(self.edge_point(vm, x));
        }

        let Some((code, body)) = body else {
            Err(Fault::throw(format!(
                "SpawnUnit() could not make unit of type {name} (could not find description) "
            )))?
        };

        if !matches!(body.class, Class::Building | Class::Character) {
            Err(Fault::throw(format!(
                "SpawnUnit() could not make unit of type {name} (not a building or vehicle) "
            )))?;
        }

        let unsure = spawner.is_none() || self.unplaced();
        let at = options.at.or_else(|| self.exit(me));
        let placed = at.and_then(|a| match body.class {
            Class::Building => self.place_ringed(&body, a),
            _ if OWN_TEST.contains(&code.0) => self.place_among(&body, a, Blockers::Anyone),
            _ => self.place(&body, a),
        });

        match (spawner, placed) {
            (Some(spawner), Some(at)) if !unsure => {
                let owner = options.owner.unwrap_or(spawner.owner);
                let id = if body.class == Class::Building {
                    let given: Vec<(i32, i32)> =
                        stage(&body, &options).chain(options.attributes).collect();
                    let id = self.raise(vm.program(), (code, &body), owner, &given, at)?;

                    self.name(id);
                    id
                } else {
                    let given: Vec<(i32, i32)> = options
                        .attributes
                        .iter()
                        .copied()
                        .chain(picked.as_ref().and_then(artifice))
                        .collect();
                    let id = self.stand(vm.program(), (code, &body), owner, &given, Some(at))?;

                    // `SpawnUnit` passes the spawner's handle
                    // only where the spawner's owner is
                    // the new unit's; else 0, and
                    // agent create placed adds no relation 100.
                    // **Game**, the probed play: the
                    // player's sewer's ratman for `team` has no `Parent`.
                    if owner == spawner.owner {
                        self.relations.set_home(self.handle, vec![spawner.handle]);
                    }

                    id
                };

                self.dispatches
                    .queue((super::NEW_UNIT.to_owned(), vec![Value::Agent(id)]));

                ops::assign(vm.object_mut(ret), &Value::Agent(id))?;
                Ok(Answer::Given)
            }
            _ => Ok(Answer::Unknown),
        }
    }

    /// A new mobile and its agent at `at`, 32nds, as
    /// agent create placed makes it: the next agent id and handle,
    /// `VehicleRec`'s attributes then `given`, then its name. The
    /// caller sets its `HOME`. With no `at` its place is unknown.
    /// `GSUnit`, as agent create placed and
    /// agent create placed b make a unit: one world draw over
    /// `slots`, its slot, applied only to a `Directionless` descriptor.
    pub(super) fn slot_draw(&mut self, slots: usize) {
        self.rngs.draw(i32::try_from(slots).unwrap_or(i32::MAX));
    }

    /// The slot count of the set `body` is made in, for
    /// [`World::slot_draw`]; 1 without the art.
    pub(super) fn made_slots(body: &Body) -> usize {
        body.made_in
            .zip(body.poses.as_ref())
            .and_then(|(set, poses)| poses.get(&set))
            .map_or(1, |p| p.frames.len())
    }

    pub(super) fn stand(
        &mut self,
        program: &crate::gpl::load::Program,
        (code, body): (FourCc, &Body),
        owner: u32,
        given: &[(i32, i32)],
        at: Option<(i32, i32)>,
    ) -> std::result::Result<u32, Fault> {
        let id = self.agents.next_id();
        let agent = super::Agent::made(program, &body.name).map_err(|_| {
            Fault::throw(format!(
                "NewAgent() could not successfully initialize object {}",
                body.name
            ))
        })?;
        let mut attributes: BTreeMap<i32, i32> =
            body.attributes.iter().chain(given).copied().collect();

        attributes.insert(AGENT_ID.packed(), id.cast_signed());
        self.agents.advance();
        self.handle += 1;
        self.slot_draw(World::made_slots(body));

        let place = at.map(|at| {
            let fixed = (from_pos32(at.0), from_pos32(at.1));

            Place {
                kind: kind::Kind::VEHICLE,
                fixed,
                standing: (space::half(fixed.0), space::half(fixed.1)),
                holder: 0,
                state: 0,
            }
        });
        let motion = if place.is_some() {
            Motion::Still
        } else {
            Motion::Walking
        };
        let piece = Piece {
            owner,
            attributes,
            handle: self.handle,
            code,
            name: body.name.clone(),
            place,
            spells: Some(
                body.spells
                    .iter()
                    .map(|c| Charge::new(*c, self.now))
                    .collect(),
            ),
        };

        self.agents.insert(id, agent);
        self.pieces.insert(id, piece);
        self.note_items(id);

        // Agent create placed counts an unpriced henchman as bought
        // before `VehicleRec` numbers it. An
        // illusion's agent is no henchman.
        if self.henchman(id) {
            self.tally(owner, code, super::numbered::Tally::Bought);
        }

        // `VehicleRec`'s name, after the list's pairs.
        self.name(id);

        match motion {
            Motion::Walking => self.lose(id, super::loss_log::Why::Made),
            Motion::Still => {
                self.units.set_motion(id, motion);
            }
        }

        self.agents.note_made(id);
        Ok(id)
    }

    /// What a `SpawnUnit` of the unit named `name` gives the piece it
    /// makes, where the world does not place it: its descriptor's
    /// attributes, a building's stage, the list's pairs; a mobile's spell
    /// entries. `None` without the descriptor. The caller sets the owner.
    pub(super) fn spawned(&self, vm: &Vm<'_>, args: &[Handle], name: &str) -> Option<Piece> {
        let (code, body) = self.descriptors.body_named(name)?;
        let options = Options::read(vm, args).ok()?;
        let staged: Vec<(i32, i32)> = match body.class {
            Class::Building => stage(body, &options).collect(),
            _ => Vec::new(),
        };
        let attributes = body
            .attributes
            .iter()
            .copied()
            .chain(staged)
            .chain(options.attributes)
            .collect();
        let spells = (body.class == Class::Character).then(|| {
            body.spells
                .iter()
                .map(|c| Charge::new(*c, self.now))
                .collect()
        });

        Some(Piece {
            attributes,
            code,
            spells,
            ..Piece::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::Key;

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_spawned_spellbook_can_be_found_for_pickup() {
        use crate::content::dataset::{Bytecode, Datasets, Install};
        use crate::data::catalogue::Library;

        let hd = regent_corpus::hd();
        let sdk = hd.join(crate::content::env::SDK_IN_HD);
        let files = Install::new(&hd, Bytecode::Sdk(&sdk))
            .files(Datasets::Base)
            .expect("base files");
        let library = Library::from_files(&files.archives).expect("archives");
        let catalogue = library.catalogue().expect("catalogue");
        let descriptors = crate::sim::setup::descriptors(&catalogue);
        let images: Vec<Vec<u8>> = files
            .programs
            .iter()
            .map(|path| std::fs::read(path).expect("program"))
            .collect();
        let program = crate::gpl::load::Program::load(&images).expect("load");
        let (code, body) = descriptors
            .body_named(super::super::inventory::DROPPED_AS)
            .expect("item body");
        let item = Key::new(
            super::super::players::INVENTORY,
            super::super::inventory::SPELLBOOK,
        )
        .packed();
        let mut world = World::default().with_descriptors(descriptors.clone());

        let id = world
            .stand(&program, (code, body), 0, &[(item, 0)], Some((32, 32)))
            .expect("spawn");
        let handle = world.pieces[&id].handle;

        assert_eq!(
            world.first_item(handle),
            Some(i32::from(super::super::inventory::SPELLBOOK))
        );
        assert_eq!(world.carriers(item), [handle]);
        // As the panels find a dropped item; its title is `view`'s
        // (`tests/inventory.rs`).
        assert!(
            world.pieces[&id]
                .name
                .eq_ignore_ascii_case(super::super::inventory::DROPPED_AS)
        );
    }

    /// A unit made in play takes one world draw, its slot over the slot
    /// count of the set it is made in (`GSUnit`, from both creation
    /// paths); the draw is spent whatever the count.
    #[test]
    fn a_new_unit_draws_its_slot_on_the_world_rng() {
        let mut world = World {
            rngs: super::super::step::Rngs::seeded(0x0b60_9e2c),
            ..World::default()
        };
        let mut want = 0x0b60_9e2c;

        crate::base::rng::draw(&mut want, 8);
        world.slot_draw(8);

        assert_eq!(world.rngs.state(), want);

        crate::base::rng::draw(&mut want, 1);
        world.slot_draw(1);

        assert_eq!(world.rngs.state(), want);
    }
}
