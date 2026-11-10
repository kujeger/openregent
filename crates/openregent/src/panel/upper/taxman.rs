//! A tax collector's panel (`AP74`) and a caravan's (`AP90`).

use regent_core::sim::game::Input;
use regent_core::sim::world::guild::taxman::{Amount, Step, tax_far};
use regent_core::view::menu::Layout;
use regent_core::view::panel::portrait::details::Details;
use regent_core::view::panel::upper::id;

use super::{Chosen, Game};
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet::{self, Able};

/// A tax collector's amounts and their buttons
/// (`TaxCollectorController`); a caravan's load alone (`CaravanController`).
pub(super) fn taxman(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
) {
    let (world, id) = (game.world, chosen.agent);
    let carried = match chosen.details {
        Some(Details::Purse(gold)) => gold,
        _ => 0,
    };

    sheet::fill(
        canvas,
        layout,
        id::CARRIED,
        &carried.to_string(),
        (None, Align::Centre),
    );

    let rows = [
        (Amount::Return, id::RETURN, (id::RETURN_DOWN, id::RETURN_UP)),
        (Amount::Pickup, id::PICKUP, (id::PICKUP_DOWN, id::PICKUP_UP)),
    ];

    // Shift sends the single step as many times as fit: `tax_sent` takes
    // no other.
    let stride = canvas.stride();

    for (amount, shown, (down, up)) in rows.into_iter().filter(|_| world.taxman(id)) {
        let value = world.tax_amount(id, amount).unwrap_or_default();

        sheet::fill(
            canvas,
            layout,
            shown,
            &value.to_string(),
            (None, Align::Centre),
        );

        for (control, step) in [(down, Step::Lower), (up, Step::Raise)] {
            let send = world.tax_press(game.program, id, amount, step);

            let button = layout
                .control(control)
                .map(|c| sheet::hinted(c, step, tax_far(game.program)));

            if let Some(c) = &button
                && sheet::button(canvas, c, None, Able::of(send.is_some()))
            {
                let alters = world.tax_presses(game.program, (id, amount), step, stride);

                orders
                    .orders
                    .outbox
                    .extend(alters.into_iter().map(Input::Alter));
            }
        }
    }
}
