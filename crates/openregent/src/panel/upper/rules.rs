//! The library's choice rules drawn: the warriors' guild's roster
//! counts, the rogues' extortion ([`regent_core::view::panel::upper`]).

use regent_core::data::text::Text;
use regent_core::sim::world::offer::WarRoster;
use regent_core::view::menu::{Layout, ShownControl};
use regent_core::view::panel::upper::{self, Extort, Extorted, id};

use super::Game;
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet;

/// A warriors' guild's counts beside its portrait:
/// its warriors in 7949; one
/// temple hero's icon and count in the one-slot frame 7950, both in the
/// two-slot frame 7960, warriors of discord over paladins.
pub(super) fn war_roster(canvas: &mut Canvas<'_>, layout: &Layout, r: WarRoster) {
    let roster = upper::war_roster(r);

    sheet::fill(
        canvas,
        layout,
        id::WARRIORS,
        &r.warriors.to_string(),
        (None, Align::Centre),
    );

    counts(canvas, layout, roster);
}

/// A roster's frame, and each icon with its count.
pub(super) fn counts(canvas: &mut Canvas<'_>, layout: &Layout, roster: upper::Roster) {
    if let Some(c) = roster.frame.and_then(|f| layout.control(f)) {
        sheet::picture(canvas, c);
    }

    for t in roster.temples {
        if let Some(c) = layout.control(t.icon) {
            sheet::picture(canvas, c);
        }

        let living = t.living.to_string();

        sheet::fill(canvas, layout, t.count, &living, (None, Align::Centre));
    }
}

/// The rogues' guild's extortion (`AP48` 8009, hot key E): `EXTORT (%d)`
/// of the player's share, enabled while the total is at least 1.
/// A press sounds `EN01`; with a total it says
/// both shares and sends the extortion.
pub(super) fn extortion(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, id): (&Game<'_>, u32),
    orders: &mut orders::Panel,
    text: &Text,
) {
    let button = Extort::of(text, layout.id, game.extortion);

    if let (Some(c), Some(e), Some(b)) = (layout.control(id::SECOND), game.extortion, button) {
        let told = ShownControl {
            tooltip: b.tip,
            ..c.clone()
        };

        if sheet::button(canvas, &told, b.label.as_deref(), b.able) {
            let pressed = Extorted::of(e, id);

            orders.orders.sounds.push(pressed.sound);
            orders.orders.said.extend(pressed.said);
            orders.orders.outbox.extend(pressed.sent);
        }
    }
}
