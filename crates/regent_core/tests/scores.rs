//! The high scores against HD's files and the saves: the `AP64` screen,
//! its texts, and the champion a save keeps (`regent_core::view::menu::scores`,
//! `regent_core::sim::world::score`).
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::content::dataset::Datasets;
use regent_core::data::catalogue::{Catalogue, Library};
use regent_core::data::smnu::{self, Kind, Panel};
use regent_core::data::text::Text;
use regent_core::view::menu;
use regent_core::view::menu::scores::{self, Category, GameKind, id};

fn library(sets: Datasets) -> &'static Library {
    common::dataset(sets)
}

/// `AP64` in both datasets: the radio buttons name the games and
/// categories as high scores screen reads them, the buttons ERASE SCORES and
/// MAIN MENU, the list, the definition and the title.
#[test]
fn the_high_scores_panel_holds_the_controls_the_screen_reads() {
    let labels = [
        (Category::Gold.control(), "MOST GOLD"),
        (Category::City.control(), "LARGEST CITY"),
        (Category::Level.control(), "HIGHEST LEVEL"),
        (Category::Time.control(), "BEST TIME"),
        (Category::Heroes.control(), "MOST HEROES"),
        (GameKind::Multiplayer.control(), "MULTIPLAYER"),
        (GameKind::Freestyle.control(), "FREESTYLE"),
        (GameKind::Quests.control(), "EPIC QUESTS"),
        (id::ERASE, "ERASE SCORES"),
        (id::MAIN_MENU, "MAIN MENU"),
        (id::TITLE, "High Scores"),
    ];

    for sets in [Datasets::Base, Datasets::Expansion] {
        let library = library(sets);
        let catalogue = library.catalogue().expect("a catalogue");
        let text = Text::build(&catalogue);
        let layout = menu::layout(&catalogue, &text, scores::PANEL)
            .expect("the panel ships")
            .expect("it parses");
        let label = |id| layout.control(id).and_then(|c| c.text.as_deref());

        for (control, want) in labels {
            assert_eq!(label(control), Some(want), "{sets:?} {control}");
        }

        let kind = |id| layout.control(id).and_then(|c| c.kind);

        assert!(
            Category::ALL
                .map(Category::control)
                .into_iter()
                .chain(GameKind::ALL.map(GameKind::control))
                .all(|c| kind(c) == Some(Kind::Radio))
        );
        assert_eq!(kind(id::LIST), Some(Kind::Listbox));
        assert_eq!(kind(id::BAR), Some(Kind::Scrollbar));
        assert_eq!(kind(id::DEFINITION), Some(Kind::Textbox));
        assert_eq!(
            layout.control(id::MAIN_MENU).and_then(|c| c.hotkey),
            Some(regent_core::view::menu::hotkey::ESCAPE)
        );
    }
}

/// The list's rows are [`scores::ROW`] high: its `SMNU` key 28.
#[test]
fn the_list_rows_are_its_key_28_high() {
    let library = library(Datasets::Base);
    let catalogue = library.catalogue().expect("a catalogue");
    let record = panel_record(&catalogue);
    let panel = Panel::parse(record).expect("parses");
    let list = panel.control(id::LIST).expect("the list");

    assert_eq!(list.word(smnu::ROW_HEIGHT), Some(scores::ROW));
}

fn panel_record<'a>(catalogue: &Catalogue<'a>) -> &'a [u8] {
    let at = catalogue.find(b"SMNU", scores::PANEL).expect("AP64 ships");

    catalogue.record(b"SMNU", at).expect("its record").data
}

/// Each category's row takes the arguments [`scores::row`] gives it, in
/// that order, and its definition is the monarchs' record.
#[test]
fn each_categorys_texts_take_what_the_screen_passes() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let conversions = |s: &str| -> String {
        s.match_indices('%')
            .filter_map(|(i, _)| s[i + 1..].chars().next())
            .collect()
    };
    let want = [
        (Category::Gold, "sds"),
        (Category::City, "sds"),
        (Category::Level, "sssds"),
        (Category::Time, "sss"),
        (Category::Heroes, "sds"),
    ];

    for (category, signature) in want {
        let row = text.gmtx(category.row_text()).expect("the row's format");
        let definition = text
            .gmtx(category.definition_text())
            .expect("the definition");

        assert_eq!(conversions(&row), signature, "{category:?}: {row}");
        assert!(
            definition.starts_with("These monarchs hold the record"),
            "{category:?}: {definition}"
        );
    }
}

/// A row as the screen sets it, markup kept for the font.
#[test]
fn a_row_reads_as_the_game_writes_it() {
    let library = library(Datasets::Expansion);
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);
    let gmtx = |i| text.gmtx(i).map(std::borrow::Cow::into_owned);
    let entry = scores::Entry {
        monarch: "Kåre".to_owned(),
        game: "The Barren Waste".to_owned(),
        value: 4 * 60_000 + 59_999,
        hero: Some(scores::Hero {
            name: "Sir Lucas Silveredge".to_owned(),
            class: "Warrior".to_owned(),
        }),
    };
    let plain = |category| {
        regent_core::data::text::plain(&scores::row(category, &entry, &gmtx).expect("a row"))
            .into_owned()
    };

    assert_eq!(
        plain(Category::Time),
        "Kåre completed The Barren Waste in 4 Days."
    );

    let level = scores::Entry {
        value: 9,
        ..entry.clone()
    };
    let row = scores::row(Category::Level, &level, &gmtx).expect("a row");

    assert_eq!(
        regent_core::data::text::plain(&row),
        "Kåre recruited the Warrior Sir Lucas Silveredge, who reached level 9 while playing The Barren Waste."
    );
}
