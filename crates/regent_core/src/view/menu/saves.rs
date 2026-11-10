//! The save and load dialogs: Save Menu, `AP49`, and Single Player Load
//! Menu, `AP27`, from OPTIONS' SAVE GAME and LOAD GAME, the main menu's
//! Load Game and the lobby's LOAD GAME. **Binary**: `SaveGameController`
//! and its subclass `LoadGameController`.
//!
//! ```text
//!      (open)        SAVE: the name box "(quest title) N Days",
//!                    and the game's save name set to it; CURRENT GAME
//!                    DESCRIPTION the game's; the list filled
//!  the list          each *.GMP in the save folder: a game of one's
//!                    outside a network game, a network game's of as
//!                    many players in one; the row named as the
//!                    game's save name lit, else the first where there
//!                    is no name box (LOAD), else none (SAVE)
//!  a row lit         its name into the name box, its description shown
//!                    (the list's handler, message 0x1b)
//!  SAVE 9002         the game saved as the name box says, over a file of
//!                    that name, no question asked; "Game Saved." in the
//!                    chat; closed
//!  LOAD 9000         the lit row's game loaded, if any
//!  DELETE 9003       AP11 asks "Delete Selected Game"; YES removes the
//!                    lit row's file, the list filled again
//!  CANCEL 9001       back to OPTIONS (AP36)
//! ```

use std::cmp::Ordering;

use crate::data::fourcc::FourCc;
use crate::data::smnu::Rect;
use crate::data::text::{Arg, Ref, printf, spans};
use crate::sim::world::offer::UNAVAILABLE_MARK;
use crate::view::panel::day::Day;

/// The two panels.
pub const SAVE_PAGE: FourCc = FourCc(*b"AP49");
pub const LOAD_PAGE: FourCc = FourCc(*b"AP27");

/// LOAD GAME, hot key Enter, `AP27`'s alone; CANCEL, Escape; SAVE GAME,
/// Enter, `AP49`'s alone; DELETE GAME.
pub const LOAD: i32 = 9000;
pub const CANCEL: i32 = 9001;
pub const SAVE: i32 = 9002;
pub const DELETE: i32 = 9003;

/// The lit row's description (`0x232c`): SELECTED GAME DESCRIPTION's box
/// on `AP49`, GAME DESCRIPTION's on `AP27`.
pub const DESCRIPTION: i32 = 9004;

/// CURRENT GAME DESCRIPTION's box (`0x232d`), `AP49`'s alone.
pub const CURRENT: i32 = 9005;

/// The name typed (`0x232e`), `AP49`'s alone.
pub const NAME: i32 = 9006;

/// The saves' list (`0x238c`) and its scroll bar
/// ([`super::ids::SCROLL_BAR`]).
pub const LIST: i32 = 9100;

/// The title and the pog: the game's quest's, as its title bar's
/// ([`icon`](super::icon) of `CURR`, method); a freestyle game's `FRE1`'s.
pub use super::ids::{POG, TITLE};

/// A row's height. **Game**: `ui_load_game.png`'s rows, 20 pixels
/// apart, as `MX20`'s list's.
pub const ROW_HEIGHT: i32 = 20;

/// The rows a list of rectangle `r` shows: each that starts in it, the
/// last cut. **Game**: both captures show 13, `AP27`'s list 259 high.
#[must_use]
pub fn rows(r: Rect) -> usize {
    usize::try_from((r.height + ROW_HEIGHT - 1) / ROW_HEIGHT).unwrap_or_default()
}

/// The longest name: method cuts the offered one there, and
/// method sets the name box's length to it.
pub const NAME_CHARS: usize = 100;

/// The offered name: the quest's title, then the day (`"(%s) %s"`).
pub const NAME_FORMAT: &str = "(%s) %s";

/// A save's date: day, month's name, year (`"%d %s %d"`, by
/// save date text).
pub const DATE_FORMAT: &str = "%d %s %d";

/// The months' names, January first.
pub const MONTHS: [u8; 4] = *b"MNTH";

/// A game of one's description: title, "Duration: %s", "Save date: %s",
/// and a last line, empty.
pub const DESCRIPTION_TEXT: Ref = Ref::gmtx(82);

/// A network game's: "Duration: %s", "Save date: %s", then
/// [`PARTICIPANTS_TEXT`] and the players' names.
pub const LAN_DESCRIPTION_TEXT: Ref = Ref::gmtx(349);

/// "Participants:" and a line break, the names on the next line,
/// [`NAME_GAP`] between them.
pub const PARTICIPANTS_TEXT: Ref = Ref::gmtx(83);
pub const NAME_GAP: &str = ", ";

/// A freestyle game's title, "Freestyle Game": quest title of
/// `FRE1`, in a save's description and CURRENT GAME DESCRIPTION (method,
/// `SaveGameController`). **Game**: `ui_save_game.png` describes the saves so.
pub const FREESTYLE_TEXT: Ref = Ref::gmtx(80);

/// "Game Saved.", posted in the chat.
pub const SAVED_TEXT: Ref = Ref::gmtx(311);

/// DELETE's question: `AP11`'s title and text.
pub const DELETE_TITLE: Ref = Ref::gmtx(346);
pub const DELETE_TEXT: Ref = Ref::gmtx(347);

/// Which of the two dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Load,
    Save,
}

impl Mode {
    /// Its panel.
    #[must_use]
    pub const fn panel(self) -> FourCc {
        match self {
            Mode::Load => LOAD_PAGE,
            Mode::Save => SAVE_PAGE,
        }
    }

    /// The button Enter and a row's double click press (save confirm button,
    /// set by load menu init and save menu init). **Inferred**: the
    /// list's message `0x45` is the double click.
    #[must_use]
    pub const fn confirm(self) -> i32 {
        match self {
            Mode::Load => LOAD,
            Mode::Save => SAVE,
        }
    }
}

/// The name offered for a save of `game`, `days` in: "(Freestyle Game)
/// 4 Days", at most [`NAME_CHARS`] long.
#[must_use]
pub fn offered(game: &str, days: &str) -> String {
    printf(NAME_FORMAT, &[game.into(), days.into()])
        .chars()
        .take(NAME_CHARS)
        .collect()
}

/// The list's order of two saves by name: their files' names, the
/// extension's full stop after them, compared as upper case.
/// **Inferred**: the list is in the order the directory search finds
/// the files, NTFS's; `ui_load_game.png` puts "lands 2" before "lands"
/// and "0 Days" before "`0 Days_30Kgold`", as the full stop sorts.
#[must_use]
pub fn order(a: &str, b: &str) -> Ordering {
    let key = |name: &str| {
        name.chars()
            .flat_map(char::to_uppercase)
            .chain(std::iter::once('.'))
            .collect::<String>()
    };

    key(a).cmp(&key(b))
}

/// The list's order here of two saves, each its save time, seconds,
/// and name: newest first, ties as [`order`]. A departure: the
/// original's is [`order`] alone (`docs/openregent/enhancements.md`).
#[must_use]
pub fn newest((a_saved, a): (u64, &str), (b_saved, b): (u64, &str)) -> Ordering {
    b_saved.cmp(&a_saved).then_with(|| order(a, b))
}

/// A save's time as its row shows it: year, month from 1, day, hour,
/// minute.
pub type Stamp = (i64, usize, i64, i64, i64);

/// A row's save time, right of its name: "2026-10-08 14:30". Not the
/// original's, whose rows show the name alone: numbers, the narrowest
/// date that reads in any language, and the time, saves of one day
/// apart.
#[must_use]
pub fn stamp((year, month, day, hour, minute): Stamp) -> String {
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// A row's text right of its name, each right-aligned in a column of
/// its own: its [`stamp`], and the days its game had run at `clock`,
/// ms, as the description's "Duration: 4 Days" (`gmtx` the strings).
#[must_use]
pub fn side(saved: Stamp, clock: u32, gmtx: &dyn Fn(usize) -> Option<String>) -> [String; 2] {
    let days = Day::of(clock).text_by(gmtx).unwrap_or_default();

    [stamp(saved), days]
}

/// Whose game a save or the game played is: one player's, or a network
/// game's of so many players.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Players {
    #[default]
    Alone,
    Lan(usize),
}

/// Whether the list holds a save of `save`'s players, over a game of
/// `over`'s: a game of one's outside a network game (session,
/// header not 2); a network game's in a network game
/// whose players are as many as its seats taken.
#[must_use]
pub fn listed(save: Players, over: Players) -> bool {
    save == over
}

/// The row lit as the list fills: the one named `current`, the
/// game's save name, compared exactly; else the first on LOAD, none on
/// SAVE.
#[must_use]
pub fn lit(names: &[&str], current: Option<&str>, mode: Mode) -> Option<usize> {
    let own = current.and_then(|c| names.iter().position(|n| *n == c));

    match mode {
        Mode::Load => own.or((!names.is_empty()).then_some(0)),
        Mode::Save => own,
    }
}

/// A save's date as [`DATE_FORMAT`] sets it: "8 October 2026"; `month`
/// from 1, its name by `months`.
#[must_use]
pub fn date(
    (year, month, day): (i64, usize, i64),
    months: &dyn Fn(usize) -> Option<String>,
) -> String {
    let name = month.checked_sub(1).and_then(months).unwrap_or_default();

    printf(
        DATE_FORMAT,
        &[Arg::Int(day), (&name).into(), Arg::Int(year)],
    )
}

/// What a save's description says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Described<'a> {
    /// A game of one: its title.
    Alone(&'a str),
    /// A network game: its players' names.
    Lan(&'a [String]),
}

/// A save's description, `gmtx` the strings: its title or players, its
/// `days` and `date`, a line each; the last, empty line dropped.
#[must_use]
pub fn described(
    of: Described<'_>,
    (days, date): (&str, &str),
    gmtx: &dyn Fn(usize) -> Option<String>,
) -> String {
    let text = match of {
        Described::Alone(title) => gmtx(DESCRIPTION_TEXT.index)
            .map(|f| printf(&f, &[title.into(), days.into(), date.into(), "".into()])),
        Described::Lan(names) => gmtx(LAN_DESCRIPTION_TEXT.index).map(|f| {
            let who = format!(
                "{}{}",
                gmtx(PARTICIPANTS_TEXT.index).unwrap_or_default(),
                names.join(NAME_GAP)
            );

            printf(&f, &[days.into(), date.into(), (&who).into()])
        }),
    };

    text.unwrap_or_default().trim_end().to_owned()
}

/// The mods line's label, after the original's description lines: a
/// placeholder, neither `GMTX` nor HD's `InterfaceStrings.xml` having a
/// "mods" text (HD's description names none).
pub const MODS_LABEL: &str = "Mods: ";

/// A cut mods line's end, how many more: a placeholder.
pub const MORE_FORMAT: &str = " +%d";

/// Whether a mod a save needs is among those found now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    Here,
    /// The load is refused for it.
    Missing,
}

/// The line naming a save's `mods`, in load order: "Mods: A, B"; none
/// with no mods. A missing one in [`UNAVAILABLE_MARK`]'s red, the build
/// menu's for an unavailable building, then `normal`, the box's colour.
/// Cut to the most names whose line, markup left out, `fits`, ending
/// with how many more: "Mods: A +2", red if one of those is missing.
#[must_use]
pub fn mods_line(
    mods: &[(&str, Found)],
    normal: [u8; 3],
    fits: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let [r, g, b] = normal;
    let back = format!("\u{1}{r:02X}{g:02X}{b:02X}");
    let marked = |text: &str, found: Found| match found {
        Found::Here => text.to_owned(),
        Found::Missing => format!("{UNAVAILABLE_MARK}{text}{back}"),
    };

    // The first `shown` names, then how many more, missing if any is.
    let line = |shown: usize| {
        let (named, rest) = mods.split_at(shown);
        let shown_names: Vec<String> = named.iter().map(|&(n, f)| marked(n, f)).collect();
        let more = (!rest.is_empty()).then(|| {
            let count = i64::try_from(rest.len()).unwrap_or_default();
            let lost = rest.iter().find(|&&(_, f)| f == Found::Missing);

            marked(
                &printf(MORE_FORMAT, &[Arg::Int(count)]),
                lost.map_or(Found::Here, |&(_, f)| f),
            )
        });

        format!(
            "{MODS_LABEL}{}{}",
            shown_names.join(NAME_GAP),
            more.unwrap_or_default()
        )
    };
    let drawn = |text: &str| spans(text).map(|s| s.text).collect::<String>();
    let shown = (0..=mods.len())
        .rev()
        .find(|&n| fits(&drawn(&line(n))))
        .unwrap_or_default();

    (!mods.is_empty()).then(|| line(shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name where it fits, a missing one red then white again;
    /// cut, the names that fit and how many more, red where one of
    /// those is missing; no line with no mods.
    #[test]
    fn the_mods_line_marks_the_missing_and_cuts_to_fit() {
        use Found::{Here, Missing};

        let mods = [("Ann", Here), ("Bob", Missing), ("Cy", Here)];
        let white = [0xff; 3];
        let wide = |_: &str| true;
        let short = |line: &str| line.len() <= "Mods: Ann +2".len();

        assert_eq!(
            mods_line(&mods, white, &wide).as_deref(),
            Some("Mods: Ann, \u{1}FF0000Bob\u{1}FFFFFF, Cy")
        );
        assert_eq!(
            mods_line(&mods, white, &short).as_deref(),
            Some("Mods: Ann\u{1}FF0000 +2\u{1}FFFFFF")
        );
        assert_eq!(
            mods_line(&mods, white, &|_| false).as_deref(),
            Some("Mods: \u{1}FF0000 +3\u{1}FFFFFF")
        );
        assert_eq!(
            mods_line(&mods[..1], white, &short).as_deref(),
            Some("Mods: Ann")
        );
        assert_eq!(mods_line(&[], white, &wide), None);
    }

    #[test]
    fn the_offered_name_is_the_title_then_the_day_cut_at_a_hundred() {
        assert_eq!(
            offered("Freestyle Game", "4 Days"),
            "(Freestyle Game) 4 Days"
        );
        assert_eq!(
            offered(&"x".repeat(200), "1 Day").chars().count(),
            NAME_CHARS
        );
    }

    /// `ui_load_game.png`'s list, in its order.
    #[test]
    fn the_list_is_in_the_captured_order() {
        let shown = [
            "(Freestyle Game) 0 Days",
            "(Freestyle Game) 0 Days_30Kgold",
            "(Freestyle Game) 2 Days_22713gold",
            "(Freestyle Game) 4 Days distant lands 2",
            "(Freestyle Game) 4 Days distant lands",
            "(Freestyle Game) 5 Days_15101gold_3",
            "frees_s0_1",
            "frees_s0_1_b",
            "frees_s0_2_a",
            "frees_s0_2_b",
            "frees_s1",
            "frees_s1_enchanted_forest_S",
            "frees_s1_goblin_caves_L",
        ];
        let mut sorted = shown;

        sorted.reverse();
        sorted.sort_by(|a, b| order(a, b));

        assert_eq!(sorted, shown);
    }

    /// Newest first; saves of one time by name, as the original's.
    #[test]
    fn the_list_is_newest_first_ties_by_name() {
        let mut saves = [(5, "b"), (9, "z"), (5, "a"), (0, "lands"), (0, "lands 2")];

        saves.sort_by(|&a, &b| newest(a, b));

        assert_eq!(
            saves,
            [(9, "z"), (5, "a"), (5, "b"), (0, "lands 2"), (0, "lands")]
        );
    }

    #[test]
    fn a_rows_stamp_is_the_date_and_time_in_numbers() {
        assert_eq!(stamp((2026, 10, 8, 14, 5)), "2026-10-08 14:05");
        assert_eq!(stamp((2027, 1, 31, 0, 0)), "2027-01-31 00:00");
    }

    /// The stamp, then the days as the description's duration.
    #[test]
    fn a_rows_side_is_its_stamp_and_its_days() {
        use crate::view::panel::day::DAY_MS;

        let gmtx = |i: usize| match i {
            24 => Some("%d Day%s".to_owned()),
            25 => Some("s".to_owned()),
            _ => None,
        };
        let at = (2026, 10, 8, 14, 5);

        assert_eq!(side(at, 4 * DAY_MS, &gmtx), ["2026-10-08 14:05", "4 Days"]);
        assert_eq!(side(at, DAY_MS, &gmtx)[1], "1 Day");
    }

    #[test]
    fn a_save_is_listed_over_a_game_of_its_players_alone() {
        use Players::{Alone, Lan};

        assert!(listed(Alone, Alone));
        assert!(!listed(Lan(2), Alone));
        assert!(!listed(Alone, Lan(2)));
        assert!(listed(Lan(3), Lan(3)));
        assert!(!listed(Lan(2), Lan(3)));
    }

    #[test]
    fn the_games_row_is_lit_else_the_first_on_load_alone() {
        let names = ["a", "b"];

        assert_eq!(lit(&names, Some("b"), Mode::Save), Some(1));
        assert_eq!(lit(&names, Some("c"), Mode::Save), None);
        assert_eq!(lit(&names, None, Mode::Load), Some(0));
        assert_eq!(lit(&[], Some("a"), Mode::Load), None);
    }
}
