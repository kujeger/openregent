//! A box over the game in the message box's art, `AP11`, made as tall as
//! what it says: its frame's well cut or repeated (`menu::cut::rows`),
//! its title in 8005, lines, rules and rows in the well, and OK's
//! button, 5503, below where one is asked. The LAN game's held box
//! (`held`) and the stopped game's (`stopped`) say theirs in it.
//! openregent's own: the original's box keeps one size.
//!
//! ```text
//!        ╭───◆───╮
//!  ┌──[ Drop Player? ]──┐   8005, title colour
//!  │ Haven't heard from │   lines, centred and evened out
//!  │ these players ...  │
//!  │ ────────────────── │   rules
//!  │ Ann     [Drop Player]  rows: a name, its countdown under or
//!  │ drops in 55 s      │   beside it, a button in YES's art
//!  │ ────────────────── │
//!  │    [ Main Menu ]   │   OK's button, where asked
//!  └────────◆───────────┘
//! ```

use bevy_egui::egui::{self, Color32, Pos2, Rect, Vec2, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu;
use regent_core::view::menu::{Layout, MESSAGE, ShownControl, message};
use regent_draw::art::Art;
use regent_draw::data::Corpus;

use crate::menu::cut::{self, MESSAGE_BAND, MESSAGE_LEAST, MESSAGE_WELL_END};
use crate::menu::fit::Fit;
use crate::menu::font;
use crate::panel::Panels;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::ink::Pic;
use crate::panel::sheet::{self, Able};
use crate::ui::layout::Frame;

/// Between the well's sides and what it says, pixels: openregent's own.
const PAD: f32 = 10.0;

/// A row's height, pixels: YES's button, 28 high, and room.
const ROW: f32 = 34.0;

/// A rule's height, pixels, its line on the third.
const RULE: f32 = 5.0;

/// A rule's colour: the frame's dark gold. A guess.
const RULE_INK: Color32 = Color32::from_rgb(112, 92, 52);

/// A rule's ends in from the text box's sides, pixels.
const RULE_INSET: f32 = 8.0;

/// Between a row's button and the text box's right side, and between
/// its name and its countdown on one line, pixels.
const ROW_GAP: f32 = 8.0;

/// How far in from its rectangle's left `Canvas::text` sets a line,
/// pixels (`panel::canvas`'s `PAD`).
const TEXT_PAD: f32 = 4.0;

/// The box's top below the map's, HD pixels: openregent's own.
const BELOW_TOP: f32 = 11.0;

/// A row's button's press ids: past any of `AP11`'s.
const ROW_PRESS: i32 = 9000;

/// The colour a line takes: the title's, or the text's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Strong,
    Plain,
}

/// What the box says, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// Lines, wrapped and evened out (`font::balanced`).
    Text(String, Ink),
    /// Pixels of nothing.
    Gap(f32),
    Rule,
    /// A name, what is said of it (in the title's colour), and its
    /// button's words where it has one.
    Row {
        name: String,
        said: String,
        press: Option<String>,
    },
}

/// A box: its title, what it says, and OK's button's words where it
/// shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub title: String,
    pub items: Vec<Item>,
    pub button: Option<String>,
}

/// What was pressed: row `n`'s button, or the bottom one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pressed {
    Row(usize),
    Button,
}

/// Where the box stands over the map: centred across it, at its top
/// or in its middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Top,
    Middle,
}

/// The game's catalogue, its pictures and the panels' fonts.
pub struct Means<'a> {
    pub panels: &'a mut Panels,
    pub art: &'a mut Art,
    pub corpus: &'a Corpus,
}

/// `notice` drawn over the game at `place`, on a layer of its own told
/// apart by `salt`: what was pressed, and where the box is, window
/// points; `None` without `AP11`.
pub fn show(
    ctx: &egui::Context,
    means: Means<'_>,
    notice: &Notice,
    (place, salt): (Place, &'static str),
) -> Option<(Option<Pressed>, Rect)> {
    let Means {
        panels,
        art,
        corpus,
    } = means;
    let layout = panels.layout(&corpus.catalogue, &corpus.text, MESSAGE)?;
    let window = ctx.viewport_rect();
    let frame = Frame::of(window.size());
    let mut ui = egui::Ui::new(
        ctx.clone(),
        salt.into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::new(egui::Order::Foreground, salt.into()))
            .max_rect(window),
    );
    let mut canvas = Canvas {
        ui: &mut ui,
        ink: &mut panels.ink,
        art,
        source: corpus.source(),
        catalogue: &corpus.catalogue,
        fit: Fit {
            scale: frame.scale,
            origin: Pos2::ZERO,
        },
        salt,
    };
    let laid = Laid::of(&mut canvas, &layout, notice)?;
    let size = vec2(laid.width, laid.height()) * frame.scale;
    let top = match place {
        Place::Top => frame.map.min.y + BELOW_TOP * frame.scale,
        Place::Middle => frame.map.center().y - size.y / 2.0,
    };

    canvas.fit.origin = pos2(frame.map.center().x - size.x / 2.0, top);

    let pressed = draw(&mut canvas, &layout, notice, &laid);

    Some((pressed, Rect::from_min_size(canvas.fit.origin, size)))
}

/// The box measured: its frame's size and change, its text box, each
/// item's height and lines' width.
struct Laid {
    width: f32,
    whole: f32,
    /// How much taller than its frame the box is, pixels; less than 0
    /// shorter.
    grow: f32,
    text: Rect,
    /// The well's height the items are centred in, the change left out.
    room: f32,
    /// Each item's height, and the width its lines wrap to.
    items: Vec<(f32, f32)>,
}

impl Laid {
    /// `notice` measured in `layout`'s controls; `None` without its
    /// frame, text box or font.
    fn of(canvas: &mut Canvas<'_>, layout: &Layout, notice: &Notice) -> Option<Laid> {
        let frame = sheet::bounds(layout);
        let text_box = layout.control(message::TEXT)?;
        let (text, record) = (sheet::rect(text_box)?, text_box.font?);
        let wide = text.width() - 2.0 * PAD;
        let pitch = canvas.pitch(record);
        let items: Vec<(f32, f32)> = notice
            .items
            .iter()
            .map(|item| match item {
                Item::Text(said, _) => {
                    let mut lines = |w: f32| canvas.lines(record, w, said);
                    let width = font::balanced(wide, &mut lines);

                    #[allow(clippy::cast_precision_loss, reason = "a few lines")]
                    let tall = lines(width) as f32 * pitch;

                    (tall, width)
                }
                Item::Gap(tall) => (*tall, wide),
                Item::Rule => (RULE, wide),
                Item::Row { .. } => (ROW, wide),
            })
            .collect();
        let room = match notice.button {
            Some(_) => text.height(),
            None => MESSAGE_WELL_END - text.top(),
        };
        let need = items.iter().map(|(h, _)| h).sum::<f32>() + 2.0 * PAD;

        Some(Laid {
            width: frame.width(),
            whole: frame.height(),
            grow: grow(need, room, frame.height()),
            text,
            room,
            items,
        })
    }

    fn height(&self) -> f32 {
        self.whole + self.grow
    }
}

/// How much taller than its frame, `whole` high, a box is whose items
/// `need` pixels where its well holds `room`: no shorter than
/// [`MESSAGE_LEAST`], to a whole pixel.
fn grow(need: f32, room: f32, whole: f32) -> f32 {
    (need - room).max(MESSAGE_LEAST - whole).round()
}

/// `notice` drawn as `laid`: frame, title, items, button; what was
/// pressed.
fn draw(canvas: &mut Canvas<'_>, layout: &Layout, notice: &Notice, laid: &Laid) -> Option<Pressed> {
    let whole = Rect::from_min_size(Pos2::ZERO, vec2(laid.width, laid.height()));

    // Nothing under the box takes the pointer: made before its buttons.
    canvas.press(whole, "block");

    if let Some((record, set)) = layout.panel.art {
        let strips = cut::rows(laid.whole, MESSAGE_BAND, laid.height());

        canvas.strips(Pic::of(record, set), Pos2::ZERO, &strips);
    }

    sheet::fill(
        canvas,
        layout,
        message::TITLE,
        &notice.title,
        (None, Align::Centre),
    );

    let total: f32 = laid.items.iter().map(|(h, _)| h).sum();
    let mut y = laid.text.top() + (laid.room + laid.grow - total) / 2.0;
    let mut pressed = None;

    for (n, (item, &(tall, width))) in notice.items.iter().zip(&laid.items).enumerate() {
        let rect = Rect::from_min_size(pos2(laid.text.left(), y), vec2(laid.text.width(), tall));

        if item_drawn(canvas, layout, (item, n), (rect, width)) {
            pressed = Some(Pressed::Row(n));
        }

        y += tall;
    }

    // OK's button, moved with the frame's bottom.
    let button = notice.button.as_deref().zip(layout.control(message::OK));

    if let Some((words, c)) = button
        && sheet::button(
            canvas,
            &moved(c, vec2(0.0, laid.grow), None),
            Some(words),
            Able::Enabled,
        )
    {
        pressed = Some(Pressed::Button);
    }

    pressed
}

/// Item `n` drawn in panel rectangle `rect`, its lines `width` wide;
/// whether its button was pressed.
fn item_drawn(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (item, n): (&Item, usize),
    (rect, width): (Rect, f32),
) -> bool {
    let text_box = layout.control(message::TEXT);
    let record = text_box.and_then(|c| c.font);
    let strong = layout
        .control(message::TITLE)
        .map_or(Color32::WHITE, sheet::colour);
    let plain = text_box.map_or(Color32::WHITE, sheet::colour);

    match (item, record) {
        (Item::Text(said, ink), Some(record)) => {
            let colour = match ink {
                Ink::Strong => strong,
                Ink::Plain => plain,
            };
            let lines = Rect::from_center_size(rect.center(), vec2(width, rect.height()));

            canvas.paragraphs(record, lines, said, (colour, Align::Centre, 0.0));
            false
        }
        (Item::Rule, _) => {
            let line = Rect::from_min_size(
                pos2(rect.left() + RULE_INSET, rect.top() + 2.0),
                vec2(rect.width() - 2.0 * RULE_INSET - 1.0, 1.0),
            );

            canvas.fill(line, RULE_INK);
            false
        }
        (Item::Row { name, said, press }, Some(record)) => {
            let yes = layout.control(message::YES);
            let button = yes.zip(press.as_deref()).and_then(|(c, words)| {
                let size = sheet::rect(c)?.size();
                let at = pos2(
                    rect.right() - ROW_GAP - size.x,
                    rect.center().y - size.y / 2.0,
                );

                Some((c, words, at))
            });
            let column = Rect::from_min_max(
                pos2(rect.left() + PAD, rect.top()),
                pos2(
                    button.map_or(rect.right() - PAD, |(_, _, at)| at.x - ROW_GAP),
                    rect.bottom(),
                ),
            );

            row_text(canvas, record, column, (name, said), (plain, strong));

            button.is_some_and(|(c, words, at)| {
                let id = ROW_PRESS + i32::try_from(n).unwrap_or_default();
                let by = sheet::rect(c).map_or(Vec2::ZERO, |r| at - r.min);

                sheet::button(canvas, &moved(c, by, Some(id)), Some(words), Able::Enabled)
            })
        }
        _ => false,
    }
}

/// A row's name and what is said of it in panel rectangle `column`, in
/// font `record`: on one line where both fit, the name in `plain` and
/// the rest in `strong`; else the one over the other.
///
/// ```text
///  Ann  drops in 55 s       or      Ann
///                                   drops in 55 s
/// ```
fn row_text(
    canvas: &mut Canvas<'_>,
    record: FourCc,
    column: Rect,
    (name, said): (&str, &str),
    (plain, strong): (Color32, Color32),
) {
    let pitch = canvas.pitch(record);
    let named = canvas.width(record, name);
    let wide = named + ROW_GAP + canvas.width(record, said);
    // `Canvas::text` sets a line `PAD` in from its rectangle's left.
    let room = column.width() - 2.0 * TEXT_PAD;

    if wide <= room {
        let line = Rect::from_center_size(column.center(), vec2(column.width(), pitch));
        let rest = Rect::from_min_max(pos2(line.left() + named + ROW_GAP, line.top()), line.max);

        canvas.text(record, line, name, (plain, Align::Left));
        canvas.text(record, rest, said, (strong, Align::Left));
    } else {
        let top = column.center().y - pitch;
        let first = Rect::from_min_size(pos2(column.left(), top), vec2(column.width(), pitch));

        canvas.text(record, first, name, (plain, Align::Left));
        canvas.text(
            record,
            first.translate(vec2(0.0, pitch)),
            said,
            (strong, Align::Left),
        );
    }
}

/// Control `c` moved `by` pixels, its id `id` where given, with no hot
/// key: a box's buttons answer a click alone, so no key typed in the
/// chat line presses them.
fn moved(c: &ShownControl, by: Vec2, id: Option<i32>) -> ShownControl {
    #[allow(clippy::cast_possible_truncation, reason = "a box's pixels")]
    let rect = c.rect.map(|r| smnu::Rect {
        x: r.x + by.x.round() as i32,
        y: r.y + by.y.round() as i32,
        ..r
    });

    ShownControl {
        rect,
        id: id.or(c.id),
        hotkey: None,
        ..c.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AP11`'s frame and its text box's height.
    const WHOLE: f32 = 323.0;
    const TEXT: f32 = 169.0;

    #[test]
    fn a_short_box_is_cut_to_its_least() {
        assert_eq!(grow(0.0, TEXT, WHOLE), MESSAGE_LEAST - WHOLE);
        assert_eq!(grow(40.0, TEXT, WHOLE), 40.0 - TEXT);
    }

    #[test]
    fn a_box_grows_with_what_it_says() {
        assert_eq!(grow(TEXT + 68.0, TEXT, WHOLE), 68.0);
        assert_eq!(grow(TEXT, TEXT, WHOLE), 0.0);
    }

    #[test]
    fn a_box_grows_by_whole_pixels() {
        assert_eq!(grow(TEXT + 10.4, TEXT, WHOLE), 10.0);
    }
}
