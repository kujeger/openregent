//! The freestyle setup's LOAD and SAVE pages: Load Freestyle Options,
//! `MX20`, and Save Freestyle Options, `MX21`. **Binary**:
//! `FreestyleIO_Controller`.
//!
//! ```text
//!  screen switch       MX20 (the load page) or MX21 (the save page), each
//!                      closing back to MX17; Enter is LOAD or SAVE
//!      (open)          SAVE: the config's name in the name box; the
//!                      list filled
//!  the list            each settings file whose player count is the
//!                      session's; the row named as the config lit, else
//!                      the first on LOAD, none on SAVE
//!  LOAD 9000           the lit row's file read over the config, the
//!                      tree of its name loaded; closed
//!  SAVE 9002           the config named as typed and written: over the
//!                      file of that name, else a new one; no question
//!                      asked; closed
//!  DELETE 9003         the lit row's file removed; the list filled again
//!  CANCEL 9001         closed, nothing kept
//! ```
//!
//! A row pressed is lit and nothing else: the controller has no case for
//! the list, so SAVE's name box keeps what is typed.
//!
//! The original keeps the settings named N in `freeN.STY` in
//! `FreeStyleIOPath` (`freestyle`, under the user directory;
//! Freestyle io new path).

use crate::data::fourcc::FourCc;

/// The two panels, in `mx_textdata.cam`.
pub const LOAD_PAGE: FourCc = FourCc(*b"MX20");
pub const SAVE_PAGE: FourCc = FourCc(*b"MX21");

/// LOAD SETTINGS, hot key L; CANCEL, Escape; SAVE SETTINGS, S; DELETE
/// SETTINGS, D. LOAD shows on `MX20` alone, SAVE on `MX21`.
pub const LOAD: i32 = 9000;
pub const CANCEL: i32 = 9001;
pub const SAVE: i32 = 9002;
pub const DELETE: i32 = 9003;

/// The name typed (`0x232e`), `MX21`'s alone.
pub const NAME: i32 = 9006;

/// The settings' list (`0x238c`) and its scroll bar
/// ([`super::ids::SCROLL_BAR`]).
pub const LIST: i32 = 9100;

/// A row's height: the list box's keys 28 and 29.
pub const ROW_HEIGHT: i32 = 20;

/// A network game's chat, hidden alone: its texts, box and SEND CHAT;
/// its scroll bar is [`Mode::chat_bar`]. **Inferred**:
/// `FreestyleIO_Controller` hides 5004 and 5006 outside a network
/// game; the rest lie under the picture 3887, drawn last.
pub const CHAT: [i32; 5] = [2, 4, 5004, 5005, 5006];

/// The characters the name box refuses: none a file name
/// may hold on Windows, and the full stop.
pub const REFUSED: [char; 9] = ['"', '.', '/', ':', '<', '>', '?', '\\', '|'];

/// Which of the two pages.
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

    /// The chat's scroll bar: 13 on `MX20`, 15 on `MX21`, where 13 is
    /// the title.
    #[must_use]
    pub const fn chat_bar(self) -> i32 {
        match self {
            Mode::Load => 13,
            Mode::Save => 15,
        }
    }

    /// The button Enter presses.
    #[must_use]
    pub const fn confirm(self) -> i32 {
        match self {
            Mode::Load => LOAD,
            Mode::Save => SAVE,
        }
    }
}

/// Whether the name box takes `c`.
#[must_use]
pub fn typable(c: char) -> bool {
    !REFUSED.contains(&c) && !c.is_control()
}

/// The row lit as the list fills: the one named as the
/// config, compared exactly; else the first on LOAD, none on SAVE.
#[must_use]
pub fn lit(names: &[&str], config: &str, mode: Mode) -> Option<usize> {
    let own = names.iter().position(|n| *n == config);

    match mode {
        Mode::Load => own.or((!names.is_empty()).then_some(0)),
        Mode::Save => own,
    }
}

/// Whether a settings file for `players` is listed in a session of
/// `session` players. **Inferred**: the word compared,
/// read from the file, is its player count.
#[must_use]
pub const fn listed(players: u32, session: u32) -> bool {
    players == session
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_s_row_is_lit_else_the_first_on_load_alone() {
        let names = ["funbar", "more settings"];

        assert_eq!(lit(&names, "more settings", Mode::Save), Some(1));
        assert_eq!(lit(&names, "More Settings", Mode::Save), None);
        assert_eq!(lit(&names, "Default Settings", Mode::Load), Some(0));
        assert_eq!(lit(&[], "Default Settings", Mode::Load), None);
    }

    #[test]
    fn the_name_box_refuses_what_a_file_name_cannot_hold() {
        assert!("funbar 2*&'é".chars().all(typable));
        assert!(!"a.b".chars().all(typable));
        assert!(!typable('\\'));
    }
}
