//! An embassy's open and close buttons (`MX22`).

use regent_core::sim::game::Input;
use regent_core::view::menu::Layout;

use crate::input::orders;
use crate::panel::canvas::Canvas;
use crate::panel::sheet::{self, Able};
use crate::panel::upper::Game;

/// An embassy's open and close buttons (`MX22`): the one its state asks
/// for, the recruit timer's share on the close one.
pub(super) fn embassy(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    game: &Game<'_>,
    id: u32,
    orders: &mut orders::Panel,
) {
    let world = game.world;
    let offer = world.embassy_offer(game.local, id);
    let control = offer.map(|b| if b.what { OPEN_EMBASSY } else { CLOSE_EMBASSY });

    if let (Some(b), Some(c)) = (offer, control.and_then(|c| layout.control(c))) {
        let (run, duration, _) = world.embassy_timer(id);
        let label = c.text.as_deref().map(|t| {
            if b.what {
                t.trim().to_owned()
            } else {
                format!(
                    "{} {}%",
                    t.trim(),
                    run.saturating_mul(100) / duration.max(1)
                )
            }
        });

        if sheet::button(canvas, c, label.as_deref(), Able::of(b.ready)) {
            orders.orders.outbox.push(Input::Embassy(id));
        }
    }
}

/// `MX22`'s open and close buttons.
pub(super) const OPEN_EMBASSY: i32 = 8875;
pub(super) const CLOSE_EMBASSY: i32 = 8876;
