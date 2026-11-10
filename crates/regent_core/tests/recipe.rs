//! What a land hands the generator ([`regent_core::sim::mapgen::recipe`]), against both
//! freestyle quests, both charts and the saves.

mod common;

use std::path::PathBuf;

use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::{self, Chart, Map};
use regent_core::sim::mapgen::recipe::Land;

fn read(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).expect("a quest")
}

fn map(bytes: &[u8]) -> Map<'_> {
    rgs::File::parse(bytes)
        .expect("the framing")
        .map()
        .expect("the map body")
}

/// Both charts; a quest may name a pattern from either.
fn charts() -> Vec<Vec<u8>> {
    let root = common::hd();

    ["Data/constants.rgs", "DataMX/mx_constants.rgs"]
        .iter()
        .map(|rel| read(&root.join(rel)))
        .collect()
}

/// A freestyle land's zones are its region's team items, each with a share.
/// [`Land`] resolves every land in both freestyle quests.
#[test]
fn a_freestyle_lands_zones_are_its_regions_team_items() {
    /// Named regions per freestyle quest. The dialog offers only those with
    /// `active` and `expanding` set; all must resolve.
    const FREESTYLE: [(&str, usize); 2] =
        [("Quests/Freestyle.q", 29), ("QuestsMX/mx_Freestyle.q", 35)];

    /// Spot-checked zone counts.
    const NAMED: [(&[u8; 4], usize); 4] = [(b"KxHa", 2), (b"Rave", 3), (b"Valm", 5), (b"Mayh", 6)];

    let root = common::hd();
    let raw = charts();
    let parsed: Vec<Chart<'_>> = raw
        .iter()
        .filter_map(|b| rgs::File::parse(b).ok()?.chart().ok())
        .collect();

    for (rel, lands) in FREESTYLE {
        let bytes = read(&root.join(rel));
        let quest = map(&bytes);
        let codes: Vec<FourCc> = quest.used_regions().map(|r| r.code).collect();

        assert_eq!(
            codes.len(),
            lands,
            "{rel} holds a different number of lands"
        );

        for code in codes {
            let land = Land::of(&quest, code)
                .unwrap_or_else(|| panic!("{rel}: land {code} does not resolve to zones"));

            assert!(!land.zones.is_empty(), "{rel}: land {code} has no zones");

            // All three zone references resolve in one chart.
            for zone in &land.zones {
                let found = parsed.iter().any(|c| {
                    zone.profile(c).is_some()
                        && zone.ground(c).is_some()
                        && zone.scatter(c).is_some()
                });

                assert!(found, "{rel}: zone {} resolves in no chart", zone.code);
            }

            // Truncation loses up to one per mille a zone; six zones at most.
            let share = land.shares().iter().sum::<u32>();

            assert!(
                (994..=1_000).contains(&share),
                "{code}: shares sum to {share} per mille"
            );

            if let Some((_, want)) = NAMED.iter().find(|(c, _)| code == *c) {
                assert_eq!(land.zones.len(), *want, "{code} changed zone count");
            }
        }
    }
}

/// A zone's seed count is its region item's first word, plus one.
///
/// Region seed place plants `entry` plus one seeds
/// per zone, via distribution seeds. The entry is the
/// region's `SpawnItem` (`+4` code, `+8` level), so a field is `words[0]`:
/// `ZoneSpec::seeds`.
///
/// Zones with many seeds come out in several patches.
#[test]
fn a_zones_seed_count_is_its_items_first_word_plus_one() {
    /// Zones with more than one seed. Same in both quests.
    const MANY: [(&[u8; 4], u32); 11] = [
        (b"$Rac", 21),
        (b"$Thb", 6),
        (b"$Tha", 11),
        (b"$Vac", 5),
        (b"$Fea", 7),
        (b"$Fel", 4),
        (b"$Rub", 3),
        (b"$Rui", 7),
        (b"$Cha", 15),
        (b"$Chb", 9),
        (b"$Shb", 16),
    ];

    let root = common::hd();

    for rel in ["Quests/Freestyle.q", "QuestsMX/mx_Freestyle.q"] {
        let bytes = read(&root.join(rel));
        let quest = map(&bytes);
        let mut many: Vec<(FourCc, u32)> = Vec::new();
        let mut one = 0;

        for region in quest.used_regions() {
            let team = region.team.as_ref().expect("a named region has a team");

            // Only lands the dialog offers.
            if team.active != 1 || team.expanding != 1 {
                continue;
            }

            let land = Land::of(&quest, region.code).expect("the land");

            for zone in &land.zones {
                if zone.seeds > 1 {
                    many.push((zone.code, zone.seeds));
                } else {
                    one += 1;
                }
            }
        }

        let want: Vec<(FourCc, u32)> = MANY
            .iter()
            .map(|(c, n)| (FourCc::new(c), *n))
            .filter(|(c, _)| many.iter().any(|(had, _)| had == c))
            .collect();

        assert_eq!(many, want, "{rel}: the seed counts moved");
        assert!(one > 20, "{rel}: only {one} zones ask for one seed");
    }
}
