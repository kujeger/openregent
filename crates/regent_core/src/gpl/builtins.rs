//! GPL's builtins, as `MajestyHD.exe` registers them.
//!
//! **Binary.** Two straight-line functions register every builtin by name into
//! one case-insensitive map, the one script functions join when a `.bcd`
//! loads. Per builtin:
//!
//! ```text
//! name     "CreateAgent"
//! map      the GPL AI's function map
//! context  0, 0
//! returns  5: agent
//! native   the builtin's function
//! ```
//!
//! The name, the function and the return type are all the binary records. No
//! argument list: a builtin reads its own arguments, by index, and converts
//! each through an accessor of `GplType`, one per type, whose defaults throw
//! "bad conversion to". The value types override the accessors they support.
//!
//! # The call path
//!
//! ```text
//! the executor: a switch on the opcode
//!   case opLParen (38)
//!     first run: return type by callee name
//!                a value of that type
//!                cached in the instruction
//!     every run: a frame, then the call runs
//!       record 0x14 (arguments, 0x0c, 0x10)  the function is invoked
//! ```
//!
//! Argument 0 is the return slot, 1 on the call's arguments. A script function
//! is the same record with the executor as its function and its code as the
//! two contexts; a builtin passes 0 for both.
//!
//! The return type is the return slot's type before the call. A builtin may
//! replace the slot: [`Builtin::retype`].
//!
//! # Facts per builtin
//!
//! Each [`HD`] row carries what else is known of the builtin: whether the VM
//! runs it ([`Native`]), the argument it writes, its RNG draws, how it
//! replaces its return slot, and for a world builtin its [`Group`]. A
//! [`BuiltinId`] indexes the table; [`id`] is the one lookup by name.
//!
//! # Arguments
//!
//! **Binary.** Each builtin's [`Signature`] was read from its code by a
//! test, since removed, which followed every path, and every helper an
//! argument is handed to:
//!
//! ```text
//! fetch argument k
//! a test of the value's type       narrows what the path takes
//! a call of a type's accessor      the type
//! a test of the argument count     what comes after is optional
//! a counter up to the count        a rest
//! a copy of the value              any type
//! ```
//!
//! A type is the union over paths, so an overloaded builtin's position
//! takes every overload's type there: `PlaySound`. Where the SDK reference
//! differs, the binary wins; see `docs/formats/bytecode.md`.
//!
//! # Editions
//!
//! HD adds eight builtins, the expansion thirteen;
//! `docs/original/editions.md` lists them. `tests/builtins.rs` holds the
//! registrations against the SDK's documentation.

use crate::data::gpltype::TypeCode;

/// One registration: a name bound to a native function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Builtin {
    /// As registered. Lookup folds case.
    pub name: &'static str,

    /// The return slot's type, or [`NOTHING`].
    pub returns: TypeCode,

    /// The arguments it reads.
    pub args: Signature,

    /// Run by the VM itself, touching no world state; `None` for one the
    /// host answers.
    pub native: Option<Native>,

    /// The argument it writes into, from 1. **SDK** (`GPL Reference.pdf`): a
    /// list filled, an "input variable" or "output value" set.
    /// `RemoveTitles` also removes from its argument 1 what it returns.
    pub writes: Option<usize>,

    /// Draws from the GPL RNG per call.
    ///
    /// **Binary**, the callers of gpl rng next: these three builtins,
    /// `SpawnUnit` once per off-map spawn,
    /// `IsoGenUnit` twice (engine code), `MessageManager` three
    /// times after each reseed (only with `GplPseudorandom`), and
    /// gpl rng orphan, which nothing references.
    ///
    /// **Game**, `local/traces/freestyle_seed1_S_hd_play3`: one seed and these
    /// counts give every checked `RandomNumber` result in the log, 577 of
    /// 589; nothing else drew in two days of play.
    pub draws: u32,

    /// How it replaces its return slot before writing it; `None` when it
    /// writes the slot it was given.
    pub retype: Option<Retype>,

    /// What a world builtin touches, and whether it changes the world.
    /// **Inferred** from each name, the SDK reference and the block that
    /// registers it; none is read from its native code. `None` on a native.
    pub group: Option<(Group, Effect)>,
}

/// A builtin's index in [`HD`], then past it in [`OWN`]. An engine
/// builtin's function handle is one more: the null record comes first.
/// An own builtin's is far past every definition's
/// (`Vm::named_handle`), so no engine handle moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BuiltinId(pub u16);

impl BuiltinId {
    /// Its row.
    ///
    /// # Panics
    ///
    /// On an id past [`OWN`].
    #[must_use]
    pub const fn get(self) -> &'static Builtin {
        if self.index() < HD.len() {
            &HD[self.index()]
        } else {
            &OWN[self.index() - HD.len()]
        }
    }

    /// Whether it is openregent's own ([`OWN`]), not the engine's.
    #[must_use]
    pub const fn own(self) -> bool {
        self.index() >= HD.len()
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Every builtin, in table order: [`HD`]'s, then [`OWN`]'s.
    pub fn all() -> impl Iterator<Item = BuiltinId> {
        (0..HD.len() + OWN.len()).filter_map(|i| u16::try_from(i).ok().map(BuiltinId))
    }
}

impl From<BuiltinId> for u32 {
    fn from(id: BuiltinId) -> u32 {
        u32::from(id.0)
    }
}

impl From<BuiltinId> for usize {
    fn from(id: BuiltinId) -> usize {
        id.index()
    }
}

/// The builtin the SDK documents that no engine registers: the reference
/// index lists `ReRandom`, nothing in any binary names it.
pub const UNREGISTERED: [&str; 1] = ["ReRandom"];

/// Every builtin the SDK documents: the 162 in the `GPL Reference.pdf`
/// index, plus 4 only the Notepad++ template lists. That is every
/// registration but the empty name, and [`UNREGISTERED`].
///
/// Two index entries are typos. They were the only unresolved corpus calls,
/// and the Notepad++ syntax file agrees with the corpus on both:
///
/// | Index spelling | Actual | Corpus uses |
/// |---|---|---|
/// | `AgentHasInventoryInItem` | `AgentHasInventoryItem` | 13 |
/// | `SetStoppedMusicCallback` | `SetMusicStoppedCallback` | 4 |
///
/// The reference's section heading for the first spells it correctly; only
/// the index is wrong. Names are the registry's spelling: the reference
/// writes `BreakPoint`.
///
/// ```
/// use regent_core::gpl::builtins;
///
/// assert_eq!(builtins::documented().count(), 166);
/// ```
pub fn documented() -> impl Iterator<Item = &'static str> {
    HD.iter()
        .map(|b| b.name)
        .filter(|name| !name.is_empty())
        .chain(UNREGISTERED)
}

/// The builtin registered under `name`, folding case as the engine does.
///
/// ```
/// use regent_core::gpl::builtins::{self, BuiltinId};
///
/// assert_eq!(builtins::id("NullFunction").map(BuiltinId::get).map(|b| b.name), Some("NullFunction"));
/// assert_eq!(builtins::id("ReRandom"), None);
/// ```
#[must_use]
pub fn id(name: &str) -> Option<BuiltinId> {
    HD.iter()
        .chain(&OWN)
        .position(|b| b.name.eq_ignore_ascii_case(name))
        .and_then(|i| u16::try_from(i).ok())
        .map(BuiltinId)
}

/// The builtins the VM runs itself: those touching no world state.
///
/// **Binary**, each from its native function in `MajestyHD.exe`; see
/// `gpl::vm::native`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Native {
    /// `NullFunction`, and the empty name bound to its function: nothing.
    Nothing,
    RandomNumber,
    Sqrt,
    MakeCoord,
    GetX,
    GetY,
    DistanceBetweenCoords,
    ListSize,
    ListMember,
    RemoveListMember,
    ClearList,
    AddLists,
    Concatenate,
    NullAgent,
    ValidFunction,
    LookupFunction,
    DebugOut,
}

/// How a builtin replaces its return slot.
///
/// **Binary.** `GetPlayerData` gives `"palace"` an agent,
/// `gpl_value_of_type(5)`; `ListMember` a value of the member's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retype {
    /// A fresh value of this type.
    To(TypeCode),
    /// A fresh value of the copied value's type.
    Copied,
}

/// What a world builtin touches. **Inferred**, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// Agent lookup, create, delete, parent, type, name, visibility.
    Agents,
    /// Object lists and spatial queries.
    Queries,
    /// Moving, paths, teleports, buildings entered.
    Movement,
    /// Combat signals, missiles, death.
    Combat,
    Spells,
    Inventory,
    /// Player data, treasury, statistics, reward flags.
    Economy,
    /// Buildings, palaces, limits, guilds, unit types, titles.
    Buildings,
    /// Teams, owners, victory, the game's setup.
    Players,
    /// Map reveal, minimap, effectors, draw effects.
    Map,
    /// Fades, music, sound, voices, messages.
    Ui,
    Threads,
    /// Engine and GPL attributes.
    Attributes,
    /// Log and breakpoints.
    Debug,
}

/// Whether a world builtin changes the world. **Inferred**, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    /// Reads only; may write its result to an argument.
    Query,
    Changes,
}

use Effect::{Changes as C, Query as Q};
use Group::{
    Agents, Attributes, Buildings, Combat, Economy, Inventory, Map, Movement, Players, Queries,
    Spells, Threads, Ui,
};

/// What a builtin reads: arguments 1 on, as its code fetches them.
///
/// An argument it never reads is none of these, so a call may pass more;
/// `CastSpell`'s fourth, in 63 corpus calls, goes nowhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Signature {
    /// Read on every path.
    pub required: &'static [Types],

    /// Read only once the argument count is checked, after the required ones.
    pub optional: &'static [Types],

    /// Read in a loop to the last argument, each accepting these.
    pub rest: Option<Types>,
}

impl Signature {
    /// Whether a call with `n` arguments supplies every required one.
    ///
    /// ```
    /// use regent_core::gpl::builtins;
    ///
    /// let args = builtins::find("RandomCoord").unwrap().args;
    /// assert!(!args.accepts(0));
    /// assert!(args.accepts(1) && args.accepts(3));
    /// ```
    #[must_use]
    pub const fn accepts(&self, n: usize) -> bool {
        n >= self.required.len()
    }

    /// How many arguments it reads at most; `None` with a rest.
    #[must_use]
    pub const fn most(&self) -> Option<usize> {
        match self.rest {
            Some(_) => None,
            None => Some(self.required.len() + self.optional.len()),
        }
    }
}

/// A set of value types, one bit per [`TypeCode`]; bit 0 is a null value.
///
/// **Binary.** An argument's set is what the accessors it is read through
/// accept, narrowed by the type-code checks on the path. Only the by-value
/// `int` and `float` accessors take two types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Types(pub u16);

impl Types {
    /// The set of one type.
    #[must_use]
    pub const fn of(code: TypeCode) -> Types {
        Types(1 << code.code())
    }

    /// Both sets.
    #[must_use]
    pub const fn or(self, other: Types) -> Types {
        Types(self.0 | other.0)
    }
}

/// A null value, type code 0: what `gpl_value_of_type(0)` gives.
pub const NULL: Types = Types::of(NOTHING);
pub const INTEGER: Types = Types::of(TypeCode::Integer);
pub const FLOAT: Types = Types::of(TypeCode::Float);
pub const STRING: Types = Types::of(TypeCode::String);
pub const LIST: Types = Types::of(TypeCode::List);
pub const AGENT: Types = Types::of(TypeCode::Agent);
pub const BOOLEAN: Types = Types::of(TypeCode::Boolean);
pub const COORD: Types = Types::of(TypeCode::Coord);
pub const FUNCTION: Types = Types::of(TypeCode::Function);

/// What the by-value `int` and `float` accessors take.
pub const NUMBER: Types = INTEGER.or(FLOAT);

/// Any value: an argument copied, or only printed.
pub const ANY: Types = Types((1 << TypeCode::Reference.code()) - 1);

/// Return code for a builtin with no value: the slot is the shared `GplNull`
/// that gpl value of type hands out for type 0.
pub const NOTHING: TypeCode = TypeCode::Null;

/// Every registration in `MajestyHD.exe`, sorted by name, case folded.
///
/// 166: the 165 names of the SDK's documentation bar `ReRandom`, which nothing
/// registers, and the empty name. That one shares `NullFunction`'s function;
/// **Inferred**: so calling an unset function value does nothing.
///
/// Columns: name, function, return type, then the [`Signature`]: required,
/// optional, rest. Every signature is read from the code; none falls back on
/// the reference. A comment marks where the reference differs.
#[rustfmt::skip]
pub const HD: [Builtin; 166] = [
    builtin("", 0, &[], &[], None).runs(Native::Nothing),
    // The default value is copied, so any type.
    builtin("AddAttribute", 6, &[AGENT, STRING, STRING], &[ANY], None).grouped(Attributes, C),
    builtin("AddLists", 4, &[], &[], Some(LIST)).runs(Native::AddLists),
    builtin("AdjustAttribute", 1, &[AGENT, INTEGER, INTEGER], &[], None).grouped(Attributes, C),
    builtin("AdjustPlayerData", 0, &[AGENT, STRING, INTEGER], &[STRING], None).grouped(Economy, C),
    // An item by number or name; the reference says integer. A -1 is written back
    // through `int&`.
    builtin("AgentHasInventoryItem", 6, &[NUMBER.or(STRING), AGENT], &[], None).writing(1).grouped(Inventory, Q),
    // Takes an integer; the reference says an agent.
    builtin("AgentNumber", 5, &[INTEGER], &[], None).grouped(Agents, Q),
    // A string too; the reference says boolean or integer.
    builtin("Breakpoint", 0, &[INTEGER.or(STRING).or(BOOLEAN)], &[], None).grouped(Group::Debug, C),
    builtin("BuildingIsRecruiting", 6, &[AGENT], &[], None).grouped(Buildings, Q),
    builtin("CancelGuildMembership", 0, &[AGENT], &[], None).grouped(Buildings, C),
    builtin("CanDropInventoryItem", 6, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    // Hands its whole argument vector to `PerformAction`.
    builtin("CastSpell", 0, &[AGENT, STRING, AGENT.or(COORD)], &[], None).grouped(Spells, C),
    builtin("ChangeUnitType", 0, &[AGENT, STRING], &[], None).grouped(Agents, C),
    builtin("CheckEffector", 6, &[AGENT, STRING], &[], None).grouped(Map, Q),
    builtin("ClearDrawEffects", 0, &[AGENT], &[], None).grouped(Map, C),
    builtin("ClearEngineDeathFlags", 0, &[AGENT], &[], None).grouped(Combat, C),
    builtin("ClearFlag", 0, &[AGENT], &[], None).grouped(Economy, C),
    builtin("ClearList", 0, &[LIST], &[], None).runs(Native::ClearList),
    builtin("CombatSignal", 0, &[AGENT], &[], None).grouped(Combat, C),
    // Copies each argument, so any type.
    builtin("Concatenate", 4, &[], &[], Some(ANY)).runs(Native::Concatenate),
    // An item by number or name, as in every inventory builtin; the reference gives
    // seven of them integer only.
    builtin("CountInventoryItem", 1, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    // The name is read unchecked, a null taken for none; the reference calls it optional.
    builtin("CreateAgent", 5, &[STRING, NULL.or(STRING)], &[], None).grouped(Agents, C),
    // The options, by type code: integer, string.
    builtin("CreateEffector", 0, &[AGENT, STRING, INTEGER], &[], Some(INTEGER.or(STRING))).grouped(Map, C),
    builtin("CreateMissile", 0, &[STRING, AGENT, AGENT.or(COORD)], &[], None).grouped(Combat, C),
    // The third is an integer; the reference says boolean.
    builtin("CreateNewInventoryItem", 0, &[INTEGER.or(STRING), AGENT], &[INTEGER], None).grouped(Inventory, C),
    // Reads a third, an agent; the reference lists two.
    builtin("CreateSpellUnit", 5, &[AGENT, STRING, AGENT], &[], None).grouped(Spells, C),
    // Prints every argument, only with engine flags value 2 set or the integer
    // 911 first. The reference says one string.
    builtin("DebugOut", 0, &[], &[], Some(ANY)).runs(Native::DebugOut),
    builtin("DeclareLoss", 0, &[INTEGER.or(AGENT)], &[], None).grouped(Players, C),
    builtin("DeclareVictory", 0, &[INTEGER.or(AGENT)], &[AGENT.or(COORD)], None).grouped(Players, C),
    builtin("DeleteAgent", 0, &[AGENT], &[], None).grouped(Agents, C),
    builtin("DeleteAllEffectors", 0, &[AGENT], &[], None).grouped(Map, C),
    builtin("DeleteEffector", 0, &[AGENT, STRING], &[], None).grouped(Map, C),
    builtin("DeleteGamePiece", 0, &[AGENT], &[], None).grouped(Agents, C),
    builtin("DeleteInventoryItem", 0, &[INTEGER.or(STRING), AGENT], &[], None).grouped(Inventory, C),
    builtin("DisableUnitType", 0, &[STRING], &[], None).grouped(Buildings, C),
    builtin("DistanceBetweenAgents", 1, &[AGENT, AGENT], &[], None).grouped(Queries, Q),
    builtin("DistanceBetweenCoords", 1, &[COORD, COORD], &[], None).runs(Native::DistanceBetweenCoords),
    builtin("DwarvesVoice_SetOperative", 0, &[INTEGER], &[], None).grouped(Ui, C),
    builtin("ElvesVoice_SetOperative", 0, &[INTEGER], &[], None).grouped(Ui, C),
    builtin("EnableUnitType", 0, &[STRING], &[], None).grouped(Buildings, C),
    builtin("EnchantWizTower", 0, &[AGENT], &[], None).grouped(Spells, C),
    builtin("FadeIn", 0, &[AGENT, INTEGER], &[], None).grouped(Ui, C),
    builtin("FadeOut", 0, &[AGENT, INTEGER], &[], None).grouped(Ui, C),
    builtin("FillSpecialLairList", 1, &[AGENT, LIST], &[], None).writing(2).grouped(Queries, Q),
    builtin("FindInventoryItem", 5, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    builtin("ForgetSpell", 0, &[AGENT, STRING], &[], None).grouped(Spells, C),
    builtin("GetAttribute", 1, &[AGENT, INTEGER], &[], None).grouped(Attributes, Q),
    builtin("GetBestSpell", 3, &[AGENT, INTEGER], &[], None).grouped(Spells, Q),
    builtin("GetBoardExtents", 7, &[], &[], None).grouped(Queries, Q),
    builtin("GetBuildingContainer", 5, &[AGENT], &[], None).grouped(Buildings, Q),
    builtin("GetBuildingContents", 4, &[AGENT], &[], None).grouped(Buildings, Q),
    builtin("GetContainerAgent", 5, &[AGENT], &[], None).grouped(Buildings, Q),
    builtin("GetEntranceLoc", 7, &[AGENT], &[], None).grouped(Buildings, Q),
    builtin("GetFlag", 5, &[AGENT], &[], None).grouped(Economy, Q),
    builtin("GetInventoryItemAgentType", 3, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    builtin("GetInventoryItemDropType", 1, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    builtin("GetInventoryItems", 4, &[AGENT], &[], None).grouped(Inventory, Q),
    builtin("GetNearestHiddenCoord", 6, &[AGENT, COORD], &[], None).writing(2).drawing(1).grouped(Queries, Q),
    builtin("GetNextWayPoint", 7, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("GetParent", 5, &[AGENT], &[], None).grouped(Agents, Q),
    builtin("GetPlayerData", 1, &[AGENT, STRING], &[], None).retyping(Retype::To(TypeCode::Agent)).grouped(Economy, Q),
    builtin("GetPlayerTeamNumber", 1, &[AGENT], &[], None).grouped(Players, Q),
    builtin("GetSpecialEvent1Script", 3, &[], &[], None).grouped(Players, Q),
    builtin("GetSpecialEvent2Script", 3, &[], &[], None).grouped(Players, Q),
    builtin("GetSpellAttribute", 1, &[STRING, STRING], &[], None).grouped(Spells, Q),
    builtin("GetUnitPlayerNumber", 1, &[AGENT], &[], None).grouped(Players, Q),
    builtin("GetVictoryConditionIndex", 1, &[], &[], None).grouped(Players, Q),
    builtin("GetVictoryConditionModifier", 1, &[], &[], None).grouped(Players, Q),
    builtin("GetX", 1, &[COORD], &[], None).runs(Native::GetX),
    builtin("GetY", 1, &[COORD], &[], None).runs(Native::GetY),
    builtin("Halt", 0, &[], &[], None).grouped(Group::Debug, C),
    builtin("HasAttribute", 6, &[STRING, AGENT], &[], None).grouped(Attributes, Q),
    builtin("HasSpeedTrail", 6, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("HasWayPoints", 6, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("Hide", 0, &[AGENT, AGENT], &[INTEGER], None).grouped(Agents, C),
    builtin("IncrementStatCounter", 0, &[AGENT, STRING], &[], None).grouped(Economy, C),
    builtin("InsideBuilding", 6, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("IsAdjacent", 6, &[AGENT, AGENT], &[], None).grouped(Movement, Q),
    builtin("IsEnteringBuilding", 6, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("IsHidden", 6, &[AGENT], &[], None).grouped(Agents, Q),
    builtin("IsInventoryItem", 6, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    builtin("IsMessageFlagPresent", 6, &[INTEGER.or(STRING)], &[], None).grouped(Ui, Q),
    builtin("IsMoving", 6, &[AGENT], &[], None).grouped(Movement, Q),
    builtin("IsRunning", 6, &[FUNCTION], &[], None).grouped(Threads, Q),
    // The third is an integer; the reference says boolean.
    builtin("IsSpellAvailable", 6, &[AGENT, STRING], &[INTEGER], None).grouped(Spells, Q),
    builtin("IsValidGamePiece", 6, &[AGENT], &[], None).grouped(Agents, Q),
    builtin("KillThread", 0, &[FUNCTION], &[], None).grouped(Threads, C),
    builtin("LastMusicTrack", 1, &[], &[], None).grouped(Ui, Q),
    builtin("LearnSpell", 0, &[AGENT, STRING], &[BOOLEAN], None).grouped(Spells, C),
    builtin("ListMember", 5, &[LIST, INTEGER], &[], None).runs(Native::ListMember).retyping(Retype::Copied),
    // The modifiers, by type code: integer, string, coordinate.
    builtin("ListObjects", 1, &[AGENT, STRING, INTEGER, LIST], &[], Some(INTEGER.or(STRING).or(COORD))).writing(4).grouped(Queries, Q),
    builtin("ListPalaces", 4, &[], &[], None).grouped(Buildings, Q),
    builtin("ListSize", 1, &[LIST], &[], None).runs(Native::ListSize),
    builtin("ListTitles", 4, &[LIST, STRING], &[], None).grouped(Buildings, Q),
    builtin("LocalChatMessage", 0, &[INTEGER, INTEGER.or(STRING)], &[AGENT], None).grouped(Ui, C),
    builtin("LocationOf", 7, &[AGENT], &[], None).grouped(Queries, Q),
    builtin("LogOff", 0, &[], &[], None).grouped(Group::Debug, C),
    builtin("LogOn", 0, &[], &[], None).grouped(Group::Debug, C),
    builtin("LookupFunction", 8, &[STRING], &[], None).runs(Native::LookupFunction),
    builtin("MagicalAdjustAttribute", 1, &[AGENT, INTEGER, INTEGER], &[], None).grouped(Attributes, C),
    builtin("MakeCoord", 7, &[INTEGER, INTEGER], &[], None).runs(Native::MakeCoord),
    builtin("MakeInventoryAttribute", 1, &[INTEGER.or(STRING)], &[], None).grouped(Inventory, Q),
    // The message is optional; the reference says required.
    builtin("MessageFlag", 0, &[AGENT.or(COORD)], &[INTEGER.or(STRING)], None).grouped(Ui, C),
    builtin("MiniMapAnimation", 0, &[AGENT.or(COORD), STRING], &[], None).grouped(Map, C),
    // The modifier is optional, and an integer may follow; the reference requires the
    // one and omits the other.
    builtin("Move", 0, &[AGENT, AGENT.or(COORD)], &[STRING, INTEGER], None).grouped(Movement, C),
    builtin("NeutralTeamNumber", 1, &[], &[], None).grouped(Players, Q),
    builtin("NewTeamNumber", 1, &[], &[], None).grouped(Players, C),
    // The rest are copied into the thread: any type.
    builtin("NewThread", 0, &[FUNCTION, INTEGER], &[], Some(ANY)).grouped(Threads, C),
    builtin("NoDebugMessages", 0, &[], &[], None).grouped(Group::Debug, C),
    builtin("NullAgent", 5, &[], &[], None).runs(Native::NullAgent),
    builtin("NullFunction", 0, &[], &[], None).runs(Native::Nothing),
    builtin("Parent", 5, &[AGENT], &[], None).grouped(Agents, Q),
    builtin("PathCost", 2, &[COORD, COORD, STRING, BOOLEAN, AGENT], &[], None).writing(4).grouped(Movement, Q),
    builtin("PerformAction", 0, &[AGENT, STRING, AGENT.or(COORD)], &[], None).grouped(Agents, C),
    builtin("PlaceRewardFlag", 5, &[AGENT, INTEGER, INTEGER], &[], None).grouped(Economy, C),
    builtin("PlayMusic", 0, &[INTEGER], &[], None).grouped(Ui, C),
    // Overloads by the first argument's type and the count: (agent, phase, [float]),
    // (agent, description, phase, [float]), (description, phase, [integer]).
    builtin("PlaySound", 6, &[STRING.or(AGENT), STRING], &[NUMBER.or(STRING), FLOAT], None).grouped(Ui, C),
    // Both integers optional. **Inferred** from the reference: one is a radius, two a
    // minimum and a radius.
    builtin("RandomCoord", 7, &[AGENT.or(COORD)], &[INTEGER, INTEGER], None).drawing(1).grouped(Queries, Q),
    builtin("RandomNumber", 1, &[INTEGER], &[], None).runs(Native::RandomNumber).drawing(1),
    builtin("RemoveAllBuildingLimits", 0, &[], &[], None).grouped(Buildings, C),
    builtin("RemoveAttribute", 0, &[AGENT, STRING], &[], None).grouped(Attributes, C),
    builtin("RemoveBuildingLimit", 0, &[STRING], &[], None).grouped(Buildings, C),
    builtin("RemoveListMember", 0, &[LIST, INTEGER], &[], None).runs(Native::RemoveListMember),
    builtin("RemoveTitles", 4, &[LIST, STRING], &[], None).grouped(Buildings, C),
    builtin("ResumeThread", 0, &[FUNCTION], &[], None).grouped(Threads, C),
    builtin("RetrieveAgent", 5, &[STRING], &[], None).grouped(Agents, Q),
    builtin("RevealArea", 0, &[AGENT, COORD, INTEGER], &[], None).grouped(Map, C),
    builtin("RevealWholeMap", 0, &[INTEGER], &[], None).grouped(Map, C),
    builtin("RevertUnitType", 0, &[AGENT], &[], None).grouped(Agents, C),
    // As `NewThread`.
    builtin("RunThread", 0, &[FUNCTION, INTEGER], &[], Some(ANY)).grouped(Threads, C),
    // As `NewThread`.
    builtin("RunThreadOnce", 0, &[FUNCTION, INTEGER], &[], Some(ANY)).grouped(Threads, C),
    builtin("SetAttribute", 0, &[AGENT, INTEGER, INTEGER], &[], None).grouped(Attributes, C),
    builtin("SetBuildingLimit", 0, &[STRING, INTEGER], &[], None).grouped(Buildings, C),
    builtin("SetDrawEffects", 0, &[AGENT, STRING, INTEGER], &[], None).grouped(Map, C),
    // Read by value: a float converts.
    builtin("SetEffectorDirection", 0, &[AGENT, STRING, NUMBER], &[], None).grouped(Map, C),
    builtin("SetHallFlag", 0, &[AGENT], &[], None).grouped(Buildings, C),
    builtin("SetMusicStoppedCallback", 0, &[STRING], &[], None).grouped(Ui, C),
    builtin("SetPalace", 0, &[AGENT, INTEGER], &[], None).grouped(Buildings, C),
    builtin("SetParent", 0, &[AGENT, AGENT], &[], None).grouped(Agents, C),
    builtin("SetPlayerTeamNumber", 0, &[AGENT, INTEGER], &[], None).grouped(Players, C),
    builtin("SetShareReveal", 0, &[INTEGER, BOOLEAN], &[], None).grouped(Players, C),
    builtin("SetSpellUnitTimeout", 0, &[AGENT, INTEGER], &[], None).grouped(Spells, C),
    builtin("SetThreadInterval", 0, &[FUNCTION, INTEGER], &[], None).grouped(Threads, C),
    builtin("SetTreasuryValues", 0, &[INTEGER, INTEGER], &[], None).grouped(Economy, C),
    builtin("SetUnitPlayerNumber", 0, &[AGENT, INTEGER], &[], None).grouped(Players, C),
    builtin("ShowMinimap", 0, &[AGENT, BOOLEAN], &[], None).grouped(Map, C),
    builtin("SpawnGraveyard", 5, &[AGENT, STRING], &[COORD], None).grouped(Agents, C),
    // The options, in any order, by type code: coordinate, integer, string, list.
    builtin("SpawnUnit", 5, &[AGENT, STRING], &[], Some(INTEGER.or(STRING).or(LIST).or(COORD))).grouped(Agents, C),
    builtin("SpecifyIntent", 0, &[AGENT, INTEGER.or(STRING)], &[], None).grouped(Agents, C),
    // A string or its number; the reference says string.
    builtin("SpecifyName", 0, &[AGENT, INTEGER.or(STRING)], &[], None).grouped(Agents, C),
    // Read by value: an integer converts.
    builtin("Sqrt", 2, &[NUMBER], &[], None).runs(Native::Sqrt),
    builtin("StartEarthquakeSpell", 0, &[AGENT, INTEGER, INTEGER], &[INTEGER], None).grouped(Spells, C),
    builtin("StopEarthquakeSpell", 0, &[AGENT], &[], None).grouped(Spells, C),
    builtin("StopMoving", 0, &[AGENT], &[], None).grouped(Movement, C),
    builtin("SuspendThread", 0, &[FUNCTION], &[], None).grouped(Threads, C),
    builtin("TeleportToPoint", 0, &[AGENT, INTEGER, COORD], &[], None).grouped(Movement, C),
    builtin("TeleportToUnit", 0, &[AGENT, INTEGER, AGENT, INTEGER], &[], None).grouped(Movement, C),
    builtin("TransferInventoryItem", 0, &[INTEGER.or(STRING), AGENT, AGENT], &[], None).grouped(Inventory, C),
    builtin("TurnOffSpeedTrail", 0, &[AGENT], &[], None).grouped(Movement, C),
    builtin("TurnOnSpeedTrail", 0, &[AGENT, INTEGER], &[INTEGER], None).grouped(Movement, C),
    builtin("UnHide", 0, &[AGENT], &[COORD], None).grouped(Agents, C),
    builtin("UnitIsLocalPlayer", 6, &[AGENT], &[], None).grouped(Players, Q),
    builtin("UnitIsOffmapLair", 6, &[AGENT], &[], None).grouped(Agents, Q),
    builtin("UnitsAreSamePlayer", 6, &[AGENT, AGENT], &[], None).grouped(Players, Q),
    builtin("UpgradeAgentAttributes", 0, &[AGENT], &[], None).grouped(Attributes, C),
    builtin("ValidFunction", 6, &[FUNCTION], &[], None).runs(Native::ValidFunction),
];

/// openregent's own builtins, registered after [`HD`]'s for mods'
/// scripts. No shipped script calls or defines these names, so none
/// behaves otherwise. None has a native function in any binary.
///
/// | Builtin | Returns |
/// |---|---|
/// | `GetBuildingLevel(agent)` | its building's level, its code's fourth letter: 1 to 3 for the engine's; 1 for a code not ending in a digit, 0 for a unit not a building. The completed one: 0 while first built, N while upgrading from N |
#[rustfmt::skip]
pub const OWN: [Builtin; 1] = [
    builtin("GetBuildingLevel", 1, &[AGENT], &[], None).grouped(Buildings, Q),
];

const fn builtin(
    name: &'static str,
    returns: u8,
    required: &'static [Types],
    optional: &'static [Types],
    rest: Option<Types>,
) -> Builtin {
    Builtin {
        name,
        returns: TypeCode::of(returns),
        args: Signature {
            required,
            optional,
            rest,
        },
        native: None,
        writes: None,
        draws: 0,
        retype: None,
        group: None,
    }
}

// The facts a row adds to its registration.
impl Builtin {
    const fn runs(self, native: Native) -> Builtin {
        Builtin {
            native: Some(native),
            ..self
        }
    }

    const fn writing(self, arg: usize) -> Builtin {
        Builtin {
            writes: Some(arg),
            ..self
        }
    }

    const fn drawing(self, draws: u32) -> Builtin {
        Builtin { draws, ..self }
    }

    const fn retyping(self, retype: Retype) -> Builtin {
        Builtin {
            retype: Some(retype),
            ..self
        }
    }

    const fn grouped(self, group: Group, effect: Effect) -> Builtin {
        Builtin {
            group: Some((group, effect)),
            ..self
        }
    }
}

/// The builtin registered under `name`, folding case as the engine does.
///
/// ```
/// use regent_core::gpl::builtins;
///
/// assert_eq!(builtins::find("breakpoint").unwrap().name, "Breakpoint");
/// assert_eq!(builtins::find("ReRandom"), None);
/// ```
#[must_use]
pub fn find(name: &str) -> Option<&'static Builtin> {
    id(name).map(BuiltinId::get)
}
