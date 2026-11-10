//! The readers over HD's whole install, `SDK/` included.
//!
//! `game_files.rs` pins `Data` and `DataMX` in detail.
//!
//! Counts are exact; they change only if Steam patches the game.
//!
//! Needs `MAJESTY_HD` (environment or `.env`); missing, it panics.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::{FourCc, cam, desc, imag, payload, tile};
use regent_core::view::{Colour, PaletteSource, Sprite};

/// One edition: a label for failure messages, and its install root.
struct Edition {
    name: &'static str,
    root: PathBuf,
}

/// HD. Archives are found by walking the whole root.
fn editions() -> [Edition; 1] {
    [Edition {
        name: "Windows HD",
        root: common::hd(),
    }]
}

/// The one archive of that name in an edition, wherever it sits. Zero or two
/// matches panic.
fn one_archive(edition: &Edition, name: &str) -> PathBuf {
    let found: Vec<_> = common::archives(&edition.root)
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name)))
        .collect();

    match found.as_slice() {
        [only] => only.clone(),
        other => panic!("{}: {} copies of {name}", edition.name, other.len()),
    }
}

/// The CYLB container reads in both editions. HD's extra archives are
/// the SDK's and the expansion's.
#[test]
fn every_parses_as_cylb() {
    for edition in editions() {
        let files = common::archives(&edition.root);
        let mut records = 0;

        assert!(!files.is_empty(), "{}: found no archives", edition.name);

        for path in &files {
            let data = std::fs::read(path).unwrap();
            let archive = cam::Archive::parse(&data)
                .unwrap_or_else(|e| panic!("{} {}: {e}", edition.name, path.display()));

            assert_eq!(
                archive.version,
                (1, 1),
                "{} {}",
                edition.name,
                path.display()
            );
            assert!(
                !archive.chunks.is_empty(),
                "{} {}",
                edition.name,
                path.display()
            );
            records += archive.records().count();
        }

        let expected = (46, 37_668);

        assert_eq!(
            (files.len(), records),
            expected,
            "{}: archive and record counts changed",
            edition.name
        );
    }
}

/// Every tile decodes in every edition.
#[test]
fn every_decodes_its_tiles() {
    for edition in editions() {
        let (mut v3, mut raw) = (0, 0);

        for path in common::archives(&edition.root) {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            for (chunk, record) in archive.records() {
                if chunk.tag != b"TILE" || record.data.is_empty() {
                    continue;
                }

                let version = tile::Tile::version_word(record.data).unwrap();

                assert!(
                    matches!(version, 1..=3),
                    "{} {}: tile version {version}",
                    edition.name,
                    path.display()
                );

                let t = tile::Tile::parse(record.data).unwrap_or_else(|e| {
                    panic!(
                        "{} {} tile v{version} {}: {e}",
                        edition.name,
                        path.display(),
                        record.name()
                    )
                });

                assert!(
                    t.max_end() <= t.width(),
                    "{} {}",
                    edition.name,
                    path.display()
                );

                if version == tile::VERSION_RLE {
                    v3 += 1;
                } else {
                    raw += 1;
                }
            }
        }

        let (expected_v3, expected_raw) = (29_938, 2_976);

        assert_eq!(v3, expected_v3, "{}: v3 tile count changed", edition.name);
        assert_eq!(
            raw, expected_raw,
            "{}: v1/v2 tile count changed",
            edition.name
        );
    }
}

/// Every payload type with a reader parses in every edition. No counts.
#[test]
fn every_reads_its_known_payloads() {
    for edition in editions() {
        for path in common::archives(&edition.root) {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            for (chunk, record) in archive.records() {
                let tag = chunk.tag.as_bytes();
                let at = || format!("{} {} {}", edition.name, path.display(), chunk.tag);

                let outcome = if payload::DESCRIPTOR_TAGS.contains(&tag) {
                    payload::Descriptor::parse(record.data).map(drop)
                } else if payload::PALETTE_TAGS.contains(&tag) {
                    payload::Palette::parse(record.data).map(drop)
                } else if payload::WAVE_TAGS.contains(&tag) {
                    payload::wave(record.data).map(drop)
                } else if tag == b"STRT" {
                    payload::StringTable::parse(record.data).map(drop)
                } else {
                    Ok(())
                };

                outcome.unwrap_or_else(|e| panic!("{}: {e}", at()));
            }
        }
    }
}

/// Every `PICT` is a four-byte kind word and a standard file: kind 7 Bink
/// video, kind 1 TGA. Not decoded here.
///
/// Checked: a Bink's stated length ends exactly at the payload end; a TGA has
/// the version-2 footer in its last 18 bytes. Holds on all 34 distinct records.
///
#[test]
fn every_pict_wraps_a_file_that_checks_out() {
    let mut videos = std::collections::BTreeMap::new();
    let mut images = 0;

    for edition in editions() {
        let mut picts = 0;

        for path in common::archives(&edition.root) {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            let Some(chunk) = archive.chunk_by_tag(b"PICT") else {
                continue;
            };

            picts += chunk.records.len();

            for record in &chunk.records {
                match payload::Pict::parse(record.data)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), record.id))
                {
                    payload::Pict::Video { revision, .. } => {
                        *videos.entry(revision).or_insert(0usize) += 1;
                    }
                    payload::Pict::Image { .. } => images += 1,
                }
            }
        }

        assert!(picts > 0, "{}: expected PICT records", edition.name);
    }

    assert_eq!(videos[b"BIKh"], 19, "base videos");
    assert_eq!(videos[b"BIKf"], 1, "QM13");
    assert_eq!(videos[b"BIKi"], 13, "expansion videos");
    assert_eq!(images, 1, "SPLH is the only still");
}

/// `textdata.cam`'s `UNTN` and `ACTN` (unit and action names) are keyed by
/// record id.
///
/// Every key names a record in `unittype.cam` or `action.cam`. Read without
/// stripping the key, the first name comes out `XL00-` instead of `-`.
///
#[test]
fn hd_fills_in_the_unit_and_action_name_tables() {
    for edition in editions() {
        let text = std::fs::read(one_archive(&edition, "textdata.cam")).unwrap();
        let archive = cam::Archive::parse(&text).unwrap();
        let table = |id: &[u8; 4]| {
            let Some((_, r)) = archive
                .records()
                .find(|(chunk, r)| chunk.tag == b"STRT" && r.id == FourCc::new(id))
            else {
                panic!("{}: no {} table", edition.name, FourCc::new(id));
            };

            payload::StringTable::parse(r.data).unwrap()
        };

        let named = |file: &str, tag: &[u8; 4], names: &payload::StringTable| {
            let data = std::fs::read(one_archive(&edition, file)).unwrap();
            let source = cam::Archive::parse(&data).unwrap();
            let ids: Vec<_> = source
                .records()
                .filter(|(chunk, _)| chunk.tag == tag)
                .map(|(_, r)| r.id)
                .collect();

            names.keys().filter(|k| ids.contains(k)).count()
        };

        let units = table(b"UNTN");
        let actions = table(b"ACTN");
        let resolved = (
            named("unittype.cam", b"DUNT", &units),
            named("action.cam", b"DACT", &actions),
        );

        assert_eq!((units.len(), actions.len()), (394, 39));
        assert_eq!(
            resolved,
            (394, 39),
            "a name table key stopped naming a record"
        );
        assert_eq!(
            units.get_by_key(FourCc::new(b"XL20")).unwrap(),
            "Meteor Storm"
        );
        assert_eq!(
            actions.get_by_key(FourCc::new(b"WRa0")).unwrap(),
            "Power Shock"
        );
    }
}

/// Every `IMAG` record decodes in every edition.
#[test]
fn every_decodes_its_imag_records() {
    for edition in editions() {
        let (mut records, mut slots, mut frames) = (0, 0, 0);

        for path in common::archives(&edition.root) {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            for (chunk, record) in archive.records() {
                if chunk.tag != b"IMAG" || record.data.is_empty() {
                    continue;
                }

                let image = imag::Image::parse(record.data).unwrap_or_else(|e| {
                    panic!("{} {} {}: {e}", edition.name, path.display(), record.name())
                });

                records += 1;
                slots += image.slot_count();
                frames += image.frame_count();
            }
        }

        let expected = (654, 13_515, 87_800);

        assert_eq!(
            (records, slots, frames),
            expected,
            "{}: imag totals changed",
            edition.name
        );
    }
}

/// `IMAG` versions 2 and 3 stay at 98 and 28 records in every edition; 4 and 5
/// grow. Evidence that `+0x00` is a version word.
#[test]
fn the_older_imag_layouts_are_frozen() {
    for edition in editions() {
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();

        for path in common::archives(&edition.root) {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            for (chunk, record) in archive.records() {
                if chunk.tag == b"IMAG" && !record.data.is_empty() {
                    *counts
                        .entry(imag::Image::version_word(record.data).unwrap())
                        .or_default() += 1;
                }
            }
        }

        assert_eq!(counts[&2], 98, "{}: version 2 count moved", edition.name);
        assert_eq!(counts[&3], 28, "{}: version 3 count moved", edition.name);

        let expected = (502, 26);

        assert_eq!(
            (counts[&4], counts[&5]),
            expected,
            "{}: version 4 or 5 count changed",
            edition.name
        );
    }
}

/// Every tile in every archive resolves a palette, in both editions.
///
/// `view.rs` covers only the 34 archives the dataset definitions name.
/// No dangling reference anywhere, so `Colour::Indexed` holds its palette
/// directly, not as an `Option`.
#[test]
fn every_tile_resolves_its_palette() {
    for edition in editions() {
        let (mut tiles, mut indexed, mut embedded) = (0, 0, 0);

        for path in common::archives(&edition.root) {
            // Palette references are archive-local, so one archive per catalogue.
            let owned = vec![(path.display().to_string(), std::fs::read(&path).unwrap())];
            let cat = Catalogue::build(&owned).unwrap();

            let Some(chunk) = cat.archives[0].1.chunk_by_tag(b"TILE") else {
                continue;
            };

            for (index, record) in chunk.records.iter().enumerate() {
                if record.data.is_empty() {
                    continue;
                }

                let sprite = Sprite::resolve(&cat, 0, record.data).unwrap_or_else(|e| {
                    panic!("{} {} tile {index}: {e}", edition.name, path.display())
                });

                tiles += 1;

                if matches!(sprite.colour, Colour::Indexed { .. }) {
                    indexed += 1;
                    embedded += usize::from(sprite.source == PaletteSource::Embedded);
                }
            }
        }

        let expected = [32_914, 30_634, 3_103];

        assert_eq!(
            [tiles, indexed, embedded],
            expected,
            "{}: [tiles, indexed, embedded palettes] changed",
            edition.name
        );
    }
}

/// HD's `UNTN`/`ACTN` names match each descriptor's `HEAD` display name.
///
/// A `HEAD` holds an internal and a display name: `MRb1` is
/// `hands_of_steel_icon` and "Hands of Steel". The names also appear in
/// `GMTX`, a keyless pool of 388 UI strings no descriptor references.
///
/// Where the table writes `-` (no player-facing name), the record's display
/// name is its internal name with spaces for underscores.
#[test]
fn the_hd_name_tables_agree_with_every_descriptor() {
    let hd = editions()
        .into_iter()
        .find(|e| e.name == "Windows HD")
        .expect("the HD edition");

    let text = std::fs::read(one_archive(&hd, "textdata.cam")).unwrap();
    let archive = cam::Archive::parse(&text).unwrap();
    let table = |id: &[u8; 4]| {
        let (_, r) = archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"STRT" && r.id == FourCc::new(id))
            .unwrap_or_else(|| panic!("no {} table", FourCc::new(id)));

        payload::StringTable::parse(r.data).unwrap()
    };

    let (mut checked, mut nameless, mut differ) = (0, 0, BTreeMap::new());

    for (file, tag, names) in [
        ("unittype.cam", &b"DUNT"[..], table(b"UNTN")),
        ("action.cam", &b"DACT"[..], table(b"ACTN")),
    ] {
        let data = std::fs::read(one_archive(&hd, file)).unwrap();
        let source = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in source.records() {
            if chunk.tag.as_bytes() != tag {
                continue;
            }

            let Some(listed) = names.get_by_key(r.id) else {
                continue;
            };
            let Ok(part) = desc::Part::parse(r.data) else {
                continue;
            };

            let head =
                desc::Head::parse(part.find(b"HEAD").map_or(&[][..], |p| p.body)).expect("a HEAD");

            // Nameless: trees, rocks, mockups.
            if listed == "-" {
                // e.g. "mockup flag" for `mockup_flag`.
                let loose = |b: &[u8]| String::from_utf8_lossy(b).replace('_', " ").to_lowercase();

                assert_eq!(
                    loose(head.description),
                    loose(head.name),
                    "{} is unnamed in the table and really named in the record",
                    r.id
                );

                nameless += 1;
                continue;
            }

            checked += 1;

            let mine = String::from_utf8_lossy(head.description);

            if listed != mine {
                differ.insert(
                    r.id.trimmed().to_owned(),
                    (mine.into_owned(), listed.into_owned()),
                );
            }
        }
    }

    assert!(checked > 300, "only {checked} descriptors were named twice");
    assert!(
        nameless > 50,
        "only {nameless} nameless ones, so the `-` case went untested"
    );
    assert_eq!(differ, BTreeMap::new(), "a table and its record disagree");

    // `MRb1` is both a `DUNT` (icon overlay on a buffed Monk) and a `DACT`
    // (the cast), spelled differently. One id, two things.
    assert_eq!(
        table(b"UNTN").get_by_key(FourCc::new(b"MRb1")).unwrap(),
        "Hands of Steel"
    );
    assert_eq!(
        table(b"ACTN").get_by_key(FourCc::new(b"MRb1")).unwrap(),
        "Hand of Steel"
    );
}

/// A spell has a name and no description text.
///
/// Every spell overlay in the SDK XML sets `DialogID` (the `STRT` a selected
/// thing's panel shows) to zero; actions have no `DialogID`. Spell book text
/// is built at run time from descriptor numbers.
#[test]
fn a_spell_has_a_name_and_no_description_text() {
    let hd = editions()
        .into_iter()
        .find(|e| e.name == "Windows HD")
        .expect("the HD edition");

    let mut found = Vec::new();

    for path in common::archives(&hd.root) {
        let data = std::fs::read(&path).unwrap();
        let Ok(archive) = cam::Archive::parse(&data) else {
            continue;
        };

        for (chunk, r) in archive.records() {
            if chunk.tag != b"STRT" {
                continue;
            }
            let Ok(table) = payload::StringTable::parse(r.data) else {
                continue;
            };

            for i in 0..table.len() {
                if table.get(i).is_some_and(|s| s.contains("Hands of Steel")) {
                    found.push(format!("{}", r.id));
                }
            }
        }
    }

    found.sort();
    found.dedup();

    // Two names, no description: a description would be a third hit.
    assert_eq!(found, ["GMTX", "UNTN"]);
}

/// Totals over one `DUNT` archive's variable-length tails: GPL callback
/// string, optional attachment tree, sound overrides, two shadow words. See
/// `docs/formats/payloads.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tails {
    records: usize,
    attachments: usize,
    weapons: usize,
    movement: usize,
    overrides: usize,
    scripts: usize,
    /// Records whose last word is non-zero. Zero in every edition.
    trailing: usize,
}

/// `unittype.cam`, the base game.
const BASE: Tails = Tails {
    records: 394,
    attachments: 249,
    weapons: 226,
    movement: 93,
    overrides: 5,
    scripts: 68,
    trailing: 0,
};

/// `mx_unittype.cam`, the expansion.
const EXPANSION: Tails = Tails {
    records: 174,
    attachments: 90,
    weapons: 73,
    movement: 40,
    overrides: 0,
    scripts: 45,
    trailing: 0,
};

/// `mdl1_unittype.cam`, the downloadable Krolm content.
const KROLM: Tails = Tails {
    records: 9,
    attachments: 6,
    weapons: 6,
    movement: 5,
    overrides: 0,
    scripts: 0,
    trailing: 0,
};

/// `xqd1_unittype.cam`, the downloadable expansion quest.
const BONUS_QUEST: Tails = Tails {
    records: 6,
    attachments: 4,
    weapons: 4,
    movement: 2,
    overrides: 0,
    scripts: 1,
    trailing: 0,
};

/// The four `DUNT` archive filenames.
const DATASETS: [&str; 4] = [
    "unittype.cam",
    "mx_Unittype.cam",
    "MDL1_unittype.cam",
    "XQD1_UnitType.cam",
];

/// The one archive of that name under `root`, or `None` where it is not shipped.
fn dataset(root: &std::path::Path, name: &str) -> Option<PathBuf> {
    let found: Vec<_> = common::archives(root)
        .into_iter()
        .filter(|p| p.file_name().is_some_and(|f| f.eq_ignore_ascii_case(name)))
        .collect();

    match found.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        other => panic!("{} copies of {name} under {}", other.len(), root.display()),
    }
}

/// Every `DUNT` tail of one dataset, and the units that cast a shadow. Panics
/// if the archive is not shipped.
fn tails(root: &std::path::Path, name: &str) -> (Tails, Vec<(String, desc::ShadowType)>) {
    let path = dataset(root, name).unwrap_or_else(|| panic!("no {name} under {}", root.display()));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let archive = cam::Archive::parse(&bytes).unwrap();

    let mut found = Tails {
        records: 0,
        attachments: 0,
        weapons: 0,
        movement: 0,
        overrides: 0,
        scripts: 0,
        trailing: 0,
    };
    let mut shadows = Vec::new();

    for (chunk, record) in archive.records() {
        if chunk.tag != b"DUNT" || record.data.is_empty() {
            continue;
        }

        let part = desc::Part::parse(record.data).unwrap();
        let unit = desc::Unit::parse(&part)
            .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), record.id));
        let tail = unit.tail();

        found.records += 1;
        found.overrides += tail.overrides().count();
        found.scripts += usize::from(!tail.script.is_empty());

        if let Some(attached) = tail.attachments {
            found.attachments += 1;
            found.movement += usize::from(attached.movement_class().is_some());
            found.weapons += usize::from(attached.weapon.is_some());
        }

        let kind = unit.shadow_type();

        if kind != desc::ShadowType::None {
            shadows.push((record.id.to_string(), kind));
        }

        // The word past `ShadowType`. Guess: `ShadowOptions`; see
        // `docs/formats/payloads.md`.
        found.trailing += usize::from(unit.prim[unit.prim.len() - 4..] != [0; 4]);
    }

    (found, shadows)
}

/// The one `Dynamic` shadow in the game.
fn horrify() -> Vec<(String, desc::ShadowType)> {
    vec![("WPk1".to_owned(), desc::ShadowType::Dynamic)]
}

#[test]
fn the_base_datasets_unit_tails_read() {
    assert_eq!(tails(&common::hd(), DATASETS[0]), (BASE, vec![]));
}

#[test]
fn the_expansions_unit_tails_read() {
    assert_eq!(tails(&common::hd(), DATASETS[1]), (EXPANSION, horrify()));
}

#[test]
fn the_krolm_datasets_unit_tails_read() {
    assert_eq!(tails(&common::hd(), DATASETS[2]), (KROLM, vec![]));
}

#[test]
fn the_bonus_quests_unit_tails_read() {
    assert_eq!(tails(&common::hd(), DATASETS[3]), (BONUS_QUEST, vec![]));
}

/// The SDK sets `ShadowType` once, to `Dynamic`.
///
/// HD's XML importer maps `None`, `Static`, `Dynamic` to 0, 1, 2
/// (`MajestyHD.exe:unit engine names`), so a 2 in the field is `Dynamic`.
#[test]
fn the_sdk_names_the_one_dynamic_shadow() {
    let xml =
        std::fs::read_to_string(common::sdk().join("OriginalQuests/DataMX/MX_Projectiles.xml"))
            .unwrap();

    assert_eq!(xml.matches("<ShadowType").count(), 1);
    assert!(xml.contains(r#"<ShadowType value="Dynamic"/>"#));
}
