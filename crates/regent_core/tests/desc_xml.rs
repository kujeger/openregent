//! XML descriptions made into records (`data::desc::xml`), against the
//! SDK's own: `OriginalQuests` holds every shipped descriptor as XML, so
//! each made record can be laid beside the archive's. Then the XML
//! strings: a help topic, a verdict, an item's name.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::catalogue::Library;
use regent_core::data::desc::{self, xml as dxml};
use regent_core::data::fourcc::{FourCc, string_id};
use regent_core::data::text::Text;
use regent_core::data::{cam, xml};
use regent_core::sim::world::defeat::End;
use regent_core::view::menu::end::verdict;
use regent_core::view::panel::portrait::details;

/// An edition's SDK XML and its archives of record.
struct Edition {
    sets: Datasets,
    xml: &'static [&'static str],
    archives: &'static [&'static str],
}

const BASE: Edition = Edition {
    sets: Datasets::Base,
    xml: &[
        "Data/M_Characters.xml",
        "Data/M_Buildings.xml",
        "Data/M_Overlays.xml",
        "Data/M_TerrainFeatures.xml",
        "Data/M_Projectiles.xml",
        "Data/M_ParticleSystems.xml",
        "Data/M_Actions.xml",
        "Data/M_Sounds.xml",
    ],
    archives: &["Data/unittype.cam", "Data/action.cam", "Data/sounddesc.cam"],
};

const EXPANSION: Edition = Edition {
    sets: Datasets::Expansion,
    xml: &[
        "DataMX/MX_Characters.xml",
        "DataMX/MX_Buildings.xml",
        "DataMX/MX_Overlays.xml",
        "DataMX/MX_TerrainFeatures.xml",
        "DataMX/MX_Projectiles.xml",
        "DataMX/MX_ParticleSystems.xml",
        "DataMX/MX_Actions.xml",
        "DataMX/MX_Sounds.xml",
    ],
    archives: &[
        "DataMX/mx_Unittype.cam",
        "DataMX/mx_action.cam",
        "DataMX/mx_sounddesc.cam",
    ],
};

fn image_sets() -> dxml::ImageSets {
    let text = std::fs::read_to_string(common::hd().join("Data/ImageSetIDXRef.xml"))
        .expect("the image set table");

    dxml::ImageSets::parse(&xml::parse(&text).expect("well-formed"))
}

fn texts(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| String::from_utf8_lossy(&std::fs::read(p).expect("readable")).into_owned())
        .collect()
}

/// One section's bytes per record id, by part tag.
type Sections = BTreeMap<String, Vec<u8>>;

fn sections(payload: &[u8]) -> Sections {
    let root = desc::Part::parse(payload).expect("a descriptor tree");

    root.walk()
        .filter(|p| p.parts.is_empty())
        .map(|p| (p.tag.to_string(), p.body.to_vec()))
        .collect()
}

/// Every word that differs, by (chunk, kind, section, offset), counted.
fn compare(edition: &Edition) -> (usize, BTreeMap<String, usize>) {
    let hd = common::hd();
    let files = Install::new(&hd, Bytecode::Install)
        .files(edition.sets)
        .expect("the dataset");
    let library = Library::from_files(&files.archives).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let sdk = common::sdk().join("OriginalQuests");
    let paths: Vec<PathBuf> = edition.xml.iter().map(|x| sdk.join(x)).collect();
    // HD's unit set spells the subtype `Generic`; the SDK writes
    // `General`, which HD refuses. Respelt, the 78 trees and rocks are
    // compared too.
    let texts: Vec<String> = texts(&paths)
        .into_iter()
        .map(|t| t.replace("subType=\"General\"", "subType=\"Generic\""))
        .collect();
    let roots: Vec<_> = texts
        .iter()
        .map(|t| xml::parse(t).expect("well-formed"))
        .collect();
    let refs: Vec<_> = roots.iter().collect();
    let compiled = dxml::compile(&refs, &catalogue, &image_sets());

    let shipped: Vec<Vec<u8>> = edition
        .archives
        .iter()
        .map(|a| std::fs::read(hd.join(a)).expect("an archive"))
        .collect();
    let shipped: Vec<cam::Archive<'_>> = shipped
        .iter()
        .map(|b| cam::Archive::parse(b).expect("an archive"))
        .collect();
    let find = |tag: [u8; 4], id: regent_core::data::FourCc| {
        shipped
            .iter()
            .flat_map(cam::Archive::records)
            .find(|(c, r)| c.tag == &tag && r.id == id)
            .map(|(_, r)| r.data)
    };

    let mut diffs: BTreeMap<String, usize> = BTreeMap::new();
    let mut compared = 0;

    for record in &compiled.records {
        let Some(theirs) = find(record.set.tag(), record.id) else {
            *diffs
                .entry(format!("{:?} no archive record", record.set))
                .or_default() += 1;
            continue;
        };
        let kind = desc::Head::parse(
            desc::Part::parse(theirs)
                .unwrap()
                .find(b"HEAD")
                .unwrap()
                .body,
        )
        .unwrap()
        .word;
        let (ours, theirs) = (sections(&record.payload), sections(theirs));

        compared += 1;

        for (tag, a) in &ours {
            let b = theirs.get(tag).map_or(&[][..], Vec::as_slice);

            if a.len() != b.len() {
                *diffs
                    .entry(format!("{:?} {kind} {tag} length", record.set))
                    .or_default() += 1;
            }

            for at in (0..a.len().min(b.len())).step_by(4) {
                let end = (at + 4).min(a.len()).min(b.len());

                if a[at..end] != b[at..end] {
                    let key = format!("{:?} {kind} {tag} {at:#05x}", record.set);

                    if std::env::var_os("SHOW").is_some() {
                        println!(
                            "{key} {}: ours {:02x?} theirs {:02x?}",
                            record.id,
                            &a[at..end],
                            &b[at..end]
                        );
                    }

                    *diffs.entry(key).or_default() += 1;
                }
            }
        }
    }

    (compared, diffs)
}

/// Words the SDK's XML cannot say, each the export's, not the encoder's:
///
/// - `PRIM 0x008`, `CanUse`: zero in the archive, filled from the id by
///   the loader, and the export prints the filled value.
/// - `SECD 0x058`, `0x05c`: `0xcdcdcdcd`, uninitialised, in the archive.
/// - Action `PRIM 0x034`: a `CompletionImageSet` of -1, which the export
///   leaves out and no XML value gives.
/// - Action `PRIM 0x044`, projectile `PRIM 0x018`: `Script` `cProc` and
///   `type`, which HD's XML reader sets to 0.
/// - Sound `PRIM 0x028`, `0x238`, `0x244`: an override word whose flag is
///   clear, so the export leaves it out.
/// - Overlay `XR06` `PRIM 0x02c`: `DefaultSound` `Invisibility`, the name
///   of both `SL07` and `SL49`; a name finds the lower, the archive holds
///   the higher.
const BASE_WORDS: [(&str, usize); 12] = [
    ("Action 1 PRIM 0x034", 1),
    ("Action 1 PRIM 0x044", 3),
    ("Sound 1 PRIM 0x028", 1),
    ("Sound 1 PRIM 0x238", 1),
    ("Sound 1 PRIM 0x244", 1),
    ("Unit 0 PRIM 0x008", 5),
    ("Unit 1 PRIM 0x008", 11),
    ("Unit 2 PRIM 0x008", 91),
    ("Unit 3 PRIM 0x008", 67),
    ("Unit 3 SECD 0x058", 6),
    ("Unit 3 SECD 0x05c", 1),
    ("Unit 4 PRIM 0x008", 33),
];

const EXPANSION_WORDS: [(&str, usize); 11] = [
    ("Action 1 PRIM 0x034", 2),
    ("Sound 1 PRIM 0x238", 1),
    ("Sound 1 PRIM 0x244", 1),
    ("Unit 1 PRIM 0x008", 4),
    ("Unit 1 PRIM 0x018", 2),
    ("Unit 2 PRIM 0x008", 26),
    ("Unit 3 PRIM 0x008", 20),
    ("Unit 3 SECD 0x058", 6),
    ("Unit 4 PRIM 0x008", 1),
    ("Unit 4 PRIM 0x02c", 1),
    ("Unit 4 PRIM 0x030", 1),
];

/// Every SDK description made into a record equals the archive's, byte
/// for byte, but for the words its XML cannot say. `SHOW=1` prints each.
#[test]
fn the_sdk_xml_makes_the_archive_records() {
    for (edition, records, words) in [
        (&BASE, 767, &BASE_WORDS[..]),
        (&EXPANSION, 378, &EXPANSION_WORDS[..]),
    ] {
        let (compared, diffs) = compare(edition);
        let want: BTreeMap<String, usize> =
            words.iter().map(|(k, n)| ((*k).to_owned(), *n)).collect();

        assert_eq!(compared, records, "{:?} records compared", edition.sets);
        assert_eq!(diffs, want, "{:?} words that differ", edition.sets);
    }
}

/// Each Workshop mod's files over the dataset it fits, base first.
fn workshop_files() -> Vec<(
    regent_core::content::mods::Mod,
    regent_core::content::dataset::Files,
)> {
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let library = regent_core::content::mods::scan(&[common::workshop()]);

    library
        .mods
        .into_iter()
        .filter_map(|m| {
            let sets = if m.base.fits(Datasets::Base) {
                Datasets::Base
            } else {
                Datasets::Expansion
            };
            let files = install.files_with(sets, &[&m]).ok()?;

            Some((m, files))
        })
        .collect()
}

/// Every description file a Workshop mod or quest lists, each once.
fn workshop_description_files() -> Vec<PathBuf> {
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let library = regent_core::content::mods::scan(&[common::workshop()]);
    let mut files: Vec<PathBuf> = workshop_files()
        .into_iter()
        .flat_map(|(_, f)| f.descriptions)
        .collect();

    for found in &library.quests {
        if let Ok(f) = install.quest(&found.quest) {
            files.extend(f.descriptions);
        }
    }

    files.sort();
    files.dedup();
    files
}

/// Every Workshop description file parses, and each `<Description>` is
/// read: 1,592 in 60 files, none refused. (The Krolm add-ons' numbered
/// subtypes, which HD's table lacks, are in a file nothing lists.)
#[test]
fn every_workshop_description_reads() {
    let files = workshop_description_files();
    let sources = dxml::Sources::read(&files);
    let (trees, unread) = sources.parsed();
    let descriptions: usize = trees.iter().map(|t| t.named("Description").count()).sum();
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let compiled = sources.compile(&catalogue, &image_sets());
    let refused: BTreeMap<String, usize> =
        compiled.refused.iter().fold(BTreeMap::new(), |mut out, r| {
            *out.entry(format!("{:?}", r.why)).or_default() += 1;
            out
        });

    assert_eq!(files.len(), 60, "description files");
    assert!(unread.is_empty(), "{unread:?}");
    assert_eq!(descriptions, 1_592, "descriptions");
    assert_eq!(refused, BTreeMap::new(), "refused");
    assert_eq!(compiled.records.len(), 1_375, "records made, one an id");
}

/// Each mod's records over the dataset it fits: a shipped id replaced
/// whole, the rest new. Improved Guards Plus remakes three guards and a
/// guardhouse; Majestic Majesty replaces 130 shipped records.
#[test]
fn a_mods_record_replaces_the_shipped_one() {
    let sets = image_sets();
    let mut replaced: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for (m, files) in workshop_files() {
        let library = Library::from_files(&files.archives).expect("the archives");
        let catalogue = library.catalogue().expect("a catalogue");
        let compiled = dxml::Sources::read(&files.descriptions).compile(&catalogue, &sets);
        let shipped = compiled
            .records
            .iter()
            .filter(|r| catalogue.find(&r.set.tag(), r.id).is_some())
            .count();

        assert!(compiled.refused.is_empty(), "{}", m.title());
        replaced.insert(m.title(), (compiled.records.len(), shipped));
    }

    assert_eq!(replaced.len(), 39, "every mod composes");
    assert_eq!(replaced["Majestic Majesty"], (804, 130));
    assert_eq!(replaced["New Game+"], (258, 189));
    assert_eq!(replaced["Custom Guild: Phantoms Haunt"], (71, 3));
    assert_eq!(
        replaced["Improved Guards Plus (2 guards in upgraded guardhouses)"],
        (6, 6)
    );
}

/// Improved Guards Plus over the expansion: the made archive, loaded
/// last, wins the City Guard's id, whose stats are the mod's, and its
/// guardhouse names the mod's upgrade.
#[test]
fn a_made_archive_wins_its_ids() {
    let (m, files) = workshop_files()
        .into_iter()
        .find(|(m, _)| m.title().starts_with("Improved Guards Plus"))
        .expect("the mod");
    let library = Library::from_files(&files.archives).expect("the archives");
    let base = library.catalogue().expect("a catalogue");
    let made = dxml::Sources::read(&files.descriptions)
        .compile(&base, &image_sets())
        .archive();
    let archives: Vec<(&str, &[u8])> = base
        .archives
        .iter()
        .map(|(n, _)| *n)
        .zip(library_bytes(&files.archives))
        .chain(std::iter::once(("descriptions.xml", made.as_slice())))
        .collect();
    let catalogue = regent_core::data::catalogue::Catalogue::parse(archives).expect("a catalogue");
    let guard = regent_core::data::FourCc::new(b"AVb1");
    let at = catalogue.find(b"DUNT", guard).expect("the city guard");
    let record = catalogue.record(b"DUNT", at).expect("its record");
    let part = desc::Part::parse(record.data).expect("a tree");
    let unit = desc::Unit::parse(&part).expect("a unit");
    let character = unit.character().expect("a character");

    assert_eq!(at.archive, catalogue.archives.len() - 1, "{}", m.title());
    assert_eq!(character.max_hp(), Some(20));
    assert_eq!(character.strength(), Some(14), "12 shipped");
    assert_eq!(character.attack(), Some(45), "35 shipped");
    assert!(!unit.can_use(desc::User::Monster));
    assert!(unit.can_use(desc::User::HumanPlayer), "its XML says so");
}

/// Each archive's bytes, in order, empty ones left out as the catalogue
/// leaves them.
fn library_bytes(paths: &[PathBuf]) -> Vec<&'static [u8]> {
    paths
        .iter()
        .map(|p| &*std::fs::read(p).expect("an archive").leak())
        .filter(|b| !b.is_empty())
        .collect()
}

/// Monster Kingdom's Dark Palace replaces `ABJ1`; its `HelpID`,
/// `MK_hPalace`, keys no `HPTX` topic, so its help is the mod's string.
/// The install's own strings resolve beside it.
#[test]
fn a_mods_help_is_its_string() {
    let (_, files) = workshop_files()
        .into_iter()
        .find(|(m, _)| m.title() == "Monster Kingdom")
        .expect("the mod");
    let library = Library::from_files(&files.archives).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let compiled = dxml::Sources::read(&files.descriptions).compile(&catalogue, &image_sets());
    let palace = compiled
        .records
        .iter()
        .find(|r| r.id == regent_core::data::FourCc::new(b"ABJ1"))
        .expect("the palace");
    let part = desc::Part::parse(&palace.payload).expect("a tree");
    let unit = desc::Unit::parse(&part).expect("a unit");
    let help = unit.building().and_then(|b| b.help_id()).expect("a HelpID");
    let strings = dxml::Sources::read(&files.strings).strings();
    let text = regent_core::data::text::Text::build(&catalogue).with_strings(strings);

    assert_eq!(
        help.word(),
        regent_core::data::fourcc::string_id("MK_hPalace").cast_unsigned()
    );
    assert!(
        text.help(help)
            .is_some_and(|h| h.starts_with("- Supports 3 Gnomes")),
        "{:?}",
        text.help(help)
    );
    assert_eq!(
        text.strings().of("IDTXT_MODDING_ACTIVATE_MODS"),
        Some("ACTIVATE MODS"),
        "Data/InterfaceStrings.xml, the base dataset's"
    );
}

/// Wrath of Krolm lists its win and loss messages as strings.
#[test]
fn a_quests_strings_are_listed() {
    let hd = common::hd();
    let quests = regent_core::content::quest::offered(&hd, &[]);
    let krolm = quests
        .iter()
        .find(|q| q.key == regent_core::data::FourCc::new(b"MD01"))
        .expect("Wrath of Krolm");
    let files = Install::new(&hd, Bytecode::Install)
        .quest(krolm)
        .expect("its files");
    let strings = dxml::Sources::read(&files.strings).strings();

    assert_eq!(files.strings.len(), 2, "InterfaceStrings.xml and its own");
    assert!(
        strings
            .of("IDTXT_QUEST_WIN_MESSAGE")
            .is_some_and(|s| s.contains("Krolm's Avatar"))
    );
}

/// A game's text: `files`' archives' tables and its strings, in order.
fn text_of(files: &regent_core::content::dataset::Files) -> Text<'static> {
    let library = Library::from_files(&files.archives).expect("the archives");
    let catalogue = Box::leak(Box::new(library))
        .catalogue()
        .expect("a catalogue");
    let strings = dxml::Sources::read(&files.strings).strings();

    Text::build(&catalogue).with_strings(strings)
}

/// The verdict without a quest's own messages, as the panel shows it.
fn said(text: &Text<'_>, end: End, quest: Option<FourCc>) -> String {
    let own = regent_core::content::quest::Messages::default();

    regent_core::data::text::plain(&verdict(text, end, quest, &own)).into_owned()
}

/// Stock verdicts: a slot's quest by `QEND` and `LOSX`, as
/// `ui_defeat_briefing.png` shows QU18's; a custom game's in both
/// datasets; Wrath of Krolm, which holds no slot, by its own strings,
/// not `LOSX MD01`'s "&".
#[test]
fn a_verdict_asks_the_tables_then_the_strings() {
    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let base = text_of(&install.files(Datasets::Base).expect("base files"));
    let expansion = text_of(&install.files(Datasets::Expansion).expect("expansion files"));
    let won = "Congratulations!  You have won the game.";

    assert_eq!(
        said(&base, End::Lost, Some(FourCc::new(b"QU18"))),
        "You failed to recover all the shards of Brashnard's Sphere!"
    );
    assert_eq!(said(&base, End::Won, None), won);
    assert_eq!(said(&expansion, End::Won, None), won, "FRE1 alone");
    assert_eq!(
        said(&expansion, End::Lost, None),
        "You have lost this game!"
    );

    let quests = regent_core::content::quest::offered(&hd, &[]);
    let krolm = quests
        .iter()
        .find(|q| q.key == FourCc::new(b"MD01"))
        .expect("Wrath of Krolm");
    let text = text_of(&install.quest(krolm).expect("its files"));

    assert!(said(&text, End::Won, Some(krolm.key)).starts_with("\"Well done, Your Majesty."));
    assert_eq!(
        said(&text, End::Lost, Some(krolm.key)),
        "Krolm's power has proven to be too great for you and his will remains supreme in \
         these lands - for now."
    );
}

/// The Wild, a Workshop quest, says its own win and loss strings; a
/// mod listed over it, ME Game Modifiers, replaces both, as a mod's
/// strings come after the quest's.
#[test]
fn a_mods_string_says_a_quests_verdict() {
    const WILD: &str = "{23713EFA-0595-4A99-9890-5178D253C70D}";
    const MODIFIERS: &str = "{4233391D-0EEA-4E85-B7BD-AC0DE66DFF58}";

    let hd = common::hd();
    let install = Install::new(&hd, Bytecode::Install);
    let library = regent_core::content::mods::scan(&[common::workshop()]);
    let guid = |id| regent_core::data::guid::Guid::parse(id).expect("a GUID");
    let wild = &library
        .quests
        .iter()
        .find(|f| f.quest.id == guid(WILD))
        .expect("The Wild")
        .quest;
    let modifiers = library.find(guid(MODIFIERS)).expect("the mod");
    let key = Some(wild.key);
    let own = text_of(&install.quest(wild).expect("its files"));
    let modded = text_of(&install.quest_with(wild, &[modifiers]).expect("its files"));

    assert_eq!(
        said(&own, End::Won, key),
        "We have pushed back the wild beasts! Victory is ours!"
    );
    assert_eq!(
        said(&own, End::Lost, key),
        "Our palace has fallen to the beasts... our kingdom is no more..."
    );
    assert!(said(&modded, End::Won, key).starts_with("Your Majesty, we... won?"));
    assert_eq!(
        said(&modded, End::Lost, key),
        "Our kingdom falls, but we must never forget how long we lasted against all odds!"
    );
}

/// Majestic Majesty's potions are characters flagged `IsInventoryItem`
/// whose title is a text id: their name is the mod's string, its colour
/// tag read. Without its strings, the id itself; an `INV:n` slot is still
/// `QITM`'s.
#[test]
fn a_mods_string_names_its_item() {
    let (_, files) = workshop_files()
        .into_iter()
        .find(|(m, _)| m.title() == "Majestic Majesty")
        .expect("the mod");
    let library = Library::from_files(&files.archives).expect("the archives");
    let base = library.catalogue().expect("a catalogue");
    let made = dxml::Sources::read(&files.descriptions)
        .compile(&base, &image_sets())
        .archive();
    let archives: Vec<(&str, &[u8])> = base
        .archives
        .iter()
        .map(|(n, _)| *n)
        .zip(library_bytes(&files.archives))
        .chain(std::iter::once(("descriptions.xml", made.as_slice())))
        .collect();
    let catalogue = regent_core::data::catalogue::Catalogue::parse(archives).expect("a catalogue");
    let stock = Text::build(&catalogue);
    let text = stock
        .clone()
        .with_strings(dxml::Sources::read(&files.strings).strings());
    let potion = string_id("HealingPotions1");
    let ring = regent_core::data::attrib::Key::new(*b"INV", 11).packed();
    let name = |text, key| details::item(&catalogue, text, key).map(|(name, _)| name);

    assert_eq!(
        details::item_name(&catalogue, &text, potion).as_deref(),
        Some("[color:light_red]Healing Potions:[/color] 1")
    );
    assert_eq!(
        name(&text, potion).map(|n| regent_core::data::text::plain(&n).into_owned()),
        Some("Healing Potions: 1".to_owned())
    );
    assert_eq!(
        regent_core::data::text::spans(&name(&text, potion).expect("named"))
            .find(|s| !s.text.is_empty())
            .map(|s| (s.colour, s.text)),
        Some((Some([0xff, 0x8c, 0x8c]), "Healing Potions:")),
        "light red"
    );
    assert_eq!(
        name(&stock, potion).as_deref(),
        Some("IDTXT_ITEM_HEALINGPOTIONS_1")
    );
    assert_eq!(name(&text, ring).as_deref(), Some("Ring of Protection"));
    assert_eq!(
        name(&text, string_id("Healing_Potions1")),
        None,
        "a name is no key"
    );
}
