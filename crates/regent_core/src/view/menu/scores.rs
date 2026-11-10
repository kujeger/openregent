//! The high scores: what a won game records and the HIGH SCORES screen
//! shows. **Binary** unless marked.
//!
//! ```text
//!  a game won ──end record victory     ──> GS_ScoringSystem     (game)
//!     quest 2, freestyle 1, network 0       each category: the best
//!                                           player's entry added, the
//!                                           worst dropped past 10
//!                                           (sim::world::score)
//!  main menu HIGH SCORES 5010 ──> AP64 (high scores screen)
//!     EPIC QUESTS 7002 · MULTIPLAYER 7000 · FREESTYLE 7001     the game
//!     MOST HEROES 6004 · MOST GOLD 6000 · LARGEST CITY 6001 ·
//!     HIGHEST LEVEL 6002 · BEST TIME 6003                      the category
//!     12 the category's GMTX 175+, list 8001 a row each, GMTX 170+
//!     ERASE SCORES 9001: the game's five lists, asked nothing
//!     MAIN MENU 9002 (Escape): the file written, back to AP29
//! ```
//!
//! The screen opens on EPIC QUESTS, MOST HEROES. The engine keeps the
//! table in `hiscores.sav` under the user's directory
//! (`"hiscores.sav"`), read before each record and as the screen opens, written
//! after; its layout is not read.

use crate::data::fourcc::FourCc;
use crate::data::text::{Arg, printf};
use crate::sim::world::score::Scores;
use crate::view::panel::day::Day;

/// The screen, `SMNU AP64`.
pub const PANEL: FourCc = FourCc(*b"AP64");

/// The entries a list keeps: past it the worst goes (`GS_ScoringSystem`,
/// a compare with 10).
pub const KEPT: usize = 10;

/// [`PANEL`]'s controls.
pub mod id {
    /// ERASE SCORES: the shown game's lists emptied.
    pub const ERASE: i32 = 9001;
    /// MAIN MENU, hot key Escape.
    pub const MAIN_MENU: i32 = 9002;
    /// The list, a row an entry: `fnt7`, rows [`super::ROW`] high.
    pub const LIST: i32 = 8001;
    /// The category's definition, `GMTX` 175 up.
    pub const DEFINITION: i32 = 12;
    /// The list's scroll bar.
    pub const BAR: i32 = 13;
    /// The title, "High Scores", `fn11`; the pog, `INTD` 1039 as the
    /// panel gives it, the screen sets none.
    pub use crate::view::menu::ids::POG;
    pub use crate::view::menu::ids::TITLE;
}

/// The list's row pitch, layout pixels: 8001's
/// [`smnu::ROW_HEIGHT`](crate::data::smnu::ROW_HEIGHT).
pub const ROW: i32 = 20;

/// What kind of game a list is for: the radio buttons 7000 up, the
/// argument end record victory passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GameKind {
    /// A network game (quest music kind 2).
    Multiplayer = 0,
    /// A custom game (kind 0).
    Freestyle = 1,
    /// A quest (kind 1).
    Quests = 2,
}

impl GameKind {
    pub const ALL: [GameKind; 3] = [GameKind::Multiplayer, GameKind::Freestyle, GameKind::Quests];

    /// Its radio button: MULTIPLAYER 7000, FREESTYLE 7001, EPIC QUESTS
    /// 7002.
    #[must_use]
    pub const fn control(self) -> i32 {
        7000 + self as i32
    }
}

/// A list's category: the radio buttons 6000 up, `GS_ScoringSystem`'s
/// pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Gold = 0,
    City = 1,
    Level = 2,
    Time = 3,
    Heroes = 4,
}

impl Category {
    pub const ALL: [Category; 5] = [
        Category::Gold,
        Category::City,
        Category::Level,
        Category::Time,
        Category::Heroes,
    ];

    /// Its radio button: MOST GOLD 6000, LARGEST CITY 6001, HIGHEST LEVEL
    /// 6002, BEST TIME 6003, MOST HEROES 6004.
    #[must_use]
    pub const fn control(self) -> i32 {
        6000 + self as i32
    }

    /// Its row's `GMTX` format, 170 up: "%s collected %d gold while
    /// playing %s.", city, hero, time, heroes.
    #[must_use]
    pub const fn row_text(self) -> usize {
        170 + self as usize
    }

    /// Its definition's `GMTX` text, 175 up: "These monarchs hold the
    /// record for ...".
    #[must_use]
    pub const fn definition_text(self) -> usize {
        175 + self as usize
    }

    /// Whether a larger value is better: all but [`Category::Time`].
    const fn more_is_better(self) -> bool {
        !matches!(self, Category::Time)
    }
}

/// What the screen opens on (high scores screen: 7002 and 6004 checked).
pub const OPENS: (GameKind, Category) = (GameKind::Quests, Category::Heroes);

/// A hero, as an entry names it: its name and its class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hero {
    pub name: String,
    pub class: String,
}

/// One entry: fields 0 to 4 of the engine's six; field 5 repeats the
/// level for a hero and is empty otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Field 0: the player's name.
    pub monarch: String,
    /// Field 1: the game's title (quest title of `CURR`).
    pub game: String,
    /// Field 2: what is ranked; ms for [`Category::Time`].
    pub value: i32,
    /// Fields 3 and 4: [`Category::Level`]'s hero.
    pub hero: Option<Hero>,
}

/// The entries `scores` gives, by category, for `monarch` playing
/// `game`: MOST GOLD, LARGEST CITY and BEST TIME always, HIGHEST LEVEL
/// with a champion, MOST HEROES above 0.
#[must_use]
pub fn entries(scores: &Scores, monarch: &str, game: &str) -> Vec<(Category, Entry)> {
    let entry = |value, hero| Entry {
        monarch: monarch.to_owned(),
        game: game.to_owned(),
        value,
        hero,
    };
    let level = scores.champion.as_ref().map(|c| {
        let hero = Hero {
            name: c.name.clone(),
            class: c.class.clone(),
        };

        (Category::Level, entry(c.level, Some(hero)))
    });
    let heroes = (scores.heroes > 0).then(|| (Category::Heroes, entry(scores.heroes, None)));
    let time = i32::try_from(scores.time).unwrap_or(i32::MAX);

    [
        Some((Category::Gold, entry(scores.gold, None))),
        Some((Category::City, entry(scores.city, None))),
        level,
        Some((Category::Time, entry(time, None))),
        heroes,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The lists, by game and category, each in the engine's order.
///
/// ```text
///  stored    the store's sort: field 2 greatest first, but BEST TIME,
///            which no sort names, by atoi of field 0, the name; an
///            equal goes after (score insert, score order)
///  trimmed   past 10, the worst value goes, the first of a tie in
///            stored order ('s multimap: score trim less, score trim greater)
/// ```
///
/// So BEST TIME shows in the order won, its slowest dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    lists: [[Vec<Entry>; 5]; 3],
}

impl Table {
    /// `game`'s `category` list, as shown.
    #[must_use]
    pub fn list(&self, game: GameKind, category: Category) -> &[Entry] {
        &self.lists[game as usize][category as usize]
    }

    /// `entry` added to `game`'s `category` list where the store's sort
    /// puts it, then the worst dropped while more than [`KEPT`].
    pub fn record(&mut self, game: GameKind, category: Category, entry: Entry) {
        let list = &mut self.lists[game as usize][category as usize];
        let key = sort_key(category, &entry);
        let at = list
            .iter()
            .position(|e| key > sort_key(category, e))
            .unwrap_or(list.len());

        list.insert(at, entry);

        while list.len() > KEPT {
            let worst = worst(category, list);

            list.remove(worst);
        }
    }

    /// Every entry of a won game added.
    pub fn record_all(&mut self, game: GameKind, entries: Vec<(Category, Entry)>) {
        for (category, entry) in entries {
            self.record(game, category, entry);
        }
    }

    /// `entry` put last on `game`'s `category` list as it stands: a kept
    /// table read back in its order.
    pub fn restore(&mut self, game: GameKind, category: Category, entry: Entry) {
        self.lists[game as usize][category as usize].push(entry);
    }

    /// ERASE SCORES: `game`'s five lists emptied.
    pub fn erase(&mut self, game: GameKind) {
        for list in &mut self.lists[game as usize] {
            list.clear();
        }
    }
}

/// What the store sorts `entry` by, greatest first: field 2, or for
/// [`Category::Time`] field 0 read as `atoi` reads it.
fn sort_key(category: Category, entry: &Entry) -> i32 {
    if category.more_is_better() {
        entry.value
    } else {
        atoi(&entry.monarch)
    }
}

/// The index of `list`'s worst value, the first of a tie.
fn worst(category: Category, list: &[Entry]) -> usize {
    let better = |a: i32, b: i32| {
        if category.more_is_better() {
            a > b
        } else {
            a < b
        }
    };

    (0..list.len())
        .reduce(|w, i| {
            if better(list[w].value, list[i].value) {
                i
            } else {
                w
            }
        })
        .unwrap_or(0)
}

/// `text` as C's `atoi` reads it: blanks skipped, a sign, then digits;
/// 0 for none. **Inferred**: past `i32` it saturates, as MSVC's does.
fn atoi(text: &str) -> i32 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let (sign, digits) = match text.as_bytes().first() {
        Some(b'-') => (-1, &text[1..]),
        Some(b'+') => (1, &text[1..]),
        _ => (1, text),
    };

    digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i32, |n, d| {
            n.saturating_mul(10)
                .saturating_add(sign * i32::from(d - b'0'))
        })
}

/// `entry`'s row on `category`'s list, `gmtx` giving a
/// `GMTX` string; `None` without its format.
///
/// ```text
///  170 gold, 171 city, 174 heroes   monarch, value, game
///  172 level                        monarch, class, name, level, game
///  173 time                         monarch, game, the value as days
///                                   (day text)
/// ```
#[must_use]
pub fn row(
    category: Category,
    entry: &Entry,
    gmtx: &dyn Fn(usize) -> Option<String>,
) -> Option<String> {
    let format = gmtx(category.row_text())?;
    let (monarch, game) = (entry.monarch.as_str(), entry.game.as_str());
    let (name, class) = entry
        .hero
        .as_ref()
        .map_or(("", ""), |h| (h.name.as_str(), h.class.as_str()));
    let days = Day::of(entry.value.max(0).cast_unsigned()).text_by(gmtx);
    let args: Vec<Arg<'_>> = match category {
        Category::Gold | Category::City | Category::Heroes => {
            vec![monarch.into(), entry.value.into(), game.into()]
        }
        Category::Level => vec![
            monarch.into(),
            class.into(),
            name.into(),
            entry.value.into(),
            game.into(),
        ],
        Category::Time => vec![
            monarch.into(),
            game.into(),
            days.as_deref().unwrap_or_default().into(),
        ],
    };

    Some(printf(&format, &args))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(monarch: &str, value: i32) -> Entry {
        Entry {
            monarch: monarch.to_owned(),
            game: "The Barren Waste".to_owned(),
            value,
            hero: None,
        }
    }

    fn values(table: &Table, category: Category) -> Vec<i32> {
        table
            .list(GameKind::Quests, category)
            .iter()
            .map(|e| e.value)
            .collect()
    }

    #[test]
    fn a_list_keeps_the_ten_best_greatest_first() {
        let mut table = Table::default();

        for value in [5, 30, 10, 20, 1, 7, 9, 12, 40, 3, 2, 8] {
            table.record(GameKind::Quests, Category::Gold, entry("a", value));
        }

        assert_eq!(
            values(&table, Category::Gold),
            [40, 30, 20, 12, 10, 9, 8, 7, 5, 3]
        );
    }

    #[test]
    fn a_tie_at_the_bottom_drops_the_older_entry() {
        let mut table = Table::default();

        for n in 0..KEPT {
            table.record(
                GameKind::Quests,
                Category::City,
                entry(&format!("old {n}"), 5),
            );
        }

        table.record(GameKind::Quests, Category::City, entry("new", 5));

        let list = table.list(GameKind::Quests, Category::City);

        assert_eq!(list.len(), KEPT);
        assert_eq!(list.last().map(|e| e.monarch.as_str()), Some("new"));
        assert_eq!(list.first().map(|e| e.monarch.as_str()), Some("old 1"));
    }

    #[test]
    fn best_time_keeps_the_order_won_and_drops_the_slowest() {
        let mut table = Table::default();

        for value in [500, 100, 900, 300, 700, 200, 800, 400, 600, 1000, 50] {
            table.record(
                GameKind::Quests,
                Category::Time,
                entry("Your Majesty", value),
            );
        }

        assert_eq!(
            values(&table, Category::Time),
            [500, 100, 900, 300, 700, 200, 800, 400, 600, 50]
        );
    }

    #[test]
    fn best_time_sorts_by_a_name_read_as_a_number() {
        let mut table = Table::default();

        for name in ["Ann", "12 Kings", "-3", "7th Son"] {
            table.record(GameKind::Quests, Category::Time, entry(name, 1));
        }

        let names: Vec<&str> = table
            .list(GameKind::Quests, Category::Time)
            .iter()
            .map(|e| e.monarch.as_str())
            .collect();

        assert_eq!(names, ["12 Kings", "7th Son", "Ann", "-3"]);
    }

    #[test]
    fn erase_empties_one_games_lists() {
        let mut table = Table::default();

        table.record(GameKind::Quests, Category::Gold, entry("a", 1));
        table.record(GameKind::Freestyle, Category::Gold, entry("a", 1));
        table.erase(GameKind::Quests);

        assert_eq!(table.list(GameKind::Quests, Category::Gold).len(), 0);
        assert_eq!(table.list(GameKind::Freestyle, Category::Gold).len(), 1);
    }

    #[test]
    fn atoi_reads_as_c_does() {
        assert_eq!(atoi("  42abc"), 42);
        assert_eq!(atoi("-7"), -7);
        assert_eq!(atoi("Your Majesty"), 0);
        assert_eq!(atoi("99999999999"), i32::MAX);
    }

    #[test]
    fn a_won_game_records_every_category_it_reaches() {
        let scores = Scores {
            gold: 0,
            city: 3,
            champion: None,
            time: 60_000,
            heroes: 0,
        };
        let got: Vec<Category> = entries(&scores, "Your Majesty", "Freestyle GameKind")
            .into_iter()
            .map(|(c, _)| c)
            .collect();

        assert_eq!(got, [Category::Gold, Category::City, Category::Time]);
    }

    #[test]
    fn controls_name_their_game_and_category() {
        assert_eq!(Category::Heroes.row_text(), 174);
        assert_eq!(Category::Gold.definition_text(), 175);
    }
}
