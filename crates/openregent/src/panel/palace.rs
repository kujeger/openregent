//! The palace's whole-column pages: its statistics and its three rosters,
//! `UIData`'s 202 × 490 from (0, 182), over both slots.
//!
//! ```text
//!  STATISTICS  AP40  SummaryChartsController     World::statistics
//!  ROSTER      AP85  Top5Controller               heroes     ┐ tabs 5013,
//!              APc1  KingdomBuildingsController   buildings  │ 5012, 5011
//!              APc2  HenchmenListController       henchmen   ┘
//!  each: list 5000 and its bar, back 8013 (Tab) to the palace; a roster's
//!  detail check 5004, sorts 5005-5007, SELECT 5003
//! ```

use std::sync::Arc;

use bevy_egui::egui::{Rect, pos2, vec2};
use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::INTERFACE_ICON;
use regent_core::data::text::{Arg, printf};
use regent_core::sim::world::{
    self, World,
    guild::upkeep::Setting,
    kingdom::{self, Roster, Sorted},
    ledger,
};
use regent_core::view::menu::ShownControl;
use regent_core::view::panel::portrait;
use regent_core::view::panel::tip;
use regent_draw::data::Corpus;

use super::canvas::Canvas;
use super::ink::{Pic, Tone};
use super::lower::{self, Asked, Row, Span};
use super::sheet::{self, Able, Lit};
use super::upper::{Chosen, Game};
use super::{Page, Panels};

/// Each page's panel.
pub const STATISTICS: FourCc = FourCc(*b"AP40");
pub const HEROES: FourCc = FourCc(*b"AP85");
pub const BUILDINGS: FourCc = FourCc(*b"APc1");
pub const HENCHMEN: FourCc = FourCc(*b"APc2");

/// The pages' controls.
mod id {
    use super::{Page, kingdom};

    pub const LIST: i32 = 5000;
    pub const STATISTICS_BAR: i32 = 5001;
    pub const ROSTER_BAR: i32 = 5010;
    pub const SELECT: i32 = 5003;
    /// The roster's zoom, placed off the page at (1000, 1000).
    pub const ZOOM: i32 = 5001;
    pub const DETAIL: i32 = 5004;
    pub const SORTS: [(i32, kingdom::Sort); 3] = [
        (5005, kingdom::Sort::Name),
        (5006, kingdom::Sort::Level),
        (5007, kingdom::Sort::Type),
    ];
    pub const TABS: [(i32, Page); 3] = [
        (5013, Page::Heroes),
        (5012, Page::Buildings),
        (5011, Page::Henchmen),
    ];
}

/// A statistics line's height: `AP40` 5000's key 28.
const STATISTICS_LINE: f32 = 15.0;

/// A roster row's height with no detail;
/// with it, `lower::UNIT_ROW`, `lower::HENCHMAN_ROW` for henchmen.
const BRIEF_ROW: f32 = 16.0;

/// A building row's text: `GMTX` 216 "%s (%d of %d hp), %d Gold", a
/// guild's 217 with "%d of %d Members"; a henchman's 218, a tax
/// collector's 219 with its gold.
const BUILDING_ROW: usize = 216;
const GUILD_ROW: usize = 217;
const HENCHMAN_TEXT: usize = 218;
const TAXMAN_ROW: usize = 219;

/// The tax collector, whose row says its gold.
const TAX_COLLECTOR: FourCc = FourCc(*b"AVg1");

/// A building row's tax and repair marks, `INBb`: route, once;
/// neither, the route's in slot 2.
const MARKS: FourCc = FourCc(*b"INBb");
const TAX_ROUTE: i32 = 1035;
const TAX_ONCE: i32 = 1036;
const REPAIR_ROUTE: i32 = 1037;
const REPAIR_ONCE: i32 = 1038;
const UNSET_SLOT: usize = 2;

/// The marks' place in a row: 28 from its right, repair at its top, tax
/// 20 below.
const MARK_LEFT: f32 = 28.0;
const MARK_DROP: f32 = 20.0;

/// A roster's rows and their agents, by game frame, palace, roster and
/// [`Rosters`].
pub(super) type Listed = crate::ui::Memo<(u32, u32, Roster, Rosters), Arc<(Vec<u32>, Vec<Row>)>>;

/// The rosters' sorts, one each as the engine's globals, and the detail
/// check (a byte's bit 0). **Inferred**: detail on at first, as
/// the capture shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rosters {
    pub sorts: [kingdom::Sort; 3],
    pub detail: Detail,
}

/// Whether roster rows say hit points and gold, or only the name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Detail {
    #[default]
    Shown,
    Hidden,
}

/// The frames' middle, `INse` 1013, 202 × 16: `UIData` gives it a taller
/// rectangle (382 on `AP40`), key 43 2 where the other pieces have 1.
/// **Inferred**: key 43 2 repeats the picture down its rectangle.
const MIDDLE: (FourCc, i32) = (FourCc(*b"INse"), 1013);

/// The page's pictures and labels, the frame's middle repeated down its
/// rectangle.
fn frame(canvas: &mut Canvas<'_>, layout: &regent_core::view::menu::Layout, own: &[i32]) {
    lower::frame(canvas, layout, None, own);

    for c in layout.controls.iter().filter(|c| c.art == Some(MIDDLE)) {
        if let Some(r) = sheet::rect(c) {
            canvas.tiled(Pic::of(MIDDLE.0, MIDDLE.1), r);
        }
    }
}

/// The kingdom's statistics: the 26 lines of `World::statistics`, each
/// its `GMTX` text with its number.
pub fn statistics(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &regent_core::view::menu::Layout,
    game: &Game<'_>,
) {
    let text = &game.corpus.text;
    let rows: Vec<Row> = game
        .world
        .statistics(game.local)
        .iter()
        .take(ledger::LINES)
        .map(|line| {
            let format = text.gmtx(line.text).unwrap_or_default();
            let numbers: Vec<Arg> = line.number.into_iter().map(Arg::from).collect();

            Row {
                icon: None,
                lead: None,
                text: printf(&format, &numbers),
                tip: None,
                able: Able::Enabled,
            }
        })
        .collect();

    frame(canvas, layout, &[id::LIST, id::STATISTICS_BAR]);
    lower::back(canvas, panels, layout, Span::Both);

    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::STATISTICS_BAR).and_then(sheet::rect);

    lower::list(canvas, panels, (area, bar), &rows, STATISTICS_LINE);
}

/// A roster: the palace owner's heroes, buildings or henchmen, sorted;
/// a building row picked centres the view on it.
pub fn roster(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &regent_core::view::menu::Layout,
    (chosen, page): (&Chosen, Page),
    game: &Game<'_>,
) -> Asked {
    let which = match page {
        Page::Buildings => Roster::Buildings,
        Page::Henchmen => Roster::Henchmen,
        _ => Roster::Heroes,
    };
    let own: Vec<i32> = [id::LIST, id::ROSTER_BAR, id::SELECT, id::ZOOM, id::DETAIL]
        .into_iter()
        .chain(id::SORTS.iter().map(|(c, _)| *c))
        .chain(id::TABS.iter().map(|(c, _)| *c))
        .collect();

    frame(canvas, layout, &own);
    lower::back(canvas, panels, layout, Span::Both);
    controls(canvas, panels, layout, (page, which), &game.corpus.text);

    // The rows, made again only on a new game frame or roster: each
    // names its unit and finds it among every piece.
    let rosters = panels.state.rosters;
    let listed = panels
        .state
        .listed
        .get((game.frame, chosen.agent, which, rosters), || {
            let ids = sorted(game, chosen.agent, which, rosters.sorts[which as usize]);
            let rows = ids
                .iter()
                .map(|&id| match rosters.detail {
                    Detail::Shown => detailed(game, id, which),
                    Detail::Hidden => brief(game, id),
                })
                .collect();

            Arc::new((ids, rows))
        })
        .clone();
    let (ids, rows) = &*listed;
    let pitch = match (rosters.detail, which) {
        (Detail::Hidden, _) => BRIEF_ROW,
        (Detail::Shown, Roster::Henchmen) => lower::HENCHMAN_ROW,
        (Detail::Shown, _) => lower::UNIT_ROW,
    };
    let area = layout
        .control(id::LIST)
        .and_then(sheet::rect)
        .unwrap_or(Rect::ZERO);
    let bar = layout.control(id::ROSTER_BAR).and_then(sheet::rect);
    let marked = (which, rosters.detail) == (Roster::Buildings, Detail::Shown);
    let inset = if marked { MARK_LEFT } else { 0.0 };
    let pressed = lower::inset_list(canvas, panels, (area, bar), rows, (pitch, inset));

    if marked {
        marks(canvas, game.world, ids, (area, pitch, panels.state.scroll));
    }

    let at = |id: u32| game.now.iter().find(|s| s.agent == id).map(|s| s.at);
    let mut asked = Asked::default();

    // `KingdomBuildingsController`: a building picked is centred;
    // another roster's has the tracking window follow it
    // (`Base_UnitListController`, 5000).
    let picked = pressed.and_then(|n| ids.get(n)).copied();

    if which == Roster::Buildings {
        asked.zoom = picked.and_then(at);
    } else {
        asked.track = picked;
    }

    let lit = panels.state.row.and_then(|n| ids.get(n)).copied();

    if let Some(c) = layout.control(id::SELECT)
        && sheet::button(canvas, c, None, Able::of(lit.is_some()))
    {
        asked.select = lit;
    }

    asked
}

/// The tabs, the sorts and the detail check; a press changes the page or
/// the state.
fn controls(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &regent_core::view::menu::Layout,
    (page, which): (Page, Roster),
    text: &regent_core::data::text::Text,
) {
    let lit = |on: bool| if on { Lit::On } else { Lit::Off };

    for (control, to) in id::TABS {
        if let Some(c) = layout.control(control)
            && sheet::check(canvas, c, lit(to == page), Able::Enabled)
            && to != page
        {
            panels.state.page = to;
            panels.state.scroll = 0.0;
            panels.state.row = None;
        }
    }

    // Mode 0 lights the name radio.
    let sort = &mut panels.state.rosters.sorts[which as usize];

    for (control, by) in id::SORTS {
        let on = *sort == by || (by == kingdom::Sort::Name && *sort == kingdom::Sort::Unset);

        if let Some(c) = layout.control(control)
            && sheet::check(canvas, c, lit(on), Able::Enabled)
        {
            *sort = by;
        }
    }

    let detail = &mut panels.state.rosters.detail;
    // `Base_UnitListController`: the list's way, by the check.
    let line = match *detail {
        Detail::Shown => tip::line::DETAILED,
        Detail::Hidden => tip::line::SIMPLE,
    };
    let shown = layout.control(id::DETAIL).map(|c| ShownControl {
        tooltip: tip::said(text, line).or_else(|| c.tooltip.clone()),
        ..c.clone()
    });

    if let Some(c) = shown
        && sheet::check(canvas, &c, lit(*detail == Detail::Shown), Able::Enabled)
    {
        *detail = match *detail {
            Detail::Shown => Detail::Hidden,
            Detail::Hidden => Detail::Shown,
        };
    }
}

/// The roster's agents in `sort`'s order.
fn sorted(game: &Game<'_>, palace: u32, which: Roster, sort: kingdom::Sort) -> Vec<u32> {
    let world = game.world;
    let mut rows: Vec<Sorted> = world
        .roster_of(palace, which)
        .into_iter()
        .filter_map(|id| {
            let shown = game.now.iter().find(|s| s.agent == id)?;

            Some(Sorted {
                id,
                name: listed(game.corpus, world, shown),
                level: world.word_of(id, attrib::EXPERIENCE_LEVEL),
                code: shown.code,
                guild: world.members(id).is_some(),
            })
        })
        .collect();

    kingdom::sort(&mut rows, sort);
    rows.iter().map(|r| r.id).collect()
}

/// A roster row's name ([`portrait::listed_name`]).
fn listed(corpus: &Corpus, world: &World, shown: &world::draw_view::Shown) -> String {
    let who = portrait::who(world, shown.agent, shown.code);

    portrait::listed_name(&corpus.catalogue, &corpus.text, &who)
}

/// A row with no detail: the name alone, no icon.
fn brief(game: &Game<'_>, id: u32) -> Row {
    let name = game
        .now
        .iter()
        .find(|s| s.agent == id)
        .map(|s| listed(game.corpus, game.world, s))
        .unwrap_or_default();

    Row {
        icon: None,
        lead: None,
        text: name,
        tip: None,
        able: Able::Enabled,
    }
}

/// A detailed row: a hero's level and doing,
/// a henchman's or building's hit points and gold, a guild's members.
fn detailed(game: &Game<'_>, id: u32, which: Roster) -> Row {
    let (world, text) = (game.world, &game.corpus.text);
    let shown = game.now.iter().find(|s| s.agent == id);
    let chosen = shown.map(|s| Chosen::of(game.corpus, (world, game.local), s));
    let name = shown
        .map(|s| listed(game.corpus, world, s))
        .unwrap_or_default();
    let (now, most) = chosen
        .as_ref()
        .and_then(|c| c.health)
        .map_or((0, 0), |h| (h.now, h.max));
    let gold = world.word_of(id, attrib::GOLD);
    let code = shown.map(|s| s.code);
    let line = match which {
        Roster::Heroes => None,
        Roster::Henchmen if code == Some(TAX_COLLECTOR) => text
            .gmtx(TAXMAN_ROW)
            .map(|f| printf(&f, &[(&name).into(), now.into(), most.into(), gold.into()])),
        Roster::Henchmen => text
            .gmtx(HENCHMAN_TEXT)
            .map(|f| printf(&f, &[(&name).into(), now.into(), most.into()])),
        Roster::Buildings => match world.members(id) {
            Some(m) => {
                let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);

                let args = [
                    (&name).into(),
                    now.into(),
                    most.into(),
                    count(m.living).into(),
                    m.spots.into(),
                    gold.into(),
                ];

                text.gmtx(GUILD_ROW).map(|f| printf(&f, &args))
            }
            None => text
                .gmtx(BUILDING_ROW)
                .map(|f| printf(&f, &[(&name).into(), now.into(), most.into(), gold.into()])),
        },
    };

    match (which, line) {
        (Roster::Heroes, _) => lower::unit_row(game, id, text),
        (_, line) => Row {
            icon: code.map(|c| Pic::of(c, i32::from(INTERFACE_ICON))),
            lead: None,
            text: line.unwrap_or(name),
            tip: None,
            able: Able::Enabled,
        },
    }
}

/// Each shown building row's repair and tax marks at its right: the
/// route's, the once's, else the route's greyed; no tax mark on a
/// building that pays none.
fn marks(
    canvas: &mut Canvas<'_>,
    world: &World,
    ids: &[u32],
    (area, pitch, scroll): (Rect, f32, f32),
) {
    for (n, &id) in ids.iter().enumerate() {
        #[allow(clippy::cast_precision_loss, reason = "a list's rows")]
        let top = area.top() + n as f32 * pitch - scroll;
        let left = area.right() - MARK_LEFT;
        let upkeeps = world.upkeeps(id);
        let repair = Setting::of(
            world.word_of(id, attrib::IS_REPAIRED),
            world.word_of(id, attrib::QUICK_REPAIR),
        );
        let marks = [
            (Some(repair), REPAIR_ROUTE, REPAIR_ONCE, 0.0),
            (upkeeps.and_then(|u| u.tax), TAX_ROUTE, TAX_ONCE, MARK_DROP),
        ];

        for (setting, route, once, drop) in marks {
            let pic = match setting {
                Some(Setting::Always) => Some(Pic::of(MARKS, route)),
                Some(Setting::Once) => Some(Pic::of(MARKS, once)),
                Some(Setting::Never) => Some(Pic::of(MARKS, route).slot(UNSET_SLOT)),
                None => None,
            };
            let at = pos2(left, top + drop);
            let seen = Rect::from_min_size(at, vec2(MARK_LEFT, MARK_DROP)).intersects(area);

            if let (Some(pic), true) = (pic, seen) {
                canvas.clipped(pic, Tone::Plain, at, area);
            }
        }
    }
}
