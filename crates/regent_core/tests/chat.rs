//! Game messages: what the world says, when, to whom, and how the chat
//! area shows it (`world::chat::Said`, `view::panel::chat`).
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::Image;
use regent_core::data::payload::{Palette, Rgb};
use regent_core::data::text::Text;
use regent_core::gpl::exec::Literal;
use regent_core::gpl::load::Program;
use regent_core::sim::world::{self, chat::Said};
use regent_core::view::menu::font::Font;
use regent_core::view::panel::chat::{self, FONT, PALETTE};

/// `IBOX`'s entry 32, the nearest to green.
const GREEN: Rgb = Rgb { r: 0, g: 216, b: 0 };

/// Its entry 66, the nearest to the chat's orange.
const YELLOW: Rgb = Rgb {
    r: 241,
    g: 220,
    b: 31,
};

fn sdk() -> Program {
    let images: Vec<Vec<u8>> = common::sdk_bytes();

    Program::load(&images).expect("SDK loads")
}

/// A record's payload by tag and id.
fn payload<'a>(catalogue: &'a Catalogue<'_>, tag: [u8; 4], id: FourCc) -> &'a [u8] {
    catalogue
        .find(&tag, id)
        .and_then(|at| catalogue.record(&tag, at))
        .map_or_else(|| panic!("no {id}"), |r| r.data)
}

/// `fnt4`'s line is 15 px, so the 60 px box holds four rows.
#[test]
fn the_box_holds_four_rows_of_its_font() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let font = Image::parse(payload(&catalogue, *b"IMAG", FONT)).expect("fnt4");
    let line = Font::of_image(&font).line().expect("a line");

    assert_eq!(line, 15);
    assert_eq!(usize::try_from(chat::HEIGHT / line).ok(), Some(chat::ROWS));
}

/// A game message is `IBOX`'s nearest to green; the player's own chat its
/// nearest to orange, a yellow.
#[test]
fn the_box_colours_are_its_palettes_nearest() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let palette = Palette::parse(payload(&catalogue, *b"PALT", PALETTE)).expect("IBOX");
    assert_eq!(chat::colour(&palette, chat::GAME_RGB), GREEN);
    assert_eq!(chat::colour(&palette, chat::CHAT_RGB), YELLOW);
}

/// The idle scroll's period is the scripts' constant.
#[test]
fn the_idle_scroll_is_the_scripts_constant() {
    let program = sdk();

    assert_eq!(
        program.constant(chat::SCROLL_CONSTANT),
        Some(Literal::Int(i32::try_from(chat::SCROLL_MS).unwrap_or(0)))
    );
}

/// The rogues' guild's press says `GMTX` 194 of the player's share and
/// the guild's: the market probe's 103 and 55.
#[test]
fn the_extortion_says_both_shares() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let said = Said {
        at: 0,
        to: None,
        line: world::chat::Line::Extorted {
            amount: 103,
            kept: 55,
        },
        from: None,
    };

    assert_eq!(
        chat::text(&catalogue, &text, &said).as_deref(),
        Some("Extorted 103 gold, the guild kept 55 gold.")
    );
}

/// A press on a low hero's name box says `GMTX` 192 of the level
/// `#HeroNameEditingLevel` asks.
#[test]
fn a_low_heros_name_box_says_the_level() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let said = Said {
        at: 0,
        to: None,
        line: world::chat::Line::Counted {
            text: regent_core::sim::world::naming::RENAME_REFUSED,
            n: 1,
        },
        from: None,
    };

    assert_eq!(
        chat::text(&catalogue, &text, &said).as_deref(),
        Some("You can't rename heroes until they reach level 1.")
    );
}
