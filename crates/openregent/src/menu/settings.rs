//! The settings editor's page (`editor`), from the Build Tree Editor's
//! art, `MX18`: its frame cut to the page's width, list, check boxes,
//! plus and minus buttons, value boxes, scroll bar and SET ALL's
//! buttons; the setup page's combo boxes and their lists (`MX17`).
//!
//! ```text
//!  ┌(pog)── Adjust Settings ───────┐
//!  │ (SETTINGS)(ENHANCEMENTS)       │   the tabs, the shown one lit
//!  │           INTERFACE           ^│
//!  │ Bars          [damaged    v]  #│   a row a key, under its group
//!  │ Scroll speed       [+][-][32] #│
//!  │                               v│
//!  │ [Hit point bars      ] (RESET) │   the setting the doc is of
//!  │ [Which units show a hit point ]│   the doc, its default, or a refusal
//!  │   [(SAVE)(DEFAULTS)(CANCEL)]   │
//!  └────────────────────────────────┘
//! ```
//!
//! The page is as wide as its rows or tabs need, its frame `MX18`'s
//! picture cut in bands, its right side drawn in from the picture's
//! ([`BANDS`]); the bottom bar's two slots join into one. The tabs are
//! SET ALL's picture, the shown one lit as a held button (the panels'
//! lit flag), sharing the page's width, their picture cut ([`TAB_CAP`]). SETTINGS
//! holds the keys by [`Group`];
//! ENHANCEMENTS the departures from the original, by the same, under a
//! preset list, the experimental keys last behind a fold with their switch:
//!
//! ```text
//!  Preset              [Modern   v]    picks the keys below; Custom if changed
//!            DISPLAY
//!  Rings               [modern   v]
//!            GAME ...
//!  [+] Experimental             [ ]    + opens it; [ ] turns its keys on
//! ```
//!
//! The rows are this game's: a section's name, then a row a key, 25 px apart
//! so the setup page's 24 px combo box fits one. The doc shown is the
//! hovered row's, else the last hovered, else the tab's first; RESET puts
//! that key back to its default. A value box dragged across moves its
//! number through its range. Centred at the interface's scale, over the
//! main menu or the paused game.

use bevy_egui::egui::{self, Color32, Id, LayerId, Order, Rect, Sense, UiBuilder, pos2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu;
use regent_core::view::menu::colour::Inks;
use regent_core::view::menu::{self, Layout, ShownControl, State, Tick, tree};

use super::Art;
use super::art::{Pack, Pic, Store};
use super::fit::{self, Fit};
use super::paint::{self, Able, Align, Canvas};
use super::setup::{self, Listed, boxed, ground, line, list, on_top};
use crate::files::config::Preset;
pub use crate::files::setting::Tab;
use crate::files::setting::{Gate, Group, Key, Now, Show, number};
use crate::ui::editor::{Draft, Press};
use regent_draw::art as service;

/// The title, in place of "Build Tree Editor": the main menu button's
/// label.
const TITLE: &str = "Adjust Settings";

/// The buttons' texts, on SET ALL's picture: the shown key's default,
/// then the page's.
const RESET: &str = "RESET";
const SAVE: &str = "SAVE";
const DEFAULTS: &str = "DEFAULTS";
const CANCEL: &str = "CANCEL";

/// A row's height, layout pixels: this game's, the setup page's combo
/// box (24 px) and a pixel.
const ROW: i32 = 25;

/// The preset row's name and doc.
const PRESET: &str = "Preset";
const PRESET_DOC: &str = "Sets the enhancements at once. Modern: the recommended set. \
    Classic: as the original game. Custom: anything changed by hand. \
    Modern and Classic turn the experimental settings off, but keep their values.";

/// How far in from the list's left a key's name starts, as the Build
/// Tree Editor's names.
const NAME_INDENT: i32 = 15;

/// Between a name and its control, and a control and the list's right.
const GAP: i32 = 10;
const INSET: i32 = 4;

/// The list's box past the rows' controls: `MX18`'s 516 to 510.
const BOX_RIGHT: i32 = 6;

/// Between the list and its scroll bar: `MX18`'s value boxes' right
/// (510) to its bar (519).
const BAR_GAP: i32 = 9;

/// The rows' and the doc's margin from the frame's right, as the list's
/// from its left (`MX18`'s 20).
const RIGHT: i32 = 20;

/// The doc box: under the list by this, its lines this many, its text
/// this far in from each edge; its bottom the body's, over the frame's
/// bottom bar.
const DOC_GAP: i32 = 6;
const DOC_LINES: f32 = 5.0;
const DOC_PAD: i32 = 6;

/// The doc's text this much larger than the chat box's font: some 1.5
/// points at its size.
const DOC_SIZE: f32 = 1.15;
const BODY_BOTTOM: i32 = 530;

/// A tab's picture is cut to its width with end caps this wide, so its
/// corners and edges keep their size. **Inferred** from the picture: its corners are
/// some 3 px. A label has this much to spare each side.
const TAB_CAP: f32 = 6.0;
const TAB_PAD: i32 = 12;

/// A number's controls, `MX18`'s: + and - 16 wide a step of 20, then the
/// value box, 80.
const NUMBER_WIDTH: i32 = 120;
const PLUS_STEP: i32 = 20;

/// A combo box's arrow, as the setup page draws one with no picture, and
/// its text's pad either side.
const ARROW_WIDTH: i32 = 20;
const COMBO_PAD: i32 = 6;

/// The bottom bar's buttons: their top, in the joined slot, and the gap
/// between them and at its ends.
const FOOT_Y: i32 = 555;
const BUTTON_GAP: i32 = 8;

/// The joined slot's ends in a cut frame: the left slot's start, and the
/// right slot's end from the picture's right (768 of 800). **Inferred**
/// from the picture.
const SLOT_LEFT: i32 = 35;
const SLOT_RIGHT: i32 = 32;

/// The title's room: right of the pog, and its right ornaments.
const TITLE_LEFT: i32 = 94;
const TITLE_RIGHT: i32 = 30;

/// How a band of `MX18`'s picture is cut to a narrower width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    /// Its left half and right half: the title bar's two ends meet, the
    /// bottom bar's two slots join into one.
    Halves,
    /// Its left part, and its last pixels from the picture's right edge,
    /// the frame's right side, skipping the chat column left of them.
    Strip(i32),
}

/// `MX18`'s picture cut to a width, in bands from the top: the title
/// bar; the body's top corner; its right side; its bottom corner; the
/// bottom bar. **Inferred** from the picture: the bands' edges are where
/// its ornaments end.
const BANDS: [(i32, i32, Cut); 5] = [
    (0, 76, Cut::Halves),
    (76, 100, Cut::Strip(16)),
    (100, 532, Cut::Strip(10)),
    (532, 548, Cut::Strip(23)),
    (548, 600, Cut::Halves),
];

/// A value's colour: the Build Tree Editor's counts'.
const VALUE: Color32 = Color32::from_rgb(0xf6, 0xad, 0x52);

/// A shaded row's band over the list's art: black, translucent. This
/// game's, for the eye; the original's lists have none.
const SHADE: Color32 = Color32::from_black_alpha(48);

/// A refusal's colour, as the setup page's.
const REFUSED: Color32 = Color32::RED;

/// + and - step a number this share of its range.
const STEPS: f64 = 20.0;

/// The combo box the lists copy: the setup page's Landscape.
const COMBO: i32 = menu::setup::LAND;

/// Room for this many keys in a row control's id ([`own_id`]).
const ROW_IDS: i32 = 1000;

impl Tab {
    /// The tabs, left to right.
    const ALL: [Tab; 2] = [Tab::Settings, Tab::Enhancements];

    /// Its button's text.
    const fn label(self) -> &'static str {
        match self {
            Tab::Settings => "SETTINGS",
            Tab::Enhancements => "ENHANCEMENTS",
        }
    }
}

impl Group {
    /// Its place on `tab`, top first. Every group has one on each tab, so
    /// a key in any group shows.
    const fn rank(self, tab: Tab) -> u8 {
        match (tab, self) {
            (Tab::Settings, Group::Game) | (Tab::Enhancements, Group::Display) => 0,
            (Tab::Settings, Group::Sound) | (Tab::Enhancements, Group::Game) => 1,
            (Tab::Settings, Group::Display) | (Tab::Enhancements, Group::Interface) => 2,
            (Tab::Settings, Group::Interface) | (Tab::Enhancements, Group::Sound) => 3,
        }
    }

    /// Its heading's text.
    const fn label(self) -> &'static str {
        match self {
            Group::Display => "display",
            Group::Game => "game",
            Group::Interface => "interface",
            Group::Sound => "sound",
        }
    }
}

/// Whether the experimental keys' rows show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Fold {
    #[default]
    Shut,
    Open,
}

/// A list that can be open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Opened {
    /// The preset row's.
    Preset,
    /// A key's, by its place in the keys.
    Key(usize),
}

/// How the page shows, between passes.
#[derive(Clone, Debug, Default)]
pub struct Look {
    /// The tab shown.
    tab: Tab,
    /// The first row shown.
    scroll: f32,
    /// The list open, and how far it is scrolled, in rows.
    open: Option<(Opened, f32)>,
    /// The key whose doc shows: the last hovered; `None` over the preset.
    shown: Option<usize>,
    /// Whether the experimental keys show.
    fold: Fold,
    /// The doc's scroll, pixels.
    doc_scroll: f32,
    /// The number a value box held as its drag began.
    drag: Option<f64>,
}

impl Look {
    /// Shown on `tab`.
    #[must_use]
    pub fn on(tab: Tab) -> Look {
        Look {
            tab,
            ..Look::default()
        }
    }

    /// `tab` shown, from its top, no list open.
    fn turn(&mut self, tab: Tab) {
        *self = Look::on(tab);
    }
}

/// A row of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// A group's name.
    Heading(&'static str),
    /// A key, by its place in the keys.
    Key(usize),
    /// The preset list.
    Preset,
    /// The experimental switch, by its place in the keys, and its fold.
    Fold(usize),
}

/// The rows of `keys` on `tab`, each key under its group's heading; the
/// enhancements as [`grouped`] lists them. Hidden keys are not listed.
fn rows(keys: &[Key], tab: Tab, fold: Fold) -> Vec<Row> {
    let listed: Vec<usize> = (0..keys.len())
        .filter(|&i| keys[i].meta.show == Show::Page && keys[i].part.tab == tab)
        .collect();

    match tab {
        Tab::Enhancements => grouped(keys, &listed, fold),
        Tab::Settings => headed(keys, &listed, tab),
    }
}

/// `listed`'s keys, each group under its heading, the groups in `tab`'s
/// order; a group with no key has no heading.
fn headed(keys: &[Key], listed: &[usize], tab: Tab) -> Vec<Row> {
    let mut groups: Vec<Group> = listed.iter().filter_map(|&i| keys[i].meta.group).collect();

    groups.sort_by_key(|g| g.rank(tab));
    groups.dedup();

    groups
        .into_iter()
        .flat_map(|group| {
            let of = listed
                .iter()
                .filter(move |i| keys[**i].meta.group == Some(group))
                .map(|&i| Row::Key(i));

            std::iter::once(Row::Heading(group.label())).chain(of)
        })
        .collect()
}

/// The enhancements' rows: the preset, each topic under its heading, the
/// experimental switch, its keys while `fold` is open.
fn grouped(keys: &[Key], listed: &[usize], fold: Fold) -> Vec<Row> {
    let gated = |gate: Gate| listed.iter().filter(move |i| keys[**i].meta.gate == gate);
    let always: Vec<usize> = gated(Gate::Always).copied().collect();
    let experimental = gated(Gate::Experimental)
        .filter(|_| fold == Fold::Open)
        .map(|&i| Row::Key(i));

    std::iter::once(Row::Preset)
        .chain(headed(keys, &always, Tab::Enhancements))
        .chain(gated(Gate::Switch).next().map(|&i| Row::Fold(i)))
        .chain(experimental)
        .collect()
}

/// The page's places, layout pixels, sized to its rows.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sheet {
    width: i32,
    /// The tabs, in [`Tab::ALL`]'s order.
    tabs: [smnu::Rect; Tab::ALL.len()],
    /// The key's name, and RESET.
    heading: smnu::Rect,
    reset: smnu::Rect,
    /// The rows, their sunken box and scroll bar; the doc's box.
    list: smnu::Rect,
    list_box: smnu::Rect,
    bar: smnu::Rect,
    doc: smnu::Rect,
    /// SAVE, DEFAULTS, CANCEL.
    foot: [smnu::Rect; 3],
    /// A combo box's width.
    combo: i32,
}

/// The widths [`Sheet::of`] fits, layout pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Widths {
    /// The longest key's name, a combo box, the title.
    name: i32,
    combo: i32,
    title: i32,
    /// The scroll bar.
    bar: i32,
    /// The longest tab label, with its pad.
    tab: i32,
}

impl Sheet {
    /// The places for names and controls of `widths`, from `MX18`'s:
    /// SET ALL `button` (its size, and the top row, the tabs'), the list
    /// `list_at` in its box `box_at`; the doc's lines `pitch` apart.
    fn of(
        widths: Widths,
        (button, list_at, box_at): (smnu::Rect, smnu::Rect, smnu::Rect),
        pitch: f32,
    ) -> Sheet {
        let (left, top) = (list_at.x - box_at.x, list_at.y - box_at.y);
        let under = box_at.y + box_at.height - list_at.y - list_at.height;
        let control = widths.combo.max(NUMBER_WIDTH);
        let rows_wide = NAME_INDENT + widths.name + GAP + control + INSET;
        let foot_wide = 3 * button.width + 4 * BUTTON_GAP;
        let tab_least = widths.tab.max(button.width);
        let tabs_right = list_at.x + 2 * tab_least + BUTTON_GAP;
        let width = (list_at.x + rows_wide + BAR_GAP + widths.bar + RIGHT)
            .max(SLOT_LEFT + foot_wide + SLOT_RIGHT)
            .max(TITLE_LEFT + widths.title + TITLE_RIGHT)
            .max(tabs_right + RIGHT);
        let list_width = width - list_at.x - BAR_GAP - widths.bar - RIGHT;
        // The tabs share the top row, from the list's left to the right
        // margin, a gap apart.
        let tab_wide = (width - RIGHT - list_at.x - BUTTON_GAP) / 2;
        let tabs = [0, 1].map(|n| smnu::Rect {
            x: list_at.x + n * (tab_wide + BUTTON_GAP),
            width: tab_wide,
            ..button
        });
        #[allow(clippy::cast_possible_truncation, reason = "a box's pixels")]
        let doc_height = (DOC_LINES * pitch * DOC_SIZE).ceil() as i32 + 2 * DOC_PAD;
        // Under the rows: the key's name row, then the doc, a gap each.
        let named = DOC_GAP + button.height;
        let rows = (BODY_BOTTOM - DOC_GAP - doc_height - named - under - list_at.y) / ROW;
        let list = smnu::Rect {
            x: list_at.x,
            y: list_at.y,
            width: list_width,
            height: rows * ROW,
        };
        let list_box = smnu::Rect {
            x: box_at.x,
            y: box_at.y,
            width: left + list.width + BOX_RIGHT,
            height: top + list.height + under,
        };
        let named_y = list_box.y + list_box.height + DOC_GAP;
        let bottom = named_y + button.height;
        let reset = smnu::Rect {
            x: width - RIGHT - button.width,
            y: named_y,
            ..button
        };
        let first = (width - foot_wide) / 2 + BUTTON_GAP;
        let foot = |n: i32| smnu::Rect {
            x: first + n * (button.width + BUTTON_GAP),
            y: FOOT_Y,
            ..button
        };

        Sheet {
            width,
            tabs,
            heading: smnu::Rect {
                x: list.x,
                y: named_y,
                width: reset.x - BAR_GAP - list.x,
                ..button
            },
            reset,
            list,
            list_box,
            bar: smnu::Rect {
                x: list.x + list.width + BAR_GAP,
                y: list.y,
                width: widths.bar,
                height: list.height,
            },
            doc: smnu::Rect {
                x: list_box.x,
                y: bottom + DOC_GAP,
                width: width - list_box.x - RIGHT,
                height: BODY_BOTTOM - bottom - DOC_GAP,
            },
            foot: [foot(0), foot(1), foot(2)],
            combo: widths.combo,
        }
    }

    /// The rows shown at once.
    fn rows(&self) -> usize {
        usize::try_from(self.list.height / ROW).unwrap_or_default()
    }

    /// The controls' right edge.
    const fn right(&self) -> i32 {
        self.list.x + self.list.width - INSET
    }
}

/// The widths of `draft`'s names and choices, the title and the pieces,
/// as `layout`'s fonts in `ink` set them.
fn widths(
    ctx: &egui::Context,
    ink: &Store,
    (layout, combo): (&Layout, Option<&ShownControl>),
    draft: &Draft,
) -> Widths {
    let font = |record: Option<FourCc>| record.and_then(|r| ink.font(r));
    #[allow(clippy::cast_possible_truncation, reason = "a text's pixels")]
    let widest = |record: Option<FourCc>, texts: &mut dyn Iterator<Item = String>| {
        font(record).map_or(0, |f| {
            texts
                .map(|t| f.width(ctx, 1.0, &t).ceil() as i32)
                .max()
                .unwrap_or_default()
        })
    };
    let width_of = |id: i32| {
        layout
            .control(id)
            .and_then(|c| c.rect)
            .map_or(0, |r| r.width)
    };
    let names = widest(
        layout.control(tree::LIST).and_then(|c| c.font),
        &mut draft.keys().iter().map(|k| k.meta.name.to_owned()),
    );
    let choices = widest(
        combo.and_then(|c| c.font),
        &mut draft
            .keys()
            .iter()
            .filter_map(|&k| match draft.value(k) {
                Some(Now::Choice(_, labels)) => Some(labels),
                _ => None,
            })
            .flatten()
            .map(|label| (*label).to_owned()),
    );

    Widths {
        name: names,
        combo: choices + 2 * COMBO_PAD + ARROW_WIDTH,
        title: widest(
            layout.control(tree::TITLE).and_then(|c| c.font),
            &mut std::iter::once(TITLE.to_owned()),
        ),

        bar: width_of(menu::ids::SCROLL_BAR),
        tab: widest(
            layout.control(tree::SET_ALL).and_then(|c| c.font),
            &mut Tab::ALL.iter().map(|t| t.label().to_owned()),
        ) + 2 * TAB_PAD,
    }
}

/// The page over the main menu or a game, `draft` edited: the button
/// pressed, Escape as CANCEL while no list is open.
pub fn page(
    ctx: &egui::Context,
    (art, shared): (&mut Art, &mut service::Art),
    (draft, look): (&mut Draft, &mut Look),
) -> Option<Press> {
    let listing = look.open.is_some();
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let pressed = art.book.tree.as_ref().and_then(|layout| {
        let combo = art.book.setup.as_ref().and_then(|s| s.control(COMBO));
        let button = layout.control(tree::SET_ALL)?.rect?;
        let list_at = layout.control(tree::LIST)?.rect?;
        let box_at = layout.control(tree::LIST_BOX)?.rect?;
        let pitch = layout
            .control(tree::CHAT_BOX)
            .and_then(|c| c.font)
            .and_then(|f| art.ink.font(f))
            .map_or(0.0, |f| f.pitch);
        let sheet = Sheet::of(
            widths(ctx, &art.ink, (layout, combo), draft),
            (button, list_at, box_at),
            pitch,
        );
        #[allow(clippy::cast_precision_loss, reason = "a page's pixels")]
        let size = egui::vec2(sheet.width as f32, paint::size(layout).y);
        let fit = Fit::page(ctx.viewport_rect(), size);
        let mut canvas = Canvas::at(above(ctx), (&mut art.ink, shared), fit);

        drawn(&mut canvas, (layout, combo, &sheet), (draft, look))
    });

    pressed.or((escape && !listing).then_some(Press::Cancel))
}

/// The page's ui: a layer over the main menu's and the game's.
fn above(ctx: &egui::Context) -> egui::Ui {
    egui::Ui::new(
        ctx.clone(),
        Id::new("settings"),
        UiBuilder::new()
            .layer_id(LayerId::new(Order::Middle, Id::new("settings")))
            .max_rect(ctx.viewport_rect()),
    )
}

/// The click a press made this pass, taken, as the menus take theirs
/// (`menu::show`): a game's, which the menus do not draw.
#[must_use]
pub fn click(ctx: &egui::Context, art: &Art) -> Option<&'static [u8]> {
    paint::click(ctx).and_then(|wave| art.clicks.get(&wave).copied())
}

/// The page on `canvas`, from `layout` and the setup page's `combo`, in
/// `sheet`'s places.
fn drawn(
    canvas: &mut Canvas<'_>,
    (layout, combo, sheet): (&Layout, Option<&ShownControl>, &Sheet),
    (draft, look): (&mut Draft, &mut Look),
) -> Option<Press> {
    let keys = draft.keys().to_vec();
    let at = |id: i32, rect: smnu::Rect| {
        layout.control(id).map(|c| ShownControl {
            rect: Some(rect),
            ..c.clone()
        })
    };

    canvas.block(canvas.ui.max_rect());

    if let Some((record, set)) = layout.panel.art {
        frame(canvas, Pic::of(record, set), sheet.width);
    }

    if let Some(pog) = layout.control(tree::POG) {
        boxed(canvas, pog, None);
    }

    let title = layout
        .control(tree::TITLE)
        .and_then(|c| c.rect)
        .map(|r| smnu::Rect {
            width: sheet.width - r.x - TITLE_RIGHT,
            ..r
        });

    if let Some(c) = title.and_then(|r| at(tree::TITLE, r)) {
        boxed(canvas, &c, Some(TITLE));
    }

    let pressed = foot(canvas, (layout, sheet));

    // A tab pressed while no list is open turns to it.
    if let Some(tab) = tabs(canvas, (layout, sheet), look.tab)
        && look.open.is_none()
    {
        look.turn(tab);
    }

    let rows = rows(&keys, look.tab, look.fold);
    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let most = rows.len().saturating_sub(sheet.rows()) as f32;

    // The rows' sunken box, and the doc's in its look.
    for rect in [sheet.list_box, sheet.doc] {
        if let Some(c) = at(tree::LIST_BOX, rect) {
            ground(canvas, &c);
        }
    }

    if let Some(c) = at(tree::LIST, sheet.list) {
        ground(canvas, &c);
    }

    scroll_bar(canvas, (layout, sheet), look, most);
    wheel(canvas, sheet, look, most);
    listed(
        canvas,
        (layout, combo, sheet),
        (draft, look),
        (&keys, &rows),
    );
    doc(canvas, (layout, sheet), draft, (&keys, &rows, look));

    // The open list over everything, the page under it taking no press.
    let bar = layout.control(menu::ids::SCROLL_BAR).and_then(|b| b.art);

    if let (Some((at, first)), Some(combo)) = (look.open, combo) {
        let key = match at {
            Opened::Preset => None,
            Opened::Key(i) => keys.get(i).copied(),
        };
        let (names, picked): (Vec<String>, Option<usize>) = match key.map(|k| draft.value(k)) {
            None => (
                Preset::ALL.map(|p| p.label().to_owned()).to_vec(),
                Preset::ALL.iter().position(|p| *p == draft.preset()),
            ),
            Some(Some(Now::Choice(at, labels))) => {
                (labels.iter().map(|l| (*l).to_owned()).collect(), Some(at))
            }
            Some(_) => (Vec::new(), None),
        };
        let c = ShownControl {
            rect: row_of(sheet, &rows, at, look).map(|top| combo_rect(sheet, combo, top)),
            ..combo.clone()
        };
        let bottom = paint::size(layout).y;
        let mut first = first;
        let listed = on_top(canvas, |canvas| {
            list(canvas, (&c, bar), (&names, picked), (&mut first, bottom))
        });

        look.open = match listed {
            Listed::Open => Some((at, first)),
            Listed::Closed => None,
            Listed::Picked(n) => {
                match key {
                    None => {
                        if let Some(&preset) = Preset::ALL.get(n) {
                            draft.pick(preset);
                        }
                    }
                    Some(k) => draft.set(k, Now::Choice(n, &[])),
                }

                None
            }
        };
    }

    pressed
}

/// The bottom bar's SAVE, DEFAULTS and CANCEL on SET ALL's picture; the
/// one pressed.
fn foot(canvas: &mut Canvas<'_>, (layout, sheet): (&Layout, &Sheet)) -> Option<Press> {
    let buttons = [
        (Press::Save, SAVE),
        (Press::Defaults, DEFAULTS),
        (Press::Cancel, CANCEL),
    ];
    let set_all = layout.control(tree::SET_ALL)?;
    let mut pressed = None;

    for (n, (rect, (press, text))) in sheet.foot.into_iter().zip(buttons).enumerate() {
        let c = ShownControl {
            id: Some(own_id(tree::SET_ALL, n + 1)),
            rect: Some(rect),
            text: Some(text.to_owned()),
            ..set_all.clone()
        };

        if canvas.button(Pack::Expansion, &c, Able::Enabled) {
            pressed = Some(press);
        }
    }

    pressed
}

/// The tabs on SET ALL's picture, `shown` lit, as a held button is, in
/// its normal text colour: SET ALL's lit white does not read on its lit
/// picture. The one pressed.
fn tabs(canvas: &mut Canvas<'_>, (layout, sheet): (&Layout, &Sheet), shown: Tab) -> Option<Tab> {
    let set_all = layout.control(tree::SET_ALL)?;
    let mut lit_colours = set_all.colours.clone();

    if let (Some(&normal), Some(lit)) = (set_all.colours.first(), lit_colours.get_mut(1)) {
        *lit = normal;
    }

    let lit = ShownControl {
        flags: set_all.flags | smnu::LIT,
        inks: Inks::of((&lit_colours, &set_all.hotkey_colours), None),
        colours: lit_colours,
        ..set_all.clone()
    };
    let mut pressed = None;

    for (n, (rect, tab)) in sheet.tabs.into_iter().zip(Tab::ALL).enumerate() {
        let look = if tab == shown { &lit } else { set_all };
        let c = ShownControl {
            // REMOVE ALL's id, which the page does not draw: ids apart
            // from the bottom bar's.
            id: Some(own_id(tree::REMOVE_ALL, n)),
            rect: Some(rect),
            text: Some(tab.label().to_owned()),
            ..look.clone()
        };

        canvas.caps = Some(TAB_CAP);

        if canvas.button(Pack::Expansion, &c, Able::Enabled) {
            pressed = Some(tab);
        }

        canvas.caps = None;
    }

    pressed
}

/// The list's scroll bar at `sheet`'s, its rails each inside it: the
/// list scrolled by its arrows and thumb, `most` rows at most.
fn scroll_bar(
    canvas: &mut Canvas<'_>,
    (layout, sheet): (&Layout, &Sheet),
    look: &mut Look,
    most: f32,
) {
    for (rail, by) in [(tree::RAIL_TOP, 0), (tree::RAIL_BOTTOM, 1)] {
        let rail = layout.control(rail).and_then(|c| {
            let r = c.rect?;

            Some(ShownControl {
                rect: Some(smnu::Rect {
                    x: sheet.bar.x,
                    y: sheet.bar.y + by * (sheet.bar.height - r.height).max(0),
                    ..r
                }),
                ..c.clone()
            })
        });

        if let Some(c) = rail {
            canvas.clip = Some(fit::rect(sheet.bar));
            boxed(canvas, &c, None);
            canvas.clip = None;
        }
    }

    if let Some(bar) = layout.control(menu::ids::SCROLL_BAR) {
        let c = ShownControl {
            rect: Some(sheet.bar),
            ..bar.clone()
        };
        let moved = canvas.scroll_bar(Pack::Expansion, &c, (look.scroll, most, 1.0));

        look.scroll = (look.scroll + moved).clamp(0.0, most);
    }
}

/// `pic`, `MX18`'s frame, cut to `width` band by band ([`BANDS`]): its
/// left part as drawn, its right part moved in.
fn frame(canvas: &mut Canvas<'_>, pic: Pic, width: i32) {
    let Some(size) = canvas.size(Pack::Expansion, pic) else {
        return;
    };
    #[allow(clippy::cast_precision_loss, reason = "a page's pixels")]
    let wide = width as f32;
    let shift = wide - size.x;

    for (top, bottom, cut) in BANDS {
        let edge = match cut {
            Cut::Halves => width / 2,
            Cut::Strip(strip) => width - strip,
        };
        #[allow(clippy::cast_precision_loss, reason = "a page's pixels")]
        let (top, bottom, edge) = (top as f32, bottom as f32, edge as f32);
        let parts = [
            (Rect::from_min_max(pos2(0.0, top), pos2(edge, bottom)), 0.0),
            (
                Rect::from_min_max(pos2(edge, top), pos2(wide, bottom)),
                shift,
            ),
        ];

        for (clip, x) in parts {
            canvas.clip = Some(clip);
            canvas.picture(Pack::Expansion, pic, pos2(x, 0.0));
        }
    }

    canvas.clip = None;
}

/// The wheel over the list scrolls it a row a notch, while no list is
/// open.
fn wheel(canvas: &Canvas<'_>, sheet: &Sheet, look: &mut Look, most: f32) {
    let notches = paint::notches(canvas.ui.ctx());
    let over = paint::pointed(
        &canvas.ui,
        canvas.fit.rect(fit::rect(sheet.list)),
        "settings list",
    );

    if over && look.open.is_none() && notches != 0.0 {
        look.scroll = (look.scroll - notches.signum()).clamp(0.0, most);
    }
}

/// The rows shown, from the first scrolled to: a heading's name, a key's
/// name and control. The hovered key's doc is the one shown.
fn listed(
    canvas: &mut Canvas<'_>,
    (layout, combo, sheet): (&Layout, Option<&ShownControl>, &Sheet),
    (draft, look): (&mut Draft, &mut Look),
    (keys, rows): (&[Key], &[Row]),
) {
    let (Some(list), Some(heading)) =
        (layout.control(tree::LIST), layout.control(tree::NAME_LABEL))
    else {
        return;
    };
    let r = sheet.list;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = look.scroll.round() as usize;

    for (n, row) in rows.iter().enumerate().skip(first).take(sheet.rows()) {
        let top = r.y + i32::try_from(n - first).unwrap_or_default() * ROW;
        let whole = smnu::Rect {
            y: top,
            height: ROW,
            ..r
        };

        match *row {
            Row::Heading(section) => {
                let c = ShownControl {
                    rect: Some(whole),
                    ..heading.clone()
                };

                canvas.label(
                    &c,
                    &section.to_uppercase(),
                    paint::pick(heading, State::Normal),
                );
            }
            Row::Preset => {
                preset_row(canvas, (combo, list, sheet, top), (draft, look));
            }
            Row::Key(i) | Row::Fold(i) => {
                let key = keys[i];
                let folds = matches!(*row, Row::Fold(_));

                // Every other key a shade darker, its box's width, so a
                // name leads the eye to its control.
                if !folds && shaded(rows, n) {
                    let band = smnu::Rect {
                        y: top,
                        height: ROW,
                        ..sheet.list_box
                    };

                    canvas
                        .painter()
                        .rect_filled(canvas.fit.rect(fit::rect(band)), 0.0, SHADE);
                }

                let row = canvas.fit.rect(fit::rect(whole));

                if paint::pointed(&canvas.ui, row, ("settings row", i)) && look.shown != Some(i) {
                    look.shown = Some(i);
                    look.doc_scroll = 0.0;
                }

                let left = control(canvas, (layout, combo, sheet, top), (draft, look), (key, i));
                let indent = if folds {
                    fold_button(canvas, layout, sheet, (top, i), look);
                    NAME_INDENT + PLUS_STEP
                } else {
                    NAME_INDENT
                };
                let name = ShownControl {
                    rect: Some(smnu::Rect {
                        x: r.x + indent,
                        y: top,
                        width: left.unwrap_or(r.x + r.width) - r.x - indent,
                        height: ROW,
                    }),
                    ..list.clone()
                };

                line(
                    canvas,
                    &name,
                    (key.meta.name, paint::pick(list, State::Normal)),
                );
            }
        }
    }
}

/// The preset row at `top`: its name, and its list's box; hovered, its
/// doc shows.
fn preset_row(
    canvas: &mut Canvas<'_>,
    (combo, list, sheet, top): (Option<&ShownControl>, &ShownControl, &Sheet, i32),
    (draft, look): (&Draft, &mut Look),
) {
    let whole = smnu::Rect {
        y: top,
        height: ROW,
        ..sheet.list
    };

    if paint::pointed(
        &canvas.ui,
        canvas.fit.rect(fit::rect(whole)),
        "settings preset",
    ) && look.shown.is_some()
    {
        look.shown = None;
        look.doc_scroll = 0.0;
    }

    let now = draft.preset().label();
    let name = ShownControl {
        rect: Some(smnu::Rect {
            x: sheet.list.x + NAME_INDENT,
            width: sheet.list.width - NAME_INDENT,
            ..whole
        }),
        ..list.clone()
    };

    line(canvas, &name, (PRESET, paint::pick(list, State::Normal)));

    if let Some(combo) = combo {
        let c = ShownControl {
            rect: Some(combo_rect(sheet, combo, top)),
            // Past every key's place.
            id: Some(own_id(COMBO, ROW_IDS as usize - 1)),
            ..combo.clone()
        };

        if setup::combo(canvas, &c, (now, Able::Enabled)) && look.open.is_none() {
            #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
            let at = Preset::ALL
                .iter()
                .position(|p| p.label() == now)
                .unwrap_or_default() as f32;

            look.open = Some((Opened::Preset, at));
        }
    }
}

/// The fold's button in the row at `top`, of key `i`: + opens it, - shuts
/// it, `MX18`'s pictures, at the name's indent.
fn fold_button(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    sheet: &Sheet,
    (top, i): (i32, usize),
    look: &mut Look,
) {
    let (id, next) = match look.fold {
        Fold::Shut => (tree::PLUS, Fold::Open),
        Fold::Open => (tree::MINUS, Fold::Shut),
    };
    let Some(c) = layout.control(id) else {
        return;
    };
    let c = ShownControl {
        rect: c.rect.map(|r| smnu::Rect {
            x: sheet.list.x + NAME_INDENT,
            y: top + (ROW - r.height) / 2,
            ..r
        }),
        id: Some(own_id(id, i)),
        font: c.font.or(layout.panel.font),
        ..c.clone()
    };

    if canvas.button(Pack::Expansion, &c, Able::Enabled) && look.open.is_none() {
        look.fold = next;
    }
}

/// Whether `key` can be edited: not an experimental key with their
/// switch off.
fn live(draft: &Draft, key: Key) -> Able {
    let off = !draft.settings().enhancements.experimental;

    able(!(off && key.meta.gate == Gate::Experimental))
}

/// `key`'s control in the row at `top`, by its kind, at the list's
/// right: the left of what it drew.
fn control(
    canvas: &mut Canvas<'_>,
    (layout, combo, sheet, top): (&Layout, Option<&ShownControl>, &Sheet, i32),
    (draft, look): (&mut Draft, &mut Look),
    (key, i): (Key, usize),
) -> Option<i32> {
    let value = draft.value(key)?;
    let right = sheet.right();
    let live = live(draft, key);
    let on = live == Able::Enabled;
    // Control `id` of `MX18`'s first row, its right `from_right` left of
    // the controls' right.
    let in_row = |id: i32, from_right: i32| {
        layout.control(id).map(|c| ShownControl {
            rect: c.rect.map(|r| smnu::Rect {
                x: right - from_right - r.width,
                y: top + (ROW - r.height) / 2,
                ..r
            }),
            id: Some(own_id(id, i)),
            font: c.font.or(layout.panel.font),
            ..c.clone()
        })
    };
    let count = || in_row(tree::COUNT, 0);
    let count_wide = count().and_then(|c| c.rect).map_or(0, |r| r.width);
    let minus = || in_row(tree::MINUS, count_wide + PLUS_STEP - 16);
    let plus = || in_row(tree::PLUS, count_wide + 2 * PLUS_STEP - 16);

    match value {
        Now::Check(ticked) => {
            let c = in_row(tree::BOX, 0)?;
            let tick = if ticked { Tick::Checked } else { Tick::Clear };

            if canvas.ticked(Pack::Expansion, &c, (live, tick)) {
                draft.set(key, Now::Check(!ticked));
            }

            c.rect.map(|r| r.x)
        }
        Now::Number(x, whole) => {
            let (plus, minus, count) = (plus()?, minus()?, count()?);
            let range = key.meta.range.as_ref();
            let step = range.map_or(1.0, |r| (r.end() - r.start()) / STEPS);
            let (low, high) = range.map_or((f64::MIN, f64::MAX), |r| (*r.start(), *r.end()));
            let mut to = None;

            if canvas.button(Pack::Expansion, &plus, able(on && x < high)) {
                to = Some(x + step);
            }

            if canvas.button(Pack::Expansion, &minus, able(on && x > low)) {
                to = Some(x - step);
            }

            ground(canvas, &count);
            line(canvas, &count, (&value.shown(), VALUE));

            if on {
                to = to.or_else(|| dragged(canvas, &count, (x, range), look));
            }

            if let Some(to) = to {
                draft.set(key, Now::Number(to, whole));
            }

            plus.rect.map(|r| r.x)
        }
        Now::Choice(at, _) => {
            let c = ShownControl {
                rect: Some(combo_rect(sheet, combo?, top)),
                id: Some(own_id(COMBO, i)),
                ..combo?.clone()
            };

            // Opened at the pick, as the setup page's lists.
            if setup::combo(canvas, &c, (&value.shown(), live)) && look.open.is_none() {
                #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
                let at = at as f32;

                look.open = Some((Opened::Key(i), at));
            }

            c.rect.map(|r| r.x)
        }
    }
}

/// Value box `c` dragged across: `x`, held as the drag began, moved
/// through `range` as far as the drag through the box's width (a point
/// a step unranged); the number, while dragged.
fn dragged(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (x, range): (f64, Option<&std::ops::RangeInclusive<f64>>),
    look: &mut Look,
) -> Option<f64> {
    let r = c.rect?;
    let response = canvas.ui.interact(
        canvas.fit.rect(fit::rect(r)),
        canvas.ui.id().with(("settings value", c.id)),
        Sense::drag(),
    );

    if response.drag_started() {
        look.drag = Some(x);
    }

    let origin = canvas.ui.ctx().input(|i| i.pointer.press_origin());
    let moved = response
        .interact_pointer_pos()
        .zip(origin)
        .map(|(now, from)| f64::from((now.x - from.x) / canvas.fit.scale));
    let per_pixel = range.map_or(1.0, |range| {
        (range.end() - range.start()) / f64::from(r.width.max(1))
    });

    if response.drag_stopped() {
        look.drag = None;
    }

    response
        .dragged()
        .then(|| look.drag.zip(moved).map(|(from, by)| from + by * per_pixel))
        .flatten()
}

/// The shown key's name in the top row, as "SAVE AS:" sits, and RESET,
/// as SET ALL; its doc and default in the doc box (the list box's look,
/// `MX18`'s chat box's text), or why the settings will not read or save.
fn doc(
    canvas: &mut Canvas<'_>,
    (layout, sheet): (&Layout, &Sheet),
    draft: &mut Draft,
    (keys, rows, look): (&[Key], &[Row], &mut Look),
) {
    let at = |id: i32, rect: smnu::Rect| {
        layout.control(id).map(|c| ShownControl {
            rect: Some(rect),
            ..c.clone()
        })
    };

    // The preset has no key: its name and doc alone, no RESET.
    if look.shown.is_none() && rows.first() == Some(&Row::Preset) {
        if let Some(c) = at(tree::NAME_LABEL, sheet.heading) {
            boxed(canvas, &c, Some(PRESET));
        }

        if let Some(c) = at(tree::CHAT_BOX, padded(sheet.doc)) {
            canvas.text_box(
                (Pack::Expansion, &c, None),
                (PRESET_DOC, Align::Left, DOC_SIZE),
                &mut look.doc_scroll,
            );
        }

        return;
    }

    let Some(key) = shown(rows, look).and_then(|i| keys.get(i).copied()) else {
        return;
    };

    if let Some(c) = at(tree::NAME_LABEL, sheet.heading) {
        boxed(canvas, &c, Some(key.meta.name));
    }

    let default = Draft::default_of(key);
    let reset = at(tree::SET_ALL, sheet.reset).map(|c| ShownControl {
        text: Some(RESET.to_owned()),
        ..c
    });

    if let (Some(c), Some(default)) = (reset, default) {
        let differs = draft.value(key) != Some(default);

        if canvas.button(Pack::Expansion, &c, able(differs)) {
            draft.set(key, default);
        }
    }

    // The chat box's text, inside the doc's box.
    let Some(c) = at(tree::CHAT_BOX, padded(sheet.doc)) else {
        return;
    };

    if let Some(error) = draft.error() {
        let first = ShownControl {
            rect: Some(smnu::Rect {
                height: ROW,
                ..padded(sheet.doc)
            }),
            ..c.clone()
        };

        line(canvas, &first, (error, REFUSED));
    } else {
        let text = format!("{}\n{}", key.meta.about, hint(key));

        canvas.text_box(
            (Pack::Expansion, &c, None),
            (&text, Align::Left, DOC_SIZE),
            &mut look.doc_scroll,
        );
    }
}

/// The key whose doc shows: the last hovered, else the tab's first.
fn shown(rows: &[Row], look: &Look) -> Option<usize> {
    look.shown.or_else(|| {
        rows.iter().find_map(|r| match r {
            Row::Key(i) | Row::Fold(i) => Some(*i),
            Row::Heading(_) | Row::Preset => None,
        })
    })
}

/// `key`'s default, and its range: "Default: 1 (0 to 1)".
fn hint(key: Key) -> String {
    let default = Draft::default_of(key).map_or_default(|d| d.shown());
    let range = key.meta.range.as_ref().map_or_else(String::new, |r| {
        format!(" ({} to {})", number(*r.start()), number(*r.end()))
    });

    format!("Default: {default}{range}")
}

/// `rect` shrunk by [`DOC_PAD`] each side: the doc's text inside its box.
const fn padded(rect: smnu::Rect) -> smnu::Rect {
    smnu::Rect {
        x: rect.x + DOC_PAD,
        y: rect.y + DOC_PAD,
        width: rect.width - 2 * DOC_PAD,
        height: rect.height - 2 * DOC_PAD,
    }
}

/// Whether row `n` is shaded: every other key under a section's name or
/// the fold, the first not, so the bands start again under each.
fn shaded(rows: &[Row], n: usize) -> bool {
    let heading = rows[..n]
        .iter()
        .rposition(|r| matches!(r, Row::Heading(_) | Row::Fold(_)))
        .unwrap_or_default();

    (n - heading).is_multiple_of(2)
}

/// Control `id` as key `i`'s row draws it: an id of its own, so egui
/// tells the rows apart.
fn own_id(id: i32, i: usize) -> i32 {
    id * ROW_IDS + i32::try_from(i).unwrap_or_default()
}

/// Enabled when `on`.
const fn able(on: bool) -> Able {
    if on { Able::Enabled } else { Able::Disabled }
}

/// The combo box in the row at `top`: as wide as the longest choice
/// needs, its right at the controls' right, as high as the setup page's.
fn combo_rect(sheet: &Sheet, combo: &ShownControl, top: i32) -> smnu::Rect {
    let high = combo.rect.map_or(ROW, |r| r.height);

    smnu::Rect {
        x: sheet.right() - sheet.combo,
        y: top + (ROW - high) / 2,
        width: sheet.combo,
        height: high,
    }
}

/// The top of list `at`'s row, as scrolled.
fn row_of(sheet: &Sheet, rows: &[Row], at: Opened, look: &Look) -> Option<i32> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = look.scroll.round() as usize;
    let wanted = match at {
        Opened::Preset => Row::Preset,
        Opened::Key(i) => Row::Key(i),
    };
    let n = rows.iter().position(|r| *r == wanted)?;
    let shown = i32::try_from(n).ok()? - i32::try_from(first).ok()?;

    Some(sheet.list.y + shown * ROW)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::config::Settings;

    fn headings(rows: &[Row]) -> Vec<&'static str> {
        rows.iter()
            .filter_map(|r| match r {
                Row::Heading(s) => Some(*s),
                _ => None,
            })
            .collect()
    }

    fn key(draft: &Draft, name: &str) -> Key {
        *draft
            .keys()
            .iter()
            .find(|k| k.meta.key == name)
            .expect("the key")
    }

    /// Every heading on the settings tab has a key under it.
    #[test]
    fn no_settings_group_is_empty() {
        let draft = Draft::new(&Settings::default());
        let rows = rows(draft.keys(), Tab::Settings, Fold::Shut);

        assert!(matches!(rows.last(), Some(Row::Key(_))));
        assert!(
            rows.windows(2)
                .all(|w| !matches!(w, [Row::Heading(_), Row::Heading(_)]))
        );
        assert_eq!(headings(&rows), ["game", "sound", "display", "interface"]);
    }

    /// The experimental keys are editable only with their switch on.
    #[test]
    fn the_switch_decides_whether_its_keys_are_live() {
        let mut draft = Draft::new(&Settings::default());

        assert_eq!(live(&draft, key(&draft, "cast")), Able::Disabled);
        assert_eq!(live(&draft, key(&draft, "rings")), Able::Enabled);

        draft.set(key(&draft, "experimental"), Now::Check(true));

        assert_eq!(live(&draft, key(&draft, "cast")), Able::Enabled);
    }

    /// Every key the page lists appears once; hidden ones (the debug
    /// keys) stay in the file only.
    #[test]
    fn every_key_shows_on_exactly_one_tab() {
        let draft = Draft::new(&Settings::default());
        let keys = draft.keys();
        let mut listed: Vec<usize> = Tab::ALL
            .iter()
            .flat_map(|&tab| rows(keys, tab, Fold::Open))
            .filter_map(|r| match r {
                Row::Key(i) | Row::Fold(i) => Some(i),
                Row::Heading(_) | Row::Preset => None,
            })
            .collect();

        listed.sort_unstable();

        assert_eq!(
            listed,
            (0..keys.len())
                .filter(|&i| keys[i].meta.show == Show::Page)
                .collect::<Vec<_>>()
        );
        assert_eq!(key(&draft, "debug_keys").meta.show, Show::Hidden);
    }

    /// The shading starts again under each section's name: its first key
    /// clear.
    #[test]
    fn every_other_key_of_a_section_is_shaded() {
        let draft = Draft::new(&Settings::default());

        for tab in Tab::ALL {
            let rows = rows(draft.keys(), tab, Fold::Open);

            for n in 1..rows.len() {
                match (rows[n - 1], rows[n]) {
                    (Row::Heading(_), Row::Key(_)) => assert!(!shaded(&rows, n)),
                    (Row::Key(_), Row::Key(_)) => {
                        assert_ne!(shaded(&rows, n), shaded(&rows, n - 1));
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn the_hint_gives_the_default_and_range() {
        let draft = Draft::new(&Settings::default());

        assert_eq!(hint(key(&draft, "music")), "Default: 1 (0 to 1)");
        assert_eq!(hint(key(&draft, "bars")), "Default: When hurt");
        assert_eq!(hint(key(&draft, "speed_keys")), "Default: On");
    }

    /// What `tab` lists, `fold` as given, row by row.
    fn listed_rows(tab: Tab, fold: Fold) -> Vec<String> {
        let draft = Draft::new(&Settings::default());
        let f = draft.keys();

        rows(f, tab, fold)
            .into_iter()
            .map(|r| match r {
                Row::Heading(h) => format!("heading {h}"),
                Row::Preset => "preset".to_owned(),
                Row::Key(i) => format!("key {}.{}", f[i].part.name, f[i].meta.key),
                Row::Fold(i) => format!("fold {}.{}", f[i].part.name, f[i].meta.key),
            })
            .collect()
    }

    /// Each tab's rows, in order.
    #[test]
    fn each_tab_lists_its_rows_in_order() {
        assert_eq!(
            listed_rows(Tab::Settings, Fold::Shut),
            [
                "heading game",
                "key game.autosave",
                "key game.speed",
                "heading sound",
                "key audio.effects",
                "key audio.music",
                "key audio.muted",
                "heading display",
                "key ui.window",
                "heading interface",
                "key ui.bars",
                "key ui.intro",
                "key ui.main_menu",
                "key ui.player_list",
                "key ui.scroll_speed",
                "key ui.names",
                "key ui.wasd_scroll",
            ]
        );

        let shut = listed_rows(Tab::Enhancements, Fold::Shut);
        let open = listed_rows(Tab::Enhancements, Fold::Open);

        assert_eq!(
            shut,
            [
                "preset",
                "heading display",
                "key enhancements.ui_upscale",
                "key enhancements.view_upscale",
                "key enhancements.rings",
                "key enhancements.shadows",
                "key enhancements.side_ring",
                "key enhancements.smooth_position",
                "key enhancements.video_upscale",
                "heading game",
                "key enhancements.speed_keys",
                "key enhancements.show_wizards_guild_spell_range",
                "heading interface",
                "key enhancements.message_links",
                "key enhancements.rage_on_spell_bar",
                "key enhancements.smooth_arrows",
                "key enhancements.smooth_zoom",
                "fold enhancements.experimental",
            ]
        );
        assert_eq!(open[..shut.len()], shut);
        assert_eq!(
            open[shut.len()..],
            [
                "key enhancements.cast",
                "key enhancements.self_shadows",
                "key enhancements.sun_direction",
                "key enhancements.sun_elevation",
                "key enhancements.sun_size",
                "key enhancements.wind",
                "key enhancements.wind_amplitude",
            ]
        );
    }

    /// Each key, its hint and its choices.
    const HINTS: [(&str, &str, &str); 35] = [
        (
            "game.autosave",
            "Default: Every 5 days",
            "Off,Every day,Every 5 days,Every 10 days",
        ),
        (
            "enhancements.cast",
            "Default: Painted",
            "Painted,From models",
        ),
        ("enhancements.debug_keys", "Default: Off", ""),
        ("audio.effects", "Default: 1 (0 to 1)", ""),
        ("enhancements.experimental", "Default: Off", ""),
        (
            "game.speed",
            "Default: 5 (normal)",
            "1 (slowest),2,3,4,5 (normal),6,7,8,9 (fastest)",
        ),
        ("ui.bars", "Default: When hurt", "Always,When hurt,Never"),
        ("enhancements.ui_upscale", "Default: xBRZ", "Off,xBRZ"),
        ("ui.intro", "Default: On", "On,Off"),
        (
            "ui.main_menu",
            "Default: Majesty",
            "Majesty,Northern Expansion",
        ),
        (
            "enhancements.view_upscale",
            "Default: Sharp",
            "Off,Sharp,FSR,Sharp, per sprite,FSR, per sprite",
        ),
        ("enhancements.message_links", "Default: On", ""),
        ("audio.music", "Default: 1 (0 to 1)", ""),
        ("audio.muted", "Default: Off", ""),
        (
            "ui.player_list",
            "Default: Players and timing",
            "Off,Players,Players and timing",
        ),
        ("enhancements.rage_on_spell_bar", "Default: On", ""),
        ("ui.scroll_speed", "Default: 32 (32 to 64)", ""),
        (
            "enhancements.rings",
            "Default: Modern",
            "Original art,Drawn,Modern",
        ),
        ("enhancements.self_shadows", "Default: Off", ""),
        ("enhancements.shadows", "Default: Soft", "Stacked,Soft"),
        ("enhancements.side_ring", "Default: On", ""),
        ("enhancements.smooth_arrows", "Default: On", ""),
        ("enhancements.smooth_position", "Default: On", ""),
        ("enhancements.smooth_zoom", "Default: On", ""),
        ("enhancements.speed_keys", "Default: On", ""),
        (
            "enhancements.show_wizards_guild_spell_range",
            "Default: On",
            "",
        ),
        (
            "enhancements.sun_direction",
            "Default: 29 (-180 to 180)",
            "",
        ),
        ("enhancements.sun_elevation", "Default: 50 (10 to 89)", ""),
        ("enhancements.sun_size", "Default: 3 (0 to 8)", ""),
        (
            "ui.names",
            "Default: All units",
            "Off,All units,Heroes only",
        ),
        ("enhancements.video_upscale", "Default: FSR", "Smooth,FSR"),
        ("ui.wasd_scroll", "Default: Off", ""),
        ("enhancements.wind", "Default: Off", ""),
        ("enhancements.wind_amplitude", "Default: 2 (0 to 4)", ""),
        (
            "ui.window",
            "Default: Windowed",
            "Windowed,Full screen,Exclusive full screen",
        ),
    ];

    /// Each key's hint, and its choices where it has a list.
    #[test]
    fn each_key_shows_its_hint_and_choices() {
        let draft = Draft::new(&Settings::default());
        let shown: Vec<(String, String, String)> = draft
            .keys()
            .iter()
            .map(|&k| {
                let choices = match draft.value(k) {
                    Some(Now::Choice(_, labels)) => labels.join(","),
                    _ => String::new(),
                };

                (format!("{}.{}", k.part.name, k.meta.key), hint(k), choices)
            })
            .collect();

        assert_eq!(
            shown,
            HINTS.map(|(k, h, c)| (k.to_owned(), h.to_owned(), c.to_owned()))
        );
    }

    /// `MX18`'s SET ALL and list.
    const BUTTON: smnu::Rect = smnu::Rect {
        x: 338,
        y: 89,
        width: 100,
        height: 28,
    };
    const LIST: smnu::Rect = smnu::Rect {
        x: 20,
        y: 125,
        width: 350,
        height: 400,
    };
    const BOX: smnu::Rect = smnu::Rect {
        x: 13,
        y: 118,
        width: 503,
        height: 412,
    };

    fn widths(name: i32) -> Widths {
        Widths {
            name,
            combo: 100,
            title: 200,
            bar: 25,
            tab: 150,
        }
    }

    /// Short names: the bottom bar sets the width; long ones widen the
    /// page, the controls kept at the list's right.
    #[test]
    fn the_page_is_as_wide_as_its_rows_or_buttons_need() {
        let short = Sheet::of(widths(50), (BUTTON, LIST, BOX), 12.0);

        assert_eq!(
            short.width,
            SLOT_LEFT + 3 * 100 + 4 * BUTTON_GAP + SLOT_RIGHT
        );
        assert!(short.tabs.iter().all(|t| t.y == BUTTON.y));
        assert!(short.tabs.iter().all(|t| t.width >= 150));
        assert_eq!(short.tabs[0].x, LIST.x);
        assert_eq!(
            short.tabs[0].x + short.tabs[0].width + BUTTON_GAP,
            short.tabs[1].x
        );

        // A pixel short at most: an odd width halved.
        let right = short.tabs[1].x + short.tabs[1].width;

        assert!((short.width - RIGHT - right).abs() <= 1);

        let long = Sheet::of(widths(400), (BUTTON, LIST, BOX), 12.0);

        assert_eq!(
            long.width,
            LIST.x + NAME_INDENT + 400 + GAP + NUMBER_WIDTH + INSET + BAR_GAP + 25 + RIGHT
        );
        assert_eq!(long.bar.x + long.bar.width, long.width - RIGHT);
        assert!(long.width < 800);
    }

    /// One pass of the page's list in a game, the wheel turned `notches`
    /// (+ up) with the pointer at the list's middle; the first row shown.
    fn wheeled_in_game(ctx: &egui::Context, look: &mut Look, notches: f32) -> f32 {
        let sheet = Sheet::of(widths(120), (BUTTON, LIST, BOX), 12.0);
        let window = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let fit = Fit::page(window, egui::vec2(800.0, 600.0));
        let middle = fit.rect(fit::rect(sheet.list)).center();
        let turned = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, notches),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        };
        let input = egui::RawInput {
            screen_rect: Some(window),
            events: vec![egui::Event::PointerMoved(middle), turned],
            ..egui::RawInput::default()
        };
        let (mut ink, mut art) = (Store::default(), service::Art::default());
        let mut output = ctx.run_ui(input, |ui| {
            let canvas = Canvas::at(above(ui.ctx()), (&mut ink, &mut art), fit);

            canvas.block(canvas.ui.max_rect());
            wheel(&canvas, &sheet, look, 50.0);
        });

        // No painter takes the font atlas here.
        output.textures_delta.clear();
        look.scroll
    }

    /// The wheel over the list scrolls it in a game, as over the menus.
    #[test]
    #[allow(clippy::float_cmp, reason = "the list scrolls whole rows")]
    fn the_wheel_scrolls_the_list_in_a_game() {
        let ctx = crate::ui::typeface::context();
        let mut look = Look::default();

        // A pass to lay the page out, as the game's last frame did.
        wheeled_in_game(&ctx, &mut look, 0.0);

        assert_eq!(wheeled_in_game(&ctx, &mut look, -1.0), 1.0);
    }

    /// The rows end above the key's name and RESET, which end above the
    /// doc box, which ends above the bottom bar.
    #[test]
    fn the_doc_box_fits_under_the_rows() {
        let sheet = Sheet::of(widths(120), (BUTTON, LIST, BOX), 12.0);

        assert_eq!(sheet.list.height % ROW, 0);
        assert!(sheet.heading.y >= sheet.list_box.y + sheet.list_box.height);
        assert_eq!(sheet.reset.y, sheet.heading.y);
        assert!(sheet.doc.y >= sheet.heading.y + sheet.heading.height);
        assert_eq!(sheet.doc.y + sheet.doc.height, BODY_BOTTOM);
        assert!(f64::from(sheet.doc.height) >= 5.0 * 12.0);
        assert!(sheet.foot.iter().all(|b| b.y > BODY_BOTTOM));
    }
}
