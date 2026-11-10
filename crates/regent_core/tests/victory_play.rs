//! Each victory condition the one-player custom-game dialog offers
//! (`freestyle::Victory::OFFERED`), played with no log to its `DeclareVictory`:
//! a new game with no save (`regent_core::sim::mapgen::fresh`), its setup holding the
//! condition and goal, brought to a state meeting it. No cheat ends the
//! game; no answer is unknown.
//!
//! Corpora: `MAJESTY_HD`.

use regent_corpus as common;

use common::read;
use regent_core::data::rgs;
use regent_core::gpl::load::Program;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::freestyle::Picks;
use regent_core::sim::freestyle::{self, Victory};
use regent_core::sim::game::{Game, Input};
use regent_core::sim::mapgen::custom::{self, Art};
use regent_core::sim::mapgen::fresh::{self, Made};
use regent_core::sim::setup;
use regent_core::sim::world::cheat::{Cheat, CheatCode, GOLD};
use regent_core::sim::world::{
    CROWN, World,
    defeat::{End, NO_POINT},
};

/// The quiet day's GPL seed.
const SEED: u32 = 0x0b60_9e2c;

/// The freestyle quest and the chart, under the install.
const QUEST: &str = "QuestsMX/mx_Freestyle.q";
const CHART: &str = "DataMX/mx_constants.rgs";

/// `#VictoryCondition_callback_frequency`: the gold and eliminate checks'
/// interval, game ms.
const CHECK_MS: u32 = 4000;

/// A game day, game ms.
const DAY_MS: u32 = 60_000;

/// Who stands at the start.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Foes {
    /// The start as the generator stands it.
    Kept,
    /// Only the crown's units, and resources: every foe already gone.
    Gone,
}

/// The default picks' new game (Wizb, Ruib, `NONE`, `KxHa`, map 1), its setup
/// holding `victory` and `goal`.
fn founded(program: &Program, victory: Victory, goal: u32, foes: Foes) -> World {
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
    let choice = custom::chosen(&Picks::default(), &map).expect("choice");
    let mut made: Made =
        fresh::generate(&map, &patterns, choice, freestyle::GOLD, &art).expect("made");

    made.setup = freestyle::with_victory(made.setup, victory, goal);

    if foes == Foes::Gone {
        made.units
            .retain(|u| u.owner == CROWN || u.owner == u32::MAX);
    }

    setup::fresh(&catalogue, program, &made.start())
}

/// Frames until the crown's game ends, at most `most`.
fn run_to_end(game: &mut Game<'_>, most: u32) {
    while game.world().ended(CROWN).is_none() && game.frames < most {
        game.frame().expect("frame");
    }
}

/// The game ended as won at the crown's palace, by script, no answer
/// unknown, no script error.
fn assert_won_at_the_palace(game: &Game<'_>) {
    let w = game.world();
    let palace = w
        .shown()
        .iter()
        .find(|s| s.owner == CROWN && s.code.0.starts_with(b"ABJ"))
        .map(|s| s.agent)
        .expect("a palace");

    assert_eq!(w.ended(CROWN), Some(End::Won));
    assert_eq!(w.end_point(CROWN), w.position(palace));
    assert_eq!(game.calls().get("DeclareVictory"), Some(&1));
    assert!(
        game.faults().unknown.is_empty(),
        "{:?}",
        game.faults().unknown
    );
    assert_eq!(game.reports().count, 0);
}

/// "Eliminate all foes": with no lair, building or hero of another team
/// left, the first check (`VictoryCondition_Three`, each 4,000 ms) finds
/// none and `victoryforall` declares the crown's palace's victory.
#[test]
fn with_every_foe_gone_the_first_check_declares_victory() {
    let program = common::sdk_program();
    let mut world = founded(&program, Victory::Eliminate, 0, Foes::Gone);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");
    run_to_end(&mut game, 2 * CHECK_MS / MS_PER_FRAME);

    println!("ended at frame {}", game.frames);
    assert_won_at_the_palace(&game);
    assert!(game.frames * MS_PER_FRAME <= CHECK_MS + MS_PER_FRAME);
}

/// With the foes standing, the same check declares nothing.
#[test]
fn with_foes_standing_no_victory_is_declared() {
    let program = common::sdk_program();
    let mut world = founded(&program, Victory::Eliminate, 0, Foes::Kept);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");
    run_to_end(&mut game, 3 * CHECK_MS / MS_PER_FRAME);

    assert_eq!(game.world().ended(CROWN), None);
    assert!(game.calls().contains_key("ListObjects"));
}

/// "Gather gold": the dialog's goal for 30,000 to start is 60,000
/// (`Victory::goal`); the treasury short of it declares nothing; four
/// gifts of 10,000 pass it, and the next check (`VictoryCondition_Two`)
/// declares the richest palace's victory.
#[test]
fn a_treasury_past_the_goal_declares_victory() {
    let program = common::sdk_program();
    let goal = Victory::Gold.goal(freestyle::GOLD);
    let mut world = founded(&program, Victory::Gold, goal, Foes::Kept);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);
    let short = 2 * CHECK_MS / MS_PER_FRAME;

    assert_eq!(goal, 60_000);

    game.setup(startup::FREESTYLE).expect("setup");
    run_to_end(&mut game, short);

    assert_eq!(game.world().ended(CROWN), None);
    assert!(game.world().treasury(CROWN) < Some(goal.cast_signed()));

    let gifts = goal.cast_signed() / GOLD + 1;
    let gift = Input::Cheat(CheatCode {
        cheat: Cheat::Gold,
        arg: CROWN,
    });

    for _ in 0..gifts {
        game.input(game.frames + 1, gift).expect("on time");
    }

    run_to_end(&mut game, short + 2 * CHECK_MS / MS_PER_FRAME);

    println!("ended at frame {}", game.frames);
    assert_won_at_the_palace(&game);
}

/// "Survive specified time": the dialog's shortest, 10 days, runs
/// `VictoryCondition_One` once at 600,000 ms, which declares the victory.
#[test]
fn lasting_the_days_declares_victory() {
    let program = common::sdk_program();
    let days = Victory::Survive.goals()[0];
    let mut world = founded(&program, Victory::Survive, days, Foes::Gone);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);
    let due = days * DAY_MS / MS_PER_FRAME;

    game.setup(startup::FREESTYLE).expect("setup");
    run_to_end(&mut game, due - 1);

    assert_eq!(game.world().ended(CROWN), None);

    run_to_end(&mut game, due + 1);

    println!("ended at frame {}", game.frames);
    assert_won_at_the_palace(&game);
}

/// The one way a one-player freestyle game is lost: its palace razed
/// with no outpost to take over, `Palace_Death` declares the loss, no
/// point (the gold condition's `multiple_loss` is for other players; no
/// freestyle script times a loss). The default game with no input falls
/// to its raiders.
#[test]
fn a_razed_palace_declares_the_loss() {
    let program = common::sdk_program();
    let mut world = founded(&program, Victory::Eliminate, 0, Foes::Kept);
    let vm = Vm::new(&program, Config::default());
    let mut game = Game::free(vm, SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");
    run_to_end(&mut game, LOST_BY);

    let w = game.world();

    println!("ended at frame {}", game.frames);
    assert_eq!(w.ended(CROWN), Some(End::Lost));
    assert_eq!(w.end_point(CROWN), Some(NO_POINT));
    assert_eq!(game.calls().get("DeclareLoss"), Some(&1));
    assert_eq!(game.calls().get("DeclareVictory"), None);
    assert!(
        game.faults().unknown.is_empty(),
        "{:?}",
        game.faults().unknown
    );
    assert_eq!(game.reports().count, 0);
}

/// Frames within which the default game is lost: `openregent --headless`
/// loses it at 7,618.
const LOST_BY: u32 = 10_000;
