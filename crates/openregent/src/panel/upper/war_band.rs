//! A warriors' guild's band with a temple: two or three recruit buttons.

use bevy_egui::egui::{Color32, Rect, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::sim::game::Input;
use regent_core::sim::world::{offer::Button, recruit::Recruit};
use regent_core::view::menu::{Layout, ShownControl, Tick};

use super::band::{BAND, BAND_BUTTON, BAND_BUTTON_SET, FULL_TEXT, SMALL_FONT, recruit_tip};
use super::{Game, SHORT};
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::ink::{Pic, Tone};
use crate::panel::sheet::{self, Able, Purse};

/// The warriors' guild's recruits with a temple, each its button and
/// price control: the warrior of discord
/// and the paladin share the left half, the warrior the right.
const WAR_BUTTONS: [(FourCc, i32, i32); 3] = [
    (FourCc(*b"AVM1"), 5000, 5970),
    (FourCc(*b"AVF1"), 8008, 5970),
    (FourCc(*b"AVL1"), 8910, 8017),
];

/// The warrior of discord's label, `Warrior Discord`: `GMTX` 18.
const DISCORD_TEXT: usize = 18;

/// A three-recruit button's price box, pixels wide: room for "1000" in
/// `fnt7`; and its gap to the button's right edge.
const PRICE_ROOM: f32 = 23.0;
const PRICE_EDGE: f32 = 3.0;

/// A warriors' guild's recruits with a temple:
/// two in the original's half-band buttons, labels in their text
/// rectangles, prices in their boxes; a full guild's each `GMTX` 19, no
/// price. Three (both temples, a departure) split the band in thirds,
/// each price box narrowed at its right.
pub(super) fn war_band(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, guild): (&Game<'_>, u32),
    (recruits, full): (&[Button<FourCc>], bool),
    orders: &mut orders::Panel,
) {
    let text = &game.corpus.text;
    let shown: Vec<(&Button<FourCc>, &ShownControl, i32)> = WAR_BUTTONS
        .iter()
        .filter_map(|&(code, button, price)| {
            let b = recruits.iter().find(|b| b.what == code)?;

            Some((b, layout.control(button)?, price))
        })
        .collect();
    let thirds = shown.len() > 2;

    for (n, &(b, c, price)) in shown.iter().enumerate() {
        let label = if full {
            text.gmtx(FULL_TEXT).map(std::borrow::Cow::into_owned)
        } else if b.what == WAR_BUTTONS[0].0 {
            text.gmtx(DISCORD_TEXT).map(std::borrow::Cow::into_owned)
        } else {
            Some(orders.title(game.corpus, b.what))
        };
        let place = if thirds {
            Some(third(layout, n, price))
        } else {
            half(layout, c, price)
        };
        let short = !game.world.affords(game.local, b.price);
        let cost = (!full && b.price > 0).then(|| {
            let colour = sheet::price_ink(layout, price, Purse::of(short)).unwrap_or(SHORT);

            (b.price.to_string(), colour)
        });

        let button = Button {
            what: (),
            price: b.price,
            ready: b.ready,
        };
        let c = &ShownControl {
            tooltip: recruit_tip(c, text, (button, game.world.members(guild))),
            ..c.clone()
        };

        if let Some(place) = place
            && war_button(
                canvas,
                c,
                place,
                (label.as_deref(), cost),
                Able::of(b.ready),
            )
        {
            orders.orders.outbox.push(Input::Recruit(Recruit {
                guild,
                code: b.what,
            }));
        }
    }
}

/// Where a recruit button draws, panel pixels.
#[derive(Clone, Copy)]
struct WarPlace {
    /// The button and its picture (none: the band's wide one stretched).
    button: (Rect, Option<(FourCc, i32)>),
    /// Its label's rectangle.
    words: Rect,
    /// Its price's box and the box's picture.
    cost: (Rect, Option<(FourCc, i32)>),
}

/// Button `n` of three across the band, its price box narrowed at its
/// right.
fn third(layout: &Layout, n: usize, price: i32) -> WarPlace {
    #[allow(clippy::cast_precision_loss, reason = "three buttons")]
    let (wide, left) = (BAND.width() / 3.0, n as f32);
    let r = Rect::from_min_size(
        pos2(BAND.left() + wide * left, BAND.top()),
        vec2(wide, BAND.height()),
    );
    let price = layout.control(price);
    let tall = price
        .and_then(sheet::rect)
        .map_or(r.height(), |b| b.height());
    let cost = Rect::from_center_size(
        pos2(r.right() - PRICE_ROOM / 2.0 - PRICE_EDGE, r.center().y),
        vec2(PRICE_ROOM, tall),
    );

    WarPlace {
        button: (r, None),
        words: Rect::from_min_max(r.min, pos2(cost.left(), r.bottom())),
        cost: (cost, price.and_then(|p| p.art)),
    }
}

/// Half-band control `c` where the panel puts it: its text rectangle,
/// its price control `price`, their pictures.
fn half(layout: &Layout, c: &ShownControl, price: i32) -> Option<WarPlace> {
    let r = sheet::rect(c)?;
    #[allow(clippy::cast_precision_loss, reason = "panel pixels")]
    let words = c.text_rect.map_or(r, |t| {
        Rect::from_min_size(
            r.min + vec2(t.x as f32, t.y as f32),
            vec2(t.width as f32, t.height as f32),
        )
    });
    let price = layout.control(price);

    Some(WarPlace {
        button: (r, c.art),
        words,
        cost: (
            price.and_then(sheet::rect).unwrap_or(r),
            price.and_then(|p| p.art),
        ),
    })
}

/// Recruit control `c` drawn at `place`: its picture in its state, `label`
/// wrapped in its font, the price in its box; whether it was pressed, by
/// click or hot key.
fn war_button(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    place: WarPlace,
    (label, price): (Option<&str>, Option<(String, Color32)>),
    able: Able,
) -> bool {
    let (r, art) = place.button;
    let response = canvas.press(r, ("war band", c.id));
    let pic = art.map_or(Pic::of(BAND_BUTTON, BAND_BUTTON_SET), |(record, set)| {
        Pic::of(record, set)
    });
    let look = sheet::look(&response, Tick::Clear);
    let (slot, tone) = sheet::slot(canvas, pic, look, able.state(c.flags));

    match art {
        Some(_) => {
            canvas.picture(pic.slot(slot), tone, r.min);
        }
        None => {
            canvas.stretched(pic.slot(slot), tone, r);
        }
    }

    if let (Some(label), Some(font)) = (label, c.font) {
        let state = able.state(c.flags).pressed(look.2);
        let markup = sheet::marked(c, label, state);

        sheet::wrapped(canvas, font, place.words, &markup, sheet::ink(c, state));
    }

    if let Some((price, colour)) = price {
        let (cost, box_art) = place.cost;

        if let Some((record, set)) = box_art {
            canvas.stretched(Pic::of(record, set), Tone::Plain, cost);
        }

        canvas.text(SMALL_FONT, cost, &price, (colour, Align::Centre));
    }

    Canvas::tip(&response, c.tooltip.as_deref());

    let key = crate::input::hotkey::hit_in(canvas.ui, c.hotkey, able);

    able == Able::Enabled && response.clicked() || key
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three recruits (both temples) tile the band, each label in two
    /// lines at most ("Warrior Discord") and each price in its box.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn three_recruits_fit_their_thirds_of_the_band() {
        let layout = Layout {
            id: FourCc(*b"AP52"),
            panel: ShownControl::default(),
            centred: false,
            style: None,
            controls: vec![],
        };
        let places: Vec<WarPlace> = (0..3)
            .map(|n| third(&layout, n, WAR_BUTTONS[0].2))
            .collect();
        let tiled = places
            .iter()
            .map(|p| p.button.0)
            .reduce(Rect::union)
            .expect("three");

        assert_eq!(tiled, BAND);

        crate::panel::canvas::tests::painted_textures(|c| {
            for p in &places {
                let lines = c.lines(SMALL_FONT, p.words.width(), "Warrior Discord");

                assert!(lines <= 2, "{lines} lines in {:?}", p.words);
                assert!(c.pitch(SMALL_FONT) * 2.0 <= p.words.height());
                assert!(c.width(SMALL_FONT, "1000") <= p.cost.0.width());
                assert!(BAND.contains_rect(p.cost.0) && BAND.contains_rect(p.words));
            }
        });
    }
}
