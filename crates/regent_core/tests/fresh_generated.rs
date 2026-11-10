//! A new game made with no save (`regent_core::sim::mapgen::fresh`) from the start
//! trace's picks alone: the generator's ground, scatter and start, the
//! setup made from the picks. It is the world the start save gives, its
//! drawing holds the start save's ground and scatter, and the quiet day
//! from it reaches the day1 save's script state.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use common::read;
use std::collections::BTreeMap;

use regent_core::data::fourcc::FourCc;
use regent_core::data::rgs;
use regent_core::gpl::load::Program;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::freestyle::Picks;
use regent_core::sim::freestyle::{self, List};
use regent_core::sim::game::check::Refusal;
use regent_core::sim::game::{Game, Order};
use regent_core::sim::mapgen::custom::{self, Art};
use regent_core::sim::mapgen::fresh::{self};
use regent_core::sim::setup;
use regent_core::sim::world::{self, World};

/// The day's GPL seed.
const SEED: u32 = 0x0b60_9e2c;

/// The freestyle quest and the chart, under the install.
const QUEST: &str = "QuestsMX/mx_Freestyle.q";
const CHART: &str = "DataMX/mx_constants.rgs";

/// A setup's picks as the dialog gives them.
const fn chose(settlement: [u8; 4], scenario: [u8; 4], land: [u8; 4], seed: i32) -> Picks {
    Picks {
        land: Some(FourCc(land)),
        settlement: Some(FourCc(settlement)),
        scenario: Some(FourCc(scenario)),
        monsters: Some(FourCc(*b"NONE")),
        seed: Some(seed),
        side: None,
        players: None,
    }
}

/// The fresh world `picks` make, with [`GOLD`].
fn founded(program: &Program, picks: &Picks) -> World {
    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let quest = read(&find(QUEST));
    let chart = read(&find(CHART));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let art = Art::of(&catalogue);
    let choice = custom::chosen(picks, &map).expect("choice");
    let made = fresh::generate(&map, &patterns, choice, GOLD, &art).expect("made");

    setup::fresh(&catalogue, program, &made.start())
}

/// The default game's gold.
const GOLD: u32 = 30_000;

/// A Map Number below one is raised to one, as the dialog does
/// (**Measured**), and the setup holds the
/// number the map was drawn from: a game rebuilt from it draws the same.
#[test]
fn a_negative_map_number_draws_and_holds_map_one() {
    const NEGATIVE: i32 = -5;

    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let quest = read(&find(QUEST));
    let chart = read(&find(CHART));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let art = Art::of(&catalogue);
    let picks = chose(*b"Cham", *b"Gobf", *b"East", NEGATIVE);
    let choice = custom::chosen(&picks, &map).expect("choice");
    let made = fresh::generate(&map, &patterns, choice, GOLD, &art).expect("made");

    assert_eq!(made.choice.seed, custom::MAP_NUMBER_MIN);
    assert_eq!(made.setup.seed(), freestyle::Setup::SEED_MIN);
}

/// A new game of two players: the first Player Forces and Enemy Forces
/// entries for two. Slots 0 and 1 are human seats, each with the gold and
/// a palace of its own; slot 7 holds the rest.
#[test]
fn a_two_player_game_seats_each_player_with_a_palace() {
    const TWO: u32 = 2;

    let program = common::sdk_program();
    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let quest = read(&find(QUEST));
    let chart = read(&find(CHART));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let first = |list| {
        freestyle::offers(&map, list, TWO)
            .first()
            .expect("offered")
            .code
    };
    let picks = Picks {
        settlement: Some(first(List::Settlement)),
        scenario: Some(first(List::Scenario)),
        monsters: Some(FourCc(*b"NONE")),
        seed: Some(1),
        players: Some(TWO),
        ..Picks::default()
    };
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let art = Art::of(&catalogue);
    let choice = custom::chosen(&picks, &map).expect("choice");
    let made = fresh::generate_for(&map, &patterns, choice, (GOLD, TWO), &art).expect("made");
    let world = setup::fresh(&catalogue, &program, &made.start());
    let opening = world.opening();
    let gold = i32::try_from(GOLD).expect("gold");

    assert_eq!(world.humans(), 0..TWO);
    assert_eq!(opening.players, [0, 1, 7]);
    assert_eq!(opening.gold, BTreeMap::from([(0, gold), (1, gold)]));

    let palaces: Vec<Option<u32>> = (0..TWO).map(|p| world.player_palace(p)).collect();

    assert!(palaces.iter().all(Option::is_some), "{palaces:?}");
    assert_ne!(palaces[0], palaces[1]);
}

/// A `openregent --bot` session of two seats on Arcane Ally against the
/// Ruins of Esterhausen, K'Hal-Kratis, map 760930813: its orders to
/// frame 1590, each line before its frame.
const SEATED_ORDERS: [(u32, &str); 28] = [
    (3, "p0 unpreview ABV1"),
    (3, "p0 build ABV1 864 3200 0"),
    (3, "p1 flag ARA2 192 3520 1 200 2"),
    (203, "p0 unpreview ABE1"),
    (203, "p0 build ABE1 544 3232 0"),
    (303, "p0 research 2 5030 0"),
    (303, "p1 flag ARA2 2880 960 1 200 5"),
    (403, "p0 unpreview ABV1"),
    (403, "p0 build ABV1 1056 3488 0"),
    (403, "p1 flag ARA4 448 2791 1 200 0"),
    (503, "p0 flag ARA4 0 4265 0 200 0"),
    (503, "p1 flag ARA2 1056 3488 1 200 15"),
    (603, "p1 alter 0 9 APR:1 100 0 0"),
    (703, "p0 cast WVA1 716 3836 0"),
    (803, "p0 flag ARA2 2880 960 0 200 5"),
    (803, "p1 flag ARA2 320 3840 1 200 3"),
    (903, "p0 unpreview ABG1"),
    (903, "p0 build ABG1 608 3904 0"),
    (1003, "p0 alter 0 19 APR:1 100 0 0"),
    (1003, "p1 flag ARA2 464 3312 1 200 12"),
    (1103, "p0 cast WVA1 351 3346 0"),
    (1203, "p0 bless Sp11 2 0 331 3322"),
    (1303, "p0 recruit 7 AVN1"),
    (1303, "p1 flag ARA4 662 3358 1 200 0"),
    (1403, "p0 research 2 5031 0"),
    (1403, "p1 flag ARA4 857 2868 1 200 0"),
    (1503, "p0 enchant 3"),
    (1503, "p1 flag ARA4 964 3333 1 200 0"),
];

/// Past the session's stop: frame 1590, `Crash::FreedNode`.
const SEATED_FRAMES: u32 = 1_700;

/// The bot's Invisibility on its own Library, agent 2 (frame 1203), is an
/// order no cursor sends (`World::spell_target`): the check drops it. Let
/// through, `Invisibility_End` read the building's missing
/// `original_type` at frame 1303, and an attack flag on it ran
/// `target_eval` on a building, whose `attacktype` throws inside
/// `check_rewards`' loop; the Wizard's next call met the cursor left on
/// a freed node.
#[test]
fn a_two_seat_game_drops_a_spell_no_cursor_sends() {
    const TWO: u32 = 2;

    let program = common::sdk_program();
    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let quest = read(&find(QUEST));
    let chart = read(&find(CHART));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let picks = Picks {
        players: Some(TWO),
        ..chose(*b"Wizb", *b"Ruib", *b"KxHa", 760_930_813)
    };
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let art = Art::of(&catalogue);
    let choice = custom::chosen(&picks, &map).expect("choice");
    let made = fresh::generate_for(&map, &patterns, choice, (GOLD, TWO), &art).expect("made");
    let mut world = setup::fresh(&catalogue, &program, &made.start());
    let vm = Vm::new(&program, Config::RELEASE);
    let mut game = Game::free(vm, SEED, &mut world);

    for (frame, line) in SEATED_ORDERS {
        let order: Order = line.parse().expect("order");

        game.input(frame, order).expect("on time");
    }

    game.setup(startup::FREESTYLE).expect("setup");

    for _ in 0..SEATED_FRAMES {
        game.frame().expect("frame");
    }

    let dropped: Vec<(u32, Refusal)> = game
        .take_dropped()
        .iter()
        .map(|d| (d.frame, d.why))
        .collect();

    assert_eq!(dropped, [(1203, Refusal::Target)]);
    assert_eq!(game.reports().count, 0, "{:?}", game.reports().first);
}

/// `LearnSpell` gives a unit an entry of the spell's code, ready, beyond
/// its descriptor's own: the one call of Champions of Law against the
/// Goblin Stronghold, map 470427919.
#[test]
fn a_learned_spell_joins_the_units_list_ready() {
    let program = common::sdk_program();
    let mut world = founded(&program, &chose(*b"Cham", *b"Gobf", *b"East", 470_427_919));
    let vm = Vm::new(&program, Config::default());
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let bodies = regent_core::sim::setup::descriptors(&catalogue)
        .bodies()
        .clone();
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");

    let learned = |g: &Game<'_>| g.calls().get("LearnSpell").copied().unwrap_or(0);
    let mut frames = 0;

    while learned(&game) == 0 && frames < LEARN_BY {
        game.frame().expect("frame");
        frames += 1;
    }

    let w = game.world();
    let taught: Vec<(u32, FourCc, u32)> = (0..AGENTS)
        .filter_map(|id| Some((id, w.piece(id)?)))
        .flat_map(|(id, p)| {
            let own = bodies
                .get(&p.code)
                .map(|b| b.spells.clone())
                .unwrap_or_default();

            p.spells
                .iter()
                .flatten()
                .filter(|c| !own.contains(&c.code))
                .map(|c| (id, c.code, c.timeout))
                .collect::<Vec<_>>()
        })
        .collect();

    println!("frame {frames}: {taught:?}");
    assert_eq!(learned(&game), 1);
    assert_eq!(taught.len(), 1);
    assert_eq!(taught[0].2, 0, "ready");
}

/// A start unit whose descriptor has a `NameGenType` draws its name as
/// the generators are seeded, before the game's first: the Broken
/// Alliance's inn, the one in the freestyle quest, takes `NM17`'s first.
#[test]
fn a_start_inn_takes_the_first_name() {
    let program = common::sdk_program();
    let mut world = founded(&program, &chose(*b"Brok", *b"Ruib", *b"East", 1));
    let mut names = world.names().cloned().expect("generators");
    let name = world::naming::NAME.packed();
    let inn = FourCc(*b"ABF1");
    let inn_names = |w: &World| -> Vec<Option<i32>> {
        w.opening()
            .pieces
            .values()
            .filter(|p| p.code == inn)
            .map(|p| p.attributes.get(&name).copied())
            .collect()
    };

    assert_eq!(inn_names(&world), [None]);

    world.seed_names(SEED);
    names.reseed(SEED);

    let first = names.next(FourCc(*b"NM17")).map(u32::cast_signed);

    assert_eq!(inn_names(&world), [first]);
    assert_eq!(world.names(), Some(&names));
}

/// Frames within which the Champions' game learns its spell.
const LEARN_BY: u32 = 6000;

/// Agent ids searched for a learner.
const AGENTS: u32 = 1000;

/// `TurnOnSpeedTrail`: the East Cheswick war party's `Speed_Monster` trails its
/// target with two clones, after the spell's
/// effectors' handles, its own handle in turn, each hidden on its leader
/// as made, where the unit stood. Then each run a clone stands where the clone before it stood
/// a frame ago, hidden where that is where it stands now.
#[test]
fn a_speed_spell_trails_its_unit_with_clones() {
    let program = common::sdk_program();
    let mut world = founded(&program, &chose(*b"Wizb", *b"Cona", *b"East", 1));
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");

    let trailed = |g: &Game<'_>| g.calls().get("TurnOnSpeedTrail").copied().unwrap_or(0);
    let mut frames = 0;

    while trailed(&game) == 0 && frames < LEARN_BY {
        game.frame().expect("frame");
        frames += 1;
    }

    let w = game.world();
    let clones = w.clones().to_vec();
    let unit = clones.first().map(|c| c.unit).expect("a clone");
    let n = w
        .piece(unit)
        .and_then(|p| p.attributes.get(&world::trail::NUM_CLONES.packed()))
        .copied();
    let effectors: Vec<u32> = w
        .effectors()
        .iter()
        .filter(|e| e.agent == unit)
        .map(|e| e.handle)
        .collect();

    println!("frame {frames}: unit {unit}, clones {clones:?}");
    assert!(
        game.faults().unknown.is_empty(),
        "{:?}",
        game.faults().unknown
    );
    assert_eq!(n, Some(SPEED_CLONES));
    assert_eq!(clones.len(), 2);
    assert_eq!(clones[1].handle, clones[0].handle + 1);
    assert!(effectors.iter().all(|e| *e < clones[0].handle));
    assert_eq!(clones[0].leader, world::trail::Leader::Unit(unit));
    assert_eq!(
        clones[1].leader,
        world::trail::Leader::Clone(clones[0].handle)
    );

    // Made where the unit stood at the cast, in the frame's orders.
    assert_eq!(clones[0].shows.fixed, clones[1].shows.fixed);
    assert!(
        clones
            .iter()
            .all(|c| c.shown == world::trail::Showing::Hidden)
    );

    // Each frame: clone 1 where clone 0 was a frame ago.
    for _ in 0..FOLLOWED {
        let before = game.world().clones()[0].shows.fixed;

        game.frame().expect("frame");

        let now = game.world().clones();
        let hidden = now[1].shows.fixed == now[0].shows.fixed;

        assert_eq!(now[1].shows.fixed, before);
        assert_eq!(now[1].shown == world::trail::Showing::Hidden, hidden);
    }
}

/// `Speed_Monster_Begin`'s `TurnOnSpeedTrail(Target, 2)`.
const SPEED_CLONES: i32 = 2;

/// Frames the trail is followed.
const FOLLOWED: usize = 40;
