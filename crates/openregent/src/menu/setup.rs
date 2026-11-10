//! The freestyle setup page, `MX17`, in its
//! own art: as large as fits, centred, the faces around it, as the main
//! menu; its boxes tiled and bevelled by their style
//! (`view::menu::bevel`), its lists combo boxes of `INcb`, the Map
//! Number typed in its box.
//!
//! ```text
//!  ┌(pog)────── Single Player Freestyle Options ──────────────┐
//!  │ LOAD SAVE [Default Settings              ] ┌──────────┐ │
//!  │ PLAYER FORCES  [pick       v] [blurb     ] │ portrait │ │
//!  │ ...                                        │          │ │
//!  │ BUILDING LIMITS [default v] [EDIT] MAP NUMBER [n]     │ │
//!  │                                    [  RANDOM  ]       │ │
//!  │ (ACCEPT)            DIFFICULTY             (CANCEL)     │
//!  └──────────────────────────────────────────────────────────┘
//! ```
//!
//! Drawn in the panel's order, but the chat (a network game's) and
//! ALLOW CHEATS, which the capture does not show. LOAD and SAVE open
//! their pages (`preset_page`), the box beside them naming the settings
//! last loaded or saved; BUILDING EDIT MENU opens the Build Tree Editor
//! (`tree`). RANDOM is this game's, where ALLOW CHEATS stands. As a LAN
//! host's settings (`lan`): titled with the session's name, the chat and
//! ALLOW CHEATS shown, the portrait and RANDOM not.

use bevy_egui::egui::{self, Color32, Id, LayerId, Order, Rect, Sense, UiBuilder, pos2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu::{self, style};
use regent_core::sim::freestyle::ONE_PLAYER;
use regent_core::sim::world::cheat::Cheats;
use regent_core::view::menu::bevel::Bevel;
use regent_core::view::menu::{self, Layout, ShownControl, State, Tick, setup};
use regent_core::view::raster::shade::{LEVELS, Shade};

use super::art::{self, Art, Pack, Pic};
use super::fit;
use super::font;
use super::freestyle::{Choice, Custom, Dialog, MAP_NUMBER_MIN};
use super::page::{Button, Press};
use super::paint::{self, Able, Canvas, Inset};
use regent_core::sim::mapgen::preset;
use regent_draw::art as service;
use regent_draw::soft::{self, Blending};

/// The headings' text, which sets no colour. **Game**: the capture's
/// `#d89848`, a glyph's top at `0xe0` of `0xff` (`font::HOT_KEY`), so
/// this at full.
const HEADING: Color32 = Color32::from_rgb(0xf6, 0xad, 0x52);

/// A combo box's text, this far inside its field. **Game**: "Wizard
/// Hold" starts 3 px in.
const PAD: f32 = 3.0;

/// A combo box's arrow where its set has no picture: 20 px.
const ARROW_WIDTH: f32 = 20.0;

/// The picture that stands for none on a text box, `INTI` 1016, 10 × 6,
/// all holes ([`ground`]).
const NO_ART: (FourCc, i32) = (FourCc(*b"INTI"), 1016);

/// RANDOM: this game's, over the Map Number where the capture shows no
/// ALLOW CHEATS; the chat's SEND CHAT bar, `INMs` 1007, 162 × 20.
const RANDOM: i32 = setup::MAP_NUMBER + 1;
const RANDOM_ART: (FourCc, i32) = (FourCc(*b"INMs"), 1007);
const RANDOM_AT: smnu::Rect = smnu::Rect {
    x: 381,
    y: 520,
    width: 162,
    height: 20,
};
const RANDOM_TEXT: &str = "RANDOM";

/// The Map Number's most digits: `i32::MAX`'s.
const MAP_DIGITS: usize = 10;

/// The lists, by their combo box, and the blurb box beside each.
const CHOICES: [(i32, Choice, Option<i32>); 10] = [
    (
        setup::SETTLEMENT,
        Choice::Settlement,
        Some(setup::SETTLEMENT_TEXT),
    ),
    (
        setup::SCENARIO,
        Choice::Scenario,
        Some(setup::SCENARIO_TEXT),
    ),
    (
        setup::MONSTERS,
        Choice::Monsters,
        Some(setup::MONSTERS_TEXT),
    ),
    (setup::EVENT_1, Choice::Event(0), Some(setup::EVENT_1_TEXT)),
    (setup::EVENT_2, Choice::Event(1), Some(setup::EVENT_2_TEXT)),
    (setup::LAND, Choice::Land, None),
    (setup::VICTORY, Choice::Victory, None),
    (setup::GOLD, Choice::Gold, None),
    (setup::LIMITS, Choice::Limits, None),
    (setup::BUILDINGS, Choice::Buildings, None),
];

const CHEATS_LABEL_INSET: i32 = 24;

fn cheats_parts(c: &ShownControl) -> (ShownControl, ShownControl) {
    let icon = ShownControl {
        text: None,
        ..c.clone()
    };
    let label = ShownControl {
        rect: c.rect.map(|mut rect| {
            rect.x += CHEATS_LABEL_INSET;
            rect.width -= CHEATS_LABEL_INSET;
            rect
        }),
        art: None,
        ..c.clone()
    };

    (icon, label)
}

fn cheats_look(custom: &Custom) -> (Able, Tick) {
    let tick = if custom.cheats == Cheats::Allowed {
        Tick::Checked
    } else {
        Tick::Clear
    };

    (Able::Enabled, tick)
}

const fn toggle_cheats(custom: &mut Custom) {
    custom.cheats = match custom.cheats {
        Cheats::Allowed => Cheats::Barred,
        Cheats::Barred => Cheats::Allowed,
    };
}

/// The page's state between passes.
#[derive(Debug, Default)]
pub struct Setup {
    /// The list open, by its combo box, and how far it is scrolled, in
    /// rows: the first shown, rounded.
    open: Option<(i32, f32)>,
    /// The Map Number as typed, while its box has the keys.
    typed: Option<String>,
}

impl Setup {
    /// The page with combo box `id`'s list open, scrolled to its top:
    /// `--menu setup:ID`'s, for screenshots.
    #[must_use]
    pub const fn listing_of(id: Option<i32>) -> Setup {
        Setup {
            open: match id {
                Some(id) => Some((id, 0.0)),
                None => None,
            },
            typed: None,
        }
    }

    /// Whether a list is open: it takes Escape.
    #[must_use]
    pub const fn listing(&self) -> bool {
        self.open.is_some()
    }
}

/// What an open list did this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Listed {
    Open,
    Closed,
    Picked(usize),
}

/// The page: `custom` shown and edited, `failed` why the last game
/// would not start; Accept or Cancel pressed. With `lan`, the host's
/// settings of a LAN game: titled with the session's name, its chat
/// drawn where the portrait stands, ALLOW CHEATS shown, no RANDOM.
#[allow(
    clippy::too_many_lines,
    reason = "every control of the dialog, in one place"
)]
pub fn page(
    ctx: &egui::Context,
    (art, shared, state): (&mut Art, &mut service::Art, &mut Setup),
    custom: &mut Custom,
    (failed, lan): (Option<&str>, Option<&mut super::lan::Lan>),
) -> Option<Press> {
    let Art {
        book, ink, lists, ..
    } = art;
    let layout = book.setup.as_ref()?;
    let size = paint::size(layout);
    let mut canvas = Canvas::filling(ctx, (ink, shared), size);
    let listing = state.open.map(|(id, _)| id);
    let title = lan.as_deref().map(super::lan::Lan::title);
    // A LAN game's lists are those of its players.
    let players = lan.as_deref().map_or(ONE_PLAYER, super::lan::Lan::players);
    let dialog = lists.dialog(players);
    let mut opened = None;
    let mut pressed = None;

    canvas.backdrop();

    canvas.panel_art(Pack::Expansion, layout);

    for c in &layout.controls {
        let id = c.id.unwrap_or_default();
        let choice = CHOICES
            .iter()
            .find(|(at, ..)| *at == id)
            .map(|(_, ch, _)| *ch);
        let blurb = CHOICES
            .iter()
            .find(|(.., text)| *text == Some(id))
            .and_then(|(_, ch, _)| ch.blurb(custom, dialog));

        match (id, choice) {
            // The chat drawn after, a LAN game's; its portrait not.
            (_, _) if setup::CHAT.contains(&id) => {}
            (setup::PORTRAIT, _) if title.is_some() => {}
            (setup::TITLE, _) if title.is_some() => boxed(&mut canvas, c, title.as_deref()),
            (setup::CHEATS, _) => {
                if title.is_some() {
                    let (icon, label) = cheats_parts(c);

                    if canvas.ticked(Pack::Expansion, &icon, cheats_look(custom)) {
                        toggle_cheats(custom);
                    }
                    if let Some(text) = label.text.as_deref() {
                        line(&mut canvas, &label, (text, paint::pick(c, State::Normal)));
                    }
                }
            }
            (_, Some(choice)) => {
                let able = if choice.offered(custom, dialog) {
                    Able::Enabled
                } else {
                    Able::Disabled
                };
                let shown = choice.shown(custom, dialog);

                if combo(&mut canvas, c, (&shown, able)) && listing.is_none() {
                    opened = Some(id);
                }
            }
            (setup::ACCEPT | setup::CANCEL, _) => {
                let button = if id == setup::ACCEPT {
                    Button::Accept
                } else {
                    Button::Cancel
                };

                if canvas.button(Pack::Expansion, c, Able::Enabled) {
                    pressed = Some(Press::Button(button));
                }
            }
            (setup::EDIT_MENU, _) => {
                if canvas.button(Pack::Expansion, c, Able::Enabled) {
                    pressed = Some(Press::Button(Button::EditMenu));
                }
            }
            (setup::LOAD | setup::SAVE, _) => {
                let button = if id == setup::LOAD {
                    Button::LoadOptions
                } else {
                    Button::SaveOptions
                };

                if canvas.button(Pack::Expansion, c, Able::Enabled) {
                    pressed = Some(Press::Button(button));
                }
            }
            (setup::MAP_NUMBER, _) => {
                ground(&mut canvas, c);
                map_number(&mut canvas, c, (custom, &mut state.typed));
            }
            (setup::SAVE_NAME, _) => {
                let name = custom.name.as_deref().unwrap_or(&lists.save_name);

                boxed(&mut canvas, c, Some(name));
            }
            (setup::DIFFICULTY, _) => boxed(&mut canvas, c, custom.difficulty(lists)),
            (setup::GOAL_TEXT, _) => {
                boxed(&mut canvas, c, None);

                if let Some(why) = failed {
                    line(&mut canvas, c, (why, Color32::RED));
                }
            }
            (_, _) if blurb.is_some() => {
                boxed(&mut canvas, c, None);
                canvas.note(c, blurb.unwrap_or_default());
            }
            _ => boxed(&mut canvas, c, c.text.as_deref()),
        }
    }

    // RANDOM stands where ALLOW CHEATS does: alone only.
    if let Some(lan) = lan {
        let ids = &super::lan::SETUP_CHAT;

        // Why the lobby stopped, above the chat's lines.
        let notes: Vec<String> = lan.stopped().map(str::to_owned).into_iter().collect();

        super::lan::chat(&mut canvas, layout, ids, (lan, &notes), &book.words);
    } else if random(&mut canvas, layout) {
        custom.map_number = preset::map_number(freestyle_clock()).max(MAP_NUMBER_MIN);
        state.typed = None;
    }

    state.open = opened.map(|id| (id, 0.0)).or(state.open);

    // The open list over everything, the page under it taking no press.
    state.open = state.open.and_then(|open| {
        let fresh = if opened.is_some() {
            Fresh::Opened
        } else {
            Fresh::Kept
        };

        open_list(&mut canvas, layout, (custom, dialog), (open, fresh), size.y)
    });

    pressed.or_else(|| keyed(ctx, layout, state))
}

/// Whether a list opened this pass or was open before.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fresh {
    Opened,
    Kept,
}

/// Combo box `id`'s list `open`, scrolled `first` rows, drawn over the
/// page above `bottom`: a pick written to `custom`; still open, how far
/// it is scrolled. Just opened, it starts at the pick.
fn open_list(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (custom, dialog): (&mut Custom, Dialog<'_>),
    ((id, first), fresh): ((i32, f32), Fresh),
    bottom: f32,
) -> Option<(i32, f32)> {
    let entry = CHOICES.iter().find(|(at, ..)| *at == id);
    let (&(_, choice, _), c) = entry.zip(layout.control(id))?;
    let names = choice.names(custom, dialog);
    let at = choice.at(custom, dialog);
    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let mut first = match fresh {
        Fresh::Opened => at.unwrap_or_default() as f32,
        Fresh::Kept => first,
    };
    // The scroll bar an open list takes: the dialog's own, the chat's
    // (`INDs` 1000, its arrows 25 wide as `INcb`'s). **Inferred**: the
    // combo box's own, its set plus 4 (`INcb` 1004), does not ship, and
    // what the original draws for it is not read.
    let bar = layout.control(menu::ids::SCROLL_BAR).and_then(|b| b.art);
    let listed = on_top(canvas, |canvas| {
        list(canvas, (c, bar), (&names, at), (&mut first, bottom))
    });

    match listed {
        Listed::Open => Some((id, first)),
        Listed::Closed => None,
        Listed::Picked(index) => {
            choice.pick(custom, dialog, index);
            None
        }
    }
}

/// Accept or Cancel by its hot key, A or Escape, while no list is open
/// and no box has the keys.
fn keyed(ctx: &egui::Context, layout: &Layout, state: &Setup) -> Option<Press> {
    let free = state.open.is_none() && ctx.memory(|m| m.focused().is_none());
    let buttons = [Button::Accept, Button::Cancel];

    free.then(|| paint::hot(layout, (&buttons, control), &paint::keys(ctx)))
        .flatten()
        .map(Press::Button)
}

/// The control's id that is `button` on the page.
const fn control(button: Button) -> i32 {
    match button {
        Button::Cancel => setup::CANCEL,
        _ => setup::ACCEPT,
    }
}

/// The clock RANDOM rolls on, as the pick page's.
fn freestyle_clock() -> i32 {
    super::freestyle::clock()
}

/// A layout rectangle in whole pixels, its edges rounded.
#[allow(clippy::cast_possible_truncation, reason = "a page's pixels")]
pub(super) fn whole(r: Rect) -> smnu::Rect {
    let [x0, y0, x1, y1] = [r.min.x, r.min.y, r.max.x, r.max.y].map(|n| n.round() as i32);

    smnu::Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

/// Control `c`'s ground, bevel and `text` centred in its font, shrunk
/// to fit (`Canvas::fitted`), in its colour or the headings'.
pub(super) fn boxed(canvas: &mut Canvas<'_>, c: &ShownControl, text: Option<&str>) {
    ground(canvas, c);

    if let Some(text) = text {
        let colour = c
            .colours
            .first()
            .copied()
            .and_then(font::colour)
            .unwrap_or(HEADING);

        canvas.fitted(c, text.trim(), colour);
    }
}

/// Control `c`'s ground, as its text box draws it: its picture tiled or
/// once, none when bare; then its bevel.
/// `INTI` 1016, all holes, stands for none: the box is filled in colour
/// role 0, black where the panel sets none (item colour's
/// `0x80000000`). **Inferred** from the capture, whose difficulty box is
/// black while a combo box's field, its picture missing, is not filled.
pub(super) fn ground(canvas: &mut Canvas<'_>, c: &ShownControl) {
    let art = c.art.filter(|_| c.style & style::BARE == 0);

    if let (Some((record, set)), Some(r)) = (art, c.rect) {
        let rect = fit::rect(r);
        let pic = Pic::of(record, set);

        if (record, set) == NO_ART {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(rect), 0.0, Color32::BLACK);
        } else if c.style & style::TILED == 0 {
            canvas.picture(Pack::Expansion, pic, rect.min);
        } else {
            canvas.tiled(Pack::Expansion, pic, rect);
        }
    }

    if let Some(r) = c.rect {
        bevel(canvas, r, c.style);
    }
}

/// The bevel `style` draws round `r`, if any: each strip a shade over
/// what is drawn (`bevel::Bevel::strips`).
fn bevel(canvas: &Canvas<'_>, r: smnu::Rect, style: i32) {
    if let Some(bevel) = Bevel::of(style) {
        let painter = canvas.painter();

        for (s, shade) in bevel.strips(r.corners()) {
            #[allow(clippy::cast_precision_loss, reason = "a page's pixels")]
            let strip = Rect::from_min_max(
                pos2(s[0] as f32, s[1] as f32),
                pos2(s[2] as f32, s[3] as f32),
            );

            painter.rect_filled(canvas.fit.rect(strip), 0.0, tint(shade));
        }
    }
}

/// A shade as egui blends it over what is drawn: a darkening black at
/// soft shadows' share ([`soft::veil`]), egui blending on linear colour.
/// **Inferred**: added steps are grey added, each channel as far as the
/// value would move.
fn tint(shade: Shade) -> Color32 {
    let byte = |n: u32| u8::try_from(n).unwrap_or(u8::MAX);

    match shade {
        Shade::Keep(_) => {
            Color32::from_black_alpha(soft::veil(shade, Blending::Linear).unwrap_or(0))
        }
        Shade::Raise(steps) => {
            let v = byte(steps * 255 / (LEVELS - 1));

            Color32::from_rgba_premultiplied(v, v, v, 0)
        }
    }
}

/// `text` on one line in control `c`'s font, `PAD` in from its left,
/// cut at its right.
pub(super) fn line(canvas: &mut Canvas<'_>, c: &ShownControl, (text, colour): (&str, Color32)) {
    if let (Some(r), Some(font)) = (c.rect, c.font.and_then(|f| canvas.ink.font(f))) {
        let rect = fit::rect(r);
        let pen = pos2(
            rect.min.x + PAD,
            rect.center().y + font.cap(canvas.fit.scale) / 2.0,
        );
        let painter = canvas.painter().with_clip_rect(canvas.fit.rect(rect));

        font.draw(&painter, canvas.fit, pen, &font::spans(text, colour));
    }
}

/// Combo box `c` showing `shown`: its field sunken with the text, its
/// arrow in its state at the right; whether it was
/// pressed.
pub(super) fn combo(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (shown, able): (&str, Able),
) -> bool {
    let (Some(r), Some((record, set))) = (c.rect, c.art) else {
        return false;
    };
    let rect = fit::rect(r);
    let arrow = Pic::of(record, set + setup::ARROW);
    let wide = canvas
        .size(Pack::Expansion, arrow)
        .map_or(ARROW_WIDTH, |s| s.x);
    let field = Rect::from_min_max(rect.min, pos2(rect.max.x - wide, rect.max.y));
    let response = canvas.ui.interact(
        canvas.fit.rect(rect),
        canvas.ui.id().with(("combo", c.id)),
        Sense::click(),
    );
    let (_, hover, press) = crate::panel::sheet::look(&response, Tick::Clear);
    let state = match able {
        Able::Enabled => State::of(c.flags).pressed(press),
        Able::Disabled => State::Disabled,
    };
    let count = (0..art::SLOTS)
        .filter(|&s| canvas.size(Pack::Expansion, arrow.slot(s)).is_some())
        .max()
        .map_or(0, |s| s + 1);
    let slot = menu::drawn_slot((Tick::Clear, hover), state, count);
    let colour = paint::pick(c, state);

    bevel(canvas, whole(field), c.style);
    line(
        canvas,
        &ShownControl {
            rect: Some(whole(field)),
            ..c.clone()
        },
        (shown, colour),
    );
    canvas.picture(
        Pack::Expansion,
        arrow.slot(slot),
        pos2(field.max.x, rect.min.y),
    );

    able == Able::Enabled && response.clicked()
}

/// The Map Number's box `c`: the number typed, digits alone, kept as
/// typed while the box has the keys; a number past `i32::MAX` held
/// there, 0 made 1 as Accept makes it.
fn map_number(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    (custom, typed): (&mut Custom, &mut Option<String>),
) {
    let mut text = typed
        .clone()
        .unwrap_or_else(|| custom.map_number.to_string());
    let response = c.rect.and_then(|r| {
        canvas.typed(
            (fit::rect(r), c.font, paint::pick(c, State::Normal)),
            (&mut text, MAP_DIGITS, egui::Align::Center),
            ("map number", Inset::Edges),
        )
    });

    if let Some(response) = response {
        if response.changed() {
            text.retain(|ch| ch.is_ascii_digit());

            if let Ok(n) = text.parse::<i64>() {
                custom.map_number = i32::try_from(n).unwrap_or(i32::MAX).max(MAP_NUMBER_MIN);
            }

            *typed = Some(text);
        } else if !response.has_focus() {
            *typed = None;
        }
    }
}

/// RANDOM, as LOAD's text on the SEND CHAT bar; whether it was pressed.
fn random(canvas: &mut Canvas<'_>, layout: &Layout) -> bool {
    let shown = layout.control(setup::LOAD).map(|load| ShownControl {
        id: Some(RANDOM),
        rect: Some(RANDOM_AT),
        art: Some(RANDOM_ART),
        text: Some(RANDOM_TEXT.to_owned()),
        hotkey: None,
        ..load.clone()
    });

    shown.is_some_and(|c| canvas.button(Pack::Expansion, &c, Able::Enabled))
}

/// `draw` on a layer over the page's, the page taking no press under it.
pub(super) fn on_top<T>(canvas: &mut Canvas<'_>, draw: impl FnOnce(&mut Canvas<'_>) -> T) -> T {
    let ctx = canvas.ui.ctx().clone();
    let layer = LayerId::new(Order::Foreground, Id::new("setup list"));
    let top = egui::Ui::new(
        ctx.clone(),
        Id::new("setup list"),
        UiBuilder::new()
            .layer_id(layer)
            .max_rect(ctx.viewport_rect()),
    );
    let page = std::mem::replace(&mut canvas.ui, top);
    let drawn = draw(canvas);

    canvas.ui = page;
    drawn
}

/// Combo box `c`'s list open under it, `names` its rows, `at` the pick:
/// the list's texture (`INcb` 1003) tiled, sunken, as many rows as fit
/// above `bottom`, scrolled `scroll` rows by the wheel and, where rows
/// are left over, scroll bar `bar` at its right; the row under the
/// pointer lit, else the pick. A press on a row picks it, one off the
/// list or Escape closes it.
///
/// ```text
///  [Ravenswood     ][v]
///  | K'Hal-Kratis  |^|
///  | Dragon Tooth  |#|   rows    bar: the arrow's column
///  | Isle of ...   |v|
/// ```
pub(super) fn list(
    canvas: &mut Canvas<'_>,
    (c, bar): (&ShownControl, Option<(FourCc, i32)>),
    (names, at): (&[String], Option<usize>),
    (scroll, bottom): (&mut f32, f32),
) -> Listed {
    let (Some(r), Some((record, set)), Some(font)) =
        (c.rect, c.art, c.font.and_then(|f| canvas.ink.font(f)))
    else {
        return Listed::Closed;
    };
    let combo = fit::rect(r);
    #[allow(clippy::cast_precision_loss, reason = "a bevel's pixels")]
    let edge = regent_core::view::menu::bevel::DEPTH as f32;
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "rows that fit"
    )]
    let room = ((bottom - combo.max.y - 2.0 * edge) / font.pitch)
        .floor()
        .max(1.0) as usize;
    let rows = names.len().min(room).max(1);
    #[allow(clippy::cast_precision_loss, reason = "a few rows")]
    let rect = Rect::from_min_size(
        pos2(combo.min.x, combo.max.y),
        egui::vec2(combo.width(), rows as f32 * font.pitch + 2.0 * edge),
    );
    let window = canvas.ui.max_rect();
    let outside = canvas
        .ui
        .interact(window, canvas.ui.id().with("off the list"), Sense::click());
    let inside = canvas.ui.interact(
        canvas.fit.rect(rect),
        canvas.ui.id().with("list"),
        Sense::click(),
    );
    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let most = names.len().saturating_sub(rows) as f32;

    // The bar, where rows are left over: as wide as its arrow, down the
    // list's right side, where the combo box puts its scroll bar.
    let bar = bar.filter(|_| most > 0.0).and_then(|(record, set)| {
        let wide = canvas.size(Pack::Expansion, Pic::of(record, set))?.x;

        Some((
            (record, set),
            Rect::from_min_max(pos2(rect.max.x - wide, rect.min.y), rect.max),
        ))
    });
    let right = bar.map_or(rect.max.x - edge, |(_, b)| b.min.x);
    let wheel = paint::notches(canvas.ui.ctx());
    let pointer = inside
        .hover_pos()
        .map(|p| canvas.fit.back(p))
        .filter(|p| p.x < right);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a row under the pointer"
    )]
    let under = pointer.map(|p| ((p.y - rect.min.y - edge) / font.pitch).max(0.0) as usize);

    if inside.hovered() && wheel != 0.0 {
        *scroll -= wheel.signum();
    }

    *scroll = scroll.clamp(0.0, most);

    canvas.tiled(Pack::Expansion, Pic::of(record, set + setup::LIST), rect);
    bevel(canvas, whole(rect), c.style);

    if let Some((art, b)) = bar {
        let shown = ShownControl {
            id: Some(menu::ids::SCROLL_BAR),
            rect: Some(whole(b)),
            art: Some(art),
            ..c.clone()
        };
        let moved = canvas.scroll_bar(Pack::Expansion, &shown, (*scroll, most, 1.0));

        *scroll = (*scroll + moved).clamp(0.0, most);
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a list's rows"
    )]
    let first = scroll.round() as usize;
    let lit = under.filter(|u| *u < rows).map(|u| first + u).or(at);
    let colour = paint::pick(c, State::Normal);

    for (n, name) in names.iter().enumerate().skip(first).take(rows) {
        #[allow(clippy::cast_precision_loss, reason = "a few rows")]
        let top = rect.min.y + edge + (n - first) as f32 * font.pitch;
        let row = Rect::from_min_max(pos2(rect.min.x + edge, top), pos2(right, top + font.pitch));

        if lit == Some(n) {
            canvas
                .painter()
                .rect_filled(canvas.fit.rect(row), 0.0, paint::SELECTION);
        }

        line(
            canvas,
            &ShownControl {
                rect: Some(whole(row)),
                ..c.clone()
            },
            (name, colour),
        );
    }

    let escape = canvas.ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));

    match (inside.clicked(), under) {
        (true, Some(u)) if u < rows => Listed::Picked(first + u),
        _ if outside.clicked() || escape => Listed::Closed,
        _ => Listed::Open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::view::menu::{Hover, drawn_slot};

    #[test]
    fn allow_cheats_label_starts_beside_box() {
        let control = ShownControl {
            rect: Some(smnu::Rect {
                x: 386,
                y: 519,
                width: 142,
                height: 21,
            }),
            art: Some((FourCc(*b"INDk"), 1003)),
            text: Some("ALLOW CHEATS".to_owned()),
            ..ShownControl::default()
        };

        let (icon, label) = cheats_parts(&control);

        assert!(icon.text.is_none());
        assert_eq!(label.rect.unwrap().x, 410);
        assert!(label.art.is_none());
    }

    #[test]
    fn allow_cheats_toggles_off() {
        let mut custom = Custom::default();

        toggle_cheats(&mut custom);

        assert_eq!(cheats_look(&custom).1, Tick::Clear);

        toggle_cheats(&mut custom);
        assert_eq!(cheats_look(&custom).1, Tick::Checked);
    }

    /// ALLOW CHEATS's set in `MX17`, a check box.
    const CHEATS_ART: (FourCc, i32) = (FourCc(*b"INDk"), 1003);

    /// ALLOW CHEATS at rest shows its tick: its picture is not the
    /// clear box's.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn allow_cheats_draws_its_tick() {
        let catalogue = crate::testdata::hd();
        let (record, set) = CHEATS_ART;
        let (able, tick) = cheats_look(&Custom::default());
        let state = match able {
            Able::Enabled => State::Normal,
            Able::Disabled => State::Disabled,
        };
        let count = (0..art::SLOTS)
            .take_while(|&s| menu::state(catalogue, record, set, s).is_some())
            .count();
        let picture = |tick| {
            let slot = drawn_slot((tick, Hover::Away), state, count);
            let sprite = menu::state(catalogue, record, set, slot).expect("the slot");

            regent_draw::art::read::image(&sprite.expect("a sprite"))
        };

        assert_eq!(tick, Tick::Checked);
        assert_ne!(picture(tick), picture(Tick::Clear));
    }
}
