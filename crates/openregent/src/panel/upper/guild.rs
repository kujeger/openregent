//! Guild actions on a building's second row: the rangers' camp move,
//! the warriors' Call to Arms, the Temple to Krolm's Rage.

use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::sim::game::orders as sends;
use regent_core::sim::world::guild::rage::Rage;
use regent_core::view::menu::{Layout, ShownControl};
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::id;

use super::{COUNT_BOX, Game, WORKING, press, priced};
use crate::input::orders::{self, Mode};
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet::{self, Able, Purse};

/// The rangers' guild's camp move, its second button.
pub(super) fn camp(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    game: &Game<'_>,
    id: u32,
    orders: &mut orders::Panel,
) {
    let moving = orders.orders.mode == Mode::Camp(id);
    let offer = game
        .world
        .camp_offer(game.program, game.local, id)
        .filter(|_| !moving);

    if let (Some(c), Some(b)) = (layout.control(id::SECOND), offer)
        && c.text.as_deref().is_some_and(|t| t.contains("CAMP"))
    {
        let short = !game.world.affords(game.local, b.price);
        let colour = sheet::price_ink(layout, CAMP_PRICE, Purse::of(short));
        let pressed = sheet::button(
            canvas,
            &priced(c, &game.corpus.text, b.price),
            None,
            Able::of(b.ready),
        );

        sheet::fill(
            canvas,
            layout,
            CAMP_PRICE,
            &b.price.to_string(),
            (colour, Align::Centre),
        );

        // Paid as it begins (`Sim::camp_begin`).
        if pressed {
            orders.orders.camp_begun = Some((id, b.price));
            orders.orders.mode = Mode::Camp(id);
        }
    }
}

/// The camp move's price on the rangers' guild's panel (`AP47`).
const CAMP_PRICE: i32 = 4485;

/// The warriors' guild's Call to Arms (`AP52` 8009, hot key C): enabled
/// while the guild has a living member, its price in 4481, red over the
/// treasury. A press sends what the library's
/// `orders::assemble` says: its price and the call, or `GMTX` 37.
pub(super) fn assembly(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, id): (&Game<'_>, u32),
    orders: &mut orders::Panel,
    text: &Text,
) {
    let offer = game
        .world
        .assembly_offer(game.program, game.local, id)
        .filter(|_| layout.id == WAR_PANEL);

    if let (Some(c), Some(b)) = (layout.control(id::SECOND), offer) {
        let short = !game.world.affords(game.local, b.price);
        let colour = sheet::price_ink(layout, ASSEMBLY_PRICE, Purse::of(short));
        // `WarGuildController`: the price after the tip, or why not.
        let tooltip = if b.ready {
            c.tooltip
                .as_deref()
                .map(|own| tip::appended(text, own, tip::line::PRICE, &[b.price.into()]))
        } else {
            tip::said(text, tip::line::NO_CALL)
        };
        let shown = ShownControl {
            tooltip,
            ..c.clone()
        };
        let pressed = sheet::button(canvas, &shown, None, Able::of(b.ready));

        if let Some(p) = layout.control(ASSEMBLY_PRICE) {
            sheet::picture(canvas, p);
        }

        sheet::fill(
            canvas,
            layout,
            ASSEMBLY_PRICE,
            &b.price.to_string(),
            (colour, Align::Centre),
        );

        if pressed {
            press(orders, sends::assemble(game.world, game.local, id, b.price));
        }
    }
}

/// The warriors' guild's panel.
const WAR_PANEL: FourCc = FourCc(*b"AP52");

/// The Call to Arms' price box on it.
const ASSEMBLY_PRICE: i32 = 4481;

/// The Temple to Krolm's Rage (`AP24`, rage panel state): 8009
/// RAGE OF KROLM, hot key K, and its price 4484, red under the treasury,
/// enabled while the crown has a hero; while one runs, the bar 8201 of
/// its length and 8826 over it, the button hidden. A press sends what the
/// library's `orders::rage` says: its price and the Rage, or `GMTX` 36.
pub(super) fn rage(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, id): (&Game<'_>, u32),
    orders: &mut orders::Panel,
) {
    let text = &game.corpus.text;
    let rage = game
        .world
        .rage(game.program, game.local, id)
        .filter(|_| layout.id == KROLM_PANEL);

    match rage {
        Some(Rage::Ready(b)) => {
            let short = !game.world.affords(game.local, b.price);
            let colour = sheet::price_ink(layout, RAGE_PRICE, Purse::of(short));
            let pressed = layout
                .control(id::SECOND)
                .map(|c| priced(c, text, b.price))
                .is_some_and(|c| sheet::button(canvas, &c, None, Able::of(b.ready)));

            // Its `INTI` 1016 box, as the seekers' count's.
            if let Some(r) = layout.control(RAGE_PRICE).and_then(sheet::rect) {
                canvas.fill(r, COUNT_BOX);
            }

            sheet::fill(
                canvas,
                layout,
                RAGE_PRICE,
                &b.price.to_string(),
                (colour, Align::Centre),
            );

            if pressed {
                press(orders, sends::rage(game.world, game.local, id, b.price));
            }
        }
        Some(Rage::Running { elapsed, duration }) => {
            #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
            let filled = elapsed as f32 / duration.max(1) as f32;

            if let Some(c) = layout.control(RAGE_BAR) {
                sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
            }

            if let Some(t) = layout.control(RAGE_TEXT).and_then(|c| c.text.clone()) {
                sheet::fill(canvas, layout, RAGE_TEXT, &t, (None, Align::Centre));
            }
        }
        None => {}
    }
}

/// The Temple to Krolm's panel, and its Rage's price, bar and text.
const KROLM_PANEL: FourCc = FourCc(*b"AP24");
const RAGE_PRICE: i32 = 4484;
const RAGE_BAR: i32 = 8201;
const RAGE_TEXT: i32 = 8826;
