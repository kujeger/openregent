//! What a placed building deletes: `regent_core::sim::clearing`, against the
//! seed-1 traces, whose start save holds the map before the player built.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::sim::clearing::{Kind, Occupant, Pass, Time};

/// Pass 0 takes resources and spent buildings; pass 1 scatter, spent
/// buildings and emptied chests; buildings survive generation.
#[test]
fn each_pass_deletes_what_the_binary_tests() {
    let one = |kind, code: &[u8; 4], ruined, agent, hit_points| Occupant {
        kind,
        code: FourCc(*code),
        ruined,
        agent,
        hit_points,
    };
    let tree = one(Kind::Scatter, b"NGa1", false, false, 0);
    let ruin = one(Kind::Building, b"ABF1", true, true, -3);
    let inn = one(Kind::Building, b"ABF1", false, true, 1);
    let plant = one(Kind::Other, b"AVk1", false, true, 10);
    let chest = one(Kind::Building, b"BBt1", false, true, 0);

    assert!(tree.cleared(Pass::Around, Time::Generating));
    assert!(!tree.cleared(Pass::Under, Time::Playing));
    assert!(ruin.cleared(Pass::Under, Time::Playing));
    assert!(ruin.cleared(Pass::Around, Time::Playing));
    assert!(!ruin.cleared(Pass::Around, Time::Generating));
    assert!(!inn.cleared(Pass::Around, Time::Playing));
    assert!(plant.cleared(Pass::Under, Time::Generating));
    assert!(!plant.cleared(Pass::Around, Time::Playing));
    assert!(chest.cleared(Pass::Around, Time::Generating));
}
