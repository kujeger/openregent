//! Hot keys: a key press as the engine codes it, and the control it
//! presses. A control's key is its `SMNU` key 5 ([`smnu::HOTKEY`]).
//!
//! **Binary**:
//!
//! ```text
//!  key window       WM_CHAR: the character; WM_KEYDOWN: F1-F15 and
//!       │           the cursor keys as SPECIAL + n; Shift, Ctrl, Alt high
//!  key queue        a 20-key queue
//!       │
//!  CYDialog         the focused control takes it first (a text
//!       │           box typing); else
//!  CYDialog         the first control in panel order that is shown, not
//!                   disabled, and whose key matches:
//!                   pressed as a click presses it
//! ```

use crate::data::smnu;
use crate::view::menu::ShownControl;

/// Shift held: the pressed code's bit (key window: `GetKeyState`).
pub const SHIFT: i32 = 0x4_0000;

/// Ctrl held.
pub const CTRL: i32 = 0x8_0000;

/// Alt held.
pub const ALT: i32 = 0x4000_0000;

/// The keys no character types: `SPECIAL` + n.
pub const SPECIAL: i32 = 0xe000;

/// The cursor keys.
pub const PAGE_UP: i32 = SPECIAL + 1;
pub const RIGHT: i32 = SPECIAL + 2;
pub const PAGE_DOWN: i32 = SPECIAL + 3;
pub const DOWN: i32 = SPECIAL + 4;
pub const END: i32 = SPECIAL + 5;
pub const LEFT: i32 = SPECIAL + 6;
pub const HOME: i32 = SPECIAL + 7;
pub const UP: i32 = SPECIAL + 8;
pub const INSERT: i32 = SPECIAL + 9;
pub const DELETE: i32 = SPECIAL + 10;

/// F1; F2 to F15 follow (`VK_F1` + `0xdfa0`). `APMK` holds F9 to F12.
pub const F1: i32 = SPECIAL + 0x10;

/// Tab, Enter, Escape: their characters.
pub const TAB: i32 = 9;
pub const ENTER: i32 = 13;
pub const ESCAPE: i32 = 27;

/// The low word: the character or special key.
pub const CODE: i32 = 0xffff;

/// The first character past the space: printable keys from here.
const PRINTABLE: std::ops::Range<i32> = 0x21..0x100;

pub use crate::view::menu::Able;

/// Whether `pressed` is hot key `key`: letters in either
/// case, Shift ignored where the key is a printable character without
/// it ('?' is Shift and '/'); Ctrl and Alt must match. The key is taken
/// upper-cased, as item hotkey set stores it.
#[must_use]
pub fn hits(key: i32, pressed: i32) -> bool {
    let key = upper(key);
    let ignored = if key & SHIFT == 0 && PRINTABLE.contains(&(key & CODE)) {
        SHIFT
    } else {
        0
    };

    key != 0 && key == upper(pressed) & !ignored
}

/// `code` with its low word upper-cased where under `0x100` (`toupper`).
fn upper(code: i32) -> i32 {
    let low = code & CODE;
    let upper = match u8::try_from(low) {
        Ok(b) => i32::from(b.to_ascii_uppercase()),
        Err(_) => low,
    };

    code & !CODE | upper
}

/// The control `pressed` presses: the first of
/// `controls`, in panel order, enabled and with a key it [`hits`].
pub fn pick<'a>(
    controls: impl IntoIterator<Item = (&'a ShownControl, Able)>,
    pressed: i32,
) -> Option<&'a ShownControl> {
    controls
        .into_iter()
        .filter(|(c, able)| *able == Able::Enabled && c.flags & smnu::DISABLED == 0)
        .find(|(c, _)| c.hotkey.is_some_and(|k| hits(k, pressed)))
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const R: i32 = b'R' as i32;

    #[test]
    fn a_letter_key_takes_either_case_and_shift_but_not_ctrl() {
        assert!(hits(R, i32::from(b'r')));
        assert!(hits(R, R));
        assert!(hits(R, R | SHIFT));
        assert!(!hits(R, i32::from(b'r') | CTRL));
        assert!(!hits(R, i32::from(b'r') | ALT));
        assert!(!hits(R, i32::from(b'e')));
    }

    #[test]
    fn a_question_mark_is_shift_and_slash_and_still_hits() {
        assert!(hits(i32::from(b'?'), i32::from(b'?') | SHIFT));
    }

    #[test]
    fn tab_and_enter_take_no_shift_off() {
        assert!(hits(TAB, TAB));
        assert!(!hits(TAB, TAB | SHIFT));
        assert!(hits(ESCAPE, ESCAPE));
        assert!(hits(F1 + 8, F1 + 8));
    }

    /// A key is stored upper-cased, so a lower
    /// case one hits as its capital.
    #[test]
    fn a_lower_case_key_hits_as_its_capital() {
        assert!(hits(i32::from(b'r'), i32::from(b'r')));
        assert!(hits(i32::from(b'r'), R));
    }

    #[test]
    fn no_key_hits_nothing() {
        assert!(!hits(0, 0));
    }

    #[test]
    fn the_first_enabled_control_with_the_key_is_pressed() {
        let control = |id, key| ShownControl {
            id: Some(id),
            hotkey: Some(key),
            ..ShownControl::default()
        };
        let (a, b, c) = (control(1, R), control(2, R), control(3, i32::from(b'H')));

        let first = [
            (&a, Able::Enabled),
            (&b, Able::Enabled),
            (&c, Able::Enabled),
        ];
        let blocked = [(&a, Able::Disabled), (&b, Able::Enabled)];
        let none = [(&a, Able::Disabled), (&c, Able::Enabled)];

        assert_eq!(pick(first, i32::from(b'r')).and_then(|c| c.id), Some(1));
        assert_eq!(pick(blocked, i32::from(b'r')).and_then(|c| c.id), Some(2));
        assert_eq!(pick(none, i32::from(b'r')), None);
    }

    #[test]
    fn a_control_disabled_in_its_flags_is_passed_over() {
        let off = ShownControl {
            hotkey: Some(R),
            flags: smnu::DISABLED,
            ..ShownControl::default()
        };

        assert_eq!(pick([(&off, Able::Enabled)], R), None);
    }
}
