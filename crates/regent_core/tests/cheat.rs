//! Cheat codes (`world::cheat`) against HD's text, saves and a new game.
//!
//! Corpora: `MAJESTY_HD`.

mod common;

use regent_core::data::catalogue::Library;
use regent_core::data::text::Text;
use regent_core::sim::world::cheat::{self, Cheat, GOLD, LEVELS, PHRASES};

/// The phrases are HD's own strings; the chat compares the line in
/// capitals, whole.
#[test]
fn the_phrases_are_hds_text() {
    let library = common::library();
    let catalogue = library.catalogue().expect("a catalogue");
    let text = Text::build(&catalogue);

    for p in PHRASES {
        assert_eq!(text.at(cheat::TABLE, p.text).as_deref(), Some(p.words));
    }

    let gold = cheat::recognise(&text, "fill this bag").map(|p| p.cheat);

    assert_eq!(gold, Some(Cheat::Gold));
    // The file's reply ends in a carriage return.
    assert_eq!(
        text.at(cheat::TABLE, 54).as_deref().map(str::trim_end),
        Some("I'm rich! 10,000 gold for me.")
    );
    assert!(cheat::recognise(&text, "fill this bag ").is_none());
    assert!(cheat::recognise(&text, "PING").is_none());
}

/// The decode has a case for each code to 0x14 but 7, the default.
#[test]
fn the_decode_has_a_case_per_cheat() {
    for code in (0..=0x14).filter(|c| *c != 7) {
        assert!(Cheat::of(code).is_some(), "{code}");
    }

    assert_eq!((GOLD, LEVELS), (0x2710, 5));
}

/// The base game's text, and the expansion's, loaded after it.
const BASE_TEXT: &str = "Data/textdata.cam";
const EXPANSION_TEXT: &str = "DataMX/mx_textdata.cam";

/// The install's `files`, in order.
fn library_of(files: &[&str]) -> Library {
    let paths: Vec<_> = files.iter().map(|f| common::hd().join(f)).collect();

    Library::from_files(&paths).expect("the archives")
}

/// Gmtx string past the table is the engine's empty string, so in
/// a base quest (`Data/textdata.cam`'s 388 strings) an empty line is
/// `PLANET FARGO` (440) and its reply (441) empty; typed, the phrase is
/// no cheat. The expansion's 442 hold both.
#[test]
fn an_empty_line_is_planet_fargo_in_a_base_quest() {
    let (base, expansion) = (
        library_of(&[BASE_TEXT]),
        library_of(&[BASE_TEXT, EXPANSION_TEXT]),
    );
    let (base, expansion) = (
        base.catalogue().expect("a catalogue"),
        expansion.catalogue().expect("a catalogue"),
    );
    let (base, expansion) = (Text::build(&base), Text::build(&expansion));
    let count = |t: &Text<'_>| t.table(cheat::TABLE).map(|s| s.iter().count());
    let cheat_of = |t: &Text<'_>, line: &str| cheat::recognise(t, line).map(|p| p.cheat);

    assert_eq!((count(&base), count(&expansion)), (Some(388), Some(442)));

    assert_eq!(cheat_of(&base, ""), Some(Cheat::Fargo));
    assert_eq!(cheat_of(&base, "planet fargo"), None);
    assert_eq!(cheat_of(&base, "fill this bag"), Some(Cheat::Gold));

    let fargo = cheat::recognise(&base, "").expect("a phrase");

    assert_eq!(cheat::reply(&base, &fargo), "");

    assert_eq!(cheat_of(&expansion, ""), None);
    assert_eq!(cheat_of(&expansion, "planet fargo"), Some(Cheat::Fargo));
    assert_ne!(cheat::reply(&expansion, &fargo).len(), 0);
}
