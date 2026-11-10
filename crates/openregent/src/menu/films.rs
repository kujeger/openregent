//! The Cinematics page: `APd8`'s panel and rows (`extras`), its words
//! changed, listing the films a profile's victories unlock
//! (`Progress::films`). openregent's; the original's CINEMATIC just plays
//! ([docs/openregent/enhancements.md](docs/openregent/enhancements.md)).

use bevy_egui::egui::{self, Sense};
use regent_core::data::smnu;
use regent_core::view::menu::extras::{CANCEL, DESCRIPTION, LIST, LOAD, TITLE};
use regent_core::view::menu::film::Film;
use regent_core::view::menu::hotkey;
use regent_core::view::menu::listbox::{self, Place};
use regent_core::view::menu::saves::{ROW_HEIGHT, rows as shown_rows};
use regent_core::view::menu::{Layout, ShownControl, State, ids};

use super::art::{Art, Pack};
use super::fit;
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas};
use super::save_page::{LIT, NAME_INDENT};
use super::setup::{boxed, ground, line};
use regent_draw::art as service;

/// `APd8`'s words here, by what it says there.
const WORDS: [(&str, &str); 4] = [
    ("Downloaded Quests", "Cinematics"),
    ("LOAD QUEST", "PLAY"),
    ("LIST OF QUESTS", "LIST OF CINEMATICS"),
    ("GAME DESCRIPTION", "DESCRIPTION"),
];

/// `extras`, its words made this page's.
#[must_use]
pub fn dressed(mut extras: Layout) -> Layout {
    for c in &mut extras.controls {
        let new = c.text.as_deref().and_then(|text| {
            WORDS
                .iter()
                .find(|(old, _)| text.trim().eq_ignore_ascii_case(old))
                .map(|(_, new)| (*new).to_owned())
        });

        if new.is_some() {
            c.text = new;
        }
    }

    extras
}

/// The list: its films and the lit row, kept while a film plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Films {
    listed: Vec<Film>,
    lit: usize,
}

impl Default for Films {
    fn default() -> Films {
        Films::of(Film::ALL.to_vec())
    }
}

impl Films {
    /// `listed`, the first lit.
    #[must_use]
    pub const fn of(listed: Vec<Film>) -> Films {
        Films { listed, lit: 0 }
    }

    /// The row a press would play.
    fn chosen(&self) -> Option<Press> {
        self.listed.get(self.lit).copied().map(Press::Film)
    }

    /// `key` moved the lit row, if it does.
    fn take(&mut self, key: i32) -> bool {
        let texts: Vec<&str> = self.listed.iter().map(|f| f.title()).collect();
        let place = Place {
            lit: Some(self.lit),
            top: 0,
        };
        let moved = listbox::key(place, key, (&texts, texts.len()));

        if let Some(lit) = moved.and_then(|p| p.lit) {
            self.lit = lit;
        }

        moved.is_some()
    }
}

/// The list over the faces: the film PLAY, Enter or a double click chose,
/// or CANCEL.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    films: &mut Films,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.films.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), paint::size(layout));

    canvas.backdrop();
    canvas.panel_art(Pack::Base, layout);

    let pressed = controls(&mut canvas, layout, films).or_else(|| keyed(ctx, layout, films));

    match pressed? {
        LOAD => films.chosen(),
        _ => Some(Press::Button(Button::Cancel)),
    }
}

/// `layout`'s controls as `extras` draws them, its scroll bar left out:
/// no list here is long enough.
fn controls(canvas: &mut Canvas<'_>, layout: &Layout, films: &mut Films) -> Option<i32> {
    let mut pressed = None;

    for c in &layout.controls {
        match c.id.unwrap_or_default() {
            id @ (LOAD | CANCEL) => {
                if canvas.button(Pack::Base, c, Able::Enabled) {
                    pressed = Some(id);
                }
            }
            LIST => {
                ground(canvas, c);

                if rows(canvas, c, films) {
                    pressed = Some(LOAD);
                }
            }
            ids::SCROLL_BAR => {}
            DESCRIPTION => {
                let text = films.listed.get(films.lit).map_or("", |f| f.blurb());

                ground(canvas, c);
                canvas.held_note(c, text);
            }
            TITLE => boxed(canvas, c, c.text.as_deref()),
            _ => {
                canvas.clip = c.rect.map(fit::rect);
                boxed(canvas, c, c.text.as_deref());
                canvas.clip = None;
            }
        }
    }

    pressed
}

/// The keys typed: the list's arrows first, the rest press a button, Enter
/// PLAY, Escape CANCEL.
fn keyed(ctx: &egui::Context, layout: &Layout, films: &mut Films) -> Option<i32> {
    let offered: Vec<(&ShownControl, hotkey::Able)> = [LOAD, CANCEL]
        .iter()
        .filter_map(|&id| Some((layout.control(id)?, hotkey::Able::Enabled)))
        .collect();

    paint::cursor_keys(ctx)
        .into_iter()
        .chain(paint::keys(ctx))
        .filter(|&key| !films.take(key))
        .find_map(|key| hotkey::pick(offered.iter().copied(), key))
        .and_then(|c| c.id)
}

/// One title a row down list `c`, the lit one's band behind; a press
/// lights its row. Whether one was double clicked.
fn rows(canvas: &mut Canvas<'_>, c: &ShownControl, films: &mut Films) -> bool {
    let colour = paint::pick(c, State::Normal);
    let mut doubled = false;

    if let Some(r) = c.rect {
        for n in 0..films.listed.len().min(shown_rows(r)) {
            let row = i32::try_from(n).unwrap_or_default();
            let band = smnu::Rect {
                y: r.y + row * ROW_HEIGHT,
                height: ROW_HEIGHT,
                ..r
            };
            let at = canvas.fit.rect(fit::rect(band));
            let response =
                canvas
                    .ui
                    .interact(at, canvas.ui.id().with(("film row", n)), Sense::click());

            if response.clicked() {
                films.lit = n;
            }

            doubled |= response.double_clicked();

            if films.lit == n {
                canvas.painter().rect_filled(at, 0.0, LIT);
            }

            let text = ShownControl {
                rect: Some(smnu::Rect {
                    x: band.x + NAME_INDENT,
                    width: band.width - NAME_INDENT,
                    ..band
                }),
                ..c.clone()
            };

            line(canvas, &text, (films.listed[n].title(), colour));
        }
    }

    doubled
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list's first row is lit, and the arrows stop at its ends.
    #[test]
    fn the_arrows_move_the_lit_film_within_the_list() {
        let mut films = Films::of(vec![Film::Opening, Film::North]);

        assert_eq!(films.chosen(), Some(Press::Film(Film::Opening)));
        assert!(films.take(hotkey::UP));
        assert_eq!(films.lit, 0);
        assert!(films.take(hotkey::DOWN));
        assert_eq!(films.chosen(), Some(Press::Film(Film::North)));
        assert!(films.take(hotkey::DOWN));
        assert_eq!(films.lit, 1);
    }
}
