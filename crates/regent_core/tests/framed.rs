//! `World::framed` against `World::shown`: a new game with no save
//! (`regent_core::sim::mapgen::fresh`), run with no input, counts each
//! frame the pieces shown in an action's frame as `shown` does.
//!
//! Corpora: `MAJESTY_HD`.

use regent_corpus as common;

use common::read;
use regent_core::data::rgs;
use regent_core::gpl::startup;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::freestyle::{self, Picks};
use regent_core::sim::game::Game;
use regent_core::sim::mapgen::custom::{self, Art};
use regent_core::sim::mapgen::fresh;
use regent_core::sim::movement::action::Look;
use regent_core::sim::setup;

/// The session word (`tests/victory_play.rs`).
const SEED: u32 = 0x0b60_9e2c;

/// Frames the game runs: 200 game seconds.
const FRAMES: u32 = 4000;

/// The freestyle quest and the chart, under the install.
const QUEST: &str = "QuestsMX/mx_Freestyle.q";
const CHART: &str = "DataMX/mx_constants.rgs";

/// The default picks' new game: each frame, `framed` is `shown`'s count
/// of action frames, and some frames show one.
#[test]
fn a_new_game_counts_framed_pieces_as_shown_does() {
    let hd = common::hd();
    let find = |name: &str| regent_core::content::on_disk(&hd.join(name)).expect("on disk");
    let (quest, chart) = (read(&find(QUEST)), read(&find(CHART)));
    let map = rgs::File::parse(&quest)
        .and_then(|f| f.map())
        .expect("quest");
    let patterns = rgs::File::parse(&chart)
        .and_then(|f| f.chart())
        .expect("chart");
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let choice = custom::chosen(&Picks::default(), &map).expect("choice");
    let made = fresh::generate(
        &map,
        &patterns,
        choice,
        freestyle::GOLD,
        &Art::of(&catalogue),
    )
    .expect("made");
    let program = common::sdk_program();
    let mut world = setup::fresh(&catalogue, &program, &made.start());
    let mut game = Game::free(Vm::new(&program, Config::default()), SEED, &mut world);

    game.setup(startup::FREESTYLE).expect("setup");

    let mut total = 0;

    for _ in 0..FRAMES {
        game.frame().expect("frame");

        let world = game.world();
        let shown = world
            .shown()
            .iter()
            .filter(|s| matches!(s.look, Some(Look::Frame { .. })))
            .count();

        assert_eq!(world.framed(), shown, "frame {}", game.frames);
        total += shown;
    }

    assert!(total > 0, "no action frame shown");
}
