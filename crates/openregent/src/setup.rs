//! The game's setup: what makes a game, whoever asks for it.
//!
//! ```text
//!  command line (cli::Recorded) ──┐
//!  menus (Custom, a quest) ───────┼──> Setup ──> begin::Plan ──> the game
//!  play.input, LAN Welcome ───────┘       └──> recorded, sent (serde)
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use clap::ValueEnum;
use regent_core::data::build_tree::Tree;
use regent_core::data::fourcc::FourCc;
use regent_core::data::guid::Guid;
use regent_core::sim::freestyle::Picks;
use regent_core::sim::freestyle::{self, Victory};
use regent_core::sim::mapgen::custom;
use regent_core::sim::world::cheat::Cheats;
use serde::{Deserialize, Serialize};

/// The quiet day's GPL seed: the session word of the traced new game.
pub const SEED: u32 = 0x0b60_9e2c;

/// A Special Event's none.
const NO_EVENT: FourCc = FourCc::new(b"NONE");

/// A new game's victory (`--victory`): what the dialog offers; Last
/// Palace with more than one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Winning {
    Eliminate,
    Gold,
    Survive,
    /// With more than one player.
    LastPalace,
}

impl Winning {
    const fn victory(self) -> Victory {
        match self {
            Winning::Eliminate => Victory::Eliminate,
            Winning::Gold => Victory::Gold,
            Winning::Survive => Victory::Survive,
            Winning::LastPalace => Victory::LastPalace,
        }
    }

    /// The option for `victory`.
    pub const fn of(victory: Victory) -> Winning {
        match victory {
            Victory::Eliminate => Winning::Eliminate,
            Victory::Gold => Winning::Gold,
            Victory::Survive => Winning::Survive,
            Victory::LastPalace => Winning::LastPalace,
        }
    }
}

/// What makes a game: the options `cli::Recorded` parses, typed. A
/// recording keeps it, so a replay makes the same game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Kept", into = "Kept")]
pub struct Setup {
    /// The session's cheat rule; a save keeps its own when unset.
    pub cheats: Option<Cheats>,
    /// A scene file (`regent_core::sim::scene`).
    pub scene: Option<PathBuf>,
    /// The GPL session seed.
    pub seed: u32,
    /// A quest, by key, name or id.
    pub quest: Option<String>,
    /// The Map Number.
    pub map_seed: Option<i32>,
    pub land: Option<FourCc>,
    /// Cells a side.
    pub side: Option<u32>,
    pub settlement: Option<FourCc>,
    pub scenario: Option<FourCc>,
    /// Wandering Monsters, or `NONE`.
    pub monsters: Option<FourCc>,
    /// The two Special Events, `EVSC` keys or `NONE`.
    pub events: [FourCc; 2],
    /// Each player's starting gold.
    pub gold: u32,
    /// A new game's players, 1 to 4.
    pub players: u32,
    pub victory: Winning,
    /// Gold to pass or days to last; unset, the dialog's.
    pub goal: Option<u32>,
    /// Building Limitations: caps by building code; unset, none.
    pub limits: Option<BTreeMap<FourCc, i32>>,
    /// Mods, in load order.
    pub mods: Vec<Wanted>,
}

impl Default for Setup {
    /// The command line's with no option given.
    fn default() -> Setup {
        Setup {
            cheats: None,
            scene: None,
            seed: SEED,
            quest: None,
            map_seed: None,
            land: None,
            side: None,
            settlement: None,
            scenario: None,
            monsters: None,
            events: [NO_EVENT; 2],
            gold: freestyle::GOLD,
            players: freestyle::ONE_PLAYER,
            victory: Winning::Eliminate,
            goal: None,
            limits: None,
            mods: Vec::new(),
        }
    }
}

impl Setup {
    /// A quest map's word, the clock's stand-in: the Map Number, else
    /// the GPL seed (**Inferred**: both are clock reads in the engine).
    pub fn word(&self) -> i32 {
        self.map_seed.unwrap_or_else(|| self.seed.cast_signed())
    }

    /// The victory condition and its goal, the dialog's where not given;
    /// a single player's Last Palace, Eliminate
    /// ([`Victory::for_players`]).
    pub fn aim(&self) -> (Victory, u32) {
        let victory = self.victory.victory().for_players(self.players);

        (
            victory,
            self.goal.unwrap_or_else(|| victory.goal(self.gold)),
        )
    }

    /// The map options, each `None` where not given.
    pub const fn picks(&self) -> Picks {
        Picks {
            land: self.land,
            settlement: self.settlement,
            scenario: self.scenario,
            monsters: self.monsters,
            seed: self.map_seed,
            side: self.side,
            players: Some(self.players),
        }
    }

    /// The building caps a new game starts on; none given, none.
    pub fn caps(&self) -> BTreeMap<FourCc, i32> {
        self.limits.clone().unwrap_or_default()
    }

    /// A new game: no quest or scene.
    pub const fn is_new(&self) -> bool {
        self.quest.is_none() && self.scene.is_none()
    }

    /// The same, as a recording keeps it to make the game again: paths
    /// absolute; a new game's unpicked map codes, Map Number and goal
    /// their defaults; a quest's map word its Map Number; no empty caps.
    #[must_use]
    pub fn recorded(&self) -> Setup {
        let new = self.is_new();
        let path = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        };
        let (settlement, scenario, monsters, land, map_seed) = custom::DEFAULTS;
        let code = |given: Option<FourCc>, default| given.or(new.then_some(default));

        Setup {
            scene: path(&self.scene),
            map_seed: self
                .map_seed
                .or(new.then_some(map_seed))
                .or(self.quest.is_some().then(|| self.word())),
            land: code(self.land, land),
            settlement: code(self.settlement, settlement),
            scenario: code(self.scenario, scenario),
            monsters: code(self.monsters, monsters),
            goal: if new { Some(self.aim().1) } else { self.goal },
            limits: self.limits.clone().filter(|caps| !caps.is_empty()),
            ..self.clone()
        }
    }
}

/// [`Setup`] as it is written: codes, caps and mods as their text.
#[derive(Clone, Serialize, Deserialize)]
struct Kept {
    seed: u32,
    cheats: Option<bool>,
    scene: Option<PathBuf>,
    quest: Option<String>,
    map_seed: Option<i32>,
    side: Option<u32>,
    land: Option<String>,
    settlement: Option<String>,
    scenario: Option<String>,
    monsters: Option<String>,
    events: [String; 2],
    gold: u32,
    players: u32,
    victory: Winning,
    goal: Option<u32>,
    /// As [`Tree::caps_text`] writes them.
    limits: Option<String>,
    /// `{GUID}` or `{GUID}@hash` ([`Wanted`]).
    mods: Vec<String>,
}

impl From<Setup> for Kept {
    fn from(s: Setup) -> Kept {
        let text = |c: Option<FourCc>| c.map(|c| c.to_string());

        Kept {
            seed: s.seed,
            cheats: s.cheats.map(|c| c == Cheats::Allowed),
            scene: s.scene,
            quest: s.quest,
            map_seed: s.map_seed,
            side: s.side,
            land: text(s.land),
            settlement: text(s.settlement),
            scenario: text(s.scenario),
            monsters: text(s.monsters),
            events: s.events.map(|e| e.to_string()),
            gold: s.gold,
            players: s.players,
            victory: s.victory,
            goal: s.goal,
            limits: s.limits.map(|caps| {
                Tree {
                    caps,
                    ..Tree::default()
                }
                .caps_text()
            }),
            mods: s.mods.iter().map(ToString::to_string).collect(),
        }
    }
}

impl TryFrom<Kept> for Setup {
    type Error = String;

    fn try_from(k: Kept) -> Result<Setup, String> {
        let code = |c: Option<String>| c.as_deref().map(custom::code).transpose();
        let [first, second] = k.events;

        Ok(Setup {
            cheats: k.cheats.map(cheat_rule),
            scene: k.scene,
            seed: k.seed,
            quest: k.quest,
            map_seed: k.map_seed,
            land: code(k.land)?,
            side: k.side,
            settlement: code(k.settlement)?,
            scenario: code(k.scenario)?,
            monsters: code(k.monsters)?,
            events: [custom::code(&first)?, custom::code(&second)?],
            gold: k.gold,
            players: k.players,
            victory: k.victory,
            goal: k.goal,
            limits: k.limits.as_deref().map(Tree::caps_of).transpose()?,
            mods: k
                .mods
                .iter()
                .map(|text| Wanted::parse(text).ok_or_else(|| format!("mods: {text} is no GUID")))
                .collect::<Result<_, _>>()?,
        })
    }
}

/// The cheat rule `--cheats` names: allowed or barred.
pub const fn cheat_rule(allowed: bool) -> Cheats {
    if allowed {
        Cheats::Allowed
    } else {
        Cheats::Barred
    }
}

/// Between a mod's GUID and its images' hash in `--mods`.
const IMAGE_MARK: char = '@';

/// A mod `--mods` names: its GUID, and for one compiled from source the
/// hash of its images where recorded (`{GUID}@hash`, [`Setup::stamped`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub guid: Guid,
    pub image: Option<String>,
}

impl Wanted {
    /// From `{GUID}` or `{GUID}@hash`.
    pub fn parse(text: &str) -> Option<Wanted> {
        let (guid, image) = match text.split_once(IMAGE_MARK) {
            Some((guid, image)) => (guid, Some(image.trim().to_owned())),
            None => (text, None),
        };

        Guid::parse(guid).map(|guid| Wanted { guid, image })
    }
}

impl std::fmt::Display for Wanted {
    /// As [`Wanted::parse`] reads it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.image {
            Some(image) => write!(f, "{}{IMAGE_MARK}{image}", self.guid),
            None => write!(f, "{}", self.guid),
        }
    }
}

impl From<Guid> for Wanted {
    fn from(guid: Guid) -> Wanted {
        Wanted { guid, image: None }
    }
}
