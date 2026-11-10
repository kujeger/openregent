//! The corpus: archives indexed, and a dataset's XML made into one more
//! ([`Sources`]).
//!
//! # Why the archives leak
//!
//! [`regent_core::view`] types borrow ([`Catalogue`] from archive bytes, a
//! `Unit` from records), but a Bevy `Resource` must be `'static`. So each
//! archive file is read once and leaked ([`archives`]): a corpus opened
//! again on the same files reads nothing, and its own index frees when
//! dropped. The result is `Send + Sync`, and a resolved `Unit` can be
//! cached across frames.
//!
//! A file is read but its videos and sounds ([`cam::DEFERRED`]): a
//! record of those reads its own bytes as it plays ([`cam::Record::bytes`]).
//!
//! ```text
//!  path ── cam::Image::read ── leaked ──> cam::Archive ─┐
//!  XML ─── desc::xml ───────── leaked ──> cam::Archive ─┴─> Catalogue::of
//! ```

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use bevy::prelude::*;
use regent_core::content::dataset::{Files, IMAGE_SETS};
use regent_core::content::flags;
use regent_core::content::unit::Unit;
use regent_core::data::cam;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::xml;
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;

use crate::art::ArtSource;

/// An archive made in memory: its name and bytes, kept for the process.
type Archive = (&'static str, &'static [u8]);

/// An archive file's name and what was read of it, kept for the process.
type Read = (&'static str, &'static cam::Image);

/// Archive files read, by path: each read once a process, every corpus
/// over it borrowing the same bytes.
static ARCHIVES: Mutex<BTreeMap<PathBuf, Read>> = Mutex::new(BTreeMap::new());

/// Whether [`archives`] leaves the `TILE` pixels on disk too
/// ([`cam::Defer::Pixels`]): set by a game without a screen, before it
/// reads any.
static HEADLESS: AtomicBool = AtomicBool::new(false);

/// Read the archives from here on without their pixels: nothing here
/// draws them, and the simulation reads the few it measures from disk.
pub fn headless() {
    HEADLESS.store(true, Ordering::Relaxed);
}

/// `paths`' archives in order, each read once a process (`ARCHIVES`), but
/// their [`cam::DEFERRED`] payloads (and, [`headless`], their pixels).
///
/// # Errors
///
/// If a path will not read.
pub fn archives(paths: &[PathBuf]) -> Result<Vec<Read>, String> {
    let mut read = ARCHIVES.lock().map_err(|e| e.to_string())?;

    paths
        .iter()
        .map(|path| {
            if let Some(archive) = read.get(path) {
                Ok(*archive)
            } else {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into(),
                );
                let defer = if HEADLESS.load(Ordering::Relaxed) {
                    cam::Defer::Pixels
                } else {
                    cam::Defer::Media
                };
                let image = cam::Image::read_as(path, defer).map_err(|e| e.to_string())?;
                let archive: Read = (name.leak(), Box::leak(Box::new(image)));

                read.insert(path.clone(), archive);
                Ok(archive)
            }
        })
        .collect()
}

/// `paths`' archives indexed, their bytes shared ([`archives`]).
///
/// # Errors
///
/// If a path will not read, or an archive will not parse.
pub fn catalogue(paths: &[PathBuf]) -> Result<Catalogue<'static>, String> {
    catalogue_of(&Sources::archives(paths))
}

/// What a corpus is opened on: archives in load order, then the XML a
/// dataset, quest or mod lists ([`Files`]).
///
/// ```text
///  archives ──────────────────────> Catalogue::parse ─┐
///  descriptions ── desc::xml over it ──> one archive ──┴─> the catalogue
///  strings ─────────────────────────────────────────────> Text::with_strings
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    pub archives: Vec<PathBuf>,
    /// `<Descriptions>`, made into one archive loaded after the rest.
    pub descriptions: Vec<PathBuf>,
    /// Inline `mod.toml` patches, applied after XML descriptions.
    pub flag_patches: Vec<(PathBuf, String)>,
    /// `<Strings>`, the text's dictionary.
    pub strings: Vec<PathBuf>,
    /// The image set names an XML action reads ([`IMAGE_SETS`]).
    pub image_sets: Option<PathBuf>,
}

impl Sources {
    /// Archives alone.
    #[must_use]
    pub fn archives(paths: &[PathBuf]) -> Sources {
        Sources {
            archives: paths.to_vec(),
            ..Sources::default()
        }
    }

    /// A dataset's files, on the install at `hd`.
    #[must_use]
    pub fn of(files: &Files, hd: &Path) -> Sources {
        Sources {
            archives: files.archives.clone(),
            descriptions: files.descriptions.clone(),
            flag_patches: Vec::new(),
            strings: files.strings.clone(),
            image_sets: Some(hd.join(IMAGE_SETS)),
        }
    }

    /// Every file in order: what a corpus is known by
    /// ([`Corpus::paths`]).
    #[must_use]
    pub fn files(&self) -> Vec<PathBuf> {
        self.archives
            .iter()
            .chain(&self.descriptions)
            .chain(self.flag_patches.iter().map(|(path, _)| path))
            .chain(&self.strings)
            .cloned()
            .collect()
    }
}

/// Archives made from XML, by the files they were made of: each made
/// once a process, as [`ARCHIVES`] reads each file once.
static MADE: Mutex<BTreeMap<Vec<PathBuf>, Archive>> = Mutex::new(BTreeMap::new());

/// The name a made archive goes by.
const MADE_NAME: &str = "descriptions.xml";
const FLAGS_NAME: &str = "unit-flags.toml";
type FlagKey = (Vec<PathBuf>, Vec<(PathBuf, String)>);
static FLAG_ARCHIVES: Mutex<BTreeMap<FlagKey, Archive>> = Mutex::new(BTreeMap::new());

/// `sources`' descriptions made into an archive over `base`, its
/// archives' catalogue ([`MADE`]); none without descriptions.
fn made(sources: &Sources, base: &Catalogue<'_>) -> Result<Option<Archive>, String> {
    let mut made = MADE.lock().map_err(|e| e.to_string())?;
    let compile = || {
        let sets = sources
            .image_sets
            .as_deref()
            .map(xml::ImageSets::read)
            .unwrap_or_default();
        let compiled = xml::Sources::read(&sources.descriptions).compile(base, &sets);

        (MADE_NAME, &*compiled.archive().leak())
    };

    if sources.descriptions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(*made.entry(sources.files()).or_insert_with(compile)))
    }
}

/// `sources`' catalogue: its archives, then its descriptions made into
/// one more (`made`).
///
/// # Errors
///
/// If a path will not read, or an archive will not parse.
pub fn catalogue_of(sources: &Sources) -> Result<Catalogue<'static>, String> {
    let parse = |(name, bytes): Archive| {
        cam::Archive::parse(bytes)
            .map(|a| (name, a))
            .map_err(|e| format!("{name}: {e}"))
    };
    let mut loaded = archives(&sources.archives)?
        .into_iter()
        .filter(|(_, image)| !image.is_empty())
        .map(|(name, image)| {
            image
                .archive()
                .map(|a| (name, a))
                .map_err(|e| format!("{name}: {e}"))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let base = Catalogue::of(loaded.clone());

    if let Some(extra) = made(sources, &base)? {
        loaded.push(parse(extra)?);
    }
    let base = Catalogue::of(loaded.clone());

    if !sources.flag_patches.is_empty() {
        let mut cache = FLAG_ARCHIVES.lock().map_err(|e| e.to_string())?;
        let key = (sources.files(), sources.flag_patches.clone());
        let patch = if let Some(patch) = cache.get(&key) {
            *patch
        } else {
            let patch = (
                FLAGS_NAME,
                &*flags::compile(&base, &sources.flag_patches)
                    .map_err(|e| e.to_string())?
                    .leak(),
            );
            cache.insert(key, patch);
            patch
        };
        loaded.push(parse(patch)?);
    }

    Ok(Catalogue::of(loaded))
}

/// `sources`' text: its catalogue's tables, its strings' dictionary.
#[must_use]
pub fn text_of(sources: &Sources, catalogue: &Catalogue<'static>) -> Text<'static> {
    Text::build(catalogue).with_strings(xml::Sources::read(&sources.strings).strings())
}

/// The opened corpus: every archive, indexed, and its text.
#[derive(Resource)]
pub struct Corpus {
    pub catalogue: Catalogue<'static>,
    /// Game text for numeric save codes (e.g. all 177 activity codes).
    pub text: Text<'static>,
    /// Named archives that hold nothing.
    pub empty: Vec<String>,
    /// The archive files it was opened on, in load order.
    paths: Vec<PathBuf>,
    /// [`ArtSource::of`] `paths`, hashed once: the art service asks per frame.
    source: ArtSource,
    /// Units parsed so far ([`Corpus::unit`]).
    units: RwLock<HashMap<FourCc, Option<Arc<Unit<'static>>>>>,
}

impl Corpus {
    /// Open the archives and index them.
    ///
    /// # Errors
    ///
    /// If a file will not read, or an archive will not parse.
    pub fn open(paths: &[PathBuf]) -> Result<Self, String> {
        Corpus::with(&Sources::archives(paths))
    }

    /// Open a dataset's files: archives, descriptions and strings
    /// ([`catalogue_of`], [`text_of`]).
    ///
    /// # Errors
    ///
    /// If a file will not read, or an archive will not parse.
    pub fn with(sources: &Sources) -> Result<Self, String> {
        let empty: Vec<String> = archives(&sources.archives)?
            .into_iter()
            .filter(|(_, image)| image.is_empty())
            .map(|(n, _)| n.to_owned())
            .collect();

        if empty.len() == sources.archives.len() {
            return Err("no non-empty .cam archives in those paths".into());
        }

        let catalogue = catalogue_of(sources)?;
        let text = text_of(sources, &catalogue);
        let paths = sources.files();

        Ok(Corpus {
            text,
            catalogue,
            empty,
            source: ArtSource::of(&paths),
            paths,
            units: RwLock::default(),
        })
    }

    /// The files it was opened on, in load order: archives, then XML
    /// ([`Sources::files`]).
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Which catalogue this is, to the art service.
    pub const fn source(&self) -> ArtSource {
        self.source
    }

    /// Unit `code` ([`Unit::find`]), parsed once a corpus: drawing asks
    /// for every piece each frame. `None` without one, or where it will
    /// not parse.
    pub fn unit(&self, code: FourCc) -> Option<Arc<Unit<'static>>> {
        let known = self.units.read().ok().and_then(|u| u.get(&code).cloned());

        known.unwrap_or_else(|| {
            let unit = Unit::find(&self.catalogue, code)
                .and_then(Result::ok)
                .map(Arc::new);

            if let Ok(mut units) = self.units.write() {
                units.insert(code, unit.clone());
            }

            unit
        })
    }
}

#[cfg(test)]
impl Corpus {
    /// A corpus of no archives: nothing in it has art.
    pub(crate) fn bare() -> Self {
        let catalogue = Catalogue::parse(std::iter::empty()).expect("nothing to parse");

        Corpus {
            text: Text::build(&catalogue),
            catalogue,
            empty: Vec::new(),
            paths: Vec::new(),
            source: ArtSource::of(&[]),
            units: RwLock::default(),
        }
    }
}

/// Zoom range for slider and wheel; value in [`crate::stage::View`]. Low floor
/// fits the whole tileset (over 6000 px tall).
pub const ZOOM_MIN: f32 = 0.02;
pub const ZOOM_MAX: f32 = 32.0;
