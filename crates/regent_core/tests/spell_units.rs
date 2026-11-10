//! Spell units (`World::expiry`) cast in a small scene: a palace and
//! monsters on flat ground.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::{Game, Input};
use regent_core::sim::scene::{Placed, Scene};
use regent_core::sim::world::spell::cast::Cast;
use regent_core::sim::world::{CROWN, World, new_map::NEUTRAL};

const PALACE: FourCc = FourCc::new(b"ABJ1");
const RAT: FourCc = FourCc::new(b"BVH1");

/// Wind storm's unit.
const STORM: FourCc = FourCc::new(b"DVB1");

/// `Wind_Storm_Active`'s `wind_storm_effector2`, hung on each target.
const BLOWN: FourCc = FourCc::new(b"DRB2");

/// `Wind_Storm_Hit`'s `SetSpellUnitTimeout`, ms.
const STORM_TIMEOUT: u32 = 4700;

/// Its art's set 0x40 from frame 8 to its last: 7 frames at 100 ms.
const STORM_LOOP_LEFT: u32 = 700;

/// The agents of the pieces with `code`.
fn all(world: &World, code: FourCc) -> Vec<u32> {
    (0..world.next_id())
        .filter(|id| world.piece(*id).is_some_and(|p| p.code == code))
        .collect()
}

/// Wind storm (`DVB1`: set 0x50, 9 frames, then 0x40, 15, no 0x60, all
/// 100 ms) cast on two rats by the palace. Its 4,700 ms timeout ends at
/// frame 8 of the loop; the loop plays on to its last, then the unit
/// goes. Its first blow, in the cast's frame, hangs its effector on
/// each rat and hurts it.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_wind_storm_ends() {
    let program = common::sdk_program();
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let scene = Scene::flat(128)
        .gold(5000)
        .unit(Placed::new(PALACE, [64.0, 64.0]))
        .unit(Placed::new(RAT, [72.0, 64.0]).owner(NEUTRAL))
        .unit(Placed::new(RAT, [72.0, 66.0]).owner(NEUTRAL));
    let mut world = scene.flat_world(&catalogue, &program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let mut game = Game::free(Vm::new(&program, Config::RELEASE), 1, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let rats = all(game.world(), RAT);
    let hp = |game: &Game<'_>| -> Vec<Option<i32>> {
        rats.iter()
            .map(|r| attrib::get(&game.world().piece(*r)?.attributes, attrib::HP))
            .collect()
    };
    let whole = hp(&game);
    let at = game.world().position(rats[0]).expect("placed");
    let cast = Input::Cast(Cast {
        code: STORM,
        at,
        owner: CROWN,
    });

    game.input(game.frames + 1, cast).expect("on time");
    game.frame().expect("a frame");

    let made = game.world().now();
    let storm = all(game.world(), STORM)[0];
    let end = made + STORM_TIMEOUT + STORM_LOOP_LEFT;

    assert_eq!(game.world().expiry(storm), Some(end));

    for (rat, (now, was)) in rats.iter().zip(hp(&game).into_iter().zip(whole)) {
        assert!(game.world().hung_on(*rat).any(|c| c == BLOWN), "{rat}");
        assert!(now < was, "{rat}: {now:?} {was:?}");
    }

    // Standing to the frame before its end; gone in the frame that
    // reaches it.
    while game.world().now() + MS_PER_FRAME < end {
        game.frame().expect("a frame");
    }

    assert!(game.world().piece(storm).is_some());

    game.frame().expect("a frame");

    assert_eq!(game.world().now(), end);

    assert!(game.world().piece(storm).is_none());
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}
