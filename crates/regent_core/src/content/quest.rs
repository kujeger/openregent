//! The quests an install offers: one `.mqxml` each under `Quests` and
//! `QuestsMX`.
//!
//! ```text
//! <Majesty>
//!   <Quest id="{36305551-0000-0000-0000-000000000000}">   key: QU06 (key_of)
//!     <Name>DAY_OF_RECKONING</Name>                        the quest's GPL function
//!     <DisplayName lang="en_US">…</DisplayName>            only where STRT QUES lacks it
//!     <Description lang="en_US"><Short>…</Short><Long>…</Long></Description>
//!     <Win lang="en_US">…</Win> <Loss lang="en_US">…</Loss>  the end panel's message; none ships
//!     <DataConfiguration>
//!       <Dataset base="Majesty">                           Majesty or MajestyExpansion
//!         <Load><CAM>…</CAM><GPL>…</GPL><Template>day_reckoning.q</Template></Load>
//! ```
//!
//! **Binary**: HD finds them by `*.mqxml` (`"*.mqxml"`); a quest's record
//! name is its GPL function, its title `STRT
//! QUES` by key, else `DisplayName`. **Corpus**: the
//! key is the code a quest save's `SVGD` writes (`QU062`), and `MD01`,
//! `XQD1` are functions of `MDL1.bcd` and `XQD1_Bytecode.bcd`.
//!
//! Order: `QDEP`'s, the quest list's dependency table (game globals ctor loads
//! it). **Inferred**: the quest menu's own order is not read.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Error;
use crate::content::dataset::{self, Listed, Lists, Sourced};
use crate::data::deps::Difficulty;
use crate::data::fourcc::FourCc;
use crate::data::guid::Guid;
use crate::data::text::Text;

/// The two directories quest definitions live in, base game first.
pub const DIRS: [&str; 2] = ["Quests", "QuestsMX"];

/// The definition files' extension.
const EXTENSION: &str = "mqxml";

/// Line breaks in a description.
const NEWLINE: &str = "[newline]";

/// The base dataset's name, as `<Dataset base>` writes it.
pub const BASE: &str = "Majesty";

/// The expansion dataset's name.
pub const EXPANSION: &str = "MajestyExpansion";

/// The language a quest's own text is read in (`"en_US"`).
const LANGUAGE: &str = "en_US";

/// `GMTX` 81, a quest with no name.
pub const NO_NAME: usize = 81;

/// The two freestyle quests: the custom game, not a quest the list offers.
const FREESTYLE: [FourCc; 2] = [FourCc::new(b"FRE0"), FourCc::new(b"FRE1")];

/// One quest definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    /// The engine's code for it, from the id ([`key_of`]): `QU06`.
    pub key: FourCc,
    /// `<Name>`: the GPL function the setup runs.
    pub name: String,
    /// `<DisplayName>`, where given.
    pub display: Option<String>,
    /// `<Difficulty>`: Easy, Medium, Hard or Master in any case, else
    /// Easy. What quest difficulty gives a quest
    /// the quest map has no slot for.
    pub difficulty: Difficulty,
    /// `<Short>` and `<Long>`, line breaks made `\n`.
    pub short: String,
    pub long: String,
    /// `<Template>`: the `.q`, beside the definition.
    pub template: String,
    /// `<Dataset base>`.
    pub base: String,
    /// The definition's own `<CAM>` entries, `$(Root)` kept.
    pub archives: Vec<(Option<String>, String)>,
    /// Its own `<GPL>` entries.
    pub programs: Vec<(Option<String>, String)>,
    /// GPL sources from a source-only quest manifest.
    pub sourced: Vec<Sourced<Listed>>,
    /// Its own `<Descriptions>` and `<Strings>`: XML ([`crate::data::desc::xml`],
    /// [`crate::data::text::Strings`]).
    pub descriptions: Vec<Listed>,
    pub strings: Vec<Listed>,
    /// `<Win>`, `<Loss>`: its own end messages.
    pub messages: Messages,
    /// The whole id, `<Quest id>`.
    pub id: Guid,
    /// The directory it was found in.
    pub dir: PathBuf,
}

impl Quest {
    /// Read one definition; `None` without an id, name or template.
    #[must_use]
    pub fn parse(text: &str, dir: &Path) -> Option<Quest> {
        let whole = text.split("id=\"").nth(1)?.split('"').next()?;
        let id = Guid::parse(whole)?;
        let set = dataset::parse(text);
        let lists = Lists::of(&set.load);

        Some(Quest {
            key: key_of(id),
            name: element(text, "Name")?.to_owned(),
            display: element(text, "DisplayName").map(str::to_owned),
            difficulty: difficulty(element(text, "Difficulty")),
            short: described(text, "Short"),
            long: described(text, "Long"),
            template: element(text, "Template")?.to_owned(),
            base: set.base.unwrap_or(BASE).to_owned(),
            archives: lists.archives,
            programs: lists.programs,
            sourced: Vec::new(),
            descriptions: lists.descriptions,
            strings: lists.strings,
            messages: Messages {
                win: localized(text, "Win"),
                loss: localized(text, "Loss"),
            },
            id,
            dir: dir.to_path_buf(),
        })
    }

    /// Read a source-only `quest.toml`.
    ///
    /// # Errors
    ///
    /// If its syntax, ID, dataset, or source paths are invalid.
    pub fn parse_toml(text: &str, manifest: &Path) -> Result<Quest, Error> {
        let source: SourceQuest = toml::from_str(text)?;
        if source.version != 1 {
            return Err(Error::Version {
                what: "quest",
                version: source.version,
            });
        }
        let id = Guid::parse(&source.id).ok_or_else(|| Error::Id {
            what: "quest",
            id: source.id.clone(),
        })?;
        if source.base != BASE && source.base != EXPANSION {
            return Err(Error::Dataset(source.base));
        }
        crate::content::mods::source_path(&source.template, &["q"])?;
        let dir = manifest.parent().unwrap_or(Path::new(""));
        let mut groups = Vec::new();

        for script in source.scripts {
            if script.name.is_empty() || script.sources.is_empty() {
                return Err(Error::Script);
            }
            let sources = script
                .sources
                .into_iter()
                .map(|path| {
                    crate::content::mods::source_path(&path, &["gpl", "dat", "gplproj"])?;
                    Ok((None, path))
                })
                .collect::<Result<Vec<_>, Error>>()?;
            groups.push(Sourced {
                at: groups.len(),
                target: None,
                sources,
            });
        }
        let descriptions = source
            .load
            .descriptions
            .into_iter()
            .map(|path| {
                crate::content::mods::source_path(&path, &["xml"])?;
                Ok((None, path))
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let strings = source
            .load
            .strings
            .into_iter()
            .map(|path| {
                crate::content::mods::source_path(&path, &["xml"])?;
                Ok((None, path))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(Quest {
            key: key_of(id),
            name: source.name,
            display: source.display_name,
            difficulty: difficulty(source.difficulty.as_deref()),
            short: source.description.unwrap_or_default(),
            long: source.details.unwrap_or_default(),
            template: source.template,
            base: source.base,
            archives: Vec::new(),
            programs: Vec::new(),
            sourced: groups,
            descriptions,
            strings,
            messages: Messages::default(),
            id,
            dir: dir.to_path_buf(),
        })
    }

    /// Whether it lists XML descriptions or strings.
    #[must_use]
    pub const fn has_xml(&self) -> bool {
        !self.descriptions.is_empty() || !self.strings.is_empty()
    }

    /// Whether it loads the expansion's dataset.
    #[must_use]
    pub fn expansion(&self) -> bool {
        self.base == EXPANSION
    }

    /// Its title: `STRT QUES` by key, else its
    /// `DisplayName`, else [`NO_NAME`], else its function's name.
    #[must_use]
    pub fn title(&self, text: &Text<'_>) -> String {
        text.quest(self.key)
            .map(|t| t.trim_end().to_owned())
            .or_else(|| self.display.clone())
            .or_else(|| text.gmtx(NO_NAME).map(Cow::into_owned))
            .unwrap_or_else(|| self.name.clone())
    }

    /// Its `.q`, in any case.
    #[must_use]
    pub fn template_path(&self) -> Option<PathBuf> {
        crate::content::on_disk(&self.dir.join(&self.template))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceQuest {
    version: u32,
    id: String,
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    details: Option<String>,
    difficulty: Option<String>,
    base: String,
    template: String,
    #[serde(default)]
    load: SourceLoad,
    #[serde(default, rename = "script")]
    scripts: Vec<SourceScript>,
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

/// `<Difficulty>`'s word as difficulty of name reads it: case ignored,
/// anything else or none Easy.
fn difficulty(word: Option<&str>) -> Difficulty {
    let named = |name: &str| word.is_some_and(|w| w.eq_ignore_ascii_case(name));

    if named("Medium") {
        Difficulty::Medium
    } else if named("Hard") {
        Difficulty::Hard
    } else if named("Master") {
        Difficulty::Master
    } else {
        Difficulty::Easy
    }
}

/// A quest's key from its id:
/// the first word, little-endian, where it is not zero and the other
/// twelve bytes are; else CRC-32 of [`Guid::bytes`]
/// ([`crate::data::crc::crc`]).
///
/// ```text
///  {36305551-0000-0000-0000-000000000000}   QU06, a shipped quest
///  {31305551-0054-0032-0000-000000000000}   0xea29f48e, a mod's copy of QU01
/// ```
#[must_use]
pub fn key_of(id: Guid) -> FourCc {
    let bytes = id.bytes();

    if by_first_word(id) {
        FourCc([bytes[0], bytes[1], bytes[2], bytes[3]])
    } else {
        FourCc(crate::data::crc::crc(&bytes).to_le_bytes())
    }
}

/// Whether [`key_of`] takes `id`'s first word: it is not zero, and the
/// other twelve bytes are.
#[must_use]
pub fn by_first_word(id: Guid) -> bool {
    let bytes = id.bytes();
    let (first, rest) = bytes.split_at(4);

    first.iter().any(|&b| b != 0) && rest.iter().all(|&b| b == 0)
}

/// Every definition under `install`'s [`DIRS`], the freestyle ones
/// dropped, in `order` (keys; `QDEP`'s), the rest after by key.
#[must_use]
pub fn offered(install: &Path, order: &[FourCc]) -> Vec<Quest> {
    let mut all: Vec<Quest> = every(install)
        .filter(|q| !FREESTYLE.contains(&q.key))
        .collect();
    let rank = |q: &Quest| {
        order
            .iter()
            .position(|k| *k == q.key)
            .unwrap_or(order.len())
    };

    all.sort_by_key(|q| (rank(q), q.key));
    all
}

/// The freestyle definitions under `install`'s [`DIRS`]: `FRE0`, the
/// base game's, and `FRE1`, the expansion's.
#[must_use]
pub fn freestyle(install: &Path) -> Vec<Quest> {
    every(install)
        .filter(|q| FREESTYLE.contains(&q.key))
        .collect()
}

/// Every definition under `install`'s [`DIRS`].
fn every(install: &Path) -> impl Iterator<Item = Quest> {
    DIRS.iter()
        .filter_map(|dir| crate::content::on_disk(&install.join(dir)))
        .flat_map(|dir| definitions(&dir))
}

/// The definitions in one directory.
fn definitions(dir: &Path) -> Vec<Quest> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
        })
        .collect();

    paths.sort();
    paths
        .iter()
        .filter_map(|p| {
            let text = std::fs::read(p).ok()?;

            Quest::parse(&String::from_utf8_lossy(&text), dir)
        })
        .collect()
}

/// The text of the first `<name …>…</name>`, trimmed; `None` if empty.
pub(crate) fn element<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let at = text
        .match_indices(&open)
        .map(|(i, _)| i)
        .find(|i| text[i + open.len()..].starts_with(['>', ' ']))?;
    let body = &text[at..];
    let start = body.find('>')? + 1;
    let end = body.find(&close)?;
    let inner = body.get(start..end)?.trim();

    (!inner.is_empty()).then_some(inner)
}

/// A quest's own end messages, `<Win>` and `<Loss>`:
/// asked before the
/// strings' `IDTXT_QUEST_WIN_MESSAGE` and `IDTXT_QUEST_LOSS_MESSAGE`.
/// **Corpus**: no install or Workshop quest has one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Messages {
    pub win: Option<String>,
    pub loss: Option<String>,
}

/// The text of the first `<name lang="en_US">`, [`text::unescaped`]:
/// one without `lang` is passed over.
/// **Inferred**: the language chosen is `en_US`, so its fallback
/// (`"en_US"`) is the same.
///
/// [`text::unescaped`]: crate::data::text::unescaped
fn localized(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let lang = format!("lang=\"{LANGUAGE}\"");

    text.match_indices(&open).find_map(|(at, _)| {
        let body = &text[at..];
        let head = &body[..body.find('>')?];
        let inner = body.get(head.len() + 1..body.find(&close)?)?;

        (body[open.len()..].starts_with(' ') && head.contains(&lang))
            .then(|| crate::data::text::unescaped(inner.trim()))
    })
}

/// A description element, `[newline]` made a line break.
pub(crate) fn described(text: &str, name: &str) -> String {
    element(text, name).map_or_else(String::new, |t| t.replace(NEWLINE, "\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KROLM: &str = r#"<Majesty>
	<Quest id="{3130444D-0000-0000-0000-000000000000}">
		<Name>MD01</Name>
		<DisplayName lang="en_US">Wrath of Krolm</DisplayName>
		<Description lang="en_US">
			<Short>One.[newline]Two.</Short>
			<Long></Long>
		</Description>
		<DataConfiguration>
			<Dataset base="Majesty">
				<Load>
					<CAM>$(MajestyDataPath)/MDL1_action.cam</CAM>
					<GPL>$(MajestyDataPath)/MDL1.bcd</GPL>
					<Template>Krolm.q</Template>
				</Load>
			</Dataset>
		</DataConfiguration>
	</Quest>
</Majesty>"#;

    #[test]
    fn a_definition_names_its_key_function_template_and_files() {
        let quest = Quest::parse(KROLM, Path::new("Quests")).expect("a quest");

        assert_eq!(quest.key, FourCc::new(b"MD01"));
        assert_eq!(quest.name, "MD01");
        assert_eq!(quest.display.as_deref(), Some("Wrath of Krolm"));
        assert_eq!(quest.difficulty, Difficulty::Easy, "none given");
        assert_eq!(quest.short, "One.\nTwo.");
        assert_eq!(quest.long, "");
        assert_eq!(quest.template, "Krolm.q");
        assert!(!quest.expansion());
        assert_eq!(
            quest.programs,
            [(Some("MajestyDataPath".to_owned()), "MDL1.bcd".to_owned())]
        );
        assert_eq!(quest.archives.len(), 1);
        assert_eq!(quest.messages, Messages::default());
    }

    /// `<Win>` and `<Loss>` are read in English alone, unescaped; one
    /// without `lang` is passed over.
    #[test]
    fn a_quests_own_messages_need_a_language() {
        let own = KROLM.replace(
            "<Name>",
            "<Win>bare</Win><Win lang=\"de_DE\">Sieg</Win>\
             <Win lang=\"en_US\">Won.[newline]Home.</Win><Loss>bare</Loss><Name>",
        );
        let quest = Quest::parse(&own, Path::new("Quests")).expect("a quest");

        assert_eq!(quest.messages.win.as_deref(), Some("Won.\r\nHome."));
        assert_eq!(quest.messages.loss, None);
    }

    #[test]
    fn source_quest_rejects_compiled_files() {
        let source = r#"
version = 1
id = "{BE9E6DFC-10F1-4984-9C79-A2F8F8F8C4A5}"
name = "Example"
base = "MajestyExpansion"
template = "Example.q"
[[script]]
name = "example"
sources = ["Data/Example.bcd"]
"#;

        let error = Quest::parse_toml(source, Path::new("quest.toml"))
            .expect_err("compiled GPL is forbidden");
        assert!(matches!(error, Error::SourcePath(_)));
        assert!(error.to_string().contains("invalid source path"));
    }
}
