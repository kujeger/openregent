//! The freestyle game's settings: the original's config, which the pick
//! page's presets write and the setup page (`setup`, for `MX17`) edits,
//! and what that page's lists offer ([`Choice`]) for the session's
//! players ([`Dialog`]).
//!
//! Single player does not offer Allow Cheats or a goal of the player's own (`AP89`).
//! Building Limitations lists `trees`; LOAD and SAVE keep `presets`.
//! Random, beside the Map Number, and the lists' Random picks are this
//! game's (`docs/openregent/enhancements.md`).
//!
//! The config outlives the session as the original's does: read at
//! start-up, written back but for the Map
//! Number. The original keeps it in the
//! registry and writes it at exit; this game in [`KEPT`], each time it
//! leaves the setup page or starts a game.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regent_core::data::build_tree::{self, Tree};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::sim::freestyle::{self, Band, List, ONE_PLAYER, RANDOM_LAND, Victory};
use regent_core::sim::mapgen::preset::{NO_EVENT, Preset};
use regent_core::sim::setup;
use regent_core::sim::world::cheat::Cheats;
use regent_core::sim::world::offer::UNAVAILABLE_MARK;
use serde::{Deserialize, Serialize};

use crate::files::presets;
use crate::files::trees::Trees;

/// The kept config's file in [`crate::files::env::OWN`], standing in for the
/// registry's `RGS*` records.
const KEPT: &str = "freestyle.toml";

/// `GMTX` "Gather %d gold" and "Survive for %d days".
const GOAL_TEXTS: [(Victory, usize); 2] = [(Victory::Gold, 124), (Victory::Survive, 126)];

/// The settings file's name the page shows, `GMTX` 411 "Default
/// Settings".
const SAVE_NAME_TEXT: usize = 411;

/// The difficulty's words: `freestyle::difficulty_step`'s five.
const DIFFICULTIES: usize = 5;

pub use regent_core::sim::mapgen::custom::MAP_NUMBER_MIN;

/// One entry of a list: its code, name and blurb.
#[derive(Clone, Debug)]
pub struct Entry {
    pub code: FourCc,
    pub name: String,
    pub detail: String,
}

/// The lists whose entries hang on the players: the dialog of that
/// many fills them so (freestyle dialog's count, freestyle dialog).
/// The events and the gold do not.
#[derive(Default)]
struct Counted {
    lands: Vec<Entry>,
    settlements: Vec<Entry>,
    scenarios: Vec<Entry>,
    monsters: Vec<Entry>,
}

/// None offered: [`Lists::default`]'s.
static NOTHING: Counted = Counted {
    lands: Vec::new(),
    settlements: Vec::new(),
    scenarios: Vec::new(),
    monsters: Vec::new(),
};

/// What the setup page lists, and the presets' event ratings.
#[derive(Default)]
pub struct Lists {
    /// By players, one to [`freestyle::MOST_PLAYERS`].
    counted: Vec<Counted>,
    events: Vec<Entry>,
    /// Every `EVSC` key and rating, table order.
    pub ratings: Vec<(FourCc, i32)>,
    /// The victory conditions' names, `GMTX` 120 on.
    victories: Vec<(Victory, String)>,
    /// The goal lines, `GMTX` 124 and 126, and each list's last entry
    /// ([`Victory::specified`]).
    goals: Vec<(Victory, String, String)>,
    /// [`freestyle::GOLD_TEXT`].
    gold_text: String,
    /// Building Limitations' trees.
    pub trees: Trees,
    /// The Build Tree Editor's rows.
    pub rows: regent_core::view::menu::tree::Rows,
    /// Where the original's `.BTD` files are read from beside ours: the
    /// install's `buildtree`.
    imports: Vec<PathBuf>,
    /// [`SAVE_NAME_TEXT`].
    pub save_name: String,
    /// The forces' ratings, by code: the Random picks and `NONE` have
    /// none.
    forces: Vec<(FourCc, freestyle::Ratings)>,
    /// The difficulty's words, `GMTX` 115 on.
    difficulties: Vec<String>,
}

impl Lists {
    /// From the freestyle quest under `hd` and `catalogue`'s tables, for
    /// each count of players, each list's Random picks first
    /// (`crate::game::load::randoms`).
    ///
    /// The Random picks are this game's: the original's lists offer none,
    /// only the
    /// presets write them (`docs/openregent/enhancements.md`). A Random force's blurb
    /// is its name, as the original sets for a tag picked
    /// (freestyle dialog, control 5001, `GMTX` 111–113).
    ///
    /// # Errors
    ///
    /// A quest that will not read.
    pub fn load(hd: &Path, catalogue: &Catalogue<'_>) -> Result<Lists, String> {
        let text = Text::build(catalogue);
        let gmtx = |i: usize| text.gmtx(i).map(|s| s.trim().to_owned());
        let detail = |code: FourCc| {
            text.scenario_detail(code)
                .map(|d| regent_core::data::text::plain(&d).trim().to_owned())
                .unwrap_or_default()
        };
        let list = |list: List, players: u32| -> Result<Vec<Entry>, String> {
            let none = (list == List::Monsters)
                .then(|| (NO_EVENT, gmtx(freestyle::NONE_TEXT).unwrap_or_default()));
            let named = |(code, name): (FourCc, String)| Entry {
                code,
                detail: name.clone(),
                name,
            };
            let offered = crate::game::load::offers(hd, list, players)?
                .into_iter()
                .map(|(code, name)| Entry {
                    code,
                    name,
                    detail: detail(code),
                });

            Ok(none
                .into_iter()
                .chain(crate::game::load::randoms(list, &text))
                .map(named)
                .chain(offered)
                .collect())
        };
        let ratings = setup::event_list(catalogue);
        let events = ratings
            .iter()
            .map(|&(code, _)| Entry {
                code,
                name: text
                    .event(code)
                    .map_or_else(|| code.to_string(), |n| n.trim().to_owned()),
                detail: text
                    .event_detail(code)
                    .map(|d| d.trim().to_owned())
                    .unwrap_or_default(),
            })
            .collect();
        let victories = Victory::ALL
            .into_iter()
            .map(|v| (v, gmtx(v.text()).unwrap_or_else(|| format!("{v:?}"))))
            .collect();
        let goals = GOAL_TEXTS
            .into_iter()
            .filter_map(|(v, i)| Some((v, gmtx(i)?, gmtx(v.specified()?)?)))
            .collect();

        let gold_text = gmtx(freestyle::GOLD_TEXT).unwrap_or_else(|| "%d Gold".to_owned());
        let imports = vec![hd.join(build_tree::DIRECTORY)];
        let trees = Trees::read(
            Tree::shipped(catalogue),
            crate::files::trees::place().as_deref(),
            &imports,
        );

        let counted = (ONE_PLAYER..=freestyle::MOST_PLAYERS)
            .map(|players| {
                Ok(Counted {
                    lands: list(List::Land, players)?,
                    settlements: list(List::Settlement, players)?,
                    scenarios: list(List::Scenario, players)?,
                    monsters: list(List::Monsters, players)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut codes: Vec<FourCc> = counted
            .iter()
            .flat_map(|c| [&c.settlements, &c.scenarios, &c.monsters])
            .flatten()
            .map(|e| e.code)
            .collect();

        codes.sort_by_key(|c| c.word());
        codes.dedup();
        let forces = codes
            .iter()
            .copied()
            .zip(crate::game::load::ratings(hd, &codes)?)
            .collect();
        let difficulties = (0..DIFFICULTIES)
            .map_while(|n| gmtx(freestyle::DIFFICULTY_TEXT + n))
            .collect();

        Ok(Lists {
            counted,
            events,
            ratings,
            victories,
            goals,
            gold_text,
            trees,
            rows: regent_core::view::menu::tree::Rows::of(catalogue),
            imports,
            save_name: gmtx(SAVE_NAME_TEXT).unwrap_or_default(),
            forces,
            difficulties,
        })
    }
}

/// The freestyle flow: the config, kept for the session as the original
/// keeps it, and where it is kept on disk.
#[derive(bevy::prelude::Resource, Default)]
pub struct Freestyle {
    pub custom: Custom,
    /// Where the config is kept; none, nowhere.
    kept: Option<PathBuf>,
}

impl Freestyle {
    /// The config kept at `path` read, and kept there as it changes. One
    /// that will not read is left alone, the defaults used.
    pub fn keep_at(&mut self, path: PathBuf) {
        match Custom::kept(&path) {
            Ok(custom) => {
                self.custom = custom;
                self.kept = Some(path);
            }
            Err(why) => bevy::log::warn!("{why}"),
        }
    }

    /// The config kept, if it has a place.
    pub fn keep(&self) {
        let failed = self.kept.as_ref().and_then(|p| self.custom.keep(p).err());

        if let Some(why) = failed {
            bevy::log::warn!("{why}");
        }
    }
}

/// The freestyle config: what a new game is made from.
/// The presets write it; the setup page shows and edits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Custom {
    pub settlement: FourCc,
    pub scenario: FourCc,
    pub monsters: FourCc,
    pub land: FourCc,
    pub map_number: i32,
    pub gold: u32,
    pub victory: Victory,
    /// The victory's goal: gold or days.
    pub goal: u32,
    pub events: [FourCc; 2],
    /// The multiplayer session's cheat rule; single player ignores it.
    pub cheats: Cheats,
    /// Building Limitations: the tree's name, a config word. Not kept:
    /// the registry keeps no name.
    pub buildings: String,
    /// The tree's caps, as the build-tree object holds them beside the
    /// name: loaded as the list fills and at a pick,
    /// written by the editor's SAVE.
    pub caps: BTreeMap<FourCc, i32>,
    /// The settings' name, a config word, shown on the setup page: the
    /// one last saved or loaded (`presets`); none, `GMTX` 411 "Default
    /// Settings". Not kept, as the registry keeps no name.
    pub name: Option<String>,
}

impl Default for Custom {
    /// The registry's defaults, `RGS1` to `RGS7`
    /// (`regent_core::data::settings::cases::PREFS`), no event (**Inferred**:
    /// the events' default is not read), Map Number 1 until the pick page
    /// rolls one.
    fn default() -> Custom {
        let beginner = Band::Beginner.tag();

        Custom {
            settlement: beginner,
            scenario: beginner,
            monsters: NO_EVENT,
            land: RANDOM_LAND,
            map_number: MAP_NUMBER_MIN,
            gold: freestyle::GOLD,
            victory: Victory::Eliminate,
            goal: 0,
            events: [NO_EVENT; 2],
            cheats: Cheats::Allowed,
            buildings: build_tree::DEFAULT.to_owned(),
            caps: BTreeMap::new(),
            name: None,
        }
    }
}

impl Custom {
    /// `preset` written over it, its events drawn on `clock` from
    /// `ratings`; the Map Number and
    /// the goal kept.
    pub fn preset(&mut self, preset: Preset, clock: i32, ratings: &[(FourCc, i32)]) {
        let picked = preset.picks(preset.gold(), clock, ratings);

        *self = Custom {
            settlement: picked.settlement,
            scenario: picked.scenario,
            monsters: picked.monsters,
            land: picked.land,
            gold: picked.gold,
            victory: picked.victory,
            events: picked.events,
            ..self.clone()
        };
    }
}

impl Custom {
    /// Fitted to what `dialog` offers, as the original's dialog writes
    /// its picks back as it fills: a force or land it does not list, its
    /// list's first, the
    /// monsters' `NONE` kept; a single player's Last Palace, Eliminate.
    /// A Random pick listed stays.
    ///
    /// ```text
    ///  Wizb (one player's), 2 players ──> the 2 players' list's first
    /// ```
    pub fn fit(&mut self, dialog: Dialog<'_>) {
        let counted = dialog.counted();

        fitted(&mut self.settlement, &counted.settlements);
        fitted(&mut self.scenario, &counted.scenarios);
        fitted(&mut self.monsters, &counted.monsters);
        fitted(&mut self.land, &counted.lands);
        self.victory = self.victory.for_players(dialog.players);
    }

    /// Its tree loaded from `trees`, as the list fills:
    /// the one of its name, else the
    /// first, `default`.
    pub fn resolve(&mut self, trees: &Trees) {
        let tree = trees
            .named(&self.buildings)
            .or_else(|| trees.shipped.first())
            .cloned()
            .unwrap_or_default();

        self.buildings = tree.name;
        self.caps = tree.caps;
    }

    /// The caps as text ([`Tree::caps_text`]).
    #[cfg(test)]
    #[must_use]
    pub fn caps_text(&self) -> String {
        Tree {
            caps: self.caps.clone(),
            ..Tree::default()
        }
        .caps_text()
    }
}

impl Custom {
    /// The dialog's difficulty word for it ([`freestyle::difficulty`]):
    /// the forces' ratings by its victory, its events' `EVSC` numbers,
    /// its gold.
    #[must_use]
    pub fn difficulty<'l>(&self, lists: &'l Lists) -> Option<&'l str> {
        let force = |code: FourCc| {
            lists
                .forces
                .iter()
                .find(|(c, _)| *c == code)
                .map_or(freestyle::Ratings::UNRATED, |(_, r)| *r)
                .by(self.victory)
        };
        let event = |code: FourCc| {
            lists
                .ratings
                .iter()
                .find(|(c, _)| *c == code)
                .map_or(0, |(_, r)| *r)
        };
        let forces = [self.settlement, self.scenario, self.monsters].map(force);
        let difficulty = freestyle::difficulty(self.gold, forces, self.events.map(event));

        lists
            .difficulties
            .get(freestyle::difficulty_step(difficulty))
            .map(String::as_str)
    }
}

/// [`KEPT`]: the config but its Map Number, as the registry keeps it
/// (`RGS1` to `RGS7`, the two `RGS_SpecialEvent`). Not kept: `RGS8`,
/// `RGS9`, which the page does not offer.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    settlement: String,
    scenario: String,
    monsters: String,
    land: String,
    gold: u32,
    /// [`Victory::word`].
    victory: u32,
    goal: u32,
    events: [String; 2],
}

/// Where the config is kept: [`KEPT`] in the XDG data directory, as the
/// profiles.
#[must_use]
pub fn kept_place() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(KEPT))
}

impl Custom {
    /// The config kept at `path`, on Map Number 1 till the pick page
    /// rolls one; none kept yet, the defaults.
    ///
    /// # Errors
    ///
    /// A file that will not read, of another version, or holding a code
    /// not four bytes or a victory not known.
    pub fn kept(path: &Path) -> Result<Custom, String> {
        if path.exists() {
            let kept: Kept = crate::files::store::read(path)?;

            crate::files::store::checked(kept.version, path)?;
            Custom::of(&kept).map_err(|e| format!("{}: {e}", path.display()))
        } else {
            Ok(Custom::default())
        }
    }

    /// Keep it at `path`, but its Map Number.
    ///
    /// # Errors
    ///
    /// A directory or file that will not be written.
    pub fn keep(&self, path: &Path) -> Result<(), String> {
        let kept = Kept {
            version: crate::files::store::VERSION,
            settlement: self.settlement.to_string(),
            scenario: self.scenario.to_string(),
            monsters: self.monsters.to_string(),
            land: self.land.to_string(),
            gold: self.gold,
            victory: self.victory.word(),
            goal: self.goal,
            events: self.events.map(|e| e.to_string()),
        };

        crate::files::store::make_parent(path)?;
        crate::files::store::write(path, &kept)
    }

    /// The config `kept` holds.
    fn of(kept: &Kept) -> Result<Custom, String> {
        let code = |text: &str| regent_core::sim::mapgen::custom::code(text);
        let victory = presets::victory_of(kept.victory)?;

        Ok(Custom {
            settlement: code(&kept.settlement)?,
            scenario: code(&kept.scenario)?,
            monsters: code(&kept.monsters)?,
            land: code(&kept.land)?,
            gold: kept.gold,
            victory,
            goal: kept.goal,
            events: [code(&kept.events[0])?, code(&kept.events[1])?],
            ..Custom::default()
        })
    }
}

impl Custom {
    /// What a preset of it keeps (`files::presets`).
    #[must_use]
    pub fn config(&self) -> presets::Config {
        presets::Config {
            settlement: self.settlement,
            scenario: self.scenario,
            monsters: self.monsters,
            land: self.land,
            map_number: self.map_number,
            gold: self.gold,
            victory: self.victory,
            goal: self.goal,
            events: self.events,
            buildings: self.buildings.clone(),
        }
    }

    /// The config `preset` holds, named, its tree loaded from `trees`:
    /// the one of its name, else `default` (freestyle io roll,
    /// build tree load named).
    ///
    /// Its Map Number is kept. The original rolls a fresh one on the clock;
    /// keeping it is this game's choice, the
    /// page's RANDOM rolling another (`docs/openregent/enhancements.md`).
    #[must_use]
    pub fn loaded(preset: &presets::Preset, trees: &Trees) -> Custom {
        let config = &preset.config;
        let mut custom = Custom {
            settlement: config.settlement,
            scenario: config.scenario,
            monsters: config.monsters,
            land: config.land,
            map_number: config.map_number.max(MAP_NUMBER_MIN),
            gold: config.gold,
            victory: config.victory,
            goal: config.goal,
            events: config.events,
            buildings: config.buildings.clone(),
            name: Some(preset.name.clone()),
            ..Custom::default()
        };

        custom.resolve(trees);
        custom
    }
}

/// The clock a menu RNG is seeded with, standing in for `timeGetTime`
/// (**Inferred**: any clock serves; the wall clock's milliseconds,
/// wrapped).
#[must_use]
pub fn clock() -> i32 {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());

    #[allow(
        clippy::cast_possible_truncation,
        reason = "the clock wraps, as timeGetTime's does"
    )]
    let wrapped = ms as u32;

    wrapped.cast_signed()
}

/// One of the setup page's lists: what it offers and what a pick
/// writes to the config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Settlement,
    Scenario,
    Monsters,
    Land,
    /// Special Event 1 or 2.
    Event(usize),
    Victory,
    Gold,
    /// The victory's goal: gold or days.
    Limits,
    /// Building Limitations: a build tree.
    Buildings,
}

/// The lists as the dialog of `players` offers them.
#[derive(Clone, Copy)]
pub struct Dialog<'l> {
    pub lists: &'l Lists,
    pub players: u32,
}

impl Lists {
    /// As the dialog of `players` offers them.
    #[must_use]
    pub const fn dialog(&self, players: u32) -> Dialog<'_> {
        Dialog {
            lists: self,
            players,
        }
    }
}

impl<'l> Dialog<'l> {
    /// Its count's lists; none for a count not loaded.
    fn counted(self) -> &'l Counted {
        let at = self.players.saturating_sub(ONE_PLAYER) as usize;

        self.lists.counted.get(at).unwrap_or(&NOTHING)
    }

    /// Its victory conditions, in order.
    const fn victories(self) -> &'static [Victory] {
        Victory::offered(self.players)
    }
}

impl Choice {
    /// Its entries' names, in order.
    #[must_use]
    pub fn names(self, custom: &Custom, dialog: Dialog<'_>) -> Vec<String> {
        let (lists, counted) = (dialog.lists, dialog.counted());
        let names = |entries: &[Entry]| entries.iter().map(|e| e.name.clone()).collect();

        match self {
            Choice::Settlement => names(&counted.settlements),
            Choice::Scenario => names(&counted.scenarios),
            Choice::Monsters => names(&counted.monsters),
            Choice::Land => names(&counted.lands),
            Choice::Event(_) => names(&lists.events),
            Choice::Victory => dialog
                .victories()
                .iter()
                .map(|v| lists.victory(*v))
                .collect(),
            Choice::Gold => freestyle::GOLDS.map(|g| lists.gold(g)).to_vec(),
            Choice::Limits => lists.goals(custom),
            Choice::Buildings => lists.trees.names(),
        }
    }

    /// The entry the config holds, if one does.
    #[must_use]
    pub fn at(self, custom: &Custom, dialog: Dialog<'_>) -> Option<usize> {
        let (lists, counted) = (dialog.lists, dialog.counted());
        let code = |entries: &[Entry], code: FourCc| entries.iter().position(|e| e.code == code);

        match self {
            Choice::Settlement => code(&counted.settlements, custom.settlement),
            Choice::Scenario => code(&counted.scenarios, custom.scenario),
            Choice::Monsters => code(&counted.monsters, custom.monsters),
            Choice::Land => code(&counted.lands, custom.land),
            Choice::Event(n) => code(&lists.events, custom.events[n]),
            Choice::Victory => dialog.victories().iter().position(|v| *v == custom.victory),
            // A gold not listed shows the first, as the original's.
            Choice::Gold => freestyle::GOLDS
                .iter()
                .position(|g| *g == custom.gold)
                .or(Some(0)),
            // A goal not one of the three is the player's own: the last
            // entry.
            Choice::Limits => {
                let three = custom.victory.goals();

                self.offered(custom, dialog)
                    .then(|| three.iter().position(|g| *g == custom.goal))
                    .map(|at| at.unwrap_or(three.len()))
            }
            // A name not listed shows the first.
            Choice::Buildings => lists
                .trees
                .all()
                .position(|t| t.name == custom.buildings)
                .or(Some(0)),
        }
    }

    /// What it shows: the entry the config holds, else its code or
    /// number as the config holds it.
    #[must_use]
    pub fn shown(self, custom: &Custom, dialog: Dialog<'_>) -> String {
        // The tree's name plain: the field drops the list's blue
        // (**Game**: the capture's field is not blue).
        let held = match self {
            Choice::Buildings => Some(custom.buildings.clone()),
            _ => self
                .at(custom, dialog)
                .and_then(|i| self.names(custom, dialog).get(i).cloned()),
        };

        held.unwrap_or_else(|| match self {
            Choice::Settlement => custom.settlement.to_string(),
            Choice::Scenario => custom.scenario.to_string(),
            Choice::Monsters => custom.monsters.to_string(),
            Choice::Land => custom.land.to_string(),
            Choice::Event(n) => custom.events[n].to_string(),
            Choice::Gold => dialog.lists.gold(custom.gold),
            Choice::Victory | Choice::Limits | Choice::Buildings => String::new(),
        })
    }

    /// Entry `index` picked; a new victory takes its goal as the dialog
    /// shows it ([`Victory::goal`]).
    pub fn pick(self, custom: &mut Custom, dialog: Dialog<'_>, index: usize) {
        let (lists, counted) = (dialog.lists, dialog.counted());
        let code = |entries: &[Entry]| entries.get(index).map(|e| e.code);

        match self {
            Choice::Settlement => set(&mut custom.settlement, code(&counted.settlements)),
            Choice::Scenario => set(&mut custom.scenario, code(&counted.scenarios)),
            Choice::Monsters => set(&mut custom.monsters, code(&counted.monsters)),
            Choice::Land => set(&mut custom.land, code(&counted.lands)),
            Choice::Event(n) => set(&mut custom.events[n], code(&lists.events)),
            Choice::Victory => {
                if let Some(&victory) = dialog.victories().get(index)
                    && victory != custom.victory
                {
                    custom.victory = victory;
                    custom.goal = victory.goal(custom.gold);
                }
            }
            // The dialog refills the goals after each,
            // lifting one under the lead ([`Victory::kept`]). The last
            // entry, the player's own, asks for a number in the
            // original (`AP89`): not offered, the goal stays.
            Choice::Gold => {
                set(&mut custom.gold, freestyle::GOLDS.get(index).copied());
                custom.goal = custom.victory.kept(custom.goal, custom.gold);
            }
            Choice::Limits => {
                set(&mut custom.goal, custom.victory.goals().get(index).copied());
                custom.goal = custom.victory.kept(custom.goal, custom.gold);
            }
            Choice::Buildings => {
                if let Some(tree) = lists.trees.all().nth(index) {
                    custom.buildings.clone_from(&tree.name);
                    custom.caps.clone_from(&tree.caps);
                }
            }
        }
    }

    /// The blurb beside it: its pick's, for the forces and the events.
    #[must_use]
    pub fn blurb<'l>(self, custom: &Custom, dialog: Dialog<'l>) -> Option<&'l str> {
        let counted = dialog.counted();
        let entries = match self {
            Choice::Settlement => Some(&counted.settlements),
            Choice::Scenario => Some(&counted.scenarios),
            Choice::Monsters => Some(&counted.monsters),
            Choice::Event(_) => Some(&dialog.lists.events),
            _ => None,
        };

        entries
            .zip(self.at(custom, dialog))
            .and_then(|(entries, i)| entries.get(i))
            .map(|e| e.detail.as_str())
    }

    /// Whether it opens: Victory Limits only for a victory with a goal.
    #[must_use]
    pub fn offered(self, custom: &Custom, dialog: Dialog<'_>) -> bool {
        match self {
            Choice::Limits => dialog
                .lists
                .goals
                .iter()
                .any(|(v, ..)| *v == custom.victory),
            _ => true,
        }
    }
}

/// `code` made the first of `entries`' own, the original's, where they
/// do not hold it: neither a Random pick nor the monsters' `NONE`.
fn fitted(code: &mut FourCc, entries: &[Entry]) {
    let own =
        |e: &&Entry| e.code != NO_EVENT && e.code != RANDOM_LAND && Band::of(e.code).is_none();
    let listed = entries.iter().any(|e| e.code == *code);

    if !listed && let Some(first) = entries.iter().find(own) {
        *code = first.code;
    }
}

/// `to` set to `value`, if there is one.
fn set<T>(to: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *to = value;
    }
}

impl Lists {
    /// The player's trees read again: after the editor's SAVE or DELETE.
    pub fn reread(&mut self) {
        let shipped = std::mem::take(&mut self.trees.shipped);

        self.trees = Trees::read(
            shipped,
            crate::files::trees::place().as_deref(),
            &self.imports,
        );
    }

    /// A victory condition's name.
    fn victory(&self, v: Victory) -> String {
        self.victories
            .iter()
            .find(|(w, _)| *w == v)
            .map_or_else(|| format!("{v:?}"), |(_, n)| n.clone())
    }

    /// A starting gold's line, `GMTX` 114.
    fn gold(&self, gold: u32) -> String {
        self.gold_text.replace("%d", &gold.to_string())
    }

    /// The goal list for `custom`'s victory, as the original fills it:
    /// its three, `GMTX` 124 or 126, a greyed one in
    /// red ([`Victory::greyed`]), then the player's own; none for a
    /// victory with no goal.
    fn goals(&self, custom: &Custom) -> Vec<String> {
        let victory = custom.victory;
        let lines = self.goals.iter().find(|(v, ..)| *v == victory);

        lines
            .map(|(_, line, own)| {
                let goal = |g: u32| {
                    let mark = if victory.greyed(g, custom.gold) {
                        UNAVAILABLE_MARK
                    } else {
                        ""
                    };

                    format!("{mark}{}", line.replace("%d", &g.to_string()))
                };

                victory
                    .goals()
                    .into_iter()
                    .map(goal)
                    .chain(std::iter::once(own.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `lists` as the single-player dialog offers them.
    fn alone(lists: &Lists) -> Dialog<'_> {
        lists.dialog(ONE_PLAYER)
    }

    /// A scratch file, `name.toml` in a folder of its own: the folder,
    /// kept while the file is used, and the file's path.
    fn scratch(name: &str) -> (crate::testdata::Scratch, PathBuf) {
        let dir = crate::testdata::scratch("freestyle");
        let path = dir.join(name).with_extension("toml");

        (dir, path)
    }

    #[test]
    fn a_kept_config_comes_back_but_its_map_number() {
        let (_dir, path) = scratch("round_trip");
        let custom = Custom {
            settlement: FourCc::new(b"Wizb"),
            scenario: FourCc::new(b"Ruib"),
            monsters: FourCc::new(b"EXPE"),
            land: FourCc::new(b"KxHa"),
            map_number: 680_631_669,
            gold: 12_000,
            victory: Victory::Survive,
            goal: 20,
            events: [FourCc::new(b"EV01"), NO_EVENT],
            ..Custom::default()
        };

        custom.keep(&path).unwrap();

        let back = Custom::kept(&path).unwrap();

        assert_eq!(back.map_number, MAP_NUMBER_MIN);
        assert_eq!(
            back,
            Custom {
                map_number: MAP_NUMBER_MIN,
                ..custom
            }
        );
    }

    #[test]
    fn a_victory_picked_takes_its_goal_and_offers_its_limits() {
        let lists = Lists {
            goals: vec![(Victory::Gold, "Gather %d gold".to_owned(), "Own".to_owned())],
            ..Lists::default()
        };
        let mut custom = Custom {
            gold: 0,
            ..Custom::default()
        };

        assert!(!Choice::Limits.offered(&custom, alone(&lists)));

        Choice::Victory.pick(&mut custom, alone(&lists), 1);

        assert_eq!(custom.victory, Victory::Gold);
        assert_eq!(custom.goal, Victory::Gold.goal(custom.gold));
        assert!(Choice::Limits.offered(&custom, alone(&lists)));
        assert_eq!(
            Choice::Limits.shown(&custom, alone(&lists)),
            format!("Gather {} gold", custom.goal)
        );
    }

    /// The lists as the page loads them from HD's expansion.
    fn hd_lists() -> Lists {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");

        Lists::load(&hd, crate::testdata::hd()).expect("the lists")
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_gold_list_is_the_dialogs_four_and_a_gold_not_listed_shows_the_first() {
        let lists = hd_lists();
        let mut custom = Custom::default();

        assert_eq!(
            Choice::Gold.names(&custom, alone(&lists)),
            ["30000 Gold", "20000 Gold", "15000 Gold", "10000 Gold"]
        );
        assert_eq!(Choice::Gold.shown(&custom, alone(&lists)), "30000 Gold");

        Choice::Gold.pick(&mut custom, alone(&lists), 2);

        assert_eq!(custom.gold, 15_000);
        assert_eq!(Choice::Gold.at(&custom, alone(&lists)), Some(2));

        custom.gold = 12_000;

        assert_eq!(Choice::Gold.at(&custom, alone(&lists)), Some(0));
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_wandering_monsters_none_is_the_dialogs_none() {
        let lists = hd_lists();
        let custom = Custom::default();
        let names = Choice::Monsters.names(&custom, alone(&lists));

        assert_eq!(custom.monsters, NO_EVENT);
        assert_eq!(names.first().map(String::as_str), Some("None"));
        assert_eq!(Choice::Monsters.shown(&custom, alone(&lists)), "None");
        assert_eq!(Choice::Monsters.blurb(&custom, alone(&lists)), Some("None"));
        assert_eq!(
            Choice::Event(0)
                .names(&custom, alone(&lists))
                .first()
                .map(String::as_str),
            Some("None")
        );
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_random_force_is_described_by_its_own_name() {
        let lists = hd_lists();
        let custom = Custom::default();

        assert_eq!(
            Choice::Settlement.blurb(&custom, alone(&lists)),
            Some("Random (Beginner)")
        );
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_victory_limits_end_with_the_player_specified_entry() {
        let lists = hd_lists();
        let mut custom = Custom::default();

        Choice::Victory.pick(&mut custom, alone(&lists), 2);

        assert_eq!(
            Choice::Limits.names(&custom, alone(&lists)),
            [
                "Survive for 10 days",
                "Survive for 20 days",
                "Survive for 30 days",
                "(Player specified time)"
            ]
        );
        assert_eq!(Choice::Limits.at(&custom, alone(&lists)), Some(1));

        custom.goal = 45;

        assert_eq!(Choice::Limits.at(&custom, alone(&lists)), Some(3));

        // The original asks for the number here (`AP89`): not offered,
        // the goal stays.
        Choice::Limits.pick(&mut custom, alone(&lists), 3);

        assert_eq!(custom.goal, 45);
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_gold_goal_under_the_lead_is_greyed_and_lifted() {
        let lists = hd_lists();
        let mut custom = Custom::default();

        Choice::Victory.pick(&mut custom, alone(&lists), 1);

        // 30,000 to start: goals under 60,000 greyed, the goal lifted.
        assert_eq!(custom.goal, 60_000);
        assert_eq!(
            Choice::Limits.names(&custom, alone(&lists)),
            [
                "\u{1}FF0000Gather 20000 gold",
                "\u{1}FF0000Gather 30000 gold",
                "\u{1}FF0000Gather 40000 gold",
                "(Player specified amount)"
            ]
        );

        Choice::Gold.pick(&mut custom, alone(&lists), 3);
        Choice::Limits.pick(&mut custom, alone(&lists), 0);

        // 10,000 to start: 20,000 is lifted to 40,000.
        assert_eq!(custom.goal, 40_000);
        assert_eq!(Choice::Limits.at(&custom, alone(&lists)), Some(2));
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn building_limitations_list_the_shipped_in_blue_and_a_pick_takes_its_caps() {
        let lists = hd_lists();
        let mut custom = Custom::default();
        let names = Choice::Buildings.names(&custom, alone(&lists));

        assert_eq!(
            names[..5],
            [
                "default",
                "Brute Force",
                "No Towers",
                "One of a Kind",
                "Wizard War"
            ]
            .map(|n| format!("{}{n}", crate::files::trees::SHIPPED_MARK))
        );
        assert!(Choice::Buildings.offered(&custom, alone(&lists)));

        Choice::Buildings.pick(&mut custom, alone(&lists), 2);

        assert_eq!(Choice::Buildings.shown(&custom, alone(&lists)), "No Towers");
        assert_eq!(custom.caps_text(), "ABB1=0,ABE1=0,ABM1=0");

        // A name no longer listed falls back to the first, as the list
        // fills.
        custom.buildings = "Gone".to_owned();
        custom.resolve(&lists.trees);

        assert_eq!(custom.buildings, build_tree::DEFAULT);
        assert!(custom.caps.is_empty());
    }

    /// A LAN game's lists are its players': Player Forces of that many
    /// (`Froa` four players'), Last Palace (`GMTX` 123) with more than
    /// one; the events alike for every count.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_lists_are_the_players_counts() {
        let lists = hd_lists();
        let custom = Custom::default();
        let codes = |players: u32| -> Vec<FourCc> {
            lists.counted[players as usize - 1]
                .settlements
                .iter()
                .map(|e| e.code)
                .collect()
        };
        let names = |choice: Choice, players| choice.names(&custom, lists.dialog(players));
        let wizard = FourCc::new(b"Wizb");
        let four = FourCc::new(b"Froa");

        assert!(codes(ONE_PLAYER).contains(&wizard) && !codes(ONE_PLAYER).contains(&four));
        assert!(codes(4).contains(&four));

        for players in 2..=freestyle::MOST_PLAYERS {
            assert!(!codes(players).contains(&wizard), "{players}");
            assert_ne!(names(Choice::Scenario, players), names(Choice::Scenario, 1));
            assert_eq!(names(Choice::Land, players), names(Choice::Land, 1));
            assert_eq!(names(Choice::Event(0), players), names(Choice::Event(0), 1));
            assert_eq!(
                names(Choice::Victory, players).last().map(String::as_str),
                Some("Last Palace standing")
            );
        }

        assert_eq!(
            names(Choice::Victory, ONE_PLAYER),
            [
                "Eliminate all foes",
                "Gather gold",
                "Survive specified time"
            ]
        );
    }

    /// Fitted to a count, a pick not offered becomes its list's first own
    /// entry; a Random pick and `NONE` stay; alone, Last Palace is
    /// Eliminate.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_config_fitted_to_the_players_takes_what_they_offer() {
        let lists = hd_lists();
        let first_own = |entries: &[Entry]| {
            entries
                .iter()
                .find(|e| Band::of(e.code).is_none() && e.code != NO_EVENT)
                .map(|e| e.code)
        };
        let two = &lists.counted[1];
        let mut custom = Custom {
            settlement: FourCc::new(b"Wizb"),
            scenario: Band::Expert.tag(),
            victory: Victory::LastPalace,
            ..Custom::default()
        };

        custom.fit(lists.dialog(2));

        assert_eq!(Some(custom.settlement), first_own(&two.settlements));
        assert_eq!(custom.scenario, Band::Expert.tag());
        assert_eq!(custom.monsters, NO_EVENT);
        assert_eq!(custom.land, RANDOM_LAND);
        assert_eq!(custom.victory, Victory::LastPalace);
        assert!(Choice::Settlement.at(&custom, lists.dialog(2)).is_some());

        custom.fit(alone(&lists));

        assert_eq!(
            Some(custom.settlement),
            first_own(&lists.counted[0].settlements)
        );
        assert_eq!(custom.victory, Victory::Eliminate);
    }

    #[test]
    fn with_none_kept_the_config_is_the_defaults() {
        assert_eq!(Custom::kept(&scratch("absent").1), Ok(Custom::default()));
    }

    #[test]
    fn a_kept_config_of_another_version_is_refused() {
        let (_dir, path) = scratch("later");

        Custom::default().keep(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let later = text.replace("version = 1", "version = 2");

        std::fs::write(&path, later).unwrap();

        assert!(Custom::kept(&path).is_err());
    }

    #[test]
    fn the_config_opens_on_the_registrys_defaults() {
        let custom = Custom::default();

        assert_eq!(custom.settlement, FourCc::new(b"BEGI"));
        assert_eq!(custom.scenario, FourCc::new(b"BEGI"));
        assert_eq!(custom.monsters, FourCc::new(b"NONE"));
        assert_eq!(custom.land, FourCc::new(b"RAND"));
        assert_eq!(custom.gold, 30_000);
        assert_eq!(custom.victory, Victory::Eliminate);
    }

    #[test]
    fn a_preset_writes_the_config_and_keeps_the_map_number() {
        let mut custom = Custom {
            map_number: 680_631_669,
            ..Custom::default()
        };

        custom.preset(Preset::Expert, 5, &[]);

        let expert = FourCc::new(b"EXPE");

        assert_eq!(
            [custom.settlement, custom.scenario, custom.monsters],
            [expert; 3]
        );
        assert_eq!(custom.land, RANDOM_LAND);
        assert_eq!(custom.gold, 10_000);
        assert_eq!(custom.map_number, 680_631_669);
        assert_eq!(custom.events, [NO_EVENT; 2]);
    }

    /// A config with every preset field off its default.
    fn odd() -> Custom {
        Custom {
            settlement: FourCc::new(b"Wizb"),
            scenario: FourCc::new(b"Ruib"),
            monsters: FourCc::new(b"EXPE"),
            land: FourCc::new(b"KxHa"),
            map_number: 680_631_669,
            gold: 30_000,
            victory: Victory::Survive,
            goal: 20,
            events: [FourCc::new(b"EV01"), FourCc::new(b"EV07")],
            cheats: Cheats::Allowed,
            buildings: "No Inns".to_owned(),
            caps: [(FourCc::new(b"ABF1"), 0)].into_iter().collect(),
            name: None,
        }
    }

    fn trees() -> Trees {
        Trees {
            shipped: vec![Tree {
                name: build_tree::DEFAULT.to_owned(),
                ..Tree::default()
            }],
            own: vec![Tree {
                name: "No Inns".to_owned(),
                caps: odd().caps,
            }],
        }
    }

    /// A preset of the config loads back as it was, named.
    #[test]
    fn a_preset_loads_back_every_field_and_its_tree() {
        let preset = presets::Preset {
            name: "funbar".to_owned(),
            players: presets::SOLO,
            config: odd().config(),
        };

        assert_eq!(
            Custom::loaded(&preset, &trees()),
            Custom {
                name: Some("funbar".to_owned()),
                ..odd()
            }
        );
    }

    #[test]
    fn an_unknown_tree_loads_as_default() {
        let preset = presets::Preset {
            name: "gone".to_owned(),
            players: presets::SOLO,
            config: Custom {
                buildings: "Gone".to_owned(),
                ..odd()
            }
            .config(),
        };
        let loaded = Custom::loaded(&preset, &trees());

        assert_eq!(loaded.buildings, build_tree::DEFAULT);
        assert!(loaded.caps.is_empty());
    }
}
