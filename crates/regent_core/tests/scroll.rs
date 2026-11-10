//! The top bar's scroll ([`regent_core::view::panel::scroll`]), against
//! HD's text and `Maj.qdd`.

mod common;

use common::read;
use regent_core::data::fourcc::FourCc;
use regent_core::data::qdd;
use regent_core::data::text::Text;
use regent_core::sim::freestyle::{self, Choice, Setup, Victory};
use regent_core::view::panel::scroll::{self, ScrollSubject};

/// A network game's waiting title is `GMTX` 345.
#[test]
fn a_waiting_game_is_titled_in_the_games_words() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);

    assert_eq!(scroll::WAITING, 0x159);
    assert_eq!(
        text.gmtx(scroll::WAITING).as_deref(),
        Some("(Waiting for Other Players)")
    );
}

/// A custom game on `scenario`, won by `victory` at `goal`.
const fn custom(scenario: [u8; 4], victory: Victory, goal: u32) -> Setup {
    let choice = Choice {
        settlement: FourCc(*b"Wizb"),
        scenario: FourCc(scenario),
        monsters: FourCc(*b"NONE"),
        land: FourCc(*b"KxHa"),
        seed: 1,
        side: 64,
    };
    let picked = freestyle::setup_of(&choice, freestyle::GOLD);

    freestyle::with_victory(picked, victory, goal)
}

/// `ui_full.png`'s "Random (Beginner)", a picked force's name, a quest's
/// title.
#[test]
fn the_scroll_names_the_game_as_the_top_bar_does() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let named = |setup: &Setup| scroll::title(ScrollSubject::Custom(setup), &text);

    assert_eq!(
        named(&custom(*b"BEGI", Victory::Eliminate, 0)).as_deref(),
        Some("Random (Beginner)")
    );
    assert_eq!(
        named(&custom(*b"EXPE", Victory::Eliminate, 0)).as_deref(),
        Some("Random (Expert)")
    );
    assert_eq!(
        named(&custom(*b"Ruib", Victory::Eliminate, 0)).as_deref(),
        Some("Ruins of Esterhausen (Advanced) - S")
    );

    let quest = ScrollSubject::Quest {
        title: "The Barren Waste",
        short: "",
    };

    assert_eq!(
        scroll::title(quest, &text).as_deref(),
        Some("The Barren Waste")
    );
}

/// The goals: `Maj.qdd`'s `FRG` blocks, the goal in the gold and days
/// ones, then the map number.
#[test]
fn a_custom_games_goals_are_its_conditions_block() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let qdd = read(&common::hd().join("Quests/Maj.qdd"));
    let quests = qdd::parse(&String::from_utf8_lossy(&qdd)).expect("a whole qdd");
    let goals = |victory, goal| {
        let setup = custom(*b"Ruib", victory, goal);

        scroll::goals(ScrollSubject::Custom(&setup), &quests, &text)
    };

    assert_eq!(
        goals(Victory::Eliminate, 0).as_deref(),
        Some("Eliminate All Enemies.\nMap Number: 1.")
    );
    assert_eq!(
        goals(Victory::Gold, 40_000).as_deref(),
        Some("Acquire 40000 gold\nMap Number: 1.")
    );
    assert_eq!(
        goals(Victory::Survive, 20).as_deref(),
        Some("Last 20 Days without being destroyed\nMap Number: 1.")
    );
}
