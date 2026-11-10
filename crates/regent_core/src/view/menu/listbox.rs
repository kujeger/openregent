//! A list box's keys. **Binary**: `CYDialogListboxItem`, which
//! takes a key before the panel's hot keys once a
//! press has focused the list (`CYDialogItem` calls method, flag
//! `0x800000`). **Inferred**: no list is focused as its panel opens.
//!
//! ```text
//!  a character  the next row after the lit one whose text starts with
//!               it, case folded, from the top again after the last
//!  Up / Down    the row above or below lit and shown; none past the ends
//!  PgUp / PgDn  a page up or down (message 0x21, mode 3), then the top
//!               or the last shown row lit
//!  Home / End   the top or the bottom shown; the lit row stays
//!  anything else  not taken: on to the hot keys
//! ```

use crate::view::menu::hotkey::{CODE, DOWN, END, HOME, PAGE_DOWN, PAGE_UP, UP};

/// A list's lit row and its first shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub lit: Option<usize>,
    pub top: usize,
}

impl Place {
    /// Scrolled so `row` is shown, as little as it takes (message
    /// `0x21`, mode 0; **Inferred**: the least scroll).
    fn showing(self, row: usize, (count, shown): (usize, usize)) -> Place {
        let top = if row < self.top {
            row
        } else if row >= self.top + shown {
            row + 1 - shown
        } else {
            self.top
        };

        Place {
            top: top.min(count.saturating_sub(shown)),
            ..self
        }
    }

    /// Row `row` lit and shown.
    fn lighting(self, row: usize, rows: (usize, usize)) -> Place {
        Place {
            lit: Some(row),
            ..self
        }
        .showing(row, rows)
    }
}

/// Where `key` leaves `place` in a focused list of `texts`, `shown`
/// rows tall; `None` where the list does not take it.
#[must_use]
pub fn key(place: Place, key: i32, (texts, shown): (&[&str], usize)) -> Option<Place> {
    let count = texts.len();
    let rows = (count, shown);
    let last = count.checked_sub(1);

    match key {
        UP => Some(match place.lit {
            Some(lit) if lit > 0 => place.lighting(lit - 1, rows),
            _ => place,
        }),
        DOWN => Some(match (place.lit, last) {
            (Some(lit), Some(last)) if lit < last => place.lighting(lit + 1, rows),
            (None, Some(_)) => place.lighting(0, rows),
            _ => place,
        }),
        PAGE_UP => {
            let top = place.top.saturating_sub(shown);

            Some(Place {
                lit: Some(top),
                top,
            })
        }
        PAGE_DOWN => {
            let top = (place.top + shown).min(count.saturating_sub(shown));
            let lit = (top + shown)
                .saturating_sub(1)
                .min(last.unwrap_or_default());

            Some(Place {
                lit: Some(lit),
                top,
            })
        }
        HOME => Some(place.showing(0, rows)),
        END => Some(place.showing(count.saturating_sub(1), rows)),
        _ => starting(place, key, texts).map(|row| place.lighting(row, rows)),
    }
}

/// The next row after `place`'s lit one whose text starts with `key`'s
/// character, case folded, the rows before it after.
fn starting(place: Place, key: i32, texts: &[&str]) -> Option<usize> {
    let wanted = char::from_u32((key & CODE).cast_unsigned())?
        .to_uppercase()
        .next()?;
    let from = place.lit.map_or(0, |lit| lit + 1);
    let first = |row: &usize| {
        texts[*row]
            .chars()
            .next()
            .and_then(|c| c.to_uppercase().next())
    };

    (from..texts.len())
        .chain(0..texts.len())
        .find(|row| first(row) == Some(wanted))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXTS: [&str; 5] = ["The Wild", "the Wrath", "Vampiric", "Tomb", "Urban"];

    fn at(lit: usize, top: usize) -> Place {
        Place {
            lit: Some(lit),
            top,
        }
    }

    #[test]
    fn up_and_down_move_the_lit_row_and_stop_at_the_ends() {
        let rows = (&TEXTS[..], 2);

        assert_eq!(key(at(0, 0), DOWN, rows), Some(at(1, 0)));
        assert_eq!(key(at(1, 0), DOWN, rows), Some(at(2, 1)));
        assert_eq!(key(at(4, 3), DOWN, rows), Some(at(4, 3)));
        assert_eq!(key(at(2, 2), UP, rows), Some(at(1, 1)));
        assert_eq!(key(at(0, 0), UP, rows), Some(at(0, 0)));
    }

    #[test]
    fn a_page_key_scrolls_a_page_and_lights_its_edge() {
        let rows = (&TEXTS[..], 2);

        assert_eq!(key(at(0, 0), PAGE_DOWN, rows), Some(at(3, 2)));
        assert_eq!(key(at(3, 2), PAGE_DOWN, rows), Some(at(4, 3)));
        assert_eq!(key(at(4, 3), PAGE_UP, rows), Some(at(1, 1)));
    }

    #[test]
    fn home_and_end_scroll_and_keep_the_lit_row() {
        let rows = (&TEXTS[..], 2);

        assert_eq!(key(at(1, 0), END, rows), Some(at(1, 3)));
        assert_eq!(key(at(1, 3), HOME, rows), Some(at(1, 0)));
    }

    #[test]
    fn a_letter_lights_the_next_row_it_starts() {
        let rows = (&TEXTS[..], 2);
        let t = i32::from(b't');

        assert_eq!(key(at(0, 0), t, rows), Some(at(1, 0)));
        assert_eq!(key(at(1, 0), t, rows), Some(at(3, 2)));
        assert_eq!(key(at(3, 2), t, rows), Some(at(0, 0)));
        assert_eq!(key(at(0, 0), i32::from(b'x'), rows), None);
        assert_eq!(key(at(0, 0), crate::view::menu::hotkey::ENTER, rows), None);
        assert_eq!(key(at(0, 0), crate::view::menu::hotkey::LEFT, rows), None);
    }
}
