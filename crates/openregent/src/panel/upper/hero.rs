//! A hero's panel (`AP20`): class, level, experience, gear, the page
//! buttons; and its stats page's numbers.

use bevy_egui::egui::{Color32, Rect, pos2};
use regent_core::data::stat::Stat;
use regent_core::sim::game::Input;
use regent_core::view::menu::{Layout, ShownControl};
use regent_core::view::panel::portrait::details::{Hero, STAT_CONTROLS};
use regent_core::view::panel::portrait::gear::Gear;
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::id;

use super::{Chosen, Game};
use crate::input::orders;
use crate::panel::canvas::{Align, Canvas};
use crate::panel::ink::{Pic, Tone};
use crate::panel::sheet::{self, Able};
use crate::panel::{Ask, Page, Panels};

/// The experience bar's blue (`ui_paladin_items.png`).
const EXPERIENCE: Color32 = Color32::from_rgb(40, 110, 210);

/// A hero's own controls: class, level, experience, gear and the three
/// page buttons.
pub(super) fn heroic(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (game, chosen, hero): (&Game<'_>, &Chosen, &Hero),
    orders: &mut orders::Panel,
) {
    sheet::fill(
        canvas,
        layout,
        id::CLASS,
        &chosen.title,
        (None, Align::Centre),
    );
    sheet::fill(
        canvas,
        layout,
        id::HERO_LEVEL,
        &hero.level.to_string(),
        (None, Align::Centre),
    );

    if let Some(c) = layout.control(id::XP_BAR) {
        let xp = hero.experience;
        #[allow(clippy::cast_precision_loss, reason = "experience is small")]
        let filled = if xp.needed > 0 {
            xp.earned as f32 / xp.needed as f32
        } else {
            0.0
        };

        sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), EXPERIENCE);
    }

    gear(canvas, layout, (id::WEAPON, id::WEAPON_BONUS), hero.weapon);
    gear(canvas, layout, (id::ARMOUR, id::ARMOUR_BONUS), hero.armour);

    // `HeroController`: a picture's tooltip is its gear's name
    //.
    for (control, worn) in [(id::WEAPON, hero.weapon), (id::ARMOUR, hero.armour)] {
        if let Some(r) = layout.control(control).and_then(sheet::rect) {
            let response = canvas.press(r, ("gear", control));

            Canvas::tip(&response, worn.name(&game.corpus.text).as_deref());
        }
    }

    for (control, page) in [
        (id::TAB_SPELLS, Page::Spells),
        (id::TAB_ITEMS, Page::Items),
        (id::TAB_STATS, Page::Stats),
    ] {
        if let Some(c) = layout.control(control).filter(|_| hero.shows(control))
            && tab(canvas, c)
        {
            panels.state.page = page;
            panels.state.hero_page = Some(page);
        }
    }

    // Dismiss asks first (`AP62`); only the player's hero, not yet
    // dismissed, shows it. One lying dead goes at
    // once: `DESTROY_VEHICLE`.
    if let Some(c) = layout
        .control(id::DESTROY)
        .filter(|_| hero.shows(id::DESTROY))
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        match game.world.piece(chosen.agent).filter(|_| chosen.grave) {
            Some(p) => orders.orders.outbox.push(Input::Clear(p.handle)),
            None => panels.state.ask = Some(Ask::Dismiss(chosen.agent)),
        }
    }
}

/// A hero page button: its picture holds an icon at the left, so its
/// label is centred right of it (**Game**: `ui_monk_spells.png`).
fn tab(canvas: &mut Canvas<'_>, c: &ShownControl) -> bool {
    let pressed = sheet::button(canvas, c, Some(""), Able::Enabled);

    if let (Some(r), Some(text)) = (sheet::rect(c), c.text.as_deref()) {
        let words = Rect::from_min_max(pos2(r.left() + TAB_ICON, r.top()), r.max);

        sheet::label(canvas, c, words, text);
    }

    pressed
}

/// The width of a hero page button's icon, pixels.
const TAB_ICON: f32 = 20.0;

/// A gear's picture at its upgrade in control `picture`, its bonus in
/// `bonus`. Level 5, past the art's frames, is level 1's in purple where
/// the original shows level 1's plain (`docs/openregent/enhancements.md`).
pub fn gear(canvas: &mut Canvas<'_>, layout: &Layout, (picture, bonus): (i32, i32), gear: Gear) {
    let tone = if gear.past_frames() {
        Tone::Purple
    } else {
        Tone::Plain
    };

    if let (Some(r), Some((record, set))) = (
        layout.control(picture).and_then(sheet::rect),
        gear.picture(),
    ) {
        canvas.picture(
            Pic::of(record, i32::from(set)).frame(gear.frame()),
            tone,
            r.min,
        );
    }

    if gear.enchant > 0 {
        sheet::fill(
            canvas,
            layout,
            bonus,
            &format!("+{}", gear.enchant),
            (None, Align::Centre),
        );
    }
}

/// A hero's ten stats with the primary one starred (`AP21`); each its
/// tooltip, a skill row's naming its cap.
pub fn stats(canvas: &mut Canvas<'_>, layout: &Layout, hero: &Hero, game: &Game<'_>) {
    const STARS: [(Stat, i32); 5] = [
        (Stat::Strength, 26),
        (Stat::Intelligence, 27),
        (Stat::Artifice, 28),
        (Stat::Vitality, 29),
        (Stat::WillPower, 30),
    ];

    for (&value, control) in hero.stats.iter().zip(STAT_CONTROLS) {
        sheet::fill(
            canvas,
            layout,
            control,
            &value.to_string(),
            (None, Align::Centre),
        );

        let skill = tip::SKILLS.iter().find(|s| s.control == control);
        let said = match skill {
            Some(s) => tip::skill(&game.corpus.text, s, s.cap.value(game.program, game.rules)),
            None => layout.control(control).and_then(|c| c.tooltip.clone()),
        };

        if let Some(r) = layout.control(control).and_then(sheet::rect) {
            let response = canvas.hover(r, ("stat", control));

            Canvas::tip(&response, said.as_deref());
        }
    }

    for (stat, control) in STARS {
        if hero.primary == Some(stat)
            && let Some(c) = layout.control(control)
        {
            sheet::picture(canvas, c);

            if let Some(r) = sheet::rect(c) {
                let response = canvas.hover(r, ("primary stat tip", control));

                Canvas::tip(&response, c.tooltip.as_deref());
            }
        }
    }
}
