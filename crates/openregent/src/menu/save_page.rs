//! The save and load dialogs drawn (`save::dialog`): `AP49` and `AP27` from the
//! base game's art, as `preset_page` draws `MX20` and `MX21`, and `AP11`
//! over them asking DELETE GAME's question, or SAVE GAME's on a name
//! taken, or telling why a choice did nothing.
//!
//! ```text
//!  AP27  ┌(pog)── Single Player Load Menu ──┐
//!        │      [LIST OF SAVED GAMES]       │
//!        │ ┌──────────────────────────────┐^│
//!        │ │ (Freestyle Game) 0 Days      │#│
//!        │ │ (Freestyle Game) 4 Days  lit │v│
//!        │ [map] [GAME DESCRIPTION]  [    ] │   the lit save's minimap
//!        │ (LOAD GAME) (DELETE GAME) (CANCEL)   in the left side box
//!        └──────────────────────────────────┘
//! ```

use bevy_egui::egui::{self, Color32, Id, LayerId, Order, Rect, Sense, UiBuilder, pos2};
use regent_core::data::smnu;
use regent_core::view::menu::hotkey;
use regent_core::view::menu::message::{self, Ask};
use regent_core::view::menu::saves::{self as rules, Mode};
use regent_core::view::menu::{Layout, ShownControl, State, ids};

use super::art::{Art, Pack, Words};
use super::fit::{self, Fit};
use super::font;
use super::paint::{self, Able, Canvas, Inset};
use super::setup::{boxed, ground, line};
use crate::save;
use crate::save::dialog::{Answer, Asked, Chose, Focus, Saves, Taken};
use regent_draw::art as service;

/// How far in from the list's left a name starts. **Game**:
/// `ui_load_game.png`, 19 px.
pub(super) const NAME_INDENT: i32 = 19;

/// A row's side text: its margin from the list's right, and the gap
/// between its columns and the name. Not the original's: its rows hold
/// the name alone.
pub(super) const SIDE_MARGIN: f32 = 8.0;
const COLUMN_GAP: f32 = 16.0;

/// The lit row's band. **Game**: `ui_load_game.png`, `#002030`, the
/// list's whole width.
pub(super) const LIT: Color32 = Color32::from_rgb(0x00, 0x20, 0x30);

/// `AP27`'s left side box, inside its frame: the lit save's minimap goes
/// there. **Inferred** from `ui_load_game.png`: the frame drawn in the
/// panel's picture, `INDo` 1000, at (32, 421) to (164, 519).
const MAP_BOX: smnu::Rect = smnu::Rect {
    x: 34,
    y: 423,
    width: 128,
    height: 94,
};

/// The minimap's margin inside [`MAP_BOX`].
const MAP_MARGIN: f32 = 4.0;

/// The buttons, each on one dialog or both.
const BUTTONS: [i32; 4] = [rules::LOAD, rules::SAVE, rules::DELETE, rules::CANCEL];

/// The open dialog over the main menu, the lobby or the game, on a layer
/// of its own at the menus' scale: what the player chose, if anything.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    saves: &mut Saves,
) -> Option<Chose> {
    let dialog = saves.open?;
    let mode = dialog.mode();
    let Art { book, ink, .. } = art;
    let layout = match mode {
        Mode::Load => book.load_game.as_ref()?,
        Mode::Save => book.save_game.as_ref()?,
    };
    let words = &book.words;
    let size = paint::size(layout);
    let fit = Fit::page(ctx.viewport_rect(), size);
    let mut canvas = Canvas::at(above(ctx), (ink, shared), fit);
    let window = canvas.ui.max_rect();

    canvas.block(window);

    canvas.panel_art(Pack::Base, layout);

    let pressed = controls(&mut canvas, layout, saves, words);

    if mode == Mode::Load {
        minimap(&mut canvas, saves);
    }

    // A question or a note over the dialog takes the presses and keys.
    match (&saves.asking, saves.said.clone()) {
        (Some(asked), _) => {
            let words = match asked {
                Asked::Delete(_) => (
                    words.gmtx(rules::DELETE_TITLE.index),
                    words.gmtx(rules::DELETE_TEXT.index),
                ),
                Asked::Overwrite(_) => (
                    Some(save::dialog::OVERWRITE_TITLE.to_owned()),
                    Some(save::dialog::OVERWRITE_TEXT.to_owned()),
                ),
            };

            match told(&mut canvas, book.message.as_ref()?, words, Ask::Question) {
                Some(message::YES) => saves.answer(Answer::Yes),
                Some(message::NO) => saves.answer(Answer::No),
                _ => None,
            }
        }
        (None, Some(why)) => {
            let note = (None, Some(why));

            if told(&mut canvas, book.message.as_ref()?, note, Ask::Tell).is_some() {
                saves.said = None;
            }

            None
        }
        (None, None) => {
            let shown = layout.control(rules::LIST).and_then(|c| c.rect);

            pressed
                .or_else(|| keyed(ctx, mode, (saves, shown.map_or(0, rules::rows))))
                .and_then(|id| saves.pressed(id))
        }
    }
}

/// The dialog's ui: a layer over the menus' pages and the game's.
fn above(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        Id::new("saves"),
        UiBuilder::new()
            .layer_id(LayerId::new(Order::Foreground, Id::new("saves")))
            .max_rect(ctx.viewport_rect()),
    )
}

/// `layout`'s controls in panel order: the buttons, the name box, the
/// list and its bar, the descriptions, the rest as pictures and
/// headings. The button pressed, or the one a row's double click
/// presses.
fn controls(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    saves: &mut Saves,
    words: &Words,
) -> Option<i32> {
    let mode = saves.open.map_or(Mode::Load, save::dialog::Dialog::mode);
    let (gmtx, month) = (|i| words.gmtx(i), |i| words.month(i));
    let strings: (&save::dialog::Strings<'_>, &save::dialog::Strings<'_>) = (&gmtx, &month);
    let mut pressed = None;

    for c in &layout.controls {
        match c.id.unwrap_or_default() {
            id @ (rules::LOAD | rules::SAVE | rules::DELETE | rules::CANCEL) => {
                if canvas.button(Pack::Base, c, Able::Enabled) {
                    pressed = Some(id);
                }
            }
            rules::NAME => {
                ground(canvas, c);
                name(canvas, c, &mut saves.name);
            }
            rules::LIST => {
                ground(canvas, c);

                if rows(canvas, c, saves, words) {
                    pressed = Some(mode.confirm());
                }
            }
            ids::SCROLL_BAR => bar(canvas, layout, c, saves),
            rules::DESCRIPTION => {
                let normal = paint::pick(c, State::Normal);
                let room = c.rect.map(fit::rect);
                let fits = |line: &str| room.is_none_or(|r| canvas.fits(c.font, r, line));
                let marking: save::dialog::Marking<'_> = (
                    saves.found.as_slice(),
                    [normal.r(), normal.g(), normal.b()],
                    &fits,
                );
                let text = saves
                    .lit_save()
                    .map(|l| save::dialog::description(l, strings, marking))
                    .unwrap_or_default();

                ground(canvas, c);
                canvas.note(c, &text);
            }
            rules::CURRENT => {
                let text = saves
                    .here
                    .as_ref()
                    .map(|h| save::dialog::current(h, strings))
                    .unwrap_or_default();

                ground(canvas, c);
                canvas.note(c, &text);
            }
            rules::POG => match (saves.pog, c.rect) {
                (Some((source, pic)), Some(r)) => {
                    canvas.picture(source, pic, fit::rect(r).min);
                }
                _ => boxed(canvas, c, None),
            },
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

/// The keys typed, each to the focused list first (`Saves::take`) while
/// the name box has none; the rest press a button: Enter SAVE GAME or
/// LOAD GAME, Escape CANCEL.
fn keyed(ctx: &egui::Context, mode: Mode, (saves, shown): (&mut Saves, usize)) -> Option<i32> {
    let typing = ctx.egui_wants_keyboard_input();

    paint::cursor_keys(ctx)
        .into_iter()
        .chain(paint::keys(ctx))
        .filter(|&key| typing || saves.take(key, shown) == Taken::No)
        .find_map(|key| match key {
            hotkey::ENTER => Some(mode.confirm()),
            hotkey::ESCAPE => Some(rules::CANCEL),
            _ => None,
        })
        .filter(|id| BUTTONS.contains(id))
}

/// The saves' names from the first shown, one a row down list `c`, the
/// lit row's band behind; a row pressed is lit. Whether one was double
/// clicked.
fn rows(canvas: &mut Canvas<'_>, c: &ShownControl, saves: &mut Saves, words: &Words) -> bool {
    let Some(r) = c.rect else { return false };
    let colour = paint::pick(c, State::Normal);
    let names = saves.names();
    let gmtx = |i| words.gmtx(i);
    let sides: Vec<Option<[String; 2]>> = saves
        .listed
        .iter()
        .map(|l| save::dialog::side(l, &gmtx))
        .collect();
    let (columns, room) = side_columns(canvas, c, &sides);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = (saves.scroll.round() as usize).min(names.len().saturating_sub(rules::rows(r)));
    let mut doubled = false;

    for (row, (n, named)) in names
        .iter()
        .enumerate()
        .skip(first)
        .take(rules::rows(r))
        .enumerate()
    {
        let row = i32::try_from(row).unwrap_or_default();
        let band = smnu::Rect {
            y: r.y + row * rules::ROW_HEIGHT,
            height: rules::ROW_HEIGHT,
            ..r
        };
        let at = canvas.fit.rect(fit::rect(band));
        let response = canvas
            .ui
            .interact(at, canvas.ui.id().with(("save row", n)), Sense::click());

        // A press lights the row and focuses the list.
        if response.clicked() {
            saves.light(n);
            saves.focus = Focus::List;
        }

        doubled |= response.double_clicked();

        // The last row cut at the list's bottom.
        if saves.lit == Some(n) {
            let list = canvas.fit.rect(fit::rect(r));

            canvas
                .painter()
                .with_clip_rect(list)
                .rect_filled(at, 0.0, LIT);
        }

        let text = ShownControl {
            rect: Some(smnu::Rect {
                x: band.x + NAME_INDENT,
                width: band.width - NAME_INDENT - room,
                ..band
            }),
            ..c.clone()
        };

        line(canvas, &text, (named, colour));

        // The save's time and days, each flush right in its column.
        if let Some(Some([stamp, days])) = sides.get(n) {
            let right = fit::rect(band).max.x - SIDE_MARGIN;

            flush_right(canvas, (c, band), (days, colour), right);
            flush_right(
                canvas,
                (c, band),
                (stamp, colour),
                right - columns[1] - COLUMN_GAP,
            );
        }
    }

    doubled
}

/// The widest of `sides`' each column, layout pixels, in list `c`'s
/// font; and the room they take from a row's right, margins and gaps
/// in: none with no side text.
fn side_columns(
    canvas: &Canvas<'_>,
    c: &ShownControl,
    sides: &[Option<[String; 2]>],
) -> ([f32; 2], i32) {
    let font = c.font.and_then(|f| canvas.ink.font(f));
    let wide = |text: &str| font.map_or(0.0, |f| f.width(canvas.ui.ctx(), canvas.fit.scale, text));
    let columns = sides.iter().flatten().fold([0.0_f32; 2], |[a, b], [s, d]| {
        [a.max(wide(s)), b.max(wide(d))]
    });
    let any = sides.iter().any(Option::is_some);
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a row's width, layout pixels"
    )]
    let room = (columns[0] + columns[1] + 2.0 * COLUMN_GAP + SIDE_MARGIN).ceil() as i32;

    (columns, if any { room } else { 0 })
}

/// `text` on one line in list `c`'s font, ending at layout x `right`,
/// on row `band`'s baseline as `line` sets a name.
pub(super) fn flush_right(
    canvas: &mut Canvas<'_>,
    (c, band): (&ShownControl, smnu::Rect),
    (text, colour): (&str, Color32),
    right: f32,
) {
    if let Some(font) = c.font.and_then(|f| canvas.ink.font(f)) {
        let rect = fit::rect(band);
        let wide = font.width(canvas.ui.ctx(), canvas.fit.scale, text);
        let pen = pos2(
            right - wide,
            rect.center().y + font.cap(canvas.fit.scale) / 2.0,
        );
        let painter = canvas.painter().with_clip_rect(canvas.fit.rect(rect));

        font.draw(&painter, canvas.fit, pen, &font::spans(text, colour));
    }
}

/// The list's scroll bar `c`, a row a step, and the wheel over the list.
fn bar(canvas: &mut Canvas<'_>, layout: &Layout, c: &ShownControl, saves: &mut Saves) {
    if let Some(list) = layout.control(rules::LIST).and_then(|l| l.rect) {
        canvas.list_bar(
            Pack::Base,
            (c, Some(list)),
            (&mut saves.scroll, saves.listed.len(), rules::rows(list)),
            "save list",
        );
    }
}

/// The name typed in box `c`, at most the original's length.
fn name(canvas: &mut Canvas<'_>, c: &ShownControl, typed: &mut String) {
    if let Some(r) = c.rect {
        canvas.typed(
            (fit::rect(r), c.font, paint::pick(c, State::Normal)),
            (typed, rules::NAME_CHARS, egui::Align::Min),
            ("save name", Inset::Sides),
        );
    }
}

/// The lit save's minimap in `AP27`'s left side box, square, its
/// texture made once.
fn minimap(canvas: &mut Canvas<'_>, saves: &mut Saves) {
    let Some(texture) = saves.thumb(canvas.ui.ctx()) else {
        return;
    };
    let room = fit::rect(MAP_BOX).shrink(MAP_MARGIN);
    let side = room.width().min(room.height());
    let square = Rect::from_center_size(room.center(), egui::Vec2::splat(side));
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

    canvas
        .painter()
        .image(texture, canvas.fit.rect(square), uv, Color32::WHITE);
}

/// `AP11` centred over the dialog at its scale: its frame, `title` (else
/// its own), `text`, and the buttons `ask` shows; the one pressed, by a
/// click or its hot key.
fn told(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (title, text): (Option<String>, Option<String>),
    ask: Ask,
) -> Option<i32> {
    let window = canvas.ui.max_rect();
    let kept = canvas.fit;
    let shown = message::shown(ask);

    canvas.fit = Fit::centred(window, kept.scale, paint::size(layout));
    canvas.block(window);

    canvas.panel_art(Pack::Base, layout);

    if let Some(c) = layout.control(message::TITLE) {
        let title = title.or_else(|| c.text.clone()).unwrap_or_default();

        canvas.label(c, &title, paint::pick(c, State::Normal));
    }

    if let (Some(c), Some(text)) = (layout.control(message::TEXT), text) {
        canvas.note(c, &text);
    }

    let clicked = shown.iter().fold(None, |hit, &id| {
        let now = layout
            .control(id)
            .is_some_and(|c| canvas.button(Pack::Base, c, Able::Enabled));

        hit.or(now.then_some(id))
    });
    let offered: Vec<(&ShownControl, hotkey::Able)> = shown
        .iter()
        .filter_map(|&id| Some((layout.control(id)?, hotkey::Able::Enabled)))
        .collect();
    let keyed = paint::keys(canvas.ui.ctx())
        .into_iter()
        .find_map(|key| hotkey::pick(offered.iter().copied(), key))
        .and_then(|c| c.id);

    canvas.fit = kept;
    clicked.or(keyed)
}
