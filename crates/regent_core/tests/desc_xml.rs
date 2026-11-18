//! XML descriptions made into records (`data::desc::xml`), against the
//! SDK's own: `OriginalQuests` holds every shipped descriptor as XML, so
//! each made record can be laid beside the archive's. Then the XML
//! strings: a quest's and a verdict.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::catalogue::Library;
use regent_core::data::desc::{self, xml as dxml};
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::data::{cam, xml};
use regent_core::sim::world::defeat::End;
use regent_core::view::menu::end::verdict;

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
