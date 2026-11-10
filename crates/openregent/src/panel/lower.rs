//! The page under the selection, in the column's slot at (0, 428): what
//! HD shows there at its larger sizes, one panel at a time.
//!
//! ```text
//!  Help      AP95  the kind's HelpID text            any piece
//!            AP94                                    (a panel with 8028)
//!  Build     AP03  the build menu, by name           the palace
//!  Rewards   AP41  attack and explore flags          the palace
//!  Members   APd1  the guild's heroes                a guild
//!  Visitors  AP91  who is inside                     a building
//!            AP09                                    (the Fairgrounds)
//!  Research  APa3  the building's research           a researcher
//!  Crypt     MX05  the Mausoleum's dead, resurrect   the Mausoleum
//!  Graves    AP16  the heroes buried there           an own graveyard
//!  Bounty    MX01  the monster types slain           the Hall of Champions
//!  Casting   MX07  its six spells, cast as the bar's  the Sorcerer's Abode
//!            AP66–AP72  its spells                   a temple, wizards' guild
//!  Seekers   AP87  the heroes going for it           an own flag
//!            AP86                                    (explore)
//!  Stats Items Spells   AP21 AP22 AP78               a hero
//!  Statistics, Heroes, Buildings, Henchmen: both slots, panel::palace
//! ```

use regent_core::sim::world::kind;
use std::sync::Arc;

use bevy_egui::egui::{Color32, Pos2, Rect, pos2, vec2};
use regent_core::content::unit::Unit;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::INTERFACE_ICON;
use regent_core::data::text::{Text, printf};
use regent_core::sim::game::Input;
use regent_core::sim::game::orders::cast_now;
use regent_core::sim::world::{
    World,
    flag::Rewards,
    guild::{bounty::Bounty, mausoleum::Resurrect, taxman::Step},
    offer::{self, Aim, Button, Offer, UNAVAILABLE_MARK, Unavailable},
    players::INVENTORY,
    research::Research,
};
use regent_core::view::menu::{Layout, ShownControl, ids};
use regent_core::view::panel::icons;
use regent_core::view::panel::portrait;
use regent_core::view::panel::portrait::details::{self, Details, Hero};
use regent_core::view::panel::research as research_page;
use regent_core::view::panel::spell_bar;
use regent_core::view::panel::tip;

use super::canvas::{Align, Canvas, WHITE};
use super::ink::{Pic, Tone};
use super::sheet::{self, Able, Lit, Purse};
use super::upper::{self, Chosen, Game, SHORT};
use super::{Ask, Page, Panels, palace};
use crate::input::Press;
use crate::input::orders::{self, FlagKind, Mode};
use crate::ui::scroll;

/// Each page's panel.
const HELP: FourCc = portrait::HELP;
const BUILD: FourCc = FourCc(*b"AP03");
const REWARDS: FourCc = FourCc(*b"AP41");
const MEMBERS: FourCc = FourCc(*b"APd1");
const CRYPT: FourCc = FourCc(*b"MX05");
const VISITORS: FourCc = FourCc(*b"AP91");
const ATTACK_SEEKERS: FourCc = FourCc(*b"AP87");
const EXPLORE_SEEKERS: FourCc = FourCc(*b"AP86");
const RESEARCH: FourCc = FourCc(*b"APa3");
const BOUNTY: FourCc = FourCc(*b"MX01");
const STATS: FourCc = FourCc(*b"AP21");
const ITEMS: FourCc = FourCc(*b"AP22");
const SPELLS: FourCc = FourCc(*b"AP78");
const QUESTION: FourCc = FourCc(*b"AP62");

/// The research panel of a building whose panel is `panel`.
fn research_panel(panel: Option<FourCc>) -> FourCc {
    panel.and_then(research_page::page).unwrap_or(RESEARCH)
}

/// The Fairgrounds' own visitors panel (`FairgroundController`,
/// control 8021): `AP91`'s list on other art.
const FAIR_VISITORS: ([u8; 4], [u8; 4]) = (*b"AP08", *b"AP09");

/// The visitors panel of a building whose panel is `panel`.
fn visitors_panel(panel: Option<FourCc>) -> FourCc {
    match panel {
        Some(p) if p.0 == FAIR_VISITORS.0 => FourCc(FAIR_VISITORS.1),
        _ => VISITORS,
    }
}

/// The controls of research page `layout` its rows draw, not its
/// statics: `MX03`'s icons are no row's, so show as the panel's.
fn row_controls(layout: &Layout) -> Vec<i32> {
    research_page::rows(layout)
        .iter()
        .flat_map(research_page::Row::controls)
        .collect()
}

/// The panels' list and text controls.
mod id {
    pub const TEXT: i32 = 5000;
    pub const HELP_SCROLL: i32 = 5010;
    pub const BACK: [i32; 2] = [TOPIC_BACK, 8013];
    pub const TOPIC_BACK: i32 = 8004;
    pub const BUILD_LIST: i32 = 5003;
    pub const BUILD_BUTTON: i32 = 5000;
    pub const BUILD_PRICE: i32 = 5002;
    pub const BUILD_SCROLL: i32 = 5001;
    pub const BUILD_TOGGLES: [i32; 2] = [5404, 5405];
    pub const BUILD_TOPIC: i32 = 8029;
    pub const LIST: i32 = 5000;
    pub const LIST_SCROLL: i32 = 5010;
    pub const LIST_TITLE: i32 = 7560;
    pub const SEEKERS_TITLE: i32 = 1088;
    pub const SELECT: i32 = 5003;
    pub const ZOOM_HERO: i32 = 5001;
    pub const DETAIL: i32 = 5004;
    pub const RESURRECT: i32 = 5003;
    pub const RESURRECT_PRICE: i32 = 8006;
    pub const ATTACK: i32 = 5002;
    pub const EXPLORE: i32 = 5003;
    pub const ATTACK_AMOUNT: i32 = 8;
    pub const EXPLORE_AMOUNT: i32 = 5004;
    pub const ATTACK_DOWN: i32 = 10;
    pub const ATTACK_UP: i32 = 11;
    pub const EXPLORE_DOWN: i32 = 5001;
    pub const EXPLORE_UP: i32 = 5000;
    pub const XP_BAR: i32 = 4016;
    pub const XP_TEXT: i32 = 4015;
    pub const TABS: [(i32, super::Page); 3] = [
        (4004, super::Page::Spells),
        (4002, super::Page::Items),
        (4006, super::Page::Stats),
    ];
    pub const WEAPON_NAME: i32 = 9003;
    pub const ARMOUR_NAME: i32 = 9004;
    pub const WEAPON: i32 = 7004;
    pub const ARMOUR: i32 = 7005;
    pub const GOLD: i32 = 9000;
    pub const TAXED: i32 = 9001;
    pub const POTIONS: i32 = 9002;
    pub const POISONED: i32 = 7006;
    pub const ITEM_LIST: i32 = 9005;
    pub const ITEM_SCROLL: i32 = 7007;
    pub const SPELL_LIST: i32 = 8730;
    pub const ENCHANTMENTS: i32 = 8731;
    pub const SPELL_SCROLLS: [i32; 2] = [7007, 4865];
    pub const QUESTION_TEXT: i32 = 5000;
    pub const YES: i32 = 5501;
    pub const NO: i32 = 5001;
}

/// `Are you sure you want to `, `demolish this building?`, `dismiss this
/// character?`, `remove this flag?…`: `GMTX` 20 to 23.
const ASK_TEXT: usize = 20;
const DEMOLISH_TEXT: usize = 21;
const DISMISS_TEXT: usize = 22;
const UNFLAG_TEXT: usize = 23;

/// A list row's text, `GMTX` 220 `%s is %s (%d of %d hp)` and 218
/// `%s (%d of %d hp)`; a price `GMTX` 2 `%d gold`.
const DOING_ROW: usize = 220;
const PLAIN_ROW: usize = 218;
const PRICE_ROW: usize = 2;

/// `%d Heroes Attempting`: `GMTX` 182.
const SEEKERS_TEXT: usize = 182;

/// `Visitors`: `GMTX` 229.
const VISITORS_TEXT: usize = 229;

/// A build or spell row's height, pixels: its 25 × 25 icon and a margin.
const ROW: f32 = 30.0;

/// Two enchantments fit in `AP78`'s 52-pixel list.
const ENCHANT_ROW: f32 = 26.0;

/// A unit list's row height with detail, pixels, 32 when its first row
/// is a henchman's; a graveyard's 40
/// too.
pub(super) const UNIT_ROW: f32 = 40.0;
pub(super) const HENCHMAN_ROW: f32 = 32.0;

/// A text row's height, pixels: an item's or spell's.
const LINE: f32 = 27.0;

/// The rows' font.
pub(super) const ROW_FONT: FourCc = FourCc(*b"fnt7");

/// The lit row's backdrop (`ui_warriors_guild.png`).
const LIT_ROW: Color32 = Color32::from_rgb(0, 40, 60);

/// The lists' level numbers' font.
const LEVEL_FONT: FourCc = FourCc(*b"fn10");

/// The build list's rows, by game frame and toggles.
pub(super) type Built = crate::ui::Memo<(u32, Toggles), Arc<[Row]>>;

/// One list row.
pub(super) struct Row {
    pub(super) icon: Option<Pic>,
    pub(super) lead: Option<String>,
    pub(super) text: String,
    pub(super) tip: Option<String>,
    pub(super) able: Able,
}

/// What a page asks of the interface.
#[derive(Default)]
pub struct Asked {
    pub select: Option<u32>,
    pub zoom: Option<(i32, i32)>,
    /// The tracking window to follow this agent (8001).
    pub track: Option<u32>,
}

/// The page under `chosen`, none with nothing chosen, its presses sent
/// through `orders`.
pub fn show(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    game: &Game<'_>,
    chosen: Option<&Chosen>,
    orders: &mut orders::Panel,
) -> Asked {
    let text = &game.corpus.text;
    let catalogue = &game.corpus.catalogue;
    let mut asked = Asked::default();

    // The build list is the palace's; leaving it ends a placement.
    let page = shown(panels.state.page, Holder::of(chosen));
    orders.orders.mode = after_page(panels.state.shown, page, orders.orders.mode);
    panels.state.shown = page;

    let page_of = chosen.map(|c| c.agent);

    if panels.state.page_of != page_of {
        panels.state.enchant_scroll = 0.0;
        panels.state.page_of = page_of;
    }

    let Some(page) = page else {
        return asked;
    };
    // The selection panel's "?" names its help page.
    let selection = chosen
        .and_then(|c| c.panel)
        .and_then(|p| panels.layout(catalogue, text, p));
    let id = match page {
        Page::Help => selection.map_or(HELP, |l| portrait::help_page(&l)),
        Page::Topic(_) => HELP,
        Page::Build => BUILD,
        Page::Rewards => REWARDS,
        Page::Members => MEMBERS,
        Page::Crypt => CRYPT,
        Page::Graves => details::GRAVES_PANEL,
        Page::Visitors => visitors_panel(chosen.and_then(|c| c.panel)),
        Page::Seekers => match chosen.map(|c| c.kind) {
            Some(upper::Kind::Flag(FlagKind::Explore)) => EXPLORE_SEEKERS,
            _ => ATTACK_SEEKERS,
        },
        Page::Research => research_panel(chosen.and_then(|c| c.panel)),
        Page::Bounty => BOUNTY,
        Page::Casting => chosen
            .and_then(|c| spell_bar::spell_page(c.code))
            .unwrap_or(spell_bar::ABODE_PAGE),
        Page::Stats => STATS,
        Page::Items => ITEMS,
        Page::Spells => SPELLS,
        Page::Statistics => palace::STATISTICS,
        Page::Heroes => palace::HEROES,
        Page::Buildings => palace::BUILDINGS,
        Page::Henchmen => palace::HENCHMEN,
    };
    // A whole-column page is `UIData`'s, 490 high.
    let layout = if page.whole() {
        panels.sized((catalogue, text), id, game.hd)
    } else {
        panels.layout(catalogue, text, id)
    };
    let Some(layout) = layout else {
        return asked;
    };
    let hero = chosen.and_then(|c| match &c.details {
        Some(Details::Hero(h)) => Some(h),
        _ => None,
    });

    match (page, chosen, hero) {
        (Page::Stats | Page::Items | Page::Spells, Some(c), Some(h)) => {
            heroic(canvas, panels, &layout, (c, h, page), game);
        }
        (Page::Members | Page::Visitors | Page::Seekers, Some(c), _) => {
            asked = roster(canvas, panels, &layout, (c, page), game);
        }
        (Page::Crypt, Some(c), _) => asked = crypt(canvas, panels, &layout, (c, game), orders),
        (Page::Graves, Some(c), _) => graves(canvas, panels, &layout, (c, game)),
        (Page::Research, Some(c), _) => research(canvas, panels, &layout, (game, c), orders),
        (Page::Bounty, Some(c), _) => bounty(canvas, panels, &layout, (game, c), orders),
        (Page::Casting, Some(c), _) => casting(canvas, &layout, (game, c), orders),
        (Page::Statistics, Some(c), _) if c.palace() => {
            palace::statistics(canvas, panels, &layout, game);
        }
        (Page::Heroes | Page::Buildings | Page::Henchmen, Some(c), _) if c.palace() => {
            asked = palace::roster(canvas, panels, &layout, (c, page), game);
        }
        (Page::Rewards, _, _) => rewards(canvas, &layout, game, orders),
        (Page::Build, _, _) => build(canvas, panels, &layout, game, orders),
        (Page::Topic(code), _, _) => help(canvas, panels, &layout, game, (chosen, Some(code))),
        (_, Some(c), _) => help(canvas, panels, &layout, game, (Some(c), None)),
        _ => {}
    }

    // Every page's zoom centres the view on the selection; its track
    // button has the tracking window follow it.
    if let Some(c) = layout.control(ids::ZOOM)
        && sheet::button(canvas, c, None, Able::of(chosen.is_some()))
    {
        asked.zoom = chosen.map(|c| c.at);
    }

    if let Some(c) = layout.control(ids::TRACK)
        && sheet::button(canvas, c, None, Able::of(chosen.is_some()))
    {
        asked.track = chosen.map(|c| c.agent);
    }

    asked
}

/// What holds the selection, as the page asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holder {
    Nothing,
    /// The crown's palace, whose panel alone opens the build list
    /// (`PalaceController`, 0x138a).
    Palace,
    Other,
}

impl Holder {
    fn of(chosen: Option<&Chosen>) -> Holder {
        match chosen {
            None => Holder::Nothing,
            Some(c) if c.palace() => Holder::Palace,
            Some(_) => Holder::Other,
        }
    }
}

/// The page drawn for `page` under `holder`: none with nothing
/// selected, as clearing the selection closes its panels;
/// the build list and its tips only under the
/// palace, another piece its tips.
const fn shown(page: Page, holder: Holder) -> Option<Page> {
    match (page, holder) {
        (_, Holder::Nothing) => None,
        (Page::Build | Page::Topic(_), Holder::Other) => Some(Page::Help),
        _ => Some(page),
    }
}

/// The mode once the page drawn goes from `was` to `now`: a closed
/// build list leaves any mode (building list dtor,
/// map mode reset), so a new selection or none drops a building
/// being placed. **Inferred**: another palace page, or the "?" tips,
/// closes it too.
fn after_page(was: Option<Page>, now: Option<Page>, mode: Mode) -> Mode {
    if was == Some(Page::Build) && now != was {
        Mode::Idle
    } else {
        mode
    }
}

/// How much of the column a page fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    /// The slot under the selection.
    One,
    /// Both slots, the selection's too: the palace's statistics and
    /// roster.
    Both,
}

/// Back buttons return to the kind's tips. A one-slot page has none:
/// the selection above it is always shown, so its back is redundant. A
/// departure (docs/openregent/screenshots.md); the original draws it, hot key Tab too. A
/// two-slot page's opens the palace's panel again, its first page.
pub(super) fn back(canvas: &mut Canvas<'_>, panels: &mut Panels, layout: &Layout, span: Span) {
    let shown = match span {
        Span::One => [].as_slice(),
        Span::Both => id::BACK.as_slice(),
    };

    for &control in shown {
        if let Some(c) = layout.control(control)
            && sheet::button(canvas, c, None, Able::Enabled)
        {
            panels.state.page = match span {
                Span::One => Page::Help,
                Span::Both => Page::Build,
            };
            panels.state.scroll = 0.0;
            panels.state.row = None;
        }
    }
}

/// A topic's back, 8004, to the build list: `HelpController` opens
/// `AP03` for a palace with a topic.
fn topic_back(canvas: &mut Canvas<'_>, panels: &mut Panels, layout: &Layout) {
    if let Some(c) = layout.control(id::TOPIC_BACK)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        panels.state.page = Page::Build;
        panels.state.scroll = 0.0;
        panels.state.row = None;
    }
}

/// The page's common pictures and name, skipping `own` and the backs.
pub(super) fn frame(canvas: &mut Canvas<'_>, layout: &Layout, name: Option<&str>, own: &[i32]) {
    let skip = |c: i32| {
        own.contains(&c)
            || id::BACK.contains(&c)
            || c == ids::TITLE
            || [ids::ZOOM, ids::TRACK].contains(&c)
    };

    sheet::statics(canvas, layout, None, &skip);

    if let Some(name) = name {
        sheet::fill(canvas, layout, ids::TITLE, name, (None, Align::Centre));
    } else if let Some(c) = layout.control(ids::TITLE)
        && let (Some(r), Some(t)) = (sheet::rect(c), c.text.as_deref())
    {
        sheet::label(canvas, c, r, t);
    }
}

/// The engine's unit kind for a panel's kind: a flag is a reward flag, a
/// chest or sign a building, as saves keep them in `BLDG`.
const fn engine_kind(kind: upper::Kind) -> kind::Kind {
    match kind {
        upper::Kind::Building | upper::Kind::Chest | upper::Kind::Sign => kind::Kind::BUILDING,
        upper::Kind::Flag(_) => kind::Kind::REWARD,
        upper::Kind::Hero | upper::Kind::Purse | upper::Kind::Monster | upper::Kind::Other => {
            kind::Kind::VEHICLE
        }
    }
}

/// The kind's tips: its `HelpID` text, scrolled; or building type
/// `topic`'s (`HelpController`'s code), under its own
/// name, and a back to the build list.
fn help(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    (chosen, topic): (Option<&Chosen>, Option<FourCc>),
) {
    let (text, catalogue) = (&game.corpus.text, &game.corpus.catalogue);
    let asked = topic
        .map(|code| (code, kind::Kind::BUILDING))
        .or_else(|| chosen.map(|c| (c.code, engine_kind(c.kind))));
    let unit = asked.and_then(|(code, kind)| Some((Unit::find(catalogue, code)?.ok()?, kind)));
    let help = unit
        .as_ref()
        .and_then(|(u, kind)| portrait::help(*kind, &u.desc))
        .and_then(|h| text.help(h))
        .map(|t| t.trim().to_owned());

    // A topic's own name, "TEMPLE TO KROLM". A departure: the original
    // keeps the selection's, "PALACE".
    let titled = unit
        .as_ref()
        .filter(|_| topic.is_some())
        .and_then(|(u, _)| portrait::title(catalogue, text, &u.desc, None).ok());
    let name = titled.as_deref().or(chosen.map(|c| c.name.as_str()));

    frame(canvas, layout, name, &[id::TEXT, id::HELP_SCROLL]);

    match topic {
        Some(_) => topic_back(canvas, panels, layout),
        None => back(canvas, panels, layout, Span::One),
    }

    if let Some(c) = layout.control(id::TEXT)
        && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
    {
        let body = help.or_else(|| c.text.clone()).unwrap_or_default();
        let tall = canvas.paragraphs(
            font,
            r,
            &body,
            (sheet::colour(c), Align::Left, panels.state.scroll),
        );

        scroll(
            canvas,
            panels,
            layout.control(id::HELP_SCROLL).and_then(sheet::rect),
            (r, tall),
        );
    }
}

/// The page's scroll moved by the wheel over `area` and the arrows of
/// scroll bar `bar`, kept within `tall` lines.
fn scroll(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    bar: Option<Rect>,
    (area, tall): (Rect, f32),
) {
    scrolled(canvas, &mut panels.state.scroll, bar, (area, tall));
}

/// A scroll bar's art: the up arrow, the panels' `INDj` 1000; the down
/// arrow the next set, the thumb the one after (`INDj` 1002, 25 × 10:
/// `CYDialogScrollbarItem` sizes the thumb by set + 2's picture;
/// `scroll::DOWN`, `scroll::THUMB`).
const SCROLL_ART: FourCc = FourCc(*b"INDj");
const UP_ARROW: i32 = 1000;

/// An arrow's and the thumb's height, pixels.
const ARROW_TALL: f32 = 17.0;
const THUMB_TALL: f32 = 10.0;

/// Scroll `by`, pixels, moved by the wheel over `area`, the arrows of
/// scroll bar `bar` and its thumb's drag, kept within `tall` lines. A
/// text that fits shows no arrows and no thumb (**Game**: `ui_palace.png`,
/// `ui_warriors_guild.png`).
pub fn scrolled(
    canvas: &mut Canvas<'_>,
    by: &mut f32,
    bar: Option<Rect>,
    (area, tall): (Rect, f32),
) {
    let most = scroll::most(tall, area.height());
    let step = canvas.pitch(ROW_FONT).max(1.0) * 2.0;
    let mut moved = canvas.wheel(area);

    if let Some(rect) = bar.filter(|_| most > 0.0) {
        let bar = scroll::Bar {
            rect,
            arrow: vec2(rect.width(), ARROW_TALL),
            thumb: vec2(rect.width(), THUMB_TALL),
        };

        let scale = canvas.fit.scale;
        let id = (rect.min.x.to_bits(), rect.min.y.to_bits());

        // Each arrow steps two lines.
        moved += scroll::pass(
            canvas,
            (&bar, scale),
            (*by, most, step),
            |c, r, past| c.press(r, ("arrow", id, UP_ARROW + past)),
            |c, at| c.drag(at, ("thumb", id)),
            |c, past, slot, at| {
                c.picture(
                    Pic::of(SCROLL_ART, UP_ARROW + past).slot(slot),
                    Tone::Plain,
                    at,
                );
            },
        );
    }

    *by = (*by + moved).clamp(0.0, most);
}

/// `rows` in list rectangle `area` from the page's scroll; the row
/// pressed, if any.
pub(super) fn list(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    (area, bar): (Rect, Option<Rect>),
    rows: &[Row],
    pitch: f32,
) -> Option<usize> {
    inset_list(canvas, panels, (area, bar), rows, (pitch, 0.0))
}

/// [`list`], each row's text kept `inset` short of its right: room for
/// marks drawn there.
pub(super) fn inset_list(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    (area, bar): (Rect, Option<Rect>),
    rows: &[Row],
    (pitch, inset): (f32, f32),
) -> Option<usize> {
    let mut pressed = None;
    let lit = panels.state.row;

    for (n, row) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
        let top = area.top() + n as f32 * pitch - panels.state.scroll;
        let r = Rect::from_min_size(pos2(area.left(), top), vec2(area.width(), pitch));

        if r.bottom() <= area.top() || r.top() >= area.bottom() {
            continue;
        }

        let shown = r.intersect(area);

        if lit == Some(n) {
            canvas.fill(shown, LIT_ROW);
        }

        let mut x = r.left() + 2.0;

        if let Some(icon) = row.icon {
            if let Some(at) = icon_at(pos2(x, r.top()), pitch, area) {
                let tone = if row.able == Able::Enabled {
                    Tone::Plain
                } else {
                    Tone::Dim
                };

                canvas.clipped(icon, tone, at, area);
            }

            x += 27.0;
        }

        if let Some(lead) = &row.lead {
            let cell = Rect::from_min_size(pos2(x, r.top()), vec2(14.0, pitch));

            if area.contains_rect(cell) {
                canvas.text(LEVEL_FONT, cell, lead, (WHITE, Align::Centre));
            }

            x += 16.0;
        }

        let words = Rect::from_min_max(pos2(x, r.top()), pos2(r.right() - inset, r.bottom()));
        let colour = if row.able == Able::Enabled {
            WHITE
        } else {
            WHITE.gamma_multiply(0.45)
        };
        row_text(canvas, area, words, &row.text, colour);

        let response = canvas.press(shown, ("row", n));

        Canvas::tip(&response, row.tip.as_deref());

        if response.clicked() {
            panels.state.row = Some(n);
            pressed = Some(n);
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
    let tall = rows.len() as f32 * pitch;

    scroll(canvas, panels, bar, (area, tall));
    pressed
}

/// A row's icon, [`ICON`] square, centred in a row `pitch` tall from
/// `corner`: where it goes, if any of it shows in `area`. A row partly
/// scrolled out draws cut to the list, as the original's.
fn icon_at(corner: Pos2, pitch: f32, area: Rect) -> Option<Pos2> {
    let at = pos2(corner.x, corner.y + (pitch - ICON) / 2.0);

    (at.y + ICON > area.top() && at.y < area.bottom()).then_some(at)
}

/// A list row's icon, pixels.
const ICON: f32 = 25.0;

/// A unit list's row height with detail: [`HENCHMAN_ROW`] when its first
/// unit is a henchman, else [`UNIT_ROW`]. The Mausoleum's list too
/// (**Inferred**: `MausoleumP2_Controller` is a `Base_UnitListController`).
fn unit_pitch(world: &World, ids: &[u32]) -> f32 {
    if ids.first().is_some_and(|&id| world.henchman(id)) {
        HENCHMAN_ROW
    } else {
        UNIT_ROW
    }
}

/// A row's `text` in its `words` rectangle, drawn only inside list
/// `area`: wrapped, as the palace's rosters' rows are
/// (`ui_palace_roster_buildings.png`, `ui_warriors_guild.png`), the lines
/// past the row's cut. **Inferred**: the original's cut is not seen; the
/// "…" is ours.
fn row_text(canvas: &mut Canvas<'_>, area: Rect, words: Rect, text: &str, colour: Color32) {
    canvas.fitted(ROW_FONT, (words, area), text, colour);
}

/// A guild's members, a building's visitors or a flag's seekers.
fn roster(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (chosen, page): (&Chosen, Page),
    game: &Game<'_>,
) -> Asked {
    let world = game.world;
    let text = &game.corpus.text;
    let mut asked = Asked::default();
    let ids: Vec<u32> = match page {
        Page::Members => world
            .roster(chosen.agent)
            .into_iter()
            .map(|(id, _)| id)
            .collect(),
        Page::Seekers => world.seekers(chosen.agent),
        _ => world
            .piece(chosen.agent)
            .and_then(|p| world.occupants(p.handle))
            .unwrap_or_default(),
    };
    // The guild's heroes against its spots; "VISITORS";
    // "2 Heroes Attempting".
    let title = match page {
        Page::Members => world
            .members(chosen.agent)
            .map(crate::panel::details::members),
        Page::Seekers => text
            .gmtx(SEEKERS_TEXT)
            .map(|t| printf(&t, &[u32::try_from(ids.len()).unwrap_or(u32::MAX).into()])),
        _ => text.gmtx(VISITORS_TEXT).map(|t| t.to_uppercase()),
    };

    // A flag's page keeps its own words, "REWARD ATTACK FLAG".
    let (name, title_at) = match page {
        Page::Seekers => (None, id::SEEKERS_TITLE),
        _ => (Some(chosen.name.as_str()), id::LIST_TITLE),
    };

    frame(
        canvas,
        layout,
        name,
        &[
            id::LIST,
            id::LIST_SCROLL,
            id::LIST_TITLE,
            id::SEEKERS_TITLE,
            id::SELECT,
            id::ZOOM_HERO,
            id::DETAIL,
        ],
    );
    back(canvas, panels, layout, Span::One);

    if let Some(title) = title {
        sheet::fill(canvas, layout, title_at, &title, (None, Align::Centre));
    }

    let rows: Vec<Row> = ids.iter().map(|&id| unit_row(game, id, text)).collect();
    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::LIST_SCROLL).and_then(sheet::rect);
    let pressed = list(canvas, panels, (area, bar), &rows, unit_pitch(world, &ids));

    // A row picked: the tracking window follows it (`AP86`, `AP87`;
    // **Inferred** for members and visitors, the same unit list).
    asked.track = pressed.and_then(|n| ids.get(n)).copied();

    let lit = panels.state.row.and_then(|n| ids.get(n)).copied();

    if let Some(c) = layout.control(id::SELECT)
        && sheet::button(canvas, c, None, Able::of(lit.is_some()))
    {
        asked.select = lit;
    }

    if let Some(c) = layout.control(id::ZOOM_HERO)
        && sheet::button(canvas, c, None, Able::of(lit.is_some()))
    {
        asked.zoom = lit
            .and_then(|id| game.now.iter().find(|s| s.agent == id))
            .map(|s| s.at);
    }

    asked
}

/// The Mausoleum's dead (`MX05`, `MausoleumP2_Controller`), rows as a
/// guild's members. A press lights a row; RESURRECT (5003, hot key R)
/// raises it, its price in 8006, then
/// selects the hero, as the base list's SELECT.
fn crypt(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (chosen, game): (&Chosen, &Game<'_>),
    orders: &mut orders::Panel,
) -> Asked {
    let text = &game.corpus.text;
    let mut asked = Asked::default();
    let ids = game.world.interred(chosen.agent);

    frame(
        canvas,
        layout,
        Some(&chosen.name),
        &[
            id::LIST,
            id::LIST_SCROLL,
            id::DETAIL,
            id::RESURRECT,
            id::RESURRECT_PRICE,
        ],
    );
    back(canvas, panels, layout, Span::One);

    let rows: Vec<Row> = ids.iter().map(|&id| unit_row(game, id, text)).collect();
    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::LIST_SCROLL).and_then(sheet::rect);

    list(
        canvas,
        panels,
        (area, bar),
        &rows,
        unit_pitch(game.world, &ids),
    );

    let lit = panels.state.row.and_then(|n| ids.get(n)).copied();
    let raise = raised(game.crypt, lit);

    // The price lies over the button's right end, so it draws after.
    if let Some(c) = layout.control(id::RESURRECT)
        && labelled(canvas, c, resurrect_able(raise))
        && let Some(b) = raise
    {
        orders.orders.outbox.push(resurrect(chosen.agent, b));
        asked.select = Some(b.what);
        panels.state.row = None;
    }

    if let Some(b) = raise {
        sheet::fill(
            canvas,
            layout,
            id::RESURRECT_PRICE,
            &b.price.to_string(),
            (Some(price_colour(b)), Align::Centre),
        );
    }

    asked
}

/// A graveyard's heroes (`AP16`, `GraveyardListController`), in
/// list order: each row its descriptor's icon and `GMTX` 221, name and
/// level. Not modelled: the list's detail switch (a byte's bit
/// 0), off giving the name alone in 16 high rows; rows show detail.
fn graves(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (chosen, game): (&Chosen, &Game<'_>),
) {
    let (catalogue, text) = (&game.corpus.catalogue, &game.corpus.text);
    let buried = game.world.buried(chosen.agent).unwrap_or_default();

    frame(
        canvas,
        layout,
        Some(&chosen.name),
        &[id::LIST, id::LIST_SCROLL],
    );
    back(canvas, panels, layout, Span::One);

    let rows: Vec<Row> = buried
        .iter()
        .map(|g| Row {
            icon: Some(Pic::of(g.code, i32::from(INTERFACE_ICON))),
            lead: None,
            text: details::grave_row(catalogue, text, g)
                .unwrap_or_else(|| details::grave_name(catalogue, text, g)),
            tip: None,
            able: Able::Enabled,
        })
        .collect();
    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::LIST_SCROLL).and_then(sheet::rect);

    list(canvas, panels, (area, bar), &rows, UNIT_ROW);
}

/// The lit hero's offer in `crypt`, if any: none lit, no price.
fn raised(crypt: &[Button<u32>], lit: Option<u32>) -> Option<Button<u32>> {
    crypt.iter().find(|b| Some(b.what) == lit).copied()
}

/// RESURRECT sends with a row lit whose price the treasury holds.
fn resurrect_able(raise: Option<Button<u32>>) -> Able {
    Able::of(raise.is_some_and(|b| b.ready))
}

/// A short treasury's price: `\x01FF0000` before `%d`.
const SHORT_PRICE: Color32 = Color32::from_rgb(0xFF, 0, 0);

const fn price_colour(b: Button<u32>) -> Color32 {
    if b.ready { WHITE } else { SHORT_PRICE }
}

/// `GUILD_ACTION` 0x15: the Mausoleum, the hero, its price.
const fn resurrect(mausoleum: u32, b: Button<u32>) -> Input {
    Input::Resurrect(Resurrect {
        mausoleum,
        hero: b.what,
        price: b.price,
    })
}

/// Control `c` as a button, its label in its text rectangle (key 42),
/// not the whole: RESURRECT's right end holds its price.
fn labelled(canvas: &mut Canvas<'_>, c: &ShownControl, able: Able) -> bool {
    let bare = ShownControl {
        text: None,
        ..c.clone()
    };
    let (pressed, state) = sheet::stated(canvas, &bare, None, able);
    #[allow(clippy::cast_precision_loss, reason = "panel pixels")]
    let words = sheet::rect(c).map(|r| {
        c.text_rect.map_or(r, |t| {
            Rect::from_min_size(
                r.min + vec2(t.x as f32, t.y as f32),
                vec2(t.width as f32, t.height as f32),
            )
        })
    });

    if let (Some(r), Some(t), Some(font)) = (words, c.text.as_deref(), c.font) {
        canvas.text(
            font,
            r,
            &sheet::marked(c, t.trim(), state),
            (sheet::ink(c, state), Align::Centre),
        );
    }

    pressed
}

/// A unit's list row: its icon, its level, "Name is doing (hp)".
pub(super) fn unit_row(game: &Game<'_>, id: u32, text: &Text) -> Row {
    let world = game.world;
    let shown = game.now.iter().find(|s| s.agent == id);
    let chosen = shown.map(|s| Chosen::of(game.corpus, (world, game.local), s));
    let (name, health) = chosen
        .as_ref()
        .map_or((String::new(), None), |c| (c.name.clone(), c.health));
    let hero = chosen.as_ref().and_then(|c| match &c.details {
        Some(Details::Hero(h)) => Some(h.clone()),
        _ => None,
    });
    let doing = hero
        .as_ref()
        .and_then(|h| h.activity)
        .and_then(|a| text.activity(a))
        .map(|s| s.trim().to_owned());
    let (now, most) = health.map_or((0, 0), |h| (h.now, h.max));
    let line = match doing {
        Some(doing) => text.gmtx(DOING_ROW).map(|f| {
            printf(
                &f,
                &[(&name).into(), (&doing).into(), now.into(), most.into()],
            )
        }),
        None => text
            .gmtx(PLAIN_ROW)
            .map(|f| printf(&f, &[(&name).into(), now.into(), most.into()])),
    };

    Row {
        icon: chosen
            .as_ref()
            .map(|c| Pic::of(c.code, i32::from(INTERFACE_ICON))),
        lead: hero.map(|h| h.level.to_string()),
        text: line.unwrap_or(name),
        tip: None,
        able: Able::Enabled,
    }
}

/// The bounty page's buttons: a short and a long bounty
/// (`HallOfChampionsP2_Controller`, `GUILD_ACTION` 0x14).
const BOUNTIES: [i32; 2] = [2050, 2051];

/// A bounty row's height with no detail.
const BOUNTY_ROW: f32 = 16.0;

/// The Hall of Champions' bounty page: the monster types the player has
/// slain, each row its title (**Inferred**:
/// descriptor is the `HEAD` title), the first lit until one is
/// picked (**Inferred**: `ui_hall_champions.png`). No detail rows. Each
/// term's button says its price (`HallOfChampions_Bounty_Cost`), enabled
/// while the treasury holds it; a press
/// sends the bounty on the lit row's type and opens the hall's panel
/// again.
fn bounty(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (game, chosen): (&Game<'_>, &Chosen),
    orders: &mut orders::Panel,
) {
    let catalogue = &game.corpus.catalogue;

    frame(
        canvas,
        layout,
        Some(&chosen.name),
        &[
            id::LIST,
            id::LIST_SCROLL,
            id::DETAIL,
            BOUNTIES[0],
            BOUNTIES[1],
        ],
    );
    back(canvas, panels, layout, Span::One);

    let slain = game.world.slain(game.local);
    let rows: Vec<Row> = slain
        .iter()
        .map(|(code, _)| Row {
            icon: None,
            lead: None,
            text: Unit::find(catalogue, *code)
                .and_then(Result::ok)
                .map_or_else(|| code.to_string(), |u| u.desc.head.title().into_owned()),
            tip: None,
            able: Able::Enabled,
        })
        .collect();
    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::LIST_SCROLL).and_then(sheet::rect);

    if panels.state.row.is_none() && !rows.is_empty() {
        panels.state.row = Some(0);
    }

    list(canvas, panels, (area, bar), &rows, BOUNTY_ROW);

    let lit = panels.state.row.and_then(|n| slain.get(n)).map(|(c, _)| *c);

    for (offer, control) in game.bounty.into_iter().zip(BOUNTIES) {
        let ready = offer.is_some_and(|b| b.ready) && lit.is_some();
        let label = offer.map(|b| b.price.to_string());

        if let Some(c) = layout.control(control)
            && sheet::button(
                canvas,
                c,
                Some(label.as_deref().unwrap_or("")),
                Able::of(ready),
            )
            && let (Some(code), Some(b)) = (lit, offer)
        {
            orders.orders.outbox.push(Input::Bounty(Bounty {
                hall: chosen.agent,
                code,
                term: b.what,
            }));
            panels.state.page = Page::Help;
            panels.state.row = None;
        }
    }
}

/// A researcher's research: each row a button with its price.
fn research(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (game, chosen): (&Game<'_>, &Chosen),
    orders: &mut orders::Panel,
) {
    let world = game.world;
    let buttons = world.researches(game.program, chosen.agent);
    let rows = research_page::rows(layout);
    let owned = row_controls(layout);
    let skip = |c: i32| {
        owned.contains(&c)
            || [ids::ZOOM, ids::TRACK, ids::TITLE].contains(&c)
            || id::BACK.contains(&c)
    };

    sheet::statics(canvas, layout, None, &skip);
    sheet::fill(
        canvas,
        layout,
        ids::TITLE,
        &chosen.name,
        (None, Align::Centre),
    );
    back(canvas, panels, layout, Span::One);

    for b in &buttons {
        // A row not shown hides its button, price and icon.
        let Some((row, price)) = rows
            .iter()
            .find(|r| r.id() == b.what.id)
            .and_then(|r| Some((r, r.price.and_then(sheet::rect)?)))
        else {
            continue;
        };
        let button = row.button;

        if let Some(icon) = row.icon
            && let (Some(r), Some((record, set))) = (sheet::rect(icon), icon.art)
        {
            canvas.picture(Pic::of(record, set), Tone::Plain, r.min);
        }

        if research_row(canvas, button, Able::of(b.ready)) {
            orders.orders.outbox.push(Input::Research(Research {
                building: chosen.agent,
                item: b.what.id,
                frame: 0,
            }));
        }

        let short = Purse::of(!world.affords(game.local, b.price));
        let colour = row.price.map_or(SHORT, |c| sheet::price_in(c, short));

        // A row that cannot be pressed shows a star for its price
        // (research button: the box's picture `INBb` 1015, no text).
        if b.ready {
            canvas.fill(price, Color32::from_black_alpha(200));

            canvas.text(
                ROW_FONT,
                price,
                &b.price.to_string(),
                (colour, Align::Centre),
            );
        } else {
            canvas.picture(RESEARCH_STAR, Tone::Plain, price.min);
        }
    }
}

/// The Abode's spell page, `MX07` (`SpellsController`): each button lit
/// where the crown may cast it, its icon and price box with it, the price
/// always written. A press aims it as the
/// spell bar's does, the Abode at rest too (`SpellsController`
/// asks no timer, as `FastBar` does).
fn casting(
    canvas: &mut Canvas<'_>,
    layout: &Layout,
    (game, chosen): (&Game<'_>, &Chosen),
    orders: &mut orders::Panel,
) {
    let world = game.world;
    let offers = world.spells(game.program, game.local);
    let buttons: Vec<_> = spell_bar::BUTTONS
        .iter()
        .filter(|b| layout.control(b.control).is_some())
        .collect();
    let owned: Vec<i32> = buttons
        .iter()
        .flat_map(|b| {
            [
                b.control,
                spell_bar::page_price(b.control),
                spell_bar::page_icon(b.control),
            ]
        })
        .collect();
    let skip = |c: i32| {
        owned.contains(&c)
            || [ids::ZOOM, ids::TRACK, ids::TITLE].contains(&c)
            || id::BACK.contains(&c)
    };

    sheet::statics(canvas, layout, None, &skip);
    sheet::fill(
        canvas,
        layout,
        ids::TITLE,
        &chosen.name,
        (None, Align::Centre),
    );

    for b in buttons {
        let offer = offers.iter().find(|o| o.row.spell == b.spell);
        let able = Able::of(offer.is_some());
        let tone = match able {
            Able::Enabled => Tone::Plain,
            Able::Disabled => Tone::Unlit,
        };

        if let Some(icon) = layout.control(spell_bar::page_icon(b.control))
            && let (Some(r), Some((record, set))) = (sheet::rect(icon), icon.art)
        {
            canvas.picture(Pic::of(record, set), tone, r.min);
        }

        let pressed = layout
            .control(b.control)
            .is_some_and(|c| sheet::stated(canvas, c, None, able).0);

        match offer.filter(|_| pressed) {
            Some(o) if o.aim == Aim::Now => {
                orders.orders.outbox.push(cast_now(game.local, o.row.spell));
            }
            Some(o) => orders.orders.mode = Mode::Spell(*o),
            None => {}
        }

        let price = offer::SPELLS
            .iter()
            .find(|r| r.spell == b.spell)
            .map(|r| r.price(game.program));

        if let (Some(c), Some(price)) = (layout.control(spell_bar::page_price(b.control)), price)
            && let Some(r) = sheet::rect(c)
        {
            let colour = sheet::ink(c, able.state(c.flags));
            let font = c.font.unwrap_or(ROW_FONT);

            // On the box's dark, as a research's price.
            canvas.fill(r, Color32::from_black_alpha(200));
            canvas.text(font, r, &price.to_string(), (colour, Align::Centre));
        }
    }
}

/// A research's price box while it cannot be pressed: a star.
const RESEARCH_STAR: Pic = Pic::of(FourCc(*b"INBb"), 1015);

/// Research button `c`: its picture, its panel label ("LVL 2 Weapons")
/// in its own font with the hot key letter marked, set in its text
/// rectangle (key 42) left of the price box; whether it was pressed, by
/// click or hot key. Research button gives the button no text,
/// so every row shows its panel's label, as the original does.
fn research_row(canvas: &mut Canvas<'_>, c: &ShownControl, able: Able) -> bool {
    let bare = ShownControl {
        text: None,
        ..c.clone()
    };
    let (pressed, state) = sheet::stated(canvas, &bare, None, able);
    #[allow(clippy::cast_precision_loss, reason = "panel pixels")]
    let words = sheet::rect(c).map(|r| {
        c.text_rect.map_or(r, |t| {
            Rect::from_min_size(
                r.min + vec2(t.x as f32, t.y as f32),
                vec2(t.width as f32, t.height as f32),
            )
        })
    });

    // "Teleportation\nAmulets" breaks where the panel does; a row being
    // researched, disabled, is red.
    if let (Some(r), Some(t), Some(font)) = (words, c.text.as_deref(), c.font) {
        let markup = sheet::marked(c, t.trim(), state);

        sheet::wrapped(canvas, font, r, &markup, sheet::ink(c, state));
    }

    pressed
}

/// The palace's rewards: each flag's setting, ± `#RewardDelta` within
/// the treasury, and its button (`Reward1Controller`).
fn rewards(canvas: &mut Canvas<'_>, layout: &Layout, game: &Game<'_>, orders: &mut orders::Panel) {
    let (world, local) = (game.world, game.local);
    let o = &mut orders.orders;

    if let Some(g) = world.treasury(local) {
        o.rewards = o.rewards.clamped(g, o.initial());
    }

    let rows = [
        (
            FlagKind::Attack,
            id::ATTACK,
            id::ATTACK_AMOUNT,
            (id::ATTACK_DOWN, id::ATTACK_UP),
        ),
        (
            FlagKind::Explore,
            id::EXPLORE,
            id::EXPLORE_AMOUNT,
            (id::EXPLORE_DOWN, id::EXPLORE_UP),
        ),
    ];
    let skip = |c: i32| {
        rows.iter()
            .any(|(_, b, a, (d, u))| [*b, *a, *d, *u].contains(&c))
    };

    sheet::statics(canvas, layout, None, &skip);

    // Shift goes `REWARD_STRIDE` steps, or as many as fit.
    let stride = canvas.stride();

    for (kind, button, amount, (down, up)) in rows {
        let setting = match kind {
            FlagKind::Attack => &mut o.rewards.attack,
            FlagKind::Explore => &mut o.rewards.explore,
        };
        let short = !world.affords(local, *setting);
        let colour = sheet::price_ink(layout, amount, Purse::of(short));

        sheet::fill(
            canvas,
            layout,
            amount,
            &setting.to_string(),
            (colour, Align::Centre),
        );

        let lower = layout
            .control(down)
            .map(|c| sheet::hinted(c, Step::Lower, Rewards::far()));

        if let Some(c) = &lower
            && sheet::button(canvas, c, None, Able::of(Rewards::lowers(*setting)))
        {
            *setting = Rewards::lowered(*setting, stride);
        }

        let raise = world.raises_reward(local, *setting);

        let raiser = layout
            .control(up)
            .map(|c| sheet::hinted(c, Step::Raise, Rewards::far()));

        if let Some(c) = &raiser
            && sheet::button(canvas, c, None, Able::of(raise))
        {
            *setting = world.raised_reward(local, *setting, stride);
        }

        if let Some(c) = layout.control(button)
            && sheet::button(canvas, c, None, Able::Enabled)
        {
            o.mode = Mode::Flag(kind);
        }
    }
}

/// The palace's build list: what is offered, by name, with the player's
/// toggles, icons (5404) and the unavailable (5405) listed red; a press
/// lights a row and its price, a double click or BUILD places it.
fn build(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    orders: &mut orders::Panel,
) {
    let text = &game.corpus.text;
    let toggles = panels.state.build;
    let seat = (game.world, game.local);
    let offers = orders.menu(game.corpus, seat, (game.frame, toggles.unavailable));
    let own = [
        id::BUILD_LIST,
        id::BUILD_BUTTON,
        id::BUILD_PRICE,
        id::BUILD_SCROLL,
        id::BUILD_TOGGLES[0],
        id::BUILD_TOGGLES[1],
        id::BUILD_TOPIC,
    ];

    frame(canvas, layout, None, &own);
    back(canvas, panels, layout, Span::One);

    // A row says the list's own tooltip. The rows are made again only on
    // a new game frame or toggle.
    let list_tip = layout
        .control(id::BUILD_LIST)
        .and_then(|c| c.tooltip.clone());
    let rows = panels
        .state
        .built
        .get((game.frame, toggles), || {
            offers
                .iter()
                .map(|(name, offer)| {
                    let price = text.gmtx(PRICE_ROW).map_or_else(
                        || offer.price.to_string(),
                        |f| printf(&f, &[offer.price.into()]),
                    );
                    let short = !game.world.affords(game.local, offer.price);
                    let price = if short {
                        format!("\u{1}E63C32{price}")
                    } else {
                        format!("{PLAIN_MARK}{price}")
                    };
                    let mark = if offer.offered { "" } else { UNAVAILABLE_MARK };

                    Row {
                        icon: (toggles.icons == Icons::Shown)
                            .then(|| Pic::of(offer.code, i32::from(INTERFACE_ICON))),
                        lead: None,
                        text: format!("{mark}{name}\n{price}"),
                        tip: list_tip.clone(),
                        able: Able::Enabled,
                    }
                })
                .collect()
        })
        .clone();
    let area = layout
        .control(id::BUILD_LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::BUILD_SCROLL).and_then(sheet::rect);

    if let Some(n) = list(canvas, panels, (area, bar), &rows, ROW)
        && let Some((_, offer)) = offers.get(n)
    {
        let (ms, at) = canvas.ui.ctx().input(|i| {
            let at = i.pointer.interact_pos().unwrap_or_default();

            (i.time * 1000.0, bevy::math::Vec2::new(at.x, at.y))
        });
        let press = panels.state.build_clicks.press(ms, at);

        orders.orders.mode = build_press(*offer, press);
    }

    let lit = panels.state.row.and_then(|n| offers.get(n));

    if let Some((_, offer)) = lit {
        let short = !game.world.affords(game.local, offer.price);
        let colour = sheet::price_ink(layout, id::BUILD_PRICE, Purse::of(short));

        sheet::fill(
            canvas,
            layout,
            id::BUILD_PRICE,
            &offer.price.to_string(),
            (colour, Align::Centre),
        );
    }

    // `BuildingListController`: disabled, `GMTX` 272.
    let able = Able::of(lit.is_some_and(|(_, o)| o.offered));
    let shown = layout.control(id::BUILD_BUTTON).map(|c| ShownControl {
        tooltip: match able {
            Able::Enabled => c.tooltip.clone(),
            Able::Disabled => tip::said(text, tip::line::NO_BUILDINGS),
        },
        ..c.clone()
    });

    if let Some(c) = shown
        && sheet::button(canvas, &c, None, able)
        && let Some((_, offer)) = lit
    {
        orders.orders.mode = Mode::Build(offer.code);
    }

    // "?": the lit row's tips, the first row's when none is lit
    // (`BuildingListController`, 8029: a selection of -1 reads 0).
    let asked = lit.or_else(|| offers.first());

    if let Some(c) = layout.control(id::BUILD_TOPIC)
        && sheet::button(canvas, c, None, Able::of(asked.is_some()))
        && let Some((_, offer)) = asked
    {
        panels.state.page = Page::Topic(offer.code);
        panels.state.scroll = 0.0;
    }

    toggle(canvas, panels, layout);
}

/// The mode a press on build list offer `offer` leaves:
/// a single press lights the
/// row and leaves any mode (`BuildingListController`'s list case,
/// map mode reset); a double click presses BUILD, which places an
/// offered building.
fn build_press(offer: Offer, press: Press) -> Mode {
    if press == Press::Double && offer.offered {
        Mode::Build(offer.code)
    } else {
        Mode::Idle
    }
}

/// The build list's toggles: icons (5404) and the unavailable (5405),
/// each a check lit while on. A changed list starts at its top.
fn toggle(canvas: &mut Canvas<'_>, panels: &mut Panels, layout: &Layout) {
    let toggles = &mut panels.state.build;
    let [icons, unavailable] = id::BUILD_TOGGLES.map(|c| layout.control(c));
    let lit = |on: bool| if on { Lit::On } else { Lit::Off };

    if let Some(c) = icons
        && sheet::check(canvas, c, lit(toggles.icons == Icons::Shown), Able::Enabled)
    {
        toggles.icons = match toggles.icons {
            Icons::Shown => Icons::Hidden,
            Icons::Hidden => Icons::Shown,
        };
    }

    let listed = toggles.unavailable == Unavailable::Listed;

    if let Some(c) = unavailable
        && sheet::check(canvas, c, lit(listed), Able::Enabled)
    {
        toggles.unavailable = if listed {
            Unavailable::Hidden
        } else {
            Unavailable::Listed
        };
        panels.state.row = None;
        panels.state.scroll = 0.0;
    }
}

/// The build list's two toggles, `BuildingListController`'s flags,
/// each a prefs record the original keeps
/// between games; here they last the session's game.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggles {
    pub icons: Icons,
    pub unavailable: Unavailable,
}

/// Whether the build list draws its icons: bit 0, `BuildingListVerbose`,
/// prefs record 5, default 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Icons {
    #[default]
    Shown,
    Hidden,
}

/// A price's colour where it is not short: white, after a red name.
const PLAIN_MARK: &str = "\u{1}FFFFFF";

/// A hero's statistics, items or spells.
fn heroic(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (chosen, hero, page): (&Chosen, &Hero, Page),
    game: &Game<'_>,
) {
    let text = &game.corpus.text;
    let own: Vec<i32> = match page {
        Page::Stats => {
            let mut own = vec![id::XP_BAR, id::XP_TEXT, 26, 27, 28, 29, 30];

            own.extend(details::STAT_CONTROLS);
            own
        }
        Page::Items => vec![
            id::WEAPON_NAME,
            id::ARMOUR_NAME,
            id::WEAPON,
            id::ARMOUR,
            id::GOLD,
            id::TAXED,
            id::POTIONS,
            id::POISONED,
            id::ITEM_LIST,
            id::ITEM_SCROLL,
        ],
        _ => vec![
            id::SPELL_LIST,
            id::ENCHANTMENTS,
            id::SPELL_SCROLLS[0],
            id::SPELL_SCROLLS[1],
        ],
    };
    let mut skipped = own.clone();

    skipped.extend(id::TABS.iter().map(|(c, _)| *c));
    frame(canvas, layout, Some(&chosen.name), &skipped);
    back(canvas, panels, layout, Span::One);

    for (control, tab) in id::TABS {
        if let Some(c) = layout.control(control)
            && sheet::button(canvas, c, None, Able::Enabled)
        {
            panels.state.page = tab;
            panels.state.hero_page = Some(tab);
            panels.state.scroll = 0.0;
            panels.state.enchant_scroll = 0.0;
        }
    }

    match page {
        Page::Stats => {
            if let Some(c) = layout.control(id::XP_BAR) {
                let xp = hero.experience;
                #[allow(clippy::cast_precision_loss, reason = "experience is small")]
                let filled = if xp.needed > 0 {
                    xp.earned as f32 / xp.needed as f32
                } else {
                    0.0
                };

                sheet::art_bar(
                    canvas,
                    c,
                    (filled, sheet::Fill::Plain),
                    Color32::from_rgb(40, 110, 210),
                );
            }

            if let Some(line) = hero.experience.text(text) {
                sheet::fill(canvas, layout, id::XP_TEXT, &line, (None, Align::Centre));
            }

            upper::stats(canvas, layout, hero, game);
        }
        Page::Items => items(canvas, panels, layout, hero, (&game.corpus.catalogue, text)),
        _ => spells(canvas, panels, layout, hero, game),
    }
}

/// A hero's gear, purse, potions and items (`HeroItemController`).
fn items(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    hero: &Hero,
    (catalogue, text): (&Catalogue<'_>, &Text),
) {
    for (gear, name, picture) in [
        (hero.weapon, id::WEAPON_NAME, id::WEAPON),
        (hero.armour, id::ARMOUR_NAME, id::ARMOUR),
    ] {
        if let Some(line) = gear.name(text) {
            sheet::fill(canvas, layout, name, &line, (None, Align::Left));
        }

        upper::gear(canvas, layout, (picture, 0), gear::without_bonus(gear));
    }

    for (control, value) in [
        (id::GOLD, hero.gold),
        (id::TAXED, hero.taxed),
        (id::POTIONS, hero.potions),
    ] {
        sheet::fill(
            canvas,
            layout,
            control,
            &value.to_string(),
            (None, Align::Centre),
        );
    }

    let rows: Vec<Row> = hero
        .items
        .iter()
        .map(|&key| {
            let (name, effect) = details::item(catalogue, text, key)
                .unwrap_or_else(|| (unnamed(key), String::new()));

            Row {
                icon: None,
                lead: None,
                text: name,
                tip: (!effect.is_empty()).then_some(effect),
                able: Able::Enabled,
            }
        })
        .collect();
    let area = layout
        .control(id::ITEM_LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::ITEM_SCROLL).and_then(sheet::rect);

    list(canvas, panels, (area, bar), &rows, LINE);
}

/// An item key no text names: `INV:11`, else its code.
fn unnamed(key: i32) -> String {
    let [a, b, c, n] = key.to_le_bytes();

    if [a, b, c] == INVENTORY {
        format!("INV:{n}")
    } else {
        FourCc::from_word(key.cast_unsigned()).to_string()
    }
}

/// A hero's spells, each its `INTn` icon and title (`HeroSpellsController`).
fn spells(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    hero: &Hero,
    game: &Game<'_>,
) {
    let book = icons::SPELLBOOK;
    let rows: Vec<Row> = hero
        .spells
        .iter()
        .map(|&code| Row {
            icon: Some(Pic::of(book, i32::from(icons::spell_icon(code)))),
            lead: None,
            text: details::spell_title(&game.corpus.catalogue, code)
                .unwrap_or_else(|| code.to_string()),
            tip: None,
            able: Able::Enabled,
        })
        .collect();
    let area = layout
        .control(id::SPELL_LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::SPELL_SCROLLS[0]).and_then(sheet::rect);

    list(canvas, panels, (area, bar), &rows, ROW);
    enchantments(canvas, panels, layout, game, hero);
}

/// What enchants the hero ([`Hero::enchantments`]): each its icon and
/// `GMTX` title.
fn enchantments(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    hero: &Hero,
) {
    let Some(area) = layout.control(id::ENCHANTMENTS).and_then(sheet::rect) else {
        return;
    };

    for (n, e) in hero.enchantments.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a few rows")]
        let r = Rect::from_min_size(
            pos2(
                area.left(),
                area.top() + ENCHANT_ROW * n as f32 - panels.state.enchant_scroll,
            ),
            vec2(area.width(), ENCHANT_ROW),
        );

        if r.bottom() <= area.top() || r.top() >= area.bottom() {
            continue;
        }

        let title = e.name(&game.corpus.catalogue, &game.corpus.text);

        if let Some((record, set)) = e.icon {
            canvas.clipped(
                Pic::of(record, i32::from(set)),
                Tone::Plain,
                pos2(r.left() + 2.0, r.top() + (ENCHANT_ROW - ICON) / 2.0),
                area,
            );
        }

        let inset = if e.icon.is_some() { 29.0 } else { 2.0 };
        row_text(
            canvas,
            area,
            Rect::from_min_max(pos2(r.left() + inset, r.top()), r.max),
            &title,
            WHITE,
        );
    }

    let bar = layout.control(id::SPELL_SCROLLS[1]).and_then(sheet::rect);
    #[allow(clippy::cast_precision_loss, reason = "a short list")]
    let tall = hero.enchantments.len() as f32 * ENCHANT_ROW;

    scrolled(canvas, &mut panels.state.enchant_scroll, bar, (area, tall));
}

/// The question, `AP62`, in the selection's slot, where one waits: a
/// demolition's, a dismissal's, a flag's removal's (**Game**:
/// `ui-hero-dismissal-flow.png`, under the minimap; `panel_open`
/// opens it, as `APb5` after it). Whether one was drawn.
pub fn ask(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
) -> bool {
    let flag = orders.orders.confirm.filter(|c| chosen.agent == *c);
    let layout = (panels.state.ask.is_some() || flag.is_some())
        .then(|| panels.layout(&game.corpus.catalogue, &game.corpus.text, QUESTION))
        .flatten();

    if let Some(layout) = &layout {
        question(canvas, panels, layout, game, orders);
    }

    layout.is_some()
}

/// The question: demolish this building, dismiss this hero, or remove
/// this flag.
fn question(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    game: &Game<'_>,
    orders: &mut orders::Panel,
) {
    let text = &game.corpus.text;
    let tail = match panels.state.ask {
        Some(Ask::Demolish(_)) => DEMOLISH_TEXT,
        Some(Ask::Dismiss(_)) => DISMISS_TEXT,
        None => UNFLAG_TEXT,
    };
    let asked = format!(
        "{}{}",
        text.gmtx(ASK_TEXT).unwrap_or_default(),
        text.gmtx(tail).unwrap_or_default()
    );

    sheet::statics(canvas, layout, None, &|c| {
        [id::QUESTION_TEXT, id::YES, id::NO].contains(&c)
    });

    if let Some(c) = layout.control(id::QUESTION_TEXT)
        && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
    {
        canvas.paragraphs(font, r, &asked, (sheet::colour(c), Align::Centre, 0.0));
    }

    let yes = layout
        .control(id::YES)
        .is_some_and(|c| sheet::button(canvas, c, None, Able::Enabled));
    let no = layout
        .control(id::NO)
        .is_some_and(|c| sheet::button(canvas, c, None, Able::Enabled));

    match (yes, no, panels.state.ask) {
        (true, _, Some(Ask::Demolish(building))) => {
            orders.orders.outbox.push(Input::Demolish(building));
            panels.state.ask = None;
        }
        (true, _, Some(Ask::Dismiss(hero))) => {
            orders.orders.outbox.push(Input::Dismiss(hero));
            panels.state.ask = None;
        }
        (true, _, None) => orders::confirmed(&mut orders.orders, game.world),
        (_, true, _) => {
            panels.state.ask = None;
            orders.orders.confirm = None;
        }
        _ => {}
    }
}

/// A gear without its bonus: the items page shows it in the name.
mod gear {
    use regent_core::view::panel::portrait::gear::Gear;

    pub const fn without_bonus(gear: Gear) -> Gear {
        Gear { enchant: 0, ..gear }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::config::Typeface;

    #[test]
    fn the_fairgrounds_research_page_is_apa0() {
        // `FairgroundController`: 8851 opens `APa0`.
        let fair = Some(FourCc(*b"AP08"));

        assert_eq!(research_panel(fair), FourCc(*b"APa0"));
    }

    #[test]
    fn the_fairgrounds_visitors_page_is_ap09() {
        // `FairgroundController`: 8021 opens `AP09`, not `AP91`.
        assert_eq!(visitors_panel(Some(FourCc(*b"AP08"))), FourCc(*b"AP09"));
        assert_eq!(visitors_panel(Some(FourCc(*b"APb8"))), VISITORS);
        assert_eq!(visitors_panel(None), VISITORS);
    }

    #[test]
    fn a_chest_and_a_sign_are_buildings_to_the_help_page() {
        assert_eq!(engine_kind(upper::Kind::Chest), kind::Kind::BUILDING);
        assert_eq!(engine_kind(upper::Kind::Sign), kind::Kind::BUILDING);
        assert_eq!(
            engine_kind(upper::Kind::Flag(FlagKind::Attack)),
            kind::Kind::REWARD
        );
    }

    /// One press on a build list row picks it; two place it, if offered.
    #[test]
    fn a_build_row_is_placed_on_a_double_click_only() {
        let offer = |offered| Offer {
            code: FourCc(*b"ABH1"),
            price: 400,
            offered,
        };

        assert_eq!(build_press(offer(true), Press::Single), Mode::Idle);
        assert_eq!(
            build_press(offer(true), Press::Double),
            Mode::Build(FourCc(*b"ABH1"))
        );
        assert_eq!(build_press(offer(false), Press::Double), Mode::Idle);
    }

    /// The build list and its tips are the palace's alone: nothing
    /// selected draws no page; anything else its tips.
    #[test]
    fn the_build_list_shows_only_under_the_palace() {
        let topic = Page::Topic(FourCc(*b"ABH1"));

        assert_eq!(shown(Page::Build, Holder::Palace), Some(Page::Build));
        assert_eq!(shown(topic, Holder::Palace), Some(topic));
        assert_eq!(shown(Page::Build, Holder::Nothing), None);
        assert_eq!(shown(Page::Help, Holder::Nothing), None);
        assert_eq!(shown(Page::Build, Holder::Other), Some(Page::Help));
        assert_eq!(shown(topic, Holder::Other), Some(Page::Help));
        assert_eq!(shown(Page::Members, Holder::Other), Some(Page::Members));
    }

    /// Closing the build list leaves any mode (building list dtor,
    /// map mode reset): a building being placed is dropped.
    #[test]
    fn leaving_the_build_list_drops_a_placement() {
        let placing = Mode::Build(FourCc(*b"ABH1"));
        let list = Some(Page::Build);

        assert_eq!(after_page(list, None, placing), Mode::Idle);
        assert_eq!(after_page(list, Some(Page::Help), placing), Mode::Idle);
        assert_eq!(after_page(list, Some(Page::Rewards), placing), Mode::Idle);
        assert_eq!(after_page(list, list, placing), placing);
        assert_eq!(after_page(None, list, placing), placing);
        assert_eq!(after_page(Some(Page::Help), None, placing), placing);
    }

    /// The Mausoleum's two interred, one the treasury cannot pay for.
    const CRYPT_ROWS: [Button<u32>; 2] = [
        Button {
            what: 16,
            price: 2_000,
            ready: true,
        },
        Button {
            what: 18,
            price: 4_500,
            ready: false,
        },
    ];

    /// No row lit, no price and RESURRECT dimmed.
    #[test]
    fn resurrect_waits_for_a_lit_row() {
        assert_eq!(raised(&CRYPT_ROWS, None), None);
        assert_eq!(raised(&CRYPT_ROWS, Some(99)), None);
        assert_eq!(resurrect_able(None), Able::Disabled);
    }

    /// A lit row shows its price; RESURRECT sends only what the treasury
    /// holds, the price red otherwise.
    #[test]
    fn a_lit_row_prices_resurrect() {
        let paid = raised(&CRYPT_ROWS, Some(16));
        let short = raised(&CRYPT_ROWS, Some(18));

        assert_eq!(paid.map(|b| b.price), Some(2_000));
        assert_eq!(resurrect_able(paid), Able::Enabled);
        assert_eq!(paid.map(price_colour), Some(WHITE));
        assert_eq!(short.map(|b| b.price), Some(4_500));
        assert_eq!(resurrect_able(short), Able::Disabled);
        assert_eq!(short.map(price_colour), Some(SHORT_PRICE));
    }

    /// RESURRECT sends `GUILD_ACTION` 0x15 for the lit hero.
    #[test]
    fn resurrect_sends_the_lit_hero_and_its_price() {
        assert_eq!(
            resurrect(7, CRYPT_ROWS[0]),
            Input::Resurrect(Resurrect {
                mausoleum: 7,
                hero: 16,
                price: 2_000,
            })
        );
    }

    /// Each row of every research page shows an icon the game files
    /// hold: its own at the button's id + 500, else a static of the
    /// panel's on its line (`MX03`'s 5500 to 5505 beside 5061 to 5066).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn every_research_row_shows_an_icon_from_the_game_files() {
        use regent_core::view::menu;
        use regent_draw::art::read::sprite;

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let corpus = regent_draw::data::Corpus::open(
            &crate::game::load::archives(&hd).expect("the dataset"),
        )
        .expect("corpus");

        for (_, page) in research_page::PAGES {
            let layout = menu::layout(&corpus.catalogue, &corpus.text, page)
                .expect("the page")
                .expect("parsed");
            let rows = row_controls(&layout);
            let buttons = layout
                .controls
                .iter()
                .filter(|c| c.id.is_some_and(|i| research_page::BUTTONS.contains(&i)));

            for button in buttons {
                let id = button.id.unwrap_or_default();
                let line = sheet::rect(button).expect("a rectangle");
                let beside = |c: &&ShownControl| {
                    c.id.is_some_and(|i| !rows.contains(&i))
                        && sheet::rect(c).is_some_and(|r| {
                            r.center().x < line.left() && r.y_range().contains(line.center().y)
                        })
                };
                let icon = layout
                    .control(id + research_page::ICON_STEP)
                    .or_else(|| layout.controls.iter().find(beside));
                let art = icon.and_then(|c| c.art);

                assert!(
                    art.is_some_and(|(record, set)| sprite(
                        &corpus.catalogue,
                        Pic::of(record, set)
                    )
                    .is_some()),
                    "{page:?} row {id}: {art:?}"
                );
            }
        }
    }

    #[test]
    fn a_row_scrolled_half_out_of_its_list_still_shows_its_icon() {
        let area = Rect::from_min_size(pos2(11.0, 30.0), vec2(164.0, 182.0));

        assert!(icon_at(pos2(13.0, 15.0), ROW, area).is_some());
        assert!(icon_at(pos2(13.0, 200.0), ROW, area).is_some());
        assert!(icon_at(pos2(13.0, 0.0), ROW, area).is_none());
    }

    /// A unit row's text too long for its row is cut short, not drawn
    /// over the next row (a Fairgrounds visitor's long name and errand).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_long_row_text_stays_within_its_row() {
        let area = Rect::from_min_size(pos2(10.0, 10.0), vec2(160.0, 200.0));
        let words = Rect::from_min_size(pos2(50.0, 50.0), vec2(120.0, UNIT_ROW));
        let text = "Sir Persis of North Glen the Unrelenting Hammer of Krolm is \
                    Purchasing healing potions at the Marketplace of Esterhausen \
                    (18 of 18 hp)";
        // A glyph's dark edge reaches a pixel out.
        let room = words.expand(1.0);

        for face in [Typeface::Egui, Typeface::Inknut] {
            let shown = crate::panel::canvas::tests::painted_bounds_in(face, |c| {
                row_text(c, area, words, text, WHITE);
            });

            assert!(!shown.is_empty(), "{face:?}");
            assert!(
                shown.iter().all(|r| room.contains_rect(*r)),
                "{face:?}: {shown:?}"
            );
        }
    }
}
