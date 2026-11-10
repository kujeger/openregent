//! Autosaves (`[game] autosave`): the original's
//! (`regent_core::view::menu::autosave`), every 1, 5 or 10 game days where it
//! offers 5, 10 and 20, the last [`KEEP`] kept where it writes one file
//! over (`docs/openregent/enhancements.md`).
//!
//! ```text
//!  Sim::step  frame 1, N + 1, 2N + 1... run ──> GMTX 335 said; Taken, the
//!             game at that frame: on every LAN peer at the same one
//!  ui::keep   Taken, this machine's view and minimap ──> save::thread:
//!             save::made, then written as autosave.<n>.tar.zst in the
//!             profile's saves folder for the game (save::dir); the
//!             autosaves past the newest KEEP removed
//! ```
//!
//! Each peer saves alone, unasked, at its own setting: a frame is one
//! game on every peer, so peers of one setting hold the same game
//! (`save::game_hash`) and a lobby finds it as it finds a saved one. The
//! original's guests take the host's interval instead. The files' `.`
//! is a character [`crate::files::profile::safe`] never gives a save's file, so
//! no save the player named is ever counted or removed.

use std::path::{Path, PathBuf};

use regent_core::view::menu::autosave::NAME;

use crate::save::{self, Contents, FromSim};

/// Autosaves kept: the oldest past it removed.
pub const KEEP: usize = 10;

/// An autosave's file name starts so, its number after.
const PREFIX: &str = "autosave.";

/// An autosave a step took: the day reached, and the game.
pub struct Taken {
    pub day: u32,
    pub game: FromSim,
}

/// An autosave's name: the original's, and the day three digits wide,
/// so the list, sorted by name, holds them in day order up to day 999.
pub fn name(day: u32) -> String {
    format!("{NAME} Day {day:03}")
}

/// `path`'s autosave number, if it is an autosave's file.
fn number(path: &Path) -> Option<u32> {
    let file = path.file_name()?.to_str()?;

    file.strip_prefix(PREFIX)?
        .strip_suffix(save::EXTENSION)?
        .parse()
        .ok()
}

/// The autosaves in `dir`, oldest first.
fn numbered(dir: &Path) -> Vec<(u32, PathBuf)> {
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| number(&p).map(|n| (n, p)))
        .collect();

    found.sort();
    found
}

/// `contents` written to `dir` as the next autosave, then the oldest
/// past [`KEEP`] removed: the file written. The caller writes one save
/// at a time (`save::thread`): the number and the rotation read the
/// folder.
///
/// # Errors
///
/// A file that will not be written. A removal that fails is warned of.
pub fn write(dir: &Path, contents: &Contents) -> Result<PathBuf, String> {
    rotated(dir, |path| save::write(path, contents))
}

/// [`write()`]'s, the file written by `put`.
fn rotated(dir: &Path, put: impl FnOnce(&Path) -> Result<(), String>) -> Result<PathBuf, String> {
    let next = numbered(dir).last().map_or(1, |(n, _)| n + 1);
    let path = dir.join(format!("{PREFIX}{next}{}", save::EXTENSION));

    put(&path)?;

    let found = numbered(dir);
    let past = found.len().saturating_sub(KEEP);

    for (_, old) in &found[..past] {
        if let Err(e) = std::fs::remove_file(old) {
            bevy::log::warn!("autosave {}: {e}", old.display());
        }
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_autosaves_name_sorts_by_day() {
        let names = [name(9), name(10), name(100)];

        assert_eq!(names[0], "autosave Day 009");
        assert!(names.is_sorted_by(|a, b| { regent_core::view::menu::saves::order(a, b).is_le() }));
    }

    /// A fresh scratch folder of its own.
    fn scratch(test: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("autosave-{test}"))
    }

    /// Twelve autosaves leave the newest ten; a save the player named,
    /// and a file named as an autosave would be had `.` been allowed,
    /// stay.
    #[test]
    fn rotation_keeps_the_newest_ten_autosaves_and_every_other_save() {
        let dir = scratch("rotation");
        let mine = dir.join(format!("autosave 3{}", save::EXTENSION));
        let named = dir.join(format!("autosave Day 001{}", save::EXTENSION));

        std::fs::write(&mine, b"").expect("a save");
        std::fs::write(&named, b"").expect("a save");

        for _ in 1..=12 {
            rotated(&dir, |p| std::fs::write(p, b"").map_err(|e| e.to_string())).expect("written");
        }

        let kept: Vec<u32> = numbered(&dir).into_iter().map(|(n, _)| n).collect();

        assert_eq!(kept, (3..=12).collect::<Vec<_>>());
        assert!(mine.exists() && named.exists());
        assert_eq!(save::list(&dir).len(), KEEP + 2);
    }
}
