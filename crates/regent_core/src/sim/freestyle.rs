//! The custom game: the single-player dialog's lists and picks, its
//! Random draws, its victory conditions, and the setup a new game holds.
//!
//! ```text
//!  dialog refresh                  the lists               offers
//!  game setup                      Random picks drawn      randoms, random_lands,
//!                                                          random, tagged
//!  dialog picks ──> setup words    SVGD, RGS1..RGS9        setup_of, with_victory
//!  game setup                      the template, its lairs template, lairs
//!  game setup                      each placement's slot   seats, Play::humans
//! ```
//!
//! Rules of the original's dialog and game start, over the quest map
//! ([`crate::data::rgs::Map`]) and the setup words
//! ([`Setup`]), which only read them.

use crate::base::rng;
use crate::data::fourcc::FourCc;
use crate::data::rgs::map::{
    ENEMY_FORCES, FLAG_SET, Map, Placement, Spawner, Team, UnitPattern, WANDERING_MONSTERS,
};
use crate::sim::start::NEUTRAL;
use serde::{Deserialize, Deserializer};
use std::collections::BTreeMap;
use std::ops::Range;

/// One of the single-player custom-game dialog's four lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum List {
    /// Landscape: a region.
    Land,
    /// Player Forces, one player's entries.
    Settlement,
    /// Enemy Forces.
    Scenario,
    /// Wandering Monsters, less the dialog's own "none", `NONE`.
    Monsters,
}

/// A force list's Random pick: the tag the setup holds in place of a
/// code, freestyle setup drawing the code at the game's start
/// ([`randoms`]). **Binary**: freestyle setup; freestyle title
/// names them "Random (Beginner)" and on, `GMTX` 111–113.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    Beginner,
    Advanced,
    Expert,
}

impl Band {
    pub const ALL: [Band; 3] = [Band::Beginner, Band::Advanced, Band::Expert];

    /// The tag a setup holds.
    #[must_use]
    pub const fn tag(self) -> FourCc {
        match self {
            Band::Beginner => FourCc::new(b"BEGI"),
            Band::Advanced => FourCc::new(b"ADVA"),
            Band::Expert => FourCc::new(b"EXPE"),
        }
    }

    /// The band `code` is the tag of, if any.
    #[must_use]
    pub fn of(code: FourCc) -> Option<Band> {
        Band::ALL.into_iter().find(|b| b.tag() == code)
    }

    /// The ratings it draws from, inclusive.
    #[must_use]
    pub const fn limits(self) -> (i32, i32) {
        crate::data::settings::DIFFICULTY_BANDS[self as usize]
    }
}

/// The Landscape list's Random pick: the tag a setup holds,
/// `RGS4`'s default.
pub const RANDOM_LAND: FourCc = FourCc::new(b"RAND");

/// The players a single-player game has: the count freestyle setup
/// filters a Random pick by, its second argument
/// ([`Play`]).
pub const ONE_PLAYER: u32 = 1;

/// The most players a dialog offers lists for: its count filter's cases
/// run 1 to 4 (freestyle dialog refresh, the pattern's four trailer
/// flags).
pub const MOST_PLAYERS: u32 = 4;

/// Whether a land's team is offered in a game of `players`: `active`
/// for one player, `expanding` for more (**Binary**:
/// Freestyle land fill, and the Random land's pick at
/// freestyle setup). Both flags agree on every shipped region.
const fn land_for(team: &Team<'_>, players: u32) -> bool {
    let flag = if players == ONE_PLAYER {
        team.active
    } else {
        team.expanding
    };

    flag == FLAG_SET
}

/// Whether `pattern`'s rating is in `band`.
#[must_use]
pub fn in_band(pattern: &UnitPattern<'_>, band: Band) -> bool {
    let (low, high) = band.limits();

    pattern
        .rating()
        .and_then(|r| i32::try_from(r).ok())
        .is_some_and(|r| (low..=high).contains(&r))
}

/// One entry of a dialog list: the code a save's setup writes, and the
/// dialog's string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Offer<'a> {
    pub code: FourCc,
    pub name: &'a [u8],
}

/// The entries the custom-game dialog of `players` players offers in
/// `list`, in its order.
///
/// A land is a region whose team is offered for the count
/// (`land_for`); a force entry is a set 1 record bucketed by `words[0]`
/// ([`UnitPattern::players`], [`ENEMY_FORCES`], [`WANDERING_MONSTERS`]),
/// the last two with the count's trailer flag set
/// ([`UnitPattern::for_players`]; one player's
/// [`ONE_PLAYER_FLAG`](crate::data::rgs::map::ONE_PLAYER_FLAG)).
/// **Binary**: the dialog's count filter, freestyle dialog.
///
/// The lands keep file order: the region map's key order on the shipped
/// quest. The forces go by code word, as the world's
/// map holds them, then stably by their eliminate
/// rating, least first (a list sort).
#[must_use]
pub fn offers<'a>(map: &'a Map<'_>, list: List, players: u32) -> Vec<Offer<'a>> {
    let kind = |u: &UnitPattern<'_>| match list {
        List::Settlement => u.players() == Some(players),
        List::Scenario => u.words[0] == ENEMY_FORCES && u.for_players(players),
        List::Monsters => u.words[0] == WANDERING_MONSTERS && u.for_players(players),
        List::Land => false,
    };

    if list == List::Land {
        map.regions
            .iter()
            .filter_map(|r| r.team.as_ref())
            .filter(|t| land_for(t, players))
            .map(|t| Offer {
                code: t.code,
                name: t.name,
            })
            .collect()
    } else {
        let mut forces: Vec<&UnitPattern<'_>> = map.units[1].iter().filter(|u| kind(u)).collect();

        forces.sort_by_key(|u| u.faction.word());
        forces.sort_by_key(|u| Ratings::of(map, u.faction).by(Victory::Eliminate));
        forces
            .into_iter()
            .map(|u| Offer {
                code: u.faction,
                name: u.name,
            })
            .collect()
    }
}

/// What a Random force pick of `list` and `band` draws from in a game
/// of `players`, in the order the engine holds them: a map keyed by
/// code, the word ascending.
/// **Binary**, freestyle setup, the count its second argument:
///
/// ```text
///  Settlement  for the count, not kept out, in band
///  Scenario    Enemy Forces, not kept out, in band, the
///              count's flag
///  Monsters    Wandering Monsters, the same
/// ```
///
/// None for [`List::Land`]: see [`random_lands`].
#[must_use]
pub fn randoms(map: &Map<'_>, list: List, band: Band, players: u32) -> Vec<FourCc> {
    let kind = |u: &UnitPattern<'_>| match list {
        List::Settlement => u.words[0] == players,
        List::Scenario => u.words[0] == ENEMY_FORCES && u.for_players(players),
        List::Monsters => u.words[0] == WANDERING_MONSTERS && u.for_players(players),
        List::Land => false,
    };

    keyed(
        map.units[1]
            .iter()
            .filter(|u| kind(u) && !u.kept_out() && in_band(u, band))
            .map(|u| (u.faction, u.faction)),
    )
}

/// What a Random land pick in a game of `players` draws from: each
/// region whose team is offered for the count (`land_for`), by region
/// code ascending (pattern section read's map; freestyle setup).
#[must_use]
pub fn random_lands(map: &Map<'_>, players: u32) -> Vec<FourCc> {
    keyed(
        map.regions
            .iter()
            .filter(|r| r.code != b"NONE")
            .filter_map(|r| Some((r.code, r.team.as_ref()?)))
            .filter(|(_, t)| land_for(t, players))
            .map(|(key, t)| (key, t.code)),
    )
}

/// `pairs`' codes in the order of their keys as the engine's map holds
/// them: the little-endian word, ascending.
fn keyed(pairs: impl Iterator<Item = (FourCc, FourCc)>) -> Vec<FourCc> {
    let mut keyed: Vec<(u32, FourCc)> = pairs.map(|(key, code)| (key.word(), code)).collect();

    keyed.sort_by_key(|(key, _)| *key);
    keyed.into_iter().map(|(_, code)| code).collect()
}

/// The four lists in the order a Random pick draws: land, settlement,
/// scenario, monsters.
pub const DRAWN: [List; 4] = [List::Land, List::Settlement, List::Scenario, List::Monsters];

/// The setup slot of each of [`DRAWN`]'s lists.
const DRAWN_SLOTS: [usize; 4] = [
    Setup::LAND_SLOT,
    Setup::SETTLEMENT_SLOT,
    Setup::SCENARIO_SLOT,
    Setup::MONSTERS_SLOT,
];

/// Whether `code` is `list`'s Random tag.
fn is_random(list: List, code: FourCc) -> bool {
    match list {
        List::Land => code == RANDOM_LAND,
        _ => Band::of(code).is_some(),
    }
}

/// `asked`, in [`DRAWN`]'s order, each Random pick of a game of
/// `players` drawn as
/// freestyle setup draws it: the world RNG seeded with the Map
/// Number, then one `rng_next` over each Random list's
/// candidates ([`randoms`], [`random_lands`]) in [`DRAWN`]'s order.
/// The charts
/// reseed with the same number, so these draws move nothing else. A code
/// that is no tag stands.
///
/// ```text
///  RAND KxHa? BEGI ADVA  ──> seed ──> land: rng_next(lands)
///                                     settlement: rng_next(Beginner's)
///                                     scenario: rng_next(Advanced's)
/// ```
///
/// # Errors
///
/// The tag of a Random pick with nothing to draw (the engine indexes an
/// empty vector).
pub fn random(
    map: &Map<'_>,
    asked: [FourCc; 4],
    seed: i32,
    players: u32,
) -> Result<[FourCc; 4], FourCc> {
    let mut state = seed.cast_unsigned();
    let mut drawn = asked;

    for (code, list) in drawn.iter_mut().zip(DRAWN) {
        let candidates = match (list, Band::of(*code)) {
            (List::Land, _) => (*code == RANDOM_LAND).then(|| random_lands(map, players)),
            (_, band) => band.map(|b| randoms(map, list, b, players)),
        };

        if let Some(candidates) = candidates {
            let count = i32::try_from(candidates.len()).unwrap_or(i32::MAX);
            let at = usize::try_from(rng::draw(&mut state, count)).unwrap_or(usize::MAX);

            *code = *candidates.get(at).ok_or(*code)?;
        }
    }

    Ok(drawn)
}

/// `setup` holding the Random tags of `asked` ([`DRAWN`]'s order) in
/// place of the codes drawn: the dialog's config keeps the tag, the
/// world the code (**Binary**: freestyle setup writes the drawn
/// land to world, not the config; freestyle title reads
/// the config's tag).
#[must_use]
pub fn tagged(setup: Setup, asked: [Option<FourCc>; 4]) -> Setup {
    let tags = DRAWN
        .into_iter()
        .zip(DRAWN_SLOTS)
        .zip(asked)
        .filter_map(|((list, slot), code)| Some((slot, code.filter(|c| is_random(list, *c))?)));

    tags.fold(setup, |mut setup, (slot, tag)| {
        setup.slots[slot] = tag.word();
        setup
    })
}

/// Starting gold the dialog offers. **Corpus**: 30,000 on every
/// freestyle setup.
pub const GOLD: u32 = 30_000;

/// The dialog's Starting Gold list, control 5007 (`0x138f`), in its
/// order: these GPL constants, read once into
/// four globals. A pick writes its
/// value to the config's gold, setup slot 6. A gold not
/// among them shows the first. [`GOLDS`] their shipped values.
pub const GOLD_CONSTANTS: [&str; 4] = [
    "#starting_gold_very_easy",
    "#starting_gold_easy",
    "#starting_gold_normal",
    "#starting_gold_hard",
];
pub const GOLDS: [u32; 4] = [30_000, 20_000, 15_000, 10_000];

/// A starting gold's line, `GMTX` 114 "%d Gold".
pub const GOLD_TEXT: usize = 114;

/// The dialog's own none, `GMTX` 109 "None": the Wandering Monsters
/// list's first entry, `NONE`, and its blurb.
pub const NONE_TEXT: usize = 109;

/// The only tileset. **Corpus**: `GRIT` on every freestyle setup.
pub const TILESET: FourCc = FourCc::new(b"GRIT");

/// The setup a new game with `choice` holds, each seat with `gold`.
/// Slots 7 and 8 are 0, as on every corpus setup. Slots 4, 5, 9 and 10
/// are 0 here: nothing found reads them; 4 and 5 are the dialog's
/// [`difficulty`] and its [`difficulty_step`], 9 and 10 stale dialog
/// state. The side is not a setup word.
#[must_use]
pub const fn setup_of(choice: &Choice, gold: u32) -> Setup {
    let mut slots = [0; Setup::LEN];

    slots[0] = choice.settlement.word();
    slots[1] = choice.scenario.word();
    slots[2] = choice.monsters.word();
    slots[3] = choice.land.word();
    slots[6] = gold;
    slots[11] = TILESET.word();
    slots[12] = choice.seed.cast_unsigned();

    Setup { slots }
}

/// `setup` with the dialog's victory condition and goal.
#[must_use]
pub const fn with_victory(mut setup: Setup, victory: Victory, goal: u32) -> Setup {
    setup.slots[7] = victory.word();
    setup.slots[8] = goal;
    setup
}

/// The custom-game dialog's picks; `None` takes the default
/// (`crate::sim::mapgen::custom::DEFAULTS`). A scene file's `[map]`
/// reads the same, less the players ([`crate::sim::scene::Scene`]
/// gives its own).
///
/// ```text
///  Picks ──custom::chosen──> Choice ──setup_of──> Setup
///                              └──picks──> Forces
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Picks {
    #[serde(default, deserialize_with = "some_code")]
    pub land: Option<FourCc>,
    #[serde(default, deserialize_with = "some_code")]
    pub settlement: Option<FourCc>,
    #[serde(default, deserialize_with = "some_code")]
    pub scenario: Option<FourCc>,
    /// Wandering Monsters, or `NONE`.
    #[serde(default, deserialize_with = "some_code")]
    pub monsters: Option<FourCc>,
    /// The Map Number.
    #[serde(rename = "map_number")]
    pub seed: Option<i32>,
    /// Cells a side.
    pub side: Option<u32>,
    /// Players; one if not given.
    #[serde(skip)]
    pub players: Option<u32>,
}

impl Picks {
    /// The game's players: one if not given.
    #[must_use]
    pub fn players(&self) -> u32 {
        self.players.unwrap_or(ONE_PLAYER)
    }

    /// The four picks in the order a Random pick draws ([`DRAWN`]).
    #[must_use]
    pub const fn asked(&self) -> [Option<FourCc>; 4] {
        [self.land, self.settlement, self.scenario, self.monsters]
    }
}

/// The dialog's picks, every one resolved: what the generator takes.
///
/// The four choices a freestyle save writes back (`crate::sim::freestyle::Setup`),
/// plus seed and side.
#[derive(Debug, Clone, Copy)]
pub struct Choice {
    pub land: FourCc,
    pub settlement: FourCc,
    pub scenario: FourCc,
    /// Wandering Monsters, or `NONE` (every corpus save). Merged after the
    /// scenario. See [`crate::sim::mapgen::setup`].
    pub monsters: FourCc,
    /// The Map Number: the seed. The dialog clamps it at 1.
    pub seed: i32,
    /// Side in cells, carried by the Enemy Forces choice.
    pub side: u32,
}

impl Choice {
    /// The three force picks, in merge order.
    #[must_use]
    pub const fn picks(&self) -> Forces {
        Forces {
            settlement: self.settlement,
            scenario: self.scenario,
            monsters: self.monsters,
        }
    }
}

/// A four-character code from a scene file's text.
pub(crate) fn code<'de, D: Deserializer<'de>>(d: D) -> Result<FourCc, D::Error> {
    let text = String::deserialize(d)?;

    <[u8; 4]>::try_from(text.as_bytes())
        .map(FourCc)
        .map_err(|_| serde::de::Error::custom(format!("{text} is not a four-character code")))
}

/// [`code`], given.
fn some_code<'de, D: Deserializer<'de>>(d: D) -> Result<Option<FourCc>, D::Error> {
    code(d).map(Some)
}

/// The dialog's three force picks, in merge order. The land is not a
/// force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Forces {
    pub settlement: FourCc,
    pub scenario: FourCc,
    /// The Wandering Monsters entry, or `NONE`.
    pub monsters: FourCc,
}

impl Forces {
    /// The three, in merge order.
    const fn merged(&self) -> [FourCc; 3] {
        [self.settlement, self.scenario, self.monsters]
    }
}

/// The placements a freestyle setup hands the force chart: the
/// template at world.
///
/// **Binary** — freestyle setup copies the
/// settlement entry into the template, then merges the scenario at
/// freestyle setup and the monsters (only if not `NONE`).
/// An unknown pick, `NONE` included, contributes nothing, as in the
/// engine.
#[must_use]
pub fn template<'a>(map: &'a Map<'a>, forces: &Forces) -> Vec<Placement<'a>> {
    forces
        .merged()
        .iter()
        .filter_map(|code| entry(map, *code))
        .flat_map(|pattern| pattern.placements.iter().cloned())
        .collect()
}

/// The template's lairs by key ([`lair_key`]).
///
/// **Binary** — freestyle setup assigns the settlement entry,
/// lairs and all, then layout pattern merge
/// appends the scenario's and the monsters': per placement `i` of the
/// entry and unit `j` of the block record it names, the entry's spawner
/// `1000 i + j` goes under `1000 i' + j`, `i'` the placement's index in
/// the template.
#[must_use]
pub fn lairs<'a>(map: &'a Map<'a>, forces: &Forces) -> BTreeMap<u32, &'a Spawner<'a>> {
    let block = |code: FourCc| map.units[0].iter().find(|u| u.faction == code);
    let mut out = BTreeMap::new();
    let mut placed = 0;

    for (n, pattern) in forces
        .merged()
        .iter()
        .filter_map(|code| entry(map, *code))
        .enumerate()
    {
        let own: BTreeMap<u32, &Spawner<'a>> =
            pattern.spawners.iter().map(|(k, s)| (*k, s)).collect();

        if n == 0 {
            out.extend(own.iter().map(|(k, s)| (*k, *s)));
        } else {
            for (i, placement) in pattern.placements.iter().enumerate() {
                let units = block(placement.id).map_or(0, |b| b.placements.len());

                for j in 0..units {
                    if let Some(s) = own.get(&lair_key(i, j)) {
                        out.insert(lair_key(placed + i, j), *s);
                    }
                }
            }
        }

        placed += pattern.placements.len();
    }

    out
}

/// A lair's key: `1000 × placement + unit` (unit chart build,
/// layout pattern merge).
#[must_use]
pub fn lair_key(placement: usize, unit: usize) -> u32 {
    u32::try_from(placement * LAIR_STRIDE + unit).unwrap_or(u32::MAX)
}

/// Lair keys a placement apart.
pub const LAIR_STRIDE: usize = 1000;

/// Who plays a freestyle game. **Binary**: freestyle setup tests
/// session for a network game, and takes the
/// player count as its second argument: 1 from freestyle start cfg
/// and the single-player dialog, session from
/// freestyle start net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    Single,
    /// A network game of this many players.
    Network(u32),
}

/// A set 0 record's `words[0]` on a player's block: a placement naming
/// one is a player's settlement. **Binary**: freestyle setup.
pub const PLAYER_BLOCK: u32 = 1;

/// Player entries before the monsters' [`NEUTRAL`]: slots 0 to 6.
pub const PLAYER_SLOTS: i32 = 7;

impl Play {
    /// Who plays a game of `players` humans: one alone, more over the
    /// network. **Inferred**: a network game of one would place as one
    /// alone does, a Player Forces entry for one holding one player block.
    #[must_use]
    pub const fn of(players: u32) -> Play {
        if players > ONE_PLAYER {
            Play::Network(players)
        } else {
            Play::Single
        }
    }

    /// The slots with a live player entry: the humans. The other player
    /// slots are "Dummy Player", not live, and get no `Player`;
    /// [`NEUTRAL`] is "Monster Player".
    ///
    /// **Binary**: alone, slot 0, "Human Player"; in a
    /// network game, the session's player list in its
    /// order, one slot each.
    #[must_use]
    pub fn humans(self) -> Range<i32> {
        match self {
            Play::Single => 0..1,
            Play::Network(n) => 0..i32::try_from(n).map_or(PLAYER_SLOTS, |n| n.min(PLAYER_SLOTS)),
        }
    }

    /// How many [`Play::humans`]: the world's session count
    /// ([`crate::sim::world::World::humans`]).
    #[must_use]
    pub fn session(self) -> u32 {
        self.humans().end.cast_unsigned()
    }
}

/// Each [`template`] placement's owner slot, in template order.
///
/// **Binary** — the freestyle setup fills the force lists the engine's
/// force owner lookup reads, not the quest's slot items:
///
/// ```text
///  for p in template:
///      block = units[0][p.id]
///      if block.words[0] == 1: list[next] += p
///          next += 1 in a network game only
///      else: list[7] += p
/// ```
///
/// So the n-th player block goes to slot n in a network game; alone,
/// every one goes to slot 0. **Corpus**: a Player Forces entry for n
/// players holds n placements, all player blocks; no Enemy Forces or
/// Wandering Monsters entry holds one.
#[must_use]
pub fn seats(map: &Map<'_>, forces: &Forces, play: Play) -> Vec<i32> {
    let player = |p: &Placement<'_>| {
        map.units[0]
            .iter()
            .find(|u| u.faction == p.id)
            .is_some_and(|b| b.words[0] == PLAYER_BLOCK)
    };
    let step = match play {
        Play::Single => 0,
        Play::Network(_) => 1,
    };

    template(map, forces)
        .iter()
        .scan(0, |next, p| {
            let slot = if player(p) {
                *next += step;
                *next - step
            } else {
                NEUTRAL
            };

            Some(slot)
        })
        .collect()
}

/// One dialog entry, by the code a save's setup writes.
fn entry<'a>(map: &'a Map<'a>, code: FourCc) -> Option<&'a UnitPattern<'a>> {
    map.units[1].iter().find(|u| u.faction == code)
}

/// The custom-game dialog's victory conditions, control 5005 (`0x138d`),
/// by the setup word [`Setup::victory`] holds: `SetVictoryCondition`'s
/// index, named by `GMTX` 120 to 123. The goal is
/// [`Setup::goal`]. **Binary**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Victory {
    /// "Eliminate all foes": no lair, building or hero of another team
    /// (`VictoryCondition_Three`).
    Eliminate,
    /// "Gather gold": a treasury past the goal (`VictoryCondition_Two`).
    Gold,
    /// "Survive specified time": the goal's days (`VictoryCondition_One`).
    Survive,
    /// "Last Palace standing": offered with more than one player; a
    /// single player's dialog turns it back to [`Victory::Eliminate`].
    LastPalace,
}

impl Victory {
    /// Each by its setup word.
    pub const ALL: [Victory; 4] = [
        Victory::Eliminate,
        Victory::Gold,
        Victory::Survive,
        Victory::LastPalace,
    ];

    /// What a one-player dialog offers.
    pub const OFFERED: [Victory; 3] = [Victory::Eliminate, Victory::Gold, Victory::Survive];

    /// What the dialog of `players` offers, in its order: Last Palace
    /// only with more than one (victory dialog fill, `GMTX` 123).
    #[must_use]
    pub const fn offered(players: u32) -> &'static [Victory] {
        if players > ONE_PLAYER {
            &Victory::ALL
        } else {
            &Victory::OFFERED
        }
    }

    /// As the dialog of `players` holds it: a single player's Last
    /// Palace turned back to Eliminate.
    #[must_use]
    pub const fn for_players(self, players: u32) -> Victory {
        match self {
            Victory::LastPalace if players <= ONE_PLAYER => Victory::Eliminate,
            v => v,
        }
    }

    /// The setup word.
    #[must_use]
    pub const fn word(self) -> u32 {
        self as u32
    }

    /// The dialog's name for it, a `GMTX` index.
    #[must_use]
    pub const fn text(self) -> usize {
        120 + self as usize
    }

    /// The goal's control (5008, `0x1390`) offers three, then "(Player
    /// specified …)": gold;
    /// days.
    #[must_use]
    pub const fn goals(self) -> [u32; 3] {
        match self {
            Victory::Gold => GOLD_GOALS,
            Victory::Survive => DAY_GOALS,
            Victory::Eliminate | Victory::LastPalace => [0; 3],
        }
    }

    /// The goal list's last entry, a `GMTX` index: 125 "(Player
    /// specified amount)", 127 "(Player specified time)"; picked, the
    /// original asks for the number (`AP89`). None for a
    /// victory with no goal.
    #[must_use]
    pub const fn specified(self) -> Option<usize> {
        match self {
            Victory::Gold => Some(125),
            Victory::Survive => Some(127),
            Victory::Eliminate | Victory::LastPalace => None,
        }
    }

    /// The goal the dialog shows as the condition is picked with `gold` to
    /// start: the second; for gold, the first of the three
    /// at least [`GOLD_LEAD`] past the start, else that sum.
    #[must_use]
    pub fn goal(self, gold: u32) -> u32 {
        let floor = gold.saturating_add(GOLD_LEAD);

        match self {
            Victory::Gold if GOLD_GOALS[1] < floor => GOLD_GOALS
                .into_iter()
                .find(|g| *g >= floor)
                .unwrap_or(floor),
            _ => self.goals()[1],
        }
    }

    /// `goal` as the dialog keeps it with `gold` to start, each time it
    /// refills (victory dialog fill, after a gold or goal pick): for gold, 0
    /// made the first, then one under [`GOLD_LEAD`] past the start
    /// lifted to the first of the three at least that, else that sum.
    #[must_use]
    pub fn kept(self, goal: u32, gold: u32) -> u32 {
        let floor = gold.saturating_add(GOLD_LEAD);
        let set = if goal == 0 { GOLD_GOALS[0] } else { goal };

        match self {
            Victory::Gold if set < floor => GOLD_GOALS
                .into_iter()
                .find(|g| *g >= floor)
                .unwrap_or(floor),
            Victory::Gold => set,
            _ => goal,
        }
    }

    /// Whether goal `goal` shows greyed, in red markup
    /// ([`crate::sim::world::offer::UNAVAILABLE_MARK`]), with `gold` to start:
    /// a gold goal under [`GOLD_LEAD`] past it.
    #[must_use]
    pub const fn greyed(self, goal: u32, gold: u32) -> bool {
        matches!(self, Victory::Gold) && goal < gold.saturating_add(GOLD_LEAD)
    }
}

/// What a force pick rates, by victory condition: three of its pattern's
/// trailer words (gold, days, the others), which
/// the dialog's difficulty reads (`freestyle_difficulty`, freestyle difficulty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ratings(pub [i32; 3]);

impl Ratings {
    /// A code the quest holds no pattern for, `NONE` or a Random tag:
    /// the default pattern's, 50 each, which the lookup
    /// answers with.
    pub const UNRATED: Ratings = Ratings([50; 3]);

    /// `code`'s in `map`'s dialog entries.
    #[must_use]
    pub fn of(map: &Map<'_>, code: FourCc) -> Ratings {
        let words = entry(map, code).and_then(|u| u.trailer);
        let rated = words.map(|t| [t[0], t[1], t[2]].map(|w| i32::try_from(w).unwrap_or(i32::MAX)));

        rated.map_or(Ratings::UNRATED, Ratings)
    }

    /// The rating `victory` reads.
    #[must_use]
    pub const fn by(self, victory: Victory) -> i32 {
        match victory {
            Victory::Gold => self.0[0],
            Victory::Survive => self.0[1],
            Victory::Eliminate | Victory::LastPalace => self.0[2],
        }
    }
}

/// `#Starting_Gold_Diff_Divider` and `#Starting_Gold_Diff_Threshold`
/// (`mx_Globals.gpl`): starting gold takes `gold / 1000 - 20` off the
/// difficulty.
pub const GOLD_DIVIDER: i32 = 1000;
pub const GOLD_THRESHOLD: i32 = 20;

/// The dialog's difficulty, GPL `CalculateDifficulty` (`mx_LowLevel.gpl`),
/// which freestyle difficulty dispatches with the starting gold, the Player,
/// Enemy and Wandering forces' ratings and the two events' (their `EVSC`
/// numbers, 0 for a code with none): the forces weighted 1, 3, 2, each
/// event above that pulling its half toward it, less the gold's share.
///
/// ```text
///  d  = (player + 3 enemy + 2 wandering) / 6
///  dn = (d + event n) / 2 if event n > d and that is not 0, else d
///  difficulty = (d1 + d2) / 2 − (gold / 1000 − 20)
/// ```
///
/// In 32 bits, wrapping, as the VM's operators.
#[must_use]
pub fn difficulty(gold: u32, [player, enemy, wandering]: [i32; 3], events: [i32; 2]) -> i32 {
    let forces = player
        .wrapping_add(enemy.wrapping_mul(3))
        .wrapping_add(wandering.wrapping_mul(2))
        / 6;
    // `DiffMod` starts 0, and 0 after the pull reads as unpulled.
    let pulled = |event: i32| {
        let half = if event > forces {
            forces.wrapping_add(event) / 2
        } else {
            0
        };

        if half == 0 { forces } else { half }
    };
    let gold = (gold.cast_signed() / GOLD_DIVIDER).wrapping_sub(GOLD_THRESHOLD);

    (pulled(events[0]).wrapping_add(pulled(events[1])) / 2).wrapping_sub(gold)
}

/// The difficulty's word, GPL `CalculateDifficultyString`: one step a
/// 20, at most 4. The dialog shows `GMTX` [`DIFFICULTY_TEXT`] on.
#[must_use]
pub const fn difficulty_step(difficulty: i32) -> usize {
    match difficulty {
        ..20 => 0,
        20..40 => 1,
        40..60 => 2,
        60..80 => 3,
        _ => 4,
    }
}

/// The first difficulty word, `GMTX` 115 "Beginner".
pub const DIFFICULTY_TEXT: usize = 115;

/// "Gather %d gold" (`GMTX` 124): the dialog's three gold goals.
pub const GOLD_GOALS: [u32; 3] = [20_000, 30_000, 40_000];

/// "Survive for %d days" (`GMTX` 126): the dialog's three.
pub const DAY_GOALS: [u32; 3] = [10, 20, 30];

/// A gold goal is at least this past the starting gold:
/// the three below it are greyed.
pub const GOLD_LEAD: u32 = 30_000;

/// Freestyle setup length in words.
const SETUP: usize = 13;

/// A freestyle game's setup: the engine's freestyle config, `0x00` to
/// `0x30`.
///
/// The registry keeps nine of its words, `RGS1` … `RGS9`; a freestyle save
/// keeps all thirteen. Raw words, since not all are decoded:
///
/// | Slot | Reads as |
/// |---|---|
/// | 0, 1, 3 | The starting settlement, the scenario and the land, keyed into `mx_rgstext.cam` as `RGS1`, `RGS2` and `RGS4` are |
/// | 2 | `NONE`, as `RGS3` is |
/// | 4, 5 | 19, 21, 24, 30, 31 or 34; and 0 or 1. Unread; track the scenario, not the map size |
/// | 6 | Starting gold, `RGS5` |
/// | 7, 8 | The victory condition and its goal (`sim::freestyle::Victory`); zero everywhere, as `RGS6` and `RGS7` are |
/// | 9, 10 | The dialog's size boxes, `RGS9` the first; the setup overwrites both from the scenario, so 128 or 512, independent of map size |
/// | 11 | The tileset, `RGS8` — `GRIT`, which `terrtype.cam` names `GRIT Gritty` |
/// | 12 | The seed |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setup {
    /// The thirteen words, in file order.
    pub slots: [u32; SETUP],
}

impl Setup {
    /// Words in a setup.
    pub const LEN: usize = SETUP;

    /// The slots of the four picks ([`Setup::settlement`] and on) and of
    /// the Map Number ([`Setup::seed`]).
    pub const SETTLEMENT_SLOT: usize = 0;
    pub const SCENARIO_SLOT: usize = 1;
    pub const MONSTERS_SLOT: usize = 2;
    pub const LAND_SLOT: usize = 3;
    pub const SEED_SLOT: usize = 12;

    /// The starting settlement, a `SCEN` key: `Wizb` is "Wizard Hold
    /// (Beginner)". See [`crate::data::text::Text::scenario`].
    #[must_use]
    pub const fn settlement(&self) -> FourCc {
        FourCc::from_word(self.slots[Self::SETTLEMENT_SLOT])
    }

    /// The scenario, another `SCEN` key — "Ruins of Esterhausen (Advanced) - S".
    ///
    /// The trailing letter is the map size; see [`MAP_SIZES`](crate::content::terrain::grid::MAP_SIZES).
    #[must_use]
    pub const fn scenario(&self) -> FourCc {
        FourCc::from_word(self.slots[Self::SCENARIO_SLOT])
    }

    /// The Wandering Monsters pick, the dialog's third force list; may be
    /// `NONE`.
    ///
    /// **Binary**: freestyle config, read by freestyle cfg monsters, saved as
    /// `RGS3` — prefs write freestyle writes the config
    /// in `RGS1`…`RGS9` order. Freestyle dialog refresh
    /// fills three lists from the quest's set 1, bucketed by `words[0]`: player
    /// count for Player Forces, 9 Enemy Forces, 10 this list. Only this list
    /// has "None".
    ///
    /// When not `NONE`, freestyle setup merges its
    /// placements into the force chart's template, shifting every later draw.
    /// `NONE` on all 74 corpus setups.
    #[must_use]
    pub const fn monsters(&self) -> FourCc {
        FourCc::from_word(self.slots[Self::MONSTERS_SLOT])
    }

    /// The land, a `LAND` key — `KxHa` is "K'Hal-Kratis".
    #[must_use]
    pub const fn land(&self) -> FourCc {
        FourCc::from_word(self.slots[Self::LAND_SLOT])
    }

    /// The map generator seed — the dialog's "Map Number".
    ///
    /// Confirmed: Map Number 1 to 5 store 1 to 5. Left unchosen, a large
    /// number, constant within a game. Clamped at [`Setup::SEED_MIN`]. See
    /// `docs/formats/saves.md`.
    #[must_use]
    pub const fn seed(&self) -> u32 {
        self.slots[Self::SEED_SLOT]
    }

    /// The lowest seed. Map Number 0 gives the same heightfield as 1; see
    /// **Measured**.
    pub const SEED_MIN: u32 = 1;

    /// Starting gold — 30,000 on every corpus freestyle save.
    ///
    /// Named from prefs: `RGS5` reads 30,000 in a profile saved as `30Kgold`.
    #[must_use]
    pub const fn gold(&self) -> u32 {
        self.slots[6]
    }

    /// The victory condition: config, `RGS6`, which
    /// `GetVictoryConditionIndex` answers. 0 on every corpus setup.
    #[must_use]
    pub const fn victory(&self) -> u32 {
        self.slots[7]
    }

    /// The victory condition's goal: config, `RGS7`, which
    /// `GetVictoryConditionModifier` answers: gold, or days. 0 on every
    /// corpus setup.
    #[must_use]
    pub const fn goal(&self) -> u32 {
        self.slots[8]
    }

    /// The tileset, as the `DTRN` id it names.
    #[must_use]
    pub const fn tileset(&self) -> FourCc {
        FourCc::from_word(self.slots[11])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As the script: an event's pull that comes to 0 is not taken, and
    /// sums wrap as the VM's.
    #[test]
    fn the_difficulty_is_the_scripts() {
        // Gold of 20,000 takes nothing off.
        let gold = 20_000;

        assert_eq!(difficulty(gold, [60, 60, 60], [0, 0]), 60);
        assert_eq!(difficulty(gold, [60, 60, 60], [80, 0]), 65);
        assert_eq!(difficulty(gold, [-6, 0, 0], [1, -5]), -1);
        assert_eq!(difficulty(gold, [i32::MAX; 3], [0, 0]), -1);
    }

    /// A Random tag replaces the drawn code in its slot; a plain code, or
    /// a pick not given, leaves the setup's.
    #[test]
    fn a_setup_keeps_the_random_tags_asked() {
        let choice = Choice {
            settlement: FourCc::new(b"Wizb"),
            scenario: FourCc::new(b"Ruib"),
            monsters: FourCc::new(b"NONE"),
            land: FourCc::new(b"KxHa"),
            seed: 1,
            side: 64,
        };
        let drawn = setup_of(&choice, GOLD);
        let asked = [
            Some(RANDOM_LAND),
            Some(FourCc::new(b"Wizb")),
            Some(Band::Expert.tag()),
            None,
        ];
        let setup = tagged(drawn, asked);

        assert_eq!(setup.land(), RANDOM_LAND);
        assert_eq!(setup.settlement(), FourCc::new(b"Wizb"));
        assert_eq!(setup.scenario(), Band::Expert.tag());
        assert_eq!(setup.monsters(), FourCc::new(b"NONE"));
        assert_eq!(
            tagged(drawn, [Some(Band::Beginner.tag()), None, None, None]),
            drawn
        );
    }
}
