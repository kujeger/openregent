//! Saved freestyle settings: what the setup page's LOAD and SAVE pages
//! (`menu::preset_page`) list, read and write.
//!
//! The original writes the whole config as `freeN.STY` in
//! `FreeStyleIOPath` (`regent_core::view::menu::presets`). This game keeps
//! its own as TOML in [`place`], one file a preset, beside the build
//! trees; it reads none of the original's (no `.STY` file is at hand to
//! check a reader against):
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/freestyle/funbar.toml
//!
//!  version = 1
//!  name = "funbar"          as typed; the file's name is made safe
//!  players = 1              listed only in a session of that many
//!  settlement = "Wizb" ...  the config, Map Number and tree name too
//! ```
//!
//! The tree is kept by name, as the original keeps it: loading takes the
//! tree of that name, else `default` (`menu::freestyle::Custom::loaded`).

use std::path::{Path, PathBuf};

use regent_core::data::fourcc::FourCc;
use regent_core::sim::freestyle::Victory;
use serde::{Deserialize, Serialize};

use super::store;

/// The directory in the data directory: the original's
/// `FreeStyleIOPath` default.
pub const DIRECTORY: &str = "freestyle";

/// The extension of this game's preset files.
const EXTENSION: &str = "toml";

/// A single-player session's player count.
pub const SOLO: u32 = 1;

/// The most characters of a file name made from a preset's name.
const STEM_CHARS: usize = 64;

/// The file name of a preset whose name holds nothing safe.
const BARE_STEM: &str = "settings";

/// A preset file of ours.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    version: u32,
    name: String,
    players: u32,
    settlement: String,
    scenario: String,
    monsters: String,
    land: String,
    map_number: i32,
    gold: u32,
    /// `Victory::word`.
    victory: u32,
    goal: u32,
    events: [String; 2],
    /// The Building Limitations tree's name.
    buildings: String,
}

/// The freestyle config's part a preset keeps
/// (`menu::freestyle::Custom`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub settlement: FourCc,
    pub scenario: FourCc,
    pub monsters: FourCc,
    pub land: FourCc,
    /// As written: the menus raise it to their least on loading.
    pub map_number: i32,
    pub gold: u32,
    pub victory: Victory,
    pub goal: u32,
    pub events: [FourCc; 2],
    /// The Building Limitations tree's name.
    pub buildings: String,
}

/// A saved preset: its name, player count and config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub players: u32,
    pub config: Config,
}

/// Where this game keeps the presets: [`DIRECTORY`] in the XDG data
/// directory, beside the profiles and the build trees.
#[must_use]
pub fn place() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(DIRECTORY))
}

/// The presets in `dir` for a session of `players`, by name; a file
/// that will not read is left out.
#[must_use]
pub fn list(dir: &Path, players: u32) -> Vec<Preset> {
    let mut listed: Vec<Preset> = store::with_extension(dir, EXTENSION)
        .iter()
        .filter_map(|p| read(p).ok())
        .filter(|p| regent_core::view::menu::presets::listed(p.players, players))
        .collect();

    listed.sort_by(|a, b| a.name.cmp(&b.name));
    listed
}

/// `config` named `name` for `players` written to `dir`: over the file
/// holding that name, else a new one. The name kept,
/// trimmed: the config's name from now.
///
/// # Errors
///
/// An empty name, or a directory or file that will not be written.
pub fn save(dir: &Path, name: &str, players: u32, config: &Config) -> Result<String, String> {
    let name = name.trim();

    if name.is_empty() {
        Err("a preset needs a name".to_owned())
    } else {
        store::make_dir(dir)?;

        let path = held(dir, name).unwrap_or_else(|| fresh(dir, name));
        let kept = Kept {
            version: store::VERSION,
            name: name.to_owned(),
            players,
            settlement: config.settlement.to_string(),
            scenario: config.scenario.to_string(),
            monsters: config.monsters.to_string(),
            land: config.land.to_string(),
            map_number: config.map_number,
            gold: config.gold,
            victory: config.victory.word(),
            goal: config.goal,
            events: config.events.map(|e| e.to_string()),
            buildings: config.buildings.clone(),
        };

        store::write(&path, &kept)?;
        Ok(name.to_owned())
    }
}

/// The file in `dir` holding the preset named `name` removed.
///
/// # Errors
///
/// No file holds it, or it will not go.
pub fn delete(dir: &Path, name: &str) -> Result<(), String> {
    let path = held(dir, name).ok_or_else(|| format!("no preset {name}"))?;

    std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The victory whose setup word is `word`.
///
/// # Errors
///
/// A word no victory has.
pub fn victory_of(word: u32) -> Result<Victory, String> {
    Victory::ALL
        .into_iter()
        .find(|v| v.word() == word)
        .ok_or_else(|| format!("victory {word} is not known"))
}

/// The preset at `path`.
fn read(path: &Path) -> Result<Preset, String> {
    let kept: Kept = store::read(path)?;
    let code = |text: &str| regent_core::sim::mapgen::custom::code(text);
    let codes = |texts: &[String; 2]| -> Result<[FourCc; 2], String> {
        Ok([code(&texts[0])?, code(&texts[1])?])
    };

    store::checked(kept.version, path)?;

    let config = Config {
        settlement: code(&kept.settlement)?,
        scenario: code(&kept.scenario)?,
        monsters: code(&kept.monsters)?,
        land: code(&kept.land)?,
        map_number: kept.map_number,
        gold: kept.gold,
        victory: victory_of(kept.victory)?,
        goal: kept.goal,
        events: codes(&kept.events)?,
        buildings: kept.buildings,
    };

    Ok(Preset {
        name: kept.name,
        players: kept.players,
        config,
    })
}

/// Our file in `dir` holding the preset named `name`, whatever its
/// player count: the original finds a file by name alone.
fn held(dir: &Path, name: &str) -> Option<PathBuf> {
    store::with_extension(dir, EXTENSION)
        .into_iter()
        .find(|p| read(p).is_ok_and(|preset| preset.name == name))
}

/// A new file in `dir` for the preset named `name`: its name made safe,
/// numbered past any file already there.
///
/// ```text
///  "funbar"      funbar.toml
///  "a/b", "a?b"  a_b.toml, a_b-2.toml
/// ```
fn fresh(dir: &Path, name: &str) -> PathBuf {
    let short: String = name.chars().take(STEM_CHARS).collect();
    let stem = store::stem(&short).unwrap_or_else(|| BARE_STEM.to_owned());

    store::fresh(dir, &stem, EXTENSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("presets-{name}"))
    }

    /// Every field off its default.
    fn odd() -> Config {
        Config {
            settlement: FourCc::new(b"Wizb"),
            scenario: FourCc::new(b"Ruib"),
            monsters: FourCc::new(b"EXPE"),
            land: FourCc::new(b"KxHa"),
            map_number: 680_631_669,
            gold: 30_000,
            victory: Victory::Survive,
            goal: 20,
            events: [FourCc::new(b"EV01"), FourCc::new(b"EV07")],
            buildings: "No Inns".to_owned(),
        }
    }

    #[test]
    fn a_saved_preset_lists_and_reads_back_every_field() {
        let dir = scratch("round_trip");

        assert_eq!(save(&dir, " funbar ", SOLO, &odd()).unwrap(), "funbar");

        let listed = list(&dir, SOLO);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "funbar", "named as saved");
        assert_eq!(listed[0].config, odd());
    }

    #[test]
    fn saving_a_name_again_overwrites_and_delete_removes_it() {
        let dir = scratch("overwrite");
        let mut config = odd();

        save(&dir, "funbar", SOLO, &config).unwrap();
        config.gold = 10_000;
        save(&dir, "funbar", SOLO, &config).unwrap();

        let listed = list(&dir, SOLO);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].config.gold, 10_000);

        delete(&dir, "funbar").unwrap();

        assert_eq!(list(&dir, SOLO).len(), 0);
        assert!(delete(&dir, "funbar").is_err());
    }

    #[test]
    fn odd_names_keep_apart_and_come_back_as_typed() {
        let dir = scratch("odd_names");
        let names = [
            "a/b",
            "a?b",
            "../..",
            "Ærlig talt *&'",
            "x".repeat(200).leak(),
        ];

        for name in names {
            save(&dir, name, SOLO, &odd()).unwrap();
        }

        let mut back: Vec<String> = list(&dir, SOLO).into_iter().map(|p| p.name).collect();
        let mut want: Vec<String> = names.iter().map(|&n| n.to_owned()).collect();

        back.sort();
        want.sort();
        assert_eq!(back, want);
        assert_eq!(
            store::with_extension(&dir, EXTENSION).len(),
            names.len(),
            "inside the directory"
        );
        assert!(save(&dir, "  ", SOLO, &odd()).is_err());
    }

    #[test]
    fn a_session_lists_its_player_counts_presets_alone() {
        let dir = scratch("players");

        save(&dir, "solo", SOLO, &odd()).unwrap();
        save(&dir, "three", 3, &odd()).unwrap();

        let names =
            |players| -> Vec<String> { list(&dir, players).into_iter().map(|p| p.name).collect() };

        assert_eq!(names(SOLO), ["solo"]);
        assert_eq!(names(3), ["three"]);
    }
}
