//! A game's end and the dialog's victory conditions, against HD's text:
//! [`regent_core::sim::world::defeat::End`],
//! [`regent_core::sim::freestyle::Victory`].

mod common;

use regent_core::content::dataset::{Bytecode, Datasets, Install};
use regent_core::data::catalogue::Library;
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::Kind;
use regent_core::data::text::Text;
use regent_core::data::text::plain;
use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::freestyle::{self, Victory};
use regent_core::sim::world::{
    defeat::{End, FREESTYLE_NAME, Summary},
    ledger,
    sound::ADVISOR,
};
use regent_core::view::menu::end::{self, Button, Page, Panel, id};
use regent_core::view::menu::{self, message};

/// The end panel's strings, the dialog's names and the summary's rows, in
/// HD's text.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_ends_strings_are_the_games() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let gmtx = |i: usize| text.gmtx(i).map(|t| plain(&t).trim().to_owned());
    let heading = |end: End| {
        let (table, index) = end.heading();

        text.at(table, index).map(std::borrow::Cow::into_owned)
    };
    let verdict = |end: End| {
        let (table, key) = end.verdict();

        text.keyed(table, key).map(std::borrow::Cow::into_owned)
    };

    assert_eq!(heading(End::Won).as_deref(), Some("YOU HAVE WON!"));
    assert_eq!(heading(End::Lost).as_deref(), Some("YOU LOST!"));
    assert_eq!(
        verdict(End::Won).as_deref(),
        Some("Congratulations!  You have won the game.")
    );
    assert_eq!(
        verdict(End::Lost).as_deref(),
        Some("You have lost this game!")
    );
    assert_eq!(gmtx(FREESTYLE_NAME).as_deref(), Some("Freestyle Game"));

    let names: Vec<Option<String>> = Victory::ALL.iter().map(|v| gmtx(v.text())).collect();

    assert_eq!(
        names,
        [
            "Eliminate all foes",
            "Gather gold",
            "Survive specified time",
            "Last Palace standing"
        ]
        .map(|n| Some(n.to_owned()))
    );

    let summary = Summary {
        gold: Some(1),
        standing: 2,
        heroes: 3,
        level: 4,
        vice: 5,
    };
    let rows: Vec<Option<String>> = summary
        .rows()
        .iter()
        .map(|(row, n)| gmtx(*row).map(|f| f.replacen("%d", &n.to_string(), 1)))
        .collect();

    assert_eq!(
        rows,
        [
            "Gold Balance: 1",
            "Standing: 2",
            "Alive: 3",
            "Average Level: 4",
            "Vice: 5"
        ]
        .map(|n| Some(n.to_owned()))
    );
    assert_eq!(FourCc(*b"GVS5"), ADVISOR);
}

/// The goal the dialog shows: the second; a gold goal at least 30,000 past
/// the start, the first of the three that is, else that sum.
#[test]
fn a_picked_conditions_goal_is_the_dialogs() {
    assert_eq!(Victory::Survive.goal(30_000), 20);
    assert_eq!(Victory::Gold.goal(0), 30_000);
    assert_eq!(Victory::Gold.goal(5_000), 40_000);
    assert_eq!(Victory::Gold.goal(30_000), 60_000);
    assert_eq!(Victory::Eliminate.goal(30_000), 0);
    assert_eq!(Victory::OFFERED.map(Victory::word), [0, 1, 2]);
}

/// A goal kept as the dialog refills: a gold goal under the lead lifted,
/// 0 made the first; a day goal as it is.
#[test]
fn a_kept_goal_is_lifted_past_the_lead() {
    assert_eq!(Victory::Gold.kept(20_000, 10_000), 40_000);
    assert_eq!(Victory::Gold.kept(0, 0), 30_000);
    assert_eq!(Victory::Gold.kept(55_000, 10_000), 55_000);
    assert_eq!(Victory::Gold.kept(40_000, 30_000), 60_000);
    assert_eq!(Victory::Survive.kept(45, 30_000), 45);
    assert!(Victory::Gold.greyed(30_000, 10_000));
    assert!(!Victory::Gold.greyed(40_000, 10_000));
    assert!(!Victory::Survive.greyed(10, 30_000));
}

/// The end panels' buttons, hot keys, texts and statistics controls, in
/// both datasets' `SMNU`s, and the fade's delay in the scripts
/// (`regent_core::view::menu::end`).
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_end_panels_hold_their_controls() {
    let hot = |b: Button| match b {
        Button::MainMenu => b'M',
        Button::Restart => b'R',
        Button::Statistics => b'S',
        Button::Continue => b'C',
    };

    for sets in [Datasets::Base, Datasets::Expansion] {
        let files = Install::new(&common::hd(), Bytecode::Install)
            .files(sets)
            .map(|f| f.archives)
            .expect("files");
        let library = Library::from_files(&files).expect("archives");
        let catalogue = library.catalogue().expect("catalogue");
        let text = Text::build(&catalogue);
        let layout = |id| {
            menu::layout(&catalogue, &text, id)
                .expect("the panel ships")
                .expect("it parses")
        };
        let panels = [End::Won, End::Lost]
            .into_iter()
            .flat_map(|end| [Page::Verdict, Page::Statistics].map(|page| Panel { end, page }));

        for panel in panels {
            let shown = layout(panel.record());
            let mut buttons: Vec<i32> = shown
                .controls
                .iter()
                .filter(|c| c.kind == Some(Kind::Button) && c.text.is_some())
                .filter_map(|c| c.id)
                .collect();
            let mut wanted: Vec<i32> = panel
                .buttons(end::Play::Alone)
                .iter()
                .map(|b| b.id())
                .collect();

            buttons.sort_unstable();
            wanted.sort_unstable();
            assert_eq!(buttons, wanted, "{}", panel.record());

            for &b in panel.buttons(end::Play::Alone) {
                let key = shown.control(b.id()).and_then(|c| c.hotkey);

                assert_eq!(key, Some(i32::from(hot(b))), "{} {b:?}", panel.record());
            }

            let text_box = panel.text().and_then(|t| shown.control(t));

            assert_eq!(
                text_box.map(|c| c.kind),
                panel.text().map(|_| Some(Kind::Textbox))
            );

            for id in [id::POG, id::TITLE, id::MAP_LABEL, id::MAP_NUMBER] {
                assert!(shown.control(id).is_some(), "{} {id}", panel.record());
            }

            let values: Vec<Option<&str>> = (0..ledger::LINES)
                .filter_map(end::stat_control)
                .map(|id| shown.control(id).and_then(|c| c.text.as_deref()))
                .collect();

            if panel.page == Page::Statistics {
                assert!(values.iter().all(|v| *v == Some("VALUE")), "{values:?}");
            }
        }

        let game_over = layout(end::GAME_OVER);

        for id in [message::TITLE, message::TEXT, message::OK] {
            assert!(game_over.control(id).is_some(), "AP11 {id}");
        }

        let gmtx = |i| text.gmtx(i).map(|t| t.trim().to_owned());

        assert_eq!(gmtx(end::GAME_OVER_TITLE).as_deref(), Some("Game Over"));
        assert_eq!(
            gmtx(end::VICTORIOUS).as_deref(),
            Some("You are victorious.")
        );
    }

    let images: Vec<Vec<u8>> = common::sdk_bytes();
    let program = Program::load(&images).expect("SDK loads");

    assert_eq!(
        program.constant(end::FADE_DELAY),
        Some(Literal::Int(i32::try_from(end::FADE_DELAY_MS).expect("ms")))
    );
}

/// The dialog's Starting Gold, its four in order: the GPL
/// constants freestyle globals read reads into globals.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialogs_starting_gold_is_its_four_gpl_constants() {
    let images: Vec<Vec<u8>> = common::sdk_bytes();
    let program = Program::load(&images).expect("SDK loads");
    let golds = freestyle::GOLD_CONSTANTS.map(|name| program.constant(name));

    assert_eq!(
        golds,
        freestyle::GOLDS.map(|g| Some(Literal::Int(g.cast_signed())))
    );
    assert_eq!(freestyle::GOLDS, [30_000, 20_000, 15_000, 10_000]);
    assert_eq!(freestyle::GOLDS[0], freestyle::GOLD);
}

/// The dialog's own words, in HD's text: its "None", a gold line, and
/// each goal list's last entry.
#[test]
#[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
fn the_dialogs_own_entries_are_named_by_gmtx() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let gmtx = |i: usize| text.gmtx(i).map(|t| t.trim().to_owned());
    let specified = |v: Victory| v.specified().and_then(gmtx);

    assert_eq!(gmtx(freestyle::NONE_TEXT).as_deref(), Some("None"));
    assert_eq!(gmtx(freestyle::GOLD_TEXT).as_deref(), Some("%d Gold"));
    assert_eq!(
        specified(Victory::Gold).as_deref(),
        Some("(Player specified amount)")
    );
    assert_eq!(
        specified(Victory::Survive).as_deref(),
        Some("(Player specified time)")
    );
    assert_eq!(Victory::Eliminate.specified(), None);
}
