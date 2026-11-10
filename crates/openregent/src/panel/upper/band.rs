//! A building's band at (7, 190): a running recruit's or research's bar,
//! else its recruit, enchant and second-row buttons.

use bevy_egui::egui::{Color32, Rect, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::{Text, printf};
use regent_core::sim::game::Input;
use regent_core::sim::world::{self, offer::Button, recruit::Recruit};
use regent_core::view::menu::{Layout, ShownControl, State, Tick};
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::HALL_PANEL;

use super::embassy::embassy;
use super::guild::{assembly, camp, rage};
use super::market::market_day;
use super::rules::extortion;
use super::war_band::war_band;
use super::{Chosen, Game, SHORT, WORKING};
use crate::input::orders::{self, Mode};
use crate::panel::canvas::{Align, Canvas};
use crate::panel::ink::{Pic, Tone};
use crate::panel::sheet::{self, Able};

/// The band of a building panel's main buttons and bars: `8008`, `5001`,
/// `8022`, `4500` all lie here.
pub(super) const BAND: Rect = Rect {
    min: pos2(7.0, 190.0),
    max: pos2(196.0, 217.0),
};

/// The band's wide button picture, `INBb` 1006 (189 × 27).
pub(super) const BAND_BUTTON: FourCc = FourCc(*b"INBb");
pub(super) const BAND_BUTTON_SET: i32 = 1006;

/// The band's progress bars: a guild's recruit, a researcher's, an
/// enchantment's.
const PROGRESS: [i32; 3] = [8022, 4500, 8202];

/// The band's labels' font, `fnt4`, and the prices', `fnt7`.
const BAND_FONT: FourCc = FourCc(*b"fnt4");
pub(super) const SMALL_FONT: FourCc = FourCc(*b"fnt7");

/// `RECRUIT %s`: `GMTX` 17.
const RECRUIT_TEXT: usize = 17;

/// `GUILD IS FULL`: `GMTX` 19.
pub(super) const FULL_TEXT: usize = 19;

/// A band button's text where no control gives its colours: the
/// controls' navy (`8000003f`).
const NAVY: Color32 = Color32::from_rgb(0, 0, 0x3f);

/// The band at (7, 190): a running recruit's or research's bar; else the
/// recruits, the research page, the enchantment, the camp move, the
/// embassy.
pub(super) fn band(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
    text: &Text,
) {
    let (world, id) = (game.world, chosen.agent);

    // The second row: the camp move, the call to arms, the extortion,
    // the Rage, the embassy's door, the Market Day.
    camp(canvas, layout, game, id, orders);
    assembly(canvas, layout, (game, id), orders, text);
    extortion(canvas, layout, (game, id), orders, text);
    rage(canvas, layout, (game, id), orders);
    embassy(canvas, layout, game, id, orders);
    market_day(canvas, layout, (game, id), orders);

    // A bounty running: the hall's bar and its words, 8851 hidden.
    if let Some((elapsed, length)) = calling(game, chosen) {
        #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
        let filled = elapsed as f32 / length.max(1) as f32;

        if let Some(c) = layout.control(CALLING_BAR) {
            sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
        }

        if let Some(t) = text.gmtx(world::guild::bounty::CALLING) {
            sheet::fill(canvas, layout, CALLING_TEXT, &t, (None, Align::Centre));
        }

        return;
    }

    // A tower's enchantment running: its bar and words, the button
    // hidden.
    if let Some(running) = world.enchanting(game.program, id) {
        enchanting(canvas, layout, running);

        return;
    }

    if let Some(p) = world.progress(id) {
        match PROGRESS.iter().find_map(|&p| layout.control(p)) {
            Some(c) => sheet::art_bar(canvas, c, (p.filled(), sheet::Fill::Plain), WORKING),
            None => sheet::bar(canvas, BAND, p.filled(), WORKING),
        }

        if let Some(line) = p.text(text) {
            let colour = sheet::plain(layout, State::Normal);

            canvas.text(SMALL_FONT, BAND, &line, (colour, Align::Centre));
        }

        return;
    }

    let moving = orders.orders.mode == Mode::Camp(id);
    let full = world
        .members(id)
        .is_some_and(|m| i32::try_from(m.living).is_ok_and(|l| l >= m.spots));
    let recruits = world.recruits(&orders.rules().0, id);
    let war = recruits.clone();
    #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
    let wide = BAND.width() / recruits.len().max(1) as f32;
    let members = world.members(id);
    let mut buttons: Vec<Banded> = recruits
        .into_iter()
        .map(|b| {
            let title = orders.title(game.corpus, b.what);
            let label = text
                .gmtx(RECRUIT_TEXT)
                .map_or_else(String::new, |f| printf(&f, &[(&title).into()]));
            let button = Button {
                what: (),
                price: b.price,
                ready: b.ready && !moving,
            };
            let control = band_control(layout, &title, wide);

            Banded {
                label,
                button,
                input: Input::Recruit(Recruit {
                    guild: id,
                    code: b.what,
                }),
                key: control.and_then(|c| c.hotkey),
                tip: control.and_then(|c| recruit_tip(c, text, (button, members))),
                control,
            }
        })
        .collect();

    if let Some(b) = world.enchant_offer(game.program, id) {
        let c = layout.control(5001);

        buttons.push(Banded {
            label: c
                .and_then(|c| c.text.clone())
                .unwrap_or_else(|| "Enchant".to_owned()),
            button: b,
            input: Input::Enchant(id),
            key: c.and_then(|c| c.hotkey),
            // `WizardsTower_Controller`: the price after its own.
            tip: c
                .and_then(|c| c.tooltip.as_deref())
                .map(|own| tip::tower(text, own, b.price)),
            control: c,
        });
    }

    let full_text = text.gmtx(FULL_TEXT).map(std::borrow::Cow::into_owned);

    match (full, full_text, buttons.is_empty()) {
        // A temple splits a warriors' guild's band.
        _ if war.len() > 1 => war_band(canvas, layout, (game, id), (&war, full), orders),
        (true, Some(t), false) => {
            let colour = sheet::plain(layout, State::Disabled);

            canvas.stretched(Pic::of(BAND_BUTTON, BAND_BUTTON_SET), Tone::Dim, BAND);
            canvas.text(BAND_FONT, BAND, &t, (colour, Align::Centre));
        }
        _ => row(canvas, &buttons, game, orders),
    }
}

/// One of the band's buttons: its label, offer, what a press sends, its
/// hot key, tooltip and colours.
struct Banded<'l> {
    label: String,
    button: Button<()>,
    input: Input,
    key: Option<i32>,
    tip: Option<String>,
    /// The control giving its colours.
    control: Option<&'l ShownControl>,
}

/// A recruit control `c`'s tooltip (`Base_GuildController`,
/// `WarGuildController`): enabled, its own, the hero it would be of
/// the guild's spots and the price; disabled, `GMTX` 270.
pub(super) fn recruit_tip(
    c: &ShownControl,
    text: &Text,
    (b, members): (Button<()>, Option<world::hire::Members>),
) -> Option<String> {
    match (b.ready, members) {
        (true, Some(m)) => c
            .tooltip
            .as_deref()
            .map(|own| tip::recruit(text, own, (m.living, m.spots), b.price)),
        (true, None) => c.tooltip.clone(),
        (false, _) => tip::said(text, tip::line::GUILD_FULL),
    }
}

/// `buttons` side by side across the band, each its label and price in
/// its control's colours (navy, a blue letter; lit held, red disabled);
/// pressed by click or its hot key.
fn row(
    canvas: &mut Canvas<'_>,
    buttons: &[Banded<'_>],
    game: &Game<'_>,
    orders: &mut orders::Panel,
) {
    #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
    let wide = BAND.width() / buttons.len().max(1) as f32;

    for (n, banded) in buttons.iter().enumerate() {
        let Banded {
            label,
            button: b,
            input,
            key,
            tip,
            control,
        } = banded;
        #[allow(clippy::cast_precision_loss, reason = "a few buttons")]
        let left = BAND.left() + wide * n as f32;
        let r = Rect::from_min_size(pos2(left, BAND.top()), vec2(wide, BAND.height()));
        let response = canvas.press(r, ("band", n));
        let pic = Pic::of(BAND_BUTTON, BAND_BUTTON_SET);
        let look = sheet::look(&response, Tick::Clear);
        let state = Able::of(b.ready).state(sheet::UNFLAGGED);
        let (slot, tone) = sheet::slot(canvas, pic, look, state);
        let short = !game.world.affords(game.local, b.price);

        canvas.stretched(pic.slot(slot), tone, r);

        let (words, price) = (
            r.shrink2(vec2(4.0, 0.0)),
            Rect::from_min_max(pos2(r.right() - 40.0, r.top()), r.max),
        );
        let words = if b.price > 0 {
            Rect::from_min_max(words.min, pos2(price.left(), words.bottom()))
        } else {
            words
        };

        let state = state.pressed(look.2);
        let key = *key;
        let (ink, hot) = control.map_or((NAVY, crate::menu::font::HOT_KEY), |c| {
            (sheet::ink(c, state), sheet::hot_ink(c, state))
        });

        canvas.text(
            BAND_FONT,
            words,
            &sheet::hot_markup(label, key, (ink, hot)),
            (ink, Align::Centre),
        );

        if b.price > 0 {
            let colour = match (short, control) {
                (true, Some(c)) => sheet::ink(c, State::Disabled),
                (true, None) => SHORT,
                (false, _) => ink,
            };

            canvas.text(
                SMALL_FONT,
                price,
                &b.price.to_string(),
                (colour, Align::Centre),
            );
        }

        Canvas::tip(&response, tip.as_deref());

        let able = Able::of(b.ready);
        let key = crate::input::hotkey::hit_in(canvas.ui, key, able);

        if b.ready && response.clicked() || key {
            orders.orders.outbox.push(*input);
        }
    }
}

/// A recruit button's control, its hot key and tooltip: the band control
/// with a hot key whose tooltip names `title` last ("Recruit a Paladin
/// "), of those the width nearest `wide`. Every guild's lone recruit is R
/// (8008; the warriors' guild's 5001); the warriors' paladin P, warrior
/// W, warrior of discord D. **Inferred**: the pairing;
/// `WarGuildController` is not read through.
fn band_control<'l>(layout: &'l Layout, title: &str, wide: f32) -> Option<&'l ShownControl> {
    let title = title.trim().to_lowercase();

    layout
        .controls
        .iter()
        .filter(|c| c.kind == Some(regent_core::data::smnu::Kind::Button))
        .filter(|c| {
            c.tooltip
                .as_deref()
                .is_some_and(|t| t.trim().to_lowercase().ends_with(&title))
        })
        .filter_map(|c| Some((c, c.hotkey.and(sheet::rect(c))?)))
        .filter(|(_, r)| (r.top() - BAND.top()).abs() < 1.0)
        .map(|(c, r)| (c, r.width()))
        .min_by(|(_, a), (_, b)| (a - wide).abs().total_cmp(&(b - wide).abs()))
        .map(|(c, _)| c)
}

/// A tower's enchantment bar, `elapsed` of `length` ms, and its words.
fn enchanting(canvas: &mut Canvas<'_>, layout: &Layout, (elapsed, length): (i32, i32)) {
    #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
    let filled = (elapsed as f32 / length.max(1) as f32).clamp(0.0, 1.0);

    if let Some(c) = layout.control(ENCHANT_BAR) {
        sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
    }

    if let Some(c) = layout.control(ENCHANT_TEXT)
        && let (Some(t), Some(r)) = (c.text.as_deref(), sheet::rect(c))
    {
        sheet::label(canvas, c, r, t);
    }
}

/// The crown's bounty, as the Hall of Champions' panel shows it:
/// elapsed of length while it runs.
pub(super) fn calling(game: &Game<'_>, chosen: &Chosen) -> Option<(i32, i32)> {
    (chosen.panel == Some(HALL_PANEL))
        .then(|| game.world.bounty_running(game.local))
        .flatten()
}

/// The hall's bar and its words while a bounty runs.
const CALLING_BAR: i32 = 4500;
const CALLING_TEXT: i32 = 4501;

/// A tower's enchantment bar, and its "Enchantment Duration." (`AP54`).
const ENCHANT_BAR: i32 = 8202;
const ENCHANT_TEXT: i32 = 8723;

#[cfg(test)]
mod tests {
    use regent_core::data::smnu::{self, Kind};

    use super::*;

    /// A band button as `AP52` holds it.
    fn button(id: i32, (x, width): (i32, i32), key: u8, tip: &str) -> ShownControl {
        ShownControl {
            kind: Some(Kind::Button),
            id: Some(id),
            rect: Some(smnu::Rect {
                x,
                y: 190,
                width,
                height: 27,
            }),
            hotkey: Some(i32::from(key)),
            tooltip: Some(tip.to_owned()),
            ..ShownControl::default()
        }
    }

    #[test]
    fn a_recruit_takes_the_key_of_the_band_control_naming_it() {
        let layout = Layout {
            id: FourCc(*b"AP52"),
            panel: ShownControl::default(),
            centred: false,
            style: None,
            controls: vec![
                button(5000, (7, 94), b'D', "Recruit a Warrior of Discord "),
                button(8008, (7, 94), b'P', "Recruit a Paladin "),
                button(8910, (102, 94), b'W', "Recruit a Warrior "),
                button(5001, (7, 189), b'R', "Recruit a Warrior "),
            ],
        };
        let key = |title, wide| {
            band_control(&layout, title, wide)
                .and_then(|c| c.hotkey)
                .and_then(|k| u8::try_from(k).ok())
        };

        assert_eq!(key("Warrior", BAND.width()), Some(b'R'));
        assert_eq!(key("Warrior", BAND.width() / 2.0), Some(b'W'));
        assert_eq!(key("Paladin", BAND.width() / 2.0), Some(b'P'));
        assert_eq!(key("Warrior of Discord", BAND.width() / 2.0), Some(b'D'));
        assert_eq!(key("Healer", BAND.width()), None);
    }
}
