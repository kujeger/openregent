//! A building's looping `Animate`: what keeps it in ORDR
//! (`sim::world::animated`).
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::fourcc::FourCc;

/// The rats' nest's `Inactive` is one layer of eight frames a slot; the
/// palace's, the building in one frame and its flag in six.
#[test]
fn a_layer_of_frames_is_what_animates() {
    let bodies = common::bodies();
    let layers = |code: &[u8; 4]| bodies[&FourCc(*code)].animated[&0xd0].clone();

    assert_eq!(layers(b"BBx1"), [vec![8, 8]]);
    assert_eq!(layers(b"ABJ1"), [vec![1], vec![6]]);
}
