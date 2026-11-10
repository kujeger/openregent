//! An oracle for the landscape chart, inverted from saves.
//!
//! The index costs one draw whatever it picks, and no save records it. But
//! where only one entry of a zone can make a saved code, the index at that
//! clump's seed is pinned.
//!
//! For a one-object clump the seed is the object's cell
//! (mobject place's loop skips sizes 0 and 1); 356 of
//! 580 entries ([`SIZES`]). That gives [`PINS`] cells, [`PER_SAVE`] a save.
//!
//! # Result
//!
//! All [`PINS`] right on every save, [`SPLIT`] by heightfield outcome, against
//! [`BY_CHANCE`] (each zone's share of cells already holding the index).
//! Worked case: **Measured**.
//!
//! # Negative result
//!
//! For larger clumps the pin only says some cell of the 9 by 9 ring holds the
//! index: [`RING`] satisfied against [`RING_BY_CHANCE`] by chance. Do not
//! score it.

use std::collections::{BTreeMap, BTreeSet};

use regent_core::data::rgs::chart::Layer;

use crate::common::hd;
use crate::fixture::{self};

/// Clump size and entry count per size.
///
/// **Corpus** — landscape patterns of the 108 freestyle zones: 356 of 580 ask
/// for one; range 1 to 9.
const SIZES: [(u32, usize); 9] = [
    (1, 356),
    (2, 27),
    (3, 42),
    (4, 36),
    (5, 20),
    (6, 23),
    (7, 35),
    (8, 22),
    (9, 19),
];

/// Object entries' word in the clump-size slot. **Corpus**: always zero, both
/// charts.
const OBJECT_SIZES: [u32; 1] = [0];

/// A clump's size is the landscape entry's word, not the object pattern's.
///
/// **Binary.** In mobject place the size's `this` is the landscape
/// entry distribution at wrote; the object pattern, another stack
/// slot, is not read for size. Distribution seeds returns the word at
/// `+0x0c`: `rest[0]`.
///
/// Data agrees: landscape entries ask [`SIZES`] (1 to 9); object entries
/// [`OBJECT_SIZES`] (all zero), which would grow no clump.
#[test]
fn a_clumps_size_is_the_landscape_entrys_word_and_not_the_object_patterns() {
    let patterns = fixture::patterns();
    let mut sizes: BTreeMap<u32, usize> = BTreeMap::new();

    for zone in &fixture::map().zones {
        if let Some(pattern) = patterns.find_in(Layer::Landscape, zone.landscape) {
            for entry in &pattern.entries {
                *sizes.entry(entry.rest[0]).or_default() += 1;
            }
        }
    }

    assert_eq!(
        sizes.into_iter().collect::<Vec<_>>(),
        SIZES,
        "what a landscape entry asks for moved",
    );

    // Both shipped charts.
    for name in ["Data/constants.rgs", "DataMX/mx_constants.rgs"] {
        let bytes = std::fs::read(hd().join(name)).expect("a chart");
        let file = regent_core::data::rgs::File::parse(&bytes).expect("a chart");
        let chart = file.chart().expect("its chart");
        let words: BTreeSet<u32> = chart
            .layer(Layer::Object)
            .iter()
            .flat_map(|pattern| pattern.entries.iter().map(|entry| entry.rest[0]))
            .collect();

        assert_eq!(
            words.into_iter().collect::<Vec<_>>(),
            OBJECT_SIZES,
            "{name}: an object entry's word at 0x0c is not zero throughout",
        );
    }
}
