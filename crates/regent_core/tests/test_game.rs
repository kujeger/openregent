//! The made-up game ([`regent_testgame`]) on the real simulation: real
//! catalogue, compiled scripts, `Vm` and `Game`, with no game data.
//!
//! Each test states a fact of a run: a seed, orders and a frame count. No
//! digest is stored: a rule change must not break these by itself.

use regent_core::data::attrib;
use regent_core::data::catalogue::Library;
use regent_core::data::desc;
use regent_core::data::fourcc::FourCc;
use regent_core::sim::game::{Input, Order, orders};
use regent_core::sim::scene::{Error, Placed, Scene};
use regent_core::sim::world::build::{BuildOrder, FIRST_HP};
use regent_core::sim::world::defeat::End;
use regent_core::sim::world::demolish::refund;
use regent_core::sim::world::recruit::Recruit;
use regent_core::sim::world::{CROWN, World, new_map::NEUTRAL};

mod common;

use regent_testgame::{BITER, HUT, KEEP, LODGE, Played, Run, TestGame, WALKER, all, only, placed};

/// What [`TestGame`]'s rules hold: `fixtures/rules.gpl`, `units.xml`.
const KEEP_HP: i32 = 500;
const HUT_COST: i32 = 120;
const HUT_HP: i32 = 80;
const HUT_INCOME: i32 = 5;
const WALKER_COST: i32 = 40;
const WALK_CELLS: i32 = 20;
const BOUNTY: i32 = 50;
const HOARD_GOAL: u32 = 50_000;

/// Frames in a game second.
const SECOND: u32 = 20;

const CELL: i32 = 32;

/// Where the keep stands.
const MIDDLE: f32 = 64.0;
const SIDE: u32 = 128;

/// A keep on a flat map.
fn kingdom() -> Scene {
    Scene::flat(SIDE).unit(Placed::new(KEEP, [MIDDLE, MIDDLE]))
}

/// A cell in 32nds.
const fn at(x: i32, y: i32) -> (i32, i32) {
    (x * CELL, y * CELL)
}

/// `run` played, with nothing going wrong that a test did not ask for.
fn played(run: &Run) -> Played {
    let p = TestGame::get().play(run, |_, _| ());

    clean(&p);
    p
}

/// No script stumbled, and the world followed every walk.
#[track_caller]
fn clean(p: &Played) {
    assert_eq!(p.errors, Vec::<String>::new());
    assert_eq!(p.lost, vec![]);
}

fn hp(world: &World, id: u32) -> Option<i32> {
    attrib::get(&world.piece(id)?.attributes, attrib::HP)
}

/// A hut ordered at cell `(x, y)`, with its preview's delete, as a player's.
fn build_at(run: Run, frame: u32, x: i32, y: i32) -> Run {
    orders::build(BuildOrder {
        code: HUT,
        at: at(x, y),
        owner: CROWN,
    })
    .into_iter()
    .fold(run, |run, input| run.input(frame, input))
}

/// Each placed unit stands where asked, owned as asked; the keep is the
/// crown's palace, and its birth ran.
#[test]
fn placed_units_stand_where_asked() {
    let scene = kingdom()
        .unit(Placed::new(HUT, [50.0, 40.0]))
        .unit(Placed::new(BITER, [100.0, 100.0]).owner(NEUTRAL))
        .unit(Placed::new(WALKER, [20.5, 90.0]).set("HP", 7));
    let p = played(&Run::new(scene.clone(), 0));

    for (placed_unit, id) in scene.units.iter().zip((0..).map(placed)) {
        let piece = p.world.piece(id).expect("its piece");

        assert_eq!(piece.code, placed_unit.code);
        assert_eq!(piece.owner, placed_unit.owner);
        assert_eq!(p.world.position(id), Some(placed_unit.pos32()));
    }

    assert_eq!(p.world.player_palace(CROWN), Some(placed(0)));
    assert_eq!(hp(&p.world, placed(3)), Some(7));
}

/// An order makes a hut, paid for, under construction at the place asked.
/// It rises to full health before it pays.
#[test]
fn a_build_order_raises_a_paid_hut() {
    let run = build_at(Run::new(kingdom().gold(1000), 30 * SECOND), 2, 40, 40);
    let mut seen = Vec::new();
    let p = TestGame::get().play(&run, |_, world| {
        if let [hut] = all(world, HUT)[..] {
            seen.push((hp(world, hut), world.treasury(CROWN)));
        }
    });

    clean(&p);

    let hut = only(&p.world, HUT);
    let (first_hp, first_gold) = seen[0];

    assert_eq!(p.world.position(hut), Some(at(40, 40)));
    assert_eq!(p.world.piece(hut).map(|h| h.owner), Some(CROWN));
    assert_eq!(first_hp, Some(FIRST_HP));
    assert_eq!(first_gold, Some(1000 - HUT_COST));
    assert_eq!(hp(&p.world, hut), Some(HUT_HP));

    // Nothing is paid until it stands whole, then income comes.
    let whole = seen
        .iter()
        .position(|(hp, _)| *hp == Some(HUT_HP))
        .expect("whole");

    assert!(seen[..whole].iter().all(|(_, gold)| *gold == first_gold));
    assert!(p.world.treasury(CROWN) >= first_gold.map(|g| g + HUT_INCOME));
}

/// A hut past the treasury is not made, and costs nothing.
#[test]
fn a_build_order_short_of_gold_makes_nothing() {
    let gold = HUT_COST.cast_unsigned() - 1;
    let p = played(&build_at(Run::new(kingdom().gold(gold), SECOND), 2, 40, 40));

    assert_eq!(p.all(HUT), vec![]);
    assert_eq!(p.world.treasury(CROWN), Some(gold.cast_signed()));
}

/// A second hut ordered on the first is pushed off it by the place search.
#[test]
fn a_second_hut_keeps_off_the_first() {
    let run = build_at(Run::new(kingdom().gold(1000), SECOND), 2, 40, 40);
    let p = played(&build_at(run, 3, 40, 40));
    let huts = p.all(HUT);

    assert_eq!(huts.len(), 2);

    let places: Vec<_> = huts.iter().filter_map(|h| p.world.position(*h)).collect();

    assert_eq!(places[0], at(40, 40));
    assert_ne!(places[1], at(40, 40));
}

/// A demolished hut goes, and an eighth of its price comes back.
#[test]
fn a_demolished_hut_goes_with_a_refund() {
    let scene = kingdom()
        .gold(1000)
        .unit(Placed::new(HUT, [40.0, 40.0]).set("HP", 1));
    let hut = placed(1);
    let p = played(&Run::new(scene, SECOND).input(2, Input::Demolish(hut)));

    assert_eq!(p.all(HUT), vec![]);
    assert_eq!(p.world.treasury(CROWN), Some(1000 + refund(HUT_COST)));
}

/// A guild recruits: paid at once, the walker out after its delay.
#[test]
fn a_lodge_recruits_a_paid_walker() {
    let scene = kingdom().gold(1000).unit(Placed::new(LODGE, [40.0, 40.0]));
    let recruit = Input::Recruit(Recruit {
        guild: placed(1),
        code: WALKER,
    });
    let run = Run::new(scene, 3 * SECOND).input(2, recruit);
    let mut before = None;
    let p = TestGame::get().play(&run, |frame, world| {
        if frame == 3 {
            before = Some((all(world, WALKER).len(), world.treasury(CROWN)));
        }
    });

    clean(&p);

    assert_eq!(before, Some((0, Some(1000 - WALKER_COST))));
    assert_eq!(p.all(WALKER).len(), 1);
}

/// A walker goes east, a step at a time, and stops at its goal.
#[test]
fn a_walker_reaches_its_goal() {
    let scene = kingdom().unit(Placed::new(WALKER, [10.0, 20.0]));
    let run = Run::new(scene, 40 * SECOND);
    let start = at(10, 20);
    let mut xs = Vec::new();
    let p = TestGame::get().play(&run, |_, world| {
        xs.push(world.position(placed(1)).expect("known").0);
    });

    clean(&p);

    assert!(xs.windows(2).all(|x| x[0] <= x[1]), "only east");
    assert!(xs.windows(2).any(|x| x[0] < x[1]));
    assert_eq!(
        p.world.position(placed(1)),
        Some((start.0 + WALK_CELLS * CELL, start.1))
    );
}

/// A biter walks to a hut before the keep, and bites it down; while the
/// hut stands the keep is untouched.
#[test]
fn a_biter_eats_a_hut() {
    let scene = kingdom()
        .unit(Placed::new(HUT, [20.0, 20.0]))
        .unit(Placed::new(BITER, [30.0, 20.0]).owner(NEUTRAL));
    let (keep, hut) = (placed(0), placed(1));
    let mut lives = Vec::new();
    let p = TestGame::get().play(&Run::new(scene, 60 * SECOND), |_, world| {
        if let Some(life) = hp(world, hut) {
            lives.push(life);
            assert_eq!(hp(world, keep), Some(KEEP_HP));
        }
    });

    clean(&p);

    assert!(lives.windows(2).any(|h| h[1] < h[0]), "bitten");
    assert_eq!(p.all(HUT), vec![]);
}

/// A biter catches a walker and kills it: it loses hit points, then goes.
#[test]
fn a_biter_kills_a_walker() {
    let scene = kingdom().unit(Placed::new(WALKER, [20.0, 20.0])).unit(
        Placed::new(BITER, [22.0, 20.0])
            .owner(NEUTRAL)
            .set("HP", 400),
    );
    let walker = placed(1);
    let mut lives = Vec::new();
    let p = TestGame::get().play(&Run::new(scene, 90 * SECOND), |_, world| {
        lives.extend(hp(world, walker));
    });

    clean(&p);

    assert!(lives.windows(2).any(|h| h[1] < h[0]), "bitten");
    assert_eq!(p.all(WALKER), vec![]);
    assert_eq!(p.all(BITER).len(), 1);
    assert!(hp(&p.world, placed(2)) < Some(400), "it hit back");
}

/// A walker that outlasts a biter earns its bounty.
#[test]
fn a_dead_biter_pays_its_bounty() {
    let scene = kingdom()
        .gold(1000)
        .unit(Placed::new(WALKER, [20.0, 20.0]).set("HP", 4000))
        .unit(Placed::new(BITER, [22.0, 20.0]).owner(NEUTRAL));
    let p = played(&Run::new(scene, 90 * SECOND));

    assert_eq!(p.all(BITER), vec![]);
    assert_eq!(p.world.treasury(CROWN), Some(1000 + BOUNTY));
}

/// A full treasury wins, and the game stops running frames.
#[test]
fn a_full_treasury_wins() {
    let frames = 5 * SECOND;
    let p = played(&Run::new(kingdom().gold(HOARD_GOAL), frames));

    assert_eq!(p.world.ended(CROWN), Some(End::Won));

    let last = p.digests.last();

    assert!(p.digests.iter().rev().take(2).all(|d| Some(d) == last));

    // Short of the goal, nobody has won.
    let short = played(&Run::new(kingdom().gold(HOARD_GOAL - 1), frames));

    assert_eq!(short.world.ended(CROWN), None);
}

/// A fallen keep loses.
#[test]
fn a_fallen_keep_loses() {
    let scene = Scene::flat(SIDE)
        .unit(Placed::new(KEEP, [MIDDLE, MIDDLE]).set("HP", 10))
        .unit(Placed::new(BITER, [MIDDLE - 10.0, MIDDLE]).owner(NEUTRAL));
    let p = played(&Run::new(scene, 30 * SECOND));

    assert_eq!(p.world.ended(CROWN), Some(End::Lost));
}

/// An order on another player's unit is refused, and changes nothing.
#[test]
fn an_order_on_anothers_unit_is_dropped() {
    let scene = kingdom().players(2).unit(Placed::new(HUT, [40.0, 40.0]));
    let order = Order {
        player: 1,
        input: Input::Demolish(placed(1)),
    };
    let p = played(&Run::new(scene, SECOND).input(2, order));

    assert_eq!(p.dropped.len(), 1);
    assert_eq!(p.dropped[0].order, order);
    assert_eq!(p.all(HUT).len(), 1);
}

/// The game that fights: a walker and a biter, a hut and an order.
fn busy(seed: u32) -> Run {
    let scene = kingdom()
        .gold(1000)
        .unit(Placed::new(WALKER, [40.0, 60.0]))
        .unit(Placed::new(BITER, [42.0, 60.0]).owner(NEUTRAL));

    build_at(Run::new(scene, 40 * SECOND).seed(seed), 2, 30, 30)
}

/// A game played twice is the same frame for frame, under any map order.
#[test]
fn a_game_plays_the_same_twice() {
    regent_core::base::digest::shuffle();

    let first = played(&busy(1));
    let second = played(&busy(1));

    assert_eq!(first.digests, second.digests);
    assert_eq!(first.digests.len(), 40 * SECOND as usize);
}

/// Every frame moves the game on: no two digests in a row are alike while
/// the fight runs.
#[test]
fn the_digest_follows_the_game() {
    let p = played(&busy(1));
    let moved = p.digests.windows(2).filter(|d| d[0] != d[1]).count();

    assert_eq!(moved, p.digests.len() - 1);
}

/// The seed reaches the rules: the same fight ends differently.
#[test]
fn a_seed_changes_the_fight() {
    let outcome = |seed| {
        let p = played(&busy(seed));
        let life = |code: FourCc| all(&p.world, code).first().and_then(|id| hp(&p.world, *id));

        (life(WALKER), life(BITER))
    };
    let outcomes: std::collections::BTreeSet<_> = (1..=8).map(outcome).collect();

    assert!(outcomes.len() > 1, "{outcomes:?}");
}

/// An order changes the game from its frame on, and not before.
#[test]
fn an_order_parts_the_digests_at_its_frame() {
    const ORDER_AT: u32 = 30;

    let without = played(&Run::new(kingdom().gold(1000), 2 * SECOND));
    let with = played(&build_at(
        Run::new(kingdom().gold(1000), 2 * SECOND),
        ORDER_AT,
        40,
        40,
    ));
    let parted = without
        .digests
        .iter()
        .zip(&with.digests)
        .position(|(a, b)| a != b);

    assert_eq!(parted, Some(ORDER_AT as usize - 1));
}

/// What a scene refuses, on this game's own descriptors.
#[test]
fn a_bad_scene_is_refused() {
    let game = TestGame::get();
    let catalogue = game.catalogue();
    let made = |scene: Scene| scene.flat_world(&catalogue, game.program()).err();
    let ghost = FourCc::new(b"ZZZZ");

    assert_eq!(made(Scene::flat(100)), Some(Error::Side(100)));
    assert_eq!(
        made(Scene::flat(SIDE).unit(Placed::new(ghost, [1.0, 1.0]))),
        Some(Error::Unknown(ghost))
    );
    assert_eq!(
        made(Scene::flat(SIDE).unit(Placed::new(KEEP, [128.0, 1.0]))),
        Some(Error::Off(KEEP, [128.0, 1.0]))
    );
    assert_eq!(
        made(Scene::flat(SIDE).unit(Placed::new(HUT, [9.0, 9.0]).owner(1))),
        Some(Error::Owner(HUT, 1))
    );
}

/// A scene's file reads as the builder makes it, on this game's codes.
#[test]
fn a_scene_file_names_the_games_units() {
    let text = "version = 1\nmap = { flat = { side = 128 } }\n\
                [[unit]]\ncode = \"ABJZ\"\nat = [64, 64]\n\
                [[unit]]\ncode = \"ZTW1\"\nat = [60.5, 64]\nattributes = { HP = 9 }\n";
    let read: Scene = toml::from_str(text).expect("a scene");
    let built = kingdom().unit(Placed::new(WALKER, [60.5, 64.0]).set("HP", 9));

    assert_eq!(read, built);
    assert_eq!(hp(&played(&Run::new(read, 0)).world, placed(1)), Some(9));
}

/// No code of the game is any shipped record's, in any chunk, in HD or a
/// Workshop mod.
/// Its names may be: ones this short are common words.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_games_codes_are_no_retail_records() {
    let paths: Vec<_> = [common::hd(), common::workshop()]
        .iter()
        .flat_map(|root| common::archives(root))
        .collect();
    let library = Library::from_files(&paths).expect("the archives");
    let catalogue = library.catalogue().expect("a catalogue");
    let retail: std::collections::BTreeSet<FourCc> = catalogue
        .archives
        .iter()
        .flat_map(|(_, archive)| archive.records().map(|(_, r)| r.id))
        .collect();

    assert!(
        retail.contains(&FourCc::new(b"ABJ1")),
        "the scan reads them"
    );

    for (_, code) in regent_testgame::CODES {
        assert!(!retail.contains(&code), "{code}");
    }

    // Reported, not held: a shared common word is no copy.
    let names: Vec<String> = catalogue
        .tagged(b"DUNT")
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let head = desc::Head::parse(part.find(b"HEAD")?.body).ok()?;

            Some(String::from_utf8_lossy(head.name).to_ascii_lowercase())
        })
        .collect();

    for name in ["keep", "hut", "lodge", "walker", "biter"] {
        println!("{name}: retail {}", names.iter().any(|n| n == name));
    }
}
