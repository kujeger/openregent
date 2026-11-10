//! The whole generator against one save: its objects, its buildings and its
//! ground.
//!
//! Oracles: saved heights for the fractal, `GRND` records for the mobject
//! chart. One draw out upstream moves every object.
//!
//! Single-save tests use the 128-cell save whose stream position was measured
//! independently ([`SPENT`]): all [`OBJECTS`] objects match, and the heights
//! follow from the map number.

use regent_core::content::unit::Unit;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::footprint;
use regent_core::sim::mapgen::mobject;

use crate::common;

/// Every object code the save's land scatters, and the footprint its art
/// gives it: `left`, `top`, `right`, `bottom` in pixels about the anchor.
///
/// Hand-read from `IMAG` records; checks `regent_core::sim::footprint`.
///
/// `WG12`'s slot rect is empty, so it takes the frame box union
/// fallback: `[-19, -2, 16, 21]`.
const FOOTPRINTS: [(&[u8; 4], [i16; 4]); 14] = [
    (b"NGa1", [-32, -19, 22, 22]),
    (b"NGh1", [-12, -11, 14, 11]),
    (b"NGc1", [-38, -29, 34, 25]),
    (b"NGo1", [-37, 72, 20, 113]),
    (b"NGq1", [-30, -22, 27, 14]),
    (b"NGs1", [-55, -29, 23, 25]),
    (b"NGn1", [-12, -13, 12, 11]),
    (b"CG03", [-46, -9, 35, 47]),
    (b"CG07", [-15, -21, 14, 27]),
    (b"CG08", [6, -22, 29, 15]),
    (b"LG01", [-15, -25, 15, 16]),
    (b"PG07", [-24, -40, 23, 26]),
    (b"WG12", [-19, -2, 16, 21]),
    (b"YG03", [-42, 21, 70, 91]),
];

/// The reader produces the rects that were read by hand.
///
/// `WG12` exercises the fallback.
#[test]
fn the_footprint_reader_agrees_with_the_rects_that_were_measured() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue of them");

    for (id, want) in FOOTPRINTS {
        let code = FourCc::new(id);
        let got = footprint::of_code(&catalogue, code).expect("the code's art");

        assert_eq!(
            got,
            Some(want),
            "{code}'s footprint reads as {got:?}, against the {want:?} measured by hand"
        );
    }
}

/// The biggest reaches the scatter chart has to keep clear, in cells.
///
/// **Binary and corpus.** `mobject::reach` truncates twice. `VG03`:
/// `-126/32 = -3`, `trunc(-68/sin 55°)/32 = -83/32 = -2`, `136/32 = 4`,
/// `trunc(84/sin 55°)/32 = 102/32 = 3`.
///
/// The last three are [`UNIONED`].
const REACHES: [(&[u8; 4], mobject::Reach); 5] = [
    (b"VG03", (3, 2, 4, 3)),
    (b"PG11", (2, 2, 3, 2)),
    (b"YG02", (2, 1, 3, 1)),
    (b"NGe1", (1, 1, 1, 0)),
    (b"NGl1", (1, 2, 2, 2)),
];

/// The codes the object layer scatters that are measured by their pictures
/// rather than by a slot rect, and the rect each way.
///
/// **Binary.** `IsoFootprintRectManager` reads slot rects on sets 8 and
/// `0xd0` only, then unions frame boxes on 8, `0xd0`, `0x40`. A `Special`-only
/// code is measured by pictures. Three of 79 are; with slot rects they would
/// read `(2, 2, 2, 1)`, `(0, 0, 1, 0)` and `(1, 1, 2, 1)`, shifting the draw
/// count.
const UNIONED: [(&[u8; 4], [i16; 4], [i16; 4]); 3] = [
    (b"YG02", [-64, -68, 90, 42], [-65, -44, 97, 44]),
    (b"NGe1", [-30, -18, 32, 18], [-37, -30, 38, 24]),
    (b"NGl1", [-40, -35, 78, 47], [-60, -61, 87, 64]),
];

/// The widest footprints reach the cells they should, and a `Special` set is
/// measured by its pictures.
///
/// [`REACHES`]: the widest in the corpus. [`UNIONED`]: picture-measured.
#[test]
fn a_special_set_is_measured_by_its_pictures_and_not_by_its_slot() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue of them");

    for (id, want) in REACHES {
        let code = FourCc::new(id);
        let rect = footprint::of_code(&catalogue, code)
            .expect("the code's art")
            .expect("a footprint");

        assert_eq!(
            mobject::reach(rect),
            want,
            "{code}'s rect {rect:?} reaches {:?}, against the {want:?} the art gives it",
            mobject::reach(rect),
        );
    }

    for (id, slot, union) in UNIONED {
        let code = FourCc::new(id);
        let unit = Unit::find(&catalogue, code)
            .expect("the code's descriptor")
            .expect("its art");

        assert!(
            unit.animations
                .iter()
                .all(|set| regent_core::data::imag::set_number(set.id) == 0x40),
            "{code} carries a set other than Special, so it is not one of these",
        );
        assert_eq!(
            unit.animations
                .first()
                .and_then(|set| set.poses.first())
                .map(|pose| pose.footprint),
            Some(slot),
            "{code}'s slot rect moved, and it is the one the engine does not read",
        );
        assert_eq!(
            footprint::of_code(&catalogue, code).expect("the code's art"),
            Some(union),
            "{code} no longer comes back as the union of its pictures",
        );
    }
}
