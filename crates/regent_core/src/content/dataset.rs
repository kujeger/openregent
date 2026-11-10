//! Which files the engine loads together.
//!
//! `Data/MajestyDatasetDefinitions.xml` and its expansion twin list a dataset's
//! archives, programs and constants, in order. Ids repeat across archives (18
//! `IMAG` ids), so lookups need this scope.
//!
//! ```text
//! <Majesty>
//!   <DataConfiguration name="MajestyExpansion">
//!     <Dataset base="Majesty">
//!       <Load>
//!         <CAM>mx_maindata.cam</CAM>
//!         <CAM>$(MajestyDataPath)/GPLDebuggerUI.cam</CAM>
//!         <GPL>$(MajestyExpansionBytecodeDataPath)/MX_Data.bcd</GPL>
//!         <Constants>mx_constants.rgs</Constants>
//!       </Load>
//!       <Unload>
//!         <Constants>$(MajestyDataPath)/constants.rgs</Constants>
//!       </Unload>
//!     </Dataset>
//!   </DataConfiguration>
//! </Majesty>
//! ```
//!
//! `base` composes: the expansion is the base list plus its own, giving
//! ambiguous ids a deterministic answer ([`compose`]). `Unload` subtracts:
//! the expansion drops the base `constants.rgs` (**Inferred** from
//! Dataset compose; no shipped file unloads more).
//!
//! [`parse`] keeps `$(Name)` prefixes; [`Install`] resolves them and
//! composes HD's two definitions, a quest's own lists after, then the
//! active mods' ([`crate::content::mods`]).
//!
//! A scanner, not an XML library, like `gpl::dat` and `data::qdd`: the file
//! has a dozen fixed element names, and [`view`](crate::view) has no
//! dependencies.

mod install;

use std::path::Path;

use crate::data::catalogue::Library;

pub use install::{
    BASE_DEFINITIONS, Bytecode, Datasets, EXPANSION_DEFINITIONS, Error, Files, IMAGE_SETS, Install,
};

/// A path as written, `$(Name)` root split off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathRef<'a> {
    /// `Name` from `$(Name)`. `None`: relative to the definition's directory.
    pub root: Option<&'a str>,
    pub path: &'a str,
}

impl<'a> PathRef<'a> {
    /// Split `$(Root)/path`.
    fn parse(text: &'a str) -> Self {
        let rooted = text
            .strip_prefix("$(")
            .and_then(|rest| rest.split_once(')'))
            .map(|(root, rest)| (Some(root), rest.trim_start_matches(['/', '\\'])));

        let (root, path) = rooted.unwrap_or((None, text));

        PathRef { root, path }
    }
}

/// One `<Load>` or `<Unload>` list, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSet<'a> {
    /// `.cam` archives. Order breaks ties between shared ids.
    pub archives: Vec<PathRef<'a>>,
    /// `.bcd` programs.
    pub programs: Vec<PathRef<'a>>,
    /// `.rgs` generator constants.
    pub constants: Vec<PathRef<'a>>,
    /// `<GPLSource>`: a directory of `.gpl`. HD only (ships the SDK).
    pub sources: Vec<PathRef<'a>>,
    /// `<Strings>`: XML interface text. HD only.
    pub strings: Vec<PathRef<'a>>,
    /// `<Descriptions>`: XML descriptors, a mod's or a quest's. HD only.
    pub descriptions: Vec<PathRef<'a>>,
    /// Each `<GPL>` that lists a `<Source>`. HD never compiles one;
    /// openregent does, where its target is missing.
    pub sourced: Vec<Sourced<PathRef<'a>>>,
}

/// One `<GPL><Target/><Source/>…</GPL>`: its target, if any, and its
/// sources, `.gpl`, `.dat` or `.gplproj`, in order.
///
/// ```text
///  <GPL>
///    <Target>Data/GuardHouse.bcd</Target>   programs[at]
///    <Source>GPL/GuardHouse.gpl</Source>
///    <Source>GPL/GuardHouse.dat</Source>
///  </GPL>
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sourced<P> {
    /// Its place in the list's programs: its target's, or where a
    /// compiled image goes in with none.
    pub at: usize,
    pub target: Option<P>,
    pub sources: Vec<P>,
}

impl<'a> FileSet<'a> {
    fn push(&mut self, element: &str, text: &'a str) -> bool {
        let list = match element {
            "CAM" => &mut self.archives,
            // `<GPL><Target>x.bcd</Target><Source>x.gpl</Source></GPL>`:
            // HD loads the target and never compiles the source, which
            // only the GPL debugger shows (dataset apply,
            // dataset apply).
            "GPL" | "Target" => &mut self.programs,
            "Constants" => &mut self.constants,
            "GPLSource" => &mut self.sources,
            "Strings" => &mut self.strings,
            "Descriptions" => &mut self.descriptions,
            // `<Template></Template>` is empty in both shipped files; a
            // quest's names its `.q`.
            "Template" | "Source" => return true,
            _ => return false,
        };

        list.push(PathRef::parse(text));
        true
    }

    /// A `<GPL>` opened: a group its `<Target>` and `<Source>`s join.
    fn open_gpl(&mut self) {
        self.sourced.push(Sourced {
            at: self.programs.len(),
            target: None,
            sources: Vec::new(),
        });
    }

    /// `element` noted in the open `<GPL>`, if a target or a source.
    fn note(&mut self, element: &str, text: &'a str) {
        if let Some(group) = self.sourced.last_mut() {
            match element {
                "Target" => group.target = Some(PathRef::parse(text)),
                "Source" => group.sources.push(PathRef::parse(text)),
                _ => {}
            }
        }
    }

    /// The open `<GPL>` closed: dropped if it lists no source.
    fn close_gpl(&mut self) {
        if self.sourced.last().is_some_and(|g| g.sources.is_empty()) {
            self.sourced.pop();
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.archives.is_empty()
            && self.programs.is_empty()
            && self.sourced.is_empty()
            && self.constants.is_empty()
            && self.sources.is_empty()
            && self.strings.is_empty()
            && self.descriptions.is_empty()
    }
}

/// One entry as written, owned: `$(Root)` and path.
pub type Listed = (Option<String>, String);

/// A [`FileSet`] owned: a mod's or a quest's lists, kept past the text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lists {
    pub archives: Vec<Listed>,
    pub programs: Vec<Listed>,
    pub constants: Vec<Listed>,
    pub descriptions: Vec<Listed>,
    pub strings: Vec<Listed>,
    /// [`FileSet::sourced`].
    pub sourced: Vec<Sourced<Listed>>,
}

impl Lists {
    #[must_use]
    pub fn of(set: &FileSet<'_>) -> Lists {
        let one = |p: &PathRef<'_>| (p.root.map(str::to_owned), p.path.to_owned());
        let owned = |list: &[PathRef<'_>]| list.iter().map(one).collect();

        Lists {
            archives: owned(&set.archives),
            programs: owned(&set.programs),
            constants: owned(&set.constants),
            descriptions: owned(&set.descriptions),
            strings: owned(&set.strings),
            sourced: set
                .sourced
                .iter()
                .map(|g| Sourced {
                    at: g.at,
                    target: g.target.as_ref().map(one),
                    sources: owned(&g.sources),
                })
                .collect(),
        }
    }

    /// Whether it lists XML descriptions or strings.
    #[must_use]
    pub const fn has_xml(&self) -> bool {
        !self.descriptions.is_empty() || !self.strings.is_empty()
    }
}

/// One dataset: what the engine loads and drops.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dataset<'a> {
    /// `<DataConfiguration name>`, e.g. `MajestyExpansion`.
    pub name: &'a str,
    /// `<Dataset base>`: the dataset this extends.
    pub base: Option<&'a str>,
    pub load: FileSet<'a>,
    pub unload: FileSet<'a>,
    /// Unrecognised elements with text. Empty for both shipped files.
    pub unknown: Vec<&'a str>,
}

impl Dataset<'_> {
    /// Read the archives this definition names, in its order: its own
    /// `<Load>` list alone, no `base` and no `<Unload>`. For a definition
    /// against another install's files. What the engine loads is
    /// [`Install::files`], read with [`Library::from_files`].
    ///
    /// A `$(Name)` root is looked up in `roots`; unrooted or unknown-root paths
    /// are relative to `dir`. Case-insensitive, as Windows paths are.
    #[must_use]
    pub fn library(&self, dir: &Path, roots: &[(&str, &Path)]) -> Library {
        let mut out = Library::default();

        for archive in &self.load.archives {
            let base = archive
                .root
                .and_then(|want| roots.iter().find(|(name, _)| *name == want))
                .map_or(dir, |(_, path)| *path);

            match super::on_disk(&base.join(archive.path)).and_then(|p| std::fs::read(p).ok()) {
                Some(bytes) if bytes.is_empty() => out.empty.push(archive.path.to_owned()),
                Some(bytes) => out.loaded.push((archive.path.to_owned(), bytes)),
                None => out.missing.push(archive.path.to_owned()),
            }
        }

        out
    }
}

/// Read a dataset definition.
///
/// Infallible, like [`crate::gpl::proj::parse`]: failure shows as an empty
/// `load`. Unrecognised elements go to [`Dataset::unknown`].
pub fn parse(src: &str) -> Dataset<'_> {
    let mut out = Dataset::default();
    let mut list = List::Load;
    // Inside a `<GPL>…</GPL>`: its `<Target>` and `<Source>`s join it.
    let mut grouped = false;
    let mut rest = src;

    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];

        let Some(close) = after.find('>') else {
            break;
        };

        let inside = &after[..close];
        let tail = &after[close + 1..];
        rest = tail;

        // Text runs to the next tag. Empty text (container or empty leaf) is
        // skipped below.
        let text = tail.split('<').next().unwrap_or_default().trim();
        let (element, attrs) = inside
            .split_once(|c: char| c.is_ascii_whitespace())
            .unwrap_or((inside, ""));
        let element = element.trim_end_matches('/');

        match element {
            "DataConfiguration" => out.name = attribute(attrs, "name").unwrap_or_default(),
            "Dataset" => out.base = attribute(attrs, "base"),
            "Load" => list = List::Load,
            "Unload" => list = List::Unload,
            "GPL" if text.is_empty() && !inside.ends_with('/') => {
                grouped = true;
                set_of(&mut out, list).open_gpl();
            }
            "/GPL" if grouped => {
                grouped = false;
                set_of(&mut out, list).close_gpl();
            }
            _ if element.starts_with('/') || element.starts_with('?') => {}
            // E.g. `<CAM></CAM>`, the whole unload archive list in both
            // shipped files.
            _ if text.is_empty() => {}
            _ => {
                let set = set_of(&mut out, list);

                if grouped {
                    set.note(element, text);
                }

                if !set.push(element, text) {
                    out.unknown.push(element);
                }
            }
        }
    }

    out
}

/// Which of a dataset's lists a tag belongs to.
#[derive(Clone, Copy)]
enum List {
    Load,
    Unload,
}

/// The list `list` picks of `out`.
const fn set_of<'d, 'a>(out: &'d mut Dataset<'a>, list: List) -> &'d mut FileSet<'a> {
    if matches!(list, List::Unload) {
        &mut out.unload
    } else {
        &mut out.load
    }
}

/// A dataset's list, composed as dataset compose does: the base's
/// list, then each later list's entries not already present.
#[must_use]
pub fn compose<T: PartialEq + Clone>(lists: &[&[T]]) -> Vec<T> {
    let mut out: Vec<T> = Vec::new();

    for item in lists.iter().flat_map(|list| list.iter()) {
        if !out.contains(item) {
            out.push(item.clone());
        }
    }

    out
}

/// HD's load order for a listed GPL list: tail first.
/// Its own inverse.
#[must_use]
pub fn load_order<T>(listed: Vec<T>) -> Vec<T> {
    listed.into_iter().rev().collect()
}

/// The `LoadGPL` entries of one group of a `datasets.xml`, in document order,
/// repeats dropped as gpl is loaded drops them. Not how HD loads:
/// Dataset xml load is dead code there.
///
/// ```text
/// <LoadData>
///   <MajXCommonData>
///     <LoadCAM>mx_action.cam</LoadCAM>
///     <LoadGPL>MX_Build.bcd</LoadGPL>
/// ```
///
/// Only `<LoadData>` is read; `<UnloadData>` lists what a switch drops.
/// Which groups a game walks is not read here.
#[must_use]
pub fn datasets_gpl<'a>(xml: &'a str, group: &str) -> Vec<&'a str> {
    let load = section(xml, "LoadData");
    let body = load.map_or("", |l| section(l, group).unwrap_or(""));
    let mut out: Vec<&str> = Vec::new();

    for piece in body.split("<LoadGPL>").skip(1) {
        let name = piece.split("</LoadGPL>").next().unwrap_or_default().trim();

        if !out.iter().any(|seen| seen.eq_ignore_ascii_case(name)) {
            out.push(name);
        }
    }

    out
}

/// The text inside the first `<tag ...>` element, up to its close.
fn section<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");

    let start = xml.find(&open)?;
    let inner = &xml[start..];
    let body = &inner[inner.find('>')? + 1..];

    body.find(&close).map(|end| &body[..end])
}

/// Value of `key="..."` in an attribute list.
fn attribute<'a>(attrs: &'a str, key: &str) -> Option<&'a str> {
    // A whole name: at the start or after a space, `=` next. `name` is not
    // inside `basename="..."`.
    let whole = |&(at, _): &(usize, &str)| {
        let before = attrs[..at].chars().next_back();
        let after = attrs[at + key.len()..].trim_start();

        before.is_none_or(|c| c.is_ascii_whitespace()) && after.starts_with('=')
    };
    let (at, _) = attrs.match_indices(key).find(whole)?;

    attrs[at + key.len()..]
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .strip_prefix('"')?
        .split('"')
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPANSION: &str = r#"<Majesty>
    <DataConfiguration name="MajestyExpansion">
        <Dataset base="Majesty">
            <Load>
                <CAM>mx_maindata.cam</CAM>
                <CAM>$(MajestyDataPath)/GPLDebuggerUI.cam</CAM>
                <GPL>$(MajestyExpansionBytecodeDataPath)/MX_Data.bcd</GPL>
                <Constants>mx_constants.rgs</Constants>
                <Template></Template>
            </Load>
            <Unload>
                <CAM></CAM>
                <Constants>$(MajestyDataPath)/constants.rgs</Constants>
            </Unload>
        </Dataset>
    </DataConfiguration>
</Majesty>"#;

    #[test]
    fn a_definition_reads_its_name_base_and_files() {
        let d = parse(EXPANSION);

        assert_eq!(d.name, "MajestyExpansion");
        assert_eq!(d.base, Some("Majesty"), "datasets compose");
        assert_eq!(d.load.archives.len(), 2);
        assert_eq!(d.load.programs.len(), 1);
        assert_eq!(d.unknown.len(), 0);
    }

    #[test]
    fn an_attribute_is_matched_by_its_whole_name() {
        // `name` must not match inside `basename`.
        assert_eq!(attribute(r#"basename="x" name="y""#, "name"), Some("y"));
        assert_eq!(attribute(r#"basename="x""#, "name"), None);
    }

    #[test]
    fn order_is_preserved() {
        // Load order breaks id ties, so it must survive parsing.
        let d = parse(EXPANSION);

        assert_eq!(d.load.archives[0].path, "mx_maindata.cam");
        assert_eq!(d.load.archives[1].path, "GPLDebuggerUI.cam");
    }

    #[test]
    fn a_variable_root_is_kept_apart_from_the_path() {
        let d = parse(EXPANSION);

        assert_eq!(d.load.archives[0].root, None);
        assert_eq!(
            d.load.archives[1],
            PathRef {
                root: Some("MajestyDataPath"),
                path: "GPLDebuggerUI.cam"
            }
        );
    }

    #[test]
    fn unloading_is_separate_from_loading() {
        // The expansion drops the base game's constants.
        let d = parse(EXPANSION);

        assert!(d.unload.archives.is_empty(), "<CAM></CAM> says nothing");
        assert_eq!(d.unload.constants.len(), 1);
        assert_eq!(d.unload.constants[0].path, "constants.rgs");
    }

    #[test]
    fn an_unrecognised_element_is_kept() {
        let d = parse("<Load><Surprise>x</Surprise></Load>");

        assert_eq!(d.unknown, ["Surprise"]);
        assert!(d.load.is_empty());
    }

    #[test]
    fn a_later_list_appends_only_what_is_new() {
        let base = ["Bytecode.bcd", "MX_Compatibility.bcd"];
        let expansion = ["MX_Build.bcd", "Bytecode.bcd", "MX_Task.bcd"];

        assert_eq!(
            compose(&[&base[..], &expansion[..]]),
            [
                "Bytecode.bcd",
                "MX_Compatibility.bcd",
                "MX_Build.bcd",
                "MX_Task.bcd"
            ]
        );
    }

    #[test]
    fn hd_loads_the_list_tail_first() {
        assert_eq!(load_order(vec!["base", "compat"]), ["compat", "base"]);
    }

    #[test]
    fn a_datasets_group_lists_its_programs_once() {
        let xml = "<XmlDataRoot><LoadData><Rel0 ReleaseClassification=\"x\">\
                   <LoadGPL>MX_Compatibility.bcd</LoadGPL><LoadCAM>a.cam</LoadCAM>\
                   <LoadGPL>Bytecode.bcd</LoadGPL><LoadGPL>bytecode.bcd</LoadGPL></Rel0>\
                   </LoadData><UnloadData><Rel0><LoadGPL>X.bcd</LoadGPL></Rel0>\
                   </UnloadData></XmlDataRoot>";

        assert_eq!(
            datasets_gpl(xml, "Rel0"),
            ["MX_Compatibility.bcd", "Bytecode.bcd"]
        );
        assert_eq!(datasets_gpl(xml, "Rel9").len(), 0);
    }

    #[test]
    fn a_gpl_target_is_a_program_and_its_source_is_not() {
        // The SDK's Adjust Guardhouse manifest.
        let d = parse(
            "<Load><GPL><Target>Data\\AdjustGuardhouse.bcd</Target>\
             <Source>GPL\\Guardhouse.dat</Source></GPL>\
             <Descriptions>XML\\a.xml</Descriptions></Load>",
        );

        assert_eq!(d.load.programs[0].path, "Data\\AdjustGuardhouse.bcd");
        assert_eq!(d.load.programs.len(), 1);
        assert_eq!(d.load.descriptions[0].path, "XML\\a.xml");
        assert_eq!(d.unknown.len(), 0);
    }

    #[test]
    fn a_gpls_sources_are_grouped_with_its_target() {
        let d = parse(
            "<Load><GPL>Data/a.bcd</GPL>\
             <GPL><Target>Data/b.bcd</Target><Source>GPL/b.gpl</Source>\
             <Source>GPL/b.dat</Source></GPL>\
             <GPL><Source>GPL/c.gplproj</Source></GPL>\
             <GPL><Target>Data/d.bcd</Target></GPL></Load>",
        );
        let lists = Lists::of(&d.load);
        let listed = |p: &str| (None, p.to_owned());

        assert_eq!(lists.programs.len(), 3);
        assert_eq!(
            lists.sourced,
            [
                Sourced {
                    at: 1,
                    target: Some(listed("Data/b.bcd")),
                    sources: vec![listed("GPL/b.gpl"), listed("GPL/b.dat")],
                },
                Sourced {
                    at: 2,
                    target: None,
                    sources: vec![listed("GPL/c.gplproj")],
                },
            ]
        );
    }

    #[test]
    fn nothing_useful_yields_an_empty_dataset() {
        // The failure signal, in place of an error.
        let d = parse("");

        assert!(d.load.is_empty());
        assert_eq!(d.name, "");
    }
}
