//! The main screen's frame from HD's `APMK` at 1920 × 1200: the column's
//! edges and buttons, the top bar (gold, day), the bottom bar (SPELLS,
//! messages, OPTIONS) and the spell bar (`AP92`) over the map.
//!
//! ```text
//!  APMK piece          where it goes
//!  x < 206, y < 848    the column, at its scale
//!  y < 29              the top bar: from its left or right edge,
//!  y ≥ 1165            the bottom bar   a piece wider than 1000 stretched
//!  x ≥ 1916            the map's right edge: not drawn
//! ```

use bevy_egui::egui::{Color32, Pos2, Rect, pos2, vec2};
use regent_core::data::fourcc::FourCc;
use regent_core::data::smnu;
use regent_core::data::text::Text;
use regent_core::sim::game::orders::{self as sends, cast_now};
use regent_core::sim::world::{
    World,
    guild::rage::{KROLM_TEMPLE, Rage},
    offer::{Aim, SPELLS, SpellOffer},
};
use regent_core::view::menu::{Hover, Layout, Press, ShownControl, State, Tick, drawn_slot};
use regent_core::view::panel::day::Day;
use regent_core::view::panel::spell_bar::{ABODE_ICONS, Button, RAGE_ICON, RESTING_BAR};
use regent_core::view::panel::tip;
use regent_draw::data::Corpus;

use super::canvas::{Align, Canvas, WHITE};
use super::ink::{Pic, Tone};
use super::sheet::{self, Able};
use super::upper::Game;
use crate::files::config::Enhancements;
use crate::input::orders::{self, Mode};
use crate::menu::fit::Fit;
use crate::ui::layout::{Anchor, COLUMN, Frame, Grid, SCREEN_WIDTH, TOP_BAR};

/// `APMK`'s controls the column and bars fill themselves.
pub mod id {
    pub const MINIMAP: i32 = 2001;
    pub const MINI_VIEW: i32 = 2002;
    pub const SELECTION: i32 = 2005;
    pub const PAGE: i32 = 4000;
    pub const GOLD: i32 = 2006;
    pub const DAY: i32 = 7021;
    pub const DAY_BAR: i32 = 2020;
    pub const MESSAGE: i32 = 2004;
    pub const SPELLS: i32 = 6002;
    pub const OPTIONS: i32 = 9005;
    /// The title scroll's press (`MainFrameController`, 0x7d8).
    pub const SCROLL: i32 = 2008;
    /// The minimap's buttons: hit point bars, unit names, the zoom,
    /// minimap textures, select the palace.
    pub const BARS: i32 = 18;
    pub const NAMES: i32 = 19;
    pub const ZOOM: i32 = 8734;
    pub const TEXTURES: i32 = 21;
    pub const PALACE: i32 = 2009;
}

/// The column's buttons [`column_buttons`] draws.
const LIVE: [i32; 5] = [id::BARS, id::NAMES, id::ZOOM, id::TEXTURES, id::PALACE];

/// The inert check as it stands: the minimap's textures on
/// (minimap filter bit 2, `Filter::DEFAULT`).
const INERT: [(i32, Tick); 1] = [(id::TEXTURES, Tick::Checked)];

/// The bottom bar's top in HD's 1920 × 1200 `APMK`.
const BOTTOM_TOP: f32 = 1165.0;

/// A piece this wide or wider spans its bar's blank middle.
const STRETCH: f32 = 1000.0;

/// The map's right edge pieces start here.
const RIGHT_EDGE: f32 = 1916.0;

/// HD's screen centre: a piece within [`CENTRED`] of it is centred.
const CENTRE: f32 = SCREEN_WIDTH / 2.0;

/// The bars' centre on HD's screen, right of the column: a centred
/// piece keeps its distance from it. **Game**: every `UIData` puts the
/// title scroll about 20 pixels left of it, hand placed (x 398 at 1024
/// wide, 723 at 1680, 844 at 1920), not about the screen's centre.
const BAR_CENTRE: f32 = f32::midpoint(COLUMN.x, SCREEN_WIDTH);

/// How far from the centre a piece counts as centred.
const CENTRED: f32 = 200.0;

/// The day bar's colour (**Inferred** stand-in for `IMZa`).
const DAYLIGHT: Color32 = Color32::from_rgb(170, 130, 40);

/// The spell bar, `AP92` (`FastBar`).
pub const SPELL_BAR: FourCc = FourCc(*b"AP92");

/// The bar's frame picture, `AP92`'s control 1 (`INDz` 1012).
const SPELL_FRAME: i32 = 1;

/// The state a spell the crown may not cast draws: slot 2, its icon grey
/// in its temple's colour (**Game**: `ui_full.png`, every spell but the
/// two wizard ones; **Inferred**: a disabled control's state). The
/// pointer adds 6 (`menu::drawn_slot`).
const UNCASTABLE: State = State::Disabled;

/// The chosen spell's underline, pixels high.
const UNDERLINE: f32 = 2.0;

/// Where an `APMK` piece belongs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Column,
    Top,
    Bottom,
    Elsewhere,
}

/// `rect`'s place, HD pixels.
#[must_use]
pub fn place(rect: Rect) -> Place {
    if rect.left() < COLUMN.x && rect.top() < COLUMN.y {
        Place::Column
    } else if rect.left() >= RIGHT_EDGE - 1.0 && rect.width() < 10.0 {
        Place::Elsewhere
    } else if rect.top() < TOP_BAR && rect.left() >= COLUMN.x - 1.0 {
        Place::Top
    } else if rect.top() >= BOTTOM_TOP && rect.left() >= COLUMN.x - 4.0 {
        Place::Bottom
    } else {
        Place::Elsewhere
    }
}

/// A bar piece's rectangle on the window: from its edge, centred, or
/// stretched across.
#[must_use]
pub fn bar_rect(frame: &Frame, rect: Rect, (top, bar): (f32, Rect)) -> Rect {
    let s = frame.scale;
    let y = bar.top() + (rect.top() - top) * s;
    let size = vec2(rect.width() * s, rect.height() * s);
    let centre = rect.center().x;

    let (left, right) = if rect.width() >= STRETCH {
        (
            frame.bar_x(rect.left(), Anchor::Left),
            frame.bar_x(rect.right(), Anchor::Right),
        )
    } else if (centre - CENTRE).abs() < CENTRED {
        let x = bar.center().x + (rect.left() - BAR_CENTRE) * s;

        (x, x + size.x)
    } else if centre < CENTRE {
        let x = frame.bar_x(rect.left(), Anchor::Left);

        (x, x + size.x)
    } else {
        let x = frame.bar_x(rect.left(), Anchor::Right);

        (x, x + size.x)
    };

    Rect::from_min_max(pos2(left, y), pos2(right.max(left), y + size.y))
}

/// The column's frame pieces: every `APMK` picture in it but the slots.
pub fn column(canvas: &mut Canvas<'_>, main: &Layout) {
    let slots = [id::MINIMAP, id::MINI_VIEW, id::SELECTION, id::PAGE];
    let clip = Rect::from_min_size(Pos2::ZERO, COLUMN);

    if let Some((record, set)) = main.panel.art {
        canvas.tiled(Pic::of(record, set), clip);
    }

    for c in &main.controls {
        let Some(r) = sheet::rect(c) else { continue };

        if place(r) != Place::Column
            || c.id
                .is_some_and(|i| slots.contains(&i) || LIVE.contains(&i) || row_drawn(i))
        {
            continue;
        }

        let r = r.intersect(clip);

        if let Some((record, set)) = c.art {
            let tone = if c.kind == Some(regent_core::data::smnu::Kind::Button) && c.id.is_some() {
                Tone::Dim
            } else {
                Tone::Plain
            };

            canvas.stretched(Pic::of(record, set), tone, r);
        }

        if let Some(tip) = c.tooltip.as_deref() {
            let response = canvas.hover(
                r,
                ("column tip", c.id, r.min.x.to_bits(), r.min.y.to_bits()),
            );

            Canvas::tip(&response, Some(tip));
        }
    }

    for control in [id::MINIMAP, id::MINI_VIEW] {
        if let Some(c) = main.control(control)
            && let Some(r) = sheet::rect(c)
        {
            let response = canvas.hover(r, ("column slot tip", control));

            Canvas::tip(&response, c.tooltip.as_deref());
        }
    }
}

/// The tracking window's button row:
/// the four filters and Autoscan checked as `mini` says, back, Autoscan
/// and forward disabled with no filter on, saying "All filters are off"
/// (**Game**: `dragon_hover.png` greys those three, the four filters
/// not); each with its row's tooltip. The column pressed, and its check
/// after the press.
pub fn mini_row(
    canvas: &mut Canvas<'_>,
    main: &Layout,
    text: &Text,
    mini: &regent_core::view::panel::tracking::Tracker,
) -> Option<(usize, bool)> {
    use regent_core::view::panel::tracking::{Category, column};

    let own = |c: i32| main.control(c).and_then(|c| c.tooltip.clone());
    let clip = Rect::from_min_size(Pos2::ZERO, COLUMN);
    let mut pressed = None;

    for &control in tip::MINI_ROWS.iter().flatten() {
        let Some(c) = main.control(control) else {
            continue;
        };
        let Some(r) = sheet::rect(c).filter(|r| place(*r) == Place::Column) else {
            continue;
        };
        let at = tip::mini_column(control).unwrap_or_default();
        let live = tip::MINI_ROWS[0].contains(&control) && at >= column::FLAGS;
        let off = (column::BACK..=column::FORWARD).contains(&at) && !mini.scans();
        let able = Able::of(!off);
        let tick = match (Category::of_column(at), at) {
            (Some(category), _) => Some(mini.filters.on(category)),
            (None, column::AUTOSCAN) => Some(mini.autoscan),
            _ => None,
        };
        let response = canvas.press(r.intersect(clip), ("mini row", control));

        if live && let Some((record, set)) = c.art {
            let pic = Pic::of(record, set);
            let tick = if tick == Some(true) {
                Tick::Checked
            } else {
                Tick::Clear
            };
            let state = able.state(c.flags);
            let look = sheet::look(&response, tick.shown_by(c.kind));
            let (slot, tone) = sheet::slot(canvas, pic, look, state);

            canvas.picture(pic.slot(slot), tone, r.min);
        }

        if let Some(said) = tip::mini(text, control, able, own) {
            Canvas::tip(&response, Some(&said));
        }

        if live && able == Able::Enabled && response.clicked() {
            pressed = Some((at, !tick.unwrap_or(false)));
        }
    }

    pressed
}

/// Whether control `id` is one [`mini_row`] draws itself.
fn row_drawn(id: i32) -> bool {
    tip::MINI_ROWS[0][regent_core::view::panel::tracking::column::FLAGS..].contains(&id)
}

/// The top and bottom bars' backgrounds and pieces, the labels left to
/// the callers: their places on the window.
pub fn bars(canvas: &mut Canvas<'_>, main: &Layout, frame: &Frame) {
    let back = main.panel.art.map(|(record, set)| Pic::of(record, set));

    canvas.fit = WINDOW;

    for (bar, top) in [(frame.top, 0.0), (frame.bottom, BOTTOM_TOP)] {
        if let Some(pic) = back {
            canvas.tiled(pic, bar);
        }

        let wanted = if top > 0.0 { Place::Bottom } else { Place::Top };

        for (pic, r) in pieces(main, wanted) {
            canvas.stretched(pic, Tone::Plain, bar_rect(frame, r, (top, bar)));
        }
    }
}

/// The pictures [`bars`] draws in bar `wanted`, in order, and their HD
/// rectangles: not SPELLS and OPTIONS, which [`buttons`] draws, nor the
/// empty picture, which would veil the title scroll half black.
fn pieces(main: &Layout, wanted: Place) -> impl Iterator<Item = (Pic, Rect)> + '_ {
    main.controls.iter().filter_map(move |c| {
        let r = sheet::rect(c).filter(|r| place(*r) == wanted)?;
        let (record, set) = c.art.filter(|&a| a != sheet::NO_ART)?;
        let own = [id::SPELLS, id::OPTIONS].contains(&c.id.unwrap_or(0));

        (!own).then_some((Pic::of(record, set), r))
    })
}

/// A fit drawing window points one to one.
pub const WINDOW: Fit = Fit {
    scale: 1.0,
    origin: Pos2::ZERO,
};

/// A control `c` of a bar as a canvas fitted over its window rectangle:
/// its own (0, 0) at the top left.
pub fn fitted(frame: &Frame, c: &ShownControl) -> Option<(Fit, Rect)> {
    let r = sheet::rect(c)?;
    let (top, bar) = match place(r) {
        Place::Top => (0.0, frame.top),
        Place::Bottom => (BOTTOM_TOP, frame.bottom),
        _ => return None,
    };
    let at = bar_rect(frame, r, (top, bar));

    Some((
        Fit {
            scale: frame.scale,
            origin: at.min,
        },
        Rect::from_min_size(Pos2::ZERO, r.size()),
    ))
}

/// Control `c` of the bottom bar's width as [`bar_rect`] stretches it,
/// panel pixels.
pub fn stretched(frame: &Frame, c: &ShownControl) -> Option<f32> {
    let r = sheet::rect(c).filter(|r| place(*r) == Place::Bottom)?;

    Some(bar_rect(frame, r, (BOTTOM_TOP, frame.bottom)).width() / frame.scale)
}

/// The gold, the day's bar and text, or `FRAME IT`'s in its place;
/// whether the day's text was clicked (`MainFrameController`, 7021).
pub fn labels(
    canvas: &mut Canvas<'_>,
    main: &Layout,
    frame: &Frame,
    (gold, (day, over)): (Option<i32>, (Day, Option<&str>)),
    text: &Text,
) -> bool {
    let day_text = over.map_or_else(
        || day.text(text).unwrap_or_else(|| day.number.to_string()),
        str::to_owned,
    );
    let gold = gold.map_or_else(|| "?".to_owned(), |g| g.to_string());
    let lines = [(id::GOLD, Some(gold)), (id::DAY, Some(day_text))];

    if let Some(c) = main.control(id::DAY_BAR)
        && let Some((fit, _)) = fitted(frame, c)
    {
        canvas.fit = fit;
        sheet::art_bar(
            canvas,
            &at_origin(c),
            (day.filled(), sheet::Fill::Plain),
            DAYLIGHT,
        );
    }

    for (control, line) in lines {
        let shown = main.control(control).zip(line);

        if let Some((c, line)) = shown
            && let (Some((fit, r)), Some(font)) = (fitted(frame, c), c.font)
        {
            canvas.fit = fit;
            canvas.text(font, r, &line, (sheet::colour(c), Align::Centre));

            if control == id::GOLD {
                let response = canvas.hover(r, ("gold tip", control));

                Canvas::tip(&response, c.tooltip.as_deref());
            }
        }
    }

    main.control(id::DAY)
        .and_then(|c| fitted(frame, c))
        .is_some_and(|(fit, r)| {
            canvas.fit = fit;
            let response = canvas.press(r, ("day", id::DAY));

            Canvas::tip(
                &response,
                main.control(id::DAY).and_then(|c| c.tooltip.as_deref()),
            );

            response.clicked()
        })
}

/// The pause line's gap over the map's bottom, or the spell bar's top,
/// HD pixels.
const PAUSE_GAP: f32 = 40.0;

/// The pause `line` in the bottom bar message's font and colour (`id::MESSAGE`),
/// centred across the map near its bottom, over the spell bar. A
/// departure: the original writes it in the bottom bar.
pub fn pause_line(canvas: &mut Canvas<'_>, main: &Layout, frame: &Frame, line: &str) {
    let c = main.control(id::MESSAGE);
    let shown = c.and_then(|c| sheet::rect(c).zip(c.font).map(|(r, f)| (c, r, f)));

    if let Some((c, r, font)) = shown {
        let floor = frame.spells.min.y.min(frame.map.max.y);
        let height = r.height() * frame.scale;
        let top = floor - PAUSE_GAP * frame.scale - height;

        canvas.fit = Fit {
            scale: frame.scale,
            origin: pos2(frame.map.min.x, top),
        };

        let across = Rect::from_min_size(
            Pos2::ZERO,
            vec2(frame.map.width() / frame.scale, r.height()),
        );

        canvas.text(font, across, line, (sheet::colour(c), Align::Centre));
    }
}

/// Control `c` moved to (0, 0), drawn through its [`fitted`] canvas.
fn at_origin(c: &ShownControl) -> ShownControl {
    ShownControl {
        rect: c
            .rect
            .map(|r| regent_core::data::smnu::Rect { x: 0, y: 0, ..r }),
        ..c.clone()
    }
}

/// The bars' buttons pressed this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pressed {
    pub spells: bool,
    pub options: bool,
    /// The title scroll.
    pub scroll: bool,
}

/// The bottom bar's SPELLS and OPTIONS buttons, and the title scroll
/// naming the game (`title`): which was pressed.
pub fn buttons(canvas: &mut Canvas<'_>, main: &Layout, frame: &Frame, title: &str) -> Pressed {
    let mut press = |control: i32, text: Option<&str>| {
        let Some(c) = main.control(control) else {
            return false;
        };
        let Some((fit, _)) = fitted(frame, c) else {
            return false;
        };
        canvas.fit = fit;
        sheet::button(canvas, &at_origin(c), text, Able::Enabled)
    };

    Pressed {
        spells: press(id::SPELLS, None),
        options: press(id::OPTIONS, None),
        scroll: press(id::SCROLL, Some(title)),
    }
}

/// A press on the minimap's buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Bars,
    Names,
    Zoom,
    Palace,
}

/// The minimap's buttons in the column's `canvas`: the hit point bars,
/// names and zoom checks ticked as `ticks` say, select the palace; the
/// textures check drawn as it stands, inert. Which was pressed.
pub fn column_buttons(
    canvas: &mut Canvas<'_>,
    main: &Layout,
    ticks: (Tick, Tick, Tick),
) -> Option<Column> {
    let (bars, names, zoom) = ticks;

    for (control, tick) in INERT {
        if let Some(c) = main.control(control) {
            still(canvas, c, tick);
        }
    }

    let mut press = |control: i32, tick: Option<Tick>| {
        main.control(control).is_some_and(|c| match tick {
            Some(tick) => toggle(canvas, c, tick),
            None => sheet::button(canvas, c, None, Able::Enabled),
        })
    };

    [
        (id::BARS, Some(bars), Column::Bars),
        (id::NAMES, Some(names), Column::Names),
        (id::ZOOM, Some(zoom), Column::Zoom),
        (id::PALACE, None, Column::Palace),
    ]
    .into_iter()
    .fold(None, |hit, (control, tick, column)| {
        let now = press(control, tick);

        hit.or(now.then_some(column))
    })
}

/// Check `c` in the slot the engine draws (`menu::drawn_slot`): 3 on
/// while ticked, 6 on while hovered, plus its state; whether pressed,
/// by the pointer or its hot key (`APMK`: F9 names, F10 bars, F11 zoom).
pub fn toggle(canvas: &mut Canvas<'_>, c: &ShownControl, tick: Tick) -> bool {
    let (Some(r), Some(control), Some((record, set))) = (sheet::rect(c), c.id, c.art) else {
        return false;
    };
    let response = canvas.press(r, ("toggle", control));
    let pic = Pic::of(record, set);
    let look = sheet::look(&response, tick.shown_by(c.kind));
    let (slot, tone) = sheet::slot(canvas, pic, look, State::of(c.flags));

    canvas.picture(pic.slot(slot), tone, r.min);
    Canvas::tip(&response, c.tooltip.as_deref());

    let key = crate::input::hotkey::hit_in(canvas.ui, c.hotkey, Able::Enabled);

    response.clicked() || key
}

/// Check `c` at rest as `tick` says, taking no press.
pub fn still(canvas: &mut Canvas<'_>, c: &ShownControl, tick: Tick) {
    if let (Some(r), Some((record, set))) = (sheet::rect(c), c.art) {
        let pic = Pic::of(record, set);
        let look = (tick.shown_by(c.kind), Hover::Away, Press::Free);
        let (slot, tone) = sheet::slot(canvas, pic, look, State::of(c.flags));

        canvas.picture(pic.slot(slot), tone, r.min);

        let response = canvas.hover(r, ("still tip", c.id));

        Canvas::tip(&response, c.tooltip.as_deref());
    }
}

/// The temple of each of `buttons`, by the spell table: the spell bar's
/// groups.
#[must_use]
pub fn temples(buttons: &[Button]) -> Vec<[u8; 3]> {
    buttons
        .iter()
        .map(|b| {
            SPELLS
                .iter()
                .find(|r| r.spell == b.spell)
                .map_or([0; 3], |r| r.building)
        })
        .collect()
}

/// The temple whose Rage the spell bar offers after its spells: `local`'s
/// built Temple to Krolm, under `[enhancements] rage_on_spell_bar`.
#[must_use]
pub fn rage_temple(enhancements: &Enhancements, world: &World, local: u32) -> Option<u32> {
    enhancements
        .rage_on_spell_bar
        .then(|| world.rage_temple(local))
        .flatten()
}

/// The spell bar's groups: each button's temple, then the Rage's where
/// `rage` names a temple.
#[must_use]
pub fn groups(buttons: &[Button], rage: Option<u32>) -> Vec<[u8; 3]> {
    let mut groups = temples(buttons);

    groups.extend(rage.map(|_| KROLM_TEMPLE));
    groups
}

/// The Rage of Krolm of `temple` in the spell bar's `cell`, an enhancement
/// (`[enhancements] rage_on_spell_bar`): as its panel offers it
/// (`panel::upper::guild::rage`). Ready, lit while the crown has a hero;
/// a press sends what `orders::rage` says, its price and the Rage or
/// `GMTX` 36. Running, unlit, darkened from the top by the share left.
///
/// ```text
///  ready    running, 1/4 gone
///  ┌────┐   ┌────┐
///  │fist│   │████│  3/4 left
///  │    │   │████│
///  └────┘   └────┘
/// ```
pub fn rage(
    canvas: &mut Canvas<'_>,
    cell: Rect,
    (game, temple): (&Game<'_>, u32),
    orders: &mut orders::Panel,
) {
    let text = &game.corpus.text;
    let name = text.gmtx(RAGE_NAME).map(String::from).unwrap_or_default();
    let pic = Pic::of(RAGE_ICON.0, RAGE_ICON.1);
    let response = canvas.press(cell, ("rage", temple));
    let (_, hover, press) = sheet::look(&response, Tick::Clear);

    match game.world.rage(game.program, game.local, temple) {
        Some(Rage::Ready(b)) => {
            let (slot, tone) = look(1, Able::of(b.ready), (hover, press));
            let said = tip::appended(text, &name, tip::line::PRICE, &[b.price.into()]);

            canvas.picture(pic.slot(slot), tone, cell.min);
            Canvas::tip(&response, Some(&said));

            if response.clicked() && b.ready {
                match sends::rage(game.world, game.local, temple, b.price) {
                    Ok(inputs) => orders.orders.outbox.extend(inputs),
                    Err(line) => orders.orders.said.push(line),
                }
            }
        }
        Some(Rage::Running { elapsed, duration }) => {
            #[allow(clippy::cast_precision_loss, reason = "game ms within a day")]
            let left = 1.0 - elapsed as f32 / duration.max(1) as f32;
            let dark = Rect::from_min_size(
                cell.min,
                vec2(cell.width(), cell.height() * left.clamp(0.0, 1.0)),
            );

            canvas.picture(pic, Tone::Unlit, cell.min);
            canvas.fill(dark, RAGE_LEFT);
            Canvas::tip(&response, Some(&name));
        }
        None => {}
    }
}

/// `GMTX` 167, "Rage of Krolm": its heroes' effect's title.
const RAGE_NAME: usize = 167;

/// The Rage's share left, over its unlit icon (**Inferred**: taste).
const RAGE_LEFT: Color32 = Color32::from_black_alpha(150);

/// The spell bar, `width` panel pixels wide, `buttons` at their places in
/// `grid`, full size: each lit where the crown may cast it; a press or its
/// key aims it, or casts `SuperCharge` at once. A departure: the
/// original's bar is `AP92` as it stands (docs/openregent/screenshots.md).
pub fn spell_bar(
    canvas: &mut Canvas<'_>,
    (layout, grid, width): (&Layout, &Grid, f32),
    buttons: &[Button],
    (world, local, offers, corpus): (&World, u32, &[SpellOffer], &Corpus),
    orders: &mut orders::Panel,
) {
    let whole = Rect::from_min_size(Pos2::ZERO, vec2(width, grid.height()));

    if let Some((record, set)) = layout.control(SPELL_FRAME).and_then(|c| c.art) {
        framed(canvas, Pic::of(record, set), whole);
    }

    // The Abode resting: its bar over its six buttons, which are
    // neither drawn nor pressed (**Inferred**: 9500 follows them in `AP92`,
    // so draws over them; `FastBar` ignores their press meanwhile).
    let rest = world
        .abode_rest(local)
        .zip(layout.control(RESTING_BAR))
        .zip(rest_rect(grid, buttons));

    for (i, b) in buttons.iter().enumerate() {
        let Some(cell) = grid.cell(i) else { continue };

        if rest.is_some() && abode(b) {
            continue;
        }

        let c = layout.control(b.control);
        let offer: Option<&SpellOffer> = offers.iter().find(|o| o.row.spell == b.spell);
        // No gold asked: the
        // map click says `GMTX` 34 (`orders::cast`), SuperCharge's decode.
        // A resting Abode's press does nothing.
        let castable = offer.is_some_and(|o| o.ready);
        let chosen = offer.is_some_and(|o| orders.orders.mode == Mode::Spell(*o));
        let response = canvas.press(cell, ("spell", b.control));
        let (_, hover, press) = sheet::look(&response, Tick::Clear);
        let able = lit(offer);
        let pic = Pic::of(b.record, b.set);
        let (slot, tone) = look(sheet::slots(canvas, pic), able, (hover, press));

        canvas.picture(pic.slot(slot), tone, cell.min);

        if chosen {
            let under = Rect::from_min_size(
                pos2(cell.left(), cell.bottom() - UNDERLINE),
                vec2(cell.width(), UNDERLINE),
            );

            canvas.fill(under, WHITE);
        }

        // Spell tip: the price, or why not; made only under the
        // pointer, the one place it shows.
        let tip = c
            .filter(|_| response.hovered())
            .and_then(|c| c.tooltip.as_deref())
            .map(|own| match SPELLS.iter().find(|r| r.spell == b.spell) {
                Some(row) => {
                    let why = world
                        .spell_tip(offers, row, local)
                        .map(|code| orders.title(corpus, code));

                    tip::spell(&corpus.text, own, why.as_ref().map(String::as_str))
                }
                None => own.to_owned(),
            });

        Canvas::tip(&response, tip.as_deref());

        let key = crate::input::hotkey::hit_in(canvas.ui, c.and_then(|c| c.hotkey), able);

        if let (true, Some(o)) = ((response.clicked() || key) && castable, offer) {
            match o.aim {
                Aim::Now => orders.orders.outbox.push(cast_now(local, o.row.spell)),
                _ => orders.orders.mode = Mode::Spell(*o),
            }
        }
    }

    if let Some(((ms, c), over)) = rest {
        let c = ShownControl {
            rect: Some(smnu_rect(over)),
            ..c.clone()
        };

        super::upper::rest_bar(canvas, &c, ms);
    }
}

/// Whether `b` is one of the Abode's six.
fn abode(b: &Button) -> bool {
    b.record == ABODE_ICONS
}

/// The rest bar's place: across the Abode's buttons' cells in `grid`,
/// as `AP92`'s 9500 spans them (`spell_bar::RESTING_BAR`). None without
/// them (the base game).
fn rest_rect(grid: &Grid, buttons: &[Button]) -> Option<Rect> {
    buttons
        .iter()
        .enumerate()
        .filter(|(_, b)| abode(b))
        .filter_map(|(i, _)| grid.cell(i))
        .reduce(Rect::union)
}

/// HD panel rectangle `r` as a layout's, whole pixels.
#[allow(clippy::cast_possible_truncation, reason = "HD pixels")]
fn smnu_rect(r: Rect) -> smnu::Rect {
    smnu::Rect {
        x: r.min.x.round() as i32,
        y: r.min.y.round() as i32,
        width: r.width().round() as i32,
        height: r.height().round() as i32,
    }
}

/// How a spell's button draws: lit while the crown has its building at
/// the spell's level, the Abode's resting
/// too, though covered by its rest bar.
const fn lit(offer: Option<&SpellOffer>) -> Able {
    Able::of(offer.is_some())
}

/// A spell icon's slot and tone: a full set's slot as the engine draws it
/// (`menu::drawn_slot`); a one-slot set, the Abode's full-size icons, its
/// slot 0 toned to look the same; held, as hovered.
fn look(slots: usize, able: Able, (hover, press): (Hover, Press)) -> (usize, Tone) {
    let state = match able {
        Able::Enabled => State::Normal,
        Able::Disabled => UNCASTABLE,
    };

    match (slots > UNCASTABLE.index(), able, hover) {
        (true, ..) => {
            let slot = drawn_slot((Tick::Clear, hover), state.pressed(press), slots);

            (slot, Tone::Plain)
        }
        (false, Able::Disabled, _) => (0, Tone::Unlit),
        (false, Able::Enabled, Hover::Over) => (0, Tone::Lit),
        (false, Able::Enabled, Hover::Away) => (0, Tone::Plain),
    }
}

/// `pic` over panel rectangle `rect`, its edges kept: its ends where the
/// rectangle's are, its middle half repeated between, across and down.
fn framed(canvas: &mut Canvas<'_>, pic: Pic, rect: Rect) {
    if let Some(size) = canvas.size(pic) {
        for (x, w, from_x) in spans(rect.width(), size.x) {
            for (y, h, from_y) in spans(rect.height(), size.y) {
                let clip = Rect::from_min_size(rect.min + vec2(x, y), vec2(w, h));
                let at = clip.min - vec2(from_x, from_y);

                canvas.clipped(pic, Tone::Plain, at, clip);
            }
        }
    }
}

/// A length `length` cut from a picture `side` long: its first half at
/// the start, its last at the end, its middle half repeated between. Each
/// piece's start, length and start in the picture.
fn spans(length: f32, side: f32) -> Vec<(f32, f32, f32)> {
    let half = (side / 2.0).min(length / 2.0);
    let (middle, step) = (side / 4.0, side / 2.0);
    let mut out = vec![(0.0, half, 0.0)];
    let mut at = half;

    while step > 0.0 && at < length - half {
        let piece = step.min(length - half - at);

        out.push((at, piece, middle));
        at += piece;
    }

    out.push((length - half, half, side - half));
    out
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "whole numbers, exact in f32")]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(w, h))
    }

    #[test]
    fn apmk_pieces_fall_in_their_places() {
        assert_eq!(place(rect(4.0, 3.0, 176.0, 176.0)), Place::Column);
        assert_eq!(place(rect(206.0, 0.0, 104.0, 26.0)), Place::Top);
        assert_eq!(place(rect(1834.0, 1165.0, 82.0, 35.0)), Place::Bottom);
        assert_eq!(place(rect(1916.0, 28.0, 4.0, 601.0)), Place::Elsewhere);
        assert_eq!(place(rect(6.0, 852.0, 190.0, 126.0)), Place::Elsewhere);
    }

    /// The title scroll, `INse` 1007 at (844, 1) in the 1920-wide
    /// `UIData`, stands where `UIData_1680_1050` puts it, x 723, at that
    /// width: about the top bar's centre, not the screen's.
    #[test]
    fn the_title_scroll_keeps_to_the_top_bars_centre() {
        let frame = Frame::of(vec2(1680.0, 848.0));
        let scroll = bar_rect(&frame, rect(844.0, 1.0, 395.0, 26.0), (0.0, frame.top));

        assert!((scroll.left() - 723.0).abs() <= 1.0, "{}", scroll.left());
    }

    /// `APMK` 2008, the scroll's press, is `INTI` 1016 over `INse` 1007:
    /// the empty picture, all shadow, would veil the scroll half black.
    /// **Game**: `maj_mayhew.png`'s scroll is its picture's own colours.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn nothing_veils_the_title_scroll() {
        let install = crate::files::env::Install::from_env().expect("MAJESTY_HD");
        let layouts = super::super::uidata(&install.hd);
        let main = layouts.get(&super::super::MAIN).expect("APMK");
        let top: Vec<_> = pieces(main, Place::Top).collect();
        let scroll = Pic::of(FourCc(*b"INse"), 1007);
        let at = top.iter().position(|(p, _)| *p == scroll).expect("scroll");
        let over: Vec<_> = top[at + 1..]
            .iter()
            .filter(|(_, r)| r.intersects(top[at].1))
            .collect();

        assert!(over.is_empty(), "{over:?}");
    }

    #[test]
    fn a_bar_piece_keeps_to_its_edge_or_stretches() {
        let frame = Frame::of(vec2(2000.0, 848.0));
        let top = (0.0, frame.top);

        // The gold box stays at the column's edge, the day box at the
        // right, the long rule spans what lies between.
        assert_eq!(
            bar_rect(&frame, rect(206.0, 0.0, 104.0, 26.0), top).left(),
            206.0
        );
        assert_eq!(
            bar_rect(&frame, rect(1807.0, 0.0, 112.0, 26.0), top).right(),
            1999.0
        );
        assert_eq!(
            bar_rect(&frame, rect(205.0, 25.0, 1690.0, 4.0), top).width(),
            1690.0 + 80.0
        );
    }

    #[test]
    fn a_frame_picture_keeps_its_ends_and_repeats_its_middle() {
        // AP92's 590 wide over 1000: ends 295 each, 410 of middle.
        let wide = spans(1000.0, 590.0);

        assert_eq!(wide.first(), Some(&(0.0, 295.0, 0.0)));
        assert_eq!(wide.last(), Some(&(705.0, 295.0, 295.0)));
        assert!(wide[1..wide.len() - 1].iter().all(|p| p.2 == 147.5));
        assert_eq!(wide.iter().map(|p| p.1).sum::<f32>(), 1000.0);

        // Narrower than the picture: its two ends, cut.
        assert_eq!(spans(20.0, 34.0), [(0.0, 10.0, 0.0), (10.0, 10.0, 24.0)]);
    }

    /// A resting Abode's spell stays lit (spell button state
    /// greys by building level only), under its rest bar.
    #[test]
    fn a_resting_abode_spell_is_not_greyed() {
        let row = *SPELLS
            .iter()
            .find(|r| r.building == regent_core::sim::world::spell::cast::ABODE)
            .expect("an Abode spell");
        let resting = SpellOffer {
            row,
            price: 0,
            aim: Aim::Now,
            ready: false,
        };

        assert_eq!(lit(Some(&resting)), Able::Enabled);
        assert_eq!(lit(None), Able::Disabled);
    }

    /// The rest bar spans the Abode's six cells, one row, and is
    /// absent from the base game's bar.
    #[test]
    fn the_rest_bar_spans_the_abodes_six_cells() {
        use regent_core::view::menu::Release;
        use regent_core::view::panel::spell_bar::shown;

        let wide = |buttons: &[Button]| {
            let groups = groups(buttons, None);

            crate::ui::layout::spell_grid(&groups, 2000.0, 1.0)
        };
        let all = shown(Release::Expansion);
        let grid = wide(all);
        let over = rest_rect(&grid, all).expect("the Abode's cells");
        let first = grid.cell(all.len() - 6).expect("the first Abode cell");
        let last = grid.cell(all.len() - 1).expect("the last Abode cell");

        assert_eq!(over, first.union(last));
        assert_eq!(over.height(), first.height());
        assert_eq!(over.width(), 6.0 * first.width());

        let base = shown(Release::Base);

        assert_eq!(rest_rect(&wide(base), base), None);
    }

    /// The Abode's one-slot icons tone as a full set's slots draw.
    #[test]
    fn a_one_slot_icon_tones_as_the_slots_it_lacks() {
        let (away, over) = ((Hover::Away, Press::Free), (Hover::Over, Press::Free));
        let held = (Hover::Over, Press::Held);

        assert_eq!(look(1, Able::Enabled, away), (0, Tone::Plain));
        assert_eq!(look(1, Able::Enabled, over), (0, Tone::Lit));
        assert_eq!(look(1, Able::Enabled, held), (0, Tone::Lit));
        assert_eq!(look(1, Able::Disabled, over), (0, Tone::Unlit));
        assert_eq!(look(12, Able::Disabled, away), (2, Tone::Plain));
        assert_eq!(look(12, Able::Disabled, over), (8, Tone::Plain));
        assert_eq!(look(12, Able::Disabled, held), (8, Tone::Plain));
        assert_eq!(look(12, Able::Enabled, over), (6, Tone::Plain));
        assert_eq!(look(12, Able::Enabled, held), (7, Tone::Plain));
    }
}
