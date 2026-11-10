//! HIGH SCORES, `AP64`, in its own art (`regent_core::view::menu::scores`).
//!
//! ```text
//!  AP64, filling the window as the main menu: frame INDh 1000, pog 8730,
//!  title 8005, definition 12, list 8001 under scroll bar 13,
//!  categories 6000-6004, games 7000-7002 (radio boxes, E M F their keys),
//!  ERASE SCORES 9001, MAIN MENU 9002
//! ```

use bevy_egui::egui::{self, pos2};
use regent_core::view::menu::hotkey;
use regent_core::view::menu::scores::{self, Category, GameKind, Table, id};
use regent_core::view::menu::{Layout, ShownControl as Control, State, Tick};

use super::art::{Art, Pack, Pic};
use super::fit;
use super::font::spans;
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas};
use regent_draw::art as service;

/// A row's left margin inside the list, layout pixels. **Inferred**: no
/// capture shows the list; the text boxes' 5.
const ROW_PAD: f32 = 5.0;

/// The high scores' flow: the table, read as their page opens
/// (`crate::files::scores`), what the page shows of it, and where it is
/// kept.
#[derive(bevy::prelude::Resource, Default)]
pub struct Scores {
    pub table: Table,
    pub shown: Shown,
    /// Where the table is kept, and whether the page writes it; none,
    /// nowhere.
    kept: Option<(std::path::PathBuf, Writes)>,
}

/// Whether the high scores' page writes its table as it closes: not for a
/// screenshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Writes {
    Yes,
    No,
}

impl Scores {
    /// The table kept at `path`, read as the page opens and, `writes`,
    /// written as it closes.
    pub fn keep(&mut self, path: std::path::PathBuf, writes: Writes) {
        self.kept = Some((path, writes));
    }

    /// The table read, as `AP64` opens; one that
    /// will not read leaves none shown.
    pub fn read(&mut self) {
        self.shown = Shown::default();
        self.table = match &self.kept {
            Some((path, _)) => crate::files::scores::read(path).unwrap_or_else(|why| {
                bevy::log::warn!("{why}");
                Table::default()
            }),
            None => Table::default(),
        };
    }

    /// The table written, as `AP64` closes.
    pub fn write(&self) {
        if let Some((path, Writes::Yes)) = &self.kept
            && let Err(why) = crate::files::scores::write(path, &self.table)
        {
            bevy::log::warn!("{why}");
        }
    }
}

/// What the page shows: a game's list of a category, and how far the
/// list is scrolled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub game: GameKind,
    pub category: Category,
    /// Layout pixels.
    pub scroll: f32,
}

/// As the screen opens ([`scores::OPENS`]).
impl Default for Shown {
    fn default() -> Shown {
        let (game, category) = scores::OPENS;

        Shown {
            game,
            category,
            scroll: 0.0,
        }
    }
}

/// The page, `table` shown as `shown` picks; ERASE SCORES or MAIN MENU
/// pressed. A radio box picks a game or category, by a press or its key.
pub fn page(
    ctx: &egui::Context,
    (art, shared, shown): (&mut Art, &mut service::Art, &mut Shown),
    table: &Table,
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.scores.as_ref()?;
    let words = &book.words;
    let mut canvas = Canvas::filling(ctx, (ink, shared), paint::size(layout));

    canvas.backdrop();

    // The frame, then the pog the panel names: the screen sets none.
    for c in [Some(&layout.panel), layout.control(id::POG)] {
        if let Some((record, set)) = c.and_then(|c| c.art) {
            let at = c
                .and_then(|c| c.rect)
                .map_or(pos2(0.0, 0.0), |r| fit::rect(r).min);

            canvas.picture(Pack::Base, Pic::of(record, set), at);
        }
    }

    for (id, text) in [
        (
            id::TITLE,
            layout.control(id::TITLE).and_then(|c| c.text.clone()),
        ),
        (id::DEFINITION, words.gmtx(shown.category.definition_text())),
    ] {
        if let (Some(c), Some(text)) = (layout.control(id), text) {
            canvas.label(c, text.trim(), paint::pick(c, State::Normal));
        }
    }

    pick(&mut canvas, layout, shown, &paint::keys(ctx));

    let rows: Vec<String> = table
        .list(shown.game, shown.category)
        .iter()
        .filter_map(|e| scores::row(shown.category, e, &|i| words.gmtx(i)))
        .collect();

    list(&mut canvas, layout, &rows, &mut shown.scroll);

    canvas.buttons_by(
        Pack::Base,
        layout,
        (&[Button::EraseScores, Button::MainMenu], control),
    )
}

/// `AP64`'s control for `button`: MAIN MENU is its own 9002.
const fn control(button: Button) -> i32 {
    match button {
        Button::MainMenu => id::MAIN_MENU,
        _ => id::ERASE,
    }
}

/// The radio boxes drawn, the shown game and category ticked; one pressed,
/// or whose hot key is in `keys`, picked, the list back at its top.
fn pick(canvas: &mut Canvas<'_>, layout: &Layout, shown: &mut Shown, keys: &[i32]) {
    let radios: Vec<(&Control, Pick)> = GameKind::ALL
        .map(|g| (g.control(), Pick::Game(g)))
        .into_iter()
        .chain(Category::ALL.map(|c| (c.control(), Pick::Category(c))))
        .filter_map(|(id, p)| Some((layout.control(id)?, p)))
        .collect();
    let keyed = keys
        .iter()
        .find_map(|&k| hotkey::pick(radios.iter().map(|(c, _)| (*c, hotkey::Able::Enabled)), k));
    let mut picked = None;

    for (c, p) in &radios {
        let tick = if p.chosen(*shown) {
            Tick::Checked
        } else {
            Tick::Clear
        };
        let pressed = canvas.ticked(Pack::Base, c, (Able::Enabled, tick));

        if pressed || keyed.is_some_and(|k| k.id == c.id) {
            picked = Some(*p);
        }
    }

    match picked {
        Some(Pick::Game(g)) => {
            shown.game = g;
            shown.scroll = 0.0;
        }
        Some(Pick::Category(c)) => {
            shown.category = c;
            shown.scroll = 0.0;
        }
        None => {}
    }
}

/// A radio box's pick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pick {
    Game(GameKind),
    Category(Category),
}

impl Pick {
    fn chosen(self, shown: Shown) -> bool {
        match self {
            Pick::Game(g) => g == shown.game,
            Pick::Category(c) => c == shown.category,
        }
    }
}

/// The list: `rows` in its font a [`scores::ROW`] apart from its top,
/// their markup's colours over its own, clipped to it, scrolled by the
/// wheel and, where rows are left over, its scroll bar.
fn list(canvas: &mut Canvas<'_>, layout: &Layout, rows: &[String], scroll: &mut f32) {
    let Some(c) = layout.control(id::LIST) else {
        return;
    };
    let (Some(r), Some(font)) = (c.rect, c.font.and_then(|f| canvas.ink.font(f))) else {
        return;
    };
    let rect = fit::rect(r);
    #[allow(clippy::cast_precision_loss, reason = "a row's pixels")]
    let pitch = scores::ROW as f32;
    #[allow(clippy::cast_precision_loss, reason = "at most ten rows")]
    let most = crate::ui::scroll::most(rows.len() as f32 * pitch, rect.height());
    let screen = canvas.fit.rect(rect);

    canvas.wheeled(screen, ("score list", c.id), (scroll, most));

    let painter = canvas.painter().with_clip_rect(screen);
    let colour = paint::pick(c, State::Normal);
    let cap = font.cap(canvas.fit.scale);

    for (n, row) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "at most ten rows")]
        let y = rect.top() + n as f32 * pitch + f32::midpoint(pitch, cap) - *scroll;

        font.draw(
            &painter,
            canvas.fit,
            pos2(rect.left() + ROW_PAD, y),
            &spans(row, colour),
        );
    }

    canvas.barred(Pack::Base, layout.control(id::BAR), (scroll, most, pitch));
}
