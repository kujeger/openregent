//! Descriptors as the world reads them: what placing, blocking, walking
//! and drawing a unit need of its `DUNT` record and art ([`Body`]), what
//! orders and effectors read by name ([`Named`]), and a character's
//! attributes as made. **Binary** unless marked.

use crate::data::attrib::{
    ARMOR_BASIC_DAMAGE, ARMOR_TYPE_INDEX, ARTIFICE, CHARISMA, DODGE, EXPERIENCE, GOLD, HP, HTOH,
    INTELLIGENCE, LEVEL_XP, MAGIC_RESISTANCE, MAX_ATTACK_RANGE, MAX_HP, MIN_ATTACK_RANGE,
    NOT_FLAGGABLE, NOT_SPELL_TARGET, PARRY, PRIMARY_STAT, RANGED, SIGHT_RANGE, SPEED, STRENGTH,
    TEAM, VITALITY, WEAPON_BASIC_DAMAGE, WEAPON_POISONED, WEAPON_TYPE_INDEX, WILL_POWER,
};
use crate::data::fourcc::FourCc;
use crate::sim::world::piece_state;
use std::collections::BTreeMap;

/// A descriptor's kind word, as `SpawnUnit` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// 2: placed by agent create placed b.
    Building,
    /// 3: a mobile, placed by agent create placed.
    Character,
    /// 4: an effector's kind.
    Overlay,
    /// 5: a particle system.
    Particles,
    /// 0, a general unit: the start makes a chart one a terrain feature
    /// (`sim::start`). `SpawnUnit` throws.
    General,
    /// 1, a projectile: `SpawnUnit` throws.
    Projectile,
}

/// What placing a piece and blocking one need of its descriptor.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "a descriptor flag each")]
pub struct Body {
    /// `HEAD` name: the `.dat` instance its agent is made from.
    pub name: String,
    pub class: Class,
    /// `Info & 0x30`: the collision layer blocker default compares.
    pub layer: u32,
    /// Half cells it takes about its own: `sim::placement::taken`.
    pub cells: Vec<(i32, i32)>,
    /// How the grid keeps those cells on the map: `sim::placement::edge`.
    pub edge: crate::sim::placement::Edge,
    /// Where what it spawns appears, 32nds of a cell about it, by the art
    /// slot it draws: `sim::placement::exits`.
    pub exits: Vec<(i32, i32)>,
    /// How its direction picks that slot.
    pub facing: Facing,
    /// Attributes as made: [`initial`] for a character,
    /// [`super::build::initial`] for a building.
    pub attributes: Vec<(i32, i32)>,
    pub lists: Extras,
    /// `AllowedSpells`, `DACT` ids in index order: a new unit's spell
    /// entries.
    pub spells: Vec<FourCc>,
    /// `Flags` `Heals`: a character's bit 0, a building's bit 9. The
    /// periodic tick mends it (`super::combat`).
    pub heals: bool,
    /// A character's `Flags` `NotHero`: never counted as a hero.
    pub not_hero: bool,
    /// A character's `Flags` `IsInventoryItem`: its code is an item key.
    pub item: bool,
    /// A building's `Flags` `IsGuild` (`BuildingDesc` bit 1).
    pub guild: bool,
    /// A building's `IncomeType`; 0 for any other
    /// kind.
    pub income: u32,
    /// A mobile's `DMOV` class: how it walks.
    pub gait: Option<crate::sim::movement::Class>,
    /// Its pad mask about its own half cell:
    /// `IsAdjacent` and the flatten read it.
    pub mask: Option<crate::sim::placement::Pad>,
    /// What its placement clears around it, about its own half cell:
    /// `sim::clearing::ring` with `clearing::MARGIN`.
    pub ring: Option<crate::sim::placement::Pad>,
    /// Whether its placement paints its pad Dirt (`super::building_ground`).
    pub lays: super::building_ground::Lays,
    /// A building's price, `Building::cost`; a character's recruit price,
    /// `Character::cost`.
    pub cost: i32,
    /// A building's `Multiplier` in scale 1024: the price
    /// of each more of its code. 0 for a character.
    pub multiplier: i32,
    /// A character's recruit delay, ms (`VehicleDesc`); 0 for a
    /// building.
    pub delay: i32,
    /// Its `NameGenType`: the generator `AP:1` comes from.
    pub names: Option<FourCc>,
    /// Its `DialogID`, `SECD +0x00`: what `ChangeUnitType` keeps in
    /// `APU:5`.
    pub dialog: Option<FourCc>,
    /// The key its counts go under (`world::tally_key`); `None` where
    /// not read.
    pub tally: Option<FourCc>,
    /// The key a numbered name counts, a `NumberedName` building's
    /// (`world::numbered_key`).
    pub numbered: Option<FourCc>,
    /// Its art's sets by whole id, as an action's `InitAction` plays them;
    /// `None` where the art is not given.
    pub poses: Option<Poses>,
    /// Whether the minimap shows it as made: `Info`
    /// `NotVisibleInOverheadView` hides it.
    pub overhead: super::draw::Overhead,
    /// The set a new unit is made in ([`crate::content::unit::Unit::initial`]),
    /// whole id; `None` where the art is not given.
    pub made_in: Option<u32>,
    /// Slots in the set it is made in ([`Body::made_in`]): the bound of
    /// `GSUnit`'s slot draw as the start makes it (`sim::start`).
    pub slots: u16,
    /// Whether `BuildingRec` places it.
    pub placing: Placing,
    /// The set id a new unit's record names: [`Body::made_in`]'s last
    /// layer. **Corpus**: what HD loaded for the scatter of a
    /// generated map.
    pub recorded: Option<u32>,
    /// A building's `UpgradeTo`.
    pub upgrade: Option<FourCc>,
    /// A building's `UpgradeFrom`: the one it is an upgrade of.
    pub upgraded: Option<FourCc>,
    /// The `MaxHP` of a building's `UpgradeFrom`, 0 for none: its
    /// construction stages count from there ([`crate::sim::damage`]).
    pub base_hp: i32,
    /// Where a missile at it aims (`super::missile`).
    pub hit: super::missile::Hit,
    /// Where its missiles leave it (`super::missile`).
    pub launch: super::missile::Launch,
    /// Its `PRIM` callback where it has one: a projectile's impact
    /// function (`super::missile`).
    pub script: Option<super::missile::Script>,
    /// An overlay's `AttachmentPointID`, `PRIM` `+0x18`: where it hangs
    /// on its holder; 0 for any other kind.
    pub attach: u32,
    /// An overlay's `StackPriority` (`SECD +0x04`), by which
    /// `GSUnit` stacks it (`super::flag`); 0 for none, for one
    /// with `Info` bit 12 `NotVisibleInIsoView`, and for any other kind.
    pub stack: u32,
    /// Its pad's width and height in ground pixels, before the half-cell
    /// rounding: `PRIM` `+0x04`..`+0x10`,
    /// which a missile's scatter spans.
    pub span: (i32, i32),
    /// The particle systems its art's sets carry (`super::plume`).
    pub vents: super::plume::Vents,
    /// Its art's layers that take an `Animate` (`super::animate`).
    pub animated: super::animate::Animated,
    /// The set `UnitIMG` first plays ([`super::plume::FIRST_SETS`]); `None`
    /// without art.
    pub first: Option<u32>,
    /// A particle system's (`super::plume`); `None` for any other
    /// kind.
    pub particles: Option<super::plume::Particles>,
    /// Unit state's low byte as made: [`crate::sim::world::piece_state::DONT_BLOCK`] from
    /// `Info` `DontBlock`. Read for flags.
    pub state: u8,
    /// A projectile's spread at a building: `Info`
    /// `NoRandomTargetOnDestination` fixes it ([`super::missile::Missile::of`]).
    pub spread: super::missile::Spread,
    /// Who may see it: `Info` `NotVisibleToOpponent` its owner alone.
    pub sight: super::Sight,
}

impl Body {
    /// Whether it stands on the collision grid: not with `DontBlock`
    /// ([`Body::state`], unit grid register).
    #[must_use]
    pub const fn grid(&self) -> super::new_map::OnGrid {
        if self.state & piece_state::DONT_BLOCK == 0 {
            super::new_map::OnGrid::On
        } else {
            super::new_map::OnGrid::Off
        }
    }
}

/// Whether `BuildingRec` places a building: only if its art has an
/// `Inactive` set (`BuildingRec`, set `0xd0`); else it
/// stays on its chart cell, unlevelled, nothing cleared or paved.
/// **Corpus**: the Siege's four `ABA1` Siege Markers, `Stand` alone,
/// leave their scatter and ground as made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placing {
    #[default]
    Placed,
    Left,
}

/// How a unit's direction word picks the slot of a set it draws:
/// `UnitIMGPartEntry`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Facing {
    /// Rounded to the nearest of the set's slots
    /// (`movement::action::slot`).
    #[default]
    Turns,
    /// `Info` `Directionless`: the direction is the slot, 0 past the last.
    Fixed,
}

/// Animation sets by whole id.
pub type Poses = BTreeMap<u32, crate::sim::movement::action::Pose>;

/// What orders and effectors read of descriptors, by folded `HEAD` name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Named {
    /// `DACT` records: an Action order's length and aim.
    pub actions: BTreeMap<String, super::walking::Act>,
    /// `DUNT` particle systems, kind 5: an effector of one is never
    /// re-armed.
    pub particles: std::collections::BTreeSet<String>,
    /// `DACT` spells, by folded `HEAD` name: `super::spells`.
    pub spells: BTreeMap<String, super::spell::Spell>,
    /// `DACT` functions, by folded `HEAD` name: what an action's peak
    /// dispatches. An action naming none is
    /// absent.
    pub scripts: BTreeMap<String, super::missile::Script>,
    /// `DACT` folded `HEAD` names by record id: an order names its action
    /// by code (`OrderAction`).
    pub codes: BTreeMap<FourCc, String>,
}

/// Bodies by `DUNT` id.
pub type Bodies = crate::base::digest::Map<FourCc, Body>;

/// Each body's `HEAD` name, folded, to its `DUNT` id ([`body_names`]).
pub type BodyNames = BTreeMap<String, FourCc>;

/// The name index of `bodies`: where a name repeats, the lowest code read
/// as an unsigned little-endian word. Descriptor by name
/// walks a `std::map` on the code word in key order and
/// takes the first match. **Inferred**: the key is that word. HD repeats
/// one name, `speed_tonic_icon` (`XR01`, `XS01`); both orders pick `XR01`.
#[must_use]
pub fn body_names(bodies: &Bodies) -> BodyNames {
    let mut codes: Vec<(&FourCc, &Body)> = bodies.iter().collect();

    // Highest first: a later entry wins the collect.
    codes.sort_by_key(|(code, _)| std::cmp::Reverse(code.word()));
    codes
        .into_iter()
        .map(|(code, b)| (b.name.to_ascii_lowercase(), *code))
        .collect()
}

/// Whether a character's descriptor lists anything past its fixed fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extras {
    /// No entry: no spell.
    Empty,
    Held,
}

/// A character's attribute list as `VehicleRec` makes
/// it from its descriptor, in its order: bank, index, value.
///
/// | Attributes | From |
/// |---|---|
/// | `APU:0`, `APU:1` | max hp |
/// | `APU:3`, `APU:2` | sight range, gold |
/// | `APV:10`, `12`, `13`, `14` | experience, speed, attack range min, max |
/// | `APV:15`, `16` | 0 |
/// | `APV:3`, `17`, `2`, `4`, `1`, `0`, `8` | vitality, charisma, artifice, will power, intelligence, strength, magic resistance |
/// | `APV:5`, `6`, `7`, `9`, `18`, `21`, `47` | attack, ranged attack, parry, dodge, weapon and armor damage, primary stat |
/// | `APU:9`, `APU:10` | `Flags` `NotFlaggable`, `NotSpellTarget`, as 0 or the bit |
/// | `APV:29` | 1 with `Flags` `Poisons` |
/// | `APV:26`, `APV:25` | the first `AllowedArmor`, `AllowedWeapon` value, where listed |
///
/// Not `APV:11`, `ExperienceLevel`: a hero's birth script sets it 1 when
/// below 1 (`hero_birth`, SDK `Hero_Births.gpl`).
///
/// **Game**: a wizard's `WeaponTypeIndex` reads 8, its staff.
#[must_use]
pub fn initial(ch: &crate::data::desc::Character<'_>) -> Vec<(i32, i32)> {
    use crate::data::desc::CharacterFlag;

    let v = |f: Option<u32>| f.unwrap_or_default().cast_signed();
    let flags = ch.flags().unwrap_or_default();
    let bit = |f: CharacterFlag| (flags & (1 << f as u32)).cast_signed();
    let mut made = vec![
        (HP.packed(), v(ch.max_hp())),
        (MAX_HP.packed(), v(ch.max_hp())),
        (SIGHT_RANGE.packed(), v(ch.sight_range())),
        (GOLD.packed(), v(ch.gold())),
        (LEVEL_XP.packed(), v(ch.experience())),
        (SPEED.packed(), v(ch.speed())),
        (MIN_ATTACK_RANGE.packed(), v(ch.attack_range_min())),
        (MAX_ATTACK_RANGE.packed(), v(ch.attack_range_max())),
        (TEAM.packed(), 0),
        (EXPERIENCE.packed(), 0),
        (VITALITY.packed(), v(ch.vitality())),
        (CHARISMA.packed(), v(ch.charisma())),
        (ARTIFICE.packed(), v(ch.artifice())),
        (WILL_POWER.packed(), v(ch.will_power())),
        (INTELLIGENCE.packed(), v(ch.intelligence())),
        (STRENGTH.packed(), v(ch.strength())),
        (MAGIC_RESISTANCE.packed(), v(ch.magic_resistance())),
        (HTOH.packed(), v(ch.attack())),
        (RANGED.packed(), v(ch.ranged_attack())),
        (PARRY.packed(), v(ch.parry())),
        (DODGE.packed(), v(ch.dodge())),
        (WEAPON_BASIC_DAMAGE.packed(), v(ch.weapon_damage())),
        (ARMOR_BASIC_DAMAGE.packed(), v(ch.armor_damage())),
        (PRIMARY_STAT.packed(), v(ch.primary_stat())),
        (NOT_FLAGGABLE.packed(), bit(CharacterFlag::NotFlaggable)),
        (
            NOT_SPELL_TARGET.packed(),
            bit(CharacterFlag::NotSpellTarget),
        ),
    ];

    // Then `Poisons`, and the first armour and weapon type.
    let lists = ch.allowed();
    let first = |l: &[(u32, u32)]| l.first().map(|(_, v)| v.cast_signed());

    made.extend(
        ch.has(CharacterFlag::Poisons)
            .then_some((WEAPON_POISONED.packed(), 1)),
    );
    made.extend(first(&lists.armor).map(|v| (ARMOR_TYPE_INDEX.packed(), v)));
    made.extend(first(&lists.weapons).map(|v| (WEAPON_TYPE_INDEX.packed(), v)));
    made
}

impl Facing {
    /// How a descriptor's units face: fixed where `Directionless`.
    #[must_use]
    pub fn of(desc: &crate::data::desc::Unit<'_>) -> Facing {
        if desc.engine().has(crate::data::desc::Info::Directionless) {
            Facing::Fixed
        } else {
            Facing::Turns
        }
    }

    /// The slot of `slots` a unit facing `direction` draws.
    #[must_use]
    pub fn slot(self, direction: u16, slots: usize) -> usize {
        self.slot_of(i32::from(direction), slots)
    }

    /// [`Facing::slot`] of a facing as the world keeps it: a fixed one
    /// below 0 also draws slot 0.
    #[must_use]
    pub fn slot_of(self, facing: i32, slots: usize) -> usize {
        match self {
            Facing::Turns => crate::sim::movement::action::slot(facing, slots),
            Facing::Fixed => usize::try_from(facing)
                .ok()
                .filter(|f| *f < slots)
                .unwrap_or(0),
        }
    }
}
