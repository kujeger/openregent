//! The generator charts (`constants.rgs`), against HD.
//!
//! A `.q` names patterns here; patterns name terrain classes (`DTPT`) and
//! objects (`DUNT`). Checks that every reference resolves.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::desc::{self, Part};
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::{self, chart::Layer};

/// A null reference in the terrain layer.
const NONE: &[u8; 4] = b"NONE";

/// Referenced by six `#DEMO_*` object patterns and defined nowhere.
///
/// **Unknown**: an "empty spot" sentinel or a dangling reference to a cut
/// object.
const CLEAR: &[u8; 4] = b"CLER";

/// Every chart the install ships, base and expansion.
fn charts() -> Vec<(&'static str, PathBuf)> {
    let root = common::hd();

    [
        "Data/constants.rgs",
        "Data/constants_down.rgs",
        "Data/mx_constants_down.rgs",
        "DataMX/mx_constants.rgs",
        "DataMX/mx_constants_down.rgs",
    ]
    .into_iter()
    .map(|rel| (rel, root.join(rel)))
    .filter(|(_, p)| p.is_file())
    .collect()
}

fn read(path: &PathBuf) -> Vec<u8> {
    std::fs::read(path).expect("a chart")
}

/// The archives a chart's references point into.
fn corpus() -> Library {
    let root = common::hd();
    let mut files = Vec::new();

    // `data/` holds one expansion chart, so load both archive sets always.
    for dir in ["Data", "DataMX"] {
        for file in [
            "unittype.cam",
            "terrtype.cam",
            "mx_Unittype.cam",
            "mx_terrtype.cam",
        ] {
            let path = root.join(dir).join(file);

            if path.is_file() {
                files.push(path);
            }
        }
    }

    Library::from_files(&files).expect("the descriptor archives")
}

/// Every terrain class name, by id, across both datasets.
fn class_names(catalogue: &Catalogue<'_>) -> BTreeMap<FourCc, String> {
    catalogue
        .tagged(b"DTPT")
        .filter_map(|(_, record)| {
            let part = Part::parse(record.data).ok()?;
            let class = desc::TerrainPart::parse(&part).ok()?;

            Some((
                record.id,
                String::from_utf8_lossy(class.head.name).into_owned(),
            ))
        })
        .collect()
}

/// The five sections account for the whole file.
///
/// Sections are counted, not terminated, so ending on the last byte proves
/// the framing.
#[test]
fn every_shipped_chart_parses_and_fills_its_file() {
    // (terrain, surface, object, landscape, profiles)
    let want: BTreeMap<&str, [usize; 5]> = BTreeMap::from([
        ("Data/constants.rgs", [27, 21, 73, 104, 9]),
        ("Data/constants_down.rgs", [27, 21, 73, 104, 9]),
        ("Data/mx_constants_down.rgs", [31, 33, 149, 182, 14]),
        ("DataMX/mx_constants.rgs", [31, 33, 150, 182, 14]),
        ("DataMX/mx_constants_down.rgs", [31, 33, 149, 182, 14]),
    ]);

    let found = charts();
    assert_eq!(found.len(), 4, "the install ships five charts");

    for (name, path) in found {
        let bytes = read(&path);
        let file = rgs::File::parse(&bytes).expect("the framing");
        let chart = file.chart().expect("the chart body");

        let counts = [
            chart.layer(Layer::Terrain).len(),
            chart.layer(Layer::Surface).len(),
            chart.layer(Layer::Object).len(),
            chart.layer(Layer::Landscape).len(),
            chart.profiles.len(),
        ];

        assert_eq!(counts, want[name], "{name} section counts");
    }

    // Both chart versions ship.
    let tags: BTreeSet<String> = charts()
        .iter()
        .map(|(_, p)| {
            rgs::File::parse(&read(p))
                .expect("the framing")
                .tag
                .to_string()
        })
        .collect();

    assert_eq!(
        tags,
        BTreeSet::from(["RGCA".to_owned(), "RGCB".to_owned()]),
        "both chart versions"
    );
}

/// Evidence for the layer names beyond RTTI: terrain entries are `DTPT` ids,
/// object entries `DUNT` ids, surface and landscape entries pattern codes.
#[test]
fn every_chart_reference_resolves_to_a_record_or_a_pattern() {
    let library = corpus();
    let catalogue = library.catalogue().expect("the catalogue");

    for (name, path) in charts() {
        let bytes = read(&path);
        let chart = rgs::File::parse(&bytes)
            .expect("the framing")
            .chart()
            .expect("the chart body");

        let codes: BTreeSet<FourCc> = chart.patterns().map(|(_, p)| p.code).collect();
        let mut dangling = BTreeSet::new();

        for (layer, pattern) in chart.patterns() {
            for entry in &pattern.entries {
                let id = entry.id;

                let ok = match layer {
                    // A null slot, or a terrain class.
                    Layer::Terrain => {
                        id == FourCc::new(NONE) || catalogue.find(b"DTPT", id).is_some()
                    }
                    // A scatter object, or the unexplained `CLER`.
                    Layer::Object => {
                        id == FourCc::new(CLEAR) || catalogue.find(b"DUNT", id).is_some()
                    }
                    // The layers above reference patterns by code.
                    Layer::Surface | Layer::Landscape => codes.contains(&id),
                };

                if !ok {
                    dangling.insert(format!("{} {id}", layer.name()));
                }
            }
        }

        assert!(
            dangling.is_empty(),
            "{name} has references that resolve to nothing: {dangling:?}"
        );
    }
}

/// Patterns across the five shipped charts.
///
/// **Corpus** — 225 in each base chart; 396, 395, 395 in the expansion ones.
const SHIPPED_PATTERNS: usize = 1_241;

/// The scatter chart's `NONE` gate tests the pattern's own code; no shipped
/// pattern trips it.
///
/// **Binary** — mobject kind returns `NONE` undrawn when
/// the dword at `+0x0c` is `NONE`. That field is the record's code (second of
/// the two the file repeats): pattern read reads it,
/// `RGSPattern` prints it as `Pattern= %s` via the FOURCC formatter at
/// fourcc to string.
///
/// Unreachable anyway: pattern section read drops such
/// records. So `regent_core::sim::mapgen::mobject::kind` needs only the length test.
#[test]
fn no_shipped_chart_pattern_carries_the_code_the_gate_refuses() {
    let none = FourCc::new(NONE);
    let mut judged = 0_usize;

    for (name, path) in charts() {
        let bytes = read(&path);
        let chart = rgs::File::parse(&bytes).unwrap().chart().unwrap();

        for (layer, pattern) in chart.patterns() {
            assert_ne!(
                pattern.code,
                none,
                "{name}: a {} pattern carries the code the scatter chart refuses",
                layer.name(),
            );

            judged += 1;
        }
    }

    assert_eq!(judged, SHIPPED_PATTERNS, "the population moved");
}

/// The one id that resolves to nothing, pinned so it cannot grow.
#[test]
fn only_the_demo_patterns_reference_the_unexplained_clear() {
    let bytes = read(&charts()[0].1);
    let chart = rgs::File::parse(&bytes).unwrap().chart().unwrap();

    let users: Vec<String> = chart
        .layer(Layer::Object)
        .iter()
        .filter(|p| p.entries.iter().any(|e| e.id == FourCc::new(CLEAR)))
        .map(|p| String::from_utf8_lossy(p.name).into_owned())
        .collect();

    assert_eq!(users.len(), 6, "six patterns use it: {users:?}");
    assert!(
        users.iter().all(|n| n.starts_with("#DEMO_")),
        "and all of them are leftover examples: {users:?}"
    );
}

/// A terrain pattern never mixes biomes (a map may: Day of Reckoning has
/// four). The generator separates biomes with Dirt.
///
/// No pattern pairs Dirt with a `_child`, matching the missing tileset blend.
#[test]
fn a_terrain_pattern_mixes_within_one_biome() {
    let library = corpus();
    let catalogue = library.catalogue().expect("the catalogue");
    let names = class_names(&catalogue);

    for (name, path) in charts() {
        let bytes = read(&path);
        let chart = rgs::File::parse(&bytes).unwrap().chart().unwrap();

        for pattern in chart.layer(Layer::Terrain) {
            let classes: Vec<&str> = pattern
                .entries
                .iter()
                .filter(|e| e.id != FourCc::new(NONE))
                .filter_map(|e| names.get(&e.id).map(String::as_str))
                .collect();

            let biomes: BTreeSet<&str> = classes
                .iter()
                .filter_map(|n| {
                    n.strip_suffix("_parent")
                        .or_else(|| n.strip_suffix("_child"))
                })
                .collect();

            let label = String::from_utf8_lossy(pattern.name);

            assert!(
                biomes.len() <= 1,
                "{name}: {label} mixes {biomes:?} in one patch of ground"
            );

            let has_dirt = classes.contains(&"Dirt");
            let child = classes.iter().find(|n| n.ends_with("_child"));

            assert!(
                !(has_dirt && child.is_some()),
                "{name}: {label} puts Dirt beside {child:?}, \
                 which is the adjacency the tileset has no blend for"
            );
        }
    }
}
