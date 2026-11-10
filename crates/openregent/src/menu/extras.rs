//! The extra quests' list drawn, `APd8` (`regent_core::view::menu::extras`),
//! from the base game's art as `save_page` draws `AP27`, over the faces.
//!
//! ```text
//!  APd8  ┌(pog)────── Downloaded Quests ───────┐
//!        │          [LIST OF QUESTS]           │
//!        │ ┌─────────────────────────────────┐^│
//!        │ │ 13 - Big Map    Majesty Expanded│#│  rows by title; a mod
//!        │ │ The Wrath of Krolm          lit │v│  folder's quest names
//!        │ [   ]  [GAME DESCRIPTION]   [   ]   │  its item's mod, dim
//!        │ (LOAD QUEST)      ()      (CANCEL)  │
//!        └─────────────────────────────────────┘
//! ```

use bevy_egui::egui::{self, Color32, Sense};
use regent_core::content::dataset::{self, Bytecode};
use regent_core::content::mods::Library;
use regent_core::data::smnu;
use regent_core::sim::world::offer::UNAVAILABLE_MARK;
use regent_core::view::menu::extras::{self as rules, CANCEL, DESCRIPTION, LIST, LOAD};
use regent_core::view::menu::hotkey;
use regent_core::view::menu::listbox::{self, Place};
use regent_core::view::menu::quest_map::{self, Progress};
use regent_core::view::menu::saves::{ROW_HEIGHT, rows as shown_rows};
use regent_core::view::menu::{Layout, ShownControl, State, ids};

use super::art::{self, Art, Pack, Words};
use super::fit;
use super::font;
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas};
use super::save_page::{LIT, NAME_INDENT, SIDE_MARGIN, flush_right};
use super::setup::{boxed, ground, line};
use crate::files::env::Install;
use crate::game::load::{Offered, Origin};
use regent_draw::art as service;

/// A mod folder's quest's mod, in its row. Not the original's: its rows
/// hold the title alone.
const SOURCE: Color32 = Color32::from_gray(0xa0);

/// The least gap between a row's title and its mod's name.
const SOURCE_GAP: f32 = 16.0;

/// A quest the list shows.
struct Listed {
    /// Its place in the offered quests.
    quest: usize,
    title: String,
    /// A mod folder's quest's mod ([`rules::source`]).
    source: Option<String>,
    /// Why its game will not start: a file it lists not found.
    refused: Option<String>,
}

/// Whether the list takes keys first ([`listbox`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Focus {
    #[default]
    None,
    /// Since a row was pressed.
    List,
}

/// Whether the list took a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Taken {
    Yes,
    No,
}

/// The list's quests, the lit row, the scroll and the focus.
#[derive(Default, bevy::prelude::Resource)]
pub struct Extras {
    listed: Vec<Listed>,
    lit: Option<usize>,
    scroll: f32,
    focus: Focus,
}

impl Extras {
    /// The `quests` with no map slot (`book`'s), by title
    /// ([`rules::order`]), each from a mod folder named by its mod in
    /// `install`'s library and checked for its files.
    pub fn of(quests: &[Offered], book: &[art::Quest], install: &Install) -> Extras {
        let extra: Vec<(usize, &Offered)> = quests
            .iter()
            .enumerate()
            .filter(|(i, _)| book.get(*i).is_some_and(|q| q.slot.is_none()))
            .collect();
        let titles: Vec<&str> = extra.iter().map(|(_, o)| o.title.as_str()).collect();
        let files = dataset::Install::new(&install.hd, Bytecode::Sdk(&install.sdk));
        let listed = rules::order(&titles)
            .into_iter()
            .map(|at| {
                let (quest, offered) = extra[at];

                Listed {
                    quest,
                    title: offered.title.clone(),
                    source: source(&install.mods(), &offered.origin),
                    refused: refused(&files, offered),
                }
            })
            .collect();
        let mut extras = Extras {
            listed,
            ..Extras::default()
        };

        extras.open();
        extras
    }

    /// Whether offered quest `quest` is listed.
    #[cfg(test)]
    pub fn lists(&self, quest: usize) -> bool {
        self.listed.iter().any(|l| l.quest == quest)
    }

    /// As the list opens: its first row lit (list message `0x1b`), at
    /// its top.
    pub fn open(&mut self) {
        self.lit = (!self.listed.is_empty()).then_some(0);
        self.scroll = 0.0;
        self.focus = Focus::None;
    }

    /// `key` to the list, `shown` rows tall, where focused: the lit row
    /// and the scroll it leaves ([`listbox::key`]).
    fn take(&mut self, key: i32, shown: usize) -> Taken {
        let texts: Vec<&str> = self.listed.iter().map(|l| l.title.as_str()).collect();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a list's rows"
        )]
        let place = Place {
            lit: self.lit,
            top: self.scroll.round() as usize,
        };
        let moved = (self.focus == Focus::List)
            .then(|| listbox::key(place, key, (&texts, shown)))
            .flatten();

        match moved {
            Some(p) => {
                #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
                let top = p.top as f32;

                self.lit = p.lit;
                self.scroll = top;
                Taken::Yes
            }
            None => Taken::No,
        }
    }

    /// Offered quest `quest`'s row lit and shown: `--menu extras:KEY`.
    pub fn light(&mut self, quest: usize) {
        if let Some(row) = self.listed.iter().position(|l| l.quest == quest) {
            self.lit = Some(row);
            #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
            let top = row as f32;

            self.scroll = top;
        }
    }

    /// The lit row's quest, as LOAD QUEST presses it.
    fn chosen(&self) -> Option<Press> {
        self.lit
            .and_then(|row| self.listed.get(row))
            .map(|l| Press::Quest(l.quest))
    }
}

/// What a quest from `origin` names as its mod: none for the install's.
fn source(library: &Library, origin: &Origin) -> Option<String> {
    match origin {
        Origin::Install => None,
        Origin::Mod(package) => Some(rules::source(library, package)),
    }
}

/// Why `offered`'s game will not start, as starting it would say: a
/// file it lists not found, named from its folder.
fn refused(files: &dataset::Install<'_>, offered: &Offered) -> Option<String> {
    let dir = &offered.quest.dir;

    files
        .quest_with(&offered.quest, &[])
        .err()
        .map(|e| match e {
            dataset::Error::Missing(path) => format!(
                "{}: not found",
                path.strip_prefix(dir).unwrap_or(&path).display()
            ),
            other => other.to_string(),
        })
}

/// The list over the faces, centred at the menus' scale: the quest
/// LOAD QUEST, Enter or a row's double click chose, or CANCEL.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    extras: &mut Extras,
    (quests, progress): (&[Offered], &Progress),
) -> Option<Press> {
    let Art { book, ink, .. } = art;
    let layout = book.extras.as_ref()?;
    let mut canvas = Canvas::new(ctx, (ink, shared), paint::size(layout));

    canvas.backdrop();
    canvas.panel_art(Pack::Base, layout);

    let pressed = controls(&mut canvas, layout, extras, (quests, progress, &book.words));
    let keyed = keyed(ctx, layout, extras);

    match pressed.or(keyed)? {
        LOAD => extras.chosen(),
        _ => Some(Press::Button(Button::Cancel)),
    }
}

/// `layout`'s controls in panel order: the buttons, the list and its
/// bar, the description, the rest as pictures and headings. The button
/// pressed, or LOAD QUEST by a row's double click.
fn controls(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    extras: &mut Extras,
    (quests, progress, words): (&[Offered], &Progress, &Words),
) -> Option<i32> {
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

                if rows(canvas, c, extras) {
                    pressed = Some(LOAD);
                }
            }
            ids::SCROLL_BAR => bar(canvas, layout, c, extras),
            DESCRIPTION => {
                let text = extras
                    .lit
                    .and_then(|row| extras.listed.get(row))
                    .and_then(|l| described(l, quests.get(l.quest)?, (progress, words), c))
                    .unwrap_or_default();

                ground(canvas, c);
                canvas.held_note(c, &text);
            }
            // Each picture inside its own rectangle, as the editor's.
            _ => {
                canvas.clip = c.rect.map(fit::rect);
                boxed(canvas, c, c.text.as_deref());
                canvas.clip = None;
            }
        }
    }

    pressed
}

/// The keys typed, each to the focused list first ([`Extras::take`]);
/// the rest press a button: Enter LOAD QUEST, Escape CANCEL.
fn keyed(ctx: &egui::Context, layout: &Layout, extras: &mut Extras) -> Option<i32> {
    let offered: Vec<(&ShownControl, hotkey::Able)> = [LOAD, CANCEL]
        .iter()
        .filter_map(|&id| Some((layout.control(id)?, hotkey::Able::Enabled)))
        .collect();
    let shown = layout
        .control(LIST)
        .and_then(|c| c.rect)
        .map_or(0, shown_rows);

    paint::cursor_keys(ctx)
        .into_iter()
        .chain(paint::keys(ctx))
        .filter(|&key| extras.take(key, shown) == Taken::No)
        .find_map(|key| hotkey::pick(offered.iter().copied(), key))
        .and_then(|c| c.id)
}

/// The lit quest's description in box `c`: its line as the original
/// gives it (`GMTX` 136, or 137 once won), why it will not start in red
/// under it, then its `<Short>`.
fn described(
    listed: &Listed,
    offered: &Offered,
    (progress, words): (&Progress, &Words),
    c: &ShownControl,
) -> Option<String> {
    let quest = &offered.quest;
    let difficulty = words.gmtx(quest_map::difficulty_text(quest.difficulty))?;
    let line = quest_map::open_line(&|i| words.gmtx(i), &difficulty, progress.record(quest.key))?;
    let back = markup(paint::pick(c, State::Normal));
    let line = match &listed.refused {
        Some(why) => format!("{line}\r\n{UNAVAILABLE_MARK}{why}{back}"),
        None => line,
    };

    Some(rules::description(&line, &quest.short))
}

/// The titles from the first shown, one a row down list `c`, the lit
/// row's band behind, a mod folder's quest's mod flush right, red where
/// it will not start; a row pressed is lit. Whether one was double
/// clicked.
fn rows(canvas: &mut Canvas<'_>, c: &ShownControl, extras: &mut Extras) -> bool {
    let Some(r) = c.rect else { return false };
    let colour = paint::pick(c, State::Normal);
    let count = shown_rows(r);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = (extras.scroll.round() as usize).min(extras.listed.len().saturating_sub(count));
    let mut doubled = false;

    for (row, n) in (first..extras.listed.len()).take(count).enumerate() {
        let row = i32::try_from(row).unwrap_or_default();
        let band = smnu::Rect {
            y: r.y + row * ROW_HEIGHT,
            height: ROW_HEIGHT,
            ..r
        };
        let at = canvas.fit.rect(fit::rect(band));
        let response =
            canvas
                .ui
                .interact(at, canvas.ui.id().with(("extras row", n)), Sense::click());

        // A press lights the row and focuses the list.
        if response.clicked() {
            extras.lit = Some(n);
            extras.focus = Focus::List;
        }

        doubled |= response.double_clicked();

        // The last row cut at the list's bottom.
        if extras.lit == Some(n) {
            let list = canvas.fit.rect(fit::rect(r));

            canvas
                .painter()
                .with_clip_rect(list)
                .rect_filled(at, 0.0, LIT);
        }

        let listed = &extras.listed[n];
        let text = ShownControl {
            rect: Some(smnu::Rect {
                x: band.x + NAME_INDENT,
                width: band.width - NAME_INDENT,
                ..band
            }),
            ..c.clone()
        };

        // The title cut short of its mod's name.
        let font = c.font.and_then(|f| canvas.ink.font(f));
        let measure = |s: &str| font.map_or(0.0, |f| f.width(canvas.ui.ctx(), canvas.fit.scale, s));
        let source = listed
            .source
            .as_deref()
            .map_or(0.0, |s| measure(s) + SOURCE_GAP);
        #[allow(clippy::cast_precision_loss, reason = "a row's width")]
        let room = (band.width - NAME_INDENT) as f32 - SIDE_MARGIN - source;
        let title = cut((&listed.title, colour), room, &measure);

        line(canvas, &text, (&title, colour));

        if let Some(source) = &listed.source {
            // `UNAVAILABLE_MARK`'s red.
            let tint = if listed.refused.is_some() {
                Color32::RED
            } else {
                SOURCE
            };
            let right = fit::rect(band).max.x - SIDE_MARGIN;

            flush_right(canvas, (c, band), (source, tint), right);
        }
    }

    doubled
}

/// `text` in `colour`, cut with "…" to `width` by `measure` where wider,
/// its colours kept as markup: "Original Quest - Wrath Of Krolm" to
/// "Original Quest - Wra…".
fn cut((text, colour): (&str, Color32), width: f32, measure: &dyn Fn(&str) -> f32) -> String {
    if measure(text) <= width {
        text.to_owned()
    } else {
        font::ellipsised(&font::spans(text, colour), width, measure)
            .iter()
            .flat_map(|(c, s)| [markup(*c), s.clone()])
            .collect()
    }
}

/// The markup that colours the text after it `c`.
fn markup(c: Color32) -> String {
    format!("\u{1}{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// The list's scroll bar `c`, a row a step, and the wheel over the list.
fn bar(canvas: &mut Canvas<'_>, layout: &Layout, c: &ShownControl, extras: &mut Extras) {
    if let Some(list) = layout.control(LIST).and_then(|l| l.rect) {
        canvas.list_bar(
            Pack::Base,
            (c, Some(list)),
            (&mut extras.scroll, extras.listed.len(), shown_rows(list)),
            "extras list",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list takes keys once a row press focused it: Down lights the
    /// next row; before, Down is left to the hot keys.
    #[test]
    fn a_pressed_list_takes_the_arrow_keys() {
        let listed = ["The Wild", "The Wrath of Krolm", "Vampiric Revenge"]
            .iter()
            .enumerate()
            .map(|(quest, title)| Listed {
                quest,
                title: (*title).to_owned(),
                source: None,
                refused: None,
            })
            .collect();
        let mut extras = Extras {
            listed,
            ..Extras::default()
        };

        extras.open();

        assert_eq!(extras.take(hotkey::DOWN, 13), Taken::No);
        assert_eq!(extras.lit, Some(0));

        extras.focus = Focus::List;

        assert_eq!(extras.take(hotkey::DOWN, 13), Taken::Yes);
        assert_eq!(extras.lit, Some(1));
        assert_eq!(extras.take(hotkey::ENTER, 13), Taken::No);
    }

    /// A title that fits stays whole; a longer one ends in "…" within
    /// the room, its colour kept.
    #[test]
    fn a_long_title_is_cut_before_its_mods_name() {
        #[allow(clippy::cast_precision_loss, reason = "a short text")]
        let measure = |s: &str| s.chars().count() as f32;

        assert_eq!(cut(("The Wild", Color32::WHITE), 9.0, &measure), "The Wild");
        assert_eq!(
            cut(("Original Quest", Color32::WHITE), 9.0, &measure),
            "\u{1}FFFFFFOriginal…"
        );
    }

    /// A Workshop quest whose files are there starts; one listing an
    /// image not on disk says which, from its folder.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_quest_missing_a_file_says_which() {
        let install = Install::from_env().expect("MAJESTY_HD");
        let files = dataset::Install::new(&install.hd, Bytecode::Sdk(&install.sdk));
        let library = install.mods();
        let found = library
            .quests
            .iter()
            .find(|f| f.quest.name == "BigMap")
            .expect("Big Map");
        let mut offered = Offered {
            title: "13 - Big Map".to_owned(),
            quest: found.quest.clone(),
            origin: Origin::Mod(found.package.clone()),
        };

        assert_eq!(refused(&files, &offered), None);

        offered.quest.programs = vec![(None, "Data\\Gone.bcd".to_owned())];

        assert_eq!(
            refused(&files, &offered).as_deref(),
            Some("Data/Gone.bcd: not found")
        );
    }
}
