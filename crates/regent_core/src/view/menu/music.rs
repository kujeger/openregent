//! What the front end plays: music by screen, a click as a control is
//! pressed. No hover, page or room object makes a sound.
//!
//! ```text
//!  start-up, AP29, AP30/APdb ──> track 2  GeneralTheme.mp3
//!  APc9, AP44 ─────────────────> track 6  EpicQuest.mp3
//!  any other screen ───────────> what plays plays on
//!  a game's PlayMusic ─────────> its track, at the next update: a cut
//!  a press going down ─────────> CK05 (CK04 check, radio; CK07 list;
//!                                 a scroll bar, text or combo box none)
//! ```
//!
//! **Binary**: one music manager for menus and
//! game. A screen asks its track on entry; the
//! track playing is not restarted, another is stopped first, no fade. A
//! track is played once; the manager's update (method, every menu and game
//! pass) polls every [`POLL_MS`] and starts a silent track again, so a
//! screen's track loops with a gap under a poll..
//!
//! **Corpus**: `soundfx.cam`'s `OS01`–`OS10`, "OS Map", "OS Page flip",
//! "OS Horn" and the rest, are the room objects' sounds by name, but no
//! code, panel or picture of any edition names them or their `DSND`s.

use crate::data::fourcc::FourCc;
use crate::data::smnu::Kind;

use super::{BRIEFING, MAIN, SUMMARY};
use crate::view::menu::quest_map;

/// `MusicTracks.txt` line 2, `GeneralTheme.mp3`: played at start-up,
/// on the main menu
/// and the quest map.
pub const THEME: i32 = 2;

/// `MusicTracks.txt` line 6, `EpicQuest.mp3`: a quest's briefing
/// and summary.
pub const EPIC: i32 = 6;

/// The 800×600 quest map, shown when no `UIData` file holds
/// [`quest_map::PANEL`].
pub const OLD_MAP: FourCc = FourCc(*b"AP30");

/// How often the music manager looks for silence, real ms: its
/// constructor's a field. A silent track starts again at
/// the first poll that finds it silent (also 500, is met at once:
/// `GE_MusicManager`).
pub const POLL_MS: u64 = 500;

/// The track screen `panel` asks for as it opens; `None` for one that
/// asks none, under which what plays plays on (the name prompt `AP35`,
/// the options `AP37`, the extra quests' list `APd8`, the question
/// `AP11`).
///
/// ```text
///  AP29 main menu   2      AP30, APdb quest map   2
///  APc9 briefing    6      AP44 summary           6
/// ```
#[must_use]
pub fn track(panel: FourCc) -> Option<i32> {
    match panel {
        p if p == MAIN || p == OLD_MAP || p == quest_map::PANEL => Some(THEME),
        p if p == BRIEFING || p == SUMMARY => Some(EPIC),
        _ => None,
    }
}

/// The click a control of class `kind` makes as a press goes down on it,
/// a `WAVE` played straight, on the effects' category.
///
/// **Binary**: `main` registers them on the dialog manager by class
/// (`main`–`main`): every class `CK05`, then check and radio
/// `CK04`, list box `CK07`. A press going down on an enabled control, or
/// back onto it while held, sends message `0x3d` with key 0;
/// method looks the key up (the
/// control, its parent, the dialog, the manager) and plays the `WAVE`,
/// category 1, which the effects volume sets
/// (prefs registry load, record 7). Letting go sends key 1, for
/// which none is registered. A disabled control takes no press.
/// `None` for a silent class: a scroll bar's and a
/// combo box's method are empty, a text box's
/// method calls none.
#[must_use]
pub const fn click(kind: Option<Kind>) -> Option<FourCc> {
    match kind {
        Some(Kind::Scrollbar | Kind::Combobox | Kind::Textbox) => None,
        Some(Kind::Check | Kind::Radio) => Some(CHECK),
        Some(Kind::Listbox) => Some(LIST),
        _ => Some(PRESS),
    }
}

/// Every class's click but three's: `Click 5`.
pub const PRESS: FourCc = FourCc(*b"CK05");

/// A check box's or radio button's: `Click 4`.
pub const CHECK: FourCc = FourCc(*b"CK04");

/// A list box's: `Click 7`.
pub const LIST: FourCc = FourCc(*b"CK07");

/// Every click [`click`] gives.
pub const CLICKS: [FourCc; 3] = [PRESS, CHECK, LIST];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_menu_and_map_play_the_theme_a_briefing_the_epic() {
        assert_eq!(track(MAIN), Some(THEME));
        assert_eq!(track(quest_map::PANEL), Some(THEME));
        assert_eq!(track(OLD_MAP), Some(THEME));
        assert_eq!(track(BRIEFING), Some(EPIC));
        assert_eq!(track(SUMMARY), Some(EPIC));
        assert_eq!(track(super::super::NAME_PROMPT), None);
    }

    #[test]
    fn a_button_clicks_five_a_check_four_a_list_seven_a_scroll_bar_none() {
        assert_eq!(click(Some(Kind::Button)), Some(PRESS));
        assert_eq!(click(Some(Kind::Item)), Some(PRESS));
        assert_eq!(click(None), Some(PRESS));
        assert_eq!(click(Some(Kind::Radio)), Some(CHECK));
        assert_eq!(click(Some(Kind::Listbox)), Some(LIST));
        assert_eq!(click(Some(Kind::Scrollbar)), None);
        assert_eq!(click(Some(Kind::Textbox)), None);
    }
}
