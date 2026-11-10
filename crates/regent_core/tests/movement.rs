//! Walking, as `sim::movement` models it, against the inn probe:
//! every frame of both peasants' walks to the inn.
//!
//! Needs `MAJESTY_HD`.

mod common;

use common::read;
use std::collections::{BTreeMap, BTreeSet};

use regent_core::data::desc::Part;
use regent_core::data::{FourCc, cam};
use regent_core::sim::movement::plan::LAYERS;
use regent_core::sim::movement::{Class, FRAMELESS, KEEPS_FACING, LOBBED, STAND, TRAMPLES, WALK};

/// The peasants' movement class, `Class 1`.
const CLASS_1: &[u8; 4] = b"CL01";

/// The peasant, `AVj1`; the female one shares its class and shape.
const PEASANT: FourCc = FourCc(*b"AVj1");

/// Every `DMOV` record in HD's base unit archive, by id.
fn classes() -> BTreeMap<[u8; 4], Class> {
    let data = read(&common::hd().join("Data").join("unittype.cam"));
    let archive = cam::Archive::parse(&data).expect("unittype.cam");

    archive
        .records()
        .filter(|(chunk, _)| chunk.tag == b"DMOV")
        .map(|(_, r)| (r.id.0, Class::parse(r.data).expect("a DMOV")))
        .collect()
}

#[test]
fn the_peasants_movement_class_is_the_files() {
    let classes = classes();

    assert_eq!(classes.len(), 20);
    assert_eq!(
        classes[CLASS_1],
        Class {
            turn_rate: 50,
            turn_step: 4,
            move_rate: 90,
            move_dist: 6,
            size: 0,
            lift: (0, 0),
            flags: 0,
        }
    );
    assert_eq!(classes[b"ML04"].lift, (180, 60));
}

/// A peasant's `Basic_build` holds track 2 for its set's frames less one
/// at the set's playback: `Carry-2`, set
/// `0x91`, is four frames at 100 ms on `AVj1`, six on `AVj2`. Its Action,
/// `Rate` max 300, ends as `AVj1`'s `InitAction` does.
#[test]
fn a_peasants_build_holds_track_two_three_frames_or_five() {
    let bodies = common::bodies();
    let (named, _) = common::lookups();
    let build = named.actions["basic_build"];
    let init = |code: FourCc| {
        bodies[&code]
            .poses
            .as_ref()
            .and_then(|p| p.get(&build.set))
            .map(|p| p.init_ms(12))
    };

    assert_eq!(build.set, 0x91);
    assert_eq!(init(PEASANT), Some(300));
    assert_eq!(init(FourCc(*b"AVj2")), Some(500));
    assert_eq!(
        regent_core::sim::world::walking::action_ms(build.rate, 0),
        300
    );
}

/// An action's `ImageSet`, `Sound` and `Projectile` are `PRIM` words 1 to
/// 3: the engine part holds `PRIM` from a field, and the
/// XML reader stores them.
/// `Guardhouse_Arrow` plays `Attack`, sounds
/// `Guard_House` and shoots `Green_Ranger_Arrow`.
#[test]
fn an_actions_set_sound_and_projectile_are_prim_words_one_to_three() {
    use regent_core::data::desc::{Action, Part};

    let data = read(&common::hd().join("Data").join("action.cam"));
    let archive = cam::Archive::parse(&data).expect("action.cam");
    let action = |id: &[u8; 4]| {
        archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"DACT" && r.id.0 == *id)
            .map(|(_, r)| r.data)
            .expect("the action")
    };
    let arrow = Part::parse(action(b"AXA6")).expect("a part");
    let arrow = Action::parse(&arrow).expect("an action");
    let build = Part::parse(action(b"A008")).expect("a part");
    let build = Action::parse(&build).expect("an action");

    assert_eq!(arrow.image_set(), Some(0x10));
    assert_eq!(arrow.sound(), Some(FourCc(*b"BP13")));
    assert_eq!(arrow.projectile(), Some(FourCc(*b"APA2")));
    assert_eq!(build.image_set(), Some(0x91));
    assert_eq!((build.sound(), build.projectile()), (None, None));
}

/// An action's `Overlay` is `PRIM + 0x10`, the engine part's:
/// Init action peak hangs it on the actor
/// for 0 ms at the peak.
/// `level_up` (`XRC1`) shows `Level_Up` (`ARC1`); `Guardhouse_Arrow` none.
#[test]
fn an_actions_overlay_is_prim_word_four() {
    use regent_core::data::desc::{Action, Part};

    let data = read(&common::hd().join("Data").join("action.cam"));
    let archive = cam::Archive::parse(&data).expect("action.cam");
    let action = |id: &[u8; 4]| {
        let data = archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"DACT" && r.id.0 == *id)
            .map(|(_, r)| r.data)
            .expect("the action");

        Part::parse(data).expect("a part")
    };
    let level = action(b"XRC1");
    let arrow = action(b"AXA6");

    assert_eq!(
        Action::parse(&level).expect("an action").overlay(),
        Some(FourCc(*b"ARC1"))
    );
    assert_eq!(Action::parse(&arrow).expect("an action").overlay(), None);
}

/// An action's after-set is `PRIM + 0x34`, the word action after set reads
/// of the engine part: `GenUnit` plays it as the
/// `InitAction` ends. `basic_attack_with_stand` stands, `basic_attack`
/// plays nothing, `Basic_build` takes the default.
#[test]
fn an_actions_after_set_is_prim_word_thirteen() {
    use regent_core::data::desc::{Action, Part};

    // The `Stand` set's id.
    const STAND: i32 = 8;

    let data = read(&common::hd().join("Data").join("action.cam"));
    let archive = cam::Archive::parse(&data).expect("action.cam");
    let after = |id: &[u8; 4]| {
        let record = archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"DACT" && r.id.0 == *id)
            .map(|(_, r)| r.data)
            .expect("the action");
        let part = Part::parse(record).expect("a part");

        Action::parse(&part).expect("an action").after_set()
    };

    assert_eq!(after(b"A002"), Some(STAND));
    assert_eq!(after(b"A001"), Some(0));
    assert_eq!(after(b"A008"), Some(-1));
}

/// `Basic_death` (`A005`) and `Become_Rubble` (`A009`) run callback
/// `0x2000` at their peak, the dying unit's lie-down; `Henchman_death`
/// (`A010`) runs none, so a dead henchman stays where it fell.
#[test]
fn a_heros_death_peaks_into_the_dying_callback() {
    use regent_core::data::desc::{Action, Part};
    use regent_core::sim::world::walking::DYING_DONE;

    let data = read(&common::hd().join("Data").join("action.cam"));
    let archive = cam::Archive::parse(&data).expect("action.cam");
    let proc = |id: &[u8; 4]| {
        let record = archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"DACT" && r.id.0 == *id)
            .map(|(_, r)| r.data)
            .expect("the action");
        let part = Part::parse(record).expect("a part");
        let action = Action::parse(&part).expect("an action");

        Some(action.script().c_proc)
    };

    assert_eq!(proc(b"A005"), Some(DYING_DONE));
    assert_eq!(proc(b"A009"), Some(DYING_DONE));
    assert_eq!(proc(b"A010"), Some(0));
}

/// A set change keeps the frame only when both sets carry one of
/// `ImagSet` bits 17 to 19: in
/// HD only `BVm1`'s `Walk`, `Stand` and `Attack`. Every other
/// `InitAction` starts on frame 0.
#[test]
fn only_three_hd_sets_keep_their_frame_across_a_set_change() {
    use regent_core::data::imag::Image;
    use regent_core::sim::movement::action::FRAME_KEEPING_BITS;

    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let keeping: Vec<(FourCc, u32)> = catalogue
        .tagged(b"IMAG")
        .filter_map(|(_, r)| Some((r.id, Image::parse(r.data).ok()?)))
        .flat_map(|(id, i)| {
            i.sets
                .iter()
                .filter(|s| s.layer_flags() & FRAME_KEEPING_BITS != 0)
                .map(|s| (id, s.id))
                .collect::<Vec<_>>()
        })
        .collect();
    let bvm1 = FourCc(*b"BVm1");

    assert_eq!(keeping, [(bvm1, 0x1), (bvm1, 0x8), (bvm1, 0x10)]);
}

// ---------------------------------------------------------------------------
// Planning

// ---------------------------------------------------------------------------
// Home together

/// The Large Flyer, `ML04`, its unit object's, and a field:
/// `PRIM` words 6, 8 and 7. A least height of 180/32 cells, a lift of
/// 60/32, flags `0x31`: frameless steps, a facing kept through a jog.
/// No base class lobs (`Parabolic_Missile` is the expansion's). **Game**: the united winter
/// game's second ice dragon flies the probe's path frame for frame with
/// that lift and no other (**Measured**).
#[test]
fn the_large_flyer_holds_its_lift_in_word_8_and_flags_in_word_7() {
    let classes = classes();
    let flyer = classes[b"ML04"];

    assert_eq!((flyer.lift, flyer.flags), ((180, 60), 0x31));
    assert!(classes.values().all(|c| !c.lobbed()));
}

/// Every shipped `DMOV`, 146 in all three editions, is a version 2 `PRIM`
/// of 13 words whose last four, which the reader drops, are 0. Word 7
/// holds only flag bits, the lob on one class, `PL04` `Parabolic_Missile`; word 8,
/// the lift, is set only where word 6, the least height, is (**Corpus**).
#[test]
fn every_shipped_dmov_keeps_its_flags_in_word_seven() {
    /// The bits code tests in [`Class::flags`], and 2, the missiles',
    /// whose reader is not found.
    const KNOWN: u32 = FRAMELESS | 0x2 | LOBBED | TRAMPLES | KEEPS_FACING;

    let roots = [common::hd()];
    let mut seen = 0;
    let mut lobbed = BTreeSet::new();

    for path in roots.iter().flat_map(|r| common::archives(r)) {
        let data = read(&path);
        let Ok(archive) = cam::Archive::parse(&data) else {
            continue;
        };

        for (_, record) in archive.records().filter(|(c, _)| c.tag == b"DMOV") {
            let tree = Part::parse(record.data).expect("a DMOV tree");
            let prim = tree
                .parts
                .iter()
                .flat_map(|p| std::iter::once(p).chain(p.parts.iter()))
                .find(|p| &p.tag.0 == b"PRIM")
                .expect("a PRIM");
            let words: Vec<u32> = prim.body[..Class::LEN]
                .chunks(4)
                .map(|w| u32::from_le_bytes(w.try_into().expect("a word")))
                .collect();
            let class = Class::parse(record.data).expect("a DMOV");

            assert_eq!(prim.version, 2, "{}", path.display());
            assert_eq!(words[9..], [0; 4], "{}", path.display());
            assert_eq!((class.flags, class.lift.1), (words[7], words[8]));
            assert_eq!(class.flags & !KNOWN, 0, "{:#x}", class.flags);
            assert!(words[8] == 0 || words[6] != 0, "{words:?}");

            if class.lobbed() {
                lobbed.insert(record.id.0);
            }
            seen += 1;
        }
    }

    assert_eq!(seen, 62);
    assert_eq!(lobbed, BTreeSet::from([*b"PL04"]));
}

/// A walk's end plays set `0xb1` where the art has it, else a looping
/// `Stand`: of HD's 556 bodies with
/// art, only the Daemonwood and the Dryad have it.
#[test]
fn only_two_arts_end_a_walk_on_set_0xb1() {
    let bodies = common::bodies();
    let mut ending: Vec<&str> = bodies
        .values()
        .filter(|b| {
            b.poses
                .as_ref()
                .is_some_and(|p| p.contains_key(&PATH_END_SET))
        })
        .map(|b| b.name.as_str())
        .collect();
    let drawn = bodies.values().filter(|b| b.poses.is_some()).count();

    ending.sort_unstable();

    assert_eq!((drawn, ending), (556, vec!["Daemonwood", "Dryad"]));
}

/// A walk's `Stand` loops into the next walk where nothing stops it
/// (`GE_OrderProcessor` pushes the `MovePath` alone), but only a
/// `Stand` of more than one frame loops, and a frameless class's path
/// plays `Walk` over it. Of HD's bodies with `Walk` and such a `Stand`,
/// only the Dryad walks with frames, and its walk ends on set `0xb1`: so
/// only an after-set's `Stand` loops on into a Dryad's walk.
#[test]
fn only_the_dryad_steps_walk_frames_past_a_looping_stand() {
    let bodies = common::bodies();
    let mut stepped: Vec<&str> = bodies
        .values()
        .filter(|b| {
            let poses = b.poses.as_ref();
            let looping = poses
                .and_then(|p| p.get(&STAND).zip(p.get(&WALK)))
                .is_some_and(|(stand, _)| stand.frames.iter().any(|&f| f > 1));

            looping && b.gait.as_ref().is_some_and(|g| g.flags & FRAMELESS == 0)
        })
        .map(|b| b.name.as_str())
        .collect();

    stepped.sort_unstable();

    assert_eq!(stepped, ["Dryad"]);
}

/// Both arts' set `0xb1` has frames in every slot and keeps no frame, so
/// a walk's end pushes its `Animate` on track 0 from frame 0
/// and holds track 0 `frames` runs: the Daemonwood's
/// 3 at 100 ms, the Dryad's 4 at 150 ms.
#[test]
fn both_walk_end_sets_play_on_track_zero() {
    let bodies = common::bodies();
    let mut ends: Vec<(&str, Vec<u16>, u32, u32)> = bodies
        .values()
        .filter_map(|b| {
            let end = b.poses.as_ref()?.get(&PATH_END_SET)?;

            Some((b.name.as_str(), end.frames.clone(), end.rate(), end.keeps))
        })
        .collect();

    ends.sort_unstable();

    assert_eq!(
        ends,
        [
            ("Daemonwood", vec![3; 8], 100, 0),
            ("Dryad", vec![4; 8], 150, 0),
        ]
    );
}

/// The set a walk's end plays where the art has it.
const PATH_END_SET: u32 = 0xb1;

/// 53 HD bodies with a gait lack `Walk` or `Stand` in their art:
/// missiles, spell units, the Strangleweed, the illusionary hero. In HD
/// `UnitIMG` then fails and the parts keep their set; a step still
/// moves part 0's frame on, wrapping at the shown set's frames
/// (move set position; `Walker`'s
/// `a_step_without_walk_steps_the_set_shown`).
#[test]
fn some_walkers_art_lacks_walk_or_stand() {
    let bodies = common::bodies();
    let lacking: BTreeSet<&str> = bodies
        .values()
        .filter(|b| b.gait.is_some())
        .filter(|b| {
            b.poses
                .as_ref()
                .is_some_and(|p| !(p.contains_key(&WALK) && p.contains_key(&STAND)))
        })
        .map(|b| b.name.as_str())
        .collect();

    assert_eq!(lacking.len(), 53);
    assert!(lacking.contains("Strangleweed") && lacking.contains("illusionary_hero"));
}

/// The Liche Queen (`BVj1`, `Info` `DontBlock` alone) is on no collision
/// layer (`Info` bits 4 and 5): she blocks no one and nothing blocks
/// her, buildings included. Krolm (`BV01`) and the Black Phantom (`BVa1`)
/// are on the ground layer.
#[test]
fn the_liche_queen_walks_on_no_collision_layer() {
    const GROUND: u32 = 0x10;

    let bodies = common::bodies();
    let layer = |code: &[u8; 4]| bodies[&FourCc(*code)].layer & LAYERS;

    assert_eq!(layer(b"BVj1"), 0);
    assert_eq!(layer(b"BV01"), GROUND);
    assert_eq!(layer(b"BVa1"), GROUND);
}
