//! Missiles: where an action with a projectile aims, the GPL RNG draws its
//! aim and launch take at a building, and what its flight reads.
//! `sim::world::missile`; its impacts in play5 are checked in
//! `tests/trace.rs`.
//!
//! Needs `MAJESTY_HD`.

mod common;

use regent_core::data::fourcc::FourCc;
use regent_core::sim::movement::{Finish, rate, step_length};
use regent_core::sim::units::from_pos32;
use regent_core::sim::world::missile::{
    Hit, LOB_STEEPEST, MAKE_ATTACK, POINT_FORM, Script, lob_index,
};

/// An `InitAction` fires callback `0x1000` on the first frame whose
/// reference carries bit 31 (`imag::CUE`), else on its last.
/// The ratman shaman's `Cast` cues
/// frame 4 of 9: its missile leaves 300 ms after the push, not 700.
/// **Game**: play5's shaman casts `Infectious_Cloud` at the inn at
/// 290,100 ms; the launch's two draws fall between the log's draws at
/// 290,300 and 290,500 ms, and the impact runs at 290,650 ms.
#[test]
fn a_cast_launches_at_its_cued_frame() {
    /// `Cast`.
    const CAST: u32 = 0x80;
    /// The shaman's facing toward the inn.
    const FACING: i32 = 16;

    let bodies = common::bodies();
    let pose = bodies
        .get(&FourCc(*b"BVs1"))
        .and_then(|b| b.poses.as_ref()?.get(&CAST).cloned())
        .expect("the shaman's cast");

    assert_eq!((pose.frames_at(FACING), pose.rate()), (9, 100));
    assert_eq!(pose.peak_ms(FACING), 300);
}

/// The catapult's ball.
const BALL: FourCc = FourCc(*b"BPA7");

/// The original impact hook returns `make_attack` missiles only. Acid
/// Bolt has its own hit script, so the mirror cannot return that spell.
#[test]
fn magic_mirror_returns_attacks_but_not_acid_bolts() {
    let bodies = common::bodies();
    let callback = |code| {
        bodies
            .get(&FourCc(code))
            .and_then(|body| body.script.as_ref())
            .map(|script| script.function.as_str())
    };

    assert_eq!(callback(*b"BPA1"), Some(MAKE_ATTACK));
    assert_eq!(callback(*b"NPa1"), Some("Acid_Bolt_Hit"));
}

/// Every HD action's projectile homes on its target
/// but the catapult's ball, the one
/// `Parabolic_Missile`: [`movement::Class::lobbed`], so `GenUnit`
/// lobs it.
#[test]
fn every_projectile_homes_but_the_catapults_ball() {
    let bodies = common::bodies();
    let (named, _) = common::lookups();
    let mut lobbed: Vec<FourCc> = named
        .actions
        .values()
        .filter_map(|a| a.projectile)
        .filter(|m| {
            bodies
                .get(&m.code)
                .and_then(|b| b.gait)
                .is_some_and(|g| g.lobbed())
        })
        .map(|m| m.code)
        .collect();

    lobbed.dedup();

    assert!(
        named
            .actions
            .values()
            .filter(|a| a.projectile.is_some())
            .count()
            > 20
    );
    assert_eq!(lobbed, vec![BALL]);
}

/// method's angle: `ParabolaAngle` 75 indexes entry 53 of the 256-entry
/// tables; its steepest shot is `str2fixed` of "0.9".
#[test]
fn a_lob_indexes_entry_53_and_steepens_to_0_9() {
    assert_eq!(lob_index(75), 53);
    assert_eq!(LOB_STEEPEST.to_bits(), 9 * 0x1_0000 / 10);
}

/// Where a missile aims on a unit is read from its art: `Hotspot` point 1's
/// x and point 5's y, unprojected; without a
/// `Hotspot`, set 8's point 3 by slot. The ratman
/// champion ranger 36 shot at 220,700 ms: point 1 at (-2, -29), no point
/// 5, so 29 high. The inn: point 1 at (12, 68), point 5 at (-15, 109), so
/// 133 deep and 41 high.
#[test]
fn a_missiles_aim_is_read_from_the_targets_art() {
    let bodies = common::bodies();
    let hit = |code: &[u8; 4]| bodies.get(&FourCc(*code)).map(|b| b.hit.clone());

    assert_eq!(
        hit(b"BVr1"),
        Some(Hit::Attached {
            xs: [1, -2, 0],
            y: 0,
            rises: [77, 29, -2],
        })
    );
    assert_eq!(
        hit(b"ABF1"),
        Some(Hit::Attached {
            xs: [-2, 12, 44],
            y: 133,
            rises: [137, 41, 0],
        })
    );
    assert!(
        bodies
            .values()
            .any(|b| matches!(b.hit, Hit::Standing(ref xs) if !xs.is_empty()))
    );
}

/// A projectile's impact function and `Script type` are read from its
/// `PRIM`, type and `cProc` before the name: the
/// ratapult's `Ratapult_hit` takes a point (type 1 in the SDK's
/// `MX_Projectiles.xml`), the infectious cloud's `Infectious_Cloud_Hit`
/// shooter and target; a ranger's arrow runs `make_attack` at its impact.
#[test]
fn a_projectiles_function_and_form_are_read_from_its_prim() {
    let bodies = common::bodies();
    let script = |code: &[u8; 4]| bodies.get(&FourCc(*code)).and_then(|b| b.script.clone());
    let named = |function: &str, kind: u32| {
        Some(Script {
            function: function.to_owned(),
            kind,
        })
    };

    assert_eq!(script(b"BPA7"), named("Ratapult_hit", POINT_FORM));
    assert_eq!(script(b"BPB8"), named("Infectious_Cloud_Hit", 0));
    assert_eq!(script(b"APA2"), named(MAKE_ATTACK, 0));
}

/// `fast missile`, every play5 projectile's movement, steps 1/32 of a
/// cell every 1 ms: under a frame, so each frame's run goes 50 of them,
/// a cell and a half.
#[test]
fn a_fast_missile_flies_a_cell_and_a_half_a_frame() {
    let bodies = common::bodies();
    let gait = bodies
        .get(&FourCc(*b"BPB8"))
        .and_then(|b| b.gait)
        .expect("a movement");
    let every = rate(gait.move_rate, 0);

    assert_eq!((gait.move_rate, gait.move_dist, every), (1, 1, 1));
    assert_eq!(
        step_length(gait.move_dist, every, Finish::Exact),
        from_pos32(50)
    );
}
