//! Quest maps (`.q`) against HD, and their joins through the charts.
//!
//! Needs `MAJESTY_HD` (environment or `.env`).

mod common;

use regent_core::content::terrain::grid;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::map::{ENEMY_FORCES, WANDERING_MONSTERS};
use regent_core::data::rgs::{self, Chart, Map, chart::Layer};
use regent_core::sim::freestyle::{
    self, Band, Forces, List, ONE_PLAYER, Play, RANDOM_LAND, Ratings, Victory,
};
use regent_core::sim::start::NEUTRAL;

/// Null reference: an empty zone slot.
const NONE: &[u8; 4] = b"NONE";

/// Every shipped quest, base and expansion.
fn quests() -> Vec<(String, PathBuf)> {
    let root = common::hd();
    let mut out = Vec::new();

    for dir in ["Quests", "QuestsMX"] {
        for path in common::walk(&root.join(dir), &["q"]) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();

            out.push((name.into_owned(), path));
        }
    }

    out.sort();
    out
}

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

/// Every pattern and profile code in either chart, with layer and name.
fn defined(raw: &[Vec<u8>]) -> BTreeMap<FourCc, (Option<Layer>, String)> {
    let mut out = BTreeMap::new();

    for bytes in raw {
        let chart: Chart<'_> = rgs::File::parse(bytes).unwrap().chart().unwrap();

        for (layer, pattern) in chart.patterns() {
            out.entry(pattern.code).or_insert((
                Some(layer),
                String::from_utf8_lossy(pattern.name).into_owned(),
            ));
        }

        for profile in &chart.profiles {
            out.entry(profile.code)
                .or_insert((None, String::from_utf8_lossy(profile.name).into_owned()));
        }
    }

    out
}

/// A `.q` has no offsets, so ending short of the last byte means a misread
/// field.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_shipped_quest_reads_to_its_last_byte() {
    let found = quests();

    assert_eq!(found.len(), 36, "the install ships 36 quests");

    let mut versions: BTreeMap<String, usize> = BTreeMap::new();

    for (name, path) in &found {
        let bytes = read(path);
        let file = rgs::File::parse(&bytes).expect("the framing");

        // Fails unless the body is consumed exactly.
        file.map().unwrap_or_else(|e| panic!("{name}: {e}"));

        *versions.entry(file.tag.to_string()).or_default() += 1;
    }

    assert_eq!(
        versions,
        BTreeMap::from([
            ("RGM6".to_owned(), 19),
            ("RGM9".to_owned(), 13),
            ("RGMa".to_owned(), 4),
        ]),
        "the version spread"
    );
}

/// A profile's falloff is `2^(-hurst/2)`: two words, one setting.
///
/// **Inferred** from the fit: midpoint displacement in half steps. Within 0.4%,
/// never exact, so both words are authored, not computed at load.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_profiles_falloff_follows_from_its_hurst() {
    /// How far off `2^(-hurst/2)` the worst profile sits.
    const SLACK: f64 = 0.004;

    let raw = charts();
    let mut seen = 0;

    for bytes in &raw {
        let chart = rgs::File::parse(bytes)
            .expect("the framing")
            .chart()
            .expect("the chart body");

        for profile in &chart.profiles {
            let want = 2.0f64.powf(-profile.hurst() / 2.0);
            let off = (profile.falloff() - want).abs() / want;

            assert!(
                off < SLACK,
                "{}: falloff {} against 2^(-{}/2) = {want}, off by {off}",
                String::from_utf8_lossy(profile.name),
                profile.falloff(),
                profile.hurst(),
            );

            // Both are ratios. The hurst range is observed, not a rule.
            assert!((0.0..1.0).contains(&profile.falloff()));
            assert!((0.1..1.0).contains(&profile.hurst()));

            seen += 1;
        }
    }

    assert_eq!(seen, 23, "nine base profiles and fourteen expansion ones");
}

/// Every quest is square, at one of the editor's three sizes.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_quest_is_a_square_map_of_a_offered_size() {
    let mut sizes: BTreeMap<u32, usize> = BTreeMap::new();

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        assert_eq!(quest.size.0, quest.size.1, "{name} is not square");
        assert!(
            [128, 256, 512].contains(&quest.size.0),
            "{name} is {} across",
            quest.size.0
        );

        *sizes.entry(quest.size.0).or_default() += 1;
    }

    assert_eq!(
        sizes,
        BTreeMap::from([(128, 17), (256, 19)]),
        "no shipped quest uses the largest size the editor offers"
    );
}

/// A zone's landscape, surface and height codes all exist in a chart. A quest
/// never names a `DTPT` class or `DUNT` object directly.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn every_zone_names_chart_patterns_that_exist() {
    let raw = charts();
    let known = defined(&raw);
    let mut checked = 0;

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        for zone in &quest.zones {
            for reference in [zone.landscape, zone.surface, zone.height] {
                if reference == FourCc::new(NONE) {
                    continue;
                }

                assert!(
                    known.contains_key(&reference),
                    "{name}: zone {} names {reference}, which no chart defines",
                    String::from_utf8_lossy(zone.name)
                );

                checked += 1;
            }
        }
    }

    assert!(checked > 900, "only {checked} references checked");
}

/// A zone's landscape and surface agree on a biome: one themed preset, e.g.
/// `#Day_Plains` over `#Plains_All_Parent`.
///
/// Skipped:
///
/// - Dirt surfaces: every biome sits on dirt.
/// - `#DEMO_` zones (8): scorched clumps over plains, the same six patterns that
///   reference the undefined `CLER`. Leftover example data.
/// - Landscapes named after places rather than a biome.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_zones_landscape_and_surface_agree_on_a_biome() {
    const BIOMES: [&str; 4] = ["plains", "swamp", "snow", "scorched"];

    let raw = charts();
    let known = defined(&raw);
    let mut checked = 0;
    let mut leftovers = 0;

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        for zone in &quest.zones {
            let (Some((_, landscape)), Some((_, surface))) =
                (known.get(&zone.landscape), known.get(&zone.surface))
            else {
                continue;
            };

            // Case folded: e.g. `%Legend_Snow` over `%snow`.
            let (low_landscape, low_surface) = (landscape.to_lowercase(), surface.to_lowercase());

            if low_surface.contains("dirt") {
                continue;
            }

            if low_landscape.contains("#demo_") {
                leftovers += 1;
                continue;
            }

            for biome in BIOMES {
                if low_landscape.contains(biome) {
                    assert!(
                        low_surface.contains(biome),
                        "{name}: {landscape} is scattered over {surface}"
                    );

                    checked += 1;
                }
            }
        }
    }

    assert!(checked > 80, "only {checked} zones were nameable");
    assert_eq!(leftovers, 8, "the #DEMO_ zones, skipped as leftovers");
}

/// A placement names blocks of a 5x5 grid as letters `A` to `Y`.
///
/// Evidence: Wild Expanse places `Icex` (zone `$Dtar_Snow`) at `M`, the centre.
/// Four Wild Expanse saves on four lands that cannot paint snow each have a
/// snow square on cells 204 to 306 of 512, exactly block `(2, 2)`.
///
/// Non-letter cells are a bare 0, 1, 3, 5 or 7, on monster camps (`Dard`,
/// `Ania`, `Anib`) with `word` 5. Meaning unknown.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_placement_names_blocks_of_a_five_by_five_grid() {
    let mut seen: BTreeSet<u8> = BTreeSet::new();
    let (mut placements, mut odd, mut empty) = (0, 0, 0);

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        for unit in quest.units.iter().flatten() {
            for place in &unit.placements {
                empty += usize::from(place.cells.is_empty());

                let letters = place
                    .cells
                    .iter()
                    .filter(|c| c.is_ascii_uppercase())
                    .count();

                for cell in place.cells {
                    if (b'A'..=b'Y').contains(cell) {
                        seen.insert(*cell);
                    } else {
                        assert!(
                            [0, 1, 3, 5, 7].contains(cell),
                            "{name}: placement {} names cell {cell}, neither a letter \
                             nor one of the five bare numbers",
                            place.id
                        );
                        odd += 1;
                    }
                }

                // Never mixed.
                assert!(
                    letters == place.cells.len() || letters == 0,
                    "{name}: placement {} mixes letters and bare numbers",
                    place.id
                );
                assert_eq!(
                    place.blocks().count(),
                    letters,
                    "{name}: a letter did not resolve to a block"
                );
                placements += 1;
            }
        }
    }

    // All 25 letters are used.
    assert_eq!(
        seen.len(),
        25,
        "only {} of the 25 blocks are named",
        seen.len()
    );
    assert!(placements > 400, "only {placements} placements");
    assert_eq!(odd, 170, "the bare-number placements moved");
    assert_eq!(empty, 4, "the placements naming no cell at all moved");

    // Reading order from the top left, before the map's orientation turns it.
    let block = |letter: u8| {
        let place = rgs::map::Placement {
            id: FourCc::new(b"NONE"),
            word: 0,
            description: &[],
            cells: std::slice::from_ref(&letter),
        };

        place.blocks().next().expect("a block")
    };

    assert_eq!(block(b'A'), (0, 0), "A is the top left");
    assert_eq!(block(b'E'), (4, 0), "E is the top right");
    assert_eq!(block(b'M'), (2, 2), "M is the centre");
    assert_eq!(block(b'U'), (0, 4), "U is the bottom left");
    assert_eq!(block(b'Y'), (4, 4), "Y is the bottom right");

    // The centre is fixed under every turn, so Wild Expanse saves cannot
    // reveal the turn.
    for turn in rgs::map::Orientation::ALL {
        assert_eq!(turn.turn((2, 2)), (2, 2), "{turn:?} moved the centre");
    }

    // Each turn permutes the grid.
    for turn in rgs::map::Orientation::ALL {
        let moved: BTreeSet<(u32, u32)> = (0..25).map(|i| turn.turn((i % 5, i / 5))).collect();

        assert_eq!(moved.len(), 25, "{turn:?} is not a permutation");
    }

    // The Wild Expanse snow square.
    assert_eq!(rgs::map::Placement::span(2, 512), (204, 306));
}

/// The eight turns form the square's symmetry group on any span.
///
/// `Orientation::on_span` generalises the 5x5 `turn` to map coordinates.
/// Checked: agreement with `turn`, permutation, inverse, composition.
#[test]
fn the_eight_placement_turns_are_a_group_on_any_span() {
    use rgs::map::Orientation;

    const SPANS: [u32; 3] = [4, 128, 512];
    let identity = Orientation(0);

    for turn in Orientation::ALL {
        for at in (0..25).map(|i: u32| (i % 5, i / 5)) {
            assert_eq!(turn.turn(at), turn.on_span(at, 4), "{turn:?} moved a block");
        }

        for span in SPANS {
            let moved: BTreeSet<(u32, u32)> = (0..=span)
                .flat_map(|y| (0..=span).map(move |x| (x, y)))
                .map(|at| turn.on_span(at, span))
                .collect();

            assert_eq!(
                moved.len() as u64,
                u64::from(span + 1) * u64::from(span + 1),
                "{turn:?} is not a permutation of a {span} span"
            );

            for at in [(0, 0), (1, 2), (span, 3), (span / 2, span)] {
                assert_eq!(
                    turn.inverse().on_span(turn.on_span(at, span), span),
                    at,
                    "{turn:?} is not undone by its inverse"
                );
            }
        }

        assert_eq!(turn.after(identity), turn, "{turn:?} moved after nothing");
        assert_eq!(identity.after(turn), turn, "nothing moved after {turn:?}");
        assert_eq!(
            turn.after(turn.inverse()),
            identity,
            "{turn:?} after its inverse is not the identity"
        );

        for second in Orientation::ALL {
            let composed = second.after(turn);

            for at in [(1, 2), (3, 7), (0, 10)] {
                assert_eq!(
                    composed.on_span(at, 10),
                    second.on_span(turn.on_span(at, 10), 10),
                    "{second:?} after {turn:?} is not {composed:?}"
                );
            }
        }
    }
}

/// A Player Forces entry's first word is its player count.
///
/// Settled by singular against plural names: "Fortified Palace" is 1,
/// "Fortified Palaces" 2. Single-player shows only the 1s.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_player_forces_entry_says_how_many_players_it_is_for() {
    /// Entry names and their player counts.
    const PAIRS: [(&str, u32); 8] = [
        ("Frontier Settlement (Advanced)", 1),
        ("Frontier Settlements (Advanced)", 2),
        ("Frontier Keeps (Advanced)", 3),
        ("Frontier Towns (Advanced)", 4),
        ("Fortified Palace (Advanced)", 1),
        ("Fortified Palaces (Advanced)", 2),
        ("Merchant Prince (Expert)", 1),
        ("Wizard Hold (Beginner)", 1),
    ];

    let root = common::hd();
    let bytes = read(&root.join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let mut found: BTreeMap<String, u32> = BTreeMap::new();
    let mut single = 0;

    for unit in &quest.units[1] {
        let Some(players) = unit.players() else {
            continue;
        };

        found.insert(String::from_utf8_lossy(unit.name).into_owned(), players);
        single += usize::from(players == 1);
    }

    for (name, want) in PAIRS {
        assert_eq!(
            found.get(name),
            Some(&want),
            "{name} is not a {want} player entry"
        );
    }

    // Sized Enemy Forces entries hold a kind (9) there, not a count.
    for unit in &quest.units[1] {
        if unit.name.ends_with(b"- S") || unit.name.ends_with(b"- L") || unit.name.ends_with(b"- H")
        {
            assert_eq!(unit.players(), None, "a scenario read as a player count");
            assert_eq!(unit.words[0], 9);
        }
    }

    assert_eq!(single, 18, "the single-player settlements moved");
}

/// The single-player dialog offers exactly the Enemy Forces entries with
/// `trailer[3] == 1`.
///
/// **Game**: `OFFERED` is the dialog's list, written down from the game,
/// as its entries' codes, in the dialog's order.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialog_offers_a_scenario_when_its_trailer_says_one_player() {
    const OFFERED: [&[u8; 4]; 25] = [
        b"Ruib", b"Ratx", b"Skea", b"Ench", b"Gobb", b"Gobe", b"Unde", b"Curs", b"dliW", b"evaR",
        b"Gobc", b"Shad", b"Gobg", b"Swam", b"Gorg", b"Rest", b"Cona", b"Necr", b"Sera", b"Drag",
        b"Sorc", b"Serb", b"Wint", b"Gobf", b"Drad",
    ];

    let offered: BTreeSet<FourCc> = OFFERED.iter().map(|c| FourCc::new(c)).collect();

    let root = common::hd();
    let bytes = read(&root.join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let (mut shown, mut withheld) = (0, 0);

    for unit in &quest.units[1] {
        // Sized Enemy Forces entries only.
        if unit.words[0] != 9 || !unit.name.windows(3).any(|w| w == b" - ") {
            continue;
        }

        let flag = unit.trailer.expect("a sized scenario has a trailer")[3];

        assert_eq!(
            offered.contains(&unit.code),
            flag == 1,
            "{}: the dialog and trailer[3] = {flag} disagree",
            unit.code
        );

        shown += usize::from(flag == 1);
        withheld += usize::from(flag != 1);
    }

    assert_eq!((shown, withheld), (25, 30), "the Enemy Forces list moved");
    assert_eq!(offered.len(), OFFERED.len(), "a code listed twice");
}

/// `trailer[3..7]` is a mask, one flag per player count 1 to 4.
///
/// No scenario has all four clear, so the 30 the single-player dialog withholds
/// are for more players. Eight of sixteen masks occur.
///
/// The four scenarios whose trailers collide are all multiplayer, so no
/// single-player save tells them apart.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_trailers_last_four_words_are_a_player_count_mask() {
    /// Four scenarios whose trailers collide in pairs, and who may play them.
    const MULTIPLAYER: [(&str, [u32; 4]); 4] = [
        ("Goblin Raiders (Advanced) - L", [0, 0, 0, 1]),
        ("Valley of Dread (Beginner) - L", [0, 0, 0, 1]),
        ("Rampage (Expert) - L", [0, 1, 0, 0]),
        ("Deathwind (Expert) - L", [0, 1, 0, 0]),
    ];

    /// Every mask that occurs, and how many scenarios carry it.
    const MASKS: [([u32; 4], usize); 8] = [
        ([0, 0, 0, 1], 9),
        ([0, 0, 1, 0], 7),
        ([0, 1, 0, 0], 9),
        ([0, 1, 1, 1], 5),
        ([1, 0, 0, 0], 15),
        ([1, 0, 1, 0], 1),
        ([1, 1, 0, 0], 7),
        ([1, 1, 1, 1], 2),
    ];

    let root = common::hd();
    let bytes = read(&root.join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let mut seen: BTreeMap<[u32; 4], usize> = BTreeMap::new();
    let mut named: BTreeMap<String, [u32; 4]> = BTreeMap::new();

    for unit in &quest.units[1] {
        // Sized Enemy Forces entries only.
        if unit.words[0] != 9 || !unit.name.windows(3).any(|w| w == b" - ") {
            continue;
        }

        let trailer = unit.trailer.expect("a sized scenario has a trailer");
        let mask = [trailer[3], trailer[4], trailer[5], trailer[6]];
        let name = String::from_utf8_lossy(unit.name).into_owned();

        assert!(mask.contains(&1), "{name}: no player count reaches it");

        *seen.entry(mask).or_default() += 1;
        named.insert(name, mask);
    }

    for (name, mask) in MULTIPLAYER {
        assert_eq!(
            named.get(name),
            Some(&mask),
            "{name} changed who may play it"
        );
        assert_eq!(mask[0], 0, "{name} is offered to one player after all");
    }

    assert_eq!(
        seen,
        MASKS.into_iter().collect::<BTreeMap<_, _>>(),
        "the player-count masks moved"
    );
    assert_eq!(seen.values().sum::<usize>(), 55, "the scenario count moved");
}

/// `alternate` is a flag; in the freestyle quests two blocks carry it.
///
/// **Corpus**: 0 or 1 in every record of every quest; set, with a ground, on
/// `Fron` in both freestyle quests, `Icex` in the expansion's, and 40 records
/// of the story quests.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn two_freestyle_blocks_stamp_their_ground() {
    let mut stamping = BTreeSet::new();
    let mut story = 0;

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        for unit in quest.units.iter().flatten() {
            assert!(
                unit.alternate <= rgs::map::FLAG_SET,
                "{name}: {} has alternate {}",
                unit.faction,
                unit.alternate
            );

            match (unit.stamps(), name.to_lowercase().contains("freestyle")) {
                (true, true) => {
                    stamping.insert((name.clone(), unit.faction.to_string()));
                }
                (true, false) => story += 1,
                _ => {}
            }
        }
    }

    assert_eq!(story, 40, "the story quests' stamping blocks moved");

    let want: BTreeSet<(String, String)> = [
        ("Freestyle.q", "Fron"),
        ("mx_Freestyle.q", "Fron"),
        ("mx_Freestyle.q", "Icex"),
    ]
    .into_iter()
    .map(|(q, c)| (q.to_owned(), c.to_owned()))
    .collect();

    assert_eq!(stamping, want);
}

/// Expansion scenarios named `- S` that carry size bits 3, so the engine
/// gives them a 256-cell map.
const SIZED_OFF_NAME: [&[u8; 4]; 2] = [b"niuR", b"Gobg"];

/// A scenario's map size is the first size bit it carries, not its name.
///
/// **Corpus**: every sized Enemy Forces entry agrees with its name's letter
/// but [`SIZED_OFF_NAME`].
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_scenarios_map_size_is_the_first_size_bit_it_carries() {
    let root = common::hd();
    let bytes = read(&root.join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let mut off = BTreeSet::new();
    let mut sized = 0;

    for unit in &quest.units[1] {
        let letter = unit.name.trim_ascii_end().last().copied().unwrap_or(0);
        let named = grid::MAP_SIZES
            .iter()
            .find(|(l, _)| u8::try_from(*l).ok() == Some(letter));
        let scenario = unit.words[0] == 9 && unit.name.windows(3).any(|w| w == b" - ");

        if let (true, Some((_, side))) = (scenario, named) {
            sized += 1;

            if unit.side() != *side {
                off.insert(unit.faction);
            }
        }
    }

    assert_eq!(sized, 55, "the scenario count moved");
    assert_eq!(off, SIZED_OFF_NAME.into_iter().map(FourCc::new).collect());
}

/// A bare-number placement is an off-map lair, and its number the edge its
/// monsters enter by: 1, 3, 5, 7 clockwise from north.
///
/// **Corpus**: 170 bare cells over the quests,
/// each alone in its placement; all 1, 3, 5 or 7 but two 0s in
/// `clash_empires.q`, which the engine's `c - 1 < 8` refuses. In the
/// freestyle quests, 130, all in Wandering Monsters records.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_bare_number_is_the_edge_wandering_monsters_enter_by() {
    let (mut edges, mut zeros, mut freestyle) = (0, 0, 0);

    for (name, path) in quests() {
        let bytes = read(&path);
        let quest = map(&bytes);

        for unit in quest.units.iter().flatten() {
            for place in &unit.placements {
                if place.cells.is_empty() || place.cells.iter().all(u8::is_ascii_uppercase) {
                    assert_eq!(place.edge(), None);
                    continue;
                }

                assert_eq!(place.cells.len(), 1, "{name}: {} mixes cells", place.id);

                if place.edge().is_some() {
                    edges += 1;

                    if name.to_lowercase().contains("freestyle") {
                        freestyle += 1;
                        assert_eq!(unit.words[0], 10, "{name}: {} is no monster", unit.faction);
                    }
                } else {
                    assert_eq!(place.cells, [0], "{name}: {}", place.id);
                    zeros += 1;
                }
            }
        }
    }

    assert_eq!((edges, zeros, freestyle), (168, 2, 130));
}

/// The custom-game dialog's four lists from HD's freestyle quest: the
/// codes a setup writes, each with the dialog's own string.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialog_offers_each_list_with_its_names() {
    let bytes = read(&common::hd().join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let named = |list: List, code: &[u8; 4]| {
        freestyle::offers(&quest, list, ONE_PLAYER)
            .iter()
            .find(|o| o.code == FourCc::new(code))
            .map(|o| String::from_utf8_lossy(o.name).into_owned())
    };
    let counts = [List::Land, List::Settlement, List::Scenario, List::Monsters]
        .map(|list| freestyle::offers(&quest, list, ONE_PLAYER).len());

    assert_eq!(counts, [15, 18, 25, 15]);
    assert_eq!(named(List::Land, b"Rave").as_deref(), Some("Ravenswood"));
    assert_eq!(named(List::Land, b"KxHa").as_deref(), Some("K'Hal-Kratis"));
    assert_eq!(
        named(List::Land, b"Thea"),
        None,
        "a quest's region, not offered"
    );
    assert_eq!(
        named(List::Settlement, b"Wizb").as_deref(),
        Some("Wizard Hold (Beginner)")
    );
    assert_eq!(named(List::Settlement, b"Froa"), None, "four players");
    assert_eq!(
        named(List::Scenario, b"Ruib").as_deref(),
        Some("Ruins of Esterhausen (Advanced) - S")
    );
    assert_eq!(
        named(List::Monsters, b"Drac").as_deref(),
        Some("Dragon Raiders (Expert)")
    );
    assert_eq!(named(List::Monsters, b"Harb"), None, "multiplayer only");
}

/// The dialog's order: the lands as the world's region map holds them,
/// by code word; the forces by code word, then stably by
/// their rating, least first; the events as
/// `ENTX` holds them, `EVSC`'s order.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialog_orders_its_lists_as_the_engine() {
    let bytes = read(&common::hd().join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let codes = |list: List| -> Vec<FourCc> {
        freestyle::offers(&quest, list, ONE_PLAYER)
            .iter()
            .map(|o| o.code)
            .collect()
    };

    assert_eq!(
        codes(List::Land),
        freestyle::random_lands(&quest, ONE_PLAYER)
    );

    for list in [List::Settlement, List::Scenario, List::Monsters] {
        let rated: Vec<(i32, u32)> = codes(list)
            .into_iter()
            .map(|c| (Ratings::of(&quest, c).by(Victory::Eliminate), c.word()))
            .collect();

        assert!(rated.is_sorted(), "{list:?}: {rated:?}");
    }

    let first = |list: List| codes(list).first().copied();

    assert_eq!(first(List::Land), Some(FourCc::new(b"KxHa")));
    assert_eq!(first(List::Settlement), Some(FourCc::new(b"Wizb")));
    assert_eq!(first(List::Scenario), Some(FourCc::new(b"Ruib")));
    assert_eq!(first(List::Monsters), Some(FourCc::new(b"Skel")));

    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = regent_core::data::text::Text::build(&catalogue);
    let table = text.table(b"ENTX").expect("ENTX");
    let named: Vec<Option<FourCc>> = (0..table.len()).map(|i| table.key(i)).collect();
    let rated: Vec<Option<FourCc>> = regent_core::sim::setup::event_list(&catalogue)
        .into_iter()
        .map(|(code, _)| Some(code))
        .collect();

    assert_eq!(named, rated);
}

/// A Random pick is a function of the Map Number: drawn from its band,
/// the same twice; a code that is no tag stands.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_random_pick_is_drawn_from_the_map_number() {
    let bytes = read(&common::hd().join("QuestsMX/mx_Freestyle.q"));
    let map = map(&bytes);

    for band in Band::ALL {
        let asked = [RANDOM_LAND, band.tag(), band.tag(), band.tag()];

        for seed in 1..40 {
            let drawn = freestyle::random(&map, asked, seed, ONE_PLAYER).expect("drawn");

            assert_eq!(
                drawn,
                freestyle::random(&map, asked, seed, ONE_PLAYER).expect("again")
            );
            assert!(freestyle::random_lands(&map, ONE_PLAYER).contains(&drawn[0]));
            assert!(
                freestyle::randoms(&map, List::Settlement, band, ONE_PLAYER).contains(&drawn[1])
            );
            assert!(freestyle::randoms(&map, List::Scenario, band, ONE_PLAYER).contains(&drawn[2]));
            assert!(freestyle::randoms(&map, List::Monsters, band, ONE_PLAYER).contains(&drawn[3]));
        }
    }

    let fixed = [
        FourCc::new(b"KxHa"),
        FourCc::new(b"Wizb"),
        FourCc::new(b"Ruib"),
        FourCc::new(b"NONE"),
    ];

    assert_eq!(
        freestyle::random(&map, fixed, 5, ONE_PLAYER).expect("fixed"),
        fixed
    );
}

/// A freestyle placement's owner: in a network game of n players the
/// settlement's n placements are slots 0 to n - 1; alone, every player
/// block is slot 0; the scenario's and the monsters' are slot 7. Alone,
/// the quest's own slot lists (0 under slot 0, the rest slot 7) agree on
/// every one-player pair.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_freestyle_placements_slot_counts_player_blocks() {
    for rel in ["Quests/Freestyle.q", "QuestsMX/mx_Freestyle.q"] {
        let bytes = read(&common::hd().join(rel));
        let map = map(&bytes);
        let listed = |index: usize| {
            map.slots
                .iter()
                .position(|s| s.items.iter().any(|i| usize::try_from(*i) == Ok(index)))
                .and_then(|slot| i32::try_from(slot).ok())
                .unwrap_or(NEUTRAL)
        };
        let of_kind = |kind: u32, n: u32| {
            map.units[1]
                .iter()
                .filter(move |u| u.words[0] == kind && u.for_players(n))
        };
        let mut pairs = 0;

        for settlement in map.units[1].iter().filter(|u| u.players().is_some()) {
            let n = settlement.players().unwrap_or_default();
            let own = settlement.placements.len();
            let monsters = of_kind(WANDERING_MONSTERS, n).next();

            for scenario in of_kind(ENEMY_FORCES, n) {
                let forces = Forces {
                    settlement: settlement.faction,
                    scenario: scenario.faction,
                    monsters: monsters.map_or(FourCc::new(b"NONE"), |m| m.faction),
                };
                let rest = freestyle::template(&map, &forces).len() - own;
                let network = freestyle::seats(&map, &forces, Play::Network(n));
                let single = freestyle::seats(&map, &forces, Play::Single);
                let want: Vec<i32> = (0..)
                    .take(own)
                    .chain(std::iter::repeat_n(NEUTRAL, rest))
                    .collect();

                assert_eq!(own, n as usize, "{rel} {}", settlement.faction);
                assert_eq!(network, want, "{rel} {forces:?}");
                assert!(single[..own].iter().all(|s| *s == 0), "{rel} {forces:?}");
                assert_eq!(single[own..], want[own..], "{rel} {forces:?}");

                if n == 1 {
                    let lists: Vec<i32> = (0..single.len()).map(listed).collect();

                    assert_eq!(single, lists, "{rel} {forces:?}");
                }

                pairs += 1;
            }
        }

        assert!(pairs > 100, "{rel}: {pairs} pairs");
    }

    assert_eq!(Play::Single.humans(), 0..1);
    assert_eq!(Play::Network(3).humans(), 0..3);
    assert_eq!(Play::of(1), Play::Single);
    assert_eq!(Play::of(3), Play::Network(3));
}

/// A game of n players offers Player Forces entries for n, each with n
/// placements, and Enemy Forces and Wandering Monsters with the count's
/// flag; four players offer `Froa`, three and four `Harb`.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialog_offers_each_player_counts_forces() {
    let bytes = read(&common::hd().join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let entry = |code: FourCc| quest.units[1].iter().find(|u| u.faction == code);
    let offered = |list: List, n: u32| -> Vec<FourCc> {
        freestyle::offers(&quest, list, n)
            .iter()
            .map(|o| o.code)
            .collect()
    };

    for n in 1..=4 {
        let settlements = offered(List::Settlement, n);

        assert!(!settlements.is_empty(), "{n} players");
        assert!(!offered(List::Scenario, n).is_empty(), "{n} players");

        for code in settlements {
            assert_eq!(entry(code).map(|u| u.placements.len()), Some(n as usize));
        }
    }

    assert!(offered(List::Settlement, 4).contains(&FourCc::new(b"Froa")));
    let harb: Vec<u32> = (1..=4)
        .filter(|n| offered(List::Monsters, *n).contains(&FourCc::new(b"Harb")))
        .collect();

    assert_eq!(harb, [3, 4], "trailer flags 5 and 6");
}

/// Lands by count: `active` for one player, `expanding` for more. The two
/// flags agree on every shipped region, so each count offers the same
/// lands, and a Random land draws from them. Last Palace (`GMTX` 123) is
/// offered with more than one player; alone it turns back to Eliminate.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialog_offers_lands_and_victories_by_player_count() {
    let bytes = read(&common::hd().join("QuestsMX/mx_Freestyle.q"));
    let quest = map(&bytes);
    let lands = |n: u32| -> Vec<FourCc> {
        freestyle::offers(&quest, List::Land, n)
            .iter()
            .map(|o| o.code)
            .collect()
    };
    let flags: Vec<(u32, u32)> = quest
        .regions
        .iter()
        .filter_map(|r| r.team.as_ref())
        .map(|t| (t.active, t.expanding))
        .collect();

    assert!(flags.iter().all(|(active, expanding)| active == expanding));

    for n in 1..=freestyle::MOST_PLAYERS {
        let mut sorted = lands(n);

        sorted.sort_by_key(|c| c.word());
        assert_eq!(lands(n), lands(ONE_PLAYER), "{n} players");
        assert_eq!(freestyle::random_lands(&quest, n), sorted, "{n} players");
    }

    assert_eq!(Victory::offered(ONE_PLAYER), Victory::OFFERED);

    for n in 2..=freestyle::MOST_PLAYERS {
        assert_eq!(Victory::offered(n), Victory::ALL);
        assert_eq!(Victory::LastPalace.for_players(n), Victory::LastPalace);
    }

    assert_eq!(Victory::LastPalace.text(), 123);
    assert_eq!(
        Victory::LastPalace.for_players(ONE_PLAYER),
        Victory::Eliminate
    );
}
