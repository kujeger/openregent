//! A control's colours: its own lists, else its panel's palette.
//!
//! ```text
//!  an item's colour, role r at state s:
//!    own     the control's list entry; 0x80000000 with no list for r,
//!            0xc0000000 for a state its list leaves out
//!    own has no INHERIT bit                 -> own
//!    the parent's colour has none           -> the parent's
//!    the dialog's style colour has none     -> the style's
//!    else own, or with BLEND the unresolved one
//!
//!  the style (CYDialog): the panel's PALT (key 11),
//!  entry first + s
//!    role 0 fill 16   1 light 19   2 shade 22   10: 32   11: 35   12: 38
//!    role 3 text 48   4 hot key 51   6: 54   8: 57   9: 60
//!    role 7 role 0's; role 13 entries 64-66 as screen words
//! ```
//!
//! **Game**: `MMS1`, every game panel's palette, gives text orange at
//! rest (48, `f8b152`: `ui_gambling_hall_spinning.png`'s "Wheel is
//! Spinning..."), white lit (49: a held RESEARCH,
//! `ui_button_click_hold.png`) and red disabled (50, `db2124`: "Nobody
//! Here"); the hot key letter blue, light blue held, red disabled.

use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;
use crate::data::payload::Palette;
use crate::data::smnu::{self, Panel};

use super::State;

/// The archive in an install's `Data` holding the panels' palettes
/// (`MMS1`, `TTIP`, `IBOX`), which a `UIData` file lacks.
pub const PALETTES: &str = "interfacedata.cam";

/// A colour word's bit: the word is `0x80rrggbb`.
pub const SET: u32 = 0x8000_0000;

/// The bits that defer a colour word to the parent, then the style
/// (item colour tests both).
pub const INHERIT: u32 = 0x6000_0000;

/// The bit that, when no one resolves the colour, hands on the last
/// unresolved word rather than the control's own.
pub const BLEND: u32 = 0x2000_0000;

/// A control's word for a role it has no list for.
pub const NO_LIST: u32 = SET;

/// A state's word that a list leaves out (item colour set's
/// new entries).
pub const UNLISTED: u32 = 0xc000_0000;

/// The style's word for a role it has no colour for.
pub const UNSTYLED: u32 = BLEND;

/// The states a colour role holds: rest, lit, disabled.
pub const STATES: usize = 3;

/// The colour roles a text uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Role 3, the text (`crate::data::smnu::TEXT_COLOURS`).
    Text,
    /// Role 4, the hot key's letter (`crate::data::smnu::HOTKEY_COLOURS`).
    HotKey,
}

impl Role {
    /// Its number, the engine's.
    #[must_use]
    pub const fn number(self) -> i32 {
        match self {
            Role::Text => 3,
            Role::HotKey => 4,
        }
    }

    /// Its rest state's entry in the panel's palette ([`STYLE_ENTRIES`]).
    #[must_use]
    pub fn entry(self) -> usize {
        STYLE_ENTRIES
            .iter()
            .find_map(|&(role, entry)| (role == self.number()).then_some(entry))
            .unwrap_or_default()
    }
}

/// Each colour role the style takes from the panel's palette, and the
/// entry of its rest state; lit and disabled follow. Role 7 copies role
/// 0; role 13 takes entries 64 to 66 as screen words, not colour words.
///
/// **Binary**: `CYDialog` reads the palette's bytes from entry 16
/// up (entries start, red at `+2`: palette screen word,
/// palette nearest), four bytes a state.
pub const STYLE_ENTRIES: [(i32, usize); 11] = [
    (0, 16),
    (1, 19),
    (2, 22),
    (10, 32),
    (11, 35),
    (12, 38),
    (3, 48),
    (4, 51),
    (6, 54),
    (8, 57),
    (9, 60),
];

/// A panel's style colours, from its palette: role 3 and 4 by state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    text: [u32; STATES],
    hotkey: [u32; STATES],
}

impl Style {
    /// The style `palette` gives.
    #[must_use]
    pub fn of(palette: &Palette) -> Style {
        let role = |role: Role| {
            std::array::from_fn(|state| {
                let [r, g, b] = palette.colours[role.entry() + state].channels();

                SET | u32::from_be_bytes([0, r, g, b])
            })
        };

        Style {
            text: role(Role::Text),
            hotkey: role(Role::HotKey),
        }
    }

    /// The style of PALT `palette` in `catalogue` ([`Style::palette`]);
    /// `None` without one, where every role is [`UNSTYLED`].
    #[must_use]
    pub fn find(catalogue: &Catalogue<'_>, palette: Option<FourCc>) -> Option<Style> {
        let at = catalogue.find(b"PALT", palette?)?;
        let record = catalogue.record(b"PALT", at)?;

        Palette::parse(record.data).ok().map(|p| Style::of(&p))
    }

    /// The panel's palette: key 11 of its own block.
    #[must_use]
    pub fn palette(panel: &Panel) -> Option<FourCc> {
        panel.controls().first().and_then(smnu::Control::palette)
    }

    /// Role `role`'s word in `state`.
    #[must_use]
    pub const fn word(&self, role: Role, state: State) -> u32 {
        match role {
            Role::Text => self.text[state.index()],
            Role::HotKey => self.hotkey[state.index()],
        }
    }
}

/// A control's text colours as resolved, by role and state; all
/// [`None`](Inks::colour) for a control made in code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inks {
    text: [u32; STATES],
    hotkey: [u32; STATES],
}

impl Inks {
    /// The colours a control with lists `text` and `hotkey` (empty: no
    /// list) takes under `style`.
    #[must_use]
    pub fn of((text, hotkey): (&[i32], &[i32]), style: Option<Style>) -> Inks {
        let role = |list: &[i32], role: Role| {
            [State::Normal, State::Lit, State::Disabled]
                .map(|state| resolve(own(list, state), None, style.map(|s| s.word(role, state))))
        };

        Inks {
            text: role(text, Role::Text),
            hotkey: role(hotkey, Role::HotKey),
        }
    }

    /// Role `role`'s word in `state`.
    #[must_use]
    pub const fn word(&self, role: Role, state: State) -> u32 {
        match role {
            Role::Text => self.text[state.index()],
            Role::HotKey => self.hotkey[state.index()],
        }
    }

    /// That word's colour ([`rgb`]).
    #[must_use]
    pub const fn colour(&self, role: Role, state: State) -> Option<[u8; 3]> {
        rgb(self.word(role, state))
    }
}

/// The control's own word in `state` from its list `words`; `None`
/// with no list (`words` empty).
#[must_use]
pub fn own(words: &[i32], state: State) -> Option<u32> {
    (!words.is_empty()).then(|| {
        words
            .get(state.index())
            .map_or(UNLISTED, |w| w.cast_unsigned())
    })
}

/// The colour item colour gives: `own` unless it defers or is
/// missing, then `parent`'s, then `style`'s, each where it resolves; a
/// missing own word is [`NO_LIST`].
///
/// **Inferred**: no `SMNU` control has a parent; the panels
/// pass `None`.
#[must_use]
pub fn resolve(own: Option<u32>, parent: Option<u32>, style: Option<u32>) -> u32 {
    let resolves = |w: u32| w & INHERIT == 0;
    let kept = own.unwrap_or(NO_LIST);

    // Each unresolved step keeps the word, or with BLEND takes theirs.
    let step = |kept: u32, theirs: u32| {
        if kept & BLEND != 0 { theirs } else { kept }
    };

    match own {
        Some(word) if resolves(word) => word,
        _ => match parent {
            Some(p) if resolves(p) => p,
            _ => {
                let kept = parent.map_or(kept, |p| step(kept, p));

                match style {
                    Some(s) if resolves(s) => s,
                    Some(s) => step(kept, s),
                    None => kept,
                }
            }
        },
    }
}

/// A resolved word's colour, `[r, g, b]`; `None` for a word with no
/// [`SET`] bit, which the panels never draw text in (**Inferred**).
#[must_use]
pub const fn rgb(word: u32) -> Option<[u8; 3]> {
    let [_, r, g, b] = word.to_be_bytes();

    if word & SET != 0 && word & INHERIT == 0 {
        Some([r, g, b])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A control's own colour wins; an inheriting or missing one takes
    /// the style's; with no style, the control's own word stays, black
    /// without a list, and with [`BLEND`] the style's unresolved one.
    #[test]
    fn an_inheriting_colour_takes_the_styles() {
        let orange = 0x80f8_b152;

        assert_eq!(resolve(Some(0x8000_003f), None, Some(orange)), 0x8000_003f);
        assert_eq!(resolve(Some(0x4000_0000), None, Some(orange)), orange);
        assert_eq!(resolve(Some(UNLISTED), None, Some(orange)), orange);
        assert_eq!(resolve(None, None, Some(orange)), orange);
        assert_eq!(resolve(None, None, Some(UNSTYLED)), NO_LIST);
        assert_eq!(resolve(Some(0x4000_0000), None, None), 0x4000_0000);
        assert_eq!(resolve(Some(UNLISTED), None, Some(UNSTYLED)), UNLISTED);
        assert_eq!(resolve(Some(BLEND), None, Some(UNSTYLED)), UNSTYLED);
        assert_eq!(
            resolve(Some(0x4000_0000), None, Some(UNSTYLED)),
            0x4000_0000
        );
        assert_eq!(
            resolve(Some(0x4000_0000), Some(0x80ff_ffff), Some(orange)),
            0x80ff_ffff
        );
    }

    /// A list of one leaves lit and disabled unlisted, for the style.
    #[test]
    fn a_short_list_leaves_its_other_states_unlisted() {
        let navy = [0x8000_003f_u32.cast_signed()];

        assert_eq!(own(&navy, State::Normal), Some(0x8000_003f));
        assert_eq!(own(&navy, State::Lit), Some(UNLISTED));
        assert_eq!(own(&[], State::Disabled), None);
        assert_eq!(rgb(0x80f8_b152), Some([0xf8, 0xb1, 0x52]));
        assert_eq!(rgb(UNLISTED), None);
        assert_eq!(rgb(NO_LIST), Some([0, 0, 0]));
    }
}
