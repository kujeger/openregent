//! A scene: a new game's world set up by hand, for tests, shots and
//! manual checks. A map, units placed where asked with owners and
//! attributes, and the clock.
//!
//! ```text
//!  Scene (builder, or a file)
//!    map: Flat ──> Flat ground here ─────┐
//!         Freestyle ──> sim::mapgen ─────┤ the start's units, kept or not,
//!                                        │ then each placed unit, sited
//!                                        v
//!    setup::fresh ──> World::fresh: made as a chart's units are
//!    Staging::run ──> Game::setup_from: unit pass, NewUnitInit,
//!                     Palace_Pre_Birth at the scene's clock;
//!                     then each placed unit's attributes written
//! ```
//!
//! A placed unit is a start unit (`new_map::Raised`) made as the unit chart
//! makes one, by its descriptor's class: a building finished, levelled
//! and clearing its ring; a character a mobile standing still; a
//! `General` unit a terrain feature. It stands exactly where asked: no
//! place search. **Inferred**: no engine path places a unit by hand; this
//! is the chart's, minus the search.
//!
//! The file is TOML, one key per builder method:
//!
//! ```toml
//! version = 1
//! map = { flat = { side = 128 } }
//!
//! [[unit]]
//! code = "ABJ1"        # Palace1
//! at = [64, 64]        # cells, x and y
//!
//! [[unit]]
//! code = "AVL1"        # Warrior
//! at = [60.5, 64]
//! attributes = { HP = 20 }
//! items = [11, "HealingPotions1"]   # INV:11, a mod's item by its id
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;

use crate::base::fixed::Fixed;
use crate::data::attrib;
use crate::data::catalogue::Catalogue;
use crate::data::fourcc::{FourCc, string_id};
use crate::gpl::load::Program;
use crate::gpl::runtime::MS_PER_FRAME;
use crate::sim::freestyle::Setup;
use crate::sim::freestyle::{self, Choice, Picks, Play, Victory};
use crate::sim::game::{Game, Stop};
use crate::sim::movement::DIRECTIONS;
use crate::sim::paint::Cell;
use crate::sim::placement::Heights;
use crate::sim::setup::{self, NO_EVENTS};
use crate::sim::units::PER_CELL;
use crate::sim::world::{
    CROWN, World,
    bodies::{Bodies, Class},
    new_map::{Made, NEUTRAL, Raised, Seat, Sited, Spawns},
    players::INVENTORY,
};

/// The file format's version.
pub const VERSION: u32 = 1;

/// A game day, ms: `view::panel::day::DAY_MS`, the day counter's.
pub const DAY_MS: u32 = 60_000;

const MS_PER_SECOND: u32 = 1000;

/// A flat map's terrain class: `GR01 Plains_parent` (`docs/formats/terrain.md`).
pub const PLAINS: u8 = 1;

/// The map sides a freestyle game offers, cells.
pub const SIDES: [u32; 3] = [128, 256, 512];

/// A flat map's height everywhere.
const FLAT: Fixed = Fixed::ZERO;

/// Where a scene's codes stand in for a freestyle pick it leaves out.
const NONE: FourCc = FourCc::new(b"NONE");

/// A new game's world, set up by hand.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    /// [`VERSION`].
    pub version: u32,
    pub map: Ground,
    /// The generated start's units: kept, or none. A flat map has none.
    #[serde(default)]
    pub chart: Chart,
    /// Human seats, slots 0 up.
    #[serde(default = "one")]
    pub players: u32,
    /// Each seat's treasury.
    #[serde(default = "gold")]
    pub gold: u32,
    #[serde(default = "survive")]
    pub victory: Victory,
    /// Gold to pass or days to last; by default the dialog's.
    pub goal: Option<u32>,
    /// The day the game starts on, as the day counter numbers it: 0 first.
    #[serde(default)]
    pub day: u32,
    /// Seconds into [`Scene::day`].
    #[serde(default)]
    pub second: u32,
    /// Placed in this order, after the chart's.
    #[serde(default, rename = "unit")]
    pub units: Vec<Placed>,
}

const fn one() -> u32 {
    freestyle::ONE_PLAYER
}

const fn gold() -> u32 {
    freestyle::GOLD
}

/// Survive: no foe needed to keep a scene going.
const fn survive() -> Victory {
    Victory::Survive
}

/// The ground a scene stands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Ground {
    /// `side` cells a side, height 0, all [`PLAINS`], no scatter: made
    /// here.
    Flat { side: u32 },
    /// The custom-game dialog's picks, [`crate::sim::mapgen`]'s map: each `None`
    /// its default, as `openregent`'s options take them.
    Freestyle(Picks),
}

/// What a scene keeps of the generated start's units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Chart {
    #[default]
    Kept,
    Cleared,
}

/// One unit a scene places.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Placed {
    /// Its descriptor, a `DUNT` id.
    #[serde(deserialize_with = "freestyle::code")]
    pub code: FourCc,
    /// Cells, x and y: where it stands, to a 32nd.
    pub at: [f32; 2],
    #[serde(default)]
    pub owner: u32,
    /// The art slot: a mobile's direction of [`DIRECTIONS`], else its
    /// made set's pose.
    #[serde(default)]
    pub facing: u16,
    /// By the engine's names (`data::attrib`), written once the setup ran.
    #[serde(default)]
    pub attributes: BTreeMap<String, i32>,
    /// Carried, given once the setup ran as `CreateNewInventoryItem`
    /// gives one, its checks left out.
    #[serde(default)]
    pub items: Vec<Item>,
}

/// An item a placed unit carries, as a script's item argument reads
/// (`sim::world::inventory`): a number under 256 the slot `INV:n`, a
/// text its id ([`string_id`]), so a mod's `"HealingPotions1"`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Item {
    Slot(u8),
    Named(String),
}

impl Item {
    /// Its key: what the unit's attributes and the level's register hold.
    #[must_use]
    pub fn key(&self) -> i32 {
        match self {
            Item::Slot(n) => crate::data::attrib::Key::new(INVENTORY, *n).packed(),
            Item::Named(name) => string_id(name),
        }
    }
}

/// Why a scene cannot be made.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Version(u32),
    Side(u32),
    /// No descriptor has the code.
    Unknown(FourCc),
    /// Its class is none a chart makes: an overlay, particles, a missile.
    Unplaceable(FourCc),
    /// Off the map.
    Off(FourCc, [f32; 2]),
    /// No seat nor slot 7.
    Owner(FourCc, u32),
    Facing(FourCc, u16),
    Attribute(FourCc, String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Version(v) => write!(f, "scene version {v}, not {VERSION}"),
            Error::Side(s) => write!(f, "a flat map's side is one of {SIDES:?}, not {s}"),
            Error::Unknown(c) => write!(f, "{c}: no such descriptor"),
            Error::Unplaceable(c) => write!(f, "{c}: not a building, character or terrain"),
            Error::Off(c, at) => write!(f, "{c}: {at:?} is off the map"),
            Error::Owner(c, o) => write!(f, "{c}: owner {o} has no seat"),
            Error::Facing(c, n) => write!(f, "{c}: facing {n} past its art's slots"),
            Error::Attribute(c, a) => write!(f, "{c}: no attribute {a}"),
        }
    }
}

impl std::error::Error for Error {}

impl Scene {
    /// A scene on `map`, nothing placed: one seat with the dialog's
    /// gold, Survive, the clock at 0.
    #[must_use]
    pub const fn new(map: Ground) -> Scene {
        Scene {
            version: VERSION,
            map,
            chart: Chart::Kept,
            players: one(),
            gold: gold(),
            victory: survive(),
            goal: None,
            day: 0,
            second: 0,
            units: Vec::new(),
        }
    }

    /// A scene on a flat map of `side` cells.
    #[must_use]
    pub const fn flat(side: u32) -> Scene {
        Scene::new(Ground::Flat { side })
    }

    #[must_use]
    pub const fn players(mut self, players: u32) -> Scene {
        self.players = players;
        self
    }

    #[must_use]
    pub const fn gold(mut self, gold: u32) -> Scene {
        self.gold = gold;
        self
    }

    #[must_use]
    pub const fn day(mut self, day: u32) -> Scene {
        self.day = day;
        self
    }

    #[must_use]
    pub const fn second(mut self, second: u32) -> Scene {
        self.second = second;
        self
    }

    #[must_use]
    pub fn unit(mut self, placed: Placed) -> Scene {
        self.units.push(placed);
        self
    }

    /// The clock the setup runs at, ms: a whole frame's.
    #[must_use]
    pub const fn clock(&self) -> u32 {
        let ms = self
            .day
            .wrapping_mul(DAY_MS)
            .wrapping_add(self.second.wrapping_mul(MS_PER_SECOND));

        ms - ms % MS_PER_FRAME
    }

    /// The setup a flat map's game holds: its gold and victory, the
    /// freestyle tileset, every pick `NONE`. A freestyle map's is
    /// [`crate::sim::mapgen`]'s, then [`Scene::aimed`].
    #[must_use]
    pub fn setup(&self) -> Setup {
        let none = Choice {
            settlement: NONE,
            scenario: NONE,
            monsters: NONE,
            land: NONE,
            seed: 0,
            side: 0,
        };

        self.aimed(freestyle::setup_of(&none, self.gold))
    }

    /// `setup` with the scene's victory and goal, the dialog's goal where
    /// none is given.
    #[must_use]
    pub fn aimed(&self, setup: Setup) -> Setup {
        let goal = match self.goal {
            Some(goal) => goal,
            None => self.victory.goal(self.gold),
        };

        freestyle::with_victory(setup, self.victory, goal)
    }

    /// The players made: each seat with the scene's gold, then slot 7.
    #[must_use]
    pub fn seats(&self) -> Vec<Seat> {
        Seat::freestyle(&self.setup(), self.players)
    }

    /// The human seats ([`World::humans`]).
    #[must_use]
    pub fn session(&self) -> u32 {
        Play::of(self.players).session()
    }

    /// The start's units on a map of `side` cells: `chart`'s unless
    /// [`Chart::Cleared`], then each placed unit, measured by `bodies`.
    ///
    /// # Errors
    ///
    /// A placed unit the world cannot make or place.
    pub fn start_units(
        &self,
        chart: &[Raised],
        side: u32,
        bodies: &Bodies,
    ) -> Result<Vec<Raised>, Error> {
        let kept = match self.chart {
            Chart::Kept => chart,
            Chart::Cleared => &[],
        };
        let seats = self.seats();
        let placed = self.units.iter().map(|p| p.raised(side, &seats, bodies));

        kept.iter().cloned().map(Ok).chain(placed).collect()
    }

    /// The flat map's world, made by `catalogue`'s descriptors, its tick
    /// from `program` ([`setup::fresh`]).
    ///
    /// # Errors
    ///
    /// Not a flat map, or one of the wrong side, or a placed unit the
    /// world cannot make.
    pub fn flat_world(&self, catalogue: &Catalogue<'_>, program: &Program) -> Result<World, Error> {
        let side = match self.map {
            Ground::Flat { side } if SIDES.contains(&side) => side,
            Ground::Flat { side } => Err(Error::Side(side))?,
            Ground::Freestyle(_) => Err(Error::Side(0))?,
        };

        self.check()?;

        let ground = Flat::new(side);
        let bodies = setup::descriptors(catalogue);
        let units = self.start_units(&[], side, bodies.bodies())?;
        let seats = self.seats();
        let start = setup::Start {
            ground: &ground,
            cells: ground.cells(),
            scatter: &[],
            slots: &[],
            units: &units,
            setup: self.setup(),
            seats: &seats,
            session: self.session(),
            events: [NO_EVENTS; 2],
        };

        Ok(setup::fresh(catalogue, program, &start))
    }

    /// Whether the scene's version is this reader's.
    ///
    /// # Errors
    ///
    /// Another version.
    pub const fn check(&self) -> Result<(), Error> {
        if self.version == VERSION {
            Ok(())
        } else {
            Err(Error::Version(self.version))
        }
    }

    /// What the scene sets once the setup has run on `world`, the world
    /// it made: the placed units are its last handles, one each.
    ///
    /// # Errors
    ///
    /// An attribute name the engine does not register.
    pub fn staging(&self, world: &World) -> Result<Staging, Error> {
        let last = world.opening().handle;
        let count = u32::try_from(self.units.len()).unwrap_or(u32::MAX);
        let first = last.saturating_sub(count) + 1;
        let mut writes = Vec::new();
        let items = self
            .units
            .iter()
            .zip(first..)
            .flat_map(|(placed, handle)| placed.items.iter().map(move |i| (handle, i.key())))
            .collect();

        for (placed, handle) in self.units.iter().zip(first..) {
            for (name, value) in &placed.attributes {
                let key = attrib::key_of(name)
                    .ok_or_else(|| Error::Attribute(placed.code, name.clone()))?;

                writes.push((handle, key.packed(), *value));
            }
        }

        Ok(Staging {
            clock: self.clock(),
            writes,
            items,
        })
    }
}

impl Placed {
    /// `code` at cell `at`, the crown's, facing slot 0, as made.
    #[must_use]
    pub const fn new(code: FourCc, at: [f32; 2]) -> Placed {
        Placed {
            code,
            at,
            owner: CROWN,
            facing: 0,
            attributes: BTreeMap::new(),
            items: Vec::new(),
        }
    }

    #[must_use]
    pub const fn owner(mut self, owner: u32) -> Placed {
        self.owner = owner;
        self
    }

    #[must_use]
    pub const fn facing(mut self, facing: u16) -> Placed {
        self.facing = facing;
        self
    }

    /// Attribute `name` (`data::attrib`) set to `value` after the setup.
    #[must_use]
    pub fn set(mut self, name: &str, value: i32) -> Placed {
        self.attributes.insert(name.to_owned(), value);
        self
    }

    /// `item` carried after the setup.
    #[must_use]
    pub fn carry(mut self, item: Item) -> Placed {
        self.items.push(item);
        self
    }

    /// Where it stands, 32nds of a cell.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "32 a cell, on a map of 512 cells"
    )]
    pub fn pos32(&self) -> (i32, i32) {
        let to = |c: f32| (c * PER_CELL as f32).round() as i32;

        (to(self.at[0]), to(self.at[1]))
    }

    /// As the chart's start makes a unit, on a map of `side` cells with
    /// `seats`, measured by `bodies`: standing where asked, levelled and
    /// clearing there.
    fn raised(&self, side: u32, seats: &[Seat], bodies: &Bodies) -> Result<Raised, Error> {
        let code = self.code;
        let body = bodies.get(&code).ok_or(Error::Unknown(code))?;
        let (made, slots) = match body.class {
            Class::Building => (Made::Building, body.slots),
            Class::Character => (Made::Mobile, u16::try_from(DIRECTIONS).unwrap_or(u16::MAX)),
            Class::General => (Made::Feature(body.grid()), body.slots),
            _ => Err(Error::Unplaceable(code))?,
        };
        let at = self.pos32();
        let edge = i32::try_from(side).unwrap_or(i32::MAX) * PER_CELL;
        let on = |c: i32| (0..edge).contains(&c);
        let seated = seats.iter().any(|s| s.player == self.owner) || self.owner == NEUTRAL;

        if !(on(at.0) && on(at.1)) {
            Err(Error::Off(code, self.at))?;
        }

        if !seated {
            Err(Error::Owner(code, self.owner))?;
        }

        if self.facing >= slots.max(1) {
            Err(Error::Facing(code, self.facing))?;
        }

        Ok(Raised {
            code,
            owner: self.owner,
            at,
            made_at: at,
            direction: self.facing,
            spawns: Spawns::ByName,
            made,
            sited: Sited::Searched,
            lair: None,
        })
    }
}

/// What a scene sets around the setup: its clock, and each placed
/// unit's attributes after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Staging {
    /// ms.
    pub clock: u32,
    /// Handle, packed key, value.
    pub writes: Vec<(u32, i32, i32)>,
    /// Handle, item key ([`Item::key`]).
    pub items: Vec<(u32, i32)>,
}

impl Staging {
    /// `game`'s setup, `quest` its script, at the clock, then the
    /// attributes written. **Inferred**: a write lands as a script's
    /// would, no rule run on it.
    ///
    /// # Errors
    ///
    /// As [`Game::setup`].
    pub fn run(&self, game: &mut Game<'_>, quest: &str) -> Result<(), Stop> {
        game.setup_from(quest, self.clock)?;

        let world = game.host_mut().core_mut().world_mut();

        for &(handle, key, value) in &self.writes {
            if let Some(id) = world.by_handle(handle) {
                world.set_attribute(id, key, value);
            }
        }

        for &(handle, item) in &self.items {
            if let Some(id) = world.by_handle(handle) {
                world.give_item_to(id, item);
            }
        }

        Ok(())
    }
}

/// A flat map: height 0, all [`PLAINS`].
pub struct Flat {
    across: u32,
    cells: Vec<Cell>,
}

impl Flat {
    /// `side` cells a side.
    #[must_use]
    pub fn new(side: u32) -> Flat {
        Flat {
            across: side + 1,
            cells: vec![Cell::uniform(PLAINS); (side * side) as usize],
        }
    }

    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }
}

impl Heights for Flat {
    fn across(&self) -> u32 {
        self.across
    }

    fn height(&self, x: u32, y: u32) -> Option<Fixed> {
        (x < self.across && y < self.across).then_some(FLAT)
    }
}
