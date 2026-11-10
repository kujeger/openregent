//! The clump the scatter chart and a save first part on, and what a parting is
//! worth.
//!
//! Walks `mobject::Chart::grown()` in shuffle order to the first clump whose
//! surviving stamps the save lacks.
//!
//! Objects lost between stamp and `GRND` record:
//!
//! * a later `Stamp::Map` halo writes `FOOT` over a placed object;
//! * landscape place drops one whose create returns null;
//! * the settlement is built afterwards and clears the ground it covers.
//!
//! So "chart places, save lacks" is noisy: [`FLOOR_DROPPED`],
//! [`SETTLED_FLOOR`], [`CORPUS_FLOOR`] measure the noise over the [`EXACTLY`]
//! saves. [`CLEARANCE`] is one save's envelope: a floor, not a rule.
//!
//! The oracle is the other direction:
//! **Measured**.

use std::collections::BTreeSet;

use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs::chart::Layer;
use regent_core::sim::mapgen::mobject;

use crate::fixture;

/// Distinct object-layer entry codes in `mx_constants.rgs`. **Corpus**.
const OBJECT_CODES: usize = 79;

/// Every code the scatter can pick has a footprint to read.
///
/// **Binary**. Mobject footprint passes an
/// uninitialised stack rect to `FootprintRectManager`
/// and ignores its result; a code with no `IMAG` leaves the rect unwritten.
///
/// Shipped data avoids it: of [`OBJECT_CODES`], only `CLER` lacks a `DUNT`
/// record, and mobject kind refuses it first. So `mobject::POINT`
/// never fires either.
#[test]
fn every_code_the_scatter_can_pick_has_a_footprint_to_read() {
    let rects = fixture::footprints();
    let codes: BTreeSet<FourCc> = fixture::patterns()
        .layer(Layer::Object)
        .iter()
        .flat_map(|pattern| pattern.entries.iter().map(|entry| entry.id))
        .collect();
    let bare: Vec<FourCc> = codes
        .iter()
        .filter(|code| !rects.contains_key(code))
        .copied()
        .collect();

    assert_eq!(codes.len(), OBJECT_CODES, "the object layer's codes moved");
    assert_eq!(
        bare,
        vec![mobject::CLEAR],
        "a code the scatter can pick has no footprint to read, so \
         mobject footprint would measure uninitialised stack",
    );
}
