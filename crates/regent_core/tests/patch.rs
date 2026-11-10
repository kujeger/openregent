//! The quest seed patch stands in for map number 0: every HD quest has it.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::rgs;

/// Every quest HD ships, base and expansion, has map number 0: the clock.
#[test]
fn every_hd_quests_map_number_is_0() {
    let root = common::hd();
    let mut quests = 0;

    for dir in ["Quests", "QuestsMX"] {
        for path in common::walk(&root.join(dir), &["q"]) {
            let bytes = std::fs::read(&path).unwrap();
            let map = rgs::File::parse(&bytes).unwrap().map().unwrap();

            assert_eq!(map.words[1], 0, "{}", path.display());
            quests += 1;
        }
    }

    assert_eq!(quests, 36);
}
