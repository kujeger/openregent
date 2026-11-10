//! The tooltip line: what the bottom bar shows while the pointer rests on
//! a control, and the texts the controllers compose. **Binary** unless
//! marked.
//!
//! ```text
//!  CYTTipManager, a singleton made at start-up with font fnt7
//!        each interface pass:
//!     the pointer moved or a button down: its clock restarts
//!     no button down: each callback's phase 1 finds the control under
//!       the pointer (the panels' tip callback, the map's tip callback);
//!       none hides the line, another hides it and shows the new one
//!       at once; else the line shows once the pointer rests 1 ms (both
//!       callbacks answer 1 to phase 3; the
//!       manager's own wait, set when it is made, is 500 ms, for a
//!       callback answering 0, which no shipped one does)
//!   phase 2: the text, from the controller (control, list row, text):
//!     DialogController      the control's own (SMNU key 33), then an
//!     override appends to it or replaces it
//!   showing the tip (text, x0, y1 of APMK 2004, width x1 - x0):
//!     one line: raised RAISE px
//!     a window (tip window panel): the text in fnt7, palette TTIP, MARGIN
//!     px inside; the window puts its bottom at the point
//! ```
//!
//! ```text
//!   SPELLS │Cast Healing.- (Unavailable: requires Temple to Fervus) │ OPTIONS
//!           ^ 2004's left + MARGIN
//! ```
//!
//! # The map's unit line
//!
//! What the map's tooltip reads of the unit under the pointer:
//! Unit tip, from map tip callback.
//! **Binary** unless marked. The words are [`unit()`]'s.
//!
//! ```text
//!  phase 1: the view's pick (IsoGameView) at the pointer, mask
//!    0x30, reach 0: a piece met on its picture (0x20); not the build
//!    preview, no holder taken
//!  unit tip:      app state 3, no modal panel (SMenuManager)
//!    a standing building, a mobile or a message flag; not ABA0, XL00
//!    named: a see-through overlay's holder, a clone's unit (APU:8)
//!    hit points: not a building or character without HasHPBar, not AVk
//!      APU:0 held: GMTX 3 " (%d of %d hp)", with the player's own
//!      gold GMTX 5 " (%d of %d hp, %d gold)"
//!    no hit points: the player's own gold, GMTX 4 " (%d gold)"
//!      gold: APU:2 where HasGoldToolTip, above 0
//!    the player's own standing mobile with APV:30:
//!      GMTX 6 " is " and AITX[APV:30]
//! ```

use std::borrow::Cow;

use crate::content::unit::Unit;
use crate::data::attrib::Key;
use crate::data::catalogue::Catalogue;
use crate::data::desc::{BuildingFlag, CharacterFlag, UnitKind};
use crate::data::fourcc::FourCc;
use crate::data::payload::Rgb;
use crate::data::stat::Stat;
use crate::data::text::{Arg, Text, printf};
use crate::sim::clearing::RESOURCES;
use crate::sim::world::trail::CLONE_PARENT;
use crate::sim::world::{Piece, World, flag::MESSAGE_FLAG, guild::taxman::Step, piece_state};
use crate::view::menu::Tick;
use crate::view::panel::portrait;

pub use crate::sim::world::offer::{Cap, Rules, SKILLS, Skill, Spell, Upgrade};

/// A button's own tooltip, unchanged, then what Shift does to it: e.g.
/// "... Hold shift to increase by 500." A departure: the original has no
/// Shift press.
#[must_use]
pub fn shift_hint(own: Option<&str>, step: Step, amount: i32) -> String {
    let verb = match step {
        Step::Raise => "increase",
        Step::Lower => "decrease",
    };
    let hint = format!("Hold shift to {verb} by {amount}.");

    own.map_or_else(|| hint.clone(), |own| format!("{own} {hint}"))
}

/// The `APMK` control whose rectangle places the line.
pub const PLACE: i32 = 2004;

/// The line's font: `CYTTipManager`'s, `fnt7` (`main`).
pub const FONT: FourCc = FourCc(*b"fnt7");

/// The window's palette, key 11.
pub const PALETTE: FourCc = FourCc(*b"TTIP");

/// The text's colour. **Game**: every pixel of the three spell captures'
/// text (`ui_tooltip_spell_*.png`), at the screen's 565. **Inferred**:
/// `TTIP`'s role 3 through the style; the path is not read.
pub const COLOUR: Rgb = Rgb {
    r: 216,
    g: 152,
    b: 72,
};

/// Pixels a line of one row is raised.
pub const RAISE: i32 = 3;

/// Pixels between the window's edge and its text (tip window panel: the
/// window is the text plus 8, the text at 4, 4).
pub const MARGIN: i32 = 4;

/// The window's flag bits from the hovered control's [`FLAGS`]: `>> 11`,
/// four bits. Bit 2 keeps the window below the point,
/// bit 4 puts it left of it. **Corpus**: no shipped
/// control sets them (`tests/tip.rs`).
///
/// [`FLAGS`]: crate::data::smnu::FLAGS
pub const WINDOW_FLAGS: i32 = 0xf << 11;

/// How many rows the text took.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rows {
    One,
    More,
}

/// The text's top, pixels: `bottom` is 2004's lower edge, `height` the
/// text's (rows times the font's line).
#[must_use]
pub const fn top(bottom: i32, height: i32, rows: Rows) -> i32 {
    let point = match rows {
        Rows::One => bottom - RAISE,
        Rows::More => bottom,
    };
    // The window, the text and a margin round it, ends at the point.
    let window = point - (height + 2 * MARGIN);

    window + MARGIN
}

/// Whether the pointer moved this pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    Moved,
    /// Still: at a pass apart, past the rest the line waits for.
    Rested,
}

/// Whether a mouse button is down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Buttons {
    Up,
    Down,
}

/// The line's target, `CYTTipManager`'s a field and a field to a field:
/// what it shows for, by any key a front end tells controls apart by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line<T> {
    shown: Option<T>,
}

impl<T> Default for Line<T> {
    fn default() -> Self {
        Line { shown: None }
    }
}

impl<T: PartialEq> Line<T> {
    /// One pass of `CYTTipManager` with `under` the
    /// control under the pointer that has a text: whether the line shows
    /// for it. A button down shows nothing new and keeps the line only
    /// over its own control; else none hides it, another moves it at once
    /// while it shows, and it first shows once the pointer rests.
    pub fn step(&mut self, under: Option<T>, pointer: Pointer, buttons: Buttons) -> bool {
        let same = under.is_some() && under == self.shown;

        self.shown = match (buttons, pointer) {
            _ if same => under,
            (Buttons::Up, _) if self.shown.is_some() => under,
            (Buttons::Up, Pointer::Rested) => under,
            (Buttons::Down, _) | (Buttons::Up, Pointer::Moved) => None,
        };

        self.shown.is_some()
    }
}

/// A count as a format argument.
fn count(n: usize) -> Arg<'static> {
    Arg::Int(i64::try_from(n).unwrap_or(i64::MAX))
}

/// The `GMTX` lines the controllers compose with.
pub mod line {
    /// `- (%d) gold`: a recruit's price.
    pub const RECRUIT_PRICE: usize = 0;
    /// A map unit's numbers: ` (%d of %d hp)`,
    /// ` (%d gold)`, ` (%d of %d hp, %d gold)`; then ` is ` and what it
    /// does.
    pub const HEALTH: usize = 3;
    pub const GOLD: usize = 4;
    pub const HEALTH_GOLD: usize = 5;
    pub const DOING: usize = 6;
    /// `(%d of %d)`: the hero a recruit would be.
    pub const RECRUIT_COUNT: usize = 9;
    /// `Upgrade to `, then the upgrade's description.
    pub const UPGRADE_TO: usize = 26;
    /// `- (%d) gold`: the upgrade's price.
    pub const UPGRADE_PRICE: usize = 27;
    /// `- (%d) gold`: a guild action's price (Call to Arms, the Rage,
    /// the camp move).
    pub const PRICE: usize = 28;
    /// `- (%d) gold`: a spell's.
    pub const SPELL_PRICE: usize = 29;
    /// `- (%d) gold`: the Wizards' Tower's 5001.
    pub const TOWER_PRICE: usize = 75;
    /// `- (Unavailable: requires %s)`.
    pub const UNAVAILABLE: usize = 181;
    /// `Research Complete`: a researcher's page button disabled.
    pub const RESEARCH_DONE: usize = 188;
    /// `- Unavailable: Already in effect`.
    pub const IN_EFFECT: usize = 190;
    /// The upgrade's refusals, upgrade tooltip.
    pub const NO_UPGRADE: usize = 238;
    pub const FULLY_UPGRADED: usize = 239;
    pub const UPGRADING: usize = 240;
    pub const RESEARCH_FIRST: usize = 241;
    /// `You have only %d Heroes of %d needed.`
    pub const HEROES_SHORT: usize = 242;
    /// `Building needs to be repaired first.`
    pub const REPAIR_FIRST: usize = 243;
    /// `Your highest member is level %d , you need one of %d to upgrade.`
    pub const LEVEL_SHORT: usize = 244;
    pub const UPGRADE_BARRED: usize = 398;
    /// `No members to Call to Arms.`
    pub const NO_CALL: usize = 260;
    /// `No visitors present`, `The guild is full`, `No members present`,
    /// `No buildings available`: disabled controls.
    pub const NO_VISITORS: usize = 269;
    pub const GUILD_FULL: usize = 270;
    pub const NO_MEMBERS: usize = 271;
    pub const NO_BUILDINGS: usize = 272;
    /// `No heroes interested`: the reward page's 8019 and 8995 disabled.
    pub const NO_SEEKERS: usize = 276;
    pub const NO_VIEWERS: usize = 277;
    /// `Collect %d gold taxes instantly, for a cost of %d gold.`
    pub const EXTORTION: usize = 327;
    /// `Detailed Information`, `Simple List`: a list's detail toggle.
    pub const DETAILED: usize = 343;
    pub const SIMPLE: usize = 344;
    /// `This hero's current %s skill - There is a cap of %d on this
    /// skill.`, and the magic resistance's `... In deathmatch play all
    /// heroes have a minimum of %d in this skill.`
    pub const SKILL_CAP: usize = 380;
    pub const SKILL_FLOOR: usize = 381;
}

/// The mini views' button rows, mini rows: the main screen's, then
/// three more, ten controls each. `APMK` holds the
/// first two. `MainFrameController` gives a control the text of
/// the first row's control in its column.
pub const MINI_ROWS: [[i32; 10]; 4] = [
    [4466, 4467, 1015, 1013, 1021, 1014, 1012, 1016, 1009, 1040],
    [9111, 9112, 9113, 9114, 9115, 9116, 9117, 9118, 9119, 9120],
    [9121, 9122, 9123, 9124, 9125, 9126, 9127, 9128, 9129, 9130],
    [9131, 9132, 9133, 9134, 9135, 9136, 9137, 9138, 9139, 9140],
];

/// The columns that say "All filters are off" while disabled, each its
/// line: back, cycle and forward (`MainFrameController`'s cases).
pub const FILTERS_OFF: [(usize, usize); 3] = [(6, 275), (7, 273), (8, 274)];

/// The column whose text takes [`MINI_HINT`].
pub const HINTED: usize = 9;

/// What column [`HINTED`] appends.
pub const MINI_HINT: &str =
    "  Hold the control key and click to put the selected item into the mini view.";

pub use crate::view::menu::Able;

/// A mini view row's column of `control`.
#[must_use]
pub fn mini_column(control: i32) -> Option<usize> {
    MINI_ROWS
        .iter()
        .find_map(|row| row.iter().position(|c| *c == control))
}

/// A mini view row's control's line (`MainFrameController`,
/// `MainFrameController`): "All filters are off" for columns 6 to 8 disabled,
/// else `own` of the first row's control in its column, column 9 with
/// [`MINI_HINT`]. `None` for another control.
#[must_use]
pub fn mini(
    text: &Text,
    control: i32,
    able: Able,
    own: impl Fn(i32) -> Option<String>,
) -> Option<String> {
    let column = mini_column(control)?;
    let off = FILTERS_OFF
        .iter()
        .find(|(c, _)| *c == column)
        .map(|&(_, line)| line)
        .filter(|_| able == Able::Disabled);

    let hint = if column == HINTED { MINI_HINT } else { "" };

    off.map_or_else(
        || own(MINI_ROWS[0][column]).map(|shown| format!("{shown}{hint}")),
        |line| said(text, line),
    )
}

/// A hero's skill row's line, replacing its own:
/// its name and `cap`, "This hero's
/// current parry skill - There is a cap of 95 on this skill."
#[must_use]
pub fn skill(text: &Text, skill: &Skill, cap: i32) -> Option<String> {
    let format = match skill.cap {
        Cap::ToHit | Cap::ToAvoid => line::SKILL_CAP,
        Cap::MagicResist => line::SKILL_FLOOR,
    };
    let name = text.gmtx(skill.name)?;

    text.gmtx(format)
        .map(|f| printf(&f, &[name.as_ref().into(), cap.into()]))
}

/// The Wizards' Tower's enchant button's line:
/// `own` and `price`, `#WizTowerUpgradeCost`, enabled or not.
#[must_use]
pub fn tower(text: &Text, own: &str, price: i32) -> String {
    appended(text, own, line::TOWER_PRICE, &[price.into()])
}

/// A map unit's line: `name`, then what [`Told`]
/// counts and does. **Game** wanted: no capture shows one.
#[must_use]
pub fn unit(text: &Text, name: &str, told: &Told) -> String {
    let counted = match told.count {
        Count::None => name.to_owned(),
        Count::Health { now, max } => appended(text, name, line::HEALTH, &[now.into(), max.into()]),
        Count::Both { now, max, gold } => appended(
            text,
            name,
            line::HEALTH_GOLD,
            &[now.into(), max.into(), gold.into()],
        ),
        Count::Gold(gold) => appended(text, name, line::GOLD, &[gold.into()]),
    };

    // Unit doing text: an `AITX` line, or nothing. **Inferred**: a code
    // past the table says nothing (its fallback, text override find code, unread).
    let doing = told
        .doing
        .and_then(|d| text.activity(d))
        .zip(text.gmtx(line::DOING));

    match doing {
        Some((doing, is)) => format!("{counted}{is}{doing}"),
        None => counted,
    }
}

/// The map's line for piece `agent`, `local`
/// the player at the screen: named as a unit list row
/// ([`portrait::listed_name`], panel name with flags 0).
#[must_use]
pub fn map_unit(
    catalogue: &Catalogue<'_>,
    text: &Text,
    world: &World,
    (agent, local): (u32, u32),
) -> Option<String> {
    let told = told(world, catalogue, agent, local)?;
    let who = portrait::who(world, told.named, told.code);
    let name = portrait::dropped_name(catalogue, text, world, told.named)
        .unwrap_or_else(|| portrait::listed_name(catalogue, text, &who));

    Some(unit(text, &name, &told))
}

/// The map's line for a message flag: its descriptor's title, "Message
/// flag" (panel name: `GSUnit` names none). It has no
/// hit points, gold or doing.
#[must_use]
pub fn message_flag(catalogue: &Catalogue<'_>) -> Option<String> {
    let unit = Unit::find(catalogue, MESSAGE_FLAG)?.ok()?;

    Some(unit.desc.head.title().into_owned())
}

/// `Base_BuildingController`'s checks, each by its tick: the tax
/// and repair settings, (control, clear, ticked).
pub const TOGGLES: [(i32, usize, usize); 4] = [
    (8024, 254, 255),
    (8025, 252, 253),
    (8026, 258, 259),
    (8027, 256, 257),
];

/// `Base_BuildingController`'s buttons disabled, each the line it
/// says instead: the members page (8004), a recruit (8008), the visitors
/// page (8021).
pub const BARRED: [(i32, usize); 3] = [
    (8004, line::NO_MEMBERS),
    (8008, line::GUILD_FULL),
    (8021, line::NO_VISITORS),
];

/// A check's line by its tick ([`TOGGLES`]).
#[must_use]
pub fn toggle(control: i32, tick: Tick) -> Option<usize> {
    TOGGLES
        .iter()
        .find(|(c, ..)| *c == control)
        .map(|&(_, clear, ticked)| match tick {
            Tick::Checked => ticked,
            Tick::Clear => clear,
        })
}

/// A disabled button's line ([`BARRED`]).
#[must_use]
pub fn barred(control: i32) -> Option<usize> {
    BARRED
        .iter()
        .find(|(c, _)| *c == control)
        .map(|&(_, line)| line)
}

/// `GMTX` `line` as it stands.
#[must_use]
pub fn said(text: &Text, line: usize) -> Option<String> {
    text.gmtx(line).map(Cow::into_owned)
}

/// `own` with `GMTX` `line` formatted after it (`cystring_format_append`,
/// cystring format append): "Cast Farseeing." and 29 of 200, "Cast Farseeing.-
/// (200) gold". No space between: the tables' own.
#[must_use]
pub fn appended(text: &Text, own: &str, line: usize, args: &[Arg<'_>]) -> String {
    let more = text
        .gmtx(line)
        .map_or_else(String::new, |f| printf(&f, args));

    format!("{own}{more}")
}

/// A spell button's line: `own` and what [`Spell`] adds. **Game**:
/// "Cast Healing.- (Unavailable: requires Temple to Fervus)".
#[must_use]
pub fn spell(text: &Text, own: &str, spell: Spell<&str>) -> String {
    match spell {
        Spell::Ready(0) => own.to_owned(),
        Spell::Ready(price) => appended(text, own, line::SPELL_PRICE, &[price.into()]),
        Spell::InEffect => appended(text, own, line::IN_EFFECT, &[]),
        Spell::Needs(name) => appended(text, own, line::UNAVAILABLE, &[name.into()]),
    }
}

/// The upgrade button's line, replacing its own: "Upgrade to Palace
/// Level 2- (2000) gold".
#[must_use]
pub fn upgrade(text: &Text, why: Upgrade<&str>) -> Option<String> {
    let format = |line: usize, args: &[Arg<'_>]| text.gmtx(line).map(|f| printf(&f, args));

    match why {
        Upgrade::Damaged => said(text, line::REPAIR_FIRST),
        Upgrade::Building => said(text, line::UPGRADING),
        Upgrade::Full => said(text, line::FULLY_UPGRADED),
        Upgrade::None => said(text, line::NO_UPGRADE),
        Upgrade::Capped => said(text, line::UPGRADE_BARRED),
        Upgrade::Research => said(text, line::RESEARCH_FIRST),
        Upgrade::Heroes { have, need } => format(line::HEROES_SHORT, &[count(have), need.into()]),
        Upgrade::Level { best, need } => format(line::LEVEL_SHORT, &[best.into(), need.into()]),
        Upgrade::Offer { to, price } => {
            let head = said(text, line::UPGRADE_TO)?;

            Some(appended(
                text,
                &format!("{head}{to}"),
                line::UPGRADE_PRICE,
                &[price.into()],
            ))
        }
    }
}

/// A recruit button's line:
/// `own`, the hero it would be of the guild's spots, its price.
#[must_use]
pub fn recruit(text: &Text, own: &str, (living, spots): (usize, i32), price: i32) -> String {
    let counted = appended(
        text,
        own,
        line::RECRUIT_COUNT,
        &[count(living + 1), spots.into()],
    );

    appended(text, &counted, line::RECRUIT_PRICE, &[price.into()])
}

/// The Rogues' Guild's extortion button's line, replacing its own:
/// what the guild collects less its cut,
/// and the cut.
#[must_use]
pub fn extortion(text: &Text, (total, cut): (i32, i32)) -> Option<String> {
    text.gmtx(line::EXTORTION)
        .map(|f| printf(&f, &[(total - cut).into(), cut.into()]))
}

/// Codes `GSUnit` turns away: `placeholder_building`
/// and `placeholder_spell`.
pub const PLACEHOLDERS: [FourCc; 2] = [FourCc(*b"ABA0"), FourCc(*b"XL00")];

/// The numbers after the name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Count {
    None,
    /// `APU:0` raised to 0, of `APU:1` (the hit points where unset).
    Health {
        now: i32,
        max: i32,
    },
    /// [`Count::Health`] and the player's own gold.
    Both {
        now: i32,
        max: i32,
        gold: i32,
    },
    /// No hit points shown: the player's own gold.
    Gold(i32),
}

/// What the tooltip says of a unit, but its words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Told {
    /// The piece named, by agent: the one pointed at, or a see-through
    /// overlay's holder, or a clone's unit.
    pub named: u32,
    pub code: FourCc,
    pub count: Count,
    /// `APV:30`, an `AITX` code: what the player's own mobile is doing.
    pub doing: Option<i32>,
}

/// What the map's tooltip says of piece `agent`, `local` the player
/// at the screen; `None` where it says nothing.
///
/// **Inferred**: a mobile is a character piece. Projectiles fly as
/// missiles, not pieces; the effectors, made as mobiles (`GenUnit`
/// passes kind 1), are not pieces either.
#[must_use]
pub fn told(world: &World, catalogue: &Catalogue<'_>, agent: u32, local: u32) -> Option<Told> {
    let code = world.piece(agent)?.code;
    let unit = Unit::find(catalogue, code)?.ok()?;
    let passes = match unit.kind {
        Some(UnitKind::Building) => upright(world, agent),
        Some(UnitKind::Character) => true,
        _ => false,
    };

    if passes && !PLACEHOLDERS.contains(&code) {
        let named = named_for(world, &unit, agent);

        told_of(world, catalogue, named, local)
    } else {
        None
    }
}

/// The piece unit tip names for `agent`: a see-through
/// overlay's holder, else a clone's unit by
/// `APU:8`, else itself.
fn named_for(world: &World, unit: &Unit<'_>, agent: u32) -> u32 {
    let piece = world.piece(agent);
    let holder = piece.and_then(|p| p.place).map_or(0, |at| at.holder);
    let clone = piece.map_or(0, |p| p.word(CLONE_PARENT)).cast_unsigned();

    match (holder, clone) {
        (h, _) if h != 0 && unit.desc.transparent_to_mouse() => world.by_handle(h).unwrap_or(agent),
        (_, c) if c != 0 => world.by_handle(c).unwrap_or(agent),
        _ => agent,
    }
}

/// Whether piece `id` stands: not a ruin, nor lying dead
/// (`GenUnit` under 3). Where the world draws it, as drawn;
/// else as its save has it.
fn upright(world: &World, id: u32) -> bool {
    let saved = || {
        world
            .piece(id)
            .and_then(|p| p.place)
            .is_none_or(|at| at.state & piece_state::RUINED == 0)
    };

    world.shown_one(id).map_or_else(saved, |s| !s.ruined)
}

/// [`told`] of the piece named.
fn told_of(world: &World, catalogue: &Catalogue<'_>, named: u32, local: u32) -> Option<Told> {
    let piece = world.piece(named)?;
    let code = piece.code;
    let unit = Unit::find(catalogue, code)?.ok()?;
    let building = unit.desc.building();
    let character = unit.desc.character();
    let own = piece.owner == local;
    let standing = upright(world, named);

    // `BuildingDesc` bit 3, `VehicleDesc` bit 2.
    let health = match (&building, &character) {
        (Some(b), _) => b.has(BuildingFlag::HasHpBar),
        (None, Some(c)) => c.has(CharacterFlag::HasHpBar) && !RESOURCES.contains(&code),
        (None, None) => true,
    };

    // Bit 8 of either, `HasGoldToolTip`.
    let tipped = building.is_some_and(|b| b.has(BuildingFlag::HasGoldToolTip))
        || character.is_some_and(|c| c.has(CharacterFlag::HasGoldToolTip));
    let gold = if tipped { stat(piece, Stat::Gold) } else { 0 };
    let gold = Some(gold).filter(|g| *g > 0 && own);

    let hp = piece.get(key(Stat::HitPoints)).map(|hp| hp.max(0));
    let count = match (health, hp, gold) {
        (true, Some(now), None) => Count::Health {
            now,
            max: piece.get(key(Stat::MaxHitPoints)).unwrap_or(now),
        },
        (true, Some(now), Some(gold)) => Count::Both {
            now,
            max: piece.get(key(Stat::MaxHitPoints)).unwrap_or(now),
            gold,
        },
        (false, _, Some(gold)) => Count::Gold(gold),
        _ => Count::None,
    };

    // The doing: a mobile of the player's, standing.
    let mobile = unit.kind == Some(UnitKind::Character);
    let doing = piece
        .get(key(Stat::Activity))
        .filter(|_| mobile && standing && own);

    Some(Told {
        named,
        code,
        count,
        doing,
    })
}

/// A stat's attribute key.
fn key(stat: Stat) -> Key {
    let (bank, index) = stat.key();

    Key::new(*bank, index)
}

/// A stat, 0 where unset (attr get with 0).
fn stat(piece: &Piece, stat: Stat) -> i32 {
    piece.word(key(stat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shift_hint_follows_the_buttons_own_text() {
        assert_eq!(
            shift_hint(Some("Raise it."), Step::Raise, 500),
            "Raise it. Hold shift to increase by 500."
        );
        assert_eq!(
            shift_hint(None, Step::Lower, 100),
            "Hold shift to decrease by 100."
        );
    }

    #[test]
    fn a_line_of_one_row_stands_raised_above_the_places_bottom() {
        // 2004 is 1173 + 26 at 1920 × 1200; fnt7's row 12 (**Inferred**).
        assert_eq!(top(1199, 12, Rows::One), 1199 - 3 - 12 - 4);
        assert_eq!(top(1199, 24, Rows::More), 1199 - 24 - 4);
    }

    #[test]
    fn the_line_waits_for_a_rest_then_follows_the_pointer_at_once() {
        let mut line = Line::default();

        assert!(!line.step(Some(1), Pointer::Moved, Buttons::Up));
        assert!(line.step(Some(1), Pointer::Rested, Buttons::Up));
        assert!(line.step(Some(1), Pointer::Moved, Buttons::Up));
        assert!(line.step(Some(2), Pointer::Moved, Buttons::Up));
        assert!(!line.step(None, Pointer::Moved, Buttons::Up));
        assert!(!line.step(Some(2), Pointer::Moved, Buttons::Up));
        assert!(!line.step(Some(2), Pointer::Rested, Buttons::Down));
        assert!(line.step(Some(2), Pointer::Rested, Buttons::Up));
        assert!(line.step(Some(2), Pointer::Moved, Buttons::Down));
        assert!(!line.step(Some(3), Pointer::Moved, Buttons::Down));
    }

    #[test]
    fn the_checks_say_what_a_press_would_do() {
        assert_eq!(toggle(8024, Tick::Clear), Some(254));
        assert_eq!(toggle(8024, Tick::Checked), Some(255));
        assert_eq!(toggle(8001, Tick::Clear), None);
        assert_eq!(barred(8021), Some(line::NO_VISITORS));
    }
}
