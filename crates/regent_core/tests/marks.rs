//! Minimap marks: `MiniMapAnimation`'s unit, its set 300 flashed twice
//! (`sim::world::minimap_mark`). No trace's save holds one, so the builtin
//! runs on the start save's world, called as the shipped scripts call it.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::sim::world::{Sight, minimap_mark::Wave};

/// The four shipped marks, from HD's descriptors and art: each one slot
/// of set 300, drawn nowhere but the minimap.
#[test]
fn the_shipped_marks_flash_their_set_twice() {
    let bodies = common::bodies();
    let mark = |code: &[u8; 4]| {
        let body = &bodies[&FourCc(*code)];

        (Wave::of_body(body), body.sight)
    };
    let wave = |frames, interval| Some(Wave { frames, interval });

    assert_eq!(mark(b"MVa1"), (wave(8, 100), Sight::Everyone));
    assert_eq!(mark(b"MVa2"), (wave(20, 100), Sight::Owner));
    assert_eq!(mark(b"MVa3"), (wave(8, 100), Sight::Everyone));
    assert_eq!(mark(b"MVa4"), (wave(10, 150), Sight::Owner));

    let lasts = |code| mark(code).0.map(|w| w.lasts());

    assert_eq!(lasts(b"MVa1"), Some(1_500));
    assert_eq!(lasts(b"MVa2"), Some(3_900));
    assert_eq!(lasts(b"MVa4"), Some(2_850));
}
