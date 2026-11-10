//! What the generator's tests read out of the corpora: the freestyle
//! quest and chart, and the art's footprints.
//!
//! The corpus lookup is `regent_corpus`'s; the rest reads its variables
//! through `regent_corpus::env`.

// Each test binary reads a different subset.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use regent_core::content::unit::Unit;
use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::chart::Chart as Patterns;
use regent_core::data::rgs::{self, Map};
use regent_core::sim::footprint;
use regent_core::sim::mapgen::mobject;

pub use regent_corpus::hd;

/// The expansion's freestyle quest, as bytes.
pub fn quest() -> &'static [u8] {
    static QUEST: OnceLock<Vec<u8>> = OnceLock::new();

    QUEST.get_or_init(|| {
        std::fs::read(hd().join("QuestsMX/mx_Freestyle.q")).expect("a freestyle quest")
    })
}

/// The expansion's constants file, as bytes.
pub fn chart() -> &'static [u8] {
    static CHART: OnceLock<Vec<u8>> = OnceLock::new();

    CHART.get_or_init(|| std::fs::read(hd().join("DataMX/mx_constants.rgs")).expect("a chart"))
}

/// The quest's map: all 108 zones, the lands and the unit patterns.
pub fn map() -> &'static Map<'static> {
    static MAP: OnceLock<Map<'static>> = OnceLock::new();

    MAP.get_or_init(|| {
        rgs::File::parse(quest())
            .expect("a quest file")
            .map()
            .expect("its map")
    })
}

/// The chart file's patterns and height profiles.
pub fn patterns() -> &'static Patterns<'static> {
    static PATTERNS: OnceLock<Patterns<'static>> = OnceLock::new();

    PATTERNS.get_or_init(|| {
        rgs::File::parse(chart())
            .expect("a chart file")
            .chart()
            .expect("its chart")
    })
}

/// Every unit's footprint in cells, by descriptor code.
///
/// Needs both HD archive directories: every freestyle scatter code (`NGa1
/// SnowPine` and thirteen more) is in `DataMX/mx_unittype.cam`; without it
/// every footprint falls back to one cell.
pub fn footprints() -> &'static BTreeMap<FourCc, mobject::Reach> {
    static RECTS: OnceLock<BTreeMap<FourCc, mobject::Reach>> = OnceLock::new();

    RECTS.get_or_init(|| {
        let library = regent_corpus::library();
        let catalogue = library.catalogue().expect("a catalogue of them");

        catalogue
            .tagged(b"DUNT")
            .filter_map(|(at, record)| {
                let unit = Unit::at(&catalogue, at).ok()?;
                let rect = footprint::of_unit(&catalogue, &unit).ok()??;

                Some((record.id, mobject::reach(rect)))
            })
            .collect()
    })
}

/// An object's footprint reach, [`mobject::POINT`] if unknown.
pub fn reach(code: FourCc) -> mobject::Reach {
    footprints().get(&code).copied().unwrap_or(mobject::POINT)
}
