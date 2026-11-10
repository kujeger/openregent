//! The mouse cursor, the hover ring and the piece pick, against HD's
//! files: [`regent_core::view::map::cursor`],
//! [`regent_core::view::map::ring::highlights`], [`regent_core::view::map::hit`].
//!
//! Needs `MAJESTY_HD`.

mod common;

use common::catalogue_of;
use std::collections::BTreeSet;

use regent_core::content::unit::Unit;
use regent_core::data::desc::UnitKind;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::SetKind;
use regent_core::view::map::cursor::{self, BUSY, FIRST_SET, Hit, Kind, MODES, Over};
use regent_core::view::map::hit::{self, Layer};
use regent_core::view::map::ring::{self, Stand};

/// Every mode's set is in HD's `CUR1` with the slots its states pick,
/// one frame each: nothing animates.
#[test]
fn every_modes_set_holds_its_slots_unanimated() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let at = catalogue.find(b"IMAG", cursor::IMAGE).expect("the image");
    let sets = regent_core::content::imagery::sets(&catalogue, at).expect("it parses");
    let slots = |set: u32| {
        sets.iter()
            .find(|a| a.id == set)
            .map_or(0, |a| a.poses.len())
    };

    for row in MODES {
        let wanted = match &row.code.0 {
            b"Sele" | b"Drag" | b"Fl00" => 3,
            b"Fl01" => 2,
            c if c.starts_with(b"Sp") => 3,
            _ => 1,
        };

        assert!(slots(FIRST_SET + row.set) >= wanted, "{}", row.code);
    }

    assert!(slots(BUSY.0) > BUSY.1);
    assert!(
        sets.iter()
            .flat_map(|a| &a.poses)
            .all(|p| p.per_stream == 1)
    );
}

/// `CanHighlight`: two buildings, the Balance of Twilight shrines; every
/// character that walks.
#[test]
fn the_hover_ring_takes_every_walker_and_two_buildings() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let mut buildings = BTreeSet::new();
    let mut walkers_without = BTreeSet::new();

    for (at, _) in catalogue.tagged(b"DUNT") {
        let unit = Unit::at(&catalogue, at).expect("it resolves");
        let lit = ring::highlights(&unit.desc, Stand::Standing);
        let walks = unit.animation(SetKind::Walk).is_some();

        if lit && unit.kind == Some(UnitKind::Building) {
            buildings.insert(unit.id);
        }

        if !lit && walks && unit.kind == Some(UnitKind::Character) {
            walkers_without.insert(unit.id);
        }
    }

    let shrines = BTreeSet::from([FourCc(*b"CB01"), FourCc(*b"CB02")]);
    let shrine = Unit::find(&catalogue, FourCc(*b"CB01"))
        .expect("the shrine")
        .expect("it resolves");

    assert_eq!(buildings, shrines);
    assert!(walkers_without.is_empty(), "{walkers_without:?}");
    assert!(!ring::highlights(&shrine.desc, Stand::Fallen));
}

/// The palace's `Inactive` frame 0, row 169 of its 501 by 339 tile, top
/// left 269 left of and 87 above the anchor: a gap at column 0, shadow
/// from 1 to 82, body from 83 to 454.
#[test]
fn the_pick_meets_a_buildings_body_not_its_gaps_or_shadow() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let palace = Unit::find(&catalogue, FourCc(*b"ABJ1"))
        .expect("the palace")
        .expect("it resolves");
    let layers: Vec<Layer<'_>> = palace
        .stack(0xd0)
        .map(|set| Layer {
            set,
            slot: 0,
            frame: 0,
        })
        .collect();
    let parts = hit::parts(&catalogue, &layers).expect("its tiles");
    let at = |column: i32| (column - 269, 169 - 87);

    assert_eq!(parts[0].corner, (-269, -87));
    assert!(!hit::picks(&parts, at(0)), "a gap");
    assert!(!hit::picks(&parts, at(40)), "shadow");
    assert!(hit::picks(&parts, at(83)), "the body");
    assert!(hit::picks(&parts, at(454)), "the body's last column");
    assert!(!hit::picks(&parts, at(456)), "a gap");
    assert_eq!(hit::bounds(&parts), Some([-269, -87, 232, 252]));
}

/// The Magic Bazaar's `Die-6` fires are picked too:
/// after its layers' frames, its five `FXa1` parts, last entry first,
/// so the last is entry 0's fire at (-70, -4) less its tile's anchor.
#[test]
fn the_pick_meets_a_buildings_slot_effects() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let bazaar = Unit::find(&catalogue, FourCc(*b"ABl1"))
        .expect("the Bazaar")
        .expect("it resolves");
    let layers: Vec<Layer<'_>> = bazaar
        .stack(0x65)
        .map(|set| Layer {
            set,
            slot: 0,
            frame: 0,
        })
        .collect();
    let parts = hit::parts(&catalogue, &layers).expect("its tiles");
    let entry = bazaar.stack(0x65).next().expect("Die-6").poses[0].effects[0];
    let fire = regent_core::content::imagery::effect(&catalogue, &entry).expect("FXa1");
    let pose = &fire.poses[1];
    let tile = pose.frames[0].tile().expect("a tile");
    let (x, y) = pose.nudges[0];
    let want = hit::Part::tile(&catalogue, tile, (0, 0))
        .expect("the tile")
        .expect("it parses");
    let (ax, ay) = want.tile.as_ref().expect("a tile").anchor();

    assert_eq!(
        parts.last().expect("parts").corner,
        (
            -70 + i32::from(x) - i32::from(ax),
            -4 + i32::from(y) - i32::from(ay)
        )
    );
}

/// The pick goes through every character with no panel: spell effects,
/// event markers, the ring art. No other character; every walker has one.
#[test]
fn a_spell_effect_lets_the_pick_through() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let hollow: BTreeSet<FourCc> = catalogue
        .tagged(b"DUNT")
        .map(|(at, _)| Unit::at(&catalogue, at).expect("it resolves"))
        .filter(|u| u.kind == Some(UnitKind::Character) && !hit::solid(&u.desc))
        .map(|u| u.id)
        .collect();
    let want: BTreeSet<FourCc> = [
        // What cast.rs's SPELL_UNITS names.
        b"WVA1", b"WVC1", b"WVE1", b"DVB1", b"SVA1", b"SVB1", b"WVi1", b"WVh1",
        // CreateSpellUnit's: meteor storm, vortex, critical hit, a dead fire ball.
        b"WVg1", b"NVk1", b"WVj1", b"WVe2",
        // CVC1 is taken only for its agent: a clone shows its hero's code.
        b"CVC1", // Markers, the ring and bracket art.
        b"MVa1", b"MVa2", b"MVa3", b"MVa4", b"AVn1", b"AVn2",
    ]
    .into_iter()
    .map(|code| FourCc(*code))
    .collect();

    assert_eq!(hollow, want);

    // What the player must still reach: a hero, a monster, a summoned
    // medusa, a guard, a healing plant, a chest, the palace, reward flags.
    for code in [
        b"AVB1", b"BVT1", b"BVO1", b"AVb1", b"AVk1", b"AVk4", b"ABJ1", b"ARA1", b"ARA2", b"ARA3",
        b"ARA4",
    ] {
        let unit = Unit::find(&catalogue, FourCc(*code))
            .expect("the unit")
            .expect("it resolves");

        assert!(hit::solid(&unit.desc), "{}", unit.id);
    }
}

/// A warrior's box holds a point off his pixels: the pick takes him by
/// his box, and the pointing hand takes a character so.
#[test]
fn a_character_is_met_in_its_box() {
    let library = common::library();
    let catalogue = catalogue_of(library);
    let warrior = Unit::find(&catalogue, FourCc(*b"AVB1"))
        .expect("a warrior")
        .expect("it resolves");
    let stand = warrior.animation(SetKind::Stand).expect("a stand");
    let layers = [Layer {
        set: stand,
        slot: 0,
        frame: 0,
    }];
    let parts = hit::parts(&catalogue, &layers).expect("its tiles");
    let rect = hit::bounds(&parts).expect("a box");
    // Column 0 of the tile's middle row is a gap.
    let gap = (-35, -43 + 23);

    assert_eq!(rect, [-35, -43, 22, 4]);
    assert!(!hit::picks(&parts, gap));

    let picked = hit::pick(
        [()],
        |()| hit::boxed(hit::screen_box(rect, 1.0), gap),
        |()| hit::picks(&parts, gap),
    );

    assert_eq!(picked, Some(((), Hit::Box)));
    assert_eq!(cursor::over(Kind::Character, Hit::Box), Over::Piece);
    assert_eq!(
        cursor::over(Kind::Building(Stand::Standing), Hit::Box),
        Over::Ground
    );
}
