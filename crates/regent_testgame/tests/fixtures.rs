//! The game's source reads through the real readers as what it says.

use regent_core::content::unit::Unit;
use regent_core::data::imag::{Image, SetKind};
use regent_core::sim::placement;
use regent_core::sim::setup;
use regent_core::sim::world::bodies::Class;
use regent_testgame::{BITER, CODES, HUT, KEEP, LODGE, TestGame, WALKER};

/// The game's archives hold every code it names, as its chunk.
#[test]
fn every_code_is_found_in_its_chunk() {
    let catalogue = TestGame::get().catalogue();

    for (tag, code) in CODES {
        assert!(catalogue.find(tag, code).is_some(), "{code}");
    }
}

/// Each unit is its class, with art, a pad to stand on and scripts' data.
#[test]
fn each_unit_reads_as_its_kind() {
    let game = TestGame::get();
    let catalogue = game.catalogue();
    let descriptors = setup::descriptors(&catalogue);
    let bodies = descriptors.bodies();
    let class = |code| bodies.get(&code).expect("a body").class;

    for code in [KEEP, HUT, LODGE] {
        assert_eq!(class(code), Class::Building, "{code}");
    }

    for code in [WALKER, BITER] {
        assert_eq!(class(code), Class::Character, "{code}");
        assert!(bodies.get(&code).expect("a body").gait.is_some(), "{code}");
    }

    for (code, name) in [(KEEP, "Keep"), (HUT, "Hut"), (WALKER, "Walker")] {
        let unit = Unit::find(&catalogue, code)
            .expect("a unit")
            .expect("reads");

        assert_eq!(unit.desc.head.name, name.as_bytes());
        assert!(unit.art.is_some_and(|a| a.resolved()), "{code}");
        assert!(
            game.program().data_set(name).is_some(),
            "{name}: an agent's data"
        );
        assert!(
            placement::pad_rect(&catalogue, &unit)
                .expect("art")
                .is_some(),
            "{code}"
        );
    }
}

/// A building's art gives the pad its rect asks for, two cells a side or so.
#[test]
fn a_huts_pad_takes_several_half_cells() {
    let catalogue = TestGame::get().catalogue();
    let unit = Unit::find(&catalogue, HUT).expect("a unit").expect("reads");
    let cells = placement::taken(&catalogue, &unit)
        .expect("art")
        .expect("a pad");
    let record = catalogue
        .record(b"IMAG", unit.art.and_then(|a| a.at).expect("art"))
        .expect("a record");
    let image = Image::parse(record.data).expect("reads");

    assert!(cells.len() >= 4, "{cells:?}");
    assert!(image.set_of(SetKind::Inactive).is_some());
}
