//! A reward flag's panel: reward, target, owner, seekers.

use regent_core::data::text::{Text, printf};
use regent_core::sim::game::orders as sends;
use regent_core::sim::world::guild::taxman::Step;
use regent_core::sim::world::piece_state;
use regent_core::sim::world::{self, flag::Rewards};
use regent_core::view::menu::{Layout, ShownControl};
use regent_core::view::panel::portrait::details::{self};
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::id;

use super::{COUNT_BOX, Chosen, Game};
use crate::input::orders::{self, FlagKind};
use crate::panel::canvas::{Align, Canvas};
use crate::panel::sheet::{self, Able};
use crate::panel::{Page, Panels};

/// `Offered by %s.`: `GMTX` 180.
const OFFERED_TEXT: usize = 180;

/// The flag owner's line.
fn offered_line(
    text: &Text,
    owner: u32,
    local: u32,
    player: &str,
    nicks: &[String],
) -> Option<String> {
    let name = if owner == piece_state::UNALIGNED {
        text.gmtx(details::MONSTERS)?.into_owned()
    } else if nicks.len() > 1 {
        nicks.get(usize::try_from(owner).ok()?)?.clone()
    } else if owner == local {
        player.to_owned()
    } else {
        text.gmtx(details::ANOTHER_MONARCH)?.into_owned()
    };

    Some(printf(&text.gmtx(OFFERED_TEXT)?, &[name.as_str().into()]))
}

/// A flag's reward, its raise and removal, its target, its seekers.
pub(super) fn flag(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    (chosen, kind): (&Chosen, FlagKind),
    orders: &mut orders::Panel,
) {
    let (world, id) = (game.world, chosen.agent);
    let reward = world.reward(id).unwrap_or_default();

    sheet::fill(
        canvas,
        layout,
        id::REWARD,
        &reward.to_string(),
        (None, Align::Centre),
    );

    if kind == FlagKind::Attack {
        let on = game
            .now
            .iter()
            .find(|s| s.agent == id)
            .and_then(|s| match s.housed {
                world::draw_view::Housed::Hung { on, .. } => Some(on),
                _ => None,
            });
        let target = on
            .and_then(|t| game.now.iter().find(|s| s.agent == t))
            .map(|s| Chosen::of(game.corpus, (world, game.local), s).name);

        if let Some(target) = target {
            sheet::fill(canvas, layout, id::TARGET, &target, (None, Align::Centre));
        }
    }

    // The owner names the line on both panels.
    if let Some(line) = offered_line(
        &game.corpus.text,
        world.piece(id).map_or(game.local, |p| p.owner),
        game.local,
        game.player,
        game.nicks,
    ) {
        sheet::fill(canvas, layout, id::LINE, &line, (None, Align::Centre));
    }

    if !chosen.owned {
        return;
    }

    // View Heroes, dimmed with none, and the count over it.
    let seekers = world.seekers(id).len();

    // `Reward2Controller`: dimmed, `GMTX` 277.
    let able = Able::of(seekers > 0);
    let view = layout.control(id::VIEW_HEROES).map(|c| ShownControl {
        tooltip: match able {
            Able::Enabled => c.tooltip.clone(),
            Able::Disabled => tip::said(&game.corpus.text, tip::line::NO_VIEWERS),
        },
        ..c.clone()
    });

    if let Some(c) = view
        && sheet::button(canvas, &c, None, able)
    {
        panels.state.page = Page::Seekers;
        panels.state.scroll = 0.0;
        panels.state.row = None;
    }

    if let Some(r) = layout.control(id::SEEKERS).and_then(sheet::rect) {
        canvas.fill(r, COUNT_BOX);
    }

    sheet::fill(
        canvas,
        layout,
        id::SEEKERS,
        &seekers.to_string(),
        (None, Align::Centre),
    );

    // Shift sends the single raise as many times as fit: the order stays
    // the engine's.
    let raises = world.flag_raises(game.local, id, canvas.stride());

    let button = layout
        .control(id::RAISE)
        .map(|c| sheet::hinted(c, Step::Raise, Rewards::far()));

    if let Some(c) = &button
        && sheet::button(canvas, c, None, Able::of(world.raisable(game.local, id)))
    {
        let sent = std::iter::repeat_n(sends::raise_reward(id), raises);

        orders.orders.outbox.extend(sent);
    }

    if let Some(c) = layout.control(id::REMOVE)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        orders::remove(&mut orders.orders, world, id);
    }
}

#[cfg(test)]
mod tests {
    use regent_core::data::fourcc::FourCc;

    use super::*;
    use crate::panel::upper::tests::hd;

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn reward_panels_name_the_flag_owner() {
        let catalogue = hd();
        let text = Text::build(catalogue);

        for panel in [b"AP46", b"AP80", b"APb6", b"APd0"] {
            let layout = regent_core::view::menu::layout(catalogue, &text, FourCc(*panel))
                .expect("a flag panel")
                .expect("parses");

            assert!(layout.control(id::LINE).is_some(), "{panel:?}");
        }

        assert_eq!(
            offered_line(&text, 1, 0, "The Crown", &[]).as_deref(),
            Some("Offered by another monarch.")
        );
        assert_eq!(
            offered_line(&text, 0, 0, "The Crown", &[]).as_deref(),
            Some("Offered by The Crown.")
        );
        assert_eq!(
            offered_line(&text, 1, 0, "The Crown", &["Ada".into(), "Bea".into()]).as_deref(),
            Some("Offered by Bea.")
        );
    }
}
