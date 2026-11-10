//! Locating the corpora the tests read.
//!
//! Two environment variables, both directories. Each may instead
//! be set in `.env` ([`env`]):
//!
//! ```text
//! MAJESTY_HD=/mnt/games/SteamLibrary/steamapps/common/Majesty HD
//! MAJESTY_WORKSHOP=local/game/workshop
//! ```
//!
//! Values may be `~/…`, absolute, or relative to the `.env` file's folder.
//! The process environment overrides `.env`.
//!
//! Every corpus is required; unset or wrong-looking values panic.

// Panicking on a missing corpus is the point of every function here.
#![allow(clippy::missing_panics_doc)]

pub mod env;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::content::env::SDK_IN_HD;
use regent_core::data::catalogue::{Catalogue, Library};

/// The Windows HD edition. Its `SDK/` subtree is [`sdk`].
pub fn hd() -> PathBuf {
    dir("MAJESTY_HD", "Data")
}

/// The Majesty HD modding SDK, [`hd`]'s `SDK/`.
pub fn sdk() -> PathBuf {
    let sdk = hd().join(SDK_IN_HD);

    assert!(
        sdk.join("OriginalQuests").is_dir(),
        "{} has no OriginalQuests/, so MAJESTY_HD holds no SDK",
        sdk.display()
    );

    sdk
}

/// Subscribed Steam Workshop mods.
pub fn workshop() -> PathBuf {
    dir("MAJESTY_WORKSHOP", ".")
}

/// Every file under `root` whose extension is one of `extensions`, sorted.
///
/// Recursive and case-insensitive: three shipped `.gpl` files are spelled
/// `.GPL`.
///
/// Unreadable directories are skipped; the caller fails on the empty result.
pub fn walk(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    fn gather(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                gather(&path, extensions, out);
            } else if let Some(found) = path.extension().and_then(|e| e.to_str())
                && extensions
                    .iter()
                    .any(|want| found.eq_ignore_ascii_case(want))
            {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    gather(root, extensions, &mut out);
    out.sort();
    out
}

/// Every `.bcd` under `roots`, sorted, each once.
pub fn bcds(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = roots.iter().flat_map(|root| walk(root, &["bcd"])).collect();

    out.sort();
    out.dedup();
    out
}

/// Every non-empty `.cam` under `root`, sorted.
///
/// Zero-byte files are dropped.
///
/// Walking an install root also picks up HD's `SDK/` archives, on purpose.
///
/// A copy is dropped too ([`is_copy`]), so counts hold on GOG's and
/// Steam's 1.5.2.28 alike.
pub fn archives(root: &Path) -> Vec<PathBuf> {
    let mut found = walk(root, &["cam"]);
    found.retain(|path| path.metadata().is_ok_and(|m| m.len() > 0) && !is_copy(path));
    found
}

/// The base game's data directory.
const DATA: &str = "Data";

/// The expansion's data directory, beside [`DATA`].
const DATA_MX: &str = "DataMX";

/// Whether `path` is in `Data/` and byte for byte the file of its name in
/// `DataMX/` beside it: GOG ships `DataMX/mx_textdata.cam` again in
/// `Data/`, Steam once.
fn is_copy(path: &Path) -> bool {
    let in_data = path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|d| d == DATA);
    let twin = path
        .parent()
        .and_then(Path::parent)
        .zip(path.file_name())
        .map(|(install, name)| install.join(DATA_MX).join(name));

    match twin {
        Some(twin) if in_data && twin.is_file() => {
            std::fs::read(path).ok() == std::fs::read(&twin).ok()
        }
        _ => false,
    }
}

/// A file's bytes. Panics with the path if unreadable.
pub fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `library`'s catalogue.
pub fn catalogue_of(
    library: &regent_core::data::catalogue::Library,
) -> regent_core::data::catalogue::Catalogue<'_> {
    library.catalogue().expect("a catalogue")
}

/// One corpus root.
///
/// `probe` must exist under the root. Panics otherwise, or if unset.
pub fn dir(var: &str, probe: &str) -> PathBuf {
    let dir = env::lookup(var)
        .unwrap_or_else(|| panic!("{var} is not set, in the environment or in .env"));

    assert!(
        dir.join(probe).is_dir(),
        "{var}={} has no {probe}/, so it is not a Majesty corpus",
        dir.display()
    );

    dir
}

/// The SDK images in HD's load order.
pub fn sdk_images() -> Vec<PathBuf> {
    regent_core::content::dataset::Install::new(
        &hd(),
        regent_core::content::dataset::Bytecode::Sdk(&sdk()),
    )
    .files(regent_core::content::dataset::Datasets::Expansion)
    .expect("SDK images")
    .programs
}

/// [`sdk_images`]' bytes.
pub fn sdk_bytes() -> Vec<Vec<u8>> {
    sdk_images().iter().map(|p| read(p)).collect()
}

/// The SDK program.
pub fn sdk_program() -> regent_core::gpl::load::Program {
    regent_core::gpl::load::Program::load(&sdk_bytes()).expect("SDK loads")
}

/// [`sdk_program`], leaked for tests that hand out `'static` borrows.
pub fn sdk_leaked() -> &'static regent_core::gpl::load::Program {
    Box::leak(Box::new(sdk_program()))
}

/// The SDK program with `source` compiled in as one more image, `name`.
pub fn sdk_with(source: &str, name: &str) -> regent_core::gpl::load::Program {
    let mut images = sdk_bytes();

    images.push(regent_core::gpl::emit::compile_source(source, name).expect("compiles"));
    regent_core::gpl::load::Program::load(&images).expect("loads")
}

/// HD's archives, `Data` then `DataMX`, read once a process: every test
/// in it shares the one copy, as tests run at once.
pub fn library() -> &'static Library {
    static LIBRARY: OnceLock<Library> = OnceLock::new();

    LIBRARY.get_or_init(|| {
        let root = hd();
        let mut paths = archives(&root.join("Data"));

        paths.extend(archives(&root.join("DataMX")));

        Library::from_files(&paths).expect("the archives")
    })
}

/// The archives `paths`, in that order, read once a process for each list.
/// Kept until the process ends: for lists tests repeat, not one-offs.
pub fn library_of(paths: &[PathBuf]) -> &'static Library {
    static READ: Mutex<BTreeMap<Vec<PathBuf>, &'static Library>> = Mutex::new(BTreeMap::new());

    let mut read = READ.lock().expect("the archive cache");

    read.entry(paths.to_vec())
        .or_insert_with(|| Box::leak(Box::new(Library::from_files(paths).expect("the archives"))))
}

/// The archives HD's definition file `definition` lists under `dir`, as
/// the engine resolves them (`MajestyDataPath` is `Data`), read once a
/// process for each pair. `definition` and `dir` are under [`hd`].
pub fn defined(definition: &str, dir: &str) -> &'static Library {
    static READ: Mutex<BTreeMap<(String, String), &'static Library>> = Mutex::new(BTreeMap::new());

    let mut read = READ.lock().expect("the definition cache");

    read.entry((definition.to_owned(), dir.to_owned()))
        .or_insert_with(|| {
            let game = hd();
            let src = std::fs::read_to_string(game.join(definition)).expect("a definition");
            let set = regent_core::content::dataset::parse(&src);
            let data = game.join("Data");

            Box::leak(Box::new(
                set.library(&game.join(dir), &[("MajestyDataPath", data.as_path())]),
            ))
        })
}

/// One of HD's datasets, its archives in load order (a later record wins),
/// read once a process ([`library_of`]).
pub fn dataset(sets: Datasets) -> &'static Library {
    let paths = Install::new(&hd(), Bytecode::Install)
        .files(sets)
        .expect("the dataset's files")
        .archives;

    library_of(&paths)
}

/// HD's expansion dataset ([`dataset`]), indexed once a process.
pub fn expansion() -> &'static Catalogue<'static> {
    static CATALOGUE: OnceLock<Catalogue<'static>> = OnceLock::new();

    CATALOGUE.get_or_init(|| {
        dataset(Datasets::Expansion)
            .catalogue()
            .expect("a catalogue")
    })
}
