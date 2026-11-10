//! Scenes (`sim::scene`): a world set up by hand stands as asked, and
//! plays the same every time.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use std::path::PathBuf;

use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::game::Game;
use regent_core::sim::scene::{self, Error, Placed, Scene};
use regent_core::sim::world::{CROWN, World, new_map::NEUTRAL};

const PALACE: FourCc = FourCc::new(b"ABJ1");
const WARRIOR: FourCc = FourCc::new(b"AVL1");
const RATS: FourCc = FourCc::new(b"BBx1");

/// The quiet day's GPL seed.
const SEED: u32 = 0x0b60_9e2c;

/// Frames the determinism check runs: 30 game seconds.
const FRAMES: u32 = 600;

/// The example scene, beside the workspace.
fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/scenes/palace_shadow.toml")
}

/// A palace, a hurt warrior with gold west of it, a neutral lair.
fn camp() -> Scene {
    Scene::flat(128)
        .unit(Placed::new(PALACE, [64.0, 64.0]))
        .unit(
            Placed::new(WARRIOR, [60.5, 64.0])
                .facing(8)
                .set("HP", 20)
                .set("Gold", 500),
        )
        .unit(Placed::new(RATS, [90.0, 90.0]).owner(NEUTRAL))
}

/// `scene`'s game set up, then `frames` run: its digest each frame, and
/// the world.
fn played(scene: &Scene, program: &Program, frames: u32) -> (Vec<u64>, World) {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let mut world = scene.flat_world(&catalogue, program).expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let vm = Vm::new(program, Config::RELEASE);
    let mut game = Game::free(vm, SEED, &mut world);

    staging.run(&mut game, startup::FREESTYLE).expect("set up");

    let digests = (0..frames)
        .map(|_| {
            game.frame().expect("a frame");
            game.digest()
        })
        .collect();

    // No script stumbled, and nobody won or lost.
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
    drop(game);
    assert_eq!(world.ended(CROWN), None);
    (digests, world)
}

/// The agent of the piece with `code`, the only one.
fn only(world: &World, code: FourCc) -> u32 {
    let ids: Vec<u32> = (0..world.next_id())
        .filter(|id| world.piece(*id).is_some_and(|p| p.code == code))
        .collect();

    assert_eq!(ids.len(), 1, "{code}: {ids:?}");
    ids[0]
}

/// Each placed unit stands where asked, owned as asked, with an agent;
/// the warrior's attributes are the scene's after the setup.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_scenes_units_stand_where_placed() {
    let program = common::sdk_program();
    let (_, world) = played(&camp(), &program, 0);

    for placed in &camp().units {
        let id = only(&world, placed.code);
        let piece = world.piece(id).expect("its piece");

        assert_eq!(piece.owner, placed.owner, "{}", placed.code);
        assert_eq!(world.position(id), Some(placed.pos32()), "{}", placed.code);
    }

    let warrior = world.piece(only(&world, WARRIOR)).expect("the warrior");

    assert_eq!(attrib::get(&warrior.attributes, attrib::HP), Some(20));
    assert_eq!(attrib::get(&warrior.attributes, attrib::GOLD), Some(500));
    assert_eq!(world.player_palace(0), Some(only(&world, PALACE)));
}

/// A placed unit's items, read as a script's item arguments: the
/// slot `INV:11`, and a text by its id; each held at 0 after the setup.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_scenes_unit_carries_its_items() {
    let program = common::sdk_program();
    let text = "version = 1\nmap = { flat = { side = 128 } }\n\
                [[unit]]\ncode = \"ABJ1\"\nat = [64, 64]\n\
                [[unit]]\ncode = \"AVL1\"\nat = [60.5, 64]\nitems = [11, \"MagicBall\"]\n";
    let read: Scene = toml::from_str(text).expect("a scene");
    let ball = scene::Item::Named("MagicBall".to_owned());
    let built = Scene::flat(128)
        .unit(Placed::new(PALACE, [64.0, 64.0]))
        .unit(
            Placed::new(WARRIOR, [60.5, 64.0])
                .carry(scene::Item::Slot(11))
                .carry(ball.clone()),
        );

    assert_eq!(read, built);

    let (_, world) = played(&built, &program, 0);
    let warrior = world.piece(only(&world, WARRIOR)).expect("the warrior");
    let ring = regent_core::data::attrib::Key::new(*b"INV", 11).packed();

    for key in [ring, ball.key()] {
        assert_eq!(warrior.attributes.get(&key), Some(&0), "{key:#x}");
    }
}

/// A scene set up twice, then played, is the same game frame for frame.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_scene_plays_the_same_twice() {
    regent_core::base::digest::shuffle();

    let program = common::sdk_program();
    let (first, _) = played(&camp(), &program, FRAMES);
    let (second, _) = played(&camp(), &program, FRAMES);

    assert_eq!(first, second);
}

/// A scene's day starts the clock there: the setup at it, the first
/// frame 50 ms on, the frames run counted from 0.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_scenes_day_sets_the_clock() {
    let program = common::sdk_program();
    let scene = camp().day(2).second(30);
    let start = 2 * scene::DAY_MS + 30_000;

    assert_eq!(scene.clock(), start);
    assert_eq!(scene::DAY_MS, regent_core::view::panel::day::DAY_MS);

    let (_, world) = played(&scene, &program, 1);

    assert_eq!(world.now(), start + MS_PER_FRAME);
}

/// The example file reads as the builder makes it.
#[test]
fn the_example_scene_file_is_the_builders() {
    let text = std::fs::read_to_string(example()).expect("the example");
    let read: Scene = toml::from_str(&text).expect("a scene");
    let built = Scene::flat(128)
        .unit(Placed::new(PALACE, [64.0, 64.0]))
        .unit(Placed::new(WARRIOR, [57.0, 68.0]));

    assert_eq!(read, built);
}

/// What a scene refuses: another version, a field it lacks, a code no
/// descriptor has, a place off the map, an owner with no seat, an
/// attribute the engine does not name.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn a_bad_scene_is_refused() {
    let program = common::sdk_program();
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let made = |scene: Scene| scene.flat_world(&catalogue, &program).err();
    let ghost = FourCc::new(b"ZZZZ");

    assert_eq!(
        made(Scene {
            version: 2,
            ..Scene::flat(128)
        }),
        Some(Error::Version(2))
    );
    assert_eq!(made(Scene::flat(100)), Some(Error::Side(100)));
    assert_eq!(
        made(Scene::flat(128).unit(Placed::new(ghost, [1.0, 1.0]))),
        Some(Error::Unknown(ghost))
    );
    assert_eq!(
        made(Scene::flat(128).unit(Placed::new(PALACE, [128.0, 1.0]))),
        Some(Error::Off(PALACE, [128.0, 1.0]))
    );
    assert_eq!(
        made(Scene::flat(128).unit(Placed::new(PALACE, [9.0, 9.0]).owner(1))),
        Some(Error::Owner(PALACE, 1))
    );

    let scene = Scene::flat(128).unit(Placed::new(WARRIOR, [9.0, 9.0]).set("Health", 1));
    let world = scene.flat_world(&catalogue, &program).expect("a world");

    assert_eq!(
        scene.staging(&world),
        Err(Error::Attribute(WARRIOR, "Health".to_owned()))
    );
    assert!(
        toml::from_str::<Scene>("version = 1\nmap = { flat = { side = 128 } }\nfoo = 1").is_err()
    );
}
