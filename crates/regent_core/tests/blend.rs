//! Tints, against HD's archives: [`regent_core::view::raster::blend`].

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::data::payload::Palette;
use regent_core::view::raster::blend::{self, TINT_RANGE, TINTS};

/// The three tint palettes are `interfacedata.cam`'s `PALT` records of
/// those names; tinted, another palette holds only their first 247
/// colours.
#[test]
fn a_tint_is_an_interface_palette() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let palette = |tag: &[u8; 4], id: [u8; 4]| {
        let at = catalogue.find(tag, FourCc(id)).expect("the record");
        let record = catalogue.record(tag, at).expect("its bytes");

        Palette::parse(record.data).expect("a palette")
    };
    let sprite = palette(b"PALT", *b"PLYR");

    for tag in TINTS {
        let tint = palette(b"PALT", tag);
        let out = blend::tinted(&sprite, &tint);

        assert!(
            out.colours
                .iter()
                .all(|c| tint.colours[..TINT_RANGE].contains(c))
        );
    }
}
