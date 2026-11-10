//! What an install holds, found and joined: one layer above [`data`].
//!
//! Where [`data`] reads one file or record strictly, this finds the files
//! on a machine and puts them together:
//!
//! - discovery: [`env`](mod@env) (the names installs are found by), [`on_disk`] (a listed
//!   path in any case), [`dataset`] (which files load together), [`quest`]
//!   and [`mods`] (folders scanned for manifests);
//! - joins over several records: [`unit`](mod@unit) and [`imagery`] (a
//!   `DUNT` and its art across archives), [`terrain`] (a `DTRN`, its
//!   tileset and classes, and the vertex model of a map's ground);
//! - the project's own files: [`flags`] (a mod's unit patches and `.dat`
//!   member values, TOML), and the `mod.toml` and `quest.toml` manifests.
//!
//! Not a file format: a format, or a value type one stores, is [`data`].
//! Not strict either: [`dataset::parse`] reads what it can, and the
//! folder scans skip a manifest that will not read.
//! Uses only [`base`] and [`data`].
//!
//! [`base`]: crate::base
//! [`data`]: crate::data

pub mod dataset;
pub mod env;
mod error;
pub mod flags;
pub mod imagery;
pub mod mods;
pub mod quest;
pub mod terrain;
pub mod unit;

use std::path::{Path, PathBuf};

pub use error::Error;

/// Find what a listed path actually is on disk, ignoring case.
///
/// `gpl::proj::parse` fixes separators; this fixes case. The base game's project lists
/// `Check_rewards.gpl` for `check_rewards.gpl`; 46 of its 141 entries differ
/// from disk in case.
///
/// Tries the exact path first (one `stat`), then walks component by component.
///
/// `None` if no spelling of the path exists.
pub fn on_disk(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return Some(path.to_path_buf());
    }

    let mut at = PathBuf::new();

    for component in path.components() {
        let exact = at.join(component);

        if exact.exists() {
            at = exact;
            continue;
        }

        // Case differs: find the matching directory entry.
        let wanted = component.as_os_str().to_str()?;
        at = std::fs::read_dir(&at)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(wanted))
            })?;
    }

    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_disk_finds_a_differently_spelled_path() {
        let dir = std::env::temp_dir().join("majesty-proj-on-disk");
        let nested = dir.join("TaskModules");

        std::fs::create_dir_all(&nested).expect("a scratch directory");
        std::fs::write(nested.join("rest.gpl"), "").expect("a scratch file");

        // What a project would list, in the case Windows recorded.
        let listed = dir.join("taskmodules").join("Rest.GPL");

        assert_eq!(on_disk(&listed), Some(nested.join("rest.gpl")));
        // An exact path is returned unchanged.
        assert_eq!(
            on_disk(&nested.join("rest.gpl")),
            Some(nested.join("rest.gpl"))
        );
        assert_eq!(on_disk(&dir.join("nothing.gpl")), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
