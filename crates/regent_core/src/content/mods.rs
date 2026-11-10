//! Mods: `.mmxml` manifests found in folders, each `<Mod>` composed over
//! a game's dataset after its quest's own lists.
//!
//! ```text
//! <Majesty>
//!   <Mod id="{D666FCBE-9906-4582-A6A0-DC96F812441E}">     several a file
//!     <Name/> <DisplayName lang="en_US"/> <Description><Short/><Long/></Description>
//!     <DataConfiguration>
//!       <Dataset base="Any">                             Any, Majesty or MajestyExpansion
//!         <Load>                                         and <Unload>
//!           <CAM/> <Constants/> <Template/>
//!           <GPL>Data/x.bcd</GPL>  or  <GPL><Target>Data\x.bcd</Target><Source/></GPL>
//!           <Descriptions/> <Strings/>                   XML: desc::xml, text::Strings
//! ```
//!
//! **Binary** (Mods): HD scans its user `Mods`
//! folder, then each subscribed Workshop item, recursively for `*.mmxml`;
//! a `*.mqxml` there is a quest.
//! Mods compose starts from the quest's
//! dataset, then composes each active mod whose base is the quest's or
//! `Any`, in the player's order, by dataset compose's rule.
//!
//! ```text
//!  folders, in order ──> walk (sorted) ──> *.mmxml ──> Mod × n ──┐ first of a GUID kept
//!                                     └──> *.mqxml ──> Quest ────┴─> Library
//!  Library + the chosen GUIDs ──> dataset::Install::quest_with / files_with
//! ```
//!
//! A scanner, as [`dataset`] is.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Error;
use crate::content::dataset::{self, Datasets, Lists, Sourced};
use crate::content::flags;
use crate::content::quest::{self, Quest};
use crate::data::guid::Guid;

/// A manifest's extension.
pub const MANIFEST: &str = "mmxml";
/// The source-only manifest's file name.
pub const SOURCE_MANIFEST: &str = "mod.toml";
/// A source-only quest's file name.
pub const SOURCE_QUEST: &str = "quest.toml";

/// A quest definition's extension (`"*.mqxml"`, `*.mqxml`).
pub const QUEST: &str = "mqxml";

/// `<Dataset base>` of a mod for both datasets.
pub const ANY: &str = "Any";

/// Folders deeper than this under a mod folder are not walked: a guard
/// against a link loop. The deepest Workshop file is 5 down.
const MAX_DEPTH: usize = 16;

/// `<Dataset base>`: the dataset a mod is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Base {
    Any,
    Majesty,
    Expansion,
    /// Any other text, or none. **Inferred**: it fits no dataset, as no
    /// tag equals it; HD's dialog says "unknown".
    Other(String),
}

impl Base {
    fn of(text: Option<&str>) -> Base {
        match text {
            Some(ANY) => Base::Any,
            Some(quest::BASE) => Base::Majesty,
            Some(quest::EXPANSION) => Base::Expansion,
            other => Base::Other(other.unwrap_or_default().to_owned()),
        }
    }

    /// Whether a game on `sets` composes the mod.
    #[must_use]
    pub const fn fits(&self, sets: Datasets) -> bool {
        matches!(
            (self, sets),
            (Base::Any, _)
                | (Base::Majesty, Datasets::Base)
                | (Base::Expansion, Datasets::Expansion)
        )
    }

    /// As the manifest writes it.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Base::Any => ANY,
            Base::Majesty => quest::BASE,
            Base::Expansion => quest::EXPANSION,
            Base::Other(text) => text,
        }
    }
}

/// One `<Mod>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mod {
    pub guid: Guid,
    /// `<Name>`, where given.
    pub name: Option<String>,
    /// `<DisplayName>`, where given.
    pub display: Option<String>,
    /// `<Short>` and `<Long>`, line breaks made `\n`.
    pub short: String,
    pub long: String,
    pub base: Base,
    pub load: Lists,
    pub unload: Lists,
    /// A TOML manifest's unit patches and `.dat` member values, as one
    /// patch file ([`super::flags`]).
    pub inline_patch: Option<String>,
    /// The manifest's folder: an unrooted path is under it.
    pub dir: PathBuf,
    pub manifest: PathBuf,
    /// The item folder it came in ([`Library`]).
    pub package: PathBuf,
}

impl Mod {
    /// What a list shows: its display name, else its name, else its GUID.
    #[must_use]
    pub fn title(&self) -> String {
        self.display
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| self.guid.to_string())
    }

    /// Whether it lists XML descriptions or strings.
    #[must_use]
    pub const fn has_xml(&self) -> bool {
        self.load.has_xml()
    }
}

/// Every `<Mod>` of the manifest at `manifest`, its text `text`, in
/// order; one without a GUID is dropped. `package` is its item folder.
#[must_use]
pub fn parse(text: &str, manifest: &Path, package: &Path) -> Vec<Mod> {
    let dir = manifest.parent().unwrap_or(Path::new(""));

    bodies(text)
        .filter_map(|(attrs, body)| {
            let id = attrs.split("id=\"").nth(1)?.split('"').next()?;
            let set = dataset::parse(body);

            Some(Mod {
                guid: Guid::parse(id)?,
                name: quest::element(body, "Name").map(str::to_owned),
                display: quest::element(body, "DisplayName").map(str::to_owned),
                short: quest::described(body, "Short"),
                long: quest::described(body, "Long"),
                base: Base::of(set.base),
                load: Lists::of(&set.load),
                unload: Lists::of(&set.unload),
                inline_patch: None,
                dir: dir.to_path_buf(),
                manifest: manifest.to_path_buf(),
                package: package.to_path_buf(),
            })
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMod {
    version: u32,
    id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    details: Option<String>,
    base: String,
    #[serde(default)]
    load: SourceLoad,
    #[serde(default, rename = "script")]
    scripts: Vec<SourceScript>,
    #[serde(default, rename = "unit")]
    units: Vec<toml::Value>,
    #[serde(default, rename = "prototype")]
    prototypes: Vec<toml::Value>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceLoad {
    #[serde(default)]
    descriptions: Vec<String>,
    #[serde(default)]
    strings: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceScript {
    name: String,
    sources: Vec<String>,
}

/// One source-only `mod.toml` manifest.
///
/// # Errors
///
/// If its syntax, version, ID, or source paths are invalid.
pub fn parse_toml(text: &str, manifest: &Path, package: &Path) -> Result<Mod, Error> {
    let source: SourceMod = toml::from_str(text)?;
    if source.version != 1 {
        return Err(Error::Version {
            what: "mod",
            version: source.version,
        });
    }
    let guid = Guid::parse(&source.id).ok_or_else(|| Error::Id {
        what: "mod",
        id: source.id.clone(),
    })?;
    let base = Base::of(Some(&source.base));
    if matches!(base, Base::Other(_)) {
        return Err(Error::Dataset(source.base));
    }
    let dir = manifest.parent().unwrap_or(Path::new(""));
    let mut load = Lists::default();
    let mut names = BTreeSet::new();

    for path in source.load.descriptions {
        source_path(&path, &["xml"])?;
        load.descriptions.push((None, path));
    }
    for path in source.load.strings {
        source_path(&path, &["xml"])?;
        load.strings.push((None, path));
    }
    for script in source.scripts {
        if script.name.is_empty() || script.sources.is_empty() {
            return Err(Error::Script);
        }
        if !names.insert(script.name.clone()) {
            return Err(Error::DuplicateScript(script.name));
        }
        let sources = script
            .sources
            .into_iter()
            .map(|path| {
                source_path(&path, &["gpl", "dat", "gplproj"])?;
                Ok((None, path))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        load.sourced.push(Sourced {
            at: load.programs.len(),
            target: None,
            sources,
        });
    }
    let inline_patch = if source.units.is_empty() && source.prototypes.is_empty() {
        None
    } else {
        let mut patch = toml::Table::new();
        patch.insert(
            "version".to_owned(),
            toml::Value::Integer(i64::from(source.version)),
        );
        patch.insert("unit".to_owned(), toml::Value::Array(source.units));

        // Left out when empty: a unit-only patch's text, and so its
        // saves' hash, stays as it was.
        if !source.prototypes.is_empty() {
            patch.insert(
                "prototype".to_owned(),
                toml::Value::Array(source.prototypes),
            );
        }

        let text = toml::to_string(&patch)?;

        // A bad patch refuses the mod at the scan, named.
        flags::check(&[(manifest.to_path_buf(), text.clone())])?;
        Some(text)
    };

    Ok(Mod {
        guid,
        name: Some(source.name),
        display: source.display_name,
        short: source.description.unwrap_or_default(),
        long: source.details.unwrap_or_default(),
        base,
        load,
        unload: Lists::default(),
        inline_patch,
        dir: dir.to_path_buf(),
        manifest: manifest.to_path_buf(),
        package: package.to_path_buf(),
    })
}

pub(crate) fn source_path(path: &str, extensions: &[&str]) -> Result<(), Error> {
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    let relative = !parts.is_empty()
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && !part.contains(':')
                && !part.contains("$(")
        });
    let extension = Path::new(path).extension().and_then(|e| e.to_str());
    if relative && extension.is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x))) {
        Ok(())
    } else {
        Err(Error::SourcePath(path.to_owned()))
    }
}

/// Each `<Mod …>…</Mod>`: its attributes and body.
fn bodies(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.split("<Mod").skip(1).filter_map(|piece| {
        // `<Mod>` or `<Mod id=…>`, not `<Modding…>`.
        let piece = piece.strip_prefix(|c: char| c == '>' || c.is_ascii_whitespace())?;
        let (attrs, rest) = piece.split_once('>').unwrap_or(("", piece));
        let body = rest.split("</Mod>").next().unwrap_or(rest);

        Some((attrs, body))
    })
}

/// A quest found in a mod folder, and its item folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub quest: Quest,
    pub package: PathBuf,
}

/// The mods and quests in a list of folders.
///
/// An item folder (package) is a folder's direct child that holds the
/// file, or the folder itself for a file directly in it: a Workshop
/// item's folder, or one mod unpacked into a user folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Library {
    /// Each GUID once: the first found, folders in order.
    pub mods: Vec<Mod>,
    pub quests: Vec<Found>,
    /// What was passed over, and why: a GUID seen before.
    pub skipped: Vec<String>,
}

impl Library {
    /// The mod with `guid`.
    #[must_use]
    pub fn find(&self, guid: Guid) -> Option<&Mod> {
        self.mods.iter().find(|m| m.guid == guid)
    }
}

/// Every manifest and quest under `folders`, walked in order, each
/// folder's entries sorted by name. A GUID found again is skipped, the
/// first kept: **Inferred** from HD, which skips one seen from the same
/// source and scans its user folder before the
/// Workshop.
#[must_use]
pub fn scan(folders: &[PathBuf]) -> Library {
    let mut library = Library::default();

    for folder in folders {
        for path in walk(folder, 0) {
            let package = package(folder, &path);
            let text = std::fs::read(&path)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();

            let is_mod = extension(&path, MANIFEST) || source_manifest(&path);
            let found = if extension(&path, MANIFEST) {
                parse(&text, &path, &package)
            } else if source_manifest(&path) {
                match parse_toml(&text, &path, &package) {
                    Ok(m) => vec![m],
                    Err(why) => {
                        library.skipped.push(format!("{}: {why}", path.display()));
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };
            if is_mod {
                for found in found {
                    match library.find(found.guid) {
                        Some(first) => library.skipped.push(format!(
                            "{}: {} {}, already in {}",
                            path.display(),
                            found.title(),
                            found.guid,
                            first.manifest.display()
                        )),
                        None => library.mods.push(found),
                    }
                }
            } else if source_quest(&path) {
                match Quest::parse_toml(&text, &path) {
                    Ok(quest) => library.quests.push(Found { quest, package }),
                    Err(why) => library.skipped.push(format!("{}: {why}", path.display())),
                }
            } else if let Some(quest) = Quest::parse(&text, path.parent().unwrap_or(folder)) {
                library.quests.push(Found { quest, package });
            }
        }
    }

    library
}

/// The manifests and quests under `dir`, sorted, depth first.
fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();

    entries.sort();
    entries
        .into_iter()
        .flat_map(|path| {
            if path.is_dir() && depth < MAX_DEPTH {
                walk(&path, depth + 1)
            } else if extension(&path, MANIFEST)
                || extension(&path, QUEST)
                || source_manifest(&path)
                || source_quest(&path)
            {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn source_manifest(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(SOURCE_MANIFEST))
}

fn source_quest(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(SOURCE_QUEST))
}

/// Whether `path` ends in `.ext`, any case.
fn extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// `path`'s item folder under `folder`: its first part below it, unless
/// the file lies directly in `folder`.
fn package(folder: &Path, path: &Path) -> PathBuf {
    let below = path.strip_prefix(folder).ok();
    let first = below
        .filter(|b| b.components().count() > 1)
        .and_then(|b| b.components().next());

    first.map_or_else(|| folder.to_path_buf(), |c| folder.join(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_mod_lists_only_sources() {
        let path = Path::new("mods/guard/mod.toml");
        let text = r#"version = 1
id = "{D666FCBE-9906-4582-A6A0-DC96F812441E}"
name = "AdjustGuardHouse"
display_name = "Adjust Guardhouse"
base = "Any"

[load]
descriptions = ["Data/units.xml"]
strings = ["Data/text.xml"]

[[script]]
name = "guardhouse"
sources = ["GPL/Guardhouse.dat"]
"#;
        let m = parse_toml(text, path, Path::new("mods/guard")).expect("mod");

        assert_eq!(m.guid, Guid(0xD666_FCBE_9906_4582_A6A0_DC96_F812_441E));
        assert_eq!(m.load.sourced.len(), 1);
        assert_eq!(m.load.sourced[0].sources[0].1, "GPL/Guardhouse.dat");
        assert_eq!(m.load.descriptions[0].1, "Data/units.xml");
        assert_eq!(m.load.strings[0].1, "Data/text.xml");
        assert_eq!(m.load.archives.len(), 0);
        assert_eq!(m.load.programs.len(), 0);

        let compiled = text.replace("GPL/Guardhouse.dat", "Data/Guardhouse.bcd");
        assert!(parse_toml(&compiled, path, Path::new("mods/guard")).is_err());

        let inline = text.replace(
            "[[script]]",
            "[[unit]]\nid = 'ABe1'\nfield = 'building'\nadd = ['Heals']\n\n[[script]]",
        );
        let modded = parse_toml(&inline, path, Path::new("mods/guard")).expect("inline patch");
        let patch_text = modded.inline_patch.expect("patch");
        assert!(patch_text.contains("[[unit]]"), "{patch_text}");
        assert!(!patch_text.contains("display_name"));
    }

    /// Two mods in one file, as Majesty Expanded writes them.
    const TWO: &str = r#"<Majesty>
	<Mod id="{17BF53DE-722F-4683-8E63-6B93D1606DFF}">
		<DataConfiguration>
			<Dataset base="Any">
				<Load>
					<Descriptions>XML\M_Buildings_Mod.xml</Descriptions>
					<GPL>Data\MajestyExtras.bcd</GPL>
					<Strings>XML\BagTD_Text.xml</Strings>
				</Load>
			</Dataset>
		</DataConfiguration>
		<DisplayName lang="en_US">Majesty Expanded</DisplayName>
	</Mod>
	<Mod id="{47466302-B0EB-488C-BDFE-B91BCEAB1662}">
		<DataConfiguration>
			<Dataset base="MajestyExpansion">
				<Load>
					<GPL>
						<Target>Data\AI_solo_ally.bcd</Target>
					</GPL>
				</Load>
			</Dataset>
		</DataConfiguration>
		<Name>Ally</Name>
	</Mod>
</Majesty>"#;

    #[test]
    fn a_manifest_holds_each_mod_apart() {
        let mods = parse(TWO, Path::new("item/x.mmxml"), Path::new("item"));

        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].title(), "Majesty Expanded");
        assert_eq!(mods[0].base, Base::Any);
        assert_eq!(mods[0].load.programs.len(), 1);
        assert!(mods[0].has_xml());
        assert_eq!(mods[1].title(), "Ally");
        assert_eq!(mods[1].base, Base::Expansion);
        assert_eq!(
            mods[1].load.programs,
            [(None, "Data\\AI_solo_ally.bcd".to_owned())]
        );
        assert!(!mods[1].has_xml());
        assert_eq!(mods[1].dir, Path::new("item"));
    }

    #[test]
    fn a_base_fits_its_dataset_or_both() {
        assert!(Base::Any.fits(Datasets::Base));
        assert!(Base::Any.fits(Datasets::Expansion));
        assert!(Base::Majesty.fits(Datasets::Base));
        assert!(!Base::Majesty.fits(Datasets::Expansion));
        assert!(!Base::Expansion.fits(Datasets::Base));
        assert!(!Base::Other(String::new()).fits(Datasets::Base));
    }

    #[test]
    fn a_files_package_is_its_first_folder_below() {
        let folder = Path::new("w");

        assert_eq!(
            package(folder, Path::new("w/617/Data/a.mmxml")),
            folder.join("617")
        );
        assert_eq!(package(folder, Path::new("w/a.mmxml")), folder);
    }
}
