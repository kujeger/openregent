//! An interface picture's file name, as `regent_cli ui-dump` writes it and
//! openregent's override folder reads it: one rule both ways.
//!
//! ```text
//!  <archive>/<IMAG>_<set>_s<slot>_f<frame>.png   a set's frame
//!  portraits/<archive>/…                         the same, maindata's sets
//!  tiles/<archive>_<n>.png                       a raw tile
//! ```
//!
//! `<archive>` is the file name without extension, lower case, of the
//! archive holding the `IMAG` record (or the tile): base and expansion
//! pictures differ by it (`interfacedata`, `mx_interfacedata`). `<IMAG>`
//! is the record id trimmed, odd bytes `_` (`CUT ` is `CUT`); `<set>` the
//! whole packed set id.

use std::path::Path;

use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;

/// The folder raw tiles are under.
pub const TILES: &str = "tiles";

/// The folder `maindata`'s interface sets are under in a dump.
pub const PORTRAITS: &str = "portraits";

/// The files' extension.
pub const EXTENSION: &str = "png";

/// One picture, as its file names it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    /// Frame `frame` of slot `slot` of set `set` of `IMAG` `record`, in
    /// `archive`.
    Frame {
        archive: String,
        record: String,
        set: u32,
        slot: usize,
        frame: usize,
    },
    /// Tile `index` of `archive`.
    Tile { archive: String, index: usize },
}

impl Key {
    /// A set's frame; `archive` the archive's file (a path or a name).
    #[must_use]
    pub fn frame(archive: &str, record: FourCc, set: u32, slot: usize, frame: usize) -> Key {
        Key::Frame {
            archive: stem(archive),
            record: safe(record),
            set,
            slot,
            frame,
        }
    }

    /// A raw tile; `archive` the archive's file.
    #[must_use]
    pub fn tile(archive: &str, index: usize) -> Key {
        Key::Tile {
            archive: stem(archive),
            index,
        }
    }

    /// The frame of `record`'s set `set` in `catalogue`, in the archive
    /// its `IMAG` is found in (`Catalogue::find`: the last loaded).
    #[must_use]
    pub fn of(
        catalogue: &Catalogue<'_>,
        record: FourCc,
        set: u32,
        slot: usize,
        frame: usize,
    ) -> Option<Key> {
        let at = catalogue.find(b"IMAG", record)?;
        let (file, _) = catalogue.archives.get(at.archive)?;

        Some(Key::frame(file, record, set, slot, frame))
    }

    /// Its path without the extension, `/` between folders.
    ///
    /// ```text
    ///  interfacedata/AP29_8003_s0_f0    tiles/interfacedata_12
    /// ```
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Key::Frame {
                archive,
                record,
                set,
                slot,
                frame,
            } => format!("{archive}/{record}_{set}_s{slot}_f{frame}"),
            Key::Tile { archive, index } => format!("{TILES}/{archive}_{index}"),
        }
    }

    /// The picture a file at `path` (relative, `/` between folders, with
    /// its extension) names; `None` for any other file. A dump's
    /// `portraits/` folder is read as if it were not there.
    #[must_use]
    pub fn parse(path: &str) -> Option<Key> {
        let stem = path
            .strip_suffix(EXTENSION)
            .and_then(|p| p.strip_suffix('.'))?;
        let stem = stem
            .strip_prefix(PORTRAITS)
            .and_then(|p| p.strip_prefix('/'))
            .unwrap_or(stem);
        let (folder, file) = stem.split_once('/')?;

        if folder == TILES {
            let (archive, index) = file.rsplit_once('_')?;

            Some(Key::Tile {
                archive: lower(archive)?,
                index: index.parse().ok()?,
            })
        } else {
            let mut parts = file.rsplitn(4, '_');
            let frame = parts.next()?.strip_prefix('f')?.parse().ok()?;
            let slot = parts.next()?.strip_prefix('s')?.parse().ok()?;
            let set = parts.next()?.parse().ok()?;
            let record = parts.next().filter(|r| is_record(r))?;

            Some(Key::Frame {
                archive: lower(folder)?,
                record: record.to_owned(),
                set,
                slot,
                frame,
            })
        }
    }
}

/// A folder name as a key holds it: lower case; `None` with a `/`.
fn lower(name: &str) -> Option<String> {
    (!name.is_empty() && !name.contains('/')).then(|| name.to_lowercase())
}

/// Whether `name` is a record id as [`safe`] writes it.
fn is_record(name: &str) -> bool {
    (1..=4).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A file's name without its folders and extension, lower case.
#[must_use]
pub fn stem(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .map_or_else(|| file.to_owned(), |s| s.to_string_lossy().into_owned())
        .to_lowercase()
}

/// A record id safe in a file name: `CUT ` is `CUT`, odd bytes `_`.
#[must_use]
pub fn safe(id: FourCc) -> String {
    id.trimmed()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frames_name_reads_back() {
        let key = Key::frame("Data/interfacedata.cam", FourCc(*b"AP29"), 8003, 1, 2);

        assert_eq!(key.name(), "interfacedata/AP29_8003_s1_f2");
        assert_eq!(Key::parse("interfacedata/AP29_8003_s1_f2.png"), Some(key));
    }

    #[test]
    fn a_trimmed_or_odd_record_id_reads_back() {
        for id in [*b"CUT ", *b"IN#1"] {
            let key = Key::frame("DataMX/mx_interfacedata.cam", FourCc(id), 1000, 0, 0);

            assert_eq!(Key::parse(&format!("{}.png", key.name())), Some(key));
        }
    }

    #[test]
    fn a_tiles_name_reads_back() {
        let key = Key::tile("Data/interfacedata.cam", 12);

        assert_eq!(key.name(), "tiles/interfacedata_12");
        assert_eq!(Key::parse("tiles/interfacedata_12.png"), Some(key));
    }

    /// A dump's portraits folder names the same frame; the archive's case
    /// does not matter.
    #[test]
    fn a_portrait_folder_and_case_are_read_through() {
        let key = Key::frame("maindata.cam", FourCc(*b"ABJ1"), 1000, 0, 0);

        assert_eq!(
            Key::parse("portraits/maindata/ABJ1_1000_s0_f0.png"),
            Some(key.clone())
        );
        assert_eq!(Key::parse("MainData/ABJ1_1000_s0_f0.png"), Some(key));
    }

    /// A dump's second record of one id (`_r<n>`), another extension, a
    /// file at the top: no picture.
    #[test]
    fn other_files_name_no_picture() {
        for path in [
            "interfacedata/AP29_8003_s0_f0_r12.png",
            "interfacedata/AP29_8003_s0_f0.bmp",
            "AP29_8003_s0_f0.png",
            "interfacedata/LONGER_8003_s0_f0.png",
            "tiles/interfacedata_x.png",
            "index.tsv",
        ] {
            assert_eq!(Key::parse(path), None, "{path}");
        }
    }
}
