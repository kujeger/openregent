//! Message flags and script-placed reward flags: `MessageFlag`,
//! `IsMessageFlagPresent`, `GetFlag`, `PlaceRewardFlag`, and reading a
//! message flag. **Binary** (`world::message`, `world::flag`); no trace
//! calls them, so each runs on the start save's world.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::content::unit::Unit;
use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::Kind;
use regent_core::data::text::Text;
use regent_core::sim::world::{flag::MESSAGE_FLAG, message::MESSAGE};
use regent_core::view::menu;
use regent_core::view::panel::message;

/// "Good news, your Majesty…": `#Message_Found_Gold`, `Quests_3`'s.
const FOUND_GOLD: i32 = 249;

/// `#message_slaves_freed`.
const SLAVES_FREED: i32 = 118;

/// What a message flag is: `ARE1` names `message_flag`, `APM:0` is
/// `Message_Text`; its text is the
/// `AITX` entry its message indexes, so
/// `#Message_Found_Gold` reads as found gold.
#[test]
fn a_message_flags_text_is_its_activity_entry() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let unit = Unit::find(&catalogue, MESSAGE_FLAG)
        .expect("the descriptor")
        .expect("it parses");
    let text = Text::build(&catalogue);

    assert_eq!(unit.desc.head.name, b"message_flag");
    assert_eq!(
        attrib::name(MESSAGE.bank, MESSAGE.index),
        Some("Message_Text")
    );
    assert!(
        text.activity(FOUND_GOLD)
            .is_some_and(|s| s.contains("secret storeroom"))
    );
    assert!(
        text.activity(SLAVES_FREED)
            .is_some_and(|s| s.contains("freed the nearby slaves"))
    );
    assert!(message::text(&text, FOUND_GOLD).is_some_and(|s| s.contains("secret storeroom")));
    assert_eq!(message::text(&text, 0), None);
}

/// The message scroll is `AP77`: its text box in `0x3f0000`, the
/// title "MESSAGE", CLOSE, the zoom and the mini view option where
/// `EventMessageController` addresses them. `AP13`, once taken for it,
/// is a yes and no question.
#[test]
fn the_message_scroll_is_ap77() {
    let library = common::library();
    let catalogue = library.catalogue().expect("catalogue");
    let text = Text::build(&catalogue);
    let layout = menu::layout(&catalogue, &text, message::PANEL)
        .expect("the panel")
        .expect("it parses");
    let control = |id: i32| layout.control(id).unwrap_or_else(|| panic!("{id}"));

    assert_eq!(control(message::id::TEXT).kind, Some(Kind::Textbox));
    assert_eq!(
        control(message::id::TEXT).colours.first(),
        Some(&0x803f_0000_u32.cast_signed())
    );
    assert_eq!(control(message::id::TITLE).text.as_deref(), Some("MESSAGE"));
    assert_eq!(control(message::id::CLOSE).text.as_deref(), Some("CLOSE"));
    assert_eq!(control(message::id::SCROLL).kind, Some(Kind::Scrollbar));
    assert_eq!(control(message::id::ZOOM).kind, Some(Kind::Button));
    assert_eq!(control(message::id::FOLLOW).kind, Some(Kind::Check));

    let question = menu::layout(&catalogue, &text, FourCc(*b"AP13"))
        .expect("AP13")
        .expect("it parses");

    assert!(question.control(5501).is_some() && question.control(message::id::CLOSE).is_none());
}
