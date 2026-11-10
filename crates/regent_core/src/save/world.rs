//! The world's model: pieces, agents, players, ground and sight, orders
//! waiting, effects. Field by field the world's own; what it leaves out
//! is listed at `World::saved`.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::walk::{Look, Tracks};
use super::{Code, Value};

/// A GPL agent the world holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub attributes: BTreeSet<String>,
    pub title: Option<String>,
    pub name: String,
    pub values: Vec<Member>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub ty: u8,
    pub value: Value,
}

/// A game piece: owner, engine attributes, handle, descriptor, place.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Piece {
    pub owner: u32,
    pub attributes: BTreeMap<i32, i32>,
    pub handle: u32,
    pub code: Code,
    pub name: String,
    pub place: Option<Place>,
    pub spells: Option<Vec<Charge>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub kind: u32,
    pub fixed: (i32, i32),
    pub standing: (i32, i32),
    pub holder: u32,
    pub state: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Charge {
    pub code: Code,
    pub since: u32,
    pub timeout: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stand {
    pub fixed: (i32, i32),
    pub height: i32,
}

/// A record's set and its slot and frame (`world::scatter::Drawn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drawn {
    pub set: u32,
    pub drawing: (u16, u16),
}

/// A scatter object or terrain feature with no agent
/// (`world::scatter::Strewn`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strewn {
    pub code: Code,
    pub handle: u32,
    pub owner: Option<u32>,
    pub stand: Stand,
    pub standing: (i32, i32),
    pub drawn: Drawn,
    pub grid: OnGrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Motion {
    Still,
    Walking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ending {
    Wait,
    After,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effector {
    pub agent: u32,
    pub name: String,
    pub handle: u32,
    pub when: u32,
    pub duration: Option<u32>,
    pub end: Option<u32>,
    pub ending: Ending,
    pub ended: bool,
    pub value: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tint {
    pub colour: Option<u8>,
    pub transparency: Option<u8>,
}

/// A name generator's stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generator {
    pub counts: [u32; 4],
    pub seed: u32,
    pub drawn: u32,
    pub state: u32,
    pub list: Vec<u32>,
}

/// Heights per vertex, 16.16, `across` a row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeightMap {
    pub across: u32,
    pub heights: Vec<i32>,
}

/// Each cell's paint: near and far class, then blend case; and its
/// picture, variant and quarter, where known.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Painted {
    pub side: i32,
    pub cells: Vec<(u8, u8)>,
    pub pictures: Vec<Option<(u16, u16)>>,
}

/// An entry of list ORDR.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Listed {
    Unit(u32),
    Missile(u32),
    Effector(u32),
    Clone(u32),
}

/// A building order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildOrder {
    pub code: Code,
    pub at: (i32, i32),
    pub owner: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recruit {
    pub guild: u32,
    pub code: Code,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    pub code: Code,
    pub at: (i32, i32),
    pub owner: u32,
    pub reward: i32,
    pub target: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub handle: u32,
    pub text: i32,
    pub holder: u32,
    pub at: [i32; 3],
    pub made: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sight {
    Everyone,
    Owner,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wave {
    pub frames: u32,
    pub interval: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub handle: u32,
    pub code: Code,
    pub owner: i32,
    pub at: (i32, i32),
    pub made: u32,
    pub wave: Option<Wave>,
    pub sight: Sight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cast {
    pub code: Code,
    pub at: (i32, i32),
    pub owner: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Research {
    pub building: u32,
    pub item: u32,
    pub frame: u32,
}

/// Research ordered: its item by id in the research table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingResearch {
    pub order: Research,
    pub item: u32,
    pub time: i32,
    pub cost: i32,
    pub started: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assign {
    pub frame: u32,
    pub agent: u32,
    pub words: Vec<(i32, i32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Timer {
    Recruit(Recruit),
    Research,
    Enchant,
    Embassy(i32),
    MarketDay,
    Hall(u32),
    Gamble,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Count {
    Armed,
    Reset,
}

/// A building's GS timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timed {
    pub unit: u32,
    pub timer: Timer,
    pub when: u32,
    pub delta: u32,
    pub count: Count,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LairEntry {
    pub code: Code,
    pub weight: u32,
    pub force: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reread {
    pub fresh: Member,
    pub text: Option<Value>,
}

/// Explored ground: a flags byte per vertex, a bit per player.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explored {
    pub across: i32,
    pub flags: Vec<u8>,
    pub fresh: bool,
}

/// A piece as a sight pass last saw it: half cell, held bit, descriptor.
pub type Sighted = (((i32, i32), u8), [u8; 4]);

/// A piece as it last passed: 16.16 position, held bit, descriptor.
pub type Passed = Sighted;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sighting {
    Live,
    Frozen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spell {
    pub code: Code,
    pub name: String,
    pub effector: u32,
    pub timeout: u32,
    pub list: u32,
    pub level: u32,
    pub rank: u32,
    pub check: Option<String>,
}

/// A `GetBestSpell` waiting on a `ValidationScript`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asking {
    pub agent: u32,
    pub list: u32,
    pub level: i32,
    pub at: u64,
    pub best: Option<Spell>,
    pub candidate: Spell,
    pub script: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Spread {
    Scattered,
    Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Missile {
    pub code: Code,
    pub spread: Spread,
    pub set: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lob {
    pub velocity: [i32; 3],
    pub gravity: i32,
    pub floor: i32,
    pub falling: bool,
}

/// A missile in flight.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flight {
    pub handle: u32,
    pub missile: Missile,
    pub shooter: u32,
    pub target: Option<u32>,
    pub offset: [i32; 3],
    pub at: [i32; 3],
    pub goal: [i32; 3],
    pub step: i32,
    pub rate: u32,
    pub when: u32,
    pub launched: u32,
    pub lob: Option<Lob>,
    pub facing: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum End {
    Lost,
    Won,
}

/// Whether the session allows cheats, and the players each of the two
/// flags cheats set is on for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cheats {
    pub allowed: bool,
    pub every_level: BTreeSet<u32>,
    pub build: BTreeSet<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fall {
    pub start: u32,
    pub peak: u32,
    pub end: u32,
    pub fallen: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Picked {
    pub set: u16,
    pub since: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruins {
    pub falls: BTreeMap<u32, Fall>,
    pub rubble: BTreeMap<u32, u32>,
    pub orphans: BTreeSet<u32>,
    pub looks: BTreeMap<u32, Picked>,
    pub collapses: Vec<(u32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lifespan {
    pub emitter: Option<u32>,
    pub particle: u32,
}

/// A particle system standing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plume {
    pub handle: u32,
    pub code: Code,
    pub holder: u32,
    pub owner: u32,
    pub made: u32,
    pub at: [i32; 3],
    pub state: u8,
    pub since: u32,
    pub window: Option<u32>,
    pub spent: bool,
    pub wait: Option<(u32, u32)>,
    pub lifespan: Lifespan,
    pub emits: Option<u32>,
}

/// A missile playing its impact set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spent {
    pub handle: u32,
    pub code: Code,
    pub at: [i32; 3],
    pub facing: i32,
    pub since: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hung {
    pub code: Code,
    pub made: u32,
    pub base: [i32; 3],
    pub spot: [i32; 3],
    pub facing: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Doing {
    Standing,
    Walking,
    Waiting,
    Acting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Housed {
    Out,
    Held,
    Hung { on: u32, offset: [i32; 3] },
}

/// A piece as drawn: what a speed trail's clone shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shown {
    pub agent: u32,
    pub code: Code,
    pub handle: u32,
    pub owner: u32,
    pub at: (i32, i32),
    pub aloft: Option<i32>,
    pub facing: i32,
    pub doing: Doing,
    pub housed: Housed,
    pub stride: Option<u32>,
    pub walk_frame: Option<u32>,
    pub look: Option<Look>,
    pub ruined: bool,
    pub jumps: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Leader {
    Unit(u32),
    Clone(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub fixed: (i32, i32),
    pub shown: Shown,
    pub stamp: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Showing {
    Shown,
    Hidden,
}

/// A speed trail's clone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeedClone {
    pub handle: u32,
    pub unit: u32,
    pub leader: Leader,
    pub index: u32,
    pub code: Code,
    pub rate: u32,
    pub when: u32,
    pub delta: u32,
    pub shows: Seen,
    pub seen: Seen,
    pub shown: Showing,
    pub level: u8,
    pub colour: Option<u8>,
    pub deleting: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shake {
    pub min: i32,
    pub max: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quake {
    pub unit: u32,
    pub shake: Shake,
    pub runs: i32,
    pub when: u32,
}

/// Missiles in flight (`world::missile::Flights`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Flights {
    pub list: Vec<Flight>,
}

/// Particle systems and impact sets (`world::plume::Plumes`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Plumes {
    pub list: Vec<Plume>,
    pub spent: Vec<Spent>,
}

/// The music player's track and stop callback (`world::sound::Music`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Music {
    pub track: Option<i32>,
    pub stopped: Option<String>,
}

/// What stays of the drawing and audio side's (`world::Outbox`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outbox {
    pub marks: Vec<Mark>,
    pub messages: Vec<Message>,
    pub music: Music,
}

/// Attribute settings to come (`world::alter::Alters`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alters {
    pub assigns: Vec<Assign>,
}

/// Spell units' ends and which have passed (`world::spell::Timeouts`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeouts {
    pub ends: BTreeMap<u32, u32>,
    pub expired: BTreeSet<u32>,
}

/// What waits for its frame or its time (`world::Pending`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub research: Vec<PendingResearch>,
    pub alters: Alters,
    pub timers: Vec<Timed>,
    pub timeouts: Timeouts,
    pub rearming: BTreeSet<u32>,
    pub asking: Option<Asking>,
    /// Each player's camp move begun, by player.
    pub camps: BTreeMap<u32, CampMove>,
}

/// A camp move begun and not yet sent (`guild::camp::CampMove`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CampMove {
    pub guild: u32,
    pub price: i32,
}

/// What the frame moves beside units (`world::Effects`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Effects {
    pub flights: Flights,
    pub plumes: Plumes,
    pub clones: Vec<SpeedClone>,
    pub quakes: Vec<Quake>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Gpl {
    Running,
    Halted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Logging {
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breaks {
    pub countdown: Option<i32>,
    pub requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Given {
    Text { index: Option<i32>, text: String },
    Illusion(Code),
}

/// An advisor thread's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    pub last: u32,
    pub interval: i32,
    pub polled: u32,
    pub on: bool,
}

/// A human seat's advisor threads; their rules are the dataset's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advisors {
    pub treasury: Watch,
    pub trigger: i32,
    pub heroes: Watch,
    pub elves: Watch,
    pub dwarves: Watch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overhead {
    Shown,
    Hidden,
}

/// A player's statistics but vice (`world::Ledger`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub built: i32,
    pub destroyed: i32,
    pub hired: i32,
    pub killed: i32,
    pub danger: i32,
    pub made: i32,
    pub spent: [i32; 5],
    pub initial: i32,
    pub peak: i32,
}

/// A player's statistics' best hero (`world::score::Champion`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Champion {
    pub level: i32,
    pub owner: u32,
    pub name: String,
    pub class: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub disabled: BTreeSet<Code>,
    pub caps: BTreeMap<Code, i32>,
    pub graves: Vec<Vec<Grave>>,
}

/// The periodic hit point tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mend {
    pub period: u32,
    pub poison: i32,
    pub heal: i32,
    pub last: u32,
    pub count: i32,
}

/// Whether the world steps frames: the last frame's sight pass is owed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Frames {
    Unseen,
    Stepped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OnGrid {
    On,
    Off,
}

/// An engine dispatch: a function and its arguments.
pub type Dispatch = (String, Vec<Value>);

/// The engine dispatches waiting and list ORDR (`world::frame::Dispatches`):
/// what a frame's end holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dispatches {
    pub ordr: Vec<Listed>,
    pub queued: Vec<Dispatch>,
    pub deleting: Vec<u32>,
    pub count: u64,
}

/// The world at a frame's end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct World {
    pub agents: Agents,
    pub pieces: BTreeMap<u32, Piece>,
    pub map: BTreeMap<u32, Piece>,
    pub units: Units,
    pub now: u32,
    pub level: Level,
    pub outbox: Outbox,
    pub scripts: Scripts,
    pub board: Board,
    pub handle: u32,
    pub relations: Relations,
    pub dispatches: Dispatches,
    pub players: Players,
    pub effectors: Effectors,
    pub naming: Naming,
    pub frames: Frames,
    pub pending: Pending,
    pub lairs: Lairs,
    /// The world RNG's state.
    pub global: u32,
    pub fog: Fog,
    pub effects: Effects,
    pub cheats: Cheats,
    pub ruins: Ruins,
    /// By human seat.
    pub advisors: BTreeMap<u32, Advisors>,
    pub limits: Limits,
    /// Poison victim agent to attacker's owner.
    pub poison_sources: BTreeMap<u32, u32>,
}

/// The level's words, tick and inventory register (`world::Level`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    pub victory: i32,
    pub goal: i32,
    pub events: [String; 2],
    pub mending: Mend,
    pub items: BTreeMap<i32, Vec<u32>>,
}

/// `HOME` lists and flags' rosters, by handle
/// (`world::relations::Relations`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relations {
    pub homes: BTreeMap<u32, Vec<u32>>,
    pub seeking: BTreeMap<u32, Vec<u32>>,
}

/// The name generators and the start pieces to name
/// (`world::naming::Naming`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Naming {
    pub generators: Option<BTreeMap<u32, Generator>>,
    pub unnamed: Vec<u32>,
}

/// The GPL agents, their names, the next id, and what the host is still
/// to take (`world::agents::Agents`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Agents {
    pub by_id: BTreeMap<u32, Agent>,
    pub named: BTreeMap<String, u32>,
    pub next_id: u32,
    pub made: Vec<u32>,
    pub rereads: Vec<(u32, Vec<Reread>)>,
    pub removals: Vec<(u32, String)>,
    pub deleted: Vec<u32>,
}

/// The map's physical state (`world::board::Board`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub side: (i32, i32),
    pub ground: Option<HeightMap>,
    pub paint: Option<Painted>,
    pub shade: Option<Vec<u8>>,
    pub scatter: Vec<(Code, (i32, i32))>,
    pub stands: Vec<Stand>,
    pub strewn: Vec<Strewn>,
    pub looks: BTreeMap<u32, Drawn>,
    pub scenery: Vec<(Code, (i32, i32), OnGrid)>,
    pub relaid: u32,
    pub clearing: Vec<(Code, (i32, i32))>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hall {
    pub code: u32,
    pub length: i32,
    pub began: i32,
}

/// The level's players: their slots, the human seats, each one's record,
/// the team masks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Players {
    pub slots: Vec<u32>,
    /// Human seats, slots 0 up.
    pub session: u32,
    pub records: BTreeMap<u32, Player>,
    pub teams: [u32; 8],
}

/// One player's record (`world::players::Player`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub gold: Option<i32>,
    pub sharing: bool,
    pub hall: Option<Hall>,
    pub palace: Option<u32>,
    pub vice: i32,
    pub ledger: Ledger,
    pub champion: Champion,
    /// Slain monsters, by code word.
    pub slain: BTreeMap<u32, i32>,
    /// Counts, bought and gained, by code.
    pub counts: Option<[BTreeMap<Code, i32>; 2]>,
    pub end: Option<(End, (i32, i32))>,
    /// Dead heroes not yet in a graveyard.
    pub dead: Vec<Grave>,
}

/// A dead hero, as a graveyard lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grave {
    pub name: Who,
    pub code: Code,
    pub level: i32,
}

/// What names a unit: its code, `AP:1`, `APU:4`, number, own name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Who {
    pub code: Code,
    pub name: Option<u32>,
    pub table: Option<i32>,
    pub number: Option<i32>,
    pub given: Option<Given>,
}

/// The effectors made, where each hangs, which hung at the last step, and
/// the flags whose end script ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effectors {
    pub list: Vec<Effector>,
    pub hung: BTreeMap<u32, Hung>,
    pub hanging: BTreeSet<u32>,
    pub unhung: BTreeSet<u32>,
}

/// Every unit's record, and the order they entered buildings
/// (`world::unit::Units`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Units {
    pub records: BTreeMap<u32, Unit>,
    pub entered: Vec<u32>,
}

/// What the world keeps per unit beside its piece (`world::unit::Unit`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub motion: Option<Motion>,
    pub tracks: Option<Tracks>,
    pub direction: Option<u16>,
    pub jumps: u32,
    pub tint: Option<Tint>,
    pub overhead: Option<Overhead>,
    pub played: Option<u32>,
    pub skipped: bool,
    pub looping: bool,
    pub given: Option<Given>,
    pub number: Option<i32>,
    pub spelled: Option<u32>,
    pub sighted: Option<Sighted>,
    pub passed: Option<Passed>,
    pub words: Option<[i32; 8]>,
    pub relisted: Option<u32>,
    pub flag_list: Option<u8>,
}

/// Explored ground and what reveals it (`world::reveal::Fog`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fog {
    pub explored: Explored,
    pub seen: u8,
    pub blind: u8,
    pub sighting: Sighting,
    pub scatter_words: BTreeMap<(Code, (i32, i32)), [i32; 8]>,
    pub passes: u32,
}

/// The scripts' run state (`world::threads::Scripts`): whether threads
/// run, the log, `Breakpoint`'s.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scripts {
    pub gpl: Gpl,
    pub logging: Logging,
    pub breaks: Breaks,
}

/// Each spawner's lair data, by handle (`world::lairs::Lairs`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lairs {
    pub picking: BTreeMap<u32, Vec<LairEntry>>,
    pub special: BTreeMap<u32, Vec<Code>>,
}
