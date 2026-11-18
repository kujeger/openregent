//! Mods, quests and their XML, found and composed from hand-made folders
//! (`tests/fixtures/manifests`): a Workshop-shaped tree, `<item id>/...`,
//! over a tiny invented install. No game data, no retail or Workshop bytes
//! or text; every id, GUID and string is invented.
//!
//! ```text
//!  base/           an invented install: dataset definitions, placeholder files,
//!                  shipped.xml (its "shipped" records, made into an archive here)
//!  workshop/       items 4400001..4400006: mods, quests, XML, strings, a .qdd
//!  copies/         an item holding a GUID of workshop/'s
//! ```
//!
//! Each test holds a fact about mods and quests, on this tree.
//!
//! Corpora: none.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use regent_core::content::dataset::{Bytecode, Datasets, Files, Install};
use regent_core::content::mods::{self, Base, Library, Mod};
use regent_core::content::quest::{self, Quest};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::deps::Difficulty;
use regent_core::data::desc::{self, xml as dxml};
use regent_core::data::fourcc::{FourCc, string_id};
use regent_core::data::guid::Guid;
use regent_core::data::text::{self, Text};
use regent_core::data::{qdd, xml};
use regent_core::sim::world::defeat::End;
use regent_core::view::menu::end::verdict;
use regent_core::view::panel::portrait::details;

/// Plain Image: one image, no XML, any dataset.
const PLAIN: &str = "{5A1C0D11-7E22-4B30-9F41-0A6B3C5D7E01}";
const ECHO_HALL: &str = "{5A1C0D11-7E22-4B30-9F41-0A6B3C5D7E02}";
const DEEP_KEEP: &str = "{5A1C0D11-7E22-4B30-9F41-0A6B3C5D7E03}";
const OLD_WAYS: &str = "{5A1C0D11-7E22-4B30-9F41-0A6B3C5D7E04}";
const SECOND_PACK: &str = "{5A1C0D11-7E22-4B30-9F41-0A6B3C5D7E06}";

/// The shipped quest the copy is made of: only its first word set.
const SHIPPED_QUEST: &str = "{5A515831-0000-0000-0000-000000000000}";

/// The copy of it: more than its first word set.
const COPY_QUEST: &str = "{5A515831-0031-0042-0000-000000000000}";

fn tree() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/manifests")
}

fn guid(text: &str) -> Guid {
    Guid::parse(text).expect("a GUID")
}

fn name(path: &Path) -> String {
    path.file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned())
}

fn scanned() -> Library {
    mods::scan(&[tree().join("workshop")])
}

fn quest_named(library: &Library, name: &str) -> Quest {
    library
        .quests
        .iter()
        .find(|q| q.quest.name == name)
        .unwrap_or_else(|| panic!("quest {name}"))
        .quest
        .clone()
}

fn mod_of<'a>(library: &'a Library, id: &str) -> &'a Mod {
    library.find(guid(id)).expect("the mod")
}

/// A mod's files over the dataset it fits.
fn composed(install: &Install<'_>, m: &Mod) -> Files {
    let sets = if m.base.fits(Datasets::Base) {
        Datasets::Base
    } else {
        Datasets::Expansion
    };

    install.files_with(sets, &[m]).expect("its files")
}

/// The invented install's "shipped" records, as an archive: `shipped.xml`
/// made over nothing, as `regent_testgame` makes its units.
fn shipped() -> Vec<u8> {
    let text = std::fs::read_to_string(tree().join("base/Data/shipped.xml")).expect("shipped.xml");
    let tree = xml::parse(&text).expect("well-formed");
    let made = dxml::compile(
        &[&tree],
        &Catalogue::build(&[]).expect("empty"),
        &dxml::ImageSets::default(),
    );

    assert!(made.refused.is_empty(), "{:?}", made.refused);

    made.archive()
}

/// The shipped archive, then the XML `paths` made over it, loaded last.
fn archives_with(paths: &[PathBuf]) -> Vec<(String, Vec<u8>)> {
    let base = shipped();
    let catalogue = Catalogue::parse([("shipped.cam", base.as_slice())]).expect("a catalogue");
    let made = dxml::Sources::read(paths)
        .compile(&catalogue, &dxml::ImageSets::default())
        .archive();

    vec![
        ("shipped.cam".to_owned(), base),
        ("made.xml".to_owned(), made),
    ]
}

// ---------------------------------------------------------------------
// content::mods
// ---------------------------------------------------------------------

/// Six mods in three manifests, each GUID once; four quests in four
/// items. Three mods list no XML: Plain Image, Old Ways and First Pack.
/// One manifest holds three mods (`bundle.mmxml`).
#[test]
fn a_folder_holds_its_mods_and_quests() {
    let library = scanned();
    let mut plain: Vec<String> = library
        .mods
        .iter()
        .filter(|m| !m.has_xml())
        .map(Mod::title)
        .collect();
    let bundled = library
        .mods
        .iter()
        .filter(|m| name(&m.manifest) == "bundle.mmxml")
        .count();
    let mut guids: Vec<Guid> = library.mods.iter().map(|m| m.guid).collect();

    plain.sort();
    guids.sort();
    guids.dedup();

    assert_eq!(library.mods.len(), 6);
    assert_eq!(guids.len(), 6, "each GUID once");
    assert!(library.skipped.is_empty(), "{:?}", library.skipped);
    assert_eq!(library.quests.len(), 4);
    assert_eq!(bundled, 3, "several mods in one manifest");
    assert_eq!(plain, ["First Pack", "Old Ways", "Plain Image"]);
}

/// Each mod names its base: Deep Keep the expansion's, Old Ways the base
/// game's, the rest `Any`. A `<DisplayName>` after `<DataConfiguration>`
/// (Echo Hall) is read: elements come in any order.
#[test]
fn each_mod_names_a_base() {
    let library = scanned();
    let count = |base: Base| library.mods.iter().filter(|m| m.base == base).count();

    assert_eq!(count(Base::Any), 4);
    assert_eq!(count(Base::Expansion), 1);
    assert_eq!(count(Base::Majesty), 1);
    assert_eq!(mod_of(&library, ECHO_HALL).title(), "Echo Hall");
}

/// Plain Image is one image for either dataset: no archive, no XML.
#[test]
fn a_plain_image_is_one_image_for_any_dataset() {
    let library = scanned();
    let found = mod_of(&library, PLAIN);

    assert_eq!(found.title(), "Plain Image");
    assert_eq!(found.name.as_deref(), Some("PlainImage"));
    assert_eq!(found.base, Base::Any);
    assert_eq!(found.load.programs, [(None, "Data/plain.bcd".to_owned())]);
    assert_eq!(found.load.archives.len(), 0);
    assert!(!found.has_xml());
    assert_eq!(name(&found.package), "4400001");
}

/// A GUID found twice keeps the first: `copies/` scanned before
/// `workshop/` holds Plain Image's GUID, and its copy is kept, the other
/// skipped. **Inferred** (as `mods::scan`): HD's user folder comes first.
#[test]
fn a_guid_found_twice_keeps_the_first() {
    let first = tree().join("copies");
    let library = mods::scan(&[first.clone(), tree().join("workshop")]);
    let found = mod_of(&library, PLAIN);

    assert_eq!(found.name.as_deref(), Some("PlainImageCopy"));
    assert_eq!(found.package, first.join("4400099"));
    assert_eq!(library.mods.len(), 6);
    assert_eq!(library.skipped.len(), 1, "{:?}", library.skipped);
}

/// A mod's image goes last in the list, so first in load order: it owns
/// the names it shares with the base (`gpl::load`, **Binary** per
/// `docs/formats/mods.md`, "Load order"). The archives and chart are the
/// expansion's.
#[test]
fn a_mods_image_loads_first_over_the_expansion() {
    let library = scanned();
    let found = mod_of(&library, PLAIN);
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let stock = install.files(Datasets::Expansion).expect("stock");
    let modded = install
        .files_with(Datasets::Expansion, &[found])
        .expect("modded");

    assert_eq!(name(&modded.programs[0]), "plain.bcd");
    assert_eq!(modded.programs[1..], stock.programs[..]);
    assert_eq!(modded.archives, stock.archives);
    assert_eq!(modded.chart, stock.chart);
}

/// A mod for the other dataset is passed over: Deep Keep (expansion) in a
/// base game, Old Ways (base) in an expansion game.
#[test]
fn a_mod_for_the_other_dataset_is_passed_over() {
    let library = scanned();
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let deep = mod_of(&library, DEEP_KEEP);
    let old = mod_of(&library, OLD_WAYS);

    assert_eq!(
        install.files_with(Datasets::Base, &[deep]).ok(),
        install.files(Datasets::Base).ok()
    );
    assert_eq!(
        install.files_with(Datasets::Expansion, &[old]).ok(),
        install.files(Datasets::Expansion).ok()
    );
    assert_ne!(
        install.files_with(Datasets::Expansion, &[deep]).ok(),
        install.files(Datasets::Expansion).ok(),
        "its own dataset composes it"
    );
}

/// A quest lists `<GPL><Target>` with `\`: its image is found and loaded
/// first; the `<Source>` beside it is not loaded (HD never compiles it).
#[test]
fn a_quests_target_image_is_found() {
    let library = scanned();
    let big = quest_named(&library, "BIG_TRAIL");
    let base = tree().join("base");
    let files = Install::new(&base, Bytecode::Install)
        .quest_with(&big, &[])
        .expect("its files");

    assert_eq!(name(&files.programs[0]), "big.bcd");
    assert_eq!(files.programs.len(), 2, "its target and the base's");
    assert!(!big.has_xml());
}

/// A quest copy is keyed by its id's CRC, not by a code from its first
/// word. The rule (`docs/formats/mods.md`, "Quests"): an id with more than
/// its first word set is keyed by the CRC-32 of its 16 bytes, from all
/// ones, not inverted: `zlib.crc32` of the bytes, inverted. The bytes are
/// the GUID as Windows lays it out (`Guid::bytes`): the first three groups
/// little-endian, the rest as written. For this invented id:
///
/// ```text
/// {5A515831-0031-0042-0000-000000000000}
///   bytes  31 58 51 5a  31 00  42 00  00 00 00 00 00 00 00 00
///   python3 -c "import zlib; print(hex(zlib.crc32(bytes.fromhex(
///     '3158515a31004200' + '00' * 8)) ^ 0xffffffff))"
///   ==> 0x91369e27
/// ```
///
/// The shipped id's key is its first word, `1XQZ`.
#[test]
fn a_quests_copy_is_keyed_by_its_ids_crc() {
    let library = scanned();
    let copy = quest_named(&library, "XQZ_COPY");
    let found = library
        .quests
        .iter()
        .find(|q| q.quest.id == guid(COPY_QUEST))
        .expect("the copy");

    assert_eq!(copy.key, FourCc(0x9136_9e27_u32.to_le_bytes()));
    assert_ne!(copy.key, quest::key_of(guid(SHIPPED_QUEST)));
    assert_eq!(quest::key_of(guid(SHIPPED_QUEST)), FourCc::new(b"1XQZ"));
    assert!(quest::by_first_word(guid(SHIPPED_QUEST)));
    assert!(!quest::by_first_word(copy.id));
    assert_eq!(name(&found.package), "4400006");
}

/// `<Difficulty>Master</Difficulty>` is read; a quest naming none is Easy
/// (`docs/formats/mods.md`: Easy, Medium, Hard, Master, any case, else
/// Easy).
#[test]
fn a_quest_names_its_difficulty() {
    let library = scanned();

    assert_eq!(
        quest_named(&library, "LONE_PEAK").difficulty,
        Difficulty::Master
    );
    assert_eq!(
        quest_named(&library, "BIG_TRAIL").difficulty,
        Difficulty::Easy
    );
}

/// Only Lone Peak lists XML descriptions or strings.
#[test]
fn some_quests_list_xml() {
    let library = scanned();
    let mut xml: Vec<String> = library
        .quests
        .iter()
        .filter(|q| q.quest.has_xml())
        .map(|q| q.quest.name.clone())
        .collect();

    xml.sort();
    assert_eq!(xml, ["LONE_PEAK"]);
}

/// Every path a mod lists is on disk, `\` or `/` between its parts.
#[test]
fn every_mod_file_is_on_disk() {
    let library = scanned();
    let missing: Vec<PathBuf> = library
        .mods
        .iter()
        .flat_map(|m| {
            let lists = &m.load;

            lists
                .archives
                .iter()
                .chain(&lists.programs)
                .chain(&lists.descriptions)
                .chain(&lists.strings)
                .map(|(_, p)| {
                    p.split(['/', '\\'])
                        .fold(m.dir.clone(), |at, part| at.join(part))
                })
                .collect::<Vec<_>>()
        })
        .filter(|p| regent_core::content::on_disk(p).is_none())
        .collect();
    let listed: usize = library
        .mods
        .iter()
        .map(|m| m.load.programs.len() + m.load.descriptions.len() + m.load.strings.len())
        .sum();

    assert_eq!(listed, 7, "the manifests list seven paths");
    assert_eq!(missing, Vec::<PathBuf>::new());
}

/// The extra quests' list names a quest's item by its first mod (Trail
/// Run: First Pack, not Second Pack); an item with no mod, by its folder.
#[test]
fn a_quest_is_named_by_its_items_first_mod() {
    let library = scanned();
    let named: Vec<(String, String)> = library
        .quests
        .iter()
        .map(|q| {
            (
                q.quest.name.clone(),
                regent_core::view::menu::extras::source(&library, &q.package),
            )
        })
        .collect();

    assert_eq!(
        named,
        [
            ("BIG_TRAIL".to_owned(), "4400003".to_owned()),
            ("TRAIL_RUN".to_owned(), "First Pack".to_owned()),
            ("LONE_PEAK".to_owned(), "4400005".to_owned()),
            ("XQZ_COPY".to_owned(), "4400006".to_owned()),
        ]
    );
}

// ---------------------------------------------------------------------
// data::desc::xml
// ---------------------------------------------------------------------

/// Every description file a mod or quest lists, each once.
fn description_files(library: &Library) -> Vec<PathBuf> {
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let mut files: Vec<PathBuf> = library
        .mods
        .iter()
        .flat_map(|m| composed(&install, m).descriptions)
        .collect();

    for found in &library.quests {
        files.extend(install.quest(&found.quest).expect("its files").descriptions);
    }

    files.sort();
    files.dedup();
    files
}

/// Every description file parses, and each `<Description>` is read: eight
/// in three files, none refused; an id described twice is one record
/// (the later replaces, `docs/formats/mods.md`, "Descriptions"), so seven.
#[test]
fn every_description_reads() {
    let files = description_files(&scanned());
    let sources = dxml::Sources::read(&files);
    let (trees, unread) = sources.parsed();
    let descriptions: usize = trees.iter().map(|t| t.named("Description").count()).sum();
    let base = shipped();
    let catalogue = Catalogue::parse([("shipped.cam", base.as_slice())]).expect("a catalogue");
    let compiled = sources.compile(&catalogue, &dxml::ImageSets::default());

    assert_eq!(files.len(), 3, "description files");
    assert!(unread.is_empty(), "{unread:?}");
    assert_eq!(descriptions, 8, "descriptions");
    assert!(compiled.refused.is_empty(), "{:?}", compiled.refused);
    assert_eq!(compiled.records.len(), 7, "records made, one an id");
}

/// Each mod's records over the dataset it fits: a shipped id replaced
/// whole, the rest new. Echo Hall replaces two shipped records and adds
/// two; Deep Keep replaces one and adds one. What a replacement leaves
/// out is the constructor's, zero: Deep Keep's Pikeman names no
/// `SightRange`, the shipped one's was 6.
#[test]
fn a_mods_record_replaces_the_shipped_one() {
    let library = scanned();
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let shipped = shipped();
    let catalogue = Catalogue::parse([("shipped.cam", shipped.as_slice())]).expect("a catalogue");
    let mut replaced: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for m in &library.mods {
        let files = composed(&install, m);
        let compiled = dxml::Sources::read(&files.descriptions)
            .compile(&catalogue, &dxml::ImageSets::default());
        let on_shipped = compiled
            .records
            .iter()
            .filter(|r| catalogue.find(&r.set.tag(), r.id).is_some())
            .count();

        assert!(compiled.refused.is_empty(), "{}", m.title());
        replaced.insert(m.title(), (compiled.records.len(), on_shipped));
    }

    assert_eq!(replaced.len(), 6, "every mod composes");
    assert_eq!(replaced["Echo Hall"], (4, 2));
    assert_eq!(replaced["Deep Keep"], (2, 1));
    assert_eq!(replaced["Plain Image"], (0, 0));

    let deep = composed(&install, mod_of(&library, DEEP_KEEP));
    let archives = archives_with(&deep.descriptions);
    let catalogue = Catalogue::build(&archives).expect("a catalogue");
    let at = catalogue
        .find(b"DUNT", FourCc::new(b"ZQC2"))
        .expect("the pikeman");
    let record = catalogue.record(b"DUNT", at).expect("its record");
    let part = desc::Part::parse(record.data).expect("a tree");
    let unit = desc::Unit::parse(&part).expect("a unit");
    let character = unit.character().expect("a character");

    assert_eq!(at.archive, 1, "the made archive, loaded last, wins");
    assert_eq!(character.max_hp(), Some(45), "30 shipped");
    assert_eq!(character.sight_range(), Some(0), "6 shipped, left out");
}

/// Echo Hall replaces the shipped Hall; its `HelpID`, `ZQ_hEcho`, keys no
/// `HPTX` topic (the invented install has none), so its help is the mod's
/// string. The install's own strings resolve beside it, and a mod's
/// string replaces the install's of one key (a later file wins).
#[test]
fn a_mods_help_is_its_string() {
    let library = scanned();
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let files = composed(&install, mod_of(&library, ECHO_HALL));
    let archives = archives_with(&files.descriptions);
    let catalogue = Catalogue::build(&archives).expect("a catalogue");
    let hall = catalogue
        .find(b"DUNT", FourCc::new(b"ZQA1"))
        .and_then(|at| catalogue.record(b"DUNT", at))
        .expect("the hall");
    let part = desc::Part::parse(hall.data).expect("a tree");
    let unit = desc::Unit::parse(&part).expect("a unit");
    let help = unit.building().and_then(|b| b.help_id()).expect("a HelpID");
    let strings = dxml::Sources::read(&files.strings).strings();
    let stock = Text::build(&catalogue);
    let text = stock.clone().with_strings(strings);

    assert_eq!(help.word(), string_id("ZQ_hEcho").cast_unsigned());
    assert_eq!(stock.help(help), None, "no topic, no string");
    assert_eq!(
        text.help(help).as_deref(),
        Some("- Rings twice when a hero enters.")
    );
    assert_eq!(
        text.strings().of("IDTXT_ZQ_BASE_ONLY"),
        Some("KEPT"),
        "InterfaceStrings.xml, the base dataset's"
    );
    assert_eq!(
        text.strings().of("IDTXT_ZQ_BASE_LABEL"),
        Some("HALLS ECHO"),
        "the mod's string replaces the install's"
    );
}

/// A game's text: the shipped archive's tables and `files`' strings.
fn text_of<'a>(catalogue: &Catalogue<'a>, files: &Files) -> Text<'a> {
    Text::build(catalogue).with_strings(dxml::Sources::read(&files.strings).strings())
}

/// The verdict without a quest's own messages, as the panel shows it.
fn said(text: &Text<'_>, end: End, quest: Option<FourCc>) -> String {
    let own = quest::Messages::default();

    text::plain(&verdict(text, end, quest, &own)).into_owned()
}

/// Lone Peak says its own win and loss strings; Second Pack, listed over
/// it, replaces both, as a mod's strings come after the quest's. The tags
/// in the mod's strings are read.
#[test]
fn a_mods_string_says_a_quests_verdict() {
    let library = scanned();
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let peak = quest_named(&library, "LONE_PEAK");
    let second = mod_of(&library, SECOND_PACK);
    let archives = archives_with(&[]);
    let catalogue = Catalogue::build(&archives).expect("a catalogue");
    let key = Some(peak.key);
    let own = text_of(&catalogue, &install.quest(&peak).expect("its files"));
    let modded = text_of(
        &catalogue,
        &install.quest_with(&peak, &[second]).expect("its files"),
    );

    assert_eq!(said(&own, End::Won, key), "The peak is ours.");
    assert_eq!(said(&own, End::Lost, key), "The peak keeps its snow.");
    assert_eq!(
        said(&modded, End::Won, key),
        "Peak taken, and the hall stands."
    );
    assert_eq!(
        said(&modded, End::Lost, key),
        "The hall is dark. Nothing answers."
    );
}

/// Echo Hall's tonics are characters flagged `IsInventoryItem` whose title
/// is a text id: their name is the mod's string, its colour tag read.
/// Without its strings, the id itself. A name is no key, and a character
/// without the flag is no item.
#[test]
fn a_mods_string_names_its_item() {
    let library = scanned();
    let base = tree().join("base");
    let install = Install::new(&base, Bytecode::Install);
    let files = composed(&install, mod_of(&library, ECHO_HALL));
    let archives = archives_with(&files.descriptions);
    let catalogue = Catalogue::build(&archives).expect("a catalogue");
    let stock = Text::build(&catalogue);
    let text = stock
        .clone()
        .with_strings(dxml::Sources::read(&files.strings).strings());
    let tonic = string_id("ZQI1");
    let name = |text, key| details::item(&catalogue, text, key).map(|(name, _)| name);
    let light_red = text::tag_colour("light_red").to_be_bytes();

    assert_eq!(
        details::item_name(&catalogue, &text, tonic).as_deref(),
        Some("[color:light_red]Tonic:[/color] 1")
    );
    assert_eq!(
        name(&text, tonic).map(|n| text::plain(&n).into_owned()),
        Some("Tonic: 1".to_owned())
    );
    assert_eq!(
        text::spans(&name(&text, tonic).expect("named"))
            .find(|s| !s.text.is_empty())
            .map(|s| (s.colour, s.text)),
        Some((Some([light_red[1], light_red[2], light_red[3]]), "Tonic:")),
        "light red"
    );
    assert_eq!(name(&stock, tonic).as_deref(), Some("IDTXT_ZQ_TONIC_1"));
    assert_eq!(name(&text, string_id("Tonic1")), None, "a name is no key");
    assert_eq!(
        name(&text, string_id("ZQC3")),
        None,
        "a warden is no inventory item"
    );
}

// ---------------------------------------------------------------------
// data::text, data::qdd
// ---------------------------------------------------------------------

/// The tags' names, lower case, as a search finds them.
const TAG_STARTS: [&str; 4] = ["[color", "[/color", "[fcolor", "[bcolor"];

/// Files a mod's or quest's strings and scripts are in.
const TEXT_FILES: [&str; 4] = ["xml", "mqxml", "mmxml", "gpl"];

/// Every file under `dir` whose extension is one of `extensions`.
fn files_under(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .flat_map(|e| {
            let path = e.path();
            let wanted = path
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| extensions.iter().any(|t| t.eq_ignore_ascii_case(x)));

            if path.is_dir() {
                files_under(&path, extensions)
            } else if wanted {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// Every line holding a colour tag, stored as HD stores it, is left with
/// none by the reader: no tag text is drawn. Nine lines in four files.
#[test]
fn every_colour_tag_is_read() {
    let mut tagged = 0;
    let mut files = 0;

    for path in files_under(&tree(), &TEXT_FILES) {
        let text = std::fs::read_to_string(&path).expect("readable");
        let lines: Vec<&str> = text
            .lines()
            .filter(|l| {
                let lower = l.to_ascii_lowercase();

                TAG_STARTS.iter().any(|t| lower.contains(t))
            })
            .collect();

        for line in &lines {
            let shown = text::plain(&text::unescaped(line)).to_ascii_lowercase();

            assert!(
                !TAG_STARTS.iter().any(|t| shown.contains(t)),
                "{}: {line}",
                path.display()
            );
        }

        tagged += lines.len();
        files += usize::from(!lines.is_empty());
    }

    assert_eq!((files, tagged), (4, 9), "files and lines with a tag");
}

/// The tag syntax of `docs/formats/mods.md`, "Strings": nesting is a
/// stack; `[color=V]` and `[fcolor:V]` read as `[color:V]`; names fold
/// case; an unclosed tag colours to the end; a value with no `]` takes the
/// rest of the text.
#[test]
fn colour_tag_forms_are_read() {
    let path = tree().join("workshop/4400002/XML/tags.xml");
    let strings = dxml::Sources::read(&[path]).strings();
    let shown = |id| text::plain(strings.of(id).expect("a string")).into_owned();
    let runs = |id| {
        text::spans(strings.of(id).expect("a string"))
            .map(|s| (s.colour, s.text))
            .collect::<Vec<_>>()
    };

    assert_eq!(shown("ZQ_TAG_NEST"), "red green red again plain");
    assert_eq!(
        runs("ZQ_TAG_NEST"),
        [
            (None, ""),
            (Some([0xff, 0, 0]), "red "),
            (Some([0, 0xff, 0]), "green"),
            (Some([0xff, 0, 0]), " red again"),
            (None, " plain"),
        ],
        "a close goes back to the colour under it"
    );
    assert_eq!(shown("ZQ_TAG_EQUALS"), "blue and rose");
    assert_eq!(runs("ZQ_TAG_EQUALS")[1], (Some([0x70, 0x70, 0xff]), "blue"));
    assert_eq!(shown("ZQ_TAG_UNCLOSED"), "cyan to the end");
    assert_eq!(
        runs("ZQ_TAG_UNCLOSED").last(),
        Some(&(Some([0, 0xff, 0xff]), "cyan to the end"))
    );
    assert_eq!(shown("ZQ_TAG_NOBRACKET"), "before ");
    assert_eq!(shown("ZQ_TAG_CASE"), "orange shade");
}

/// Every `.qdd` of a mod reads whole: no stray line, no unclosed value;
/// a value may span lines, a `//` comment is dropped (`docs/formats/README.md`,
/// "`.qdd`").
#[test]
fn every_mod_qdd_parses() {
    let paths = files_under(&tree(), &["qdd"]);
    let text = std::fs::read(&paths[0]).expect("readable");
    let quests = qdd::parse(&String::from_utf8_lossy(&text)).expect("a .qdd");

    assert_eq!(paths.len(), 1, "the fixtures hold one .qdd");
    assert_eq!(
        quests.iter().map(|q| q.id.as_str()).collect::<Vec<_>>(),
        ["ZQ_MARCH", "ZQ_REST"]
    );
    assert_eq!(
        quests[0].field("Short Description"),
        Some("A march over\nthe invented hills.")
    );
    assert_eq!(quests[1].field("Short Description"), Some("Rest."));
}
