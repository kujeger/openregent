//! What every file of this game's own does on disk: TOML read and
//! written whole, its version checked, a name made safe for a file, a
//! fresh file name, a directory's files listed and made.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use regent_core::data::fnv;
use serde::{Deserialize, Serialize};

/// The files' format. A file of another version is refused, not
/// overwritten.
pub const VERSION: u32 = 1;

/// What stands in for a character a file name should not hold.
const STAND_IN: char = '_';

/// A file's version, refused unless [`VERSION`].
pub(crate) fn checked(version: u32, path: &Path) -> Result<(), String> {
    if version == VERSION {
        Ok(())
    } else {
        Err(format!(
            "{}: version {version}, this openregent reads {VERSION}",
            path.display()
        ))
    }
}

/// `path` read as TOML.
pub(crate) fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `value` written to `path` as TOML, whole or not at all: a file beside
/// it, then renamed over.
pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let text = toml::to_string(value).map_err(|e| format!("{}: {e}", path.display()))?;

    replace(path, &text)
}

/// `text` written to `path`, whole or not at all: a file beside it, then
/// renamed over.
pub(crate) fn replace(path: &Path, text: &str) -> Result<(), String> {
    let temporary = path.with_extension("toml.new");

    let written = std::fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });

    written
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// `dir` made, with its parents.
pub(crate) fn make_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))
}

/// The directory `path` goes in made, if it names one.
pub(crate) fn make_parent(path: &Path) -> Result<(), String> {
    path.parent().map_or(Ok(()), make_dir)
}

/// `name` safe as a file's stem: each character but a letter, digit,
/// space, `-` or `_` made `_`; none with no letter or digit left.
///
/// ```text
///  "a/b\c"  ->  "a_b_c"
///  "../"    ->  None
/// ```
pub(crate) fn stem(name: &str) -> Option<String> {
    let safe = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_');
    let stem: String = name
        .chars()
        .map(|c| if safe(c) { c } else { STAND_IN })
        .collect();

    stem.chars().any(char::is_alphanumeric).then_some(stem)
}

/// A file in `dir` named `stem` with `extension`, numbered past any
/// already there.
///
/// ```text
///  a_b.toml, a_b-2.toml, a_b-3.toml
/// ```
pub(crate) fn fresh(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let path = |n: usize| {
        let numbered = if n == 1 {
            stem.to_owned()
        } else {
            format!("{stem}-{n}")
        };

        dir.join(numbered).with_extension(extension)
    };

    (1..usize::MAX)
        .map(path)
        .find(|p| !p.exists())
        .unwrap_or_else(|| path(1))
}

/// The files in `dir`, by file name; none where it does not read.
pub(crate) fn files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.filter_map(|e| Some(e.ok()?.path())).collect())
        .unwrap_or_default();

    paths.sort();
    paths
}

/// `files`' bytes in order hashed (FNV-1a, 64 bits), as hex: what a
/// save's GPL images were (`crate::game::load::images_hash`).
pub fn hash<'a>(files: impl IntoIterator<Item = &'a [u8]>) -> String {
    // A file at a time: a slice's bytes fold in a tighter loop than the
    // files flattened.
    let h = files
        .into_iter()
        .fold(fnv::START, |h, file| fnv::fold(h, file.iter().copied()));

    format!("{h:016x}")
}

/// [`files`] of `extension` alone.
pub(crate) fn with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut paths = files(dir);

    paths.retain(|p| p.extension().is_some_and(|e| e == extension));
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("store-{name}"))
    }

    #[test]
    fn a_stem_keeps_letters_digits_space_dash_and_underscore() {
        assert_eq!(stem("Your Majesty").as_deref(), Some("Your Majesty"));
        assert_eq!(stem("a/b\\c").as_deref(), Some("a_b_c"));
        assert_eq!(stem("Ærlig-1_x").as_deref(), Some("Ærlig-1_x"));
        assert_eq!(stem("../"), None);
        assert_eq!(stem(""), None);
    }

    #[test]
    fn a_fresh_name_is_numbered_past_the_files_there() {
        let dir = scratch("fresh");

        assert_eq!(fresh(&dir, "a_b", "toml"), dir.join("a_b.toml"));

        std::fs::write(dir.join("a_b.toml"), "").unwrap();
        std::fs::write(dir.join("a_b-2.toml"), "").unwrap();

        assert_eq!(fresh(&dir, "a_b", "toml"), dir.join("a_b-3.toml"));
    }

    #[test]
    fn files_are_listed_by_name_and_by_extension() {
        let dir = scratch("listed");

        for name in ["b.toml", "a.toml", "c.BTD"] {
            std::fs::write(dir.join(name), "").unwrap();
        }

        assert_eq!(
            files(&dir),
            [dir.join("a.toml"), dir.join("b.toml"), dir.join("c.BTD")]
        );
        assert_eq!(
            with_extension(&dir, "toml"),
            [dir.join("a.toml"), dir.join("b.toml")]
        );
        assert_eq!(files(&dir.join("none")), Vec::<PathBuf>::new());
    }
}
