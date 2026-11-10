//! Engine names for save attributes, against the binaries.
//!
//! [`regent_core::data::attrib`] came from the executables' registration code.
//!
//! `Stat::ALL` (32 entries) comes from saves alone, an answer key independent
//! of the binary: see [`the_engines_names_agree_with_the_joins`].

mod common;

use regent_core::data::attrib;
use regent_core::data::stat::Stat;

/// `bank:index` is unique, so it can key a save's properties.
#[test]
fn no_attribute_is_keyed_twice() {
    let mut keys: Vec<(&[u8; 3], u8)> = attrib::EXPANSION
        .iter()
        .map(|(bank, at, _)| (*bank, *at))
        .collect();
    let before = keys.len();

    keys.sort_unstable();
    keys.dedup();

    assert_eq!(keys.len(), before, "an attribute is keyed twice");
}

/// The answer key against what the engine calls each stat.
///
/// All 32. `APB:0` `MaxGuildMembers` is the one position-as-index could not
/// place.
#[test]
fn the_engines_names_agree_with_the_joins() {
    // Engine names that differ from ours; the rest match outright.
    let renamed = |stat: Stat| -> Option<&'static str> {
        Some(match stat {
            Stat::Attack => "HtoH",
            Stat::RangedAttack => "Ranged",
            Stat::ExperienceNeeded => "LevelXP",
            Stat::ExperienceEarned => "Experience",
            Stat::Level => "ExperienceLevel",
            Stat::AttackRangeMin => "MinAttackRange",
            Stat::AttackRangeMax => "MaxAttackRange",
            Stat::WeaponDamage => "Weapon_Basic_Damage",
            Stat::WeaponUpgrades => "Weapon_Struct_Bonus",
            Stat::ArmorDamage => "Armor_Basic_Damage",
            Stat::ArmorUpgrades => "Armor_Struct_Bonus",
            Stat::TaxedGold => "StoredGold",
            Stat::Activity => "AIIntentionString",
            Stat::HealingPotions => "NumHealingPotions",
            Stat::StoneSkin => "HasEffectStoneSkin",
            Stat::TaxMinimumReturn => "ReturnAmount",
            Stat::TaxMinimumPickup => "PickupAmount",
            Stat::HitPoints => "HP",
            Stat::MaxHitPoints => "MaxHP",
            _ => return None,
        })
    };

    let mut checked = 0;

    for (stat, bank, index, _) in Stat::ALL {
        let theirs = attrib::name(*bank, index).unwrap_or_else(|| {
            panic!("{stat:?} is {bank:?}:{index}, which the engine never names")
        });
        let ours = renamed(stat).map_or_else(|| format!("{stat:?}"), ToOwned::to_owned);

        assert_eq!(theirs, ours, "{stat:?} is {bank:?}:{index}");
        checked += 1;
    }

    assert_eq!(checked, 32, "a stat went missing from the answer key");
}

/// `APV:n+100` is `MagDelta<name of APV:n>`, for exactly ten statistics.
///
/// Saves gave the offset independently. No `APV:121`
/// exists for the armour the same ring gave.
#[test]
fn a_modifier_is_named_for_what_it_modifies() {
    for (stat, bank, index, _) in Stat::ALL {
        if bank != b"APV" || index >= 10 {
            continue;
        }

        let base = attrib::name(*bank, index).expect("a statistic is mapped");
        let delta = attrib::name(*bank, index + 100).expect("so is its modifier");

        assert_eq!(delta, format!("MagDelta{base}"), "{stat:?}");
    }

    // Nothing past the ten.
    assert!(attrib::name(*b"APV", 109).is_some());
    assert!(attrib::name(*b"APV", 110).is_none());
    assert!(attrib::name(*b"APV", 121).is_none());
}

/// Banks are sparse: `APB` skips 38, `AP` has no 0, `APM` runs 0, 32, 64.
#[test]
fn a_banks_gaps_have_no_name() {
    assert_eq!(attrib::name(*b"APB", 37), Some("ResearchMarketDay"));
    assert_eq!(attrib::name(*b"APB", 38), None);
    assert_eq!(attrib::name(*b"APB", 39), Some("ResearchHealingPotions"));

    assert_eq!(attrib::name(*b"AP\0", 0), None);
    assert_eq!(attrib::name(*b"AP\0", 1), Some("NameComponents"));

    assert_eq!(attrib::name(*b"APM", 0), Some("Message_Text"));
    assert_eq!(attrib::name(*b"APM", 1), None);
    assert_eq!(attrib::name(*b"APM", 32), Some("Message_Text_Replace_Int"));

    // An inventory slot's index names an item, not an attribute.
    assert_eq!(attrib::name(*b"INV", 11), None);

    // Two-letter banks are NUL-padded; the padding is part of the key.
    assert_eq!(attrib::name(*b"AE\0", 0), Some("AgentID"));
    assert_eq!(attrib::name(*b"AE ", 0), None, "a space is not a NUL");
}

/// `APB:56` rules out position-as-index.
///
/// Position-as-index put `APB` at 11..=66, so the corpus's 71 `APB:56` entries
/// fell past the end onto `Strength`. The registrations end the bank at 56.
#[test]
fn the_building_bank_reaches_its_last_entry() {
    assert_eq!(attrib::name(*b"APB", 56), Some("ButtonStartTime"));
    assert_eq!(attrib::name(*b"APB", 57), None);

    let building = attrib::EXPANSION
        .iter()
        .filter(|(bank, _, _)| *bank == b"APB")
        .count();

    assert_eq!(
        building, 56,
        "one index is skipped, so 57 keys are 56 entries"
    );
}

/// A key serialises as the `(bank, index)` pair saves held before it was a
/// type, so save bytes and digests stay.
#[test]
fn a_key_serialises_as_its_pair() {
    use regent_core::base::digest::digest;

    assert_eq!(digest(&attrib::EXPERIENCE_LEVEL), digest(&(*b"APV", 11_u8)));
    assert_ne!(digest(&attrib::EXPERIENCE_LEVEL), digest(&(*b"APV", 12_u8)));
}
