//! The Dirt pad a building paints at construction start, against the saves.
//!
//! A rule of play, not a map-writer detail. See `regent_core::sim::placement`.

mod common;

use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Library;
use regent_core::data::desc::Info;
use regent_core::sim::placement;

/// How many descriptors carry `ModifyTerrainTextureOnPlacement`, bit 8.
///
/// HD `Data` 71, plus `DataMX` 24.
const FLAGGED: usize = 95;

/// HD's own archives, base game and expansion.
///
/// `common::hd()` is the install root; joining `DataMX` to `Data` would
/// silently give the base game alone (71, not 95).
fn catalogue_paths() -> Vec<std::path::PathBuf> {
    let root = common::hd();
    let mut paths = common::archives(&root.join("Data"));

    paths.extend(common::archives(&root.join("DataMX")));
    paths
}

/// Gate (bit 8 of the `PRIM` info word) and shape (point list 0 of the art's
/// slot) come from different records, so agreement on all [`FLAGGED`] is not a
/// tautology.
#[test]
fn every_building_that_lays_ground_has_a_polygon_to_lay_it_from() {
    let library = Library::from_files(&catalogue_paths()).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let (mut flagged, mut padded) = (0usize, 0usize);

    for (at, _) in catalogue.tagged(b"DUNT") {
        let Ok(unit) = Unit::at(&catalogue, at) else {
            continue;
        };

        if !unit
            .desc
            .engine()
            .has(Info::ModifyTerrainTextureOnPlacement)
        {
            continue;
        }

        flagged += 1;
        padded += usize::from(matches!(placement::pad(&catalogue, &unit), Ok(Some(_))));
    }

    println!("  {flagged} descriptors carry bit 8, {padded} produce a pad");

    assert_eq!(flagged, FLAGGED, "how many descriptors lay ground changed");
    assert_eq!(
        padded, flagged,
        "a building lays ground with no shape to lay"
    );
}

/// What the corpus measures a pad's bounding box to be, per descriptor.
///
/// Measured from the saves, independent of the implementation: a cell belongs
/// to its nearest building, tallied per half-cell sub-position, and enters the
/// box when wholly Dirt in most instances.
///
/// Only descriptors with five or more instances; fewer means one building
/// repeated across near-duplicate saves.
///
/// `BBL1` and `BBB1` were measured one cell narrower on the left; the column
/// test below re-measures that edge per instance, and these are its boxes.
const MEASURED: [(&[u8; 4], [i32; 4]); 29] = [
    (b"BBH1", [-4, 3, -1, 4]),
    (b"ABG1", [-3, 1, -1, 3]),
    (b"ABJ1", [-7, 6, -2, 8]),
    (b"ABY1", [-5, 2, -1, 5]),
    (b"BBB2", [-3, 4, -1, 5]),
    (b"ABF1", [-3, 2, 0, 4]),
    (b"ABf1", [-2, 1, -1, 2]),
    (b"BBK1", [-3, 2, -1, 3]),
    (b"ABc1", [-4, 2, 0, 4]),
    (b"BBB4", [-3, 2, -1, 4]),
    (b"ABC1", [-3, 2, -1, 3]),
    (b"ABP3", [-4, 3, 0, 5]),
    (b"ABS1", [-4, 3, -1, 5]),
    (b"ABU2", [-4, 4, -1, 5]),
    (b"ABb1", [-4, 3, 0, 5]),
    (b"ABO3", [-4, 3, -1, 5]),
    (b"ABC2", [-3, 2, -1, 3]),
    (b"ABH3", [-4, 3, -1, 4]),
    (b"ABP1", [-4, 3, 0, 5]),
    (b"ABQ3", [-5, 4, -1, 5]),
    (b"ABR2", [-4, 3, -1, 4]),
    (b"ABT3", [-5, 4, -1, 6]),
    (b"ABX1", [-4, 3, -1, 4]),
    (b"ABX2", [-4, 3, -1, 4]),
    (b"BBB3", [-4, 3, 0, 5]),
    (b"ABH2", [-4, 3, -1, 4]),
    (b"ABh1", [-4, 3, -1, 4]),
    (b"BBL1", [-4, 2, -1, 4]),
    (b"BBB1", [-5, 3, -1, 6]),
];

/// How many of [`MEASURED`] the model has to reproduce exactly: all.
///
/// **Binary**: one pad path. Chart, quest and script buildings all reach
/// `BuildingRec` — from
/// the unit chart place, and from `SpawnUnit` through the placed-agent creation — and its mask is cached per descriptor. Agent create placed
/// destroys an agent whose method fails, so every building a save holds ran
/// the pad. `ABW1 Rangers_Guild`, with no Dirt in the Reckoning maps, is not
/// explained by a path.
const AGREEING: usize = 29;

/// The model's pad is the box the corpus measured.
#[test]
fn a_pads_bounding_box_is_the_one_the_corpus_measures() {
    let library = Library::from_files(&catalogue_paths()).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let (mut exact, mut judged) = (0usize, 0usize);

    for (id, want) in MEASURED {
        let code = regent_core::data::fourcc::FourCc::new(id);
        let Some(Ok(unit)) = Unit::find(&catalogue, code) else {
            continue;
        };
        let Ok(Some(pad)) = placement::pad(&catalogue, &unit) else {
            continue;
        };
        let cells = pad.cells((0, 0));

        if cells.is_empty() {
            continue;
        }

        let bound = |pick: fn(&(i32, i32)) -> i32, best: fn(i32, i32) -> i32| {
            cells.iter().map(pick).reduce(best).unwrap_or(0)
        };
        let got = [
            bound(|c| c.0, i32::min),
            bound(|c| c.0, i32::max),
            bound(|c| c.1, i32::min),
            bound(|c| c.1, i32::max),
        ];

        judged += 1;

        if got == want {
            exact += 1;
        } else {
            println!("  {code}: model {got:?} against the corpus's {want:?}");
        }
    }

    println!("  {exact} of {judged} pad boxes are the ones the corpus measured");

    assert_eq!(judged, MEASURED.len(), "a measured descriptor went missing");
    assert!(
        exact >= AGREEING,
        "{exact} boxes match, against {AGREEING} before"
    );
}

/// A unit whose art draws none of the footprint's sets, the `fire_ball`
/// spell unit (`XL02`, only `Die`), measures as a point:
/// Frame box union zeroes its rect before it unions.
#[test]
fn a_unit_drawing_no_footprint_set_measures_as_a_point() {
    let library = Library::from_files(&catalogue_paths()).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let unit = Unit::find(&catalogue, regent_core::data::fourcc::FourCc::new(b"XL02"))
        .expect("fire_ball")
        .expect("its descriptor");

    assert_eq!(
        placement::pad_rect(&catalogue, &unit).expect("art"),
        Some([0; 4])
    );
    assert_eq!(
        placement::taken(&catalogue, &unit).expect("art"),
        Some(vec![])
    );
}
