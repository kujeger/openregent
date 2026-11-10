//! Where the game files are. The HD install, the first of:
//!
//! ```text
//!  1. MAJESTY_HD          the process environment
//!  2. [paths] majesty_hd  in config.toml (config::hd_at)
//!  3. $XDG_DATA_HOME/openregent/data   always, beside the mods folder
//!  4. the current folder  when it holds an install (MARKS)
//!  5. the program's folder, for a desktop that starts it elsewhere
//! ```
//!
//! A value set in 1 or 2 that is no install is an error, not passed
//! over: the install that runs is the one asked for, never a surprise.
//! 3 to 5 count only when they hold an install; 3 holding other files
//! is warned of, as a copy gone wrong, and passed over.
//!
//! Mods ([`mod_folders`]) are looked for in each of, in order:
//!
//! ```text
//!  0. the built-in mods, unpacked    always (crate::files::builtin)
//!  1. $XDG_DATA_HOME/openregent/mods   always, as HD's user Mods folder
//!  2. Mods in the current folder     when it is a folder
//!  3. [paths] mods                   in config.toml, each (config::mods_at)
//!  4. MAJESTY_WORKSHOP, else Steam's default Workshop folder for app 73230
//! ```
//!
//! The config adds folders and replaces none: the own folder and the
//! current folder's are where a mod is put by hand, the Workshop's where
//! Steam puts one. A GUID in
//! two folders: the first folder's (`content::mods::scan`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, PoisonError, RwLock};

use bevy::log::{info, warn};
use bevy::prelude::Resource;
use regent_core::content::dataset::{BASE_DEFINITIONS, EXPANSION_DEFINITIONS};
use regent_core::content::mods::{self, Library};
use regent_core::data::qdd;

use crate::files::compiled::{self, Builds};
use crate::files::config;

/// openregent's directory under each XDG base directory: settings,
/// profiles, log, recording and art cache.
pub const OWN: &str = "openregent";

/// The HD install's environment key, and the SDK's folder in it.
pub use regent_core::content::env::{HD, SDK_IN_HD};

/// Test paths may read `.env`; runtime paths do not.
#[cfg(test)]
pub use regent_corpus::env::path;

/// The tests' stand-in for the cache directory: a folder under the
/// system's temporary folder, kept across runs.
#[must_use]
pub fn test_cache() -> PathBuf {
    std::env::temp_dir().join("openregent-tests")
}

/// The freestyle quest, which holds the lands, under the install.
pub const QUEST: &str = "QuestsMX/mx_Freestyle.q";

/// Subscribed Steam Workshop items: the tests' corpus, else a user's.
pub const WORKSHOP: &str = "MAJESTY_WORKSHOP";

/// openregent's own mod folder under its XDG data directory.
const MODS: &str = "mods";

/// The mod folder under the current folder, as in an HD install.
const HERE_MODS: &str = "Mods";

/// openregent's own place for the HD install under its XDG data directory.
const DATA: &str = "data";

/// Steam's Workshop folder for Majesty HD, app 73230, under Steam's
/// folder ([`steam`]).
const STEAM_WORKSHOP: &str = "steamapps/workshop/content/73230";

/// Steam's folder, under the data directory (Linux, macOS) or the
/// 32-bit programs folder (Windows).
const STEAM: &str = "Steam";

/// The 32-bit programs folder's environment key, where Windows installs
/// Steam, and its usual value.
#[cfg(windows)]
const PROGRAMS_X86: (&str, &str) = ("ProgramFiles(x86)", r"C:\Program Files (x86)");

/// Steam's default folder ([`STEAM`]). Another Steam library is not
/// looked in.
#[cfg(not(windows))]
fn steam() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(STEAM))
}

/// Steam's default folder ([`STEAM`]). Another Steam library is not
/// looked in.
#[cfg(windows)]
#[expect(clippy::unnecessary_wraps, reason = "the other platforms' signature")]
fn steam() -> Option<PathBuf> {
    let (key, usual) = PROGRAMS_X86;
    let programs = std::env::var_os(key).map_or_else(|| PathBuf::from(usual), PathBuf::from);

    Some(programs.join(STEAM))
}

/// A custom game's goals, `FRG0` to `FRG3`, in the HD install.
const QDD: &str = "Quests/Maj.qdd";

/// Files that mark a folder as an HD install: openregent reads each.
const MARKS: [&str; 3] = [BASE_DEFINITIONS, EXPANSION_DEFINITIONS, QDD];

/// Where the HD install was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Env,
    Config,
    Xdg,
    Cwd,
    Exe,
}

/// The places [`find`] looks, in its order.
#[derive(Clone, Debug, Default)]
pub struct Places {
    /// [`HD`]'s value.
    pub env: Option<PathBuf>,
    /// The settings file.
    pub config: Option<PathBuf>,
    /// `$XDG_DATA_HOME/openregent/data`.
    pub xdg: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    /// The program's folder.
    pub exe: Option<PathBuf>,
}

impl Places {
    /// This process's.
    fn here() -> Places {
        let exe = std::env::current_exe().ok();

        Places {
            env: process_path(HD),
            config: config::path(),
            xdg: dirs::data_dir().map(|d| d.join(OWN).join(DATA)),
            cwd: std::env::current_dir().ok(),
            exe: exe.and_then(|p| p.parent().map(Path::to_path_buf)),
        }
    }
}

/// A process variable's path, with `~/` expanded.
fn process_path(key: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(key)?);

    match path.strip_prefix("~") {
        Ok(rest) => dirs::home_dir().map(|home| home.join(rest)).or(Some(path)),
        Err(_) => Some(path),
    }
}

/// The HD install, and where it was found (the module's order).
///
/// # Errors
///
/// A value set in [`HD`] or the settings that is no install; a settings
/// file that will not read; no install anywhere.
pub fn find(places: &Places) -> Result<(PathBuf, Source), String> {
    let set = match (&places.env, &places.config) {
        (Some(hd), _) => Some((hd.clone(), Source::Env)),
        (None, Some(file)) => config::hd_at(file)?.map(|hd| (hd, Source::Config)),
        (None, None) => None,
    };

    match set {
        Some((hd, source)) => match missing(&hd).as_slice() {
            [] => Ok((hd, source)),
            lacks => Err(format!(
                "{} = {}: no HD install, missing {}",
                named(source, places),
                hd.display(),
                lacks.join(", ")
            )),
        },
        None => [
            (&places.xdg, Source::Xdg),
            (&places.cwd, Source::Cwd),
            (&places.exe, Source::Exe),
        ]
        .into_iter()
        .filter_map(|(dir, source)| dir.clone().map(|dir| (dir, source)))
        .find(|(dir, source)| holds(dir, *source, places))
        .ok_or_else(|| none_found(places)),
    }
}

/// Whether `dir` is an install; the XDG folder holding files but no
/// install warned of.
fn holds(dir: &Path, source: Source, places: &Places) -> bool {
    let lacks = missing(dir);

    if source == Source::Xdg && !lacks.is_empty() && !is_empty(dir) {
        warn!(
            "{} = {}: no HD install, missing {}; passed over",
            named(source, places),
            dir.display(),
            lacks.join(", ")
        );
    }

    lacks.is_empty()
}

/// Whether `dir` is absent or holds nothing.
fn is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir).map_or(true, |mut entries| entries.next().is_none())
}

/// [`MARKS`] absent from `dir`.
fn missing(dir: &Path) -> Vec<&'static str> {
    MARKS
        .into_iter()
        .filter(|mark| !dir.join(mark).is_file())
        .collect()
}

/// A source as an error names it.
fn named(source: Source, places: &Places) -> String {
    let file = places
        .config
        .as_ref()
        .map_or_else(|| "config.toml".to_owned(), |f| f.display().to_string());

    match source {
        Source::Env => format!("{HD} (environment)"),
        Source::Config => format!("[{}] {} in {file}", config::PATHS, config::HD_KEY),
        Source::Xdg => "openregent's data folder".to_owned(),
        Source::Cwd => "the current folder".to_owned(),
        Source::Exe => "the program's folder".to_owned(),
    }
}

/// The error with no install anywhere: each place named.
fn none_found(places: &Places) -> String {
    let shown = |dir: &Option<PathBuf>| {
        dir.as_ref()
            .map_or_else(|| "unknown".to_owned(), |d| d.display().to_string())
    };

    format!(
        "no Majesty HD install found: set {HD}, or {}, or put the install in {}, \
         or start openregent in the install's folder (looked in {} and {}; an \
         install holds {})",
        named(Source::Config, places),
        shown(&places.xdg),
        shown(&places.cwd),
        shown(&places.exe),
        MARKS.join(", ")
    )
}

/// [`find`]'s error `e`, with the settings file written when absent
/// ([`config::seed`]), so the user has a `[paths]` example to fill in.
fn seeded(places: &Places, e: String) -> String {
    match places.config.as_deref().filter(|file| !file.exists()) {
        Some(file) => match config::seed(file) {
            Ok(()) => format!(
                "{e}; wrote {}: set [{}] {} there",
                file.display(),
                config::PATHS,
                config::HD_KEY
            ),
            Err(w) => format!("{e}; settings not written: {w}"),
        },
        None => e,
    }
}

/// The install the game runs on, found once in `main`: every path to the
/// game's files comes from it, none from the environment later.
#[derive(Resource, Debug)]
pub struct Install {
    pub hd: PathBuf,
    pub sdk: PathBuf,
    /// The folders mods are found in, in order ([`mod_folders`]).
    pub mod_folders: Vec<PathBuf>,
    /// [`QDD`]'s blocks, read at the first ask.
    goals: OnceLock<Vec<qdd::Quest>>,
    /// The mods in [`Install::mod_folders`], scanned at the first ask
    /// and shared by clones: the menus' list and a game's agree. A
    /// rescan replaces it for every clone.
    library: Arc<RwLock<Option<Arc<Scanned>>>>,
}

/// The mods found, and those whose GPL source compiled or not.
#[derive(Debug)]
struct Scanned {
    library: Library,
    builds: Builds,
}

/// [`Install::mods`]: a scan's library, kept as found while held.
pub struct Mods(Arc<Scanned>);

impl std::ops::Deref for Mods {
    type Target = Library;

    fn deref(&self) -> &Library {
        &self.0.library
    }
}

impl Install {
    /// The HD install [`find`] finds, said at info, and its SDK.
    ///
    /// # Errors
    ///
    /// As [`find`].
    pub fn find() -> Result<Install, String> {
        let places = Places::here();
        let (hd, source) = find(&places).map_err(|e| seeded(&places, e))?;
        let sdk = hd.join(SDK_IN_HD);
        let own = dirs::data_dir().map(|d| d.join(OWN).join(MODS));
        let here = places
            .cwd
            .as_ref()
            .map(|d| d.join(HERE_MODS))
            .filter(|d| d.is_dir());
        let configured = places
            .config
            .as_deref()
            .map(config::mods_at)
            .unwrap_or_default();
        let workshop = process_path(WORKSHOP).or_else(|| {
            steam()
                .map(|d| d.join(STEAM_WORKSHOP))
                .filter(|d| d.is_dir())
        });

        info!(
            "HD install {}, from {}",
            hd.display(),
            named(source, &places)
        );

        println!("Game data: {} ({})", hd.display(), named(source, &places));

        Ok(Install {
            hd,
            sdk,
            mod_folders: mod_folders(
                crate::files::builtin::folder(),
                own,
                here,
                configured,
                workshop,
            ),
            goals: OnceLock::new(),
            library: Arc::default(),
        })
    }

    /// From [`HD`], the SDK in the install, mods built in and in
    /// [`WORKSHOP`]: the tests'.
    ///
    /// # Errors
    ///
    /// With no [`HD`].
    #[cfg(test)]
    pub fn from_env() -> Result<Install, String> {
        let hd = path(HD).map_err(|e| e.to_string())?;
        let sdk = hd.join(SDK_IN_HD);

        Ok(Install {
            hd,
            sdk,
            mod_folders: crate::files::builtin::folder()
                .into_iter()
                .chain(path(WORKSHOP))
                .collect(),
            goals: OnceLock::new(),
            library: Arc::default(),
        })
    }

    /// The same install, mods found in `folders`: a test's.
    #[cfg(test)]
    #[must_use]
    pub fn with_mod_folders(self, folders: Vec<PathBuf>) -> Install {
        Install {
            mod_folders: folders,
            library: Arc::default(),
            ..self
        }
    }

    /// The mods and their quests in [`Install::mod_folders`], said at
    /// info once found; a GUID found twice warned of. A mod's GPL
    /// source compiled where its `.bcd` is missing (`compiled`).
    pub fn mods(&self) -> Mods {
        Mods(self.scanned())
    }

    /// The mods [`Install::mods`] compiled from source, or refused.
    pub fn builds(&self) -> Builds {
        self.scanned().builds.clone()
    }

    /// The mod folders scanned again for every clone, changed sources
    /// recompiled: a mod edited plays without a restart.
    pub fn rescan(&self) {
        let fresh = Arc::new(self.scan());

        *self.library.write().unwrap_or_else(PoisonError::into_inner) = Some(fresh);
    }

    /// [`Install::mods`] and [`Install::builds`], found at the first ask.
    fn scanned(&self) -> Arc<Scanned> {
        let found = self
            .library
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();

        found.unwrap_or_else(|| {
            let mut slot = self.library.write().unwrap_or_else(PoisonError::into_inner);

            Arc::clone(slot.get_or_insert_with(|| Arc::new(self.scan())))
        })
    }

    /// The mod folders scanned, said at info; sources compiled.
    fn scan(&self) -> Scanned {
        let mut library = mods::scan(&self.mod_folders);
        let folders: Vec<String> = self
            .mod_folders
            .iter()
            .map(|f| f.display().to_string())
            .collect();

        info!(
            "mods: {} and {} quests in {}",
            library.mods.len(),
            library.quests.len(),
            folders.join(", ")
        );

        for skipped in &library.skipped {
            warn!("mod skipped: {skipped}");
        }

        let builds = compiled::folder()
            .map(|root| compiled::resolve(&mut library, &root))
            .unwrap_or_default();

        Scanned { library, builds }
    }

    /// A custom game's goals ([`QDD`]); none where it will not read.
    pub fn goals(&self) -> &[qdd::Quest] {
        self.goals.get_or_init(|| {
            std::fs::read(self.hd.join(QDD))
                .ok()
                .and_then(|bytes| {
                    qdd::parse(&String::from_utf8_lossy(&bytes))
                        .map_err(|e| warn!("{QDD}: {e}"))
                        .ok()
                })
                .unwrap_or_default()
        })
    }
}

impl Clone for Install {
    /// The paths and the mods found; the goals read again at the first
    /// ask.
    fn clone(&self) -> Install {
        Install {
            hd: self.hd.clone(),
            sdk: self.sdk.clone(),
            mod_folders: self.mod_folders.clone(),
            goals: OnceLock::new(),
            library: Arc::clone(&self.library),
        }
    }
}

/// The folders mods are found in: `builtin` (the unpacked built-in
/// mods), `own` (openregent's), `here` (the current folder's), then
/// each of `configured`, then `workshop`; a folder named twice kept at
/// its first place.
pub fn mod_folders(
    builtin: Option<PathBuf>,
    own: Option<PathBuf>,
    here: Option<PathBuf>,
    configured: Vec<PathBuf>,
    workshop: Option<PathBuf>,
) -> Vec<PathBuf> {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());

    builtin
        .into_iter()
        .chain(own)
        .chain(here)
        .chain(configured)
        .chain(workshop)
        .fold(Vec::new(), |mut out: Vec<PathBuf>, folder| {
            if !out.iter().any(|f| real(f) == real(&folder)) {
                out.push(folder);
            }

            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("install-{name}"))
    }

    /// A manifest of one mod, `guid`, in `dir`.
    fn manifest(dir: &Path, guid: &str) {
        let text = format!("<Majesty><Mod id=\"{guid}\"><Name>M</Name></Mod></Majesty>");

        std::fs::write(dir.join(format!("{guid}.mmxml")), text).expect("written");
    }

    /// A mod put in a folder shows only after a rescan, in every clone.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_rescan_finds_a_new_mod_in_every_clone() {
        let dir = scratch("rescan");
        let install = Install::from_env()
            .expect("MAJESTY_HD")
            .with_mod_folders(vec![dir.to_path_buf()]);
        let clone = install.clone();

        manifest(&dir, "{5EC0DE00-0000-4000-8000-000000000002}");
        assert_eq!(install.mods().mods.len(), 1);

        manifest(&dir, "{5EC0DE00-0000-4000-8000-000000000003}");
        assert_eq!(install.mods().mods.len(), 1, "scanned once");

        clone.rescan();
        assert_eq!(install.mods().mods.len(), 2);
    }

    /// A folder holding [`MARKS`], empty files: enough for [`find`].
    fn fake_install(dir: &Path) -> PathBuf {
        for mark in MARKS {
            let file = dir.join(mark);

            std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
            std::fs::write(file, "").expect("written");
        }

        dir.to_path_buf()
    }

    /// A settings file in `dir` naming `hd`.
    fn settings(dir: &Path, hd: &Path) -> PathBuf {
        let file = dir.join("config.toml");
        let text = format!("[ui]\nbars = \"never\"\n\n[paths]\nmajesty_hd = {hd:?}\n");

        std::fs::write(&file, text).expect("written");
        file
    }

    /// An install in each place, the settings naming theirs.
    fn every_place(name: &str) -> (crate::testdata::Scratch, Places) {
        let dir = scratch(name);
        let config = fake_install(&dir.join("config"));
        let places = Places {
            env: Some(fake_install(&dir.join("env"))),
            config: Some(settings(&dir, &config)),
            xdg: Some(fake_install(&dir.join("xdg"))),
            cwd: Some(fake_install(&dir.join("cwd"))),
            exe: Some(fake_install(&dir.join("exe"))),
        };

        (dir, places)
    }

    fn source(places: &Places) -> Result<Source, String> {
        find(places).map(|(_, source)| source)
    }

    #[test]
    fn each_place_wins_in_order() {
        let (dir, mut places) = every_place("order");

        assert_eq!(find(&places), Ok((dir.join("env"), Source::Env)));

        places.env = None;
        assert_eq!(find(&places), Ok((dir.join("config"), Source::Config)));

        places.config = None;
        assert_eq!(find(&places), Ok((dir.join("xdg"), Source::Xdg)));

        places.xdg = None;
        assert_eq!(find(&places), Ok((dir.join("cwd"), Source::Cwd)));

        places.cwd = Some(dir.to_path_buf());
        assert_eq!(find(&places), Ok((dir.join("exe"), Source::Exe)));
    }

    /// The XDG folder missing, empty or incomplete: passed over, no
    /// error.
    #[test]
    fn an_xdg_folder_without_an_install_passes_on() {
        let (dir, mut places) = every_place("xdg_none");

        places.env = None;
        places.config = None;

        places.xdg = Some(dir.join("absent"));
        assert_eq!(source(&places), Ok(Source::Cwd));

        let empty = scratch("xdg_none_empty");

        places.xdg = Some(empty.to_path_buf());
        assert_eq!(source(&places), Ok(Source::Cwd));

        let partial = scratch("xdg_none_partial");
        let file = partial.join(BASE_DEFINITIONS);

        std::fs::create_dir_all(file.parent().expect("a folder")).expect("made");
        std::fs::write(file, "").expect("written");
        places.xdg = Some(partial.to_path_buf());
        assert_eq!(source(&places), Ok(Source::Cwd));
    }

    #[test]
    fn the_settings_are_not_read_when_the_environment_is_set() {
        let (_dir, mut places) = every_place("env_first");
        let file = places.config.clone().expect("a file");

        std::fs::write(&file, "not toml [").expect("written");
        assert_eq!(source(&places), Ok(Source::Env));

        places.env = None;
        assert!(source(&places).is_err_and(|e| e.contains("config.toml")));
    }

    #[test]
    fn no_install_writes_a_settings_file_to_fill_in() {
        let dir = scratch("seeded");
        let file = dir.join("openregent/config.toml");
        let places = Places {
            config: Some(file.clone()),
            ..Places::default()
        };

        let e = find(&places).expect_err("no install");
        let said = seeded(&places, e.clone());
        let text = std::fs::read_to_string(&file).expect("written");

        assert!(said.contains(&file.display().to_string()), "{said}");
        assert!(text.contains("# majesty_hd = "), "{text}");
        assert_eq!(config::hd_at(&file), Ok(None));

        // An existing file is left alone.
        std::fs::write(&file, "[ui]\n").expect("written");
        assert_eq!(seeded(&places, e.clone()), e);
        assert_eq!(
            std::fs::read_to_string(&file).ok().as_deref(),
            Some("[ui]\n")
        );
    }

    #[test]
    fn a_settings_file_without_the_key_passes_on() {
        let (_dir, mut places) = every_place("no_key");
        let file = places.config.clone().expect("a file");

        places.env = None;
        std::fs::write(&file, "[ui]\nbars = \"never\"\n").expect("written");
        assert_eq!(source(&places), Ok(Source::Xdg));

        places.config = Some(file.with_file_name("absent.toml"));
        assert_eq!(source(&places), Ok(Source::Xdg));
    }

    /// A set value that is no install stops the search, an install in
    /// the current folder or not: the error names the source, the path
    /// and what is missing.
    #[test]
    fn a_set_value_that_is_no_install_is_an_error() {
        let (_dir, mut places) = every_place("invalid");
        let empty = scratch("invalid_empty");

        places.env = Some(empty.to_path_buf());
        let error = find(&places).expect_err("refused");

        assert!(error.contains(HD), "{error}");
        assert!(error.contains(&empty.display().to_string()), "{error}");
        assert!(error.contains(QDD), "{error}");

        places.env = None;
        places.config = Some(settings(&empty, &empty));
        let error = find(&places).expect_err("refused");

        assert!(error.contains("[paths] majesty_hd"), "{error}");
        assert!(error.contains(BASE_DEFINITIONS), "{error}");
    }

    #[test]
    fn a_settings_value_no_string_is_an_error() {
        let dir = scratch("not_a_path");
        let file = dir.join("config.toml");

        std::fs::write(&file, "[paths]\nmajesty_hd = 1\n").expect("written");
        assert!(config::hd_at(&file).is_err());

        std::fs::write(&file, "paths = 1\n").expect("written");
        assert!(config::hd_at(&file).is_err());
    }

    #[test]
    fn a_relative_settings_path_is_from_the_files_folder() {
        let dir = scratch("relative");
        let hd = fake_install(&dir.join("Majesty HD"));
        let file = settings(&dir, Path::new("Majesty HD"));

        assert_eq!(config::hd_at(&file), Ok(Some(hd)));
    }

    /// The built-in mods first, then openregent's own folder and the
    /// current folder's, whatever the config says; the config's add to
    /// them, then the Workshop's; a folder named twice, however spelled,
    /// once.
    #[test]
    fn the_mod_folders_are_own_then_here_then_configured_then_workshop() {
        let dir = scratch("mod_folders");
        let (builtin, own, here, a, b, steam) = (
            dir.join("builtin"),
            dir.join("own"),
            dir.join("here"),
            dir.join("a"),
            dir.join("b"),
            dir.join("steam"),
        );

        for folder in [&builtin, &own, &here, &a, &b, &steam] {
            std::fs::create_dir_all(folder).expect("made");
        }

        assert_eq!(
            mod_folders(
                Some(builtin.clone()),
                Some(own.clone()),
                Some(here.clone()),
                vec![a.clone(), b.clone()],
                Some(steam.clone())
            ),
            [
                builtin,
                own.clone(),
                here.clone(),
                a.clone(),
                b.clone(),
                steam.clone()
            ]
        );
        assert_eq!(
            mod_folders(
                None,
                Some(own.clone()),
                Some(dir.join("a/../own")),
                vec![steam.clone(), dir.join("a/../own")],
                Some(steam.clone())
            ),
            [own.clone(), steam.clone()]
        );
        assert_eq!(
            mod_folders(None, None, None, Vec::new(), None),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn no_install_anywhere_names_every_place() {
        let dir = scratch("none");
        let places = Places {
            env: None,
            config: Some(dir.join("config.toml")),
            xdg: Some(dir.join("xdg")),
            cwd: Some(dir.to_path_buf()),
            exe: None,
        };
        let error = find(&places).expect_err("none");

        assert!(
            error.contains(&dir.join("xdg").display().to_string()),
            "{error}"
        );
        assert!(error.contains(HD), "{error}");
        assert!(error.contains("[paths] majesty_hd"), "{error}");
        assert!(error.contains(&dir.display().to_string()), "{error}");
    }
}
