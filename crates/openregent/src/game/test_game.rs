//! `Sim` on the made-up game ([`regent_testgame`]), with no game data: recorded,
//! replayed, saved and gone on, as a game on an install is.

use regent_core::gpl::startup;
use regent_core::gpl::vm::Config;
use regent_core::sim::game::{Input, orders};
use regent_core::sim::scene::{Placed, Scene};
use regent_core::sim::world::build::BuildOrder;
use regent_core::sim::world::{CROWN, new_map::NEUTRAL};
use regent_testgame::{BITER, HUT, KEEP, TestGame, WALKER, all, placed};

use crate::game::drive;
use crate::game::record::{self, DIGEST_EVERY, Event, Recorder, Session};
use crate::game::sim::Sim;
use crate::save::Rewards;
use crate::setup::Setup;

const SEED: u32 = 7;
const OTHER_SEED: u32 = 8;
const FRAMES: u32 = 400;
const BUILD_AT: u32 = 30;
const DEMOLISH_AT: u32 = 200;
const GOLD: u32 = 1000;

/// A keep, a hut, and a walker with a biter to fight: rolls, walks and
/// income for every digest to follow.
fn scene() -> Scene {
    Scene::flat(128)
        .gold(GOLD)
        .unit(Placed::new(KEEP, [64.0, 64.0]))
        .unit(Placed::new(HUT, [50.0, 40.0]))
        .unit(Placed::new(WALKER, [40.0, 60.0]))
        .unit(Placed::new(BITER, [42.0, 60.0]).owner(NEUTRAL))
}

/// [`scene`]'s game, as `Plan::begin` makes a scene's.
fn sim(seed: u32) -> Sim {
    let game = TestGame::get();
    let program = game.program();
    let scene = scene();
    let world = scene
        .flat_world(&game.catalogue(), program)
        .expect("a world");
    let staging = scene.staging(&world).expect("staged");
    let begun = (seed, startup::FREESTYLE);

    Sim::on(program, world, begun, &staging, Config::RELEASE).expect("set up")
}

fn setup(seed: u32) -> Setup {
    Setup {
        seed,
        ..Setup::default()
    }
}

/// A fresh scratch folder of its own.
fn scratch(name: &str) -> crate::testdata::Scratch {
    crate::testdata::scratch(&format!("test_game-{name}"))
}

/// A player's frames: a hut ordered, the first hut demolished later.
fn given(sim: &mut Sim) {
    for frame in 0..FRAMES {
        if frame == BUILD_AT {
            let build = BuildOrder {
                code: HUT,
                at: (30 * 32, 30 * 32),
                owner: CROWN,
            };

            for input in orders::build(build) {
                sim.input(input);
            }
        }

        if frame == DEMOLISH_AT {
            sim.input(Input::Demolish(placed(1)));
        }

        sim.step();
    }
}

/// What a replay must match.
fn state(sim: &Sim) -> impl PartialEq + std::fmt::Debug {
    let world = sim.world();

    (
        world.shown(),
        world.treasury(CROWN),
        world.now(),
        sim.digest(),
        sim.frames(),
        sim.reports().count,
        sim.stopped.clone(),
    )
}

/// A game played and recorded to a file replays from it to the same world,
/// and a replay records the same file.
#[test]
fn a_recorded_game_replays_to_the_same_world() {
    let dir = scratch("replays");
    let again = dir.join("again");

    std::fs::create_dir_all(&again).expect("scratch");

    let mut played = sim(SEED);
    let empty = Session {
        setup: setup(SEED),
        events: Vec::new(),
    };

    played.record(Recorder::in_dir(&dir, &empty));
    given(&mut played);

    let session = record::read(&dir.join("play.input")).expect("recorded");
    let mut replayed = sim(session.setup.seed);

    replayed.record(Recorder::in_dir(&again, &empty));
    replayed.replay(session.events.clone());

    for _ in 0..FRAMES {
        replayed.step();
    }

    let rerecorded = std::fs::read_to_string(again.join("play.input")).ok();
    let inputs = session
        .events
        .iter()
        .filter(|e| matches!(e, Event::Input { .. }))
        .count();
    let digests = session.events.len() - inputs;

    // The orders took, and the fight ran.
    assert_eq!(all(played.world(), HUT).len(), 1);
    assert_ne!(all(played.world(), HUT), [placed(1)]);
    assert_eq!(played.reports().count, 0);
    assert_eq!(played.stopped, None);

    assert_eq!(inputs, 3);
    assert_eq!(digests, (FRAMES / DIGEST_EVERY) as usize);
    assert_eq!(rerecorded, Some(session.text()));
    assert!(!replayed.replaying());
    assert_eq!(state(&replayed), state(&played));
}

/// A recording's digests catch a game that parts: another seed stops at the
/// first digest line.
#[test]
fn a_replay_that_parts_stops_at_its_digest() {
    let dir = scratch("parts");
    let mut played = sim(SEED);

    played.record(Recorder::in_dir(
        &dir,
        &Session {
            setup: setup(SEED),
            events: Vec::new(),
        },
    ));
    given(&mut played);

    let session = record::read(&dir.join("play.input")).expect("recorded");
    let mut parted = sim(OTHER_SEED);

    parted.replay(session.events);

    for _ in 0..FRAMES {
        parted.step();
    }

    assert!(parted.stopped.is_some());
    assert_eq!(parted.frames(), DIGEST_EVERY);
}

/// The headless drive plays what steps play.
#[test]
fn the_headless_drive_plays_as_steps_do() {
    let mut driven = sim(SEED);
    let mut stepped = sim(SEED);

    drive::headless(&mut driven, FRAMES, &mut None, |_| ());

    for _ in 0..FRAMES {
        stepped.step();
    }

    assert_eq!(driven.frames(), FRAMES);
    assert_eq!(state(&driven), state(&stepped));
}

/// A game saved and made again from its snapshot goes on as the one that
/// never stopped.
#[test]
fn a_game_saved_goes_on_the_same() {
    const REWARDS: Rewards = Rewards {
        attack: 0,
        explore: 0,
    };

    let mut whole = sim(SEED);

    for _ in 0..FRAMES {
        whole.step();
    }

    let snapshot = whole.snapshot().expect("a snapshot");
    let game = TestGame::get();
    let program = game.program();
    let world = scene()
        .flat_world(&game.catalogue(), program)
        .expect("a world");
    let mut resumed =
        Sim::resume(program, world, snapshot, REWARDS, Config::RELEASE).expect("resumed");

    assert_eq!(resumed.digest(), whole.digest());

    for _ in 0..FRAMES {
        whole.step();
        resumed.step();
    }

    assert_eq!(state(&resumed), state(&whole));
    assert!(
        whole.world().treasury(CROWN) > Some(GOLD.cast_signed()),
        "the hut paid"
    );
}
