//! What the selection panel shows for a unit. **Binary** unless marked.
//!
//! Every selection panel is a `StatsWindowController`: the hero, building,
//! guild and list controllers derive from it, and its fill
//! method sets the controls they share. Control ids are `SMNU` key 6.
//!
//! ```text
//!  8011  100x100  the unit's IMAG, set 1000
//!  8006  bar      INBc set 1000, 1010 when poisoned
//!  8010  text     GMTX 7, "%d of %d hp": APU:0 not under 0, APU:1
//!  8005  name     StatsWindowController
//! ```
//!
//! The hero panel (`HeroController`, layout `AP20`) also
//! puts the unit's set 1001, 200x245 stonework, under the whole panel, and
//! its class into control 7002: the `HEAD` title of `APU:6` `OriginalType`'s
//! descriptor when set, else its own.
//!
//! With nothing selected the panel's slot, control 2005 of `APMK`, holds a
//! [`Crown`].
//!
//! Every picture here is interface art: flatten it with
//! [`Blit::Interface`](crate::data::imag::Blit::Interface).
//!
//! What the panels say beyond hit points — level, gold, gear, spells,
//! items — is [`Details`].

pub mod details;
pub mod enchant;
pub mod gear;

pub use details::Details;

use crate::data::attrib;
use crate::data::attrib::Key;
use crate::data::desc;
use crate::data::error::Result;
use crate::data::fourcc::FourCc;
use crate::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT};
use crate::sim::world::kind;

use crate::content::imagery::{Animation, FrameSource};
use crate::content::unit::Unit;
use crate::data::catalogue::Catalogue;
use crate::data::text::{Ref, Text, printf};
use crate::sim::world::{self, World, chat::Who};
use crate::view::sprite::Sprite;

/// The picture with nothing selected, 202x245: a crown under the word
/// Majesty. Two exist; `APMK`'s control 2005 names one.
///
/// **Corpus**: 12 of HD's 13 `UIData_<w>_<h>.dat`, which HD opens for its
/// screen size, hold an `APMK`; each
/// names [`Crown::Gold`]. `UIData_800_600.dat` holds none, so that size
/// takes the dataset's: the base game's `textdata.cam` names
/// [`Crown::Gold`], the expansion's `mx_textdata.cam` [`Crown::Expansion`].
/// **Inferred**: a `UIData` panel shadows the dataset's.
///
/// **Binary**: `MainFrameController` forces [`Crown::Expansion`]
/// only while byte hd flag is clear; it is 1 in HD's
/// image and nothing writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crown {
    /// `INDe` set 1000, `interfacedata.cam`: gold, on stone.
    Gold,
    /// `IX72` set 1001, `mx_interfacedata.cam`: purple, on fur.
    Expansion,
}

impl Crown {
    /// The `IMAG` record and set.
    #[must_use]
    pub const fn art(self) -> (FourCc, u16) {
        match self {
            Crown::Gold => (FourCc(*b"INDe"), 1000),
            Crown::Expansion => (FourCc(*b"IX72"), 1001),
        }
    }
}

/// The main screen's selection panel, `APMK` control 2005 (`0x7d5` in
/// `MainFrameController`): a [`Crown`] until a unit is selected.
pub const PANEL_CONTROL: i32 = 2005;

/// The hit point bar's pictures, `INBc` (`AP20` control 8006).
pub const BAR: FourCc = FourCc(*b"INBc");

/// The hit point text's control: `0x1f4a` in panel health.
pub const HEALTH_CONTROL: i32 = 8010;

/// The hit point text: `GMTX` string 7, `%d of %d hp`.
pub const HEALTH_TEXT: Ref = Ref::gmtx(7);

/// The selected unit's portrait, 100x100: set 1000 of its own `IMAG`.
///
/// Panel portrait gives control 8011 the record the
/// unit draws and set 1000. **Inferred**: that
/// record is the descriptor's `ImageIDBase`. A unit without the set shows
/// nothing there: the widget finds no set.
pub fn portrait<'a>(catalogue: &Catalogue<'a>, unit: &Unit<'a>) -> Option<Result<Sprite>> {
    picture(catalogue, unit.interface(INTERFACE_PORTRAIT)?)
}

/// The hero panel's backdrop: set 1001 of the unit's `IMAG`, 200 wide and
/// 244 or 245 high. Only `HeroController` asks for it.
pub fn backdrop<'a>(catalogue: &Catalogue<'a>, unit: &Unit<'a>) -> Option<Result<Sprite>> {
    picture(catalogue, unit.interface(INTERFACE_PANEL)?)
}

/// The picture a unit's list icon (set 1002, 25x25) shrinks: a
/// character's portrait, whole frame; none for a building, whose icon is
/// a drawing of its own. Lets a list draw the icon sharper than 25 pixels.
///
/// **Corpus**: every character's icon is its portrait box-shrunk four
/// times (correlation 0.70 to 1.00); no building's is (0.20 at most).
/// `a_characters_list_icon_is_its_portrait_shrunk`.
pub fn icon_source<'a>(catalogue: &Catalogue<'a>, unit: &Unit<'a>) -> Option<Result<Sprite>> {
    let shrunk =
        unit.kind == Some(desc::UnitKind::Character) && unit.interface(INTERFACE_ICON).is_some();

    if shrunk {
        portrait(catalogue, unit)
    } else {
        None
    }
}

/// A [`Crown`]'s picture.
pub fn empty(catalogue: &Catalogue<'_>, crown: Crown) -> Option<Result<Sprite>> {
    let (record, id) = crown.art();

    Sprite::in_set(catalogue, record, id.into(), (0, 0))
}

/// Frame 0 of slot 0, the one picture an interface set holds.
fn picture(catalogue: &Catalogue<'_>, set: &Animation) -> Option<Result<Sprite>> {
    match set.poses.first()?.frames.first()? {
        FrameSource::Tile { archive, index, .. } => Sprite::at(catalogue, *archive, *index),
        _ => None,
    }
}

/// The panel's hit points: `APU:0` and `APU:1` as panel health
/// reads them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Health {
    /// `APU:0`, raised to 0 when under it.
    pub now: i32,
    /// `APU:1`, `MaxHP`.
    pub max: i32,
}

impl Health {
    #[must_use]
    pub const fn of(hp: i32, max: i32) -> Self {
        Health {
            now: if hp < 0 { 0 } else { hp },
            max,
        }
    }

    /// `51 of 74 hp`: [`HEALTH_TEXT`] with both numbers.
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        Some(printf(
            &text.get(HEALTH_TEXT)?,
            &[self.now.into(), self.max.into()],
        ))
    }
}

/// A descriptor's own hit-point maximum, its character's or building's
/// `MaxHP`: what a piece without `APU:1` shows as its most.
#[must_use]
pub fn most_hp(desc: &desc::Unit<'_>) -> Option<i32> {
    let most = desc
        .character()
        .map_or_else(|| desc.building()?.max_hp(), |c| c.max_hp());

    most.and_then(|m| i32::try_from(m).ok())
}

/// Whether the unit's panel shows hit points. Panel health fills
/// controls 8006 and 8010 for every panel, and a panel without them
/// shows none: a sign's or chest's (`APa7`, `APa6`), a flag's (`AP46`,
/// `AP80`); **Game** (the user): signs, flags and chests show none. A
/// unit naming no panel: true.
pub fn shows_health(catalogue: &Catalogue<'_>, unit: &Unit<'_>) -> bool {
    details::dialog(&unit.desc).is_none_or(|id| details::holds(catalogue, id, HEALTH_CONTROL))
}

/// Which bar picture the hit points draw on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    Plain,
    /// `APV:27` `PoisonCounter` above 0.
    Poisoned,
}

impl Bar {
    #[must_use]
    pub const fn of(poison: i32) -> Self {
        if poison > 0 {
            Bar::Poisoned
        } else {
            Bar::Plain
        }
    }

    /// The set of [`BAR`].
    #[must_use]
    pub const fn set(self) -> u16 {
        match self {
            Bar::Plain => 1000,
            Bar::Poisoned => 1010,
        }
    }
}

/// The panel's name line, as panel name writes it
/// for `StatsWindowController`: `named`, the unit's own name (its
/// method: `AP:1` spelled by [`crate::sim::world::naming::spelled`]), else its
/// descriptor's `HEAD` title. A building with no name takes the title of
/// the building its `UpgradeFrom` chain starts at, at most two steps up,
/// in capitals.
///
/// A title is first looked up in the XML strings, before the capitals:
/// a mod's `Description`
/// may be a text id.
///
/// **Inferred**: capitals by ASCII, as `toupper` in the C locale.
///
/// # Errors
///
/// If a descriptor on the chain will not parse.
pub fn title(
    catalogue: &Catalogue<'_>,
    text: &Text<'_>,
    unit: &desc::Unit<'_>,
    named: Option<&str>,
) -> Result<String> {
    let strung = |title: String| text.strings().of(&title).map_or(title, str::to_owned);

    match (named.filter(|n| !n.is_empty()), unit.building()) {
        (Some(name), _) => Ok(name.to_owned()),
        (None, Some(_)) => Ok(strung(root(catalogue, unit)?).to_ascii_uppercase()),
        (None, None) => Ok(strung(unit.head.title().into_owned())),
    }
}

/// A unit's own name, method: what `SpecifyName` gave, else `AP:1`
/// spelled, else its number, "Wizards Tower #2"; none, empty.
///
/// A descriptor that will not parse names nothing: a chat line is shown
/// whatever its names, and the world's own load reports such a record.
#[must_use]
pub fn own_name(catalogue: &Catalogue<'_>, text: &Text<'_>, who: &Who) -> String {
    let given = who
        .given
        .as_ref()
        .map(|g| {
            g.name(
                |n| text.activity(n).map(Into::into),
                |c| illusion(catalogue, text, c),
            )
        })
        .filter(|n| !n.is_empty());
    let unit = Unit::descriptor(catalogue, who.code).and_then(Result::ok);
    let spelled = unit.as_ref().and_then(|u| {
        let generator = world::naming::generator(u, who.table)?;

        Some(world::naming::spelled(text, generator, who.name?))
    });
    let numbered = || {
        let n = who.number?;

        numbered(catalogue, text, unit.as_ref()?, n).ok()?
    };

    given.or(spelled).or_else(numbered).unwrap_or_default()
}

/// Agent `agent`, of descriptor `code`, as [`own_name`] names it: `AP:1`
/// where not 0, `APU:4`, its number, what `SpecifyName` gave.
#[must_use]
pub fn who(world: &World, agent: u32, code: FourCc) -> Who {
    let piece = world.piece(agent);
    let value = |key| piece.and_then(|p| attrib::get(&p.attributes, key));

    Who {
        code,
        name: value(world::naming::NAME)
            .filter(|n| *n != 0)
            .map(i32::cast_unsigned),
        table: value(crate::data::attrib::NAME_GEN_TABLE),
        number: world.numbered(agent),
        given: world.given_name(agent).cloned(),
    }
}

/// The name a selected unit's panel shows: its
/// [`own_name`], else [`title`]; `fallback` where the descriptor will
/// not parse.
#[must_use]
pub fn panel_name(catalogue: &Catalogue<'_>, text: &Text, who: &Who, fallback: &str) -> String {
    let named = own_name(catalogue, text, who);
    let unit = Unit::descriptor(catalogue, who.code).and_then(Result::ok);

    unit.and_then(|u| title(catalogue, text, &u, Some(&named)).ok())
        .unwrap_or_else(|| fallback.to_owned())
}

/// The item key held by a dropped `Resource_Items` unit.
#[must_use]
pub fn dropped_item(world: &World, agent: u32) -> Option<i32> {
    let piece = world
        .piece(agent)
        .filter(|p| p.name.eq_ignore_ascii_case(world::inventory::DROPPED_AS))?;
    let item = world.first_item(piece.handle)?;

    Some(u8::try_from(item).map_or(item, |n| Key::new(world::players::INVENTORY, n).packed()))
}

/// A dropped item's title from `QITM` or its descriptor.
#[must_use]
pub fn dropped_name(
    catalogue: &Catalogue<'_>,
    text: &Text,
    world: &World,
    agent: u32,
) -> Option<String> {
    let key = dropped_item(world, agent)?;

    details::item(catalogue, text, key).map(|(name, _)| name)
}

/// A unit list row's name, panel name with flags 0: its
/// [`own_name`], else its descriptor's title, "Blacksmith Level 2"
/// (`ui_palace_roster_buildings.png`), else its code.
#[must_use]
pub fn listed_name(catalogue: &Catalogue<'_>, text: &Text, who: &Who) -> String {
    let own = own_name(catalogue, text, who);
    let unit = || Unit::descriptor(catalogue, who.code).and_then(Result::ok);

    if own.is_empty() {
        // Flags 0: its own title, looked up in the XML strings.
        unit().map_or_else(
            || who.code.to_string(),
            |u| {
                let own = u.head.title();

                text.strings()
                    .of(&own)
                    .map_or_else(|| own.into_owned(), str::to_owned)
            },
        )
    } else {
        own
    }
}

/// A numbered name's text: `GMTX` string 184, `%s #%d`.
pub const NUMBERED_TEXT: Ref = Ref::gmtx(184);

/// A numbered unit's name, as `BuildingRec` or
/// `VehicleRec` writes it: [`NUMBERED_TEXT`] with the
/// title its `UpgradeFrom` chain starts at (a character's own) and
/// `number` ([`crate::sim::world::World::numbered`]). "Wizards Tower
/// #2", "Tax Collector #1".
///
/// # Errors
///
/// If a descriptor on the chain will not parse.
pub fn numbered(
    catalogue: &Catalogue<'_>,
    text: &Text,
    unit: &desc::Unit<'_>,
    number: i32,
) -> Result<Option<String>> {
    let title = root(catalogue, unit)?;

    Ok(text
        .get(NUMBERED_TEXT)
        .map(|format| printf(&format, &[(&title).into(), number.into()])))
}

/// An illusion's name, as Illusionary Hero writes it:
/// `GMTX` [`world::naming::ILLUSION_TEXT`] over descriptor `code`'s `HEAD`
/// title, "Illusionary Warrior".
#[must_use]
pub fn illusion(catalogue: &Catalogue<'_>, text: &Text, code: FourCc) -> Option<String> {
    let unit = Unit::descriptor(catalogue, code)?.ok()?;
    let format = text.get(Ref::gmtx(world::naming::ILLUSION_TEXT))?;

    Some(printf(&format, &[(&unit.head.title().into_owned()).into()]))
}

/// A reward flag's help topic, "Heroes are often motivated by rewards
/// offered by the Sovereign": the constant `'h152'` in `HelpController`.
pub const REWARD_HELP: FourCc = FourCc(*b"h152");

/// The help page's `HPTX` topic for a unit of kind `kind` (unit kind,
/// [`crate::sim::world::kind::Kind`]), as `HelpController` picks it: a
/// building's `HelpID`, a reward flag's [`REWARD_HELP`], any other its
/// character `HelpID`. `None` keeps the page's own text, `AP95`'s "No
/// knowledge available".
///
/// A chest or sign is a building, kind 0: saves keep them in `BLDG`, and
/// `GSUnit` singles out `BBt` among buildings. So
/// they read their own `HelpID`: a chest's `h155`, "Heroes often discover
/// treasure…".
#[must_use]
pub fn help(kind: kind::Kind, unit: &desc::Unit<'_>) -> Option<FourCc> {
    match kind {
        kind::Kind::BUILDING => unit.building()?.help_id(),
        kind::Kind::REWARD => Some(REWARD_HELP),
        _ => unit.character()?.help_id(),
    }
}

/// The help pages, both `HelpController`s (controller factory,
/// controller factory): `AP94` has no name box and other stonework. A
/// selection panel's 8028 opens [`SHORT_HELP`], its 8029 [`HELP`]; each
/// holds one of them.
pub const HELP: FourCc = FourCc(*b"AP95");
pub const SHORT_HELP: FourCc = FourCc(*b"AP94");

/// The "?" opening [`SHORT_HELP`]: a compare with `0x1f5c` in the hero's,
/// vehicle's, caravan's, sign's, resource's and tax collector's method;
/// 8029 opens [`HELP`] in the rest.
pub const SHORT_HELP_BUTTON: i32 = 8028;

/// The help page under a selection panel: [`SHORT_HELP`] where it holds
/// [`SHORT_HELP_BUTTON`], else [`HELP`]. **Inferred** for the panels
/// whose controller is unread (`AP74`, `AP97`, `AP98`, `APb5`, `APc5`).
#[must_use]
pub fn help_page(selection: &crate::view::menu::Layout) -> FourCc {
    if selection.control(SHORT_HELP_BUTTON).is_some() {
        SHORT_HELP
    } else {
        HELP
    }
}

/// How far upgrade root walks `UpgradeFrom`.
const UPGRADE_STEPS: usize = 2;

/// The title at the start of a building's `UpgradeFrom` chain.
pub(crate) fn root(catalogue: &Catalogue<'_>, unit: &desc::Unit<'_>) -> Result<String> {
    let mut title = unit.head.title().into_owned();
    let mut from = unit.building().and_then(|b| b.upgrade_from());

    for _ in 0..UPGRADE_STEPS {
        let Some(code) = from else { break };
        let Some(up) = Unit::descriptor(catalogue, code).transpose()? else {
            break;
        };

        title = up.head.title().into_owned();
        from = up.building().and_then(|b| b.upgrade_from());
    }

    Ok(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_hit_points_show_as_zero() {
        assert_eq!(Health::of(-3, 20), Health { now: 0, max: 20 });
    }

    #[test]
    fn a_poisoned_unit_draws_the_second_bar() {
        assert_eq!(Bar::of(2).set(), 1010);
        assert_eq!(Bar::of(0).set(), 1000);
    }
}
