//! Combat, death and spells in the world, checked against the five-day
//! probe trace: hit points as scripts and the periodic tick change them,
//! the death script a write below 1 hit point dispatches, a character's
//! spells and the builtins that read them.
//!
//! Needs `MAJESTY_HD`.

mod common;

use regent_core::data::desc::{Action, Part, Unit};
use regent_core::data::fourcc::FourCc;

/// Every `DUNT` and `DACT` of HD's archives.
fn parts(tag: [u8; 4]) -> Vec<Vec<u8>> {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");

    catalogue
        .tagged(&tag)
        .map(|(_, r)| r.data.to_vec())
        .collect()
}

/// Every HD character's `SECD` ends with its fourth list: an empty
///  list, then `AllowedWeapon`, `AllowedArmor`, `AllowedSpells`,
/// each a 1, a count and index and value pairs. A wizard carries a staff,
/// type 8, and the SDK's five spells in order.
#[test]
fn every_characters_lists_end_its_secd() {
    let mut characters = 0;
    let mut wizard = None;

    for data in parts(*b"DUNT") {
        let part = Part::parse(&data).expect("a descriptor");
        let unit = Unit::parse(&part).expect("a unit");

        if let Some(ch) = unit.character() {
            let lists = ch.allowed();

            assert!(lists.first.is_empty(), "{:?}", unit.head.id);
            characters += 1;

            if &unit.head.id.0 == b"AVN1" {
                wizard = Some(lists);
            }
        }
    }

    let wizard = wizard.expect("the wizard");
    let spells: Vec<&[u8; 4]> = wizard.spells.iter().map(|(_, id)| &id.0).collect();

    assert!(characters > 100, "{characters}");
    assert_eq!(wizard.weapons, [(0, 8)]);
    assert_eq!(wizard.armor.len(), 0);
    assert_eq!(spells, [b"WRa1", b"WRe1", b"WRb1", b"WRf1", b"WRd1"]);
}

/// `teleport` asks `teleport_check` before `GetBestSpell` keeps it;
/// `energy_blast` asks nothing.
#[test]
fn a_spells_validation_script_is_read_from_its_secd() {
    let checks: Vec<(FourCc, Option<String>)> = parts(*b"DACT")
        .iter()
        .filter_map(|data| {
            let action = Action::parse(&Part::parse(data).ok()?).ok()?;
            let check = action
                .validation_script()
                .map(|s| String::from_utf8_lossy(s).into_owned());

            Some((action.head.id, check))
        })
        .collect();
    let of = |id: &[u8; 4]| {
        checks
            .iter()
            .find(|(c, _)| &c.0 == id)
            .and_then(|(_, s)| s.clone())
    };

    assert_eq!(of(b"WRd1").as_deref(), Some("teleport_check"));
    assert_eq!(of(b"WRa1"), None);
}

/// HD's descriptors repeat one `HEAD` name, `speed_tonic_icon`, at `XR01`
/// and `XS01`; a lookup by name takes `XR01`, the lower code
/// (`world::body_names`).
#[test]
fn one_descriptor_name_repeats_and_the_lower_code_wins() {
    let bodies = common::bodies();
    let mut codes = std::collections::BTreeMap::<String, Vec<[u8; 4]>>::new();

    for (code, body) in &bodies {
        codes
            .entry(body.name.to_ascii_lowercase())
            .or_default()
            .push(code.0);
    }

    codes.retain(|_, c| c.len() > 1);

    for c in codes.values_mut() {
        c.sort_unstable();
    }

    let repeated: Vec<(&str, &[[u8; 4]])> = codes
        .iter()
        .map(|(name, c)| (name.as_str(), c.as_slice()))
        .collect();

    assert_eq!(repeated, [("speed_tonic_icon", &[*b"XR01", *b"XS01"][..])]);
    assert_eq!(
        regent_core::sim::world::bodies::body_names(&bodies).get("speed_tonic_icon"),
        Some(&regent_core::data::fourcc::FourCc(*b"XR01"))
    );
}
