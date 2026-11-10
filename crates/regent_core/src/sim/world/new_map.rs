//! A new game's world with no save: the generated map and the setup's
//! picks, as the engine's world materialise and new game setup make it.
//! **Binary** unless marked.
//!
//! ```text
//!  custom-game dialog   the setup: seven player entries,
//!                       each's gold the setup's
//!  world materialise
//!    player setup             a Player per live entry, APP:0 its gold;
//!                             slot 7 flagged unaligned
//!    landscape place          scatter, handles 1, 2, ..
//!    unit chart place         each building: made (BuildingRec),
//!                             finished (agent animate), placed,
//!                             levelled, cleared (BuildingRec)
//!  the level's runtime data   slot 7 sees every vertex (GSRuntimeData)
//!  new game setup             the tick's words from GPL constants
//!                             (the periodic hit point constants)
//! ```
//!
//! Generation itself is [`crate::sim::mapgen`]'s: this takes its ground and
//! scatter, and the start's units with their places and slots
//! ([`crate::sim::start`]), as given.

use crate::data::attrib::Key;
use crate::sim::freestyle::Setup;
use crate::sim::world::kind;
use crate::sim::world::piece_state;
use std::collections::BTreeMap;

use super::board::Board;
use super::build::FIRST_HP;
use super::building_ground::Stand;
use super::combat::Mend;
use super::players::Players;
use super::reveal::{Explored, Fog};
use super::spell::Charge;
use super::walking::HeightMap;
use super::{
    AGENT_ID, CROWN, FIRST_ID, GOAL_SLOT, NO_AGENT_ID, NO_EVENT, Piece, Place, VICTORY_SLOT, World,
    descriptors::Descriptors,
    scatter::{Drawn, Strewn},
};
use crate::data::attrib::{
    CURRENT_STAGE_BUILT, FIRST_STAGE_BUILT, HP, LAIR_BASE_SPAWN_RATE, LAIR_HISTORY_MOD, MAX_HP,
};
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::sim::clearing;
use crate::sim::freestyle::Play;
use crate::sim::paint;
use crate::sim::placement::Heights;
use crate::sim::space;

/// The players of a one-kingdom freestyle game: the crown and slot 7.
/// Player setup makes a player for each live entry of seven, then
/// slot 7. **Corpus**: `[0, 7]` on 86 saves
/// (`Save::players`); which entries a rival kingdom makes live is not read.
pub const PLAYERS: [u32; 2] = [CROWN, NEUTRAL];

/// Slot 7, no kingdom.
pub const NEUTRAL: u32 = piece_state::UNALIGNED;

/// A player player setup makes and its gold, `APP:0` unless 0:
/// one per live slot entry,
/// slot 7 last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seat {
    pub player: u32,
    pub gold: i32,
}

impl Seat {
    /// A freestyle game of `players` humans: each live slot
    /// ([`Play::humans`]) with the setup's gold, then slot 7 with none.
    /// One player: [`PLAYERS`]. **Binary**: freestyle setup gives
    /// every entry the setup's gold.
    #[must_use]
    pub fn freestyle(setup: &Setup, players: u32) -> Vec<Seat> {
        let gold = setup.gold().cast_signed();
        let human = |slot: i32| Seat {
            player: slot.cast_unsigned(),
            gold,
        };
        let neutral = Seat {
            player: NEUTRAL,
            gold: 0,
        };

        Play::of(players)
            .humans()
            .map(human)
            .chain(std::iter::once(neutral))
            .collect()
    }
}

/// `GSRuntimeData` reveals every vertex to this mask:
/// slot 7's bit, `0x80` in `GSRuntimeData`. **Corpus**: every vertex of
/// every save has it.
pub const NEUTRAL_SEES: u8 = 0x80;

/// A built stage's flag.
const BUILT: i32 = 1;

/// The sight a chart building reveals with as it is made, before its
/// search: none yet.
const UNSIGHTED: i32 = 0;

/// The GPL constants periodic hp constants reads
/// into level,: the tick's period, then the
/// poison and healing intervals. `globals.gpl`: 1000, 1, 5.
pub const MEND_CONSTANTS: [&str; 3] = [
    "#PeriodicHPDeltaPeriod",
    "#PoisonInterval",
    "#HealingInterval",
];

/// The tick a new game starts with: [`MEND_CONSTANTS`] from `program`,
/// clock and count 0. A constant not an integer gives 0.
#[must_use]
pub fn mend(program: &Program) -> Mend {
    let word = |name: &str| match program.constant(name) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    };
    let [period, poison, heal] = MEND_CONSTANTS.map(word);

    Mend {
        period: period.cast_unsigned(),
        poison,
        heal,
        last: 0,
        count: 0,
    }
}

/// Whether a scatter object stands on the collision grid: `Info`
/// `DontBlock` keeps it off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnGrid {
    On,
    Off,
}

/// One scatter object, as the landscape chart placed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scatter {
    pub code: FourCc,
    /// Its half cell.
    pub at: (i32, i32),
    pub grid: OnGrid,
    /// The art slot landscape place drew.
    pub slot: u16,
}

/// What a spawner's `SpawnUnit` makes: its lair data's pick, a world
/// draw, or the script's name.
/// A chart lair's data is its [`Raised::lair`];
/// this marks a picking spawner without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spawns {
    #[default]
    ByName,
    ByPick,
}

/// The record class a chart unit is made as (`GS_Level`,
/// `GS_Level`): by its descriptor's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    /// `BuildingRec`: an agent, levelled and clearing where it stands.
    Building,
    /// `VehicleRec`, a character: an agent. **Inferred**: made as a hired
    /// mobile is, with no name; no save holds one unmoved.
    Mobile,
    /// `TerrainFeatureRec`: no agent, scatter to the grid and the clearing
    /// (**Corpus**: `AE:0` 0 in a save's `GRND`).
    Feature(OnGrid),
}

/// How a unit came to stand where it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sited {
    /// Its place search found a place: a building there is levelled and
    /// clears its ring.
    Searched,
    /// Its search found nothing: neither levelled nor cleared.
    /// **Inferred**; no save has one.
    Stuck,
    /// An off-map lair: made before
    /// the scatter, so its handle is lower; never searched.
    OffMap,
    /// A building `BuildingRec` does not place, its art lacking an
    /// `Inactive` set: on its chart cell, neither levelled
    /// nor cleared nor paved.
    Left,
}

/// A lair's settings, the chart's spawner under the unit's key
/// (`1000 × template placement + unit`): unit chart place and
/// bare placement materialise write each word not 0 as an
/// attribute once the unit is made: `words[2]` `LUC:9`, `words[1]`
/// `LUC:8` (`Lair_BaseSpawnRate`), `words[0]` `APU:1` then `APU:0`,
/// `extra` `LUC:10` (`Lair_History_Mod`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub words: [u32; 4],
    pub extra: u32,
    /// The team's four entries: the lair data the chart fills, which
    /// `SpawnUnit` picks from.
    pub entries: [LairEntry; 4],
    /// The spawner's codes, lair data: what `FillSpecialLairList`
    /// names.
    pub special: Vec<FourCc>,
}

/// One thing a lair's data may pick: a code, its weight
/// (`spawn_level`), and its force, the spawn item's last word (`resource`),
/// which the pick hands `SpawnUnit` for `APV:2` (a save's `GSGL` entry's
/// `rest[3]`). **Corpus**: 1 (`#force_raider`) on the Undead Assault
/// lairs, 3 on its fourth lair's, 0 on the Minotaurs'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LairEntry {
    pub code: FourCc,
    pub weight: u32,
    pub force: u32,
}

impl Settings {
    /// The attribute writes, in the engine's order, words of 0 skipped.
    #[must_use]
    pub fn writes(&self) -> Vec<(i32, i32)> {
        let [hp, rate, dispersion, _] = self.words;

        [
            (Key::new(*b"LUC", 9).packed(), dispersion),
            (LAIR_BASE_SPAWN_RATE.packed(), rate),
            (MAX_HP.packed(), hp),
            (HP.packed(), hp),
            (LAIR_HISTORY_MOD.packed(), self.extra),
        ]
        .into_iter()
        .filter(|(_, v)| *v != 0)
        .map(|(k, v)| (k, v.cast_signed()))
        .collect()
    }
}

/// A unit the start made, where its place search put it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Raised {
    pub code: FourCc,
    /// The chart cell's owner, else 7; `u32::MAX` for a
    /// resource.
    pub owner: u32,
    /// 32nds of a cell: a half cell, as place search leaves it.
    pub at: (i32, i32),
    /// 32nds of a cell where unit chart place made it, before its
    /// search: its chart cell's corner, a mobile's before
    /// its jitter; an off-map lair's edge position.
    pub made_at: (i32, i32),
    /// The art slot `GSUnit` drew on the world RNG.
    pub direction: u16,
    pub spawns: Spawns,
    pub made: Made,
    pub sited: Sited,
    /// A lair's settings, where the chart has a spawner for it.
    pub lair: Option<Settings>,
}

/// Everything a new game's world is made from.
pub struct Founding<'a> {
    /// The generated heights, before any building levels them.
    pub ground: &'a dyn Heights,
    /// The generated paint, before any building paves it.
    pub cells: &'a [paint::Cell],
    /// Every scatter object, in the order placed: handles 1, 2, ...
    pub scatter: &'a [Scatter],
    /// The start's units, in the order made: off-map lairs, then the unit
    /// chart's. The next handles; agents in that order, but features.
    pub units: &'a [Raised],
    /// The custom game's picks: the victory condition.
    pub setup: Setup,
    /// The players made, in slot order ([`Seat::freestyle`]).
    pub seats: &'a [Seat],
    /// The human seats, slots 0 up ([`World::humans`]).
    pub session: u32,
    /// The periodic tick: [`mend`].
    pub mend: Mend,
}

impl World {
    /// A new game's world from `founding`, measured by `descriptors`: its
    /// units made in order, buildings finished, levelled and clearing
    /// their scatter, each revealing about it for its owner; no GPL agent
    /// yet. Each building is keyed by the agent id the unit
    /// pass will give it: the next after `GplAIRoot`'s, in handle order
    /// ([`World::unit_pass`]), and holds it as `AE:0`, as a start save's.
    #[must_use]
    pub fn fresh(founding: &Founding<'_>, descriptors: Descriptors) -> World {
        let side = founding.ground.across().saturating_sub(1);
        let mut explored = Explored::blank(side);

        explored.everything(NEUTRAL_SEES);
        let on_grid = || founding.scatter.iter().filter(|s| s.grid == OnGrid::On);
        let mut players = Players::new(founding.seats.iter().map(|s| s.player).collect())
            .seated(founding.session);

        // Player setup writes no `APP:0` for no gold.
        for seat in founding.seats.iter().filter(|s| s.gold != 0) {
            players.set_gold(seat.player, seat.gold);
        }

        let mut board = Board::generated(founding.ground, founding.cells);

        // Made before any building levels: the generated ground's.
        for s in on_grid() {
            board.lay(
                s.code,
                s.at,
                Stand::on(Some(founding.ground), Stand::of_half(s.at)),
            );
        }

        let mut world = World {
            fog: Fog::new(explored, 0),
            board,
            handle: 0,
            players,
            agents: super::agents::Agents::new(FIRST_ID),
            level: super::Level {
                victory: founding.setup.slots[VICTORY_SLOT].cast_signed(),
                goal: founding.setup.slots[GOAL_SLOT].cast_signed(),
                events: [NO_EVENT.to_owned(), NO_EVENT.to_owned()],
                mending: founding.mend,
                ..super::Level::default()
            },
            descriptors: std::sync::Arc::new(descriptors),
            ..World::default()
        };

        // Player setup's statistics: the gold made and kept.
        for seat in founding.seats {
            world.count_initial(seat.player, seat.gold);
        }

        // The scatter's handles come after the off-map lairs'.
        let lairs = founding
            .units
            .iter()
            .take_while(|u| u.sited == Sited::OffMap)
            .count();
        let (lairs, chart) = founding.units.split_at(lairs);
        let id = lairs
            .iter()
            .fold(FIRST_UNIT_ID, |id, raised| world.make_started(id, raised));

        let scenery = founding
            .scatter
            .iter()
            .filter(|s| world.plays_frames(s.code, s.slot))
            .map(|s| (s.code, s.at, s.grid))
            .collect();

        world.board.set_scenery(scenery);

        // Each made on the generated ground, before any building levels
        // it; off the grid nothing reseats it (**Inferred**: no corpus
        // `GRND` record is `DontBlock`).
        for (s, handle) in founding.scatter.iter().zip(world.handle + 1..) {
            let strewn = Strewn {
                code: s.code,
                handle,
                owner: None,
                stand: Stand::on(Some(founding.ground), Stand::of_half(s.at)),
                standing: s.at,
                drawn: Drawn {
                    set: world.recorded(s.code),
                    drawing: (s.slot, 0),
                },
                grid: s.grid,
            };

            world.board.strew(strewn);
        }

        world.handle += u32::try_from(founding.scatter.len()).unwrap_or(u32::MAX);
        chart
            .iter()
            .fold(id, |id, raised| world.make_started(id, raised));

        world.reveal_started();
        world.select_palace();
        world.fog.mark_seen();
        world.pieces.unbind_all();
        world
    }

    /// One start unit, given agent `id` if it takes one; the next id.
    /// Agents go to buildings and mobiles, in the order made.
    fn make_started(&mut self, id: u32, raised: &Raised) -> u32 {
        match raised.made {
            Made::Building => {
                self.raise_started(id, raised);
                id + 1
            }
            Made::Mobile => {
                self.stand_started(id, raised);
                id + 1
            }
            Made::Feature(grid) => {
                self.feature(raised, grid);
                id
            }
        }
    }

    /// One chart building: `BuildingRec`'s attributes, finished by
    /// agent animate, then `BuildingRec` at its place:
    /// levelled, its ring cleared. The next handle.
    fn raise_started(&mut self, id: u32, raised: &Raised) {
        let Some(body) = self.descriptors.body(raised.code).cloned() else {
            return;
        };
        let mut attributes: BTreeMap<i32, i32> = body.attributes.iter().copied().collect();

        self.vf3_research(raised.code.0, &mut attributes);

        let most = attributes
            .get(&MAX_HP.packed())
            .copied()
            .unwrap_or(FIRST_HP);
        let fixed = (
            crate::sim::units::from_pos32(raised.at.0),
            crate::sim::units::from_pos32(raised.at.1),
        );
        let standing = (space::half(fixed.0), space::half(fixed.1));

        attributes.extend([
            (FIRST_STAGE_BUILT.packed(), BUILT),
            (CURRENT_STAGE_BUILT.packed(), BUILT),
            (HP.packed(), most),
            (AGENT_ID.packed(), id.cast_signed()),
        ]);
        attributes.extend(raised.lair.iter().flat_map(Settings::writes));
        self.handle += 1;

        // Made at its chart cell first, its reveal hook's disc there about
        // its pad alone: no `SightRange` written yet (**Inferred** from the
        // Liche Queen's day 0 save, the temple moved 3.5 cells south).
        if raised.sited != Sited::OffMap {
            let made = (
                crate::sim::units::from_pos32(raised.made_at.0),
                crate::sim::units::from_pos32(raised.made_at.1),
            );

            if let Some((centre, radius)) = self.pad_disc(raised.code, made, UNSIGHTED) {
                let mask = self.players.reveal_mask(raised.owner);

                self.fog.explored_mut().disc(centre, mask, radius);
            }
        }

        if raised.sited == Sited::Searched {
            self.take_ground(&body, standing, clearing::Time::Generating);
        }

        self.units.entry(id).direction = Some(raised.direction);
        self.name_later(id, &body);

        if let Some(lair) = &raised.lair {
            self.lairs.set(
                self.handle,
                lair.entries.to_vec(),
                Some(lair.special.clone()),
            );
        } else if raised.spawns == Spawns::ByPick {
            self.lairs.set(self.handle, Vec::new(), None);
        }

        self.pieces.insert(
            id,
            Piece {
                owner: raised.owner,
                attributes,
                handle: self.handle,
                code: raised.code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::BUILDING,
                    fixed,
                    standing,
                    holder: 0,
                    state: 0,
                }),
                spells: Some(Vec::new()),
            },
        );

        // Counted before it is made; numbered as
        // agent animate reaches method.
        self.tally(raised.owner, raised.code, super::numbered::Tally::Gained);

        if body.names.is_none() {
            self.number(id);
        }
    }

    /// One chart mobile: `VehicleRec`'s attributes, its spells ready,
    /// at the place its search found. The next handle. **Inferred**: as
    /// [`World::stand`] makes one.
    fn stand_started(&mut self, id: u32, raised: &Raised) {
        let Some(body) = self.descriptors.body(raised.code).cloned() else {
            return;
        };
        let mut attributes: BTreeMap<i32, i32> = body.attributes.iter().copied().collect();
        let fixed = (
            crate::sim::units::from_pos32(raised.at.0),
            crate::sim::units::from_pos32(raised.at.1),
        );

        attributes.insert(AGENT_ID.packed(), id.cast_signed());
        self.handle += 1;
        self.units.entry(id).direction = Some(raised.direction);
        self.name_later(id, &body);
        self.tally(raised.owner, raised.code, super::numbered::Tally::Gained);
        // Just placed, so known: a start save's mobile may have walked.
        self.units.set_motion(id, super::places::Motion::Still);
        self.pieces.insert(
            id,
            Piece {
                owner: raised.owner,
                attributes,
                handle: self.handle,
                code: raised.code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::VEHICLE,
                    fixed,
                    standing: (space::half(fixed.0), space::half(fixed.1)),
                    holder: 0,
                    state: 0,
                }),
                spells: Some(body.spells.iter().map(|c| Charge::new(*c, 0)).collect()),
            },
        );

        // `VehicleRec` numbers it after the count, as a building.
        if body.names.is_none() {
            self.number(id);
        }

        // A chart mobile clears as a building does (`VehicleRec`
        // passes flags 1 to `GSUnit`, `VehicleRec`).
        let standing = (space::half(fixed.0), space::half(fixed.1));

        self.clear(&body, standing, clearing::Time::Generating);
    }

    /// One chart terrain feature: the next handle, no agent; clearing
    /// about it, then on the grid as scatter is, where a later ring clears
    /// it (`GSUnit` alone, `GSUnit`).
    fn feature(&mut self, raised: &Raised, grid: OnGrid) {
        let fixed = (
            crate::sim::units::from_pos32(raised.at.0),
            crate::sim::units::from_pos32(raised.at.1),
        );

        self.handle += 1;
        self.tally(raised.owner, raised.code, super::numbered::Tally::Gained);

        let half = (space::half(fixed.0), space::half(fixed.1));

        if self.plays_frames(raised.code, raised.direction) {
            self.board.stage(raised.code, half, grid);
        }

        // `GSUnit` with flags 1 clears about it, before it is on
        // the board: the pass skips the unit itself.
        if let Some(body) = self.descriptors.body(raised.code).cloned() {
            self.clear(&body, half, clearing::Time::Generating);
        }

        let stand = Stand::on(self.board.heights(), fixed);

        let strewn = Strewn {
            code: raised.code,
            handle: self.handle,
            owner: (raised.owner != piece_state::NOBODY).then_some(raised.owner),
            stand,
            standing: half,
            drawn: Drawn {
                set: self.recorded(raised.code),
                drawing: (raised.direction, 0),
            },
            grid,
        };

        self.board.raise(strewn, self.descriptors.bodies());
    }

    /// Piece `id`'s attribute `key` set to `value`, as a scene writes it
    /// ([`crate::sim::scene::Staging`]); nothing without the piece.
    pub fn set_attribute(&mut self, id: u32, key: i32, value: i32) {
        if let Some(piece) = self.pieces.get_mut(id) {
            piece.attributes.insert(key, value);
        }
    }

    /// The set id a new `code` unit's record names ([`Body::recorded`]);
    /// 0 without art.
    ///
    /// [`Body::recorded`]: super::bodies::Body::recorded
    fn recorded(&self, code: FourCc) -> u32 {
        self.descriptors
            .body(code)
            .and_then(|b| b.recorded)
            .unwrap_or_default()
    }

    /// Each building reveals about it as it is placed
    /// (unit reveal hook, [`World::sight`]); the hook keeps no
    /// state, so neither does this.
    fn reveal_started(&mut self) {
        self.sight();
        self.units.unsight_all();
        self.fog.clear_blind();
    }

    /// Every game start first deselects every unit (deselect all,
    /// game view start), so a save's selection is dropped. After
    /// `Palace_Pre_Birth`, new game setup hands the local player's
    /// palace ([`World::player_palace`]) to unit select maybe,
    /// which sets unit state bit 0,
    /// sound off, and opens its panel. No palace, no selection. Neither
    /// sends a message or draws on an RNG. **Game**: the start save's
    /// palace is its one selected piece, and a loaded save starts with
    /// nothing selected.
    ///
    /// Each machine selected its own human's; here every human seat's
    /// ([`World::humans`]) is, so no machine's state differs
    /// ([`World::selection`]). Single play: the crown's alone.
    pub(super) fn select_palace(&mut self) {
        let palaces: Vec<u32> = self.humans().filter_map(|p| self.founded(p)).collect();

        for piece in self.pieces.all_mut() {
            let chosen = palaces.contains(&piece.handle);

            if let Some(place) = piece.place.as_mut() {
                place.state = if chosen {
                    place.state | piece_state::SELECTED
                } else {
                    place.state & !piece_state::SELECTED
                };
            }
        }
    }

    /// What the new game holds before its setup runs, to compare two.
    #[must_use]
    pub fn opening(&self) -> Opening<'_> {
        Opening {
            // A start save's scatter holds `AE:0` 0: the unit pass skips it.
            pieces: self
                .pieces
                .unbound()
                .iter()
                .filter(|(id, _)| **id != NO_AGENT_ID)
                .map(|(id, p)| (*id, p))
                .collect(),
            directions: self
                .units
                .iter()
                .filter(|(id, _)| *id != NO_AGENT_ID)
                .filter_map(|(id, u)| u.direction.map(|d| (id, d)))
                .collect(),
            scatter: self.board.scatter(),
            stands: self.board.stands(),
            heights: self.board.ground(),
            explored: self.fog.explored().flags().collect(),
            handle: self.handle,
            homes: self.relations.homes().collect(),
            gold: self.players.treasuries(),
            grid: self.board.side(),
            players: self.players.slots(),
            victory: self.level.victory,
            goal: self.level.goal,
            seen: self.fog.seen(),
            teams: self.players.teams(),
            events: &self.level.events,
            picking: self.lairs.picking(),
            special: self.lairs.specials(),
            mending: self.level.mending,
            next_id: self.agents.next_id(),
            numbers: self
                .units
                .iter()
                .filter_map(|(id, u)| u.number.map(|n| (id, n)))
                .collect(),
            tallies: self.players.tallies(),
        }
    }
}

/// A new game's world before its setup: [`World::opening`].
#[derive(Debug, PartialEq, Eq)]
pub struct Opening<'a> {
    /// Pieces by the agent the unit pass will give them.
    pub pieces: BTreeMap<u32, &'a Piece>,
    pub directions: BTreeMap<u32, u16>,
    pub scatter: &'a [(FourCc, (i32, i32))],
    /// Where each of [`Opening::scatter`] stands, and how high.
    pub stands: &'a [Stand],
    pub heights: Option<&'a HeightMap>,
    pub explored: Vec<u8>,
    /// The last handle issued.
    pub handle: u32,
    pub homes: BTreeMap<u32, &'a [u32]>,
    pub gold: BTreeMap<u32, i32>,
    pub grid: (i32, i32),
    pub players: &'a [u32],
    pub victory: i32,
    pub goal: i32,
    pub seen: u8,
    pub teams: [u32; super::TEAMS],
    pub events: &'a [String; 2],
    pub picking: &'a BTreeMap<u32, Vec<LairEntry>>,
    /// Each spawner's special list, by handle: [`Settings::special`].
    pub special: &'a BTreeMap<u32, Vec<FourCc>>,
    pub mending: Mend,
    pub next_id: u32,
    /// Each numbered building's number, by agent ([`World::numbered`]).
    pub numbers: BTreeMap<u32, i32>,
    /// Every player's counts ([`World::tallies`]).
    pub tallies: super::numbered::Tallies,
}

/// The unit pass's first id: after `InitializeGpl`'s `GplAIRoot`.
pub const FIRST_UNIT_ID: u32 = FIRST_ID + 1;

#[cfg(test)]
mod tests {
    use super::*;

    /// A game of n players seats slots 0 to n - 1 with the setup's gold,
    /// then slot 7 with none; one player, [`PLAYERS`].
    #[test]
    fn a_freestyle_game_seats_each_player_then_slot_7() {
        let mut setup = Setup {
            slots: [0; Setup::LEN],
        };

        setup.slots[6] = 20_000;

        let players = |n: u32| -> Vec<u32> {
            Seat::freestyle(&setup, n)
                .iter()
                .map(|s| s.player)
                .collect()
        };
        let three = Seat::freestyle(&setup, 3);

        assert_eq!(players(1), PLAYERS);
        assert_eq!(players(3), [0, 1, 2, NEUTRAL]);
        assert!(three[..3].iter().all(|s| s.gold == 20_000));
        assert_eq!(three[3].gold, 0);
    }
}
