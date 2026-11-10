//! The game world GPL's builtins act on: GPL agents, the game pieces they
//! drive, owners, engine attributes, and the rules the engine runs in
//! play.
//!
//! [`World::run`] answers a world builtin by its [`Checked`], found by
//! [`crate::gpl::builtins::BuiltinId`]; [`World::step`] runs a frame's
//! orders. A game ([`crate::sim::game`]) makes its agents here: the unit
//! pass, each agent's attributes as made. Its live host
//! ([`crate::sim::host`]) answers every call from here.
//!
//! ```text
//!  new game ── save's BLDG/VHCL/GRND: owner, attributes, AE:0 agent id,
//!     │        kind, position, holder; explored ground per player
//!     │   shapes ── pad rect and sight per descriptor, from the art
//!     v
//!  World ◄── engine events in the trace: NewUnitInit(agent) announces a
//!     │      piece, UnitDelete(agent) ends one, SpawnUnit names the owner
//!     │  bodies ── placing, blocking and walking per descriptor:
//!     │            SpawnUnit makes pieces (`spawn`), a player's order a
//!     │            building (`build`), a recruit a hero (`recruit`), a
//!     │            reward a flag (`flag`); Move, Hide and kin give orders,
//!     │            each frame runs them (`walking`)
//!     v
//!  world builtin ──> native answer
//!                └─> unknown: a place, sight or attribute it cannot
//!                    know, as a mobile's; play falls back
//!                    (`sim::host::Fallback`)
//! ```
//!
//! **Binary** per builtin, from its native function; what is **Inferred** says so.
//!
//! # Shape
//!
//! The world owns its subsystems; each keeps its fields private to the
//! module that runs it and lends the rest a small API. The world's own
//! fields are open to every rule module under it: a rule reaches a store
//! through that API (`pieces.owner(id)`), not around it. A group of them
//! (`Level`, `Effects`, `Pending`, `Outbox`) is a field of stores, each
//! its own module's.
//!
//! The world holds no per-machine state, so two peers' worlds are equal
//! by construction: the local player and the track a music callback
//! chose are the front end's. [`World::saved`] destructures every field;
//! each one a save leaves out says why beside it: derived, per-frame, or
//! read-only tables.
//!
//! ```text
//!  World
//!   ├─ agents: Agents        GPL agents by id and name, the next id, made
//!   │                        since taken; re-reads, removals for the host
//!   ├─ units: Units          one record per unit beside its piece: orders
//!   │                        followed, order tracks, sight state, draw
//!   │                        state, name and number; the order units
//!   │                        entered buildings; gone in one call
//!   ├─ relations: Relations  HOME lists and reward flags' rosters
//!   ├─ naming: Naming        name generators; start pieces to name
//!   ├─ players: Players      one record per player (GE_Player, GS_Player):
//!   │                        treasury, sharing, hall, palace, vice, ledger,
//!   │                        slain, counts, end; the team masks; the
//!   │                        human seats (World::humans)
//!   ├─ level: Level          GS_Level's:
//!   │    ├─ victory, goal,   the setup's words, the special events
//!   │    │  events
//!   │    ├─ mending          combat::Mend       the periodic hp tick
//!   │    └─ items            inventory::Register  carriers by item key
//!   ├─ effectors: Effectors  every effector, where it hangs, which hung
//!   ├─ fog: Fog              explored ground and what reveals it
//!   ├─ lairs: Lairs          each spawner's picks and held-back codes
//!   ├─ ruins: Ruins          buildings falling, ruins, the sets shown
//!   ├─ limits: Limits        disabled types, building caps, hero lists
//!   ├─ advisors: Seats       each human seat's advisor threads
//!   ├─ cheats: cheat::Flags  cheats allowed; the two flags, by player
//!   ├─ scripts: Scripts      halt, log, breaks
//!   ├─ rngs: step::Rngs      the world RNG; the GPL one lent for a call
//!   ├─ dispatches:           list ORDR; engine dispatches for the host:
//!   │    frame::Dispatches   a builtin's, the step's, deletes to come
//!   ├─ pending: Pending      what waits for its frame or time:
//!   │    ├─ research         research::Researching
//!   │    ├─ alters           alter::Alters      panels' changes, settings
//!   │    ├─ hires            hire::Hires        decodes, who pays, arrivals
//!   │    ├─ timers           timed::Timers      buildings' GS timers
//!   │    ├─ timeouts         spell::Timeouts    spell units' ends
//!   │    ├─ rearming         embassies re-armed this frame
//!   │    └─ asking           a GetBestSpell waiting on its script
//!   ├─ effects: Effects      what the frame moves beside units:
//!   │    ├─ flights          missile::Flights   missiles
//!   │    ├─ plumes           plume::Plumes      systems, impact sets, the
//!   │    │                                      frame's particle pass
//!   │    ├─ clones           trail::Clones      speed trails' clones
//!   │    └─ quakes           quake::Quakes
//!   ├─ outbox: Outbox        what the drawing and audio side reads that
//!   │                        stands until it ends: minimap_mark::Marks,
//!   │                        message::Messages, sound::Music; saved
//!   ├─ output: Output        what the frames leave for the front end:
//!   │                        cues, lines, shakes, FRAME IT's runs, lore
//!   │                        kills, PlayMusic's flag; taken each frame
//!   │                        (World::take_output), never saved
//!   ├─ pieces: Pieces        game pieces by agent; the new game's not yet
//!   │                        announced; which changed since the grid
//!   │                        last heard
//!   ├─ board: Board          the map: side, heights, cell paint and
//!   │                        pictures, vertex shade, scatter and its
//!   │                        stands, agentless objects as drawn, start
//!   │                        pieces' saved sets; relaid on each change;
//!   │                        the collision grid, kept up to date
//!   │                        (`World::regrid`)
//!   ├─ descriptors:          the dataset's tables, read only: shapes,
//!   │    Descriptors         bodies, their names, what orders read
//!   │                        by name
//!   ├─ losses: Losses        what the world lost track of, for a log
//!   │
//!   │  loose, each read by nearly every module:
//!   ├─ handle                the last object handle, issued to every
//!   │                        object made, piece or not
//!   ├─ frames                whether frames are stepped here: a mode
//!   └─ now                   the game clock
//! ```
//!
//! Its modules, by subsystem, each public; callers name an item by its
//! module (`world::chat::Line`):
//!
//! - agents and pieces: `agents`, `pieces`, `attributes`, `bodies`,
//!   `descriptors`, `places`, `search`, `titles`, `relations`, `housing`,
//!   `teleport`, `unit`, `naming`, `numbered`, `retype`, `inventory`;
//! - making and placing: `spawn`, `placing`, `lairs`, `new_map`, `load`,
//!   `build`, `board`, `building_ground`, `scatter`, `ruin`, `graveyard`;
//! - the frame: `frame`, `walking` (`step`, `orders`, `collide`, `resume`),
//!   `input_timing`, `cost`, `random`, `timed`, `animate`, `combat`,
//!   `missile`, `trail`, `quake`;
//! - effects: `effectors`, `stack`, `plume`, `particles`, `draw`,
//!   `minimap_mark`, `message`, `sound`, `chat`;
//! - sight: `reveal`;
//! - players: `players`, `treasury`, `ledger`, `slain`, `palace`,
//!   `defeat`, `limits`, `kingdom`, `advisor`, `score`;
//! - the player's orders: `alter`, `hire`, `recruit`, `research`,
//!   `price`, `offer`, `interface`, `flag`, `guild` and its actions
//!   (`guild::assembly`, `bounty`, `camp`, `embassy`, `extortion`,
//!   `gamble`, `market`, `rage`, `taxman`, `upkeep`, `mausoleum`),
//!   `demolish`, `dismiss`, `progress`, `cheat`;
//! - spells: `spell` (`cast`, `target`, `summon`, `illusion`, `learn`);
//! - scripts: `threads`, `step`, `action`;
//! - read out: `draw_view` (trails read it too), `loss_log`, and `read`,
//!   accessors for the front ends' queries; saved: `persist`.

use std::collections::{BTreeMap, BTreeSet};

use crate::base::digest::Map;
use crate::base::fixed::Fixed;
use crate::data::attrib::{BUTTON_DELAY_TIME, CURRENT_STAGE_BUILT, Key};
use std::sync::{Arc, OnceLock};

use super::space::HalfRect;
use crate::base::gfix::Gfix;
use crate::data::attrib;
use crate::data::fourcc::FourCc;
use crate::data::gpltype::Kind;
use crate::gpl::builtins::{self, BuiltinId};
use crate::gpl::load::Program;
use crate::gpl::ops::{self, Accessor, NULL_AGENT, OpError, Value, agent_of};
use crate::gpl::runtime::ThreadMode;
use crate::gpl::vm::host::{self, Call, NO_AGENT};
use crate::gpl::vm::native::{int_ref, put_int};
use crate::gpl::vm::threads::Busy;
use crate::gpl::vm::{Fault, Handle, ObjId, Vm};

pub mod action;
pub mod advisor;
pub mod agents;
pub mod alter;
pub mod animate;
pub mod attributes;
pub mod board;
pub mod bodies;
pub mod build;
pub mod building_ground;
pub mod chat;
pub mod cheat;
pub mod combat;
pub mod cost;
pub mod defeat;
pub mod demolish;
pub mod descriptors;
mod digest;
pub mod dismiss;
pub mod draw;
pub mod draw_view;
pub mod effectors;
pub mod flag;
pub mod frame;
pub mod graveyard;
pub mod guild;
pub mod hire;
pub mod housing;
pub mod input_timing;
pub mod interface;
pub mod inventory;
pub mod kind;
pub mod kingdom;
pub mod lairs;
pub mod ledger;
pub mod limits;
pub mod loss_log;
pub mod message;
pub mod minimap_mark;
pub mod missile;
pub mod naming;
pub mod new_map;
pub mod numbered;
pub mod offer;
pub mod palace;
pub mod particles;
mod persist;
pub mod piece_state;
pub mod pieces;
pub mod places;
pub mod placing;
pub mod players;
pub mod plume;
pub mod price;
pub mod progress;
pub mod quake;
pub mod random;
pub mod reach;
pub mod read;
pub mod recruit;
pub mod relations;
pub mod research;
pub mod retype;
pub mod reveal;
pub mod ruin;
pub mod scatter;
pub mod score;
pub mod search;
pub mod slain;
pub mod sound;
pub mod spawn;
pub mod spell;
pub mod stack;
pub mod step;
pub mod teleport;
pub mod threads;
pub mod timed;
pub mod titles;
pub mod trail;
pub mod treasury;
pub mod unit;
pub mod walking;

use advisor::Newcomer;
use draw::Fading;
use frame::Frames;
use limits::{Limits, Switch};
use spell::Charge;
use step::Draws;
use threads::Gpl;

/// One list of the world builtins answered here: the enum, and each by
/// its registered name, the variant's own unless given.
macro_rules! checked {
    ($($variant:ident $(= $name:literal)?,)*) => {
        /// The world builtins that can be answered here.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Checked {
            $($variant,)*
        }

        impl Checked {
            /// Each by its registered name.
            pub const ALL: [(&'static str, Checked); [$(Checked::$variant),*].len()] = [
                $((checked!(@name $variant $($name)?), Checked::$variant),)*
            ];
        }
    };
    (@name $variant:ident $name:literal) => { $name };
    (@name $variant:ident) => { stringify!($variant) };
}

checked! {
    RetrieveAgent,
    CreateAgent,
    HasAttribute,
    GetAttribute,
    SetAttribute,
    IsValidGamePiece,
    GetUnitPlayerNumber,
    ListTitles,
    NewThread,
    RunThread,
    BuildingIsRecruiting,
    GetVictoryConditionIndex,
    GetSpecialEvent1Script,
    GetSpecialEvent2Script,
    PlayMusic,
    SetMusicStoppedCallback,
    ListObjects,
    LocationOf,
    DistanceBetweenAgents,
    SpawnUnit,
    Hide,
    IsHidden,
    Parent,
    IsMoving,
    InsideBuilding,
    GetBestSpell,
    RemoveTitles,
    ListPalaces,
    AdjustPlayerData,
    CreateEffector,
    SetThreadInterval,
    Move,
    StopMoving,
    IsAdjacent,
    IsEnteringBuilding,
    HasWayPoints,
    UnHide,
    PerformAction,
    AgentNumber,
    PlaySound,
    LocalChatMessage,
    AdjustAttribute,
    IsSpellAvailable,
    CastSpell,
    GetSpellAttribute,
    CombatSignal,
    SpecifyIntent,
    GetPlayerTeamNumber,
    SetPlayerTeamNumber,
    UnitsAreSamePlayer,
    NeutralTeamNumber,
    CheckEffector,
    DeleteEffector,
    DeleteAllEffectors,
    DeleteGamePiece,
    AgentHasInventoryItem,
    CreateNewInventoryItem,
    DeleteInventoryItem,
    FindInventoryItem,
    MakeInventoryAttribute,
    GetInventoryItemAgentType,
    CanDropInventoryItem,
    TransferInventoryItem,
    GetInventoryItemDropType,
    CountInventoryItem,
    IsInventoryItem,
    AddAttribute,
    RemoveAttribute,
    SuspendThread,
    ResumeThread,
    KillThread,
    IsRunning,
    LastMusicTrack,
    SetParent,
    ClearFlag,
    SetHallFlag,
    GetInventoryItems,
    MiniMapAnimation,
    UpgradeAgentAttributes,
    RandomCoord,
    RevealArea,
    GetNearestHiddenCoord,
    SetSpellUnitTimeout,
    GetPlayerData,
    GetBuildingContents,
    MagicalAdjustAttribute,
    DeleteAgent,
    DeclareLoss,
    DeclareVictory,
    GetVictoryConditionModifier,
    SetPalace,
    UnitIsOffmapLair,
    LearnSpell,
    FillSpecialLairList,
    ClearDrawEffects,
    SetDrawEffects,
    TeleportToPoint,
    TeleportToUnit,
    ClearEngineDeathFlags,
    CreateMissile,
    GetBoardExtents,
    GetBuildingContainer,
    TurnOnSpeedTrail,
    TurnOffSpeedTrail,
    HasSpeedTrail,
    CancelGuildMembership,
    SetUnitPlayerNumber,
    MessageFlag,
    IsMessageFlagPresent,
    GetFlag,
    PlaceRewardFlag,
    DisableUnitType,
    EnableUnitType,
    SetBuildingLimit,
    RemoveBuildingLimit,
    RemoveAllBuildingLimits,
    ChangeUnitType,
    RevertUnitType,
    GetEntranceLoc,
    GetContainerAgent,
    SpawnGraveyard,
    CreateSpellUnit,
    ForgetSpell,
    EnchantWizTower,
    StartEarthquakeSpell,
    StopEarthquakeSpell,
    SetEffectorDirection,
    GetParent,
    GetNextWayPoint,
    PathCost,
    RunThreadOnce,
    SpecifyName,
    Breakpoint,
    Halt,
    LogOn,
    LogOff,
    NoDebugMessages,
    DwarvesVoiceSetOperative = "DwarvesVoice_SetOperative",
    ElvesVoiceSetOperative = "ElvesVoice_SetOperative",
    NewTeamNumber,
    FadeOut,
    ShowMinimap,
    FadeIn,
    IncrementStatCounter,
    SetShareReveal,
    RevealWholeMap,
    UnitIsLocalPlayer,
    SetTreasuryValues,
    GetBuildingLevel,
}

impl Checked {
    /// The one answering builtin `id`: a table by [`BuiltinId`], made
    /// from [`Checked::ALL`] on first use, so a call costs no name scan.
    #[must_use]
    pub fn of_id(id: BuiltinId) -> Option<Checked> {
        static BY_ID: OnceLock<Vec<Option<Checked>>> = OnceLock::new();

        let table = BY_ID.get_or_init(|| {
            let mut table = vec![None; BuiltinId::all().count()];

            for (name, c) in Checked::ALL {
                if let Some(id) = builtins::id(name) {
                    table[id.index()] = Some(c);
                }
            }

            table
        });

        table.get(id.index()).copied().flatten()
    }

    /// Its registered name: [`Checked::ALL`] is in variant order.
    #[must_use]
    pub const fn name(self) -> &'static str {
        Checked::ALL[self as usize].0
    }
}

/// The script the engine dispatches for each new game piece with a GPL
/// agent: `GenUnit` and `GenUnit` push its name
/// when the unit holds `AE:0`.
pub const NEW_UNIT: &str = "NewUnitInit";

/// The script agent create placed dispatches for the agent it has
/// just made, before it places it: before [`NEW_UNIT`].
pub const GENERATE: &str = "Generate_Character_Attributes";

/// The script gpl agent delete dispatches before it marks the agent
/// dead and unmaps it.
pub const UNIT_DELETE: &str = "UnitDelete";

/// The builtin that deletes an agent and leaves its unit.
pub const DELETE_AGENT: &str = "DeleteAgent";

/// The builtin whose spawned unit the next [`NEW_UNIT`] announces.
pub const SPAWN_UNIT: &str = "SpawnUnit";

/// The builtin that sets a unit's owner.
pub const SET_OWNER: &str = "SetUnitPlayerNumber";

/// The builtin that moves a unit's owner to a team: gpl set player team number calls
/// team set with the owner and the team.
pub const SET_TEAM: &str = "SetPlayerTeamNumber";

/// Teams, and players: eight masks of eight player bits.
pub const TEAMS: usize = 8;

/// A new level's teams, teams reset: every player on team 0, then slot 7
/// moved to the first empty team, 1.
pub const NEW_TEAMS: [u32; TEAMS] = [!(1 << 7), 1 << 7, 0, 0, 0, 0, 0, 0];

/// The GPL attribute `ListTitles` compares, str title.
pub const TITLE: &str = "title";

/// `AE:0`, `AgentID`: the GPL agent a game piece drives.
pub const AGENT_ID: Key = Key::new(*b"AE\0", 0);

/// The crown: slot 0, a single-player game's human.
pub const CROWN: u32 = 0;

/// The setup word `GetVictoryConditionIndex` answers: config
/// `RGS6`. Freestyle setup copies it into the world, world
/// materialise into the level, where the native reads it.
pub const VICTORY_SLOT: usize = 7;

/// The setup word `GetVictoryConditionModifier` answers:
/// config `RGS7`.
/// The gold to pass or the days to last ([`crate::sim::freestyle::Victory`]).
pub const GOAL_SLOT: usize = 8;

/// `GetSpecialEvent1Script`'s and `2`'s answer with no special event:
/// `STRT EVSC`'s `NONE` entry, `0 none`, after `sscanf("%d %s")`. The entry
/// is keyed by the config's event codes. **Game**: `none` in all three traces.
pub const NO_EVENT: &str = "none";

/// The table special events are keyed in, `mx_gpltext.cam`.
pub const EVENT_TABLE: &[u8; 4] = b"EVSC";

/// An `EVSC` entry's script: the text after its number, as
/// `sscanf("%d %s")` reads it. `60 Crunch_All_You_Want` gives
/// `Crunch_All_You_Want`; an entry that does not scan, [`NO_EVENT`]
/// (**Inferred**: no shipped entry fails).
///
/// ```
/// use regent_core::sim::world::event_script;
///
/// assert_eq!(event_script("60 Crunch_All_You_Want"), "Crunch_All_You_Want");
/// assert_eq!(event_script("0 none"), "none");
/// ```
#[must_use]
pub fn event_script(entry: &str) -> String {
    let mut words = entry.split_whitespace();
    let number = words.next().and_then(|w| w.parse::<i32>().ok());

    match (number, words.next()) {
        (Some(_), Some(script)) => script.to_owned(),
        _ => NO_EVENT.to_owned(),
    }
}

/// An `EVSC` entry's rating: its number, as `sscanf("%d %s")` reads it.
/// The freestyle presets pick events by it.
///
/// ```
/// use regent_core::sim::world::event_rating;
///
/// assert_eq!(event_rating("60 Crunch_All_You_Want"), Some(60));
/// assert_eq!(event_rating("none"), None);
/// ```
#[must_use]
pub fn event_rating(entry: &str) -> Option<i32> {
    entry.split_whitespace().next()?.parse().ok()
}

/// `HasAttribute`'s count check: the return slot and two arguments.
const HAS_ATTRIBUTE_ARGS: usize = 3;

/// An engine dispatch the world makes: a function's name and arguments.
pub type Dispatch = (String, Vec<Value>);

/// Whether the engine registered `key`: attribute registered turns the key into its
/// name and looks it up in the registry. Unknown keys throw.
fn registered(key: i32) -> bool {
    let [a, b, c, index] = key.to_le_bytes();

    attrib::name([a, b, c], index).is_some()
}

/// A game piece: what the engine keeps outside GPL.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Piece {
    /// `GenUnit`.
    pub owner: u32,
    /// The attribute list, by key.
    pub attributes: BTreeMap<i32, i32>,
    /// The save's object handle; 0 for a spawned piece.
    pub handle: u32,
    /// Its descriptor, a `DUNT` id.
    pub code: FourCc,
    /// Its descriptor's `HEAD` name: what the engine prints for its agent,
    /// and the `.dat` instance the unit
    /// pass makes the agent from.
    pub name: String,
    /// Where it stands: as the new game's save has it, or as made since.
    /// `None` where not known: a reward flag hung on a piece with no
    /// descriptor (`World::hang_on`).
    pub place: Option<Place>,
    /// Its spell entries, /; `None` where not known.
    pub spells: Option<Vec<Charge>>,
}

impl Piece {
    /// Attribute `key`; `None` where unset.
    #[must_use]
    pub fn get(&self, key: attrib::Key) -> Option<i32> {
        attrib::get(&self.attributes, key)
    }

    /// Attribute `key`, 0 where unset: attr get with its default.
    #[must_use]
    pub fn word(&self, key: attrib::Key) -> i32 {
        self.get(key).unwrap_or_default()
    }

    /// Whether its place is a building's ([`Place::is_building`]), walking
    /// or not; `World::known` is the place the world follows.
    #[must_use]
    pub fn is_building(&self) -> bool {
        self.place.is_some_and(|at| at.is_building())
    }

    /// Its place, if that cannot have changed since: a mobile walks.
    fn fixed(&self) -> Option<&Place> {
        self.place.as_ref().filter(|p| !p.is_vehicle())
    }
}

/// Where a piece of the new game stands, as its save records it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub kind: kind::Kind,
    /// 16.16 cells.
    pub fixed: (Fixed, Fixed),
    /// Its own half cell, [`space::half`] of [`Place::fixed`].
    pub standing: (i32, i32),
    /// The holder's handle, 0 for none: unit holder.
    pub holder: u32,
    /// Unit state: its [`piece_state`] bits.
    pub state: u8,
}

impl Place {
    /// Unit state bit 7: a held unit measures from itself, not its
    /// holder.
    pub const OWN_SPOT: u8 = 0x80;

    /// Whether it is a building's: kind [`crate::sim::world::kind::Kind::BUILDING`].
    #[must_use]
    pub fn is_building(&self) -> bool {
        self.kind == kind::Kind::BUILDING
    }

    /// Whether it is a vehicle's, a mobile: kind [`crate::sim::world::kind::Kind::VEHICLE`].
    #[must_use]
    pub fn is_vehicle(&self) -> bool {
        self.kind == kind::Kind::VEHICLE
    }

    /// Whether it is a reward flag's: kind [`crate::sim::world::kind::Kind::REWARD`].
    #[must_use]
    pub fn is_reward(&self) -> bool {
        self.kind == kind::Kind::REWARD
    }

    /// A building in ruins: `GenUnit`, unit state bit 4.
    fn ruined(&self) -> bool {
        self.is_building() && self.state & piece_state::RUINED != 0
    }
}

/// Who may see a unit its owner does not: `PRIM` `Info` bit 15,
/// `NotVisibleToOpponent`, zeroes every player's sight of it but its
/// owner's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sight {
    Everyone,
    Owner,
}

/// How unit sight update samples a unit: a building, or a unit
/// with `Info` bit 17 `RevealTerrainIfExposed`, over its `PRIM` rect,
/// revealing about it for each player who sees it; any other at its cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exposure {
    Point,
    Rect,
}

impl Exposure {
    /// A descriptor's.
    #[must_use]
    pub fn of(desc: &crate::data::desc::Unit<'_>) -> Exposure {
        let building = desc.kind() == crate::data::desc::UnitKind::Building;

        if building
            || desc
                .engine()
                .has(crate::data::desc::Info::RevealTerrainIfExposed)
        {
            Exposure::Rect
        } else {
            Exposure::Point
        }
    }
}

/// What a save does not say about a descriptor's units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shape {
    /// `sim::placement::pad_rect`.
    pub rect: HalfRect,
    /// `sim::placement::pixel_rect`: the rect before halving, 32nds.
    pub bounds: HalfRect,
    pub sight: Sight,
    pub exposure: Exposure,
}

/// Shapes by descriptor.
pub type Shapes = Map<FourCc, Shape>;

/// Whether the world answered a call, or cannot know the answer: a piece's
/// position, sight or attributes are not known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Given,
    Unknown,
    /// The native dispatches a script and reads its result before it
    /// answers: the host runs [`World::take_ask`]'s and passes its result
    /// to [`World::resume_ask`] (`GetBestSpell`'s `ValidationScript`).
    Asks,
}

/// A GPL agent: its attribute names and its title as made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Agent {
    /// Folded: the attribute map compares case-insensitively
    /// (gpl attribute map find calls string compare with its fold flag).
    pub attributes: BTreeSet<String>,
    /// [`TITLE`]'s value as made.
    pub title: Option<String>,
    /// Its own name, word `0x24`: `CreateAgent`'s second argument; empty
    /// for a unit's agent (unit agent new passes `""`).
    pub name: String,
    /// Each attribute as made, in prototype order.
    pub values: Vec<Member>,
}

/// An attribute as its agent was made with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// As the prototype declares it.
    pub name: String,
    /// [`crate::data::gpltype`] code.
    pub ty: u8,
    pub value: Value,
}

impl Agent {
    /// An agent made from `.dat` instance `data`: every member of its
    /// prototype, values from the instance (gpl agent create: "couldn't find
    /// initialization data", then "couldn't find prototype data"). Each
    /// member takes the instance's text for it, `""` when absent,
    /// through its class's slot 7 ([`from_text`]).
    fn made(program: &Program, data: &str) -> std::result::Result<Agent, Fault> {
        let set = program
            .data_set(data)
            .ok_or_else(|| Fault::throw("couldn't find initialization data"))?;
        let members = program
            .prototype(program.names().get(set.shape))
            .ok_or_else(|| Fault::throw("couldn't find prototype data"))?;
        let title = set
            .values
            .iter()
            .find(|(k, _)| program.names().get(*k).eq_ignore_ascii_case(TITLE))
            .map(|(_, v)| program.names().get(*v).to_owned());

        let values = members
            .iter()
            .map(|m| {
                let name = program.names().get(m.name);
                let text = set
                    .values
                    .iter()
                    .find(|(k, _)| program.names().get(*k).eq_ignore_ascii_case(name))
                    .map_or("", |(_, v)| program.names().get(*v));

                Ok(Member {
                    name: name.to_owned(),
                    ty: m.ty.code(),
                    value: from_text(program, m.ty.code(), text)?,
                })
            })
            .collect::<std::result::Result<Vec<Member>, Fault>>()?;

        Ok(Agent {
            attributes: members
                .iter()
                .map(|m| program.names().get(m.name).to_ascii_lowercase())
                .collect(),
            title,
            name: String::new(),
            values,
        })
    }
}

/// A `.dat` text as a value of type `ty`, each class's `GplType` text setter:
///
/// | Class | Slot 7 | Rule |
/// |---|---|---|
/// | integer | `GplInteger` | empty 0; a leading digit `atoi`; else the attribute registry, else throws |
/// | float | `GplFloat` | empty 0; else `Str2FixedLP`, [`str_to_fixed`] |
/// | string | `GplString` | the text |
/// | list | `GplList` | a string per token |
/// | agent | `GplAgentRef` | nothing: the null agent |
/// | boolean | `GplBoolean` | `false`, empty or `0` false; `true` or `1` true; else throws |
/// | coord | `GplCoord` | `atoi` of two tokens |
/// | function | `GplFunction` | the function map by name, `""` included; else the null record |
///
/// **Inferred**: the tokeniser, `CYToken`, splits on white space.
///
/// # Errors
///
/// The two throws, with the engine's messages.
pub fn from_text(program: &Program, ty: u8, text: &str) -> std::result::Result<Value, Fault> {
    let kind = Kind::from_code(ty).unwrap_or(Kind::Null);
    let digit = text.starts_with(|c: char| c.is_ascii_digit());

    match kind {
        Kind::Integer if text.is_empty() => Ok(Value::Integer(0)),
        Kind::Integer if digit => Ok(Value::Integer(atoi(text))),
        Kind::Integer => host::registry(text).map(Value::Integer).ok_or_else(|| {
            Fault::throw(format!("Error initializing attribute to value \"{text}\""))
        }),
        Kind::Float if text.is_empty() => Ok(Value::Float(Gfix(0))),
        Kind::Float => Ok(Value::Float(Gfix(str_to_fixed(text)))),
        Kind::String => Ok(Value::String(text.to_owned())),
        Kind::List => Ok(Value::List(
            text.split_whitespace()
                .map(|t| Value::String(t.to_owned()))
                .collect::<Vec<_>>()
                .into(),
        )),
        Kind::Boolean => boolean(text),
        Kind::Coord => {
            let mut tokens = text.split_whitespace().map(atoi);
            let x = tokens.next().unwrap_or_default();

            Ok(Value::Coord(x, tokens.next().unwrap_or_default()))
        }
        Kind::Function => Ok(Value::Function(
            Vm::named_handle(program, text).unwrap_or(ops::NULL_FUNCTION),
        )),
        Kind::Agent => Ok(Value::Agent(NULL_AGENT)),
        Kind::Null => Ok(Value::Null),
    }
}

/// `GplBoolean`.
fn boolean(text: &str) -> std::result::Result<Value, Fault> {
    let off = text.eq_ignore_ascii_case("false") || text.is_empty() || text == "0";
    let on = text.eq_ignore_ascii_case("true") || text == "1";

    if off {
        Ok(Value::Boolean(false))
    } else if on {
        Ok(Value::Boolean(true))
    } else {
        Err(Fault::throw(format!(
            "Error initializing boolean value to {text}"
        )))
    }
}

/// C `atoi`: white space, a sign, digits; 0 without any.
fn atoi(text: &str) -> i32 {
    let t = text.trim_start();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    let n = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.wrapping_mul(10).wrapping_add(i32::from(d - b'0'))
        });

    sign * n
}

/// One in fixed point: a digit is `(c - '0') * 0x400`.
const FIXED_ONE: i32 = 1024;

/// Ten in fixed point, `0x2800`.
const FIXED_TEN: i32 = 10 * FIXED_ONE;

/// `Str2FixedLP`, str to fixed: a sign, integer digits by `v * 10 + d`,
/// then the fraction from its last digit back by `f = (d + f) / 10`, all in
/// fixed point. Any other
/// character gives 0.
///
/// ```
/// use regent_core::sim::world::str_to_fixed;
///
/// assert_eq!(str_to_fixed("1.5"), 1536);
/// assert_eq!(str_to_fixed("-0.9"), -921);
/// assert_eq!(str_to_fixed("2x"), 0);
/// ```
#[must_use]
pub fn str_to_fixed(text: &str) -> i32 {
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let split = body
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(body.len());
    let (int, rest) = body.split_at(split);
    let whole = int.bytes().fold(0, |v, d| {
        ops::fixed_mul(v, FIXED_TEN).wrapping_add(i32::from(d - b'0') * FIXED_ONE)
    });
    let fraction = match rest.strip_prefix('.') {
        None if rest.is_empty() => Some(0),
        Some(tail) if tail.bytes().all(|b| b.is_ascii_digit()) => {
            tail.bytes().rev().try_fold(0, |f, d| {
                ops::fixed_div(i32::from(d - b'0') * FIXED_ONE + f, FIXED_TEN).ok()
            })
        }
        _ => None,
    };

    match fraction {
        Some(f) if negative => -(f + whole),
        Some(f) => f + whole,
        None => 0,
    }
}

/// How a checked builtin reads a GPL attribute's current value: the VM's
/// attribute object where the run has one, borrowed: a search reads one
/// per piece.
pub type Attributes<'a> = dyn for<'v> Fn(&'v Vm<'_>, u32, &str) -> Option<&'v Value> + 'a;

/// The world.
#[derive(Clone, Debug, Default)]
pub struct World {
    /// GPL agents by id and name, the next id, what the host is to take
    /// ([`agents`]).
    agents: agents::Agents,
    /// Pieces by agent, the new game's unannounced ([`pieces`]).
    pieces: pieces::Pieces,
    /// Each unit's record beside its piece, by agent ([`mod@unit`]).
    units: unit::Units,
    /// Game time, ms: game clock, the threads' "now".
    now: u32,
    /// The level's words, its tick and its inventory register.
    level: Level,
    /// What the drawing and audio side reads.
    outbox: Outbox,
    /// Whether threads run, the log, `Breakpoint` ([`threads`]).
    scripts: threads::Scripts,
    /// The dataset's descriptor tables, read only ([`descriptors`]):
    /// shared, so a copy of the world does not copy them.
    descriptors: Arc<descriptors::Descriptors>,
    /// Explored ground and what reveals it ([`reveal`]).
    fog: reveal::Fog,
    /// The map's physical state ([`board`]).
    board: board::Board,
    /// Each bound piece's pad rect, kept for `ListObjects` ([`reach`]).
    reaches: reach::Reaches,
    /// The last object handle issued.
    handle: u32,
    /// What the world lost track of: events for a log.
    losses: loss_log::Losses,
    /// `HOME` lists and flags' rosters ([`relations`]).
    relations: relations::Relations,
    /// Engine dispatches for the host to run, and list ORDR ([`frame`]).
    dispatches: frame::Dispatches,
    /// The level's players, each one's record ([`players`]).
    players: players::Players,
    /// Effectors made, where they hang, which hung at the last step
    /// ([`effectors`]).
    effectors: effectors::Effectors,
    /// The name generators and the start pieces to name ([`naming`]).
    naming: naming::Naming,
    /// Whether frames are stepped here.
    frames: Frames,
    /// Orders and timers still to come.
    pending: Pending,
    /// Each spawner's lair data, by handle ([`lairs`]).
    lairs: lairs::Lairs,
    /// The world RNG, and the GPL RNG lent for a call ([`step`]).
    rngs: step::Rngs,
    /// Whether cheats are allowed; the two flags cheats set, by player.
    cheats: cheat::Flags,
    /// Buildings falling, ruins standing, and the sets buildings show.
    ruins: ruin::Ruins,
    /// Missiles, particle systems, clones, quakes.
    effects: Effects,
    /// Each human seat's advisor threads ([`advisor`]).
    advisors: advisor::Seats,
    /// Poison's owner until its last tick, by victim agent.
    poison_sources: BTreeMap<u32, u32>,
    /// Disabled types, building caps, hero lists ([`limits`]).
    limits: Limits,
    /// Last Palace's multipliers, from the program ([`price::Lps`]).
    lps: price::Lps,
    /// What the frames left for the front end since it last took it.
    output: Output,
}

/// What the frame moves beside units, each kept by the module that runs
/// it.
#[derive(Clone, Debug, Default)]
struct Effects {
    /// Missiles in flight ([`missile`]).
    flights: missile::Flights,
    /// Particle systems, impact sets, particles ([`plume`]).
    plumes: plume::Plumes,
    /// Speed trails' clones ([`trail`]).
    clones: trail::Clones,
    /// Earthquake orders ([`quake`]).
    quakes: quake::Quakes,
}

/// What the drawing and audio side reads that stands until it ends or
/// changes, each kept by the module that makes it: marks, message flags
/// and the music. Saved: play state a load restores.
#[derive(Clone, Debug, Default)]
struct Outbox {
    /// Minimap marks ([`minimap_mark`]).
    marks: minimap_mark::Marks,
    /// Message flags ([`message`]).
    messages: message::Messages,
    /// The music player's track and callback ([`sound`]).
    music: sound::Music,
}

/// What the frames leave for the front end, taken each frame
/// ([`World::take_output`], `Game::take_output`) and never saved:
/// presentation only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// Sounds started, oldest first ([`sound`]).
    pub cues: Vec<sound::Cue>,
    /// Lines said, oldest first ([`chat`]).
    pub said: Vec<chat::Said>,
    /// Quakes' shakes, in order: each a step of the view ([`quake`]).
    pub shakes: Vec<quake::Shake>,
    /// `FRAME IT`'s runs: every peer's view toggles its frame rate
    /// (`view::panel::day::Frames`, [`cheat`]).
    pub frame_cheats: u32,
    /// Credited deaths, for the profile's lore book ([`combat`]).
    pub lore_kills: Vec<combat::LoreKill>,
    /// Buildings the engine deselects, by agent, oldest first: each
    /// player's selection drops one it holds ([`combat`], [`ruin`]).
    pub deselects: Vec<u32>,
    /// Whether `PlayMusic` ran: a track the stop callback chose gives
    /// way to the scripts' ([`sound`]).
    pub music_played: bool,
}

impl Output {
    /// `more`, added after these.
    pub fn append(&mut self, mut more: Output) {
        self.cues.append(&mut more.cues);
        self.said.append(&mut more.said);
        self.shakes.append(&mut more.shakes);
        self.frame_cheats = self.frame_cheats.wrapping_add(more.frame_cheats);
        self.lore_kills.append(&mut more.lore_kills);
        self.deselects.append(&mut more.deselects);
        self.music_played |= more.music_played;
    }
}

/// The level's words, its tick and its inventory register: `GS_Level`'s.
#[derive(Clone, Debug, Default)]
struct Level {
    /// [`VICTORY_SLOT`]'s word.
    victory: i32,
    /// [`GOAL_SLOT`]'s word.
    goal: i32,
    /// The two special events' scripts.
    events: [String; 2],
    /// The periodic hit point tick ([`combat`]).
    mending: combat::Mend,
    /// Each item key's carriers ([`inventory`]).
    items: inventory::Register,
}

/// What waits for its frame or its time, each kept by the module that
/// runs it.
#[derive(Clone, Debug, Default)]
struct Pending {
    /// Research ordered and running ([`research`]).
    research: research::Researching,
    /// Panels' attribute changes and settings ([`alter`]).
    alters: alter::Alters,
    /// Recruits' decodes, who pays, heroes due ([`hire`]).
    hires: hire::Hires,
    /// Buildings' GS timers ([`timed`]).
    timers: timed::Timers,
    /// Spell units' ends ([`spell`]).
    timeouts: spell::Timeouts,
    /// Embassies whose last tick re-arms them this frame ([`guild::embassy`]).
    rearming: BTreeSet<u32>,
    /// A `GetBestSpell` waiting on a `ValidationScript` ([`spell`]).
    asking: Option<spell::Asking>,
    /// Each player's camp move begun ([`guild::camp`]).
    camps: guild::camp::Moves,
}

/// What list ORDR holds: a unit by its agent, or a missile, a unit with
/// no agent, by its handle (`GenUnit`: each joins the back with its
/// first order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Listed {
    Unit(u32),
    Missile(u32),
    /// An effector's `Wait`, by handle.
    Effector(u32),
    /// A speed trail's clone, by handle ([`trail`]).
    Clone(u32),
}

/// The name gpl agentref own name gives an agent id no agent has:
/// the text at missing agent text.
pub const MISSING: &str = "??";

/// The first agent id, gpl ai reset.
pub const FIRST_ID: u32 = 1;

impl World {
    /// Measure, place and name pieces by `descriptors` from now on.
    #[must_use]
    pub fn with_descriptors(mut self, descriptors: descriptors::Descriptors) -> World {
        self.descriptors = Arc::new(descriptors);
        // What stands on the grid, and what scatter sight samples, is
        // measured anew.
        self.board.ungrid();
        self.board.forget_exposed();
        self
    }

    /// The map's physical state.
    #[must_use]
    pub const fn board(&self) -> &board::Board {
        &self.board
    }

    /// What the frames left for the front end since the last call.
    pub fn take_output(&mut self) -> Output {
        std::mem::take(&mut self.output)
    }

    /// The descriptor tables the world reads.
    #[must_use]
    pub fn descriptors(&self) -> &descriptors::Descriptors {
        &self.descriptors
    }

    /// The descriptor tables, this world's own: for tests that add to them.
    #[cfg(test)]
    fn descriptors_mut(&mut self) -> &mut descriptors::Descriptors {
        self.board.forget_exposed();
        self.forget_reaches();
        Arc::make_mut(&mut self.descriptors)
    }

    /// The human seats: players `0..n`, the session's `n` its setup gave,
    /// [`players::SESSION_PLAYERS`] by default. Saved:
    /// play reads it.
    #[must_use]
    pub const fn humans(&self) -> std::ops::Range<u32> {
        self.players.humans()
    }

    /// `GS_Level` with flags `0xc`: an agent for each piece
    /// of the new game, in unit order, ids from the next. **Game**: the
    /// pieces the save gave an agent, `AE:0` not 0, in handle order
    /// (**Measured**). Each
    /// agent is made from its piece's name as `.dat` instance, and its id
    /// written to `AE:0`.
    ///
    /// # Errors
    ///
    /// Unit agent new's throw, when the instance is missing.
    pub fn unit_pass(&mut self, program: &Program) -> std::result::Result<Vec<u32>, Fault> {
        let mut units: Vec<Piece> = self
            .pieces
            .drain_unbound()
            .filter(|(id, _)| *id != 0)
            .map(|(_, p)| p)
            .collect();

        units.sort_by_key(|p| p.handle);

        let mut ids = Vec::new();

        for mut piece in units {
            let id = self.agents.next_id();
            let agent = Agent::made(program, &piece.name).map_err(|_| {
                Fault::throw(format!(
                    "NewAgent() could not successfully initialize object {}",
                    piece.name
                ))
            })?;

            piece.attributes.insert(AGENT_ID.packed(), id.cast_signed());
            self.agents.advance();
            self.agents.insert(id, agent);
            self.pieces.insert(id, piece);
            self.agents.note_made(id);
            ids.push(id);
        }

        Ok(ids)
    }

    /// The agents made since the last call, in order of making.
    pub fn take_made(&mut self) -> Vec<u32> {
        self.agents.take_made()
    }

    /// Changes whenever an agent is inserted or removed, and differs
    /// between worlds that changed apart.
    #[must_use]
    pub const fn agents_stamp(&self) -> u64 {
        self.agents.stamp()
    }

    /// Agent `id`'s own stamp, if alive: changes whenever it is inserted
    /// or changed, and differs between worlds that changed apart.
    #[must_use]
    pub fn agent_stamp(&self, id: u32) -> Option<u64> {
        self.agents.stamp_of(id)
    }

    /// The next agent id, gpl ai `0x34`.
    #[must_use]
    pub const fn next_id(&self) -> u32 {
        self.agents.next_id()
    }

    /// Game time, ms: game clock.
    #[must_use]
    pub const fn now(&self) -> u32 {
        self.now
    }

    /// The game clock set to `ms`, as the frame step and the decode set it.
    pub const fn set_now(&mut self, ms: u32) {
        self.now = ms;
    }

    /// Agents reachable by name: `CreateAgent`'s second argument.
    #[must_use]
    pub fn named(&self) -> Vec<(String, u32)> {
        self.agents.names()
    }

    /// The agent `name` maps to, any case, alive or not: [`World::named`]'s
    /// entry for it, without the list.
    #[must_use]
    pub fn named_any(&self, name: &str) -> Option<u32> {
        self.agents.named_any(name)
    }

    /// The name the engine prints for agent `id` (`GplAgentRef`,
    /// `GplAgentRef`): its piece's descriptor name while the piece stands,
    /// else its own name; [`MISSING`] with no agent; `None` for an agent
    /// with neither, `agent(free)`.
    #[must_use]
    pub fn agent_name(&self, id: u32) -> Option<String> {
        let own = self.agents.get(id).map(|a| a.name.clone());
        let piece = self.unit(id).map(|p| p.name.clone());

        match own {
            None => Some(MISSING.to_owned()),
            Some(own) => piece
                .filter(|n| !n.is_empty())
                .or(Some(own))
                .filter(|n| !n.is_empty()),
        }
    }

    /// The agent `id`, if alive.
    #[must_use]
    pub fn agent(&self, id: u32) -> Option<&Agent> {
        self.agents.get(id)
    }

    /// The piece agent `id` drives, if it stands.
    #[must_use]
    pub fn piece(&self, id: u32) -> Option<&Piece> {
        self.pieces.get(id)
    }

    /// Run world builtin `call`, engine code inside drawing on the VM's
    /// GPL RNG as `draws` says.
    ///
    /// # Errors
    ///
    /// What the engine's native throws, or an accessor's bad conversion.
    pub fn run(
        &mut self,
        vm: &mut Vm<'_>,
        call: &Call<'_>,
        gpl: &Attributes<'_>,
        draws: Draws,
    ) -> std::result::Result<Answer, Fault> {
        let from = self.asking(call);

        // The GPL RNG lent for the call: every draw inside takes it.
        self.rngs.lend((draws == Draws::Lent).then_some(vm.rng));

        let answer = self.answer(vm, call, gpl);

        if let Some(state) = self.rngs.take_lent() {
            vm.rng = state;
        }

        self.asked(vm, call, from);
        answer
    }

    /// [`World::run`]'s builtin, by its [`Checked`] ([`World::checked`]).
    fn answer(
        &mut self,
        vm: &mut Vm<'_>,
        call: &Call<'_>,
        gpl: &Attributes<'_>,
    ) -> std::result::Result<Answer, Fault> {
        match Checked::of_id(call.builtin) {
            Some(c) => self.checked(c, vm, call, gpl),
            None => Ok(Answer::Given),
        }
    }

    /// [`World::answer`] for a known builtin: one flat `match`, by group,
    /// with no `_` arm, so a new [`Checked`] fails to compile until answered.
    #[allow(clippy::too_many_lines, reason = "an arm a builtin")]
    fn checked(
        &mut self,
        c: Checked,
        vm: &mut Vm<'_>,
        call: &Call<'_>,
        gpl: &Attributes<'_>,
    ) -> std::result::Result<Answer, Fault> {
        let args = call.args;

        match c {
            // Agents, their attributes, threads and the level's words: each
            // always answers.
            Checked::RetrieveAgent => given(self.retrieve(vm, args)),
            Checked::CreateAgent => given(self.create(vm, args)),
            Checked::HasAttribute => given(self.has(vm, args)),
            Checked::SetAttribute => given(self.set(vm, args)),
            Checked::IsValidGamePiece => given(self.valid(vm, args)),
            Checked::GetUnitPlayerNumber => given(self.owner(vm, args)),
            Checked::GetBuildingLevel => given(self.level(vm, args)),
            Checked::ListTitles => given(self.titles(vm, args, gpl)),
            Checked::NewThread => given(self.thread(vm, call, ThreadMode::Repeat, Busy::SetsMode)),
            Checked::RunThread => given(self.thread(vm, call, ThreadMode::Once, Busy::SetsMode)),
            Checked::RunThreadOnce => {
                given(self.thread(vm, call, ThreadMode::Once, Busy::KeepsMode))
            }
            Checked::BuildingIsRecruiting => given(self.recruiting(vm, args)),
            Checked::GetVictoryConditionIndex => given(level_word(vm, args, self.level.victory)),
            Checked::GetVictoryConditionModifier => given(level_word(vm, args, self.level.goal)),
            Checked::SetPalace => given(self.set_palace(vm, args)),
            Checked::GetSpecialEvent1Script => given(self.event(vm, args, 0)),
            Checked::GetSpecialEvent2Script => given(self.event(vm, args, 1)),
            Checked::PlayMusic => given(self.play_music(vm, args)),
            Checked::SetMusicStoppedCallback => given(self.music_callback(vm, args)),
            Checked::ListPalaces => given(self.list_palaces(vm, args)),
            Checked::SetThreadInterval => given(self.interval(vm, args)),
            Checked::AgentNumber => given(self.agent_number(vm, args)),
            Checked::SuspendThread => given(self.suspend_thread(vm, args)),
            Checked::ResumeThread => given(self.resume_thread(vm, args)),
            Checked::KillThread => given(self.kill_thread(vm, args)),
            Checked::IsRunning => given(self.is_running(vm, args)),
            Checked::PlaySound => given(self.play_sound(vm, args)),
            Checked::LocalChatMessage => given(self.local_chat(vm, args)),
            Checked::Breakpoint => given(self.breakpoint(vm, args)),
            Checked::Halt => {
                self.scripts.set_gpl(Gpl::Halted);
                Ok(Answer::Given)
            }
            Checked::NoDebugMessages => {
                self.no_debug_messages(vm);
                Ok(Answer::Given)
            }
            // `LogOn`, `LogOff`: each a bare return.
            Checked::LogOn | Checked::LogOff => Ok(Answer::Given),

            // Pieces: where they stand and go, what holds them, what they are.
            Checked::ListObjects => {
                self.sight();
                self.rereach();
                self.objects(vm, args, gpl)
            }
            Checked::LocationOf => self.location(vm, args),
            Checked::DistanceBetweenAgents => self.distance(vm, args),
            Checked::GetAttribute => self.get(vm, args),
            Checked::SpawnUnit => self.spawn(vm, args),
            Checked::Hide => self.hide(vm, args),
            Checked::IsHidden | Checked::InsideBuilding => self.inside(vm, call),
            Checked::Parent => self.parent(vm, args),
            Checked::IsMoving => self.moving(vm, args),
            Checked::RemoveTitles => self.remove_titles(vm, args, gpl),
            Checked::Move => self.walk_to(vm, args),
            Checked::StopMoving => self.stop_moving(vm, args),
            Checked::IsAdjacent => self.is_adjacent(vm, args),
            Checked::IsEnteringBuilding => self.is_entering(vm, args),
            Checked::HasWayPoints => self.has_waypoints(vm, args),
            Checked::UnHide => self.unhide(vm, args),
            Checked::PerformAction => self.perform(vm, args),
            Checked::AdjustAttribute => self.adjust_attribute(vm, args),
            Checked::CombatSignal => self.combat_signal(vm, args),
            Checked::SpecifyIntent => self.specify_intent(vm, args),
            Checked::DeleteGamePiece => self.delete_piece(vm, args),
            Checked::AddAttribute => self.add_attribute(vm, args, gpl),
            Checked::RemoveAttribute => self.remove_attribute(vm, args),
            Checked::SetParent => self.set_parent(vm, args),
            Checked::UpgradeAgentAttributes => self.upgrade_attributes(vm, args),
            Checked::RandomCoord => self.random_coord(vm, args),
            Checked::GetBuildingContents => self.contents(vm, args),
            Checked::MagicalAdjustAttribute => self.magical_adjust(vm, args),
            Checked::DeleteAgent => self.delete_agent(vm, args),
            Checked::UnitIsOffmapLair => self.is_offmap_lair(vm, args),
            Checked::FillSpecialLairList => self.special_lair(vm, args),
            Checked::TeleportToPoint => self.teleport_point(vm, args),
            Checked::TeleportToUnit => self.teleport_unit(vm, args),
            Checked::ClearEngineDeathFlags => self.raise_dead(vm, args),
            Checked::GetBoardExtents => self.board_extents(vm, args),
            Checked::GetBuildingContainer => self.container(vm, args),
            Checked::ChangeUnitType => self.change_type(vm, args),
            Checked::RevertUnitType => self.revert_type(vm, args),
            Checked::GetEntranceLoc => self.entrance(vm, args),
            Checked::GetContainerAgent => self.container_agent(vm, args),
            Checked::SpawnGraveyard => self.spawn_graveyard(vm, args),
            Checked::GetParent => self.get_parent(vm, args),
            Checked::GetNextWayPoint => self.next_waypoint(vm, args),
            Checked::PathCost => self.path_cost(vm, args),
            Checked::SpecifyName => self.specify_name(vm, args),

            // Inventory items.
            Checked::AgentHasInventoryItem => self.has_item(vm, args),
            Checked::CreateNewInventoryItem => self.create_item(vm, args),
            Checked::DeleteInventoryItem => self.delete_item(vm, args),
            Checked::FindInventoryItem => self.find_item(vm, args),
            Checked::MakeInventoryAttribute => World::make_attribute(vm, args),
            Checked::GetInventoryItemAgentType => self.item_agent_type(vm, args),
            Checked::CanDropInventoryItem => self.can_drop(vm, args),
            Checked::TransferInventoryItem => self.transfer_item(vm, args),
            Checked::GetInventoryItemDropType => self.item_drop_type(vm, args),
            Checked::CountInventoryItem => self.count_item(vm, args),
            Checked::IsInventoryItem => self.is_item(vm, args),
            Checked::GetInventoryItems => self.inventory_items(vm, args),

            // Spells, effectors, missiles, speed trails, quakes, draw effects.
            Checked::GetBestSpell => self.best_spell(vm, args),
            Checked::IsSpellAvailable => self.spell_available(vm, args),
            Checked::CastSpell => self.cast_spell(vm, args),
            Checked::SetSpellUnitTimeout => self.spell_timeout(vm, args),
            Checked::GetSpellAttribute => self.spell_attribute(vm, args),
            Checked::LearnSpell => self.learn_spell(vm, args),
            Checked::CreateSpellUnit => self.create_spell_unit(vm, args),
            Checked::ForgetSpell => self.forget_spell(vm, args),
            Checked::EnchantWizTower => self.enchant_tower(vm, args),
            Checked::CreateEffector => self.effector(vm, args),
            Checked::CheckEffector => self.check_effector(vm, args),
            Checked::DeleteEffector => self.delete_effector(vm, args),
            Checked::DeleteAllEffectors => self.delete_all_effectors(vm, args),
            Checked::SetEffectorDirection => self.effector_direction(vm, args),
            Checked::CreateMissile => self.create_missile(vm, args),
            Checked::TurnOnSpeedTrail => self.trail_on(vm, args),
            Checked::TurnOffSpeedTrail => self.trail_off(vm, args),
            Checked::HasSpeedTrail => self.has_trail(vm, args),
            Checked::StartEarthquakeSpell => self.start_quake(vm, args),
            Checked::StopEarthquakeSpell => self.stop_quake(vm, args),
            Checked::ClearDrawEffects => self.clear_draw(vm, args),
            Checked::SetDrawEffects => self.set_draw(vm, args),

            // Players and teams, flags, limits, sight, the screen.
            Checked::AdjustPlayerData => self.adjust(vm, args),
            Checked::GetPlayerTeamNumber => self.team_number(vm, args),
            Checked::SetPlayerTeamNumber => self.set_team(vm, args),
            Checked::UnitsAreSamePlayer => self.same_player(vm, args),
            Checked::NeutralTeamNumber => World::neutral_team(vm, args),
            Checked::NewTeamNumber => self.new_team(vm, args),
            Checked::SetUnitPlayerNumber => self.set_owner(vm, args),
            Checked::UnitIsLocalPlayer => self.local_player(vm, args),
            Checked::GetPlayerData => self.player_data(vm, args),
            Checked::SetTreasuryValues => self.treasury_values(vm, args),
            Checked::IncrementStatCounter => self.stat_counter(vm, args),
            Checked::DeclareLoss => self.declare_loss(vm, args),
            Checked::DeclareVictory => self.declare_victory(vm, args),
            Checked::CancelGuildMembership => self.cancel_guild(vm, args),
            Checked::ClearFlag => self.clear_flag(vm, args),
            Checked::SetHallFlag => self.hall_flag(vm, args),
            Checked::MessageFlag => self.message_flag(vm, args),
            Checked::IsMessageFlagPresent => self.message_present(vm, args),
            Checked::GetFlag => self.get_flag(vm, args),
            Checked::PlaceRewardFlag => self.place_reward_flag(vm, args),
            Checked::DisableUnitType => self.switch_type(vm, args, Switch::Disable),
            Checked::EnableUnitType => self.switch_type(vm, args, Switch::Enable),
            Checked::SetBuildingLimit => self.set_limit(vm, args),
            Checked::RemoveBuildingLimit => self.remove_limit(vm, args),
            Checked::RemoveAllBuildingLimits => Ok(self.remove_limits()),
            Checked::RevealArea => self.reveal_area(vm, args),
            Checked::GetNearestHiddenCoord => self.nearest_hidden_coord(vm, args),
            Checked::SetShareReveal => self.share_reveal(vm, args),
            Checked::RevealWholeMap => self.reveal_whole_map(vm, args),
            Checked::MiniMapAnimation => self.minimap_mark(vm, args),
            Checked::ShowMinimap => self.show_minimap(vm, args),
            Checked::LastMusicTrack => self.last_track(vm, args),
            Checked::DwarvesVoiceSetOperative => self.voice_operative(vm, args, Newcomer::Dwarves),
            Checked::ElvesVoiceSetOperative => self.voice_operative(vm, args, Newcomer::Elves),
            Checked::FadeOut => self.fade(vm, args, Fading::Out),
            Checked::FadeIn => self.fade(vm, args, Fading::In),
        }
    }

    // -----------------------------------------------------------------------
    // Engine events

    /// Agent `id`, announced, made and bound to its piece: the new game's,
    /// else `spawning`, what a `SpawnUnit` the log answers gives
    /// ([`World::calling`]), else the crown's. `name` is the agent's
    /// `.dat` instance, the name a log prints for it (**Inferred**: the six
    /// start pieces print their descriptors' names, each a `.dat`
    /// instance). Nothing for an agent with a piece.
    pub fn announce(
        &mut self,
        program: &Program,
        id: u32,
        name: Option<&str>,
        spawning: Option<&Piece>,
    ) {
        if !self.pieces.contains(id) {
            let mut piece = self
                .pieces
                .unbind(id)
                .or_else(|| spawning.cloned())
                // Slot 0's, on every machine.
                .unwrap_or(Piece {
                    owner: CROWN,
                    ..Piece::default()
                });
            let agent = name
                .and_then(|n| Agent::made(program, n).ok())
                .unwrap_or_default();

            if piece.name.is_empty() {
                name.unwrap_or_default().clone_into(&mut piece.name);
            }

            self.agents.insert(id, agent);
            self.pieces.insert(id, piece);
            self.agents.note_made(id);
            self.agents.announced(id);
        }
    }

    /// After the dispatch of [`UNIT_DELETE`] on `id`: the agent ends, and
    /// its piece, with its orders; after `DeleteAgent`, the agent only. It
    /// names the id as an integer (gpl agent delete;
    /// `UnitDelete(integer agentID)` in the SDK).
    fn unit_deleted(&mut self, id: u32) {
        self.agents.delete(id);
        self.dispatches.deleted(id);

        // `DeleteAgent`'s unit stands ([`World::orphan`]).
        if !self.orphaned(id) {
            self.unit_goes(id);
        }
    }

    /// Agent `id`'s piece gone, with its orders and all else kept per
    /// unit by agent: the engine held each on the unit it deletes.
    fn unit_goes(&mut self, id: u32) {
        self.pieces.remove(id);
        self.dispatches.unlist(Listed::Unit(id));
        self.unit_clones_go(id);

        self.units.remove(id);

        self.pending.timeouts.spend(id);
    }

    /// A `UnitDelete` not dispatched, the GPL halted ([`Gpl::Halted`]):
    /// the piece goes, the agent stays (**Inferred**).
    pub fn unit_gone(&mut self, args: &[Value]) {
        if let Some(id) = action::id_arg(args).filter(|id| !self.orphaned(*id)) {
            self.unit_goes(id);
        }
    }

    /// A builtin the log answers is about to run. A `SpawnUnit` gives the
    /// piece it announces, for [`World::announce`] while it runs:
    ///
    /// - its spawner's owner, or an integer argument's;
    /// - the key and value pairs of a list argument, attr set each.
    ///
    /// **Inferred**: a piece no `SpawnUnit` announces was made by player
    /// input, the crown's.
    ///
    /// A `DeleteAgent` leaves its piece standing (`World::orphan`).
    pub fn calling(&mut self, vm: &Vm<'_>, call: &Call<'_>) -> Option<Piece> {
        let name = call.builtin().name;
        let values: Vec<Option<&Value>> = call.args.iter().skip(1).map(|h| vm.value(*h)).collect();

        if name == SPAWN_UNIT {
            let spawner = match values.first() {
                Some(Some(Value::Agent(id))) => self.pieces.owner(*id),
                _ => None,
            };
            let given = values.iter().skip(2).find_map(|v| match v {
                Some(Value::Integer(n)) => Some(n.cast_unsigned()),
                _ => None,
            });
            let listed = || {
                values
                    .iter()
                    .skip(2)
                    .filter_map(|v| match v {
                        Some(Value::List(items)) => Some(pairs(items)),
                        _ => None,
                    })
                    .flatten()
                    .collect()
            };
            let made = match values.get(1) {
                Some(Some(Value::String(unit))) => self.spawned(vm, call.args, unit),
                _ => None,
            };
            let made = made.unwrap_or_else(|| Piece {
                attributes: listed(),
                ..Piece::default()
            });

            // Unnamed and unspawned: slot 0's, on every machine.
            Some(Piece {
                owner: given.or(spawner).unwrap_or(CROWN),
                ..made
            })
        } else {
            self.log_change(name, &values);
            None
        }
    }

    /// What a log-answered `SetOwner`, `SetTeam` or `DeleteAgent` changes
    /// here ([`World::calling`]).
    fn log_change(&mut self, name: &str, values: &[Option<&Value>]) {
        if name == SET_OWNER
            && let (Some(Some(Value::Agent(id))), Some(Some(Value::Integer(n)))) =
                (values.first(), values.get(1))
            && let Some(p) = self.pieces.get_mut(*id)
        {
            p.owner = n.cast_unsigned();

            // Counted for the humans, then method numbers
            // it anew.
            let code = p.code;

            self.tally_converted(code);
            self.number(*id);
        } else if name == SET_TEAM
            && let (Some(Some(Value::Agent(id))), Some(Some(Value::Integer(team)))) =
                (values.first(), values.get(1))
            && let Some(owner) = self.pieces.owner(*id)
        {
            self.players.join(owner, *team);
        } else if name == DELETE_AGENT
            && let Some(Some(Value::Agent(id))) = values.first()
        {
            self.orphan(*id);
        }
    }

    // -----------------------------------------------------------------------
    // The builtins

    /// `RetrieveAgent(name)`, gpl retrieve agent: the name map,
    /// then the agent map; the null agent when either misses. The result
    /// through slot 6. **Game**: `"GPLAIRoot"` finds `GplAIRoot`, so the name
    /// map folds case.
    fn retrieve(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let name = string_ref(vm, args, 1)?;
        let id = self.agents.named(&name).unwrap_or(NULL_AGENT);
        let ret = vm.arg(args, 0)?;

        ops::assign(vm.object_mut(ret), &Value::Agent(id)).map_err(Fault::from)
    }

    /// `CreateAgent(data, name)`, gpl create agent: argument 2 is no name when
    /// null. Gpl agent create makes the agent under the next id from `.dat`
    /// instance `data` and maps a non-empty name to it. The return slot is
    /// never written: it stays the null agent.
    fn create(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let second = vm.arg(args, 2)?;
        let name = match vm.object(second) {
            Value::Null => String::new(),
            _ => string_ref(vm, args, 2)?,
        };
        let data = string_ref(vm, args, 1)?;
        let agent = Agent::made(vm.program(), &data)?;
        let id = self.agents.next_id();

        self.agents.advance();
        self.agents.insert(
            id,
            Agent {
                name: name.clone(),
                ..agent
            },
        );
        self.agents.note_made(id);

        if !name.is_empty() {
            self.agents.name(&name, id);
        }

        Ok(())
    }

    /// `HasAttribute(name, agent)`, gpl has attribute: throws under two
    /// arguments; false for a missing or dead agent, whose name is not read;
    /// else whether its attribute map holds the name.
    fn has(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        if args.len() < HAS_ATTRIBUTE_ARGS {
            Err(Fault::throw(
                "HasAttribute() expected an agent and a string as parameters",
            ))?;
        }

        let id = agent_ref(vm, args, 2)?;
        let has = match self.agents.get(id) {
            Some(agent) => {
                let name = string_ref(vm, args, 1)?;
                agent.attributes.contains(&name.to_ascii_lowercase())
            }
            None => false,
        };
        let ret = vm.arg(args, 0)?;

        put_bool(vm, ret, has)
    }

    /// The piece agent `id` drives, as gpl agentref unit finds it.
    fn unit(&self, id: u32) -> Option<&Piece> {
        self.agents.get(id).and(self.pieces.get(id))
    }

    /// `GetAttribute(agent, key)`, gpl get attribute: throws when the unit is gone
    /// or the key unregistered; else the unit's value, 0 when it lacks the
    /// key (attr get, default pushed 0 in gpl get attribute).
    ///
    /// Unknown for a key a spawned piece lacks: its descriptor's values are
    /// not held here.
    fn get(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let key = int_ref(vm, args, 2)?;
        let piece = self.unit(id).ok_or_else(|| {
            Fault::throw(format!(
                "Function $GetAttribute( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            ))
        })?;

        if !registered(key) {
            Err(Fault::throw(format!(
                "Function $GetAttribute( agent#{}, #{key:x} ) unknown attribute",
                id.cast_signed()
            )))?;
        }

        let value = piece.attributes.get(&key).copied();
        let ret = vm.arg(args, 0)?;

        match (value, self.units.followed(id) && !self.complete(id)) {
            (Some(v), _) => put_int(vm, ret, v).map(|()| Answer::Given),
            (None, false) => put_int(vm, ret, 0).map(|()| Answer::Given),
            (None, true) => Ok(Answer::Unknown),
        }
    }

    /// `SetAttribute(agent, key, value)`, gpl set attribute: the same throws as
    /// `GetAttribute`, then the unit's write. `BuildingRec` and
    /// `VehicleRec` share `GSUnit` there: overwrite or
    /// insert, and hit points below 1 dispatch the death script
    /// ([`World::write`]).
    fn set(&mut self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let id = agent_ref(vm, args, 1)?;
        let key = int_ref(vm, args, 2)?;
        let value = int_ref(vm, args, 3)?;

        if self.unit(id).is_none() {
            Err(Fault::throw(format!(
                "Function $SetAttribute( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            )))?;
        }

        if !registered(key) {
            Err(Fault::throw(format!(
                "Function $SetAttribute( agent#{}, #{key:x}, {value} ) failed: unknown attribute",
                id.cast_signed()
            )))?;
        }

        let source = self.damage_owner(vm, id);

        if key == combat::poison_key() {
            self.poison_set(vm, id, value);
        }

        self.write_from(id, key, value, source);
        Ok(())
    }

    /// `IsValidGamePiece(agent)`, gpl is valid game piece: the result slot first, then
    /// gpl agentref valid: the agent is alive and its handle finds a unit.
    fn valid(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let ret = vm.arg(args, 0)?;
        let id = agent_ref(vm, args, 1)?;
        let valid = self.unit(id).is_some();

        put_bool(vm, ret, valid)
    }

    /// `GetUnitPlayerNumber(agent)`, gpl get unit player number: throws when the unit is
    /// gone; else its owner.
    fn owner(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let id = agent_ref(vm, args, 1)?;
        let owner = self
            .unit(id)
            .map(|p| p.owner)
            .ok_or_else(|| Fault::throw("GetUnitPlayerNumber() unit is null"))?;
        let ret = vm.arg(args, 0)?;

        put_int(vm, ret, owner.cast_signed())
    }

    /// `GetBuildingLevel(agent)`, openregent's own
    /// ([`builtins::OWN`]): the completed level, as the spell panel reads
    /// it ([`offer::stage`]): the code's fourth letter as a number, 1 where
    /// not a digit, one less while a stage is built. So 0 in the first
    /// construction, and N while upgrading from N. 0 for a unit not a
    /// building. Throws when the unit is gone, as `GetUnitPlayerNumber`
    /// does.
    fn level(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let id = agent_ref(vm, args, 1)?;
        let piece = self
            .unit(id)
            .ok_or_else(|| Fault::throw("GetBuildingLevel() unit is null"))?;
        let level = match piece.code.0[3] {
            _ if !piece.is_building() => 0,
            b'1'..=b'9' => offer::stage(piece),
            _ if piece.word(CURRENT_STAGE_BUILT) == 0 => 0,
            _ => 1,
        };
        let ret = vm.arg(args, 0)?;

        put_int(vm, ret, level)
    }

    /// `BuildingIsRecruiting(agent)`, gpl building is recruiting: throws when the unit is
    /// gone; else whether its [`BUTTON_DELAY_TIME`] attribute is not 0.
    fn recruiting(&self, vm: &mut Vm<'_>, args: &[Handle]) -> std::result::Result<(), Fault> {
        let id = agent_ref(vm, args, 1)?;
        let piece = self.unit(id).ok_or_else(|| {
            Fault::throw(format!(
                "Function $BuildingIsRecruiting( agent#{} ) failed: unit no longer exists",
                id.cast_signed()
            ))
        })?;
        let on = piece.get(BUTTON_DELAY_TIME).is_some_and(|v| v != 0);
        let ret = vm.arg(args, 0)?;

        put_bool(vm, ret, on)
    }

    /// `GetSpecialEvent1Script` and `2`:
    /// event `which`'s script, into the string result.
    fn event(
        &self,
        vm: &mut Vm<'_>,
        args: &[Handle],
        which: usize,
    ) -> std::result::Result<(), Fault> {
        let ret = vm.arg(args, 0)?;

        match vm.object_mut(ret) {
            Value::String(s) => {
                s.clone_from(&self.level.events[which]);
                Ok(())
            }
            _ => Err(OpError::BadConversion(Accessor::StringRef).into()),
        }
    }
}

/// A world builtin that always answers.
fn given(ran: std::result::Result<(), Fault>) -> std::result::Result<Answer, Fault> {
    ran.map(|()| Answer::Given)
}

/// `GetVictoryConditionIndex`, `GetVictoryConditionModifier`: a level
/// word in the result.
fn level_word(vm: &mut Vm<'_>, args: &[Handle], word: i32) -> std::result::Result<(), Fault> {
    let ret = vm.arg(args, 0)?;

    put_int(vm, ret, word)
}

// ---------------------------------------------------------------------------
// Space: where pieces stand, what a search finds

/// `AE:0` on a unit with no agent.
pub const NO_AGENT_ID: u32 = 0;

/// A piece as gpl agentref unit finds it.
enum Found<'a> {
    Piece(&'a Piece),
    /// No live agent or no unit: the builtins throw.
    Gone,
    /// A new game's piece the engine has not announced: the world has no
    /// agent for it yet.
    Unsure,
}

/// A `SpawnUnit` attribute list's key and value pairs.
fn pairs(items: &[Value]) -> Vec<(i32, i32)> {
    items
        .as_chunks::<2>()
        .0
        .iter()
        .filter_map(|p| match p {
            [Value::Integer(k), Value::Integer(v)] => Some((*k, *v)),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Accessors

/// Argument `i` through `string&`.
fn string_ref(vm: &Vm<'_>, args: &[Handle], i: usize) -> std::result::Result<String, Fault> {
    match vm.object(vm.arg(args, i)?) {
        Value::String(s) => Ok(s.clone()),
        _ => Err(OpError::BadConversion(Accessor::StringRef).into()),
    }
}

/// Argument `i` through `bool&`.
fn bool_ref(vm: &Vm<'_>, args: &[Handle], i: usize) -> std::result::Result<bool, Fault> {
    match vm.object(vm.arg(args, i)?) {
        Value::Boolean(b) => Ok(*b),
        _ => Err(OpError::BadConversion(Accessor::BoolRef).into()),
    }
}

/// Argument `i` through `gplagentref&`.
fn agent_ref(vm: &Vm<'_>, args: &[Handle], i: usize) -> std::result::Result<u32, Fault> {
    Ok(agent_of(vm.object(vm.arg(args, i)?))?)
}

fn list_of(v: &Value) -> std::result::Result<&[Value], Fault> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(OpError::BadConversion(Accessor::ListRef).into()),
    }
}

fn put_bool(vm: &mut Vm<'_>, o: ObjId, v: bool) -> std::result::Result<(), Fault> {
    match vm.object_mut(o) {
        Value::Boolean(b) => {
            *b = v;
            Ok(())
        }
        _ => Err(OpError::BadConversion(Accessor::BoolRef).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::RETURN_AMOUNT;
    use crate::gpl::builtins;

    /// A deleted unit leaves nothing behind: its record and its place in
    /// the lists kept per unit go, as the engine's unit, which held each,
    /// does. Its spell timeout stays, spent.
    #[test]
    fn a_deleted_unit_leaves_no_per_unit_entry() {
        const ID: u32 = 7;

        let mut world = World::default();

        world.units.set_motion(ID, places::Motion::Still);

        let u = world.units.entry(ID);

        u.tracks = None;
        u.direction = Some(1);
        u.jumps = 1;
        u.tint = Some(draw::Tint::default());
        u.overhead = Some(draw::Overhead::Hidden);
        u.played = Some(1);
        u.skipped = true;
        u.looping = true;
        u.given = Some(naming::Given::Text {
            index: None,
            text: String::new(),
        });
        u.number = Some(1);
        u.spelled = Some(100);
        u.sighted = Some((((0, 0), 0), [0; 4]));
        u.passed = Some((((Fixed::ZERO, Fixed::ZERO), 0), [0; 4]));
        u.words = Some([0; reveal::PLAYERS]);
        u.relisted = Some(1);
        u.flag_list = Some(1);
        world.pending.timeouts.set(ID, 100);
        world.units.enter(ID);

        world.unit_goes(ID);

        let kept = [
            world.units.get(ID).is_some(),
            world.units.entry_order(ID).is_some(),
        ];

        assert_eq!(kept, [false; 2]);
        assert!(!world.timing_out(ID));
        assert_eq!(world.expiry(ID), Some(100));
    }

    #[test]
    fn every_checked_builtin_is_registered() {
        for (name, _) in Checked::ALL {
            assert!(builtins::find(name).is_some(), "{name}");
        }
    }

    #[test]
    fn a_checked_builtin_is_named_as_registered() {
        for (name, c) in Checked::ALL {
            assert_eq!(c.name(), name);
        }

        assert_eq!(
            Checked::DwarvesVoiceSetOperative.name(),
            "DwarvesVoice_SetOperative"
        );
    }

    #[test]
    fn a_dat_text_reads_as_its_members_class() {
        let program = Program::load::<&[u8]>(&[]).expect("empty program");
        let read = |kind: Kind, text: &str| from_text(&program, kind.code(), text);
        let empty = builtins::HD
            .iter()
            .position(|b| b.name.is_empty())
            .and_then(|b| u32::try_from(b).ok())
            .map(|b| 1 + b);

        assert_eq!(read(Kind::Integer, "20"), Ok(Value::Integer(20)));
        assert_eq!(read(Kind::Integer, ""), Ok(Value::Integer(0)));
        assert!(read(Kind::Integer, "-5").is_err());
        assert_eq!(
            read(Kind::Integer, "ATTRIB_ReturnAmount"),
            Ok(Value::Integer(RETURN_AMOUNT.packed()))
        );
        assert_eq!(read(Kind::Boolean, "True"), Ok(Value::Boolean(true)));
        assert_eq!(read(Kind::Boolean, "0"), Ok(Value::Boolean(false)));
        assert!(read(Kind::Boolean, "yes").is_err());
        assert_eq!(read(Kind::Float, "0.9"), Ok(Value::Float(Gfix(921))));
        assert_eq!(read(Kind::Coord, "3 -4"), Ok(Value::Coord(3, -4)));
        assert_eq!(
            read(Kind::List, "a b"),
            Ok(Value::List(
                vec![Value::String("a".into()), Value::String("b".into())].into()
            ))
        );
        assert_eq!(read(Kind::Agent, "x"), Ok(Value::Agent(NULL_AGENT)));
        assert_eq!(
            read(Kind::Function, "no_such_function"),
            Ok(Value::Function(ops::NULL_FUNCTION))
        );
        assert_eq!(read(Kind::Function, "").ok(), empty.map(Value::Function));
    }

    #[test]
    fn a_new_level_puts_slot_seven_on_a_team_of_its_own() {
        let players = players::Players::new(Vec::new());
        let teams: Vec<usize> = (0..8).map(|p| players.team_of(p)).collect();

        assert_eq!(teams, [0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(search::Modifier::ALL.len(), search::MODIFIERS.len());
    }

    #[test]
    fn a_key_is_bank_then_index() {
        assert_eq!(Key::new(*b"APB", 18).packed(), 306_335_809);
        assert!(registered(Key::new(*b"LUC", 8).packed()));
        assert!(!registered(Key::new(*b"ZZZ", 0).packed()));
    }
}
