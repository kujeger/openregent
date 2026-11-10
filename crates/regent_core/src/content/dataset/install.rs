//! HD's dataset files on disk: each list composed, rooted and found.
//!
//! ```text
//!  Data/MajestyDatasetDefinitions.xml ────────┐ dataset compose:
//!  DataMX/MajestyExpansionDatasetDefinitions ─┤ base list, less own Unload,
//!  a quest's .mqxml, its own lists ───────────┤ plus own Load not present
//!  each active mod whose base fits, in order ─┘ (mods compose)
//!                                    │
//!                     $(Root) resolved, `\` made `/`, case fixed
//!                                    │
//!                                    v
//!  Files: archives (load order), programs (load order), chart
//! ```

use std::fmt;
use std::path::{Path, PathBuf};

use super::{FileSet, Listed, Lists, PathRef, compose, load_order};
use crate::content::mods::Mod;
use crate::content::quest::Quest;

/// HD's two definition files, base first.
pub const BASE_DEFINITIONS: &str = "Data/MajestyDatasetDefinitions.xml";
pub const EXPANSION_DEFINITIONS: &str = "DataMX/MajestyExpansionDatasetDefinitions.xml";

/// Each definition's directory: an unrooted path is under it.
const BASE_DIR: &str = "Data";
const EXPANSION_DIR: &str = "DataMX";

/// The install's own roots, which a quest's definition names (Krolm's
/// `$(MajestyDataPath)/MDL1.bcd`).
const INSTALL_ROOTS: [(&str, &str); 2] = [
    ("MajestyDataPath", BASE_DIR),
    ("MajestyExpansionDataPath", EXPANSION_DIR),
];

/// The roots a GPL path uses, under [`Bytecode`]'s top directory.
const BYTECODE_ROOTS: [(&str, &str); 2] = [
    ("MajestyBytecodeDataPath", BASE_DIR),
    ("MajestyExpansionBytecodeDataPath", EXPANSION_DIR),
];

/// Where the SDK keeps its copies of the install's images.
const SDK_ORIGINALS: &str = "OriginalQuests";

/// Which of HD's two datasets a game loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Datasets {
    /// The base game's alone.
    Base,
    /// The base game's, then the expansion's.
    Expansion,
}

impl Datasets {
    /// The dataset `quest`'s definition extends.
    #[must_use]
    pub fn of(quest: &Quest) -> Datasets {
        if quest.expansion() {
            Datasets::Expansion
        } else {
            Datasets::Base
        }
    }
}

/// Which copies of the GPL images the bytecode roots name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bytecode<'a> {
    /// The install's own: what HD loads without `-gpldebug`.
    Install,
    /// The SDK's (`SDK/`, given): what HD loads under `-gpldebug`, and so
    /// what the traces ran.
    Sdk(&'a Path),
}

/// What a game loads: archives in order, GPL images in load order
/// ([`load_order`]), the generator's chart, and the XML files in order:
/// descriptions ([`crate::data::desc::xml`]) and strings
/// ([`crate::data::text::Strings`]).
///
/// An XML file listed but not on disk is left out: HD registers the
/// path and its read fails (desc set load xml, the strings loader
/// strings file load), loading nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Files {
    pub archives: Vec<PathBuf>,
    pub programs: Vec<PathBuf>,
    pub chart: PathBuf,
    pub descriptions: Vec<PathBuf>,
    pub strings: Vec<PathBuf>,
}

/// Where HD keeps the image sets' names, which an XML action's
/// `ImageSet` is read through ([`crate::data::desc::xml::ImageSets`]).
pub const IMAGE_SETS: &str = "Data/ImageSetIDXRef.xml";

/// Why an install's files could not be listed.
#[derive(Debug)]
pub enum Error {
    /// A definition file that will not read.
    Read(PathBuf, std::io::Error),
    /// A `$(Root)` no table names.
    Root(String),
    /// A listed file not on disk in any case.
    Missing(PathBuf),
    /// No `<Constants>` left after composing.
    NoChart,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Read(path, e) => write!(f, "{}: {e}", path.display()),
            Error::Root(root) => write!(f, "unknown root $({root})"),
            Error::Missing(path) => write!(f, "{}: not found", path.display()),
            Error::NoChart => f.write_str("no constants listed"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Read(_, e) => Some(e),
            _ => None,
        }
    }
}

/// A file set's lists the engine loads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Archives,
    Programs,
    Constants,
    Descriptions,
    Strings,
}

/// What a listed file not on disk does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Absent {
    /// Refuses the list: archives, images, a chart.
    Refused,
    /// Loads nothing: an XML file ([`Files`]).
    Skipped,
}

impl Kind {
    const fn absent(self) -> Absent {
        match self {
            Kind::Archives | Kind::Programs | Kind::Constants => Absent::Refused,
            Kind::Descriptions | Kind::Strings => Absent::Skipped,
        }
    }
}

/// HD's install, its `$(Root)`s resolved.
///
/// ```no_run
/// use regent_core::content::dataset::{Bytecode, Datasets, Install};
///
/// let hd = std::path::Path::new("majesty_hd_windows");
/// let files = Install::new(hd, Bytecode::Install).files(Datasets::Expansion)?;
///
/// assert_eq!(files.archives.len(), 34);
/// # Ok::<(), regent_core::content::dataset::Error>(())
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Install<'a> {
    hd: &'a Path,
    bytecode: Bytecode<'a>,
}

impl<'a> Install<'a> {
    #[must_use]
    pub const fn new(hd: &'a Path, bytecode: Bytecode<'a>) -> Install<'a> {
        Install { hd, bytecode }
    }

    /// `sets`' files: what the front end loads with `Expansion`
    /// (dataset apply startup, `view::menu`), and a freestyle game.
    ///
    /// # Errors
    ///
    /// A definition file that will not read, or a file not on disk.
    pub fn files(&self, sets: Datasets) -> Result<Files, Error> {
        self.compose(sets, None, &[])
    }

    /// A quest's files: its dataset's, then its definition's own.
    ///
    /// # Errors
    ///
    /// A definition file that will not read, or a file not on disk.
    pub fn quest(&self, quest: &Quest) -> Result<Files, Error> {
        self.compose(Datasets::of(quest), Some(quest), &[])
    }

    /// [`Install::files`], then each of `mods` whose base fits `sets`, in
    /// order.
    ///
    /// # Errors
    ///
    /// As [`Install::files`]; a file a mod lists not on disk.
    pub fn files_with(&self, sets: Datasets, mods: &[&Mod]) -> Result<Files, Error> {
        self.compose(sets, None, mods)
    }

    /// [`Install::quest`], then each of `mods` whose base fits the
    /// quest's dataset, in order.
    ///
    /// # Errors
    ///
    /// As [`Install::quest`]; a file a mod lists not on disk.
    pub fn quest_with(&self, quest: &Quest, mods: &[&Mod]) -> Result<Files, Error> {
        self.compose(Datasets::of(quest), Some(quest), mods)
    }

    /// `sets`' lists, then `quest`'s own, then each fitting mod's, each
    /// composed as dataset compose does. The
    /// last chart left is the one loaded: the expansion unloads the
    /// base's.
    fn compose(
        &self,
        sets: Datasets,
        quest: Option<&Quest>,
        mods: &[&Mod],
    ) -> Result<Files, Error> {
        let base = self.text(BASE_DEFINITIONS)?;
        let extra = match sets {
            Datasets::Base => None,
            Datasets::Expansion => Some(self.text(EXPANSION_DEFINITIONS)?),
        };
        let mut defined = vec![(super::parse(&base), self.hd.join(BASE_DIR))];

        defined.extend(
            extra
                .as_deref()
                .map(|t| (super::parse(t), self.hd.join(EXPANSION_DIR))),
        );

        let list = |kind: Kind| -> Result<Vec<PathBuf>, Error> {
            let mut out: Vec<PathBuf> = Vec::new();

            for (set, dir) in &defined {
                // An unload entry not on disk has nothing to drop.
                let dropped: Vec<PathBuf> = listed(&set.unload, kind)
                    .iter()
                    .filter_map(|e| self.find(e, dir).ok())
                    .collect();

                out.retain(|p| !dropped.contains(p));
                out = compose(&[&out, &self.found(&listed(&set.load, kind), dir, kind)?]);
            }

            if let Some(q) = quest {
                out = compose(&[&out, &self.found(&own(q, kind), &q.dir, kind)?]);
            }

            // A mod for the other dataset is passed over, as HD does.
            for m in mods.iter().filter(|m| m.base.fits(sets)) {
                let dropped: Vec<PathBuf> = pick(&m.unload, kind)
                    .iter()
                    .filter_map(|e| self.find(e, &m.dir).ok())
                    .collect();

                out.retain(|p| !dropped.contains(p));
                out = compose(&[&out, &self.found(pick(&m.load, kind), &m.dir, kind)?]);
            }

            Ok(out)
        };

        Ok(Files {
            archives: list(Kind::Archives)?,
            programs: load_order(list(Kind::Programs)?),
            chart: list(Kind::Constants)?.pop().ok_or(Error::NoChart)?,
            descriptions: list(Kind::Descriptions)?,
            strings: list(Kind::Strings)?,
        })
    }

    /// The definition at `name` under the install.
    fn text(&self, name: &str) -> Result<String, Error> {
        let path = self.hd.join(name);

        std::fs::read(&path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| Error::Read(path, e))
    }

    /// Each of `entries` on disk. One file named two ways
    /// (`GPLDebuggerUI.cam`) is listed once.
    fn found(&self, entries: &[Listed], dir: &Path, kind: Kind) -> Result<Vec<PathBuf>, Error> {
        let paths = entries
            .iter()
            .map(|e| self.find(e, dir))
            .filter(|p| p.is_ok() || kind.absent() == Absent::Refused)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(compose(&[&paths]))
    }

    /// Where `entry` is: a bytecode root under [`Bytecode`]'s top, an
    /// install root, or an unrooted path under `dir`. Mods write `\`
    /// between parts (`Data\AdjustGuardhouse.bcd`). An absolute path is
    /// itself: no manifest writes one, openregent's compiled image is one.
    fn find(&self, entry: &Listed, dir: &Path) -> Result<PathBuf, Error> {
        let (root, path) = entry;
        let bytecode = match self.bytecode {
            Bytecode::Install => self.hd.to_path_buf(),
            Bytecode::Sdk(sdk) => sdk.join(SDK_ORIGINALS),
        };
        let under = |roots: &[(&str, &str)], top: &Path| {
            roots
                .iter()
                .find(|(name, _)| Some(*name) == root.as_deref())
                .map(|(_, at)| top.join(at))
        };
        let base = match root {
            None if Path::new(path).is_absolute() => PathBuf::from(std::path::MAIN_SEPARATOR_STR),
            None => dir.to_path_buf(),
            Some(name) => under(&BYTECODE_ROOTS, &bytecode)
                .or_else(|| under(&INSTALL_ROOTS, self.hd))
                .ok_or_else(|| Error::Root(name.clone()))?,
        };
        let want = path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty())
            .fold(base, |at, part| at.join(part));

        crate::content::on_disk(&want).ok_or(Error::Missing(want))
    }
}

/// One list of `set`.
fn listed(set: &FileSet<'_>, kind: Kind) -> Vec<Listed> {
    let list = match kind {
        Kind::Archives => &set.archives,
        Kind::Programs => &set.programs,
        Kind::Constants => &set.constants,
        Kind::Descriptions => &set.descriptions,
        Kind::Strings => &set.strings,
    };

    list.iter()
        .map(|p: &PathRef<'_>| (p.root.map(str::to_owned), p.path.to_owned()))
        .collect()
}

/// One list of a mod's `lists`.
const fn pick(lists: &Lists, kind: Kind) -> &[Listed] {
    match kind {
        Kind::Archives => lists.archives.as_slice(),
        Kind::Programs => lists.programs.as_slice(),
        Kind::Constants => lists.constants.as_slice(),
        Kind::Descriptions => lists.descriptions.as_slice(),
        Kind::Strings => lists.strings.as_slice(),
    }
}

/// One list of `quest`'s own definition; it names no chart.
fn own(quest: &Quest, kind: Kind) -> Vec<Listed> {
    match kind {
        Kind::Archives => quest.archives.clone(),
        Kind::Programs => quest.programs.clone(),
        Kind::Constants => Vec::new(),
        Kind::Descriptions => quest.descriptions.clone(),
        Kind::Strings => quest.strings.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base game's definition, one archive and one chart.
    const BASE: &str = "<Majesty><DataConfiguration name=\"Majesty\"><Dataset><Load>\
                        <CAM>a.cam</CAM><Constants>constants.rgs</Constants>\
                        </Load></Dataset></DataConfiguration></Majesty>";

    /// The expansion's: its own archive and chart, the base's unloaded.
    const EXPANSION: &str = "<Majesty><DataConfiguration name=\"MajestyExpansion\">\
                             <Dataset base=\"Majesty\"><Load><CAM>b.cam</CAM>\
                             <Constants>mx.rgs</Constants></Load><Unload>\
                             <CAM>$(MajestyDataPath)/a.cam</CAM>\
                             <Constants>$(MajestyDataPath)/constants.rgs</Constants>\
                             </Unload></Dataset></DataConfiguration></Majesty>";

    /// A scratch install under the system's temporary directory.
    fn install(name: &str, expansion: Option<&str>) -> PathBuf {
        let hd = std::env::temp_dir().join(name);
        let (data, mx) = (hd.join(BASE_DIR), hd.join(EXPANSION_DIR));

        std::fs::create_dir_all(&data).expect("a scratch directory");
        std::fs::create_dir_all(&mx).expect("a scratch directory");
        std::fs::write(hd.join(BASE_DEFINITIONS), BASE).expect("a definition");
        std::fs::write(data.join("a.cam"), "").expect("an archive");
        std::fs::write(data.join("constants.rgs"), "").expect("a chart");
        std::fs::write(mx.join("b.cam"), "").expect("an archive");
        std::fs::write(mx.join("mx.rgs"), "").expect("a chart");

        if let Some(text) = expansion {
            std::fs::write(hd.join(EXPANSION_DEFINITIONS), text).expect("a definition");
        }

        hd
    }

    #[test]
    fn the_base_dataset_needs_no_expansion_definition() {
        let hd = install("majesty-base-alone", None);
        let files = Install::new(&hd, Bytecode::Install).files(Datasets::Base);

        std::fs::remove_dir_all(&hd).ok();

        let files = files.expect("the base's files");

        assert_eq!(files.archives, [hd.join("Data/a.cam")]);
        assert_eq!(files.chart, hd.join("Data/constants.rgs"));
    }

    #[test]
    fn the_expansion_unloads_what_it_names() {
        let hd = install("majesty-unload", Some(EXPANSION));
        let files = Install::new(&hd, Bytecode::Install).files(Datasets::Expansion);

        std::fs::remove_dir_all(&hd).ok();

        let files = files.expect("the expansion's files");

        assert_eq!(files.archives, [hd.join("DataMX/b.cam")]);
        assert_eq!(files.chart, hd.join("DataMX/mx.rgs"));
    }
}
