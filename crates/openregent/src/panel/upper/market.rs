//! The marketplace's Market Day.

use regent_core::data::fourcc::FourCc;
use regent_core::sim::game::Input;
use regent_core::sim::world::guild::market::Market;
use regent_core::view::menu::Layout;

use super::{Game, WORKING};
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet::{self, Able};

/// The marketplace's Market Day (`AP31`, `MarketplaceController`):
/// 8200 MARKET DAY, hot key M, once researched; while one runs, the bar
/// 8201 of its duration and 8826 "Market Day in progress" over it. No
/// price. A press sends it.
pub(super) fn market_day(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, id): (&Game<'_>, u32),
    orders: &mut orders::Panel,
) {
    let market = game
        .world
        .market_day(game.program, game.local, id)
        .filter(|_| layout.id == MARKET_PANEL);

    match market {
        Some(Market::Ready) => {
            if let Some(c) = layout.control(MARKET_DAY)
                && sheet::button(canvas, c, None, Able::Enabled)
            {
                orders.orders.outbox.push(Input::MarketDay(id));
            }
        }
        Some(Market::Running { elapsed, duration }) => {
            #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
            let filled = elapsed as f32 / duration.max(1) as f32;

            if let Some(c) = layout.control(MARKET_BAR) {
                sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
            }

            if let Some(c) = layout.control(MARKET_TEXT)
                && let Some(t) = c.text.as_deref()
            {
                sheet::fill(canvas, layout, MARKET_TEXT, t, (None, Align::Centre));
            }
        }
        None => {}
    }
}

/// The marketplace's panel, and its Market Day button, bar and text.
const MARKET_PANEL: FourCc = FourCc(*b"AP31");
const MARKET_DAY: i32 = 8200;
const MARKET_BAR: i32 = 8201;
const MARKET_TEXT: i32 = 8826;
