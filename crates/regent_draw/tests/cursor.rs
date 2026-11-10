//! The cursor's picture against HD's files: [`regent_draw::art::cursor`].
//!
//! Needs `MAJESTY_HD`.

use regent_corpus as common;

use regent_core::view::map::cursor::{self, Cursor, FIRST_SET, MODES};
use regent_draw::art;

/// The hotspot is the first tile's anchor: the open hand points at its
/// finger, the fist hangs below the mouse.
#[test]
fn a_cursors_hotspot_is_its_tiles_anchor() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let picture = |c: Cursor| {
        art::cursor::picture(&catalogue, c.set().expect("a set"), c.slot())
            .expect("it decodes")
            .expect("a picture")
    };

    let hand = picture(Cursor::Select(cursor::Over::Ground));
    let fist = picture(Cursor::Drag(cursor::Purse::Holds));

    assert_eq!(hand.hotspot, (4, 2));
    assert_eq!(fist.hotspot, (2, 0));

    // Every slot of every mode composes, its hotspot inside.
    for row in MODES {
        for slot in 0..3 {
            let shown =
                art::cursor::picture(&catalogue, FIRST_SET + row.set, slot).expect("it decodes");

            if let Some(p) = shown {
                assert!(usize::from(p.hotspot.0) < p.sheet.width, "{}", row.code);
                assert!(usize::from(p.hotspot.1) < p.sheet.height, "{}", row.code);
            }
        }
    }
}
