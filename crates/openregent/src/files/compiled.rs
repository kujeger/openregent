//! Mods and quests that ship GPL source: compiled at the scan where their
//! `.bcd` is missing. An enhancement: HD loads `<Target>` only.
//!
//! ```text
//!  <GPL><Target>Data/x.bcd</Target><Source>GPL/x.gpl</Source>…</GPL>
//!     Target on disk       the .bcd loads, as in HD
//!     no Target, or none   sources hashed ──> cache hit: read
//!     on disk                            └─> miss: compiled, written
//!                          $XDG_CACHE_HOME/openregent/compiled-mods/<GUID>/<key>.bcd
//!                          └─> the mod's program list names that file
//! ```
//!
//! A `<Source>` is a `.gpl`, a `.dat`, or a `.gplproj` whose files join
//! in its place; compiled by `gpl::emit::compile_listed`, the path
//! `builtin` takes. The key hashes each file's path and bytes and the
//! compiler's source, so an edit to either recompiles. A mod that will not compile
//! is refused whole ([`Built::Refused`]).
//!
//! Its compiled images' hash ([`Built::Compiled`]) is recorded beside
//! its GUID, in a save's needs and `play.input`'s `--mods`
//! (`GUID@hash`): two sources under one GUID play apart, so a load of
//! another is refused, the mod named.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::log::{info, warn};
use regent_core::content::dataset::{Listed, Sourced};
use regent_core::content::mods::Library;
use regent_core::data::guid::Guid;
use regent_core::gpl::{emit, proj};

/// The cache folder under openregent's cache directory.
const CACHE: &str = "compiled-mods";

/// A compiled image's extension.
const IMAGE: &str = "bcd";

/// What a key hashes besides the files: the compiler's source
/// (`build.rs`), so a compiler change within a version recompiles.
const BUILD: &str = env!("MAJ_COMPILER");

/// A source's extensions: a project, and prototype data.
const PROJECT: &str = "gplproj";
const DATA: &str = "dat";

/// Whether an image was compiled now or read from the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cached {
    Fresh,
    Reused,
}

/// A mod's GPL source, compiled or refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Built {
    Compiled {
        /// Its compiled images' bytes, hashed in list order
        /// (`store::hash`).
        image: String,
        cached: Cached,
    },
    /// Why not: the compiler's `file:line:column: message`.
    Refused(String),
}

/// Each mod with source compiled, by GUID.
pub type Builds = BTreeMap<Guid, Built>;

/// The cache folder: under the cache directory, the tests' under the
/// system's temporary folder, kept across runs as a cache is.
pub fn folder() -> Option<PathBuf> {
    if cfg!(test) {
        Some(crate::files::env::test_cache().join(CACHE))
    } else {
        dirs::cache_dir().map(|d| d.join(crate::files::env::OWN).join(CACHE))
    }
}

/// Compile listed GPL sources. A failed source quest is removed.
pub fn resolve(library: &mut Library, root: &Path) -> Builds {
    let builds = library
        .mods
        .iter_mut()
        .filter_map(|m| {
            let built = build(
                m.guid,
                &m.dir,
                &m.manifest,
                &mut m.load.programs,
                &m.load.sourced,
                root,
            )?;

            match &built {
                Built::Compiled { image, cached } => {
                    info!(
                        "mod {}: GPL compiled from source ({cached:?}, {image})",
                        m.title()
                    );
                }
                Built::Refused(why) => warn!("mod {} refused: {why}", m.title()),
            }

            Some((m.guid, built))
        })
        .collect();

    let quest_root = root.join("quests");
    library.quests.retain_mut(|found| {
        let q = &mut found.quest;
        let manifest = q.dir.join("quest.toml");
        match build(
            q.id,
            &q.dir,
            &manifest,
            &mut q.programs,
            &q.sourced,
            &quest_root,
        ) {
            Some(Built::Refused(why)) => {
                warn!("quest {} refused: {why}", q.name);
                false
            }
            Some(Built::Compiled { cached, .. }) => {
                info!("quest {}: GPL compiled from source ({cached:?})", q.name);
                true
            }
            None => true,
        }
    });

    builds
}

/// Source groups to compile; `None` with none.
fn build(
    guid: Guid,
    dir: &Path,
    manifest: &Path,
    programs: &mut Vec<Listed>,
    sourced: &[Sourced<Listed>],
    root: &Path,
) -> Option<Built> {
    let wanted: Vec<_> = sourced
        .iter()
        .filter(|g| g.target.as_ref().is_none_or(|t| !present(dir, t)))
        .cloned()
        .collect();

    (!wanted.is_empty()).then(|| {
        let cache = root.join(format!("{:032X}", guid.0));
        let compiled = wanted
            .iter()
            .map(|g| image(dir, manifest, &g.sources, &cache))
            .collect::<Result<Vec<_>, String>>();

        match compiled {
            Ok(images) => {
                let files: Vec<&Path> = images.iter().map(|(f, _, _)| f.as_path()).collect();

                tidy(&cache, &files);

                // Replaced in place, or put in where the group stands;
                // from the last, so an insert moves no later place.
                for (g, (file, _, _)) in wanted.iter().zip(&images).rev() {
                    let listed = (None, file.display().to_string());

                    match &g.target {
                        Some(_) => programs[g.at] = listed,
                        None => programs.insert(g.at, listed),
                    }
                }

                let cached = if images.iter().all(|(_, _, c)| *c == Cached::Reused) {
                    Cached::Reused
                } else {
                    Cached::Fresh
                };

                Built::Compiled {
                    image: super::store::hash(images.iter().map(|(_, b, _)| b.as_slice())),
                    cached,
                }
            }
            Err(why) => Built::Refused(why),
        }
    })
}

/// Whether a target `listed` is on disk. One under a `$(Root)` is
/// taken as there: only the install resolves it.
fn present(dir: &Path, listed: &Listed) -> bool {
    match listed {
        (Some(_), _) => true,
        (None, path) => regent_core::content::on_disk(&under(dir, path)).is_some(),
    }
}

/// `path`, `\` or `/` between parts, under `dir`.
fn under(dir: &Path, path: &str) -> PathBuf {
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .fold(dir.to_path_buf(), |at, part| at.join(part))
}

/// The image `sources` compile to, its cache file under `dir`, and
/// whether it was there.
///
/// # Errors
///
/// A source not on disk or unreadable; a compile error.
fn image(
    source_dir: &Path,
    manifest: &Path,
    sources: &[Listed],
    cache: &Path,
) -> Result<(PathBuf, Vec<u8>, Cached), String> {
    let project = project(source_dir, sources)?;
    let file = cache.join(format!("{}.{IMAGE}", key(&project)?));

    if let Ok(bytes) = std::fs::read(&file) {
        Ok((file, bytes, Cached::Reused))
    } else {
        let bytes = emit::compile_listed(&project, manifest).map_err(|e| e.to_string())?;

        crate::files::builtin::write(&file, &bytes)?;
        Ok((file, bytes, Cached::Fresh))
    }
}

/// `sources` as one project: a `.gplproj`'s files in its place, a `.dat`
/// data, any other a GPL source; each found on disk in any case.
///
/// # Errors
///
/// A source under a `$(Root)`, or not on disk; a project unreadable.
fn project(dir: &Path, sources: &[Listed]) -> Result<proj::Project, String> {
    let mut out = proj::Project::default();

    for listed in sources {
        let (None, spelled) = listed else {
            return Err(format!("{}: a source under a $(Root)", listed.1));
        };
        let file = found(&under(dir, spelled))?;
        let is = |ext: &str| {
            file.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        };

        if is(PROJECT) {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{spelled}: {e}"))?;
            let listed = proj::parse(&text, file.parent().unwrap_or(dir));

            for data in &listed.data {
                out.data.push(found(data)?);
            }

            for (source, spelled) in listed.sources.iter().zip(listed.spelled) {
                out.sources.push(found(source)?);
                out.spelled.push(spelled);
            }
        } else if is(DATA) {
            out.data.push(file);
        } else {
            out.sources.push(file);
            out.spelled.push(spelled.clone());
        }
    }

    Ok(out)
}

/// `path` on disk, in any case.
fn found(path: &Path) -> Result<PathBuf, String> {
    regent_core::content::on_disk(path).ok_or_else(|| format!("{} is not on disk", path.display()))
}

/// `project`'s cache key: [`BUILD`], then each file's spelling, path and
/// bytes, each part's length before it.
///
/// # Errors
///
/// A file that will not read.
fn key(project: &proj::Project) -> Result<String, String> {
    let files = project.data.iter().chain(&project.sources);
    let mut parts: Vec<Vec<u8>> = vec![BUILD.as_bytes().to_vec()];

    parts.extend(project.spelled.iter().map(|s| s.as_bytes().to_vec()));

    for file in files {
        let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;

        parts.push(file.display().to_string().into_bytes());
        parts.push(bytes);
    }

    let framed: Vec<Vec<u8>> = parts
        .into_iter()
        .flat_map(|p| [(p.len() as u64).to_le_bytes().to_vec(), p])
        .collect();

    Ok(super::store::hash(framed.iter().map(Vec::as_slice)))
}

/// Every image in `dir` but `kept` removed: an older source's.
fn tidy(dir: &Path, kept: &[&Path]) {
    let stale = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == IMAGE) && !kept.contains(&p.as_path()));

    for path in stale {
        drop(std::fs::remove_file(path));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use regent_core::content::mods;

    /// The fixture's GUID: no real mod's.
    pub(crate) const GUID: &str = "{5EC0DE00-0000-4000-8000-000000000001}";

    /// Improved Guards (Workshop `664340597`), its v2 guardhouse's GPL.
    const ITEM: &str = "664340597";
    const SOURCES: [&str; 2] = ["GPL/Guardhouse_v2.gpl", "GPL/GuardHouse_v2.dat"];

    /// Each example mod (`examples/`) is found; one with sources compiles
    /// them, one with an inline patch carries it.
    #[test]
    fn the_example_mods_compile() {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let mut library = mods::scan(&[examples]);
        let cache = crate::testdata::scratch("compiled-examples");
        let built = resolve(&mut library, &cache);
        let guid = |g| Guid::parse(g).expect("guid");
        let peasants = guid("{4F78A1F4-E83F-4A84-A374-2F0DD097FED1}");
        let camp = guid("{56A39291-8971-4975-BB9A-C4F140535107}");
        let modded = library.find(peasants).expect("example found");

        assert!(
            matches!(built.get(&peasants), Some(Built::Compiled { .. })),
            "{built:?}"
        );
        assert!(Path::new(&modded.load.programs[0].1).exists());
        assert!(library.skipped.is_empty(), "{:?}", library.skipped);
        assert!(
            library
                .find(camp)
                .expect("example found")
                .inline_patch
                .is_some()
        );
        assert!(!built.contains_key(&camp));

        let pair = guid("{9C7D9A63-E6F7-4AC7-A7A5-A9CE7B05B3DC}");

        assert!(!built.contains_key(&pair));
        assert!(
            library
                .find(pair)
                .expect("example found")
                .inline_patch
                .is_some()
        );

        let veteran = guid("{3D01DE84-08A6-4C30-8459-EA2BF0530103}");

        assert!(
            matches!(built.get(&veteran), Some(Built::Compiled { .. })),
            "{built:?}"
        );
        assert!(
            library
                .find(veteran)
                .expect("example found")
                .inline_patch
                .is_some()
        );
    }

    /// A mod folder in a scratch folder of its own: the guardhouse's
    /// sources, a manifest naming a target not there; and a cache. The
    /// scratch folder is kept while the others are used.
    pub(crate) fn fixture(name: &str) -> (crate::testdata::Scratch, PathBuf, PathBuf) {
        let dir = crate::testdata::scratch(&format!("compiled-{name}"));
        let item = crate::files::env::path(crate::files::env::WORKSHOP)
            .expect("MAJESTY_WORKSHOP")
            .join(ITEM);
        let mods = dir.join("mods");

        std::fs::create_dir_all(mods.join("GPL")).expect("made");

        for source in SOURCES {
            std::fs::copy(item.join(source), mods.join(source)).expect("copied");
        }

        let manifest = format!(
            "<Majesty><Mod id=\"{GUID}\"><Name>Sourced</Name><DataConfiguration>\
             <Dataset base=\"Any\"><Load><GPL><Target>Data/GuardHouse_v2.bcd</Target>\
             <Source>{}</Source><Source>{}</Source></GPL></Load></Dataset>\
             </DataConfiguration></Mod></Majesty>",
            SOURCES[0], SOURCES[1]
        );

        std::fs::write(mods.join("Sourced.mmxml"), manifest).expect("written");
        let cache = dir.join("cache");

        (dir, mods, cache)
    }

    /// The fixture's mods scanned and compiled under `cache`.
    pub(crate) fn scanned(mods: &Path, cache: &Path) -> (Library, Builds) {
        let mut library = mods::scan(&[mods.to_path_buf()]);
        let builds = resolve(&mut library, cache);

        (library, builds)
    }

    pub(crate) fn guid() -> Guid {
        Guid::parse(GUID).expect("a GUID")
    }

    /// The images' hash, else the refusal.
    pub(crate) fn image(builds: &Builds) -> Result<(String, Cached), String> {
        match builds.get(&guid()).expect("built") {
            Built::Compiled { image, cached } => Ok((image.clone(), *cached)),
            Built::Refused(why) => Err(why.clone()),
        }
    }

    /// A second scan reads the cache; the image is the same.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_cached_compile_is_reused() {
        let (_dir, mods, cache) = fixture("reused");
        let (first, _) = image(&scanned(&mods, &cache).1).expect("compiled");
        let (again, cached) = image(&scanned(&mods, &cache).1).expect("compiled");

        assert_eq!(again, first);
        assert_eq!(cached, Cached::Reused);
    }

    /// An edited source compiles again, to other bytes; the old image
    /// is removed.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_source_edit_recompiles() {
        let (_dir, mods, cache) = fixture("edited");
        let (first, _) = image(&scanned(&mods, &cache).1).expect("compiled");
        let data = mods.join(SOURCES[1]);
        let text = std::fs::read_to_string(&data).expect("read");

        std::fs::write(&data, text.replacen("False", "True", 1)).expect("written");

        let (again, cached) = image(&scanned(&mods, &cache).1).expect("compiled");
        let images = std::fs::read_dir(cache.join(format!("{:032X}", guid().0)))
            .expect("the mod's cache")
            .count();

        assert_ne!(again, first);
        assert_eq!(cached, Cached::Fresh);
        assert_eq!(images, 1);
    }

    /// A source that will not parse refuses the mod, the file and line
    /// said; its program list is left as listed.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_compile_error_refuses_the_mod() {
        let (_dir, mods, cache) = fixture("broken");

        std::fs::write(mods.join(SOURCES[0]), "\n\nfunction (").expect("written");

        let (library, builds) = scanned(&mods, &cache);
        let why = image(&builds).expect_err("refused");

        assert!(why.starts_with("GPL/Guardhouse_v2.gpl:3:"), "{why}");
        assert_eq!(
            library.find(guid()).expect("listed").load.programs[0].1,
            "Data/GuardHouse_v2.bcd"
        );
    }

    /// The source files an image records, by file name, lower case: the
    /// author's spelling, often an absolute path on their machine.
    fn recorded(image: &[u8]) -> BTreeMap<String, Vec<String>> {
        let program = regent_core::data::bcd::Container::parse(image).expect("a program");
        let sections = program.sections().expect("three sections");
        let walked = regent_core::gpl::bcd::walk(sections.symbols.body).expect("walks");
        let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for text in walked
            .iter()
            .filter_map(|d| d.records.first())
            .map(|r| r.text)
        {
            let named = out.entry(file_name(text)).or_default();

            if !named.iter().any(|t| t == text) {
                named.push(text.to_owned());
            }
        }

        out
    }

    /// `path`'s last part, `\` or `/` between parts, lower case.
    fn file_name(path: &str) -> String {
        path.rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase()
    }

    /// Every Workshop mod shipping both a `<GPL>`'s sources and its
    /// `.bcd`, compiled from the manifest's list: byte for byte, once the
    /// source paths are spelled as the image records them (debug text
    /// only). Improved Guards (`664340597`) matches unaltered: its
    /// `MakeGPL.ps1` hands `Gplbcc.exe` the manifest's spellings.
    ///
    /// The exceptions are not the compiler's:
    /// - `1265189890` `Veteran_City_Guard`: the manifest omits the `.dat`
    ///   its `.gplproj` lists.
    /// - `1448403412`: `Snake_Pit_Spawn`'s one mark, as in the SDK's
    ///   `MX_Data.bcd`.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn workshop_sources_compile_to_their_bcd() {
        let workshop =
            crate::files::env::path(crate::files::env::WORKSHOP).expect("MAJESTY_WORKSHOP");
        let library = mods::scan(std::slice::from_ref(&workshop));
        let mut matched = 0;
        let mut differed = Vec::new();

        for m in &library.mods {
            for g in &m.load.sourced {
                let Some(target) = g
                    .target
                    .as_ref()
                    .and_then(|(_, t)| regent_core::content::on_disk(&under(&m.dir, t)))
                else {
                    continue;
                };
                let theirs = std::fs::read(&target).expect("shipped");
                let spelled = recorded(&theirs);
                let mut project = project(&m.dir, &g.sources).expect("a project");

                // Spelled as recorded, where one file has the name.
                for s in &mut project.spelled {
                    if let Some([one]) = spelled.get(&file_name(s)).map(Vec::as_slice) {
                        s.clone_from(one);
                    }
                }

                let shown = target
                    .strip_prefix(&workshop)
                    .expect("in the Workshop")
                    .display()
                    .to_string();

                match emit::compile_listed(&project, &m.manifest) {
                    Ok(ours) if ours == theirs => matched += 1,
                    Ok(_) => differed.push(shown),
                    Err(_) => differed.push(format!("{shown}: refused")),
                }
            }
        }

        assert!(matched >= 14, "{matched} matched");
        assert_eq!(
            differed,
            [
                "1265189890/Data/Veteran_City_Guard.bcd",
                "1448403412/GPL/GPLinfo.bcd",
            ]
        );
    }
}
