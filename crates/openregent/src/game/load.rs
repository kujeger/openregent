//! What a game is made from: the install's files, the GPL program, the
//! quests offered, a new world, and the custom-game dialog's lists.
//!
//! ```text
//!  picks ──> regent_core::sim::mapgen::fresh: the charts, the start ──> setup::fresh ──> World
//!  --quest: its .q and dataset ──> regent_core::sim::mapgen::quest ──> the same
//!  SDK images ──> Program ──> Vm
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bevy::log::info;
use regent_core::content::dataset::{Bytecode, Datasets, Files, Install};
use regent_core::content::quest::{self, Quest};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::cp1250;
use regent_core::data::deps;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs;
use regent_core::data::text::Text;
use regent_core::gpl::load::Program;
use regent_core::sim::freestyle::Setup;
use regent_core::sim::freestyle::{self, List, RANDOM_LAND, Victory};
use regent_core::sim::scene::{Ground, Scene};
use regent_core::sim::setup;
use regent_core::sim::world::World;
use regent_core::view::panel::scroll;

use regent_core::sim::freestyle::Picks;
use regent_core::sim::mapgen::custom::{self, Art};

use crate::files::env::QUEST;

/// The quest list's dependency table: its order is the list's
/// ([`regent_core::content::quest`]).
pub(crate) const QUEST_ORDER: FourCc = FourCc::new(b"QDEP");

/// `GMTX` 110, "Random": the land's Random pick ([`randoms`]).
const RANDOM_LAND_TEXT: usize = 110;

/// The expansion's dataset files, which the menus and a freestyle game
/// load.
///
/// # Errors
///
/// A definition file that will not read, or a file not on disk.
fn expansion(hd: &Path) -> Result<Files, String> {
    Install::new(hd, Bytecode::Install)
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())
}

/// The expansion's archives in load order: a later one's record wins.
///
/// # Errors
///
/// A definition file that will not read, or an archive not on disk.
pub fn archives(hd: &Path) -> Result<Vec<PathBuf>, String> {
    expansion(hd).map(|f| f.archives)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// A program loaded, and its images' hash (`save::hash`).
struct Loaded {
    program: &'static Program,
    hash: String,
}

/// Programs loaded, by their images' paths: each list loads and hashes
/// once a session, and every game on it shares it.
static PROGRAMS: Mutex<BTreeMap<Vec<PathBuf>, Loaded>> = Mutex::new(BTreeMap::new());

/// `paths`, GPL images in load order, loaded once and kept for the
/// process ([`PROGRAMS`]): a dataset's, not a game's.
///
/// # Errors
///
/// A missing image, or one that will not load.
pub fn images(paths: &[PathBuf]) -> Result<&'static Program, String> {
    loaded(paths, |l| l.program)
}

/// Programs with mods' `.dat` member values set, by images and patches.
type Patched = BTreeMap<(Vec<PathBuf>, Vec<(PathBuf, String)>), &'static Program>;

/// Programs [`patched`] made: each once a process, as [`PROGRAMS`].
static PATCHED: Mutex<Patched> = Mutex::new(BTreeMap::new());

/// [`images`], then `patches`' `[[prototype]]` values set
/// (`Program::with_members`); the images' own program when none sets
/// any.
///
/// # Errors
///
/// As [`images`]; a patch naming an instance or member the program
/// lacks, or a value its member cannot hold.
pub fn patched(
    paths: &[PathBuf],
    patches: &[(PathBuf, String)],
) -> Result<&'static Program, String> {
    let program = images(paths)?;
    let mut made = PATCHED.lock().map_err(|e| e.to_string())?;
    let key = (paths.to_vec(), patches.to_vec());

    if let Some(p) = made.get(&key) {
        Ok(*p)
    } else {
        let p = match program.with_members(patches).map_err(|e| e.to_string())? {
            Some(p) => Box::leak(Box::new(p)),
            None => program,
        };

        made.insert(key, p);
        Ok(p)
    }
}

/// The images at `paths` hashed ([`crate::save::hash`]), once a process
/// as [`images`] loads them.
///
/// # Errors
///
/// A missing image, or one that will not load.
pub fn images_hash(paths: &[PathBuf]) -> Result<String, String> {
    loaded(paths, |l| l.hash.clone())
}

/// What `of` takes of the program on `paths`, loaded if not yet.
fn loaded<T>(paths: &[PathBuf], of: impl FnOnce(&Loaded) -> T) -> Result<T, String> {
    let mut programs = PROGRAMS.lock().map_err(|e| e.to_string())?;

    if let Some(loaded) = programs.get(paths) {
        Ok(of(loaded))
    } else {
        let images = paths
            .iter()
            .map(|p| read(p))
            .collect::<Result<Vec<_>, _>>()?;
        let program = Program::load(&images).map_err(|e| format!("GPL images: {e}"))?;
        let loaded = Loaded {
            program: Box::leak(Box::new(program)),
            hash: crate::save::hash(images.iter().map(Vec::as_slice)),
        };
        let taken = of(&loaded);

        programs.insert(paths.to_vec(), loaded);
        Ok(taken)
    }
}

/// A quest the install or a mod folder offers, its title
/// ([`Quest::title`]) and where it was found.
pub struct Offered {
    pub quest: Quest,
    pub title: String,
    pub origin: Origin,
}

/// Where an offered quest was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// The install's `Quests` or `QuestsMX`.
    Install,
    /// A mod folder's item folder (`content::mods::Found`).
    Mod(PathBuf),
}

impl Offered {
    /// What `--quest` names it by, and a recording keeps: its key, or a
    /// mod's quest's whole id, as its key may be a shipped quest's.
    #[must_use]
    pub fn named(&self) -> String {
        match self.origin {
            Origin::Install => self.quest.key.to_string(),
            Origin::Mod(_) => self.quest.id.to_string(),
        }
    }
}

/// Every quest the install offers, in `QDEP`'s order, then those `found`
/// in mod folders, in theirs.
#[must_use]
pub fn quests(
    hd: &Path,
    catalogue: &Catalogue<'_>,
    found: &[regent_core::content::mods::Found],
) -> Vec<Offered> {
    let text = Text::build(catalogue);
    let order: Vec<FourCc> = catalogue
        .find(b"DATA", QUEST_ORDER)
        .and_then(|at| catalogue.record(b"DATA", at))
        .and_then(|r| deps::quests(&String::from_utf8_lossy(r.data)).ok())
        .map(|rules| rules.iter().map(|r| r.id).collect())
        .unwrap_or_default();

    let installed = quest::offered(hd, &order)
        .into_iter()
        .map(|quest| (quest, Origin::Install));
    let modded = found
        .iter()
        .map(|f| (f.quest.clone(), Origin::Mod(f.package.clone())));

    installed
        .chain(modded)
        .map(|(quest, origin)| Offered {
            title: quest.title(&text),
            quest,
            origin,
        })
        .collect()
}

/// A quest's new game on map word `map_word`, the clock's stand-in
/// ([`regent_core::sim::mapgen::quest`]), founded by the library ([`setup::fresh`]);
/// `art` the catalogue's.
///
/// # Errors
///
/// A quest or chart that will not read, or a land the quest lacks.
pub fn quest_fresh(
    (catalogue, art): (&Catalogue<'_>, &Art),
    program: &Program,
    quest: &Quest,
    chart: &Path,
    map_word: i32,
) -> Result<World, String> {
    let template = quest
        .template_path()
        .ok_or_else(|| format!("{}: not found", quest.dir.join(&quest.template).display()))?;
    let (map, patterns) = (read(&template)?, read(chart)?);
    let map = map_of(&map, &template.display())?;
    let patterns = chart_of(&patterns, &chart.display())?;
    let made = regent_core::sim::mapgen::quest::generate(&map, &patterns, map_word, art)?;
    let world = setup::fresh_by(catalogue, program, &made.start(), &art.descriptors);

    info!(
        "quest {} ({}): map word {map_word:#010x}, {} cells, {} units at the start",
        quest.key,
        quest.name,
        made.choice.side,
        made.starts.len()
    );

    Ok(world)
}

/// A new game with no save from `picks` and its special `events`, `gold`
/// each seat and the victory `aim` ([`regent_core::sim::mapgen::fresh`]), founded by the library
/// ([`setup::fresh`]): the world, and its setup, a Random pick's tag kept
/// ([`freestyle::tagged`]). `art` the catalogue's.
///
/// # Errors
///
/// A quest or chart that will not read, a scenario or land the quest
/// lacks.
pub fn fresh(
    hd: &Path,
    (catalogue, art): (&Catalogue<'_>, &Art),
    program: &Program,
    (picks, events): (&Picks, [FourCc; 2]),
    gold: u32,
    aim: (Victory, u32),
) -> Result<(World, Setup), String> {
    let (map, (path, patterns)) = (quest(hd)?, chart(hd)?);
    let map = map_of(&map, &QUEST)?;
    let patterns = chart_of(&patterns, &path.display())?;
    let choice = custom::chosen(picks, &map)?;
    let mut made = regent_core::sim::mapgen::fresh::generate_for(
        &map,
        &patterns,
        choice,
        (gold, picks.players()),
        art,
    )?;

    made.setup = freestyle::tagged(
        freestyle::with_victory(made.setup, aim.0, aim.1),
        picks.asked(),
    );
    made.events = events;

    let world = setup::fresh_by(catalogue, program, &made.start(), &art.descriptors);

    info!(
        "map: {} {} {} {}, seed {}, {} cells, {} units at the start",
        choice.land,
        choice.settlement,
        choice.scenario,
        choice.monsters,
        choice.seed,
        choice.side,
        made.starts.len()
    );

    Ok((world, made.setup))
}

/// `scene`'s new game: a flat map made by the library, else its picks
/// generated ([`regent_core::sim::mapgen::fresh::scene`]); founded by the library
/// ([`setup::fresh`]): the world, and its setup. `art` the catalogue's.
///
/// # Errors
///
/// A quest or chart that will not read, picks the dialog refuses, a unit
/// the scene cannot place.
pub fn scene_world(
    hd: &Path,
    (catalogue, art): (&Catalogue<'_>, &Art),
    program: &Program,
    scene: &Scene,
) -> Result<(World, Setup), String> {
    let (world, setup) = match scene.map {
        Ground::Flat { .. } => (
            scene
                .flat_world(catalogue, program)
                .map_err(|e| e.to_string())?,
            scene.setup(),
        ),
        Ground::Freestyle(_) => {
            let (map, (path, patterns)) = (quest(hd)?, chart(hd)?);
            let map = map_of(&map, &QUEST)?;
            let patterns = chart_of(&patterns, &path.display())?;
            let made = regent_core::sim::mapgen::fresh::scene(&map, &patterns, scene, art)?;

            (
                setup::fresh_by(catalogue, program, &made.start(), &art.descriptors),
                made.setup,
            )
        }
    };

    info!(
        "scene: {} units placed, day {} second {}",
        scene.units.len(),
        scene.day,
        scene.second
    );

    Ok((world, setup))
}

/// The entries in `list` of the dialog of `players`: code and name, as
/// the dialog spells it.
pub fn offers(hd: &Path, list: List, players: u32) -> Result<Vec<(FourCc, String)>, String> {
    let quest = quest(hd)?;
    let map = map_of(&quest, &QUEST)?;

    Ok(freestyle::offers(&map, list, players)
        .iter()
        .map(|o| (o.code, cp1250::decode(o.name).trim_end().to_owned()))
        .collect())
}

/// What each of `codes` rates in the dialog's difficulty
/// ([`freestyle::Ratings`]).
pub fn ratings(hd: &Path, codes: &[FourCc]) -> Result<Vec<freestyle::Ratings>, String> {
    let quest = quest(hd)?;
    let map = map_of(&quest, &QUEST)?;

    Ok(codes
        .iter()
        .map(|&code| freestyle::Ratings::of(&map, code))
        .collect())
}

/// The Random picks of `list`: the land's `RAND`, named `GMTX` 110
/// "Random" (**Inferred**: the dialog's name for it is not read), a force
/// list's three bands, named as the title scroll names them
/// ([`scroll::RANDOM`], `GMTX` 111–113); a text missing, its code.
#[must_use]
pub fn randoms(list: List, text: &Text<'_>) -> Vec<(FourCc, String)> {
    let named = |code: FourCc, index: usize| {
        let name = text
            .gmtx(index)
            .map_or_else(|| code.to_string(), |t| t.trim().to_owned());

        (code, name)
    };

    match list {
        List::Land => vec![named(RANDOM_LAND, RANDOM_LAND_TEXT)],
        _ => scroll::RANDOM
            .into_iter()
            .map(|(code, index)| named(code, index))
            .collect(),
    }
}

/// The Wandering Monsters list's own pick of none, `NONE`'s name:
/// `GMTX` [`freestyle::NONE_TEXT`], "None"; a text missing, its code.
#[must_use]
pub fn none_name(text: &Text<'_>) -> String {
    text.gmtx(freestyle::NONE_TEXT)
        .map_or_else(|| "NONE".to_owned(), |t| t.trim().to_owned())
}

/// A quest file's map, parsed; `name` labels its error.
fn map_of<'a>(bytes: &'a [u8], name: &dyn std::fmt::Display) -> Result<rgs::Map<'a>, String> {
    rgs::File::parse(bytes)
        .and_then(|f| f.map())
        .map_err(|e| format!("{name}: {e}"))
}

/// A constants file's chart, parsed; `name` labels its error.
fn chart_of<'a>(bytes: &'a [u8], name: &dyn std::fmt::Display) -> Result<rgs::Chart<'a>, String> {
    rgs::File::parse(bytes)
        .and_then(|f| f.chart())
        .map_err(|e| format!("{name}: {e}"))
}

/// The freestyle quest's bytes.
fn quest(hd: &Path) -> Result<Vec<u8>, String> {
    read(&find(hd, QUEST)?)
}

/// The expansion's chart: its path and bytes.
fn chart(hd: &Path) -> Result<(PathBuf, Vec<u8>), String> {
    let path = expansion(hd)?.chart;
    let bytes = read(&path)?;

    Ok((path, bytes))
}

/// `name` under the install, in any case.
fn find(hd: &Path, name: &str) -> Result<PathBuf, String> {
    regent_core::content::on_disk(&hd.join(name))
        .ok_or_else(|| format!("{}: not found", hd.join(name).display()))
}
