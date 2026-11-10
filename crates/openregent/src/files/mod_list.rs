//! The mod list: `[[mod]]` entries in their own file, in load order.
//! Mods can be many, and the settings file should stay short.
//!
//! ```text
//!  $XDG_CONFIG_HOME/openregent/mods.toml
//!
//!  [[mod]]
//!  id = "17BF53DE-722F-4683-8E63-6B93D1606DFF"   the GUID; braces and case free
//!  name = "Majesty Expanded"                     for a reader: rewritten on save
//!  description = "Short text"                    the same: <Short>, else <Long>
//!  enabled = true                                a new game loads it
//! ```
//!
//! Written by the mods page alone, the rest of the file kept. The order
//! is the load order of those enabled. On save each found mod is written,
//! its `name` its display name (else `<Name>`), its `description` its
//! manifest's `<Short>`, else `<Long>`; either left out when the manifest
//! has none. An entry no folder holds is not shown, and kept as written,
//! after the rest. An `enabled` that is no boolean reads false; an entry
//! with no `id` is dropped.
//!
//! Beside the list, one key turns mod loading off, the list kept:
//!
//! ```text
//!  mods_enabled = false     top level; absent or no boolean: true
//! ```
//!
//! A collection is a list saved apart, the same `[[mod]]` entries and
//! the same reader and writer, one file a list, named by its file:
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/mod-lists/expanded.toml
//! ```

use std::path::{Path, PathBuf};

use bevy::log::warn;
use regent_core::content::mods::{Library, Mod};
use regent_core::data::guid::Guid;
use toml::{Table, Value};

/// The file in [`crate::files::env::OWN`].
const FILE: &str = "mods.toml";

/// The list's key: `[[mod]]`.
const KEY: &str = "mod";

/// The toggle's key, at the top level: `[[mod]]` does not hold it.
const LOADING_KEY: &str = "mods_enabled";

/// The collections' folder in the data directory, beside the freestyle
/// presets and profiles: saved things, not settings.
pub const LISTS: &str = "mod-lists";

/// The extension of a collection's file.
const EXTENSION: &str = "toml";

/// The most characters of a collection's name.
const NAME_CHARS: usize = 64;

/// Whether a new game loads the list's enabled mods.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Loading {
    #[default]
    On,
    Off,
}

/// One entry as read: its GUID, where it parses, whether enabled, and
/// the table as written, kept for an entry no folder holds.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub guid: Option<Guid>,
    pub enabled: bool,
    written: Table,
}

impl Entry {
    /// An entry for `guid`, as the page makes one.
    #[must_use]
    pub fn of(guid: Guid, enabled: bool) -> Entry {
        Entry {
            guid: Some(guid),
            enabled,
            written: Table::new(),
        }
    }

    /// What the page calls it: its `name` as written, else its id.
    #[must_use]
    pub fn label(&self) -> String {
        self.written
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| self.guid.map(|g| g.to_string()))
            .unwrap_or_default()
    }
}

/// The list's file; `None` with no config directory.
#[must_use]
pub fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(crate::files::env::OWN).join(FILE))
}

/// The list in the file at `path`; none where the file or key is absent
/// or will not read.
#[must_use]
pub fn read_at(path: &Path) -> Vec<Entry> {
    let file = file_at(path);
    let listed = match file.get(KEY) {
        Some(Value::Array(list)) => list.clone(),
        Some(other) => {
            warn!("settings: {KEY} = {other} is not a [[{KEY}]] list; passed over");
            Vec::new()
        }
        None => Vec::new(),
    };

    listed
        .into_iter()
        .filter_map(|v| match v {
            Value::Table(t) => entry(t),
            other => {
                warn!("settings: [[{KEY}]] {other} is not a table; passed over");
                None
            }
        })
        .collect()
}

/// The file at `path` as a table; empty where absent or no TOML.
fn file_at(path: &Path) -> Table {
    let text = std::fs::read_to_string(path).unwrap_or_default();

    toml::from_str(&text).unwrap_or_default()
}

/// The toggle in the file at `path`: on where absent; a value no
/// boolean is warned of and on.
#[must_use]
pub fn loading_at(path: &Path) -> Loading {
    match file_at(path).get(LOADING_KEY) {
        Some(Value::Boolean(false)) => Loading::Off,
        Some(Value::Boolean(true)) | None => Loading::On,
        Some(other) => {
            warn!("settings: {LOADING_KEY} = {other} is no boolean; true used");
            Loading::On
        }
    }
}

/// The toggle in the list's file; on with no config directory.
#[must_use]
pub fn loading() -> Loading {
    path().map_or(Loading::On, |p| loading_at(&p))
}

/// One table as an entry; `None` without an `id`.
fn entry(written: Table) -> Option<Entry> {
    let guid = written.get("id").and_then(Value::as_str).map(Guid::parse);
    let enabled = written
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if guid.is_none() {
        warn!("settings: a [[{KEY}]] has no id; passed over");
    }

    guid.map(|guid| Entry {
        guid,
        enabled,
        written,
    })
}

/// The list in its file ([`path`]).
#[must_use]
pub fn read() -> Vec<Entry> {
    path().map_or_else(Vec::new, |p| read_at(&p))
}

/// The GUIDs a new game loads: those enabled, in order, as `--mods`
/// takes them.
#[must_use]
pub fn enabled(entries: &[Entry]) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.enabled)
        .filter_map(|e| e.guid)
        .map(|g| g.to_string())
        .collect()
}

/// The GUIDs a menu's game loads: those [`enabled`], none with
/// loading off.
#[must_use]
pub fn chosen(entries: &[Entry], loading: Loading) -> Vec<String> {
    match loading {
        Loading::On => enabled(entries),
        Loading::Off => Vec::new(),
    }
}

/// The page's rows over `library`: the file's found mods in its order,
/// then the found mods it lacks, disabled; and the entries no folder
/// holds, kept.
#[must_use]
pub fn rows(entries: &[Entry], library: &Library) -> (Vec<Entry>, Vec<Entry>) {
    let found = |e: &&Entry| e.guid.and_then(|g| library.find(g)).is_some();
    let (shown, kept): (Vec<Entry>, Vec<Entry>) = entries.iter().cloned().partition(|e| found(&e));
    let mut rows: Vec<Entry> = Vec::new();

    // A GUID listed twice: its first entry.
    for e in shown {
        if !rows.iter().any(|r| r.guid == e.guid) {
            rows.push(e);
        }
    }

    for m in &library.mods {
        if !rows.iter().any(|r| r.guid == Some(m.guid)) {
            rows.push(Entry::of(m.guid, false));
        }
    }

    (rows, kept)
}

/// `rows`, then `kept` as written, as the file's list, and `loading`
/// where given (a collection has none); the rest of the file at `path`
/// kept, the whole written at once (`store::replace`).
///
/// # Errors
///
/// A file that will not write.
pub fn write_at(
    path: &Path,
    (rows, kept): (&[Entry], &[Entry]),
    library: &Library,
    loading: Option<Loading>,
) -> Result<(), String> {
    let old = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml_edit::DocumentMut = old.parse().unwrap_or_default();
    let mut list = toml_edit::ArrayOfTables::new();

    for row in rows {
        if let Some(found) = row.guid.and_then(|g| library.find(g)) {
            list.push(table(found, row.enabled));
        }
    }

    for entry in kept {
        let text = toml::to_string(&entry.written).map_err(|e| e.to_string())?;
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| e.to_string())?;

        list.push(doc.as_table().clone());
    }

    doc.insert(KEY, toml_edit::Item::ArrayOfTables(list));

    if let Some(loading) = loading {
        doc.insert(LOADING_KEY, toml_edit::value(loading == Loading::On));
    }

    crate::files::store::make_parent(path)?;
    crate::files::store::replace(path, &doc.to_string())
}

/// Where the collections live: [`LISTS`] in the XDG data directory.
#[must_use]
pub fn lists_place() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN).join(LISTS))
}

/// The collections in `dir`, by name.
#[must_use]
pub fn lists(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = crate::files::store::with_extension(dir, EXTENSION)
        .iter()
        .filter_map(|p| Some(p.file_stem()?.to_string_lossy().into_owned()))
        .collect();

    names.sort();
    names
}

/// The name a typed `name` is kept by, safe as a file's: `None` with
/// nothing safe in it.
///
/// ```text
///  " expanded "   expanded
///  "a/b"          a_b
///  "///"          None
/// ```
#[must_use]
pub fn list_name(name: &str) -> Option<String> {
    let name: String = name.trim().chars().take(NAME_CHARS).collect();

    crate::files::store::stem(&name)
}

/// The file of the collection named `name` in `dir`.
#[must_use]
pub fn list_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name).with_extension(EXTENSION)
}

/// A found mod's entry: its id without braces, name and description
/// from its manifest where it has them, and `enabled`.
fn table(found: &Mod, enabled: bool) -> toml_edit::Table {
    let mut t = toml_edit::Table::new();
    let id = found.guid.to_string();
    let name = found.display.clone().or_else(|| found.name.clone());
    let description = [&found.short, &found.long]
        .into_iter()
        .find(|s| !s.is_empty())
        .cloned();

    t.insert("id", toml_edit::value(id.trim_matches(['{', '}'])));

    if let Some(name) = name {
        t.insert("name", toml_edit::value(name));
    }

    if let Some(description) = description {
        t.insert("description", toml_edit::value(description));
    }

    t.insert("enabled", toml_edit::value(enabled));
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    const GUARDHOUSE: &str = "D666FCBE-9906-4582-A6A0-DC96F812441E";
    const EXPANDED: &str = "{17bf53de-722f-4683-8e63-6b93d1606dff}";
    const UNKNOWN: &str = "00000000-0000-0000-0000-000000000001";

    /// A scratch file, `name` in a folder of its own: the folder, kept
    /// while the file is used, and the file's path.
    fn scratch(name: &str) -> (crate::testdata::Scratch, std::path::PathBuf) {
        let dir = crate::testdata::scratch("mod-list");
        let path = dir.join(name);

        (dir, path)
    }

    fn library() -> Library {
        crate::files::env::Install::from_env()
            .expect("MAJESTY_HD")
            .mods()
            .clone()
    }

    /// Read in order; braces and case free; an unknown id hidden and kept;
    /// name and description rewritten from the manifest; the rest of the
    /// file kept.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_mod_list_round_trips() {
        let (_dir, path) = scratch("round.toml");
        let text = format!(
            "[ui]\nnames = \"heroes\"\n\n\
             [[mod]]\nid = \"{EXPANDED}\"\nname = \"hand edited\"\nenabled = true\n\n\
             [[mod]]\nid = \"{UNKNOWN}\"\nname = \"gone\"\nenabled = true\n\n\
             [[mod]]\nid = \"{GUARDHOUSE}\"\nenabled = \"yes\"\n"
        );

        std::fs::write(&path, text).expect("written");

        let library = library();
        let entries = read_at(&path);

        assert_eq!(entries.len(), 3);
        assert_eq!(
            enabled(&entries),
            [
                "{17BF53DE-722F-4683-8E63-6B93D1606DFF}",
                "{00000000-0000-0000-0000-000000000001}"
            ]
        );

        let (rows, kept) = rows(&entries, &library);

        assert_eq!(rows[0].guid, Guid::parse(EXPANDED));
        assert_eq!(rows[1].guid, Guid::parse(GUARDHOUSE));
        assert!(!rows[1].enabled, "\"yes\" is no boolean");
        assert_eq!(rows.len(), library.mods.len(), "each found mod");
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].label(), "gone", "its name as written");
        assert_eq!(
            rows[1].label(),
            format!("{{{GUARDHOUSE}}}"),
            "no name: its id"
        );

        write_at(&path, (&rows, &kept), &library, None).expect("written");

        let again = read_at(&path);
        let text = std::fs::read_to_string(&path).expect("read");

        assert!(text.contains("names = \"heroes\""), "{text}");
        assert!(text.contains("name = \"Majesty Expanded\""), "{text}");
        assert!(!text.contains("hand edited"), "{text}");
        assert!(
            text.contains("id = \"17BF53DE-722F-4683-8E63-6B93D1606DFF\""),
            "{text}"
        );
        assert!(text.contains("name = \"gone\""), "kept as written: {text}");
        assert_eq!(again.len(), library.mods.len() + 1);
        assert_eq!(again[0].guid, Guid::parse(EXPANDED));
        assert!(again[0].enabled);
        assert_eq!(again.last().and_then(|e| e.guid), Guid::parse(UNKNOWN));
    }

    /// No file, no list.
    #[test]
    fn no_file_lists_nothing() {
        assert_eq!(read_at(&scratch("absent.toml").1).len(), 0);
    }

    /// Loading off: no mods for a game, the list kept; written at the
    /// top level and read back; absent or no boolean, on.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn loading_off_loads_nothing_and_keeps_the_list() {
        let (_dir, path) = scratch("loading.toml");
        let text =
            format!("[ui]\nnames = \"heroes\"\n\n[[mod]]\nid = \"{EXPANDED}\"\nenabled = true\n");

        std::fs::write(&path, &text).expect("written");
        assert_eq!(loading_at(&path), Loading::On);

        let library = library();
        let (rows, kept) = rows(&read_at(&path), &library);

        write_at(&path, (&rows, &kept), &library, Some(Loading::Off)).expect("written");

        let entries = read_at(&path);

        assert_eq!(loading_at(&path), Loading::Off);
        assert_eq!(chosen(&entries, Loading::Off).len(), 0);
        assert_eq!(chosen(&entries, Loading::On).len(), 1, "the list kept");

        std::fs::write(&path, format!("mods_enabled = 1\n{text}")).expect("written");
        assert_eq!(loading_at(&path), Loading::On);
        assert_eq!(loading_at(&scratch("absent.toml").1), Loading::On);
    }

    /// A typed name made safe as a file's.
    #[test]
    fn a_list_name_is_made_safe() {
        assert_eq!(list_name(" expanded ").as_deref(), Some("expanded"));
        assert_eq!(list_name("a/b").as_deref(), Some("a_b"));
        assert_eq!(list_name("///"), None);
    }

    /// The list is not in the settings file.
    #[test]
    fn the_mod_list_has_its_own_file() {
        let list = path().expect("a config directory");

        assert_eq!(list.file_name().and_then(|n| n.to_str()), Some("mods.toml"));
        assert_eq!(
            list.parent(),
            crate::files::config::path()
                .as_deref()
                .and_then(Path::parent)
        );
        assert_ne!(Some(list), crate::files::config::path());
    }
}
