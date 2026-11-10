//! The Build Tree Editor page, `MX18`, from the setup page's BUILDING
//! EDIT MENU, in its own art as the setup page: its rules are the
//! library's (`regent_core::view::menu::tree`).
//!
//! ```text
//!  ┌(pog)──────────── Build Tree Editor ──────────────────┐
//!  │ SAVE AS: [name     ] [SET ALL] [REMOVE ALL] ┌──────┐ │
//!  │ Blacksmith Level 2          [x]            ^│ IX22 │ │
//!  │ Dwarven Settlement          [x][+][-][ 3 ] #│ 1003 │ │
//!  │ ...  20 rows                               v│      │ │
//!  │ (SAVE)            (DELETE)            (CANCEL)       │
//!  └──────────────────────────────────────────────────────┘
//! ```
//!
//! It edits a copy of the setup's tree. SAVE keeps the copy under the
//! name typed (`trees::save`) and makes it the setup's pick; DELETE
//! removes the player's tree of that name; either refused shows the
//! original's message (`AP11`). CANCEL or Escape keeps nothing.

use bevy_egui::egui::{self, Color32};
use regent_core::data::build_tree::Tree;
use regent_core::data::smnu;
use regent_core::view::menu::{ShownControl, State, Tick, ids, tree};

use super::art::{Art, Pack};
use super::fit;
use super::freestyle::Custom;
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas, Inset};
use super::setup::{boxed, ground, line};
use crate::files::trees::{self, Refused};
use regent_draw::art as service;

/// How far in from the list's left a name starts. **Inferred** from
/// `ui_freestyle_building_limitations_setup.png`: 18 px.
const NAME_INDENT: i32 = 15;

/// A row's height: the rows' controls stand 20 px apart.
const ROW_HEIGHT: i32 = 20;

/// A count's colour, the box naming none: the capture's orange, as the
/// setup page's headings. **Inferred** from the capture.
const COUNT: Color32 = Color32::from_rgb(0xf6, 0xad, 0x52);

/// The longest name typed: what the writer keeps less its NUL
/// (`build_tree::NAME_ROOM`).
const NAME_CHARS: usize = regent_core::data::build_tree::NAME_ROOM - 1;

/// The page's state between passes.
#[derive(Debug, Default)]
pub struct Editor {
    /// The tree edited.
    tree: Tree,
    /// The name as typed.
    name: String,
    /// The first row shown.
    scroll: f32,
    /// A SAVE or DELETE refused: the message shows.
    refused: bool,
}

impl Editor {
    /// Opening on `custom`'s tree.
    #[must_use]
    pub fn of(custom: &Custom) -> Editor {
        Editor {
            tree: Tree {
                name: custom.buildings.clone(),
                caps: custom.caps.clone(),
            },
            name: custom.buildings.clone(),
            scroll: 0.0,
            refused: false,
        }
    }
}

/// What a button of the page asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asked {
    Save,
    Delete,
    Close,
    SetAll,
    RemoveAll,
}

/// The page: `custom`'s tree edited; Cancel once closed.
pub fn page(
    ctx: &egui::Context,
    (art, shared, editor): (&mut Art, &mut service::Art, &mut Editor),
    custom: &mut Custom,
) -> Option<Press> {
    let asked = {
        let Art {
            book, ink, lists, ..
        } = &mut *art;
        let layout = book.tree.as_ref()?;
        let size = paint::size(layout);
        let mut canvas = Canvas::filling(ctx, (ink, shared), size);
        let mut asked = None;

        canvas.backdrop();

        canvas.panel_art(Pack::Expansion, layout);

        let rows = &lists.rows;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a list's rows"
        )]
        let first = editor.scroll.round() as usize;

        for c in &layout.controls {
            let id = c.id.unwrap_or_default();
            let row = row_of(id).map(|(part, n)| (part, rows.rows.get(first + n)));

            match (id, row) {
                (_, _) if tree::CHAT.contains(&id) => {}
                (_, Some((part, Some(row)))) => {
                    let c = &ShownControl {
                        font: c.font.or(layout.panel.font),
                        ..c.clone()
                    };

                    one_row(&mut canvas, c, (part, row), (&mut editor.tree, rows));
                }
                (_, Some((_, None))) => {}
                (
                    tree::SAVE | tree::DELETE | tree::CANCEL | tree::SET_ALL | tree::REMOVE_ALL,
                    _,
                ) => {
                    if canvas.button(Pack::Expansion, c, Able::Enabled) {
                        asked = asked_by(id);
                    }
                }
                (tree::NAME, _) => {
                    ground(&mut canvas, c);
                    name(&mut canvas, c, &mut editor.name);
                }
                (tree::LIST, _) => {
                    ground(&mut canvas, c);
                    names(&mut canvas, c, (rows, first));
                }
                (ids::SCROLL_BAR, _) => {
                    let list = layout.control(tree::LIST).and_then(|l| l.rect);

                    canvas.list_bar(
                        Pack::Expansion,
                        (c, list),
                        (&mut editor.scroll, rows.rows.len(), tree::ROWS),
                        "tree list",
                    );
                }
                // Each picture inside its own rectangle: the rails, 3 and
                // 5, drawn whole, would show a cap mid-rail. **Inferred**
                // from the capture, which shows none.
                _ => {
                    canvas.clip = c.rect.map(fit::rect);
                    boxed(&mut canvas, c, c.text.as_deref());
                    canvas.clip = None;
                }
            }
        }

        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));

        asked.or((escape && !editor.refused).then_some(Asked::Close))
    };

    if editor.refused {
        let words = &art.book.words;
        let said = (
            words.gmtx(tree::REFUSED_TITLE),
            words.gmtx(tree::REFUSED_TEXT),
        );

        if paint::message(
            ctx,
            (art, shared),
            said,
            regent_core::view::menu::message::Ask::Tell,
        )
        .is_some()
        {
            editor.refused = false;
        }

        None
    } else {
        asked.and_then(|asked| done(asked, editor, (custom, art)))
    }
}

/// The button `id` is.
const fn asked_by(id: i32) -> Option<Asked> {
    match id {
        tree::SAVE => Some(Asked::Save),
        tree::DELETE => Some(Asked::Delete),
        tree::CANCEL => Some(Asked::Close),
        tree::SET_ALL => Some(Asked::SetAll),
        tree::REMOVE_ALL => Some(Asked::RemoveAll),
        _ => None,
    }
}

/// `asked` carried out: Cancel where the page closes.
fn done(
    asked: Asked,
    editor: &mut Editor,
    (custom, art): (&mut Custom, &mut Art),
) -> Option<Press> {
    let lists = &mut art.lists;
    let place = trees::place();
    let close = Some(Press::Button(Button::Cancel));
    let refused = |editor: &mut Editor, why: &Refused| {
        if let Refused::Unwritten(why) = why {
            bevy::log::warn!("{why}");
        }

        editor.refused = true;
        None
    };

    match asked {
        Asked::SetAll => {
            lists.rows.set_all(&mut editor.tree);
            None
        }
        Asked::RemoveAll => {
            lists.rows.remove_all(&mut editor.tree);
            None
        }
        Asked::Close => close,
        Asked::Save => {
            editor.tree.name = editor.name.trim().to_owned();

            let saved = place
                .as_deref()
                .ok_or_else(|| Refused::Unwritten("no data directory".to_owned()))
                .and_then(|dir| trees::save(dir, &lists.trees.shipped, &editor.tree));

            match saved {
                Ok(()) => {
                    lists.reread();
                    custom.buildings.clone_from(&editor.tree.name);
                    custom.caps.clone_from(&editor.tree.caps);
                    close
                }
                Err(why) => refused(editor, &why),
            }
        }
        Asked::Delete => {
            let name = editor.name.trim();
            let deleted = place
                .as_deref()
                .ok_or(Refused::Reserved)
                .and_then(|dir| trees::delete(dir, &lists.trees.shipped, name));

            match deleted {
                Ok(()) => {
                    lists.reread();
                    custom.resolve(&lists.trees);
                    close
                }
                Err(why) => refused(editor, &why),
            }
        }
    }
}

/// Which part of which shown row control `id` is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Box,
    Plus,
    Minus,
    Count,
}

/// Control `id`'s part and row, if a row's.
fn row_of(id: i32) -> Option<(Part, usize)> {
    [
        (tree::BOX, Part::Box),
        (tree::PLUS, Part::Plus),
        (tree::MINUS, Part::Minus),
        (tree::COUNT, Part::Count),
    ]
    .into_iter()
    .find_map(|(first, part)| {
        let n = usize::try_from(id - first)
            .ok()
            .filter(|n| *n < tree::ROWS)?;

        Some((part, n))
    })
}

/// One part of a shown row, pressed carried out.
fn one_row(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (part, row): (Part, &tree::Row),
    (edited, rows): (&mut Tree, &tree::Rows),
) {
    let cap = edited.caps.get(&row.code).copied();
    let able = |on: bool| if on { Able::Enabled } else { Able::Disabled };

    match part {
        Part::Box => {
            let tick = if cap.is_some() {
                Tick::Checked
            } else {
                Tick::Clear
            };
            let on = able(rows.able(edited, row.code));

            if canvas.ticked(Pack::Expansion, c, (on, tick)) {
                tree::toggle(edited, row.code);
            }
        }
        Part::Plus | Part::Minus if row.counted => {
            if canvas.button(Pack::Expansion, c, able(cap.is_some())) {
                match part {
                    Part::Plus => tree::raise(edited, row.code),
                    _ => tree::lower(edited, row.code),
                }
            }
        }
        Part::Count if row.counted => {
            ground(canvas, c);

            if let Some(cap) = cap {
                line(canvas, c, (&cap.to_string(), COUNT));
            }
        }
        _ => {}
    }
}

/// The rows' names from `first`, one a row down list `c`.
fn names(canvas: &mut Canvas<'_>, c: &ShownControl, (rows, first): (&tree::Rows, usize)) {
    let Some(r) = c.rect else { return };
    let colour = paint::pick(c, State::Normal);

    for (n, row) in rows.rows.iter().skip(first).take(tree::ROWS).enumerate() {
        let n = i32::try_from(n).unwrap_or_default();
        let at = smnu::Rect {
            x: r.x + NAME_INDENT,
            y: r.y + n * ROW_HEIGHT,
            width: r.width - NAME_INDENT,
            height: ROW_HEIGHT,
        };

        line(
            canvas,
            &ShownControl {
                rect: Some(at),
                ..c.clone()
            },
            (&row.name, colour),
        );
    }
}

/// The name typed in box `c`.
fn name(canvas: &mut Canvas<'_>, c: &ShownControl, typed: &mut String) {
    if let Some(r) = c.rect {
        canvas.typed(
            (fit::rect(r), c.font, paint::pick(c, State::Normal)),
            (typed, NAME_CHARS, egui::Align::Min),
            ("tree name", Inset::Sides),
        );
    }
}
