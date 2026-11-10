//! A building's panel: level, upgrade, upkeep, counts, page buttons.

use regent_core::data::text::Text;
use regent_core::sim::game::Input;
use regent_core::sim::world::{
    World,
    guild::upkeep::{Setting, Toggle, Upkeep},
    kingdom::Henchmen,
};
use regent_core::view::menu::{Layout, ShownControl, Tick};
use regent_core::view::panel::portrait::details::{self, Details};
use regent_core::view::panel::portrait::gear::{self, Forge};
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::{
    self, ABODE_PANEL, Contests, GAMBLE_PANEL, OUTPOST_PANEL, id,
};

use super::band::{band, calling};
use super::rules::{self, war_roster};
use super::{Chosen, Game, WORKING, research_page, tipped};
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::games;
use crate::panel::ink::{Pic, Tone};
use crate::panel::sheet::{self, Able, Lit, Purse};
use crate::panel::{Ask, Page, Panels, State};

/// A building's own controls and its band.
pub(super) fn building(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
) {
    let (world, id) = (game.world, chosen.agent);
    let text = &game.corpus.text;
    let level = chosen.code.0[3].wrapping_sub(b'0');

    // Inferred: a building's level is its code's last digit, ABJ2 the
    // palace's second.
    if (1..=9).contains(&level) {
        sheet::fill(
            canvas,
            layout,
            id::LEVEL,
            &level.to_string(),
            (None, Align::Centre),
        );
    }

    if chosen.panel == Some(gear::FORGE_PANEL)
        && let Some(piece) = world.piece(id)
    {
        forge(canvas, layout, Forge::of(piece), text);
    }

    // A wizards' guild's staff, "+n" of its level.
    if chosen.panel == Some(gear::STAFF_PANEL)
        && let Some(plus) = Forge::bonus(text, gear::staff(chosen.code))
    {
        sheet::fill(
            canvas,
            layout,
            gear::STAFF_BONUS,
            &plus,
            (None, Align::Centre),
        );
    }

    let roster = world.war_roster(&orders.rules().0, id);

    if let Some(m) = world.members(id) {
        // A warriors' guild's 7949 counts its warriors alone.
        let counts = id::COUNTS
            .into_iter()
            .filter(|&c| roster.is_none() || c != id::WARRIORS);

        for count in counts {
            sheet::fill(
                canvas,
                layout,
                count,
                &m.living.to_string(),
                (None, Align::Centre),
            );
        }
    }

    if let Some(r) = roster {
        war_roster(canvas, layout, r);
    }

    // An outpost's heroes beside its portrait; its hero icon opens its
    // member list, as HEROES does.
    if let Some(r) = upper::housed(chosen.panel, world.members(id)) {
        rules::counts(canvas, layout, r);

        let icon = layout.control(id::HOUSED.0).and_then(sheet::rect);

        if let Some(r) = icon.filter(|_| chosen.owned)
            && canvas.press(r, ("housed", id)).clicked()
        {
            panels.state.page = Page::Members;
            panels.state.scroll = 0.0;
            panels.state.row = None;
        }
    }

    if chosen.contests == Some(Contests::Open) {
        games::contests(canvas, layout, (world, id, chosen.owned), orders);
    }

    if !chosen.owned {
        return;
    }

    if chosen.palace() {
        henchmen(canvas, panels, layout, game);
    }

    if chosen.panel == Some(GAMBLE_PANEL) {
        let bet = &mut panels.state.bet;

        games::gamble(canvas, layout, (game, text, id), bet, orders);
    }

    upgrade(canvas, layout, game, chosen, orders);
    upkeep(canvas, layout, (world, text), id, orders);
    pages(
        canvas,
        panels,
        layout,
        (
            chosen,
            world.progress(id).is_some() || calling(game, chosen).is_some(),
        ),
        (visitors(world, id, text).as_deref(), world, text),
    );

    if chosen.panel == Some(ABODE_PANEL) {
        abode(canvas, panels, layout, world, id);
    }

    if let Some(c) = layout.control(id::DESTROY)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        panels.state.ask = Some(Ask::Demolish(id));
    }

    band(canvas, layout, game, chosen, orders, text);
}

/// The palace's counts, the henchmen its level supports,
/// beside their icons, which open the henchmen roster.
fn henchmen(canvas: &mut Canvas<'_>, panels: &mut Panels, layout: &Layout, game: &Game<'_>) {
    for (h, n) in Henchmen::ALL.iter().zip(game.henchmen) {
        if let Some(n) = n {
            sheet::fill(
                canvas,
                layout,
                h.control(),
                &n.to_string(),
                (None, Align::Centre),
            );
        }
    }

    for icon in id::PALACE_ICONS {
        if let Some(c) = layout.control(icon)
            && sheet::button(canvas, c, None, Able::Enabled)
        {
            panels.state.page = Page::Henchmen;
            panels.state.scroll = 0.0;
            panels.state.row = None;
        }
    }
}

/// A Blacksmith's researched gear: each picture at its level's frame,
/// "+n" beside it; nothing researched, neither.
fn forge(canvas: &mut Canvas<'_>, layout: &Layout, forge: Forge, text: &Text) {
    let shown = [
        (forge.weapon, gear::FORGE_WEAPON),
        (forge.armour, gear::FORGE_ARMOUR),
    ]
    .into_iter()
    .filter_map(|(level, ids)| Some((level?, ids)));

    for (level, (picture, bonus)) in shown {
        if let Some(c) = layout.control(picture)
            && let (Some(r), Some((record, set))) = (sheet::rect(c), c.art)
        {
            let frame = usize::try_from(level).unwrap_or_default();

            canvas.picture(Pic::of(record, set).frame(frame), Tone::Plain, r.min);
        }

        if let Some(plus) = Forge::bonus(text, level) {
            sheet::fill(canvas, layout, bonus, &plus, (None, Align::Centre));
        }
    }
}

/// The upgrade button and its price.
fn upgrade(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
) {
    let Some(offer) = game.world.upgrade_offer(chosen.agent) else {
        return;
    };
    let next = offer.what.0[3].wrapping_sub(b'0');
    let label = (1..=9).contains(&next).then(|| next.to_string());
    let short = !game.world.affords(game.local, offer.price);
    let colour = sheet::price_ink(layout, id::UPGRADE_PRICE, Purse::of(short));

    sheet::fill(
        canvas,
        layout,
        id::UPGRADE_PRICE,
        &offer.price.to_string(),
        (colour, Align::Centre),
    );

    if let Some(c) = layout.control(id::UPGRADE) {
        let tip = game
            .world
            .upgrade_tip(chosen.agent)
            .map(|why| why.map(|to| orders.title(game.corpus, to)))
            .and_then(|why| tip::upgrade(&game.corpus.text, why.as_ref().map(String::as_str)));

        if tipped(canvas, c, label.as_deref(), Able::of(offer.ready), tip) {
            orders.orders.outbox.push(Input::Upgrade(chosen.agent));
        }
    }
}

/// The tax and repair checks: each lit as its setting is, a press flips
/// it (`GUILD_ACTION` 6 to 9).
fn upkeep(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (world, text): (&World, &Text),
    id: u32,
    orders: &mut orders::Panel,
) {
    let Some(u) = world.upkeeps(id) else {
        return;
    };
    let checks = [
        (id::TAX_ONCE, u.tax, Setting::Once, Upkeep::TaxOnce),
        (id::TAX_ROUTE, u.tax, Setting::Always, Upkeep::TaxRoute),
        (id::REPAIR_ONCE, u.repair, Setting::Once, Upkeep::RepairOnce),
        (
            id::REPAIR_ROUTE,
            u.repair,
            Setting::Always,
            Upkeep::RepairRoute,
        ),
    ];

    for (control, setting, on, upkeep) in checks {
        let lit = if setting == Some(on) {
            Lit::On
        } else {
            Lit::Off
        };

        // `Base_BuildingController`: what a press would do.
        let tick = match lit {
            Lit::On => Tick::Checked,
            Lit::Off => Tick::Clear,
        };
        let told = |c: &ShownControl| ShownControl {
            tooltip: tip::toggle(control, tick)
                .and_then(|l| tip::said(text, l))
                .or_else(|| c.tooltip.clone()),
            ..c.clone()
        };

        if let Some(c) = layout.control(control).map(told)
            && sheet::check(canvas, &c, lit, Able::of(setting.is_some()))
        {
            orders.orders.outbox.push(Input::Toggle(Toggle {
                building: id,
                upkeep,
            }));
        }
    }
}

/// `GMTX` 230 "Nobody Here" while nobody is inside building `id`
/// (**Game**: `ui_inn.png`, `ui_marketplace.png`); `None` while someone
/// is, or the world cannot tell.
fn visitors(world: &World, id: u32, text: &Text) -> Option<String> {
    let inside = world.piece(id).and_then(|p| world.occupants(p.handle))?;

    if inside.is_empty() {
        text.gmtx(NOBODY_TEXT).map(std::borrow::Cow::into_owned)
    } else {
        None
    }
}

/// `Nobody Here`: `GMTX` 230.
const NOBODY_TEXT: usize = 230;

/// The buttons opening a page below, and the palace's.
fn pages(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (chosen, working): (&Chosen, bool),
    (nobody, world, text): (Option<&str>, &World, &Text),
) {
    let buttons: Vec<(i32, Option<Page>)> = if chosen.palace() {
        // An outpost's HEROES opens its member list
        // (`Outpost_Controller`: `AP18`; here `APd1`'s page).
        let outpost = (chosen.panel == Some(OUTPOST_PANEL))
            .then_some((id::OUTPOST_HEROES, Some(Page::Members)));

        [
            (id::PALACE_STATS, Some(Page::Statistics)),
            (id::PALACE_BUILD, Some(Page::Build)),
            (id::PALACE_REWARDS, Some(Page::Rewards)),
            (id::PALACE_ROSTER, Some(Page::Heroes)),
        ]
        .into_iter()
        .chain(outpost)
        .collect()
    } else {
        let crypt = chosen.panel == Some(details::MAUSOLEUM_PANEL);
        let graveyard = chosen.panel == Some(details::GRAVEYARD_PANEL);
        let heroes = match (crypt, graveyard) {
            (true, _) => Page::Crypt,
            (_, true) => Page::Graves,
            _ => Page::Members,
        };
        // A graveyard's list opens for the local player's only,
        // whose panel counts its heroes.
        let mine = !graveyard || matches!(chosen.details, Some(Details::Buried(_)));

        [
            (id::HEROES, Some(heroes)),
            (id::VISITORS, Some(Page::Visitors)),
        ]
        .into_iter()
        .filter(|(c, _)| mine || *c != id::HEROES)
        .collect()
    };
    // A running research's bar takes the research button's place, an
    // open Fairgrounds' contests too.
    let open = chosen.contests.is_some_and(|c| !c.shows(id::RESEARCH));
    let abode = chosen.panel == Some(ABODE_PANEL);
    let research =
        (!working && !open && !abode).then_some((id::RESEARCH, research_page(chosen.panel)));
    let buttons = buttons.into_iter().chain(research);

    for (control, page) in buttons {
        // An empty building's visitors button says "Nobody Here".
        let (label, able) = match (control, &nobody) {
            (id::VISITORS, Some(nobody)) => (Some(*nobody), false),
            _ => (None, page.is_some()),
        };

        // `Base_BuildingController` and
        // `Base_ResearchedBuildingController`: why it is disabled.
        let why = match (control, able) {
            (id::RESEARCH, _) if world.needs_repair(chosen.agent) => Some(tip::line::REPAIR_FIRST),
            (id::RESEARCH, false) => Some(tip::line::RESEARCH_DONE),
            (_, false) => tip::barred(control),
            _ => None,
        };
        let shown = layout.control(control).map(|c| ShownControl {
            tooltip: why
                .and_then(|l| tip::said(text, l))
                .or_else(|| c.tooltip.clone()),
            ..c.clone()
        });

        if let Some(c) = shown
            && sheet::button(canvas, &c, label, Able::of(able))
            && let Some(page) = page
        {
            panels.state.page = page;
            panels.state.scroll = 0.0;
            panels.state.row = None;
        }
    }

    if let Some(c) = layout.control(id::SECOND)
        && c.text.as_deref().is_some_and(|t| t.trim() == "SPELLS")
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        spells(&mut panels.state);
    }
}

/// A temple's SPELLS (8009) or the Abode's Spells (8851) pressed: the
/// building's own spell page (`spell_bar::spell_page`), the spell bar left
/// as it is.
const fn spells(state: &mut State) {
    state.page = Page::Casting;
    state.scroll = 0.0;
    state.row = None;
}

/// The Sorcerer's Abode's Spells (8851, `SorcerersAbode_Controller`)
/// opens its spells, `MX07`, as the page. While it rests, its bar 4500
/// and 4501 "Sorcerer Is Resting" in the button's place.
fn abode(canvas: &mut Canvas<'_>, panels: &mut Panels, layout: &Layout, world: &World, id: u32) {
    match world.resting(id) {
        Some(rest) => {
            if let Some(c) = layout.control(RESTING_BAR) {
                rest_bar(canvas, c, rest);
            }

            if let Some(t) = layout.control(RESTING_TEXT).and_then(|c| c.text.clone()) {
                sheet::fill(
                    canvas,
                    layout,
                    RESTING_TEXT,
                    t.trim(),
                    (None, Align::Centre),
                );
            }
        }
        None => {
            if let Some(c) = layout.control(id::RESEARCH)
                && sheet::button(canvas, c, None, Able::Enabled)
            {
                spells(&mut panels.state);
            }
        }
    }
}

/// An Abode's rest, `(elapsed, lasts)` ms, on bar `c`: filled by the
/// share gone (property `0x29`, `{7, clock - ASS:0, 0, ASP:0}`), as its
/// panel's 4500 and the spell bar's 9500
/// both draw it.
pub(crate) fn rest_bar(canvas: &mut Canvas<'_>, c: &ShownControl, (elapsed, lasts): (i32, i32)) {
    #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
    let filled = elapsed as f32 / lasts.max(1) as f32;

    sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
}

/// The Abode's rest's bar and words.
const RESTING_BAR: i32 = 4500;
const RESTING_TEXT: i32 = 4501;

#[cfg(test)]
mod tests {
    use super::*;

    /// A temple's SPELLS opens its spell page below, as the Abode's
    /// Spells does, and leaves the spell bar as it is (**Binary**:
    /// `Base_GuildController`).
    #[test]
    fn a_temples_spells_opens_its_page_not_the_bar() {
        let mut state = State {
            spells_closed: true,
            scroll: 40.0,
            row: Some(2),
            ..State::default()
        };

        spells(&mut state);

        assert_eq!(state.page, Page::Casting);
        assert!(state.spells_closed);
        assert_eq!((state.scroll, state.row), (0.0, None));
    }
}
