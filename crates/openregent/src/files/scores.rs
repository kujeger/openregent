//! The high scores, kept: openregent's own file, not the original's
//! `hiscores.sav` (`regent_core::view::menu::scores::FILE`). One table for the
//! install, as the original keeps one; each entry names its monarch.
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/scores.toml   version = 1
//!    [[entries]]  game = "quests", category = "gold", monarch, title,
//!                 value, hero, class      each list in its shown order
//! ```
//!
//! A won game is recorded as the original's end record victory
//! records it ([`record`]); the screen reads the file as it opens.

use std::path::{Path, PathBuf};

use bevy::prelude::Resource;
use regent_core::view::menu::scores::{Category, Entry, GameKind, Hero, Table};
use serde::{Deserialize, Serialize};

/// The file, in [`crate::files::env::OWN`].
const FILE: &str = "scores.toml";

/// Where the table is written as a game is won, and whether this game's
/// win is: the place none, nowhere (a screenshot writes nothing).
#[derive(Resource, Default)]
pub struct Scoring {
    pub place: Option<PathBuf>,
    /// This game's win recorded; cleared as a game starts.
    pub scored: bool,
}

/// `scores.toml`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    entries: Vec<Written>,
}

/// One entry as written.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Written {
    game: String,
    category: String,
    monarch: String,
    title: String,
    value: i32,
    hero: Option<String>,
    class: Option<String>,
}

/// Where the table is kept: [`FILE`] in the XDG data directory, as the
/// profiles.
#[must_use]
pub fn place() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(FILE))
}

/// A game's word in the file.
const fn game_word(game: GameKind) -> &'static str {
    match game {
        GameKind::Multiplayer => "multiplayer",
        GameKind::Freestyle => "freestyle",
        GameKind::Quests => "quests",
    }
}

/// A category's word in the file.
const fn category_word(category: Category) -> &'static str {
    match category {
        Category::Gold => "gold",
        Category::City => "city",
        Category::Level => "level",
        Category::Time => "time",
        Category::Heroes => "heroes",
    }
}

/// The table kept at `path`; none kept yet, an empty one.
///
/// # Errors
///
/// A file that will not read, of another version, or naming a game or
/// category not known.
pub fn read(path: &Path) -> Result<Table, String> {
    let mut table = Table::default();

    if path.exists() {
        let file: File = crate::files::store::read(path)?;

        crate::files::store::checked(file.version, path)?;

        for w in file.entries {
            let game = GameKind::ALL
                .into_iter()
                .find(|g| game_word(*g) == w.game)
                .ok_or_else(|| format!("{}: no game {}", path.display(), w.game))?;
            let category = Category::ALL
                .into_iter()
                .find(|c| category_word(*c) == w.category)
                .ok_or_else(|| format!("{}: no category {}", path.display(), w.category))?;
            let hero = w
                .hero
                .zip(w.class)
                .map(|(name, class)| Hero { name, class });
            let entry = Entry {
                monarch: w.monarch,
                game: w.title,
                value: w.value,
                hero,
            };

            table.restore(game, category, entry);
        }
    }

    Ok(table)
}

/// `table` kept at `path`, each list in its order.
///
/// # Errors
///
/// A directory or file that will not be written.
pub fn write(path: &Path, table: &Table) -> Result<(), String> {
    let entries = GameKind::ALL
        .into_iter()
        .flat_map(|g| Category::ALL.into_iter().map(move |c| (g, c)))
        .flat_map(|(g, c)| {
            table.list(g, c).iter().map(move |e| Written {
                game: game_word(g).to_owned(),
                category: category_word(c).to_owned(),
                monarch: e.monarch.clone(),
                title: e.game.clone(),
                value: e.value,
                hero: e.hero.as_ref().map(|h| h.name.clone()),
                class: e.hero.as_ref().map(|h| h.class.clone()),
            })
        })
        .collect();
    let file = File {
        version: crate::files::store::VERSION,
        entries,
    };

    crate::files::store::make_parent(path)?;
    crate::files::store::write(path, &file)
}

/// A won `game`'s `entries` recorded in the table at `path`: read, added,
/// written, as `GS_ScoringSystem` loads and saves around its adds.
///
/// # Errors
///
/// A file that will not read or be written.
pub fn record(path: &Path, game: GameKind, entries: Vec<(Category, Entry)>) -> Result<(), String> {
    let mut table = read(path)?;

    table.record_all(game, entries);
    write(path, &table)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch file, `name` in a folder of its own: the folder, kept
    /// while the file is used, and the file's path.
    fn scratch(name: &str) -> (crate::testdata::Scratch, PathBuf) {
        let dir = crate::testdata::scratch("scores");
        let path = dir.join(name);

        (dir, path)
    }

    fn entry(value: i32, hero: Option<Hero>) -> Entry {
        Entry {
            monarch: "Your Majesty".to_owned(),
            game: "The Barren Waste".to_owned(),
            value,
            hero,
        }
    }

    #[test]
    fn a_kept_table_comes_back_in_its_order() {
        let (_dir, path) = scratch("round_trip.toml");
        let hero = Hero {
            name: "Sir Lucas Silveredge".to_owned(),
            class: "Warrior".to_owned(),
        };

        assert_eq!(read(&path), Ok(Table::default()));

        record(
            &path,
            GameKind::Quests,
            vec![
                (Category::Gold, entry(500, None)),
                (Category::Level, entry(9, Some(hero))),
            ],
        )
        .expect("recorded");
        record(
            &path,
            GameKind::Quests,
            vec![(Category::Gold, entry(900, None))],
        )
        .expect("recorded");
        record(
            &path,
            GameKind::Freestyle,
            vec![(Category::Time, entry(60_000, None))],
        )
        .expect("recorded");

        let table = read(&path).expect("read");
        let golds: Vec<i32> = table
            .list(GameKind::Quests, Category::Gold)
            .iter()
            .map(|e| e.value)
            .collect();

        assert_eq!(golds, [900, 500]);
        assert_eq!(
            table.list(GameKind::Quests, Category::Level)[0]
                .hero
                .as_ref()
                .map(|h| h.class.as_str()),
            Some("Warrior")
        );
        assert_eq!(table.list(GameKind::Freestyle, Category::Time).len(), 1);
    }
}
