//! The Fairgrounds' contests and the Gambling Hall's wheel, on their
//! building panels. **Binary** unless marked.
//!
//! ```text
//!  AP08 FairgroundController        radio 1200-1204 ─> Set APB:5
//!  APb8 GamblingHallController
//!    open, back in the bet, the treasury moved ─> Set APB:47
//!    open                                     ─> Set APB:46
//!    colour 1200-1202 ─> Set APB:46     - + ─> the wager, Set APB:47
//!    SPIN ─> Set APB:47, Gamble         MAKE NEW BET ─> NewBet
//! ```

use bevy_egui::egui::Color32;
use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::{Text, printf};
use regent_core::sim::game::Input;
use regent_core::sim::world::{
    World,
    alter::Set,
    guild::gamble::{AMOUNT_KEY, BetLimits, COLOUR_KEY, Table, Wager},
};
use regent_core::view::menu::{Layout, ShownControl, Tick};
use regent_core::view::panel::upper::{CONTEST_ICONS, CONTESTS};

use super::canvas::{Align, Canvas};
use super::ink::Pic;
use super::sheet::{self, Able, Lit};
use super::upper::Game;
use crate::input::orders;

/// The Gambling Hall's bet's controls: the
/// colour radios by `APB:46`, red, blue, gold; the wager and its - and
/// +; SPIN; MAKE NEW BET.
const COLOURS: [i32; 3] = [1201, 1200, 1202];
const WAGER: i32 = 9011;
const LESS: i32 = 9012;
const MORE: i32 = 9013;
const SPIN: i32 = 3710;
const NEW_BET: i32 = 8834;

/// The spin's bar and words; the result's
/// pictures by `APB:50`: red, blue, gold, the skulls.
const SPIN_BAR: i32 = 8201;
const SPIN_TEXT: i32 = 8826;
const RESULTS: [i32; 4] = [8502, 8501, 8503, 8504];

/// The result's line, over its picture:
/// `gpltext.cam`'s `STRT GAMX` by `APB:50`, the won key with `APB:53`
/// above 0, else the lost; the skulls have one.
const RESULT_LINE: i32 = 8827;
const RESULT_TABLE: &[u8; 4] = b"GAMX";
const RESULT_KEYS: [(&[u8; 4], &[u8; 4]); 4] = [
    (b"REDW", b"REDL"),
    (b"BLKW", b"BLKL"),
    (b"GLDW", b"GLDL"),
    (b"SKUL", b"SKUL"),
];

/// Every control the hall's three states share.
pub const GAMBLE_IDS: [i32; 13] = [
    1200,
    1201,
    1202,
    WAGER,
    LESS,
    MORE,
    SPIN,
    NEW_BET,
    RESULTS[1],
    RESULTS[0],
    RESULTS[2],
    RESULTS[3],
    RESULT_LINE,
];

/// The spin's bar's colour, as a recruit's.
const WORKING: Color32 = Color32::from_rgb(170, 150, 90);

/// How long after SPIN the bet stays shut while the world has not yet
/// shown the spin, game ms: the send is decoded the frame after and the
/// world steps it the frame after that (`alter::SEND_TO_STEP`), so three
/// frames. **Inferred**: the engine shuts it while the message is queued;
/// a refused spin never shows one.
const SPUN_WAIT: u32 = 150;

/// An open Fairgrounds' contests: their pictures and radios, the radio
/// of `APB:5` lit; on the crown's, a radio sends its contest.
pub fn contests(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (world, id, owned): (&World, u32, bool),
    orders: &mut orders::Panel,
) {
    let event = world.word_of(id, attrib::CURRENT_EVENT);

    for c in CONTEST_ICONS.iter().filter_map(|&i| layout.control(i)) {
        sheet::picture(canvas, c);
    }

    for (value, control) in (0..).zip(CONTESTS) {
        let lit = if value == event { Lit::On } else { Lit::Off };

        if let Some(c) = layout.control(control)
            && sheet::check(canvas, c, lit, Able::of(owned))
        {
            orders
                .orders
                .outbox
                .push(set(id, attrib::CURRENT_EVENT, value));
        }
    }
}

/// `AlterAttributeMessage` of one key on `agent`.
const fn set(agent: u32, key: attrib::Key, value: i32) -> Input {
    Input::Set(Set { agent, key, value })
}

/// A Gambling Hall's panel between frames: the panel's own wager
/// and lit colour, the treasury and state it last saw, and
/// when SPIN was pressed, till the world shows the spin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bet {
    hall: u32,
    amount: i32,
    colour: i32,
    gold: i32,
    betting: bool,
    spun: Option<u32>,
}

/// What the hall's panel draws this frame.
enum Shows {
    Bet(Wager),
    Spin(f32),
    /// The winner and the winnings.
    Result(i32, i32),
}

/// Gambling Hall `id`'s three states on the crown's panel, `bet` its
/// state between frames.
pub fn gamble(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, text, id): (&Game<'_>, &Text, u32),
    bet: &mut Option<Bet>,
    orders: &mut orders::Panel,
) {
    let (world, program) = (game.world, game.program);
    let Some(table) = world.table(program, id) else {
        return;
    };
    let gold = world.treasury(game.local).unwrap_or(0);
    let limits = BetLimits::of(program);
    let clamp = |amount, colour| Wager::of(amount, colour, gold, limits);
    let betting = table == Table::Bet;
    let now = world.now();
    let (mut b, sent) = noticed(world, id, *bet, (gold, betting, limits));

    orders.orders.outbox.extend(sent);

    b.spun = b
        .spun
        .filter(|&at| betting && now.saturating_sub(at) <= SPUN_WAIT);

    let shows = match table {
        Table::Bet if b.spun.is_some() => Shows::Spin(0.0),
        Table::Bet => Shows::Bet(clamp(b.amount, b.colour)),
        Table::Spin { elapsed, interval } => {
            #[allow(clippy::cast_precision_loss, reason = "game ms within a spin")]
            let filled = elapsed as f32 / interval.max(1) as f32;

            Shows::Spin(filled)
        }
        Table::Result { winner, winnings } => Shows::Result(winner, winnings),
    };

    let new_bet = match shows {
        Shows::Bet(w) => {
            let pressed = wager(canvas, layout, &w);

            press(&mut b, pressed, limits, gold);

            if pressed == Some(Press::Spin) {
                b.spun = Some(now);
            }

            orders
                .orders
                .outbox
                .extend(pressed.map(|p| sends(&b, p)).unwrap_or_default());

            w.broke()
        }
        Shows::Spin(filled) => {
            if let Some(c) = layout.control(SPIN_BAR) {
                sheet::art_bar(canvas, c, (filled, sheet::Fill::Plain), WORKING);
            }

            if let Some(t) = layout.control(SPIN_TEXT).and_then(|c| c.text.clone()) {
                sheet::fill(canvas, layout, SPIN_TEXT, t.trim(), (None, Align::Centre));
            }

            true
        }
        Shows::Result(winner, winnings) => {
            let c = usize::try_from(winner)
                .ok()
                .and_then(|w| RESULTS.get(w))
                .and_then(|&r| layout.control(r));

            if let Some(c) = c {
                sheet::picture(canvas, c);
                tip(canvas, c);
            }

            if let Some(line) = result_line(text, winner, winnings) {
                sheet::fill(canvas, layout, RESULT_LINE, &line, (None, Align::Centre));
            }

            true
        }
    };

    if new_bet
        && let Some(c) = layout.control(NEW_BET)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        orders.orders.outbox.push(Input::NewBet(id));
    }

    b.gold = gold;
    b.betting = betting;
    *bet = Some(b);
}

/// The panel `bet` was for hall `id`, the treasury `gold` and whether
/// the world shows the bet now: as opened, the wager and colour
/// read from the hall, both sent in the bet; open, the wager sent again
/// back in the bet (`APB:48` 0) or with the treasury moved (the
/// controller's attribute notices, `GamblingHallController`; **Inferred**: told
/// apart here by frame, not by key). The wager kept within its limits in
/// the bet, as each send keeps it.
fn noticed(
    world: &World,
    id: u32,
    bet: Option<Bet>,
    (gold, betting, limits): (i32, bool, BetLimits),
) -> (Bet, Vec<Input>) {
    let read = |key| world.word_of(id, key);
    let (mut b, (amount, colour)) = match bet.filter(|b| b.hall == id) {
        None => {
            let b = Bet {
                hall: id,
                amount: read(AMOUNT_KEY),
                colour: read(COLOUR_KEY),
                gold,
                betting,
                spun: None,
            };

            (b, (betting, betting))
        }
        Some(b) => (b, (betting && (!b.betting || b.gold != gold), false)),
    };

    if betting {
        b.amount = Wager::of(b.amount, b.colour, gold, limits).amount;
    }

    let sent = [
        amount.then(|| set(id, AMOUNT_KEY, b.amount)),
        colour.then(|| set(id, COLOUR_KEY, b.colour)),
    ];

    (b, sent.into_iter().flatten().collect())
}

/// What press `p` on bet `b` sends: a
/// colour its own; - and + the wager; SPIN the
/// wager, then the spin.
fn sends(b: &Bet, p: Press) -> Vec<Input> {
    match p {
        Press::Colour(_) => vec![set(b.hall, COLOUR_KEY, b.colour)],
        Press::Less | Press::More => vec![set(b.hall, AMOUNT_KEY, b.amount)],
        Press::Spin => vec![set(b.hall, AMOUNT_KEY, b.amount), Input::Gamble(b.hall)],
    }
}

/// A press on the bet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Press {
    Colour(i32),
    Less,
    More,
    Spin,
}

/// `pressed` on the panel's wager and colour: - and + move the wager by
/// the step, then keep it within the limits and `gold`.
fn press(b: &mut Bet, pressed: Option<Press>, limits: BetLimits, gold: i32) {
    let step = match pressed {
        Some(Press::Less) => -limits.step,
        Some(Press::More) => limits.step,
        _ => 0,
    };

    if let Some(Press::Colour(colour)) = pressed {
        b.colour = colour;
    }

    b.amount = Wager::of(b.amount + step, b.colour, gold, limits).amount;
}

/// The bet's controls `w` shows.
pub fn bet_controls(w: &Wager) -> Vec<i32> {
    let shown = [
        (LESS, w.less()),
        (MORE, w.more()),
        (SPIN, w.spin()),
        (NEW_BET, w.broke()),
    ];

    COLOURS
        .into_iter()
        .chain([WAGER])
        .chain(shown.into_iter().filter(|(_, on)| *on).map(|(c, _)| c))
        .collect()
}

/// The bet's controls `w` shows, each at its place, the colour lit;
/// what was pressed. MAKE NEW BET is the caller's.
fn wager(canvas: &mut Canvas<'_>, layout: &Layout, w: &Wager) -> Option<Press> {
    let shown = bet_controls(w);
    let mut pressed = None;

    for (colour, control) in (0..).zip(COLOURS) {
        let lit = if colour == w.colour {
            Lit::On
        } else {
            Lit::Off
        };

        if let Some(c) = layout.control(control) {
            if sheet::check(canvas, c, lit, Able::Enabled) {
                pressed = Some(Press::Colour(colour));
            }

            // Its colour's name, "BLUE", over the picture, in its own colour.
            if let (Some(t), Some(r), Some(font)) = (c.text.as_deref(), sheet::rect(c), c.font) {
                let state = Able::Enabled.state(c.flags);
                let shown = sheet::marked(c, t.trim(), state);

                canvas.text(font, r, &shown, (sheet::ink(c, state), Align::Centre));
            }
        }
    }

    sheet::fill(
        canvas,
        layout,
        WAGER,
        &w.amount.to_string(),
        (None, Align::Centre),
    );

    for (control, press) in [(LESS, Press::Less), (MORE, Press::More)] {
        if shown.contains(&control)
            && let Some(c) = layout.control(control)
            && (sheet::button(canvas, c, None, Able::Enabled) | repeats(canvas, c))
        {
            pressed = Some(press);
        }
    }

    if shown.contains(&SPIN)
        && let Some(c) = layout.control(SPIN)
        && spin(canvas, c)
    {
        pressed = Some(Press::Spin);
    }

    pressed
}

/// A held - or + presses again. Its delay
/// and rate are Windows' key repeat defaults (**Inferred**: the engine's
/// are not read).
const REPEAT_DELAY: f64 = 0.5;
const REPEAT_EVERY: f64 = 0.1;

/// Whether control `c`, held down, repeats its press this frame.
fn repeats(canvas: &Canvas<'_>, c: &ShownControl) -> bool {
    let Some(r) = sheet::rect(c).map(|r| canvas.fit.rect(r)) else {
        return false;
    };

    canvas.ui.ctx().input(|i| {
        let held = i.pointer.primary_down()
            && i.pointer.press_origin().is_some_and(|at| r.contains(at))
            && i.pointer.hover_pos().is_some_and(|at| r.contains(at));
        let ticks = |t: f64| ((t - REPEAT_DELAY) / REPEAT_EVERY).floor();

        match (held, i.pointer.press_start_time()) {
            (true, Some(start)) => {
                let now = i.time - start;
                let dt = f64::from(i.unstable_dt);

                now >= REPEAT_DELAY && ticks(now) > ticks(now - dt)
            }
            _ => false,
        }
    })
}

/// SPIN: its picture, `INBb` 1007, is 89 wide, its rectangle 45
/// (**Inferred**: widget draw clips to the rectangle); whether it
/// was pressed, by click or hot key.
fn spin(canvas: &mut Canvas<'_>, c: &ShownControl) -> bool {
    let Some(r) = sheet::rect(c) else {
        return false;
    };
    let response = canvas.press(r, ("spin", SPIN));
    let look = sheet::look(&response, Tick::Clear);

    if let Some((record, set)) = c.art {
        let pic = Pic::of(record, set);
        let (slot, tone) = sheet::slot(canvas, pic, look, Able::Enabled.state(c.flags));

        canvas.clipped(pic.slot(slot), tone, r.min, r);
    }

    if let (Some(t), Some(font)) = (c.text.as_deref(), c.font) {
        let state = Able::Enabled.state(c.flags).pressed(look.2);
        let shown = sheet::marked(c, t.trim(), state);

        canvas.text(font, r, &shown, (sheet::ink(c, state), Align::Centre));
    }

    Canvas::tip(&response, c.tooltip.as_deref());

    response.clicked() || crate::input::hotkey::hit_in(canvas.ui, c.hotkey, Able::Enabled)
}

/// The result's line: `winner`'s `GAMX` string, `winnings` in it.
fn result_line(text: &Text, winner: i32, winnings: i32) -> Option<String> {
    let (won, lost) = usize::try_from(winner)
        .ok()
        .and_then(|w| RESULT_KEYS.get(w))?;
    let key = if winnings > 0 { won } else { lost };
    let format = text.keyed(RESULT_TABLE, FourCc(**key))?;

    Some(printf(&format, &[winnings.into()]).trim().to_owned())
}

/// Control `c`'s tooltip over its rectangle.
fn tip(canvas: &mut Canvas<'_>, c: &ShownControl) {
    if let (Some(r), Some(id)) = (sheet::rect(c), c.id) {
        let response = canvas.press(r, ("result", id));

        Canvas::tip(&response, c.tooltip.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMITS: BetLimits = BetLimits {
        least: 25,
        most: 1000,
        step: 25,
    };

    fn bet(amount: i32) -> Bet {
        Bet {
            hall: 10,
            amount,
            colour: 0,
            gold: 5000,
            betting: true,
            spun: None,
        }
    }

    /// The - and + buttons move the wager a step, kept within the
    /// treasury and the most; a colour lights its own and keeps the
    /// wager.
    #[test]
    fn the_steps_move_the_wager_within_its_limits() {
        let mut b = bet(200);

        press(&mut b, Some(Press::More), LIMITS, 5000);
        assert_eq!(b.amount, 225);

        press(&mut b, Some(Press::Less), LIMITS, 5000);
        press(&mut b, Some(Press::Less), LIMITS, 5000);
        assert_eq!(b.amount, 175);

        press(&mut b, Some(Press::More), LIMITS, 180);
        assert_eq!(b.amount, 180);

        let mut most = bet(1000);

        press(&mut most, Some(Press::More), LIMITS, 5000);
        assert_eq!(most.amount, 1000);

        press(&mut most, Some(Press::Colour(2)), LIMITS, 5000);
        assert_eq!((most.colour, most.amount), (2, 1000));
    }

    /// The result's line is `gpltext.cam`'s `STRT GAMX` by winner and
    /// whether anything was won (`ui_gambling_hall_*_win.png`,
    /// `_loss.png`).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_result_line_says_who_won_and_what() {
        use regent_core::data::text::Text;

        let catalogue = crate::testdata::hd();
        let text = Text::build(catalogue);
        let line = |winner, winnings| result_line(&text, winner, winnings);

        assert_eq!(line(0, 125).as_deref(), Some("Red Won! you win 125 gold"));
        assert_eq!(line(0, 0).as_deref(), Some("Red Won! you lost"));
        assert_eq!(line(1, 0).as_deref(), Some("Blue Won! you lost"));
        assert_eq!(line(2, 500).as_deref(), Some("Gold Won! you win 500 gold"));
        assert_eq!(line(3, 0).as_deref(), Some("Skulls! you lost"));
        assert_eq!(line(4, 0), None);
    }

    /// SPIN sends the wager, then the spin, as the probe's eight spins
    /// (**Measured**); a colour sends only its own.
    #[test]
    fn spin_sends_the_wager_then_the_spin() {
        let b = bet(225);
        let wager = set(10, AMOUNT_KEY, 225);

        assert_eq!(sends(&b, Press::Spin), [wager, Input::Gamble(10)]);
        assert_eq!(sends(&b, Press::Less), [wager]);
        assert_eq!(sends(&b, Press::Colour(0)), [set(10, COLOUR_KEY, 0)]);
    }
}
