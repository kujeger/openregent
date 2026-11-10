//! The selection panel: the selected piece's `DialogID` panel in the
//! column's slot at (0, 182), filled from the world.
//!
//! ```text
//!  every panel   8011 portrait (a grave's grey), 8005 name, 8006 and 8010
//!                hit points, 8014 its line, 8000 zoom, 8028/8029 tips
//!  hero   AP20   7002 class, 7003 level, 4016 xp, 8030 doing, 5001/5004
//!                weapon and armour at their upgrade, 5002/5003 bonus,
//!                SPELLS ITEMS STATS open the page below
//!         APb5   dismissed or another's: 9901 owner's banner, 8030
//!                "Follows another monarch."
//!  building      7720 level, 8007/8015 upgrade, 8024-8027 tax and repair,
//!                8018/8019 members, 8002 demolish (asks first), the band
//!                at (7, 190): recruits, enchant, camp, embassy, or a
//!                running recruit's or research's bar; page buttons
//!         APb4   another's: 9901 owner's banner, 8030
//!                "Belongs to another monarch."
//!  henchman APc5 another's: 9901 owner's banner, 8030
//!                "Follows another monarch."
//!  taxman AP74   5000 carried, 5001/5002 pickup and return, ± 11-14
//!  caravan AP90  5000 carried
//!  monster AP97  4001-4003 ratings
//!  flag          5004 reward, 5003 raise, 5000 remove, 8888 target,
//!                8014 offered by, 8019 seekers, 8995 View Heroes
//! ```
//!
//! One file per kind; `show` and what every panel draws stay here. The
//! control ids and choice rules are the library's
//! ([`regent_core::view::panel::upper`]).
//!
//! ```text
//!  show ──┬── hero       class, level, gear, page buttons; stats page
//!         ├── building   level, upgrade, upkeep, counts, page buttons
//!         │     └── band       bar, recruits, enchant; second row:
//!         │           ├── war_band   a temple's recruits
//!         │           ├── guild      camp, Call to Arms, Rage
//!         │           ├── market     Market Day
//!         │           ├── embassy    open, close
//!         │           └── rules      extortion
//!         ├── taxman, monster, flag
//!         └── rules      the warriors' roster
//! ```

mod band;
mod building;
mod embassy;
mod flag;
mod guild;
mod hero;
mod market;
mod monster;
mod rules;
mod taxman;
mod war_band;

pub(crate) use building::rest_bar;
pub use hero::{gear, stats};

use bevy_egui::egui::{self, Color32, Pos2, Rect, vec2};
use regent_core::data::attrib;
use regent_core::data::fourcc::FourCc;
use regent_core::data::text::Text;
use regent_core::gpl::load::Program;
use regent_core::sim::game::Input;
use regent_core::sim::world::{
    self, World, chat,
    guild::extortion::Extortion,
    kingdom::Henchmen,
    naming::{NameBox, Rename},
    offer::Button,
};
use regent_core::view::menu::{Layout, ShownControl, ids};
use regent_core::view::panel::portrait::details::{
    self, Board, Coffer, Details, Foreign, Posted, RATED,
};
use regent_core::view::panel::portrait::gear;
use regent_core::view::panel::portrait::{self, Crown, Health};
use regent_core::view::panel::tip;
use regent_core::view::panel::upper::{
    self, BAND_IDS, CONTEST_ICONS, CONTESTS, Contests, GAMBLE_PANEL, HALL_PANEL, OUTPOST_PANEL,
    Opens, Owner, PALACE_PANELS, ROGUES_PANEL, id,
};
use regent_draw::data::Corpus;

use super::canvas::{Align, Canvas};
use super::games::GAMBLE_IDS;
use super::ink::{Pic, Tone};
use super::sheet::{self, Able};
use super::{Page, Panels};
use crate::input::orders::{self, FlagKind};

use building::building;
use embassy::{CLOSE_EMBASSY, OPEN_EMBASSY};
use flag::flag;
use hero::heroic;
use monster::monster;
use taxman::taxman;

/// The empty panel's crown: HD's (`portrait::Crown`).
pub const CROWN_PICTURE: Crown = Crown::Gold;

/// The hit point bar's colour: the captures' red (**Inferred** stand-in
/// for `INBc`).
pub const HEALTH: Color32 = Color32::from_rgb(190, 20, 20);

/// A recruit's or research's bar.
const WORKING: Color32 = Color32::from_rgb(170, 150, 90);

/// A price over the treasury.
pub const SHORT: Color32 = Color32::from_rgb(230, 60, 50);

/// What 8851 opens as a page below, by the library's rule
/// ([`upper::research_opens`]).
const fn research_page(panel: Option<FourCc>) -> Option<Page> {
    match upper::research_opens(panel) {
        Some(Opens::Bounty) => Some(Page::Bounty),
        Some(Opens::Research) => Some(Page::Research),
        None => None,
    }
}

/// The Blacksmith's gear pictures and their "+n" boxes, drawn by `forge`.
const FORGE_IDS: [i32; 4] = [
    gear::FORGE_WEAPON.0,
    gear::FORGE_WEAPON.1,
    gear::FORGE_ARMOUR.0,
    gear::FORGE_ARMOUR.1,
];

/// What kind of panel a piece has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Hero,
    /// A tax collector or a caravan: its load in 5000.
    Purse,
    Monster,
    Building,
    Flag(FlagKind),
    Chest,
    Sign,
    /// Any other character: a peasant, a guard.
    Other,
}

/// What the panels show of the selected piece.
pub struct Chosen {
    pub agent: u32,
    pub code: FourCc,
    pub kind: Kind,
    /// Its `DialogID`'s panel.
    pub panel: Option<FourCc>,
    /// As panel name writes it: `portrait::title`.
    pub name: String,
    /// Its descriptor's title.
    pub title: String,
    pub health: Option<Health>,
    /// Lying dead: its portrait in a grave's tone, no hit point text.
    pub grave: bool,
    /// `APV:27`, `PoisonCounter`: the hit point bar green above 0.
    pub poison: i32,
    pub details: Option<Details>,
    pub foreign: Option<Foreign>,
    pub owned: bool,
    pub at: (i32, i32),
    /// A Fairgrounds' contests.
    pub contests: Option<Contests>,
}

impl Chosen {
    /// Piece `agent`, of descriptor `code`, as the world keeps it, as
    /// player `local` sees it.
    pub fn of(
        corpus: &Corpus,
        (world, local): (&World, u32),
        shown: &world::draw_view::Shown,
    ) -> Chosen {
        let (agent, code) = (shown.agent, shown.code);
        let unit = corpus.unit(code);
        let piece = world.piece(agent);
        let attribute = |key| piece.and_then(|p| attrib::get(&p.attributes, key));
        let title = unit
            .as_ref()
            .map_or_else(|| code.to_string(), |u| u.desc.head.title().into_owned());
        let who = portrait::who(world, agent, code);
        let name = portrait::dropped_name(&corpus.catalogue, &corpus.text, world, agent)
            .unwrap_or_else(|| portrait::panel_name(&corpus.catalogue, &corpus.text, &who, &title));
        let foreign = unit
            .as_ref()
            .and_then(|unit| Foreign::of(&corpus.catalogue, &unit.desc, world, agent, local));
        let details = match (foreign, unit.as_ref(), piece) {
            (Some(Foreign::Building(_)), Some(unit), Some(piece)) => {
                Some(Details::Coffer(Coffer::of(&unit.desc, piece)))
            }
            _ => Details::of(&corpus.catalogue, world, agent, local),
        };
        let name = match &details {
            Some(Details::Posted(posted)) => posted.title(&corpus.catalogue, &corpus.text, name),
            _ => name,
        };

        // The descriptor's own maximum while the piece has none.
        let most = attribute(attrib::MAX_HP).or_else(|| portrait::most_hp(&unit.as_ref()?.desc));
        let has_bar = unit
            .as_ref()
            .is_none_or(|u| portrait::shows_health(&corpus.catalogue, u));
        let health = attribute(attrib::HP)
            .zip(most)
            .filter(|_| has_bar)
            .map(|(now, most)| Health::of(now, most));

        let panel = unit.as_ref().and_then(|u| {
            u.desc
                .character()
                .and_then(|c| c.dialog_id())
                .or_else(|| u.desc.building()?.dialog_id())
        });
        // A hero's own rule: `APb5` once dismissed.
        let panel = match &details {
            Some(Details::Hero(h)) => Some(h.panel),
            _ => foreign.map(Foreign::panel).or(panel),
        };
        let building = unit.as_ref().is_some_and(|u| u.desc.building().is_some());
        let kind = match (&details, code) {
            (_, world::offer::ATTACK_FLAG) => Kind::Flag(FlagKind::Attack),
            (_, world::offer::EXPLORE_FLAG) => Kind::Flag(FlagKind::Explore),
            (Some(Details::Hero(_)), _) => Kind::Hero,
            (Some(Details::Purse(_)), _) => Kind::Purse,
            (Some(Details::Monster(_)), _) => Kind::Monster,
            (Some(Details::Posted(p)), _) if p.board == Board::Chest => Kind::Chest,
            (Some(Details::Posted(_)), _) => Kind::Sign,
            _ if building => Kind::Building,
            _ => Kind::Other,
        };

        Chosen {
            agent,
            code,
            kind,
            panel,
            name,
            title,
            health,
            grave: shown.ruined && !building,
            poison: attribute(attrib::POISON_COUNTER).unwrap_or(0),
            details,
            foreign,
            owned: shown.owner == local,
            at: shown.at,
            contests: Contests::of(panel, piece),
        }
    }

    /// The panel the piece shows: its `DialogID`'s, a flag's by kind and
    /// owner ([`upper::flag_panel`]).
    #[must_use]
    pub const fn panel_id(&self) -> Option<FourCc> {
        let owner = if self.owned {
            Owner::Local
        } else {
            Owner::Other
        };

        match self.kind {
            Kind::Flag(_) => upper::flag_panel(self.code, owner),
            _ => self.panel,
        }
    }

    /// Whether the piece's panel is a palace's: its counts and pages.
    #[must_use]
    pub fn palace(&self) -> bool {
        self.panel.is_some_and(|p| PALACE_PANELS.contains(&p))
    }

    /// Whether the piece is the crown's rogues' guild: its panel asks
    /// the extortion's amount.
    #[must_use]
    pub fn rogues(&self) -> bool {
        self.owned && self.panel == Some(ROGUES_PANEL)
    }

    /// Whether the piece is the crown's Hall of Champions: its bounty
    /// page asks the terms' prices.
    #[must_use]
    pub fn hall(&self) -> bool {
        self.owned && self.panel == Some(HALL_PANEL)
    }

    /// The page the piece opens on.
    #[must_use]
    pub fn first_page(&self, world: &World, program: &Program, hero: Option<Page>) -> Page {
        let id = self.agent;

        match self.kind {
            Kind::Hero => hero.unwrap_or(HERO_PAGE),
            Kind::Building if self.foreign.is_some() => Page::Help,
            Kind::Building if self.palace() => Page::Build,
            Kind::Building if self.panel == Some(details::MAUSOLEUM_PANEL) => Page::Crypt,
            Kind::Building if world.members(id).is_some() => Page::Members,
            // Not once RESEARCH is hidden: a Fairgrounds' open contests.
            Kind::Building
                if self.contests.is_none_or(|c| c.shows(id::RESEARCH))
                    && !world.researches(program, id).is_empty() =>
            {
                Page::Research
            }
            _ => Page::Help,
        }
    }
}

/// A hero's first page while no hero page was opened: items, the
/// user's choice; the original's not read.
const HERO_PAGE: Page = Page::Items;

/// What a panel asks of the interface beyond its orders.
#[derive(Default)]
pub struct Asked {
    /// Centre the view here, 32nds.
    pub zoom: Option<(i32, i32)>,
    /// Read this message flag (`DestroyEventFlagMessage`).
    pub read: Option<u32>,
    /// The portrait of this agent pressed: an aimed spell's target.
    pub portrait: Option<u32>,
    /// The portrait of this agent under the pointer: an aimed spell's
    /// cursor (`map::cursor::Portrait`).
    pub hovered: Option<u32>,
    /// The message scroll's option set so (5005,
    /// `EventMessageController`, message mini view).
    pub follows: Option<bool>,
    /// The tracking window to follow this agent (8001,
    /// `StatsWindowController`, view follow).
    pub track: Option<u32>,
}

/// What the selection panel reads.
pub struct Game<'a> {
    pub corpus: &'a Corpus,
    /// The game frame the world is at: panel data is kept by it
    /// ([`crate::ui::Memo`]).
    pub frame: u32,
    pub world: &'a World,
    /// This machine's player ([`crate::game::sim::Sim::local`]).
    pub local: u32,
    pub program: &'a Program,
    /// Every piece shown this frame.
    pub now: &'a [world::draw_view::Shown],
    /// The Mausoleum's dead, priced (`Sim::crypt`); empty for others.
    pub crypt: &'a [Button<u32>],
    /// The local player's name: its flags' "Offered by".
    pub player: &'a str,
    /// LAN players' names, by seat; empty in a solo game.
    pub nicks: &'a [String],
    /// The selected palace's `GetSupportedHenchmen` answers, by
    /// [`Henchmen`] index; none for any other piece.
    pub henchmen: [Option<i32>; 3],
    /// The selected rogues' guild's extortion amount
    /// (`Game::extortion`); none for any other piece.
    pub extortion: Option<Extortion>,
    /// The selected hall's bounties, short then long (`Game::bounties`);
    /// none for any other piece.
    pub bounty: [Option<Button<world::guild::bounty::Term>>; 2],
    /// Whether deathmatch rules hold: a hero's skill caps.
    pub rules: world::offer::Rules,
    /// The HD install: its `UIData`.
    pub hd: &'a std::path::Path,
}

/// The empty panel: the crown over the slot.
pub fn crown(canvas: &mut Canvas<'_>) {
    let (record, set) = CROWN_PICTURE.art();

    canvas.picture(Pic::of(record, i32::from(set)), Tone::Plain, Pos2::ZERO);
}

/// The selection panel of `chosen`, its buttons sent through `orders`.
pub fn show(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    game: &Game<'_>,
    chosen: &Chosen,
    orders: &mut orders::Panel,
) -> Asked {
    let text = &game.corpus.text;
    let Some(layout) = chosen
        .panel_id()
        .and_then(|id| panels.layout(&game.corpus.catalogue, text, id))
    else {
        crown(canvas);
        return Asked::default();
    };

    let hero = match &chosen.details {
        Some(Details::Hero(h)) => Some(h),
        _ => None,
    };
    let backdrop = hero.map(|_| Pic::Backdrop(chosen.code));

    base(canvas, &layout, chosen, backdrop);
    common(canvas, &layout, chosen, text);
    name_box(canvas, panels, &layout, (game, chosen), orders);

    let mut asked = Asked::default();

    // `StatsWindowController` 8011: a press casts an aimed spell.
    // Under the pointer, its cursor.
    if let Some(r) = layout.control(id::PORTRAIT).and_then(sheet::rect) {
        let pressed = canvas.press(r, ("portrait", id::PORTRAIT));

        asked.portrait = pressed.clicked().then_some(chosen.agent);
        asked.hovered = pressed.hovered().then_some(chosen.agent);
    }

    if let Some(c) = layout.control(ids::ZOOM)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        asked.zoom = Some(chosen.at);
    }

    if let Some(c) = layout.control(ids::TRACK)
        && sheet::button(canvas, c, None, Able::Enabled)
    {
        asked.track = Some(chosen.agent);
    }

    for tip in id::TIPS {
        if let Some(c) = layout.control(tip)
            && sheet::button(canvas, c, None, Able::Enabled)
        {
            panels.state.page = Page::Help;
        }
    }

    match (chosen.kind, hero) {
        (Kind::Hero, Some(hero)) => heroic(canvas, panels, &layout, (game, chosen, hero), orders),
        (Kind::Purse, _) => taxman(canvas, &layout, game, chosen, orders),
        (Kind::Monster, _) => monster(canvas, &layout, chosen, text),
        (Kind::Building, _) => building(canvas, panels, &layout, game, chosen, orders),
        (Kind::Flag(kind), _) => flag(canvas, panels, &layout, game, (chosen, kind), orders),
        _ => {}
    }

    asked
}

/// The name box, 8005, on the crown's units:
/// a press edits it, unless a hero below `#HeroNameEditingLevel`, which
/// says so; while editing, Enter sends the name,
/// Escape or another selection drops it. Its text over the name shown.
fn name_box(
    canvas: &mut Canvas<'_>,
    panels: &mut Panels,
    layout: &Layout,
    (game, chosen): (&Game<'_>, &Chosen),
    orders: &mut orders::Panel,
) {
    let id = chosen.agent;
    let rect = layout.control(ids::TITLE).and_then(sheet::rect);
    let editing = panels.state.naming.as_ref().is_some_and(|(a, _)| *a == id);

    if !editing {
        panels.state.naming = None;
    }

    let rule = game.world.naming(game.program, game.local, id);

    match (rect, rule, panels.state.naming.as_mut()) {
        (Some(r), Some(_), Some((_, typed))) => {
            canvas.fill(r, EDITING);
            canvas.edit(r, (typed, world::naming::RENAME_MAX), "name box");

            let (enter, escape) = canvas.ui.ctx().input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });
            let name = typed.trim().to_owned();

            if enter && !name.is_empty() {
                orders
                    .orders
                    .outbox
                    .push(Input::Rename(Rename::new(id, &name)));
            }

            if enter || escape {
                panels.state.naming = None;
            }
        }
        (Some(r), Some(rule), None)
            if !matches!(chosen.kind, Kind::Flag(_))
                && canvas.press(r, ("name box", id)).clicked() =>
        {
            match rule {
                NameBox::Open => panels.state.naming = Some((id, chosen.name.clone())),
                NameBox::TooLow(level) => orders.orders.said.push(chat::Line::Counted {
                    text: world::naming::RENAME_REFUSED,
                    n: level,
                }),
            }
        }
        _ => {}
    }
}

/// Behind a name being edited. **Inferred** stand-in: the original edits
/// in the box's own picture.
const EDITING: Color32 = Color32::from_rgb(24, 20, 16);

/// Whether `chosen`'s panel draws control `id` itself, or not at all.
fn skipped(chosen: &Chosen, id: i32) -> bool {
    let common = [
        id::PORTRAIT,
        ids::TITLE,
        id::HP_BAR,
        id::HP_TEXT,
        id::LINE,
        id::DOING,
        details::BANNER_CONTROL,
        ids::ZOOM,
        ids::TRACK,
        id::TIPS[0],
        id::TIPS[1],
    ]
    .contains(&id);
    let own = match chosen.kind {
        Kind::Hero => [
            id::CLASS,
            id::HERO_LEVEL,
            id::XP_BAR,
            id::WEAPON,
            id::WEAPON_BONUS,
            id::ARMOUR,
            id::ARMOUR_BONUS,
            id::TAB_SPELLS,
            id::TAB_ITEMS,
            id::TAB_STATS,
            id::DESTROY,
            details::BANNER_CONTROL,
        ]
        .contains(&id),
        Kind::Building => {
            BAND_IDS.contains(&id)
                || (chosen.contests.is_some()
                    && (CONTEST_ICONS.contains(&id) || CONTESTS.contains(&id)))
                || (chosen.panel == Some(GAMBLE_PANEL) && GAMBLE_IDS.contains(&id))
                || (chosen.panel == Some(gear::FORGE_PANEL) && FORGE_IDS.contains(&id))
                || id::COUNTS.contains(&id)
                || id::PALACE_ICONS.contains(&id)
                || (chosen.panel == Some(OUTPOST_PANEL) && id == id::OUTPOST_HEROES)
                || Henchmen::ALL.iter().any(|h| h.control() == id)
                || [
                    id::LEVEL,
                    id::UPGRADE,
                    id::UPGRADE_PRICE,
                    id::TAX_ONCE,
                    id::TAX_ROUTE,
                    id::REPAIR_ONCE,
                    id::REPAIR_ROUTE,
                    id::HEROES,
                    id::VISITORS,
                    id::RESEARCH,
                    id::SECOND,
                    id::DESTROY,
                    id::PALACE_STATS,
                    id::PALACE_REWARDS,
                    id::PALACE_BUILD,
                    id::PALACE_ROSTER,
                    OPEN_EMBASSY,
                    CLOSE_EMBASSY,
                ]
                .contains(&id)
        }
        Kind::Purse => [
            id::CARRIED,
            id::PICKUP,
            id::RETURN,
            id::RETURN_DOWN,
            id::RETURN_UP,
            id::PICKUP_DOWN,
            id::PICKUP_UP,
        ]
        .contains(&id),
        Kind::Monster => RATED.iter().any(|(_, c)| *c == id),
        Kind::Flag(_) => [
            id::REWARD,
            id::RAISE,
            id::REMOVE,
            id::TARGET,
            id::VIEW_HEROES,
            id::SEEKERS,
        ]
        .contains(&id),
        Kind::Chest | Kind::Sign | Kind::Other => false,
    };

    common || own
}

/// The panel's ground, statics and portrait. The panel draws in list
/// order, and lists the portrait before its frame (`INBg` 1000, `INDe`
/// 1002), so the frame's rim lies over the picture.
fn base(canvas: &mut Canvas<'_>, layout: &Layout, chosen: &Chosen, backdrop: Option<Pic>) {
    let skip = |id: i32| skipped(chosen, id);
    let at = layout
        .controls
        .iter()
        .position(|c| c.id == Some(id::PORTRAIT))
        .unwrap_or(0);
    let (under, over) = layout.controls.split_at(at);

    sheet::ground(canvas, layout, backdrop);
    sheet::controls(canvas, layout, under, &skip);
    portrait(canvas, layout, chosen);
    sheet::controls(canvas, layout, over, &skip);
}

/// The selected piece's portrait in control 8011; none on a sign.
fn portrait(canvas: &mut Canvas<'_>, layout: &Layout, chosen: &Chosen) {
    if let (Some(r), false) = (
        layout.control(id::PORTRAIT).and_then(sheet::rect),
        sign(chosen),
    ) {
        let tone = if chosen.grave {
            Tone::Grave
        } else {
            Tone::Plain
        };

        canvas.picture(Pic::Portrait(chosen.code), tone, r.min);
    }
}

/// Whether `chosen` is a sign post.
const fn sign(chosen: &Chosen) -> bool {
    matches!(
        chosen.details,
        Some(Details::Posted(Posted {
            board: Board::Sign,
            ..
        }))
    )
}

/// Name, hit points and the line every panel may hold.
fn common(canvas: &mut Canvas<'_>, layout: &Layout, chosen: &Chosen, text: &Text) {
    let sign = sign(chosen);

    // `APb5`'s banner frames the portrait, in the owner's colour.
    let banner = match (&chosen.details, chosen.foreign) {
        (Some(Details::Hero(hero)), _) => Some(hero.banner()),
        (_, Some(foreign)) => Some(foreign.banner()),
        _ => None,
    };

    if let (Some((record, set)), Some(r)) = (
        banner,
        layout
            .control(details::BANNER_CONTROL)
            .and_then(sheet::rect),
    ) {
        canvas.picture(Pic::of(record, set), Tone::Plain, r.min);
    }

    // A flag's panel keeps its own words, "REWARD ATTACK FLAG".
    match (chosen.kind, layout.control(ids::TITLE)) {
        (Kind::Flag(_), Some(c)) => {
            if let (Some(r), Some(t)) = (sheet::rect(c), c.text.as_deref()) {
                sheet::label(canvas, c, r, t);
            }
        }
        _ => sheet::fill(
            canvas,
            layout,
            ids::TITLE,
            &chosen.name,
            (None, Align::Centre),
        ),
    }

    if let (Some(health), Some(c)) = (chosen.health, layout.control(id::HP_BAR)) {
        #[allow(clippy::cast_precision_loss, reason = "hit points are small")]
        let filled = if health.max > 0 {
            health.now as f32 / health.max as f32
        } else {
            0.0
        };
        // `INBc` 1010 when poisoned (`portrait::Bar`).
        let fill = match portrait::Bar::of(chosen.poison) {
            portrait::Bar::Poisoned => sheet::Fill::Poisoned,
            portrait::Bar::Plain => sheet::Fill::Plain,
        };

        sheet::art_bar(canvas, c, (filled, fill), HEALTH);

        if !chosen.grave
            && let Some(line) = health.text(text)
        {
            sheet::fill(canvas, layout, id::HP_TEXT, &line, (None, Align::Centre));
        }
    }

    let line = match &chosen.details {
        Some(Details::Coffer(coffer)) => coffer.text(text),
        Some(Details::Buried(count)) => details::buried(text, *count),
        Some(Details::Posted(posted)) => posted.text(text),
        _ => None,
    };

    match (line, sign) {
        (Some(line), true) => {
            if let Some(c) = layout.control(id::LINE)
                && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
            {
                #[allow(clippy::cast_precision_loss, reason = "a sign's few lines")]
                let tall = (canvas.pitch(font) * canvas.lines(font, r.width(), &line) as f32)
                    .min(r.height());
                let wrapped = Rect::from_center_size(r.center(), vec2(r.width(), tall));

                canvas.paragraphs(font, wrapped, &line, (sheet::colour(c), Align::Centre, 0.0));
            }
        }
        (Some(line), false) => sheet::fill(canvas, layout, id::LINE, &line, (None, Align::Centre)),
        (None, _) => {}
    }

    let doing = match &chosen.details {
        Some(Details::Hero(hero)) => hero
            .activity
            .and_then(|d| text.activity(d))
            .or_else(|| hero.leader(text).map(Into::into)),
        _ => chosen.foreign.and_then(|f| f.line(text).map(Into::into)),
    };

    if let Some(doing) = doing.filter(|_| !chosen.grave)
        && let Some(c) = layout.control(id::DOING)
        && let (Some(r), Some(font)) = (sheet::rect(c), c.font)
    {
        canvas.paragraphs(
            font,
            r,
            doing.trim(),
            (sheet::colour(c), Align::Centre, 0.0),
        );
    }
}

/// Button `c` with `tip` for its tooltip; whether it was pressed. One
/// press area: a second over it would take the click.
fn tipped(
    canvas: &mut Canvas<'_>,
    c: &ShownControl,
    label: Option<&str>,
    able: Able,
    tip: Option<String>,
) -> bool {
    let c = ShownControl {
        tooltip: tip,
        ..c.clone()
    };

    sheet::button(canvas, &c, label, able)
}

/// Control `c` with `price` after its tooltip, `GMTX` 28: a guild
/// action's.
fn priced(c: &ShownControl, text: &Text, price: i32) -> ShownControl {
    ShownControl {
        tooltip: c
            .tooltip
            .as_deref()
            .map(|own| tip::appended(text, own, tip::line::PRICE, &[price.into()])),
        ..c.clone()
    }
}

/// A paid press's inputs sent, or its refusal said.
fn press(orders: &mut orders::Panel, sent: Result<Vec<Input>, chat::Line>) {
    match sent {
        Ok(inputs) => orders.orders.outbox.extend(inputs),
        Err(line) => orders.orders.said.push(line),
    }
}

/// The seekers' count's box: `ui_attack_flag.png`'s grey (**Inferred**
/// stand-in for its `INTI` 1016).
const COUNT_BOX: Color32 = Color32::from_rgb(72, 72, 64);

#[cfg(test)]
mod tests {
    use bevy_egui::egui::{self, Event, Modifiers, PointerButton, RawInput, pos2};
    use regent_core::data::catalogue::Catalogue;
    use regent_core::data::smnu;
    use regent_core::sim::world::interface::Stride;
    use regent_core::view::panel::upper::{ABODE_PANEL, FAIR_PANEL};

    use super::*;
    use crate::menu::fit::Fit;
    use crate::panel::games;
    use crate::panel::ink::Ink;

    /// The panel's size, pixels.
    const PANEL: egui::Vec2 = egui::vec2(202.0, 245.0);

    /// Whether `draw` reports a press when the pointer clicks `at`,
    /// panel pixels, over three frames: move, press, release.
    fn clicks(at: Pos2, draw: impl FnMut(&mut Canvas<'_>) -> bool) -> bool {
        clicks_held(at, Modifiers::NONE, draw)
    }

    /// [`clicks`] with `modifiers` held throughout.
    fn clicks_held(
        at: Pos2,
        modifiers: Modifiers,
        mut draw: impl FnMut(&mut Canvas<'_>) -> bool,
    ) -> bool {
        let ctx = egui::Context::default();
        let catalogue: &'static Catalogue<'static> =
            Box::leak(Box::new(Catalogue::build(&[]).expect("an empty catalogue")));
        let mut ink = Ink::default();
        let mut art = regent_draw::art::Art::default();
        let window = Rect::from_min_size(Pos2::ZERO, PANEL);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        };
        let frames = [
            vec![Event::ModifiersChanged(modifiers), Event::PointerMoved(at)],
            vec![button(true)],
            vec![button(false)],
        ];
        let mut pressed = false;

        for events in frames {
            let input = RawInput {
                events,
                screen_rect: Some(window),
                ..RawInput::default()
            };

            let mut output = ctx.run_ui(input, |ui| {
                let mut canvas = Canvas {
                    ui,
                    ink: &mut ink,
                    art: &mut art,
                    source: regent_draw::art::ArtSource::of(&[]),
                    catalogue,
                    fit: Fit::of(window, PANEL),
                    salt: "test",
                };

                pressed |= draw(&mut canvas);
            });

            // No painter takes the font atlas here.
            output.textures_delta.clear();
        }

        pressed
    }

    /// `AP02`'s upgrade button.
    fn upgrade_button() -> ShownControl {
        ShownControl {
            id: Some(id::UPGRADE),
            rect: Some(smnu::Rect {
                x: 157,
                y: 89,
                width: 33,
                height: 22,
            }),
            ..ShownControl::default()
        }
    }

    /// A point inside [`upgrade_button`].
    const ON_UPGRADE: Pos2 = pos2(170.0, 100.0);

    #[test]
    fn a_button_takes_its_click() {
        let c = upgrade_button();

        assert!(clicks(ON_UPGRADE, |canvas| sheet::button(
            canvas,
            &c,
            None,
            Able::Enabled
        )));
    }

    /// Shift with the left button pans only on the map: over a panel the
    /// button takes the click, and the canvas reads the stride from it.
    #[test]
    fn a_shift_click_presses_a_panel_button_far() {
        let c = upgrade_button();
        let press = |modifiers| {
            let mut stride = None;
            let pressed = clicks_held(ON_UPGRADE, modifiers, |canvas| {
                stride = Some(canvas.stride());
                sheet::button(canvas, &c, None, Able::Enabled)
            });

            (pressed, stride)
        };

        assert_eq!(press(Modifiers::SHIFT), (true, Some(Stride::Far)));
        assert_eq!(press(Modifiers::NONE), (true, Some(Stride::One)));
    }

    #[test]
    fn a_tipped_button_takes_its_click() {
        let c = upgrade_button();
        let pressed = clicks(ON_UPGRADE, |canvas| {
            tipped(canvas, &c, None, Able::Enabled, Some("tip".to_owned()))
        });

        assert!(pressed, "the upgrade button lost its click");
    }

    /// HD's catalogue, once.
    pub(super) fn hd() -> &'static Catalogue<'static> {
        crate::testdata::hd()
    }

    /// The window rectangles `base` draws for `code` on panel `panel` at
    /// `scale`, in draw order, and the portrait's and panel's places.
    fn based(
        catalogue: &'static Catalogue<'static>,
        (panel, code): (&[u8; 4], &[u8; 4]),
        scale: f32,
    ) -> (Vec<Rect>, Rect, Rect) {
        based_as(catalogue, (panel, code, None), scale)
    }

    /// [`based`], a Fairgrounds' contests `contests`.
    fn based_as(
        catalogue: &'static Catalogue<'static>,
        (panel, code, contests): (&[u8; 4], &[u8; 4], Option<Contests>),
        scale: f32,
    ) -> (Vec<Rect>, Rect, Rect) {
        let text = Text::build(catalogue);
        let layout = regent_core::view::menu::layout(catalogue, &text, FourCc(*panel))
            .expect("a panel")
            .expect("parses");
        let chosen = Chosen {
            agent: 0,
            code: FourCc(*code),
            kind: super::Kind::Building,
            panel: Some(FourCc(*panel)),
            name: String::new(),
            title: String::new(),
            health: None,
            grave: false,
            poison: 0,
            details: None,
            foreign: None,
            owned: true,
            at: (0, 0),
            contests,
        };
        let fit = Fit {
            scale,
            origin: pos2(0.0, 182.0 * scale),
        };
        let ctx = egui::Context::default();
        let mut ink = Ink::default();
        let mut art = regent_draw::art::Art::default();

        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            let mut canvas = Canvas {
                ui,
                ink: &mut ink,
                art: &mut art,
                source: regent_draw::art::ArtSource::of(&[]),
                catalogue,
                fit,
                salt: "test",
            };

            base(&mut canvas, &layout, &chosen, None);
        });

        output.textures_delta.clear();

        let shapes = output
            .shapes
            .iter()
            .map(|s| s.shape.visual_bounding_rect())
            .filter(Rect::is_positive)
            .collect();
        let portrait = layout
            .control(id::PORTRAIT)
            .and_then(sheet::rect)
            .expect("a portrait");

        (shapes, fit.rect(portrait), fit.rect(sheet::bounds(&layout)))
    }

    /// The frame (`INBg` 1000 on a building, `INDe` 1002 on a hero) is
    /// listed after the portrait and drawn over it, its rounded corners
    /// on the picture (`ui_warriors_guild.png`).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_frame_lies_over_the_portrait() {
        let catalogue = hd();
        let near =
            |a: Rect, b: Rect| (a.min - b.min).length() < 1.0 && (a.max - b.max).length() < 1.0;

        for (panel, code) in [(b"AP52", b"ABV1"), (b"AP20", b"AVJ1")] {
            for scale in [1.0, 1.415, 2.0] {
                let (shapes, portrait, whole) = based(catalogue, (panel, code), scale);
                let drawn = shapes.iter().position(|r| near(*r, portrait));
                let frame = shapes.iter().rposition(|r| near(*r, whole));

                assert!(drawn.is_some() && frame.is_some(), "{shapes:?} at {scale}");
                assert!(drawn < frame, "portrait over the frame at {scale}");
            }
        }
    }

    /// Where `base` would draw controls `ids` of panel `panel` at scale
    /// 1, in window points.
    fn rects_of(catalogue: &'static Catalogue<'static>, panel: FourCc, ids: &[i32]) -> Vec<Rect> {
        let text = Text::build(catalogue);
        let layout = regent_core::view::menu::layout(catalogue, &text, panel)
            .expect("a panel")
            .expect("parses");
        let fit = Fit {
            scale: 1.0,
            origin: pos2(0.0, 182.0),
        };

        ids.iter()
            .filter_map(|&id| layout.control(id).and_then(sheet::rect))
            .map(|r| fit.rect(r))
            .collect()
    }

    /// A Gambling Hall's panel draws none of the controls its bet, spin
    /// and result share, which lie on each other (`APb8`: the results
    /// over the colours, MAKE NEW BET over the wager).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_gambling_hall_stacks_no_controls() {
        let catalogue = hd();
        let shared = rects_of(catalogue, GAMBLE_PANEL, &GAMBLE_IDS);
        let (shapes, _, _) = based(catalogue, (b"APb8", b"ABd1"), 1.0);

        assert_eq!(shared.len(), GAMBLE_IDS.len());

        for r in shared {
            assert!(
                !shapes.iter().any(|s| (s.min - r.min).length() < 1.0),
                "{r:?} drawn"
            );
        }
    }

    /// The bet's controls lie apart: each at its own place.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_bets_controls_lie_apart() {
        let catalogue = hd();
        let limits = regent_core::sim::world::guild::gamble::BetLimits {
            least: 25,
            most: 1000,
            step: 25,
        };
        let w = regent_core::sim::world::guild::gamble::Wager::of(25, 0, 5000, limits);
        let ids = games::bet_controls(&w);
        let rects = rects_of(catalogue, GAMBLE_PANEL, &ids);

        assert_eq!(rects.len(), 7);

        for (n, a) in rects.iter().enumerate() {
            for b in &rects[n + 1..] {
                assert!(!a.intersects(*b), "{a:?} on {b:?}");
            }
        }
    }

    /// 8851 opens the Hall of Champions' bounties, nothing on the
    /// Sorcerer's Abode, else research (each controller's method).
    #[test]
    fn place_bounty_opens_the_bounty_page() {
        assert_eq!(research_page(Some(HALL_PANEL)), Some(Page::Bounty));
        assert_eq!(research_page(Some(ABODE_PANEL)), None);
        assert_eq!(research_page(Some(FAIR_PANEL)), Some(Page::Research));
    }

    /// A Fairgrounds draws no contest picture or radio before the
    /// Tournament.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_contests_wait_for_the_tournament() {
        let catalogue = hd();
        let ids: Vec<i32> = CONTEST_ICONS.iter().chain(&CONTESTS).copied().collect();
        let contests = rects_of(catalogue, FAIR_PANEL, &ids);
        let (shapes, _, _) = based_as(catalogue, (b"AP08", b"ABD1", Some(Contests::Closed)), 1.0);

        assert_eq!(contests.len(), 10);

        for r in contests {
            assert!(
                !shapes.iter().any(|s| (s.min - r.min).length() < 1.0),
                "{r:?} drawn"
            );
        }

        assert!(!Contests::Closed.shows(1200) && Contests::Closed.shows(id::RESEARCH));
        assert!(Contests::Open.shows(1200) && !Contests::Open.shows(id::RESEARCH));
    }

    /// The outpost opens on its build list, as the palace does: both are
    /// palace panels.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_outpost_opens_on_its_build_list() {
        let sim = crate::game::begin::test_game(&[]).0.sim;
        let (world, program) = (sim.world(), sim.program());
        let palace = world
            .player_palace(regent_core::sim::world::CROWN)
            .expect("a palace");
        let first = |panel| {
            Chosen {
                agent: palace,
                code: FourCc(*b"ABJ1"),
                kind: super::Kind::Building,
                panel: Some(panel),
                name: String::new(),
                title: String::new(),
                health: None,
                grave: false,
                poison: 0,
                details: None,
                foreign: None,
                owned: true,
                at: (0, 0),
                contests: None,
            }
            .first_page(world, program, None)
        };

        for panel in PALACE_PANELS {
            assert_eq!(first(panel), Page::Build, "{panel}");
        }
    }
}
