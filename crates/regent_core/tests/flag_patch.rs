mod common;

use std::path::PathBuf;

use regent_core::content::flags;
use regent_core::data::cam::{self, ChunkFlag, Written};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::{BuildingFlag, Part, Unit};
use regent_core::data::fourcc::FourCc;

#[test]
fn a_flag_patch_keeps_the_winning_unit_record() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let id = FourCc(*b"ABe1");
    let at = catalogue.find(b"DUNT", id).expect("house");
    let original = catalogue.record(b"DUNT", at).expect("house record");
    let files = [(
        PathBuf::from("house.toml"),
        "version = 1\n[[unit]]\nid = 'ABe1'\nfield = 'building'\nadd = ['Heals']\n".to_owned(),
    )];

    let bytes = flags::compile(&catalogue, &files).expect("patch");
    let archive = cam::Archive::parse(&bytes).expect("patch archive");
    let patched = archive
        .chunk_by_tag(b"DUNT")
        .expect("units")
        .records
        .iter()
        .find(|r| r.id == id)
        .expect("house");
    let unit = Unit::parse(&Part::parse(patched.data).expect("part")).expect("unit");

    assert!(unit.building().expect("building").has(BuildingFlag::Heals));
    assert_eq!(original.data.len(), patched.data.len());
    assert_eq!(
        original
            .data
            .iter()
            .zip(patched.data)
            .filter(|(a, b)| a != b)
            .count(),
        1
    );
}

#[test]
fn patches_compose_and_reject_bad_input() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let file = |text: &str| [(PathBuf::from("flags.toml"), text.to_owned())];
    let base = "version = 1\n[[unit]]\nid = 'ABe1'\nfield = 'building'\n";

    let bytes = flags::compile(
        &catalogue,
        &file(&format!(
            "{base}add = ['Heals']\n[[unit]]\nid = 'ABe1'\nfield = 'building'\nclear = ['Heals']\n"
        )),
    )
    .expect("composed patch");
    let archive = cam::Archive::parse(&bytes).expect("archive");
    let record = archive
        .chunk_by_tag(b"DUNT")
        .expect("units")
        .records
        .iter()
        .find(|r| r.id == FourCc(*b"ABe1"))
        .expect("house");
    let unit = Unit::parse(&Part::parse(record.data).expect("part")).expect("unit");
    assert!(!unit.building().expect("building").has(BuildingFlag::Heals));

    for (text, reason) in [
        (
            "version = 1\n[[unit]]\nid = 'zzzz'\nfield = 'building'\nadd = ['Heals']\n".to_owned(),
            "unknown unit",
        ),
        (format!("{base}add = ['NoSuchFlag']\n"), "unknown flag"),
        (
            "version = 1\n[[unit]]\nid = 'ABe1'\nfield = 'character'\nadd = ['Heals']\n".to_owned(),
            "wrong unit type",
        ),
    ] {
        assert!(
            flags::compile(&catalogue, &file(&text))
                .expect_err("bad patch accepted")
                .to_string()
                .contains(reason)
        );
    }
}

#[test]
fn a_patch_keeps_another_mods_house_hp() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let id = FourCc(*b"ABe1");
    let at = catalogue.find(b"DUNT", id).expect("house");
    let original = catalogue.record(b"DUNT", at).expect("record");
    let name = original.name();
    let mut changed = original.data.to_vec();
    let part = Part::parse(&changed).expect("part");
    let secd = part.find(b"SECD").expect("building");
    let hp = secd.body.as_ptr() as usize - changed.as_ptr() as usize + 0x1c;
    changed[hp..hp + 4].copy_from_slice(&88_u32.to_le_bytes());

    let earlier = cam::write(&[Written {
        tag: FourCc(*b"DUNT"),
        flag: ChunkFlag::Clear,
        records: vec![(id, name.as_ref(), original.data)],
    }]);
    let other_mod = cam::write(&[Written {
        tag: FourCc(*b"DUNT"),
        flag: ChunkFlag::Clear,
        records: vec![(id, name.as_ref(), &changed)],
    }]);
    let base = Catalogue::parse([
        ("base", earlier.as_slice()),
        ("other mod", other_mod.as_slice()),
    ])
    .expect("composed catalogue");
    let files = [(
        PathBuf::from("flags.toml"),
        "version = 1\n[[unit]]\nid = 'ABe1'\nfield = 'building'\nadd = ['Heals']\n".to_owned(),
    )];
    let patch = flags::compile(&base, &files).expect("patch");
    let archive = cam::Archive::parse(&patch).expect("archive");
    let record = &archive.chunk_by_tag(b"DUNT").expect("units").records[0];
    let unit = Unit::parse(&Part::parse(record.data).expect("part")).expect("unit");

    assert_eq!(unit.building().expect("building").max_hp(), Some(88));
    assert!(unit.building().expect("building").has(BuildingFlag::Heals));
}
