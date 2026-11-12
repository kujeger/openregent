//! A mod's files, hashed: what a LAN lobby compares, so every peer plays
//! the same mod.
//!
//! ```text
//!  manifest bytes            its Load and Unload lists, a TOML mod's patches
//!  each file Load lists      CAM, GPL (a compiled image where built from
//!                            source), constants, descriptions, strings
//!                            └─ one under a $(Root) is the install's: passed over
//!  ──> FNV-1a, 16 hex digits
//! ```
//!
//! Bytes alone, no paths: a compiled image's cache path differs between
//! machines. A listed file not on disk folds a mark of its own. Each
//! file's hash is kept by path, length and modification time, so a
//! lobby joined again reads only what changed.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::SystemTime;

use regent_core::content::dataset::Listed;
use regent_core::content::mods::{Library, Mod};
use regent_core::data::fnv;
use regent_core::data::guid::Guid;

/// What a listed file not on disk folds in place of its bytes' hash.
const ABSENT: u64 = 0;

/// A mod this machine holds: its title and its files' hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub title: String,
    pub hash: String,
}

/// Every mod a library holds, by GUID.
pub type Holdings = BTreeMap<Guid, Held>;

/// A file's length and modification time: unchanged, its hash is kept.
type Stamp = (u64, SystemTime);

/// Each file's hash, by path, as last read.
static FILES: LazyLock<Mutex<HashMap<PathBuf, (Stamp, u64)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Every mod in `library`, hashed.
pub fn holdings(library: &Library) -> Holdings {
    library
        .mods
        .iter()
        .map(|m| {
            let held = Held {
                title: m.title(),
                hash: hash(m),
            };

            (m.guid, held)
        })
        .collect()
}

/// `m`'s manifest and the files it loads, hashed.
pub fn hash(m: &Mod) -> String {
    let lists = [
        &m.load.archives,
        &m.load.programs,
        &m.load.constants,
        &m.load.descriptions,
        &m.load.strings,
    ];
    let files = lists
        .into_iter()
        .flatten()
        .filter_map(|listed| own(&m.dir, listed));

    let h = std::iter::once(m.manifest.clone())
        .chain(files)
        .map(|path| file(&path))
        .fold(fnv::START, |h, file| fnv::fold(h, file.to_le_bytes()));

    format!("{h:016x}")
}

/// `listed` on disk, if the mod's own: one under a `$(Root)` is the
/// install's. A compiled image's path is whole.
fn own(dir: &Path, listed: &Listed) -> Option<PathBuf> {
    match listed {
        (Some(_), _) => None,
        (None, path) if Path::new(path).is_absolute() => Some(PathBuf::from(path)),
        (None, path) => Some(super::compiled::under(dir, path)),
    }
}

/// The file at `path`, hashed; [`ABSENT`] where none is, in any case.
fn file(path: &Path) -> u64 {
    let found = regent_core::content::on_disk(path);
    let stamp = found
        .as_deref()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|meta| Some((meta.len(), meta.modified().ok()?)));

    match (found, stamp) {
        (Some(found), Some(stamp)) => kept(&found, stamp).unwrap_or_else(|| {
            let h = std::fs::read(&found).map_or(ABSENT, |bytes| fnv::fold(fnv::START, bytes));

            FILES
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(found, (stamp, h));
            h
        }),
        _ => ABSENT,
    }
}

/// The hash kept for `path`, if read at `stamp`.
fn kept(path: &Path, stamp: Stamp) -> Option<u64> {
    FILES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(path)
        .filter(|(at, _)| *at == stamp)
        .map(|(_, h)| *h)
}

#[cfg(test)]
mod tests {
    use regent_core::content::mods;

    use super::*;

    const GUID: &str = "{0B5C6A43-7D7E-4E8A-9C1F-2A6B3C4D5E6F}";

    /// A mod folder in `dir`: a manifest loading a CAM and a GPL image.
    fn folder(dir: &Path, cam: &[u8]) -> Library {
        let manifest = format!(
            "<Majesty><Mod id=\"{GUID}\"><Name>Hashed</Name><DataConfiguration>\
             <Dataset base=\"Any\"><Load><CAM>Data/a.cam</CAM><GPL>Data/a.bcd</GPL>\
             </Load></Dataset></DataConfiguration></Mod></Majesty>"
        );

        std::fs::create_dir_all(dir.join("Data")).expect("made");
        std::fs::write(dir.join("Hashed.mmxml"), manifest).expect("written");
        std::fs::write(dir.join("Data/a.cam"), cam).expect("written");
        std::fs::write(dir.join("Data/a.bcd"), b"image").expect("written");

        mods::scan(&[dir.to_path_buf()])
    }

    fn hashed(library: &Library) -> String {
        let guid = Guid::parse(GUID).expect("a GUID");

        holdings(library).remove(&guid).expect("held").hash
    }

    /// Two folders of the same files hash alike, wherever they are.
    #[test]
    fn the_same_files_hash_alike_in_any_folder() {
        let (one, two) = (
            crate::testdata::scratch("mod-hash-one"),
            crate::testdata::scratch("mod-hash-two"),
        );

        assert_eq!(
            hashed(&folder(one.path(), b"art")),
            hashed(&folder(two.path(), b"art"))
        );
    }

    /// A byte changed in a listed file, of the same length, changes it.
    #[test]
    fn a_changed_byte_changes_the_hash() {
        let (one, two) = (
            crate::testdata::scratch("mod-hash-same"),
            crate::testdata::scratch("mod-hash-edited"),
        );

        assert_ne!(
            hashed(&folder(one.path(), b"art")),
            hashed(&folder(two.path(), b"arT"))
        );
    }

    /// A listed file gone changes it.
    #[test]
    fn a_missing_file_changes_the_hash() {
        let dir = crate::testdata::scratch("mod-hash-missing");
        let library = folder(dir.path(), b"art");
        let whole = hashed(&library);

        std::fs::remove_file(dir.path().join("Data/a.bcd")).expect("removed");

        assert_ne!(hashed(&library), whole);
    }

    /// The test game's mod (`regent_testgame::TOUGH_WALKERS`) in two
    /// folders hashes alike; its patch edited, otherwise.
    #[test]
    fn the_test_games_mod_hashes_alike_and_an_edit_differs() {
        use regent_testgame::{TOUGH_WALKERS, TOUGH_WALKERS_FILE};

        let held = |dir: &Path, text: &str| {
            std::fs::write(dir.join(TOUGH_WALKERS_FILE), text).expect("written");

            let library = mods::scan(&[dir.to_path_buf()]);
            let found = library.mods.first().expect("the mod");

            (found.title(), hash(found))
        };
        let (one, two, edited) = (
            crate::testdata::scratch("mod-hash-test-one"),
            crate::testdata::scratch("mod-hash-test-two"),
            crate::testdata::scratch("mod-hash-test-edited"),
        );
        let (title, first) = held(one.path(), TOUGH_WALKERS);
        let tougher = TOUGH_WALKERS.replace("MaxHP = 90", "MaxHP = 91");

        assert_eq!(title, "Tough Walkers");
        assert_eq!(held(two.path(), TOUGH_WALKERS).1, first);
        assert_ne!(held(edited.path(), &tougher).1, first);
    }
}
