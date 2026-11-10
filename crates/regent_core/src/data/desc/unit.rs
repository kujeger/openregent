//! `DUNT` — units, buildings, projectiles, overlays and effects.

use super::atch::{self, PAIR_LEN, Slots};
use super::particle::BLOCK_LEN;
use super::{
    Head, Origin, Part, ParticleSystem, bit, cstr, field, fixed, id_field, require, word, word_in,
};
use crate::data::error::{Error, Result, slice};
use crate::data::fourcc::FourCc;

/// What a `DUNT` record describes, from the word at `HEAD + 4`.
///
/// Six values; 394 of 394 records match the SDK's `subType`. The only type tag
/// in the binary, and it selects `SECD`'s layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitKind {
    General,
    Projectile,
    Building,
    Character,
    Overlay,
    ParticleSystem,
}

impl UnitKind {
    const fn from_word(word: u32) -> Option<Self> {
        match word {
            0 => Some(UnitKind::General),
            1 => Some(UnitKind::Projectile),
            2 => Some(UnitKind::Building),
            3 => Some(UnitKind::Character),
            4 => Some(UnitKind::Overlay),
            5 => Some(UnitKind::ParticleSystem),
            _ => None,
        }
    }
}

/// A bit of the `Info` word at `PRIM + 0x00`. Sample counts in brackets.
/// Shared by every unit subtype.
///
/// **Binary**: names and masks are HD's XML importer table, registered by
/// `MajestyHD.exe:unit engine names`. The five bits at 0 are set on no shipped record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Info {
    Static = 0,                          // 136
    TerrainDependent = 1,                // 0
    Directionless = 2,                   // 349
    DontBlock = 3,                       // 280
    BlockGround = 4,                     // 262
    BlockFlying = 5,                     // 102
    VisibilityIgnoresTeam = 6,           // 0
    BoundingBoxSelectOnly = 7,           // 0
    ModifyTerrainTextureOnPlacement = 8, // 92
    ModifyTerrainHeightOnPlacement = 9,  // 37
    IgnorePlayerColors = 10,             // 0
    AllocateLocalId = 11,                // 2
    NotVisibleInIsoView = 12,            // 9
    NotVisibleInOverheadView = 13,       // 8
    NotVisibleIn3dView = 14,             // 0
    NotVisibleToOpponent = 15,           // 3
    NoGplAgent = 16,                     // 55
    RevealTerrainIfExposed = 17,         // 17
    NoRandomTargetOnDestination = 18,    // 7
}

impl Info {
    pub const ALL: [Info; 19] = [
        Info::Static,
        Info::TerrainDependent,
        Info::Directionless,
        Info::DontBlock,
        Info::BlockGround,
        Info::BlockFlying,
        Info::VisibilityIgnoresTeam,
        Info::BoundingBoxSelectOnly,
        Info::ModifyTerrainTextureOnPlacement,
        Info::ModifyTerrainHeightOnPlacement,
        Info::IgnorePlayerColors,
        Info::AllocateLocalId,
        Info::NotVisibleInIsoView,
        Info::NotVisibleInOverheadView,
        Info::NotVisibleIn3dView,
        Info::NotVisibleToOpponent,
        Info::NoGplAgent,
        Info::RevealTerrainIfExposed,
        Info::NoRandomTargetOnDestination,
    ];

    /// The SDK's own spelling, which is what the XML carries.
    pub const fn name(self) -> &'static str {
        match self {
            Info::Static => "Static",
            Info::TerrainDependent => "TerrainDependent",
            Info::Directionless => "Directionless",
            Info::DontBlock => "DontBlock",
            Info::BlockGround => "BlockGround",
            Info::BlockFlying => "BlockFlying",
            Info::VisibilityIgnoresTeam => "VisibilityIgnoresTeam",
            Info::BoundingBoxSelectOnly => "BoundingBoxSelectOnly",
            Info::ModifyTerrainTextureOnPlacement => "ModifyTerrainTextureOnPlacement",
            Info::ModifyTerrainHeightOnPlacement => "ModifyTerrainHeightOnPlacement",
            Info::IgnorePlayerColors => "IgnorePlayerColors",
            Info::AllocateLocalId => "AllocateLocalID",
            Info::NotVisibleInIsoView => "NotVisibleInISOView",
            Info::NotVisibleInOverheadView => "NotVisibleInOverheadView",
            Info::NotVisibleIn3dView => "NotVisibleIn3DView",
            Info::NotVisibleToOpponent => "NotVisibleToOpponent",
            Info::NoGplAgent => "NoGPLAgent",
            Info::RevealTerrainIfExposed => "RevealTerrainIfExposed",
            Info::NoRandomTargetOnDestination => "NoRandomTargetOnDestination",
        }
    }
}

/// A bit of a building's `Flags` word at `SECD + 0x28`. Counts in brackets.
///
/// `NoFlaggable` as `M_Buildings.xml` spells it; a character's `Flags` and GPL
/// attribute [`crate::data::attrib`] `APU:9` say `NotFlaggable`.
///
/// **Binary**: the ten names and masks are HD's XML importer table, built at
/// `MajestyHD.exe:building flag names`. `BuildingRec`
/// copies `NoFlaggable` into attribute `APU:9` and `NotSpellTarget` into
/// `APU:10`. `CanHighlight`, set only on the Balance of Twilight
/// shrines `CB01` and `CB02`, which no XML describes: the cursor test in can highlight
/// accepts a building only with it (a test of bit `0x80`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildingFlag {
    NotBuildable = 0,   // 82
    IsGuild = 1,        // 31
    NoFlaggable = 2,    // 16
    HasHpBar = 3,       // 98
    NotSpellTarget = 4, // 18
    NotInMiniMap = 5,   // 12
    NumberedName = 6,   // 12
    CanHighlight = 7,   // 2, no XML
    HasGoldToolTip = 8, // 54
    Heals = 9,          // 3
}

impl BuildingFlag {
    pub const ALL: [BuildingFlag; 10] = [
        BuildingFlag::NotBuildable,
        BuildingFlag::IsGuild,
        BuildingFlag::NoFlaggable,
        BuildingFlag::HasHpBar,
        BuildingFlag::NotSpellTarget,
        BuildingFlag::NotInMiniMap,
        BuildingFlag::NumberedName,
        BuildingFlag::CanHighlight,
        BuildingFlag::HasGoldToolTip,
        BuildingFlag::Heals,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            BuildingFlag::NotBuildable => "NotBuildable",
            BuildingFlag::IsGuild => "IsGuild",
            BuildingFlag::NoFlaggable => "NoFlaggable",
            BuildingFlag::HasHpBar => "HasHPBar",
            BuildingFlag::NotSpellTarget => "NotSpellTarget",
            BuildingFlag::NotInMiniMap => "NotInMiniMap",
            BuildingFlag::NumberedName => "NumberedName",
            BuildingFlag::CanHighlight => "CanHighlight",
            BuildingFlag::HasGoldToolTip => "HasGoldToolTip",
            BuildingFlag::Heals => "Heals",
        }
    }
}

/// A bit of a character's `Flags` word at `SECD + 0x60`. Counts in brackets.
///
/// Bits differ from [`BuildingFlag`]'s for the same names.
///
/// **Binary**: the eleven names and masks are HD's XML importer table, built
/// at `MajestyHD.exe:character flag names`.
/// `VehicleRec` copies `NotFlaggable` into attribute `APU:9` and
/// `NotSpellTarget` into `APU:10`. The XML names both on the
/// same two records, so only the binary separates them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharacterFlag {
    Heals = 0,            // 39
    NotFlaggable = 1,     // 2
    HasHpBar = 2,         // 80
    Poisons = 3,          // 3
    NotSpellTarget = 4,   // 2
    NotInMiniMap = 5,     // 12
    NumberedName = 6,     // 7
    CanHighlight = 7,     // 81
    HasGoldToolTip = 8,   // 2
    NotHero = 9,          // 2
    IsInventoryItem = 10, // 0
}

impl CharacterFlag {
    pub const ALL: [CharacterFlag; 11] = [
        CharacterFlag::Heals,
        CharacterFlag::NotFlaggable,
        CharacterFlag::HasHpBar,
        CharacterFlag::Poisons,
        CharacterFlag::NotSpellTarget,
        CharacterFlag::NotInMiniMap,
        CharacterFlag::NumberedName,
        CharacterFlag::CanHighlight,
        CharacterFlag::HasGoldToolTip,
        CharacterFlag::NotHero,
        CharacterFlag::IsInventoryItem,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            CharacterFlag::Heals => "Heals",
            CharacterFlag::NotFlaggable => "NotFlaggable",
            CharacterFlag::HasHpBar => "HasHPBar",
            CharacterFlag::Poisons => "Poisons",
            CharacterFlag::NotSpellTarget => "NotSpellTarget",
            CharacterFlag::NotInMiniMap => "NotInMiniMap",
            CharacterFlag::NumberedName => "NumberedName",
            CharacterFlag::CanHighlight => "CanHighlight",
            CharacterFlag::HasGoldToolTip => "HasGoldToolTip",
            CharacterFlag::NotHero => "NotHero",
            CharacterFlag::IsInventoryItem => "IsInventoryItem",
        }
    }
}

/// Who may use a unit: a bit of `CanUse`, the `PRIM` word at `0x08`.
///
/// **Binary**: names and masks from HD's importer in unit engine names. The build
/// menu lists a building only with [`User::HumanPlayer`];
/// character usable accepts a character only with [`User::HumanPlayer`] or
/// [`User::Monster`]. Its callers read `NotFlaggable` and `NotSpellTarget`;
/// **Inferred**: they are the reward-flag and spell-target tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum User {
    HumanPlayer = 0,
    Monster = 1,
    /// Registered, never set.
    AiPlayer = 2,
}

impl User {
    pub const ALL: [User; 3] = [User::HumanPlayer, User::Monster, User::AiPlayer];

    /// The SDK's own spelling.
    pub const fn name(self) -> &'static str {
        match self {
            User::HumanPlayer => "HumanPlayer",
            User::Monster => "Monster",
            User::AiPlayer => "AIPlayer",
        }
    }

    /// The loader's default for a zero `CanUse`: `A` ids are the player's,
    /// `B` ids the monsters'.
    pub const fn default_for(id: FourCc) -> Option<User> {
        match id.as_bytes()[0] {
            b'A' => Some(User::HumanPlayer),
            b'B' => Some(User::Monster),
            _ => None,
        }
    }
}

/// Whether the engine gives a unit a shadow of its own.
///
/// Separate from the shadow baked into sprite pixels ([`crate::view::raster::shade`]).
/// Only the expansion's `WPk1` Horrify bolt sets it, to
/// [`ShadowType::Dynamic`]; every other unit reads [`ShadowType::None`].
///
/// **Binary**: names and values from HD's descriptor XML importer, registered
/// at `MajestyHD.exe:unit engine names`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowType {
    /// 0. All but one record.
    None,
    /// 1. Unused in shipped data.
    Static,
    /// 2.
    Dynamic,
}

impl ShadowType {
    const fn from_word(word: u32) -> Option<Self> {
        match word {
            0 => Some(ShadowType::None),
            1 => Some(ShadowType::Static),
            2 => Some(ShadowType::Dynamic),
            _ => None,
        }
    }
}

/// A `DUNT` record: one unit, building, projectile, overlay or effect.
#[derive(Debug, Clone, Copy)]
pub struct Unit<'a> {
    pub head: Head<'a>,
    /// The `<Engine>` section, common to every subtype.
    pub prim: &'a [u8],
    /// The `<Game>` section, laid out by [`Unit::kind`]. Empty for three
    /// subtypes.
    pub secd: &'a [u8],
    kind: UnitKind,
    tail: Tail<'a>,
    origin: Origin,
}

impl<'a> Unit<'a> {
    /// Read a record, checking every field the accessors read.
    ///
    /// # Errors
    ///
    /// If the tree lacks a `HEAD` or `PRIM`, `HEAD` will not parse, the
    /// subtype is unknown, `PRIM`'s [`Tail`] does not sum to its length or
    /// names an unknown `ShadowType`, or `SECD` is short of its subtype's
    /// fields and lists. No shipped record fails.
    pub fn parse(part: &Part<'a>) -> Result<Self> {
        let head = Head::parse(require(part, *b"HEAD")?.body)?;
        let kind = UnitKind::from_word(head.word).ok_or(Error::BadValue {
            what: "unit subtype",
            value: u64::from(head.word),
        })?;
        let prim = fixed(
            require(part, *b"PRIM")?.body,
            "unit engine fields",
            0x14 + engine_block(kind),
        )?;
        let secd = part.find(b"SECD").map_or(&[][..], |p| p.body);
        let tail = tail(prim, kind)?;

        if ShadowType::from_word(word(prim, prim.len() - 8)).is_none() {
            return Err(Error::BadValue {
                what: "unit shadow type",
                value: u64::from(word(prim, prim.len() - 8)),
            });
        }

        Self::check_secd(kind, secd)?;

        Ok(Unit {
            head,
            prim,
            secd,
            kind,
            tail,
            origin: Origin::of(part),
        })
    }

    /// `SECD` holds its subtype's fixed fields, and a character's or a
    /// building's lists end it.
    fn check_secd(kind: UnitKind, secd: &[u8]) -> Result<()> {
        let whole = match kind {
            UnitKind::Character => {
                lists(fixed(secd, "character fields", CHARACTER_FIXED)?).is_some()
            }
            UnitKind::Building => {
                produced(fixed(secd, "building fields", BUILDING_FIXED)?).is_some()
            }
            UnitKind::Overlay => {
                fixed(secd, "overlay fields", OVERLAY_FIXED)?;
                true
            }
            UnitKind::General | UnitKind::Projectile | UnitKind::ParticleSystem => true,
        };

        if whole {
            Ok(())
        } else {
            Err(Error::BrokenInvariant {
                what: "unit SECD lists end it",
                expected: secd.len() as u64,
                found: 0,
            })
        }
    }

    /// The subtype, from the word at `HEAD + 4`.
    pub const fn kind(&self) -> UnitKind {
        self.kind
    }

    /// The `<Engine>` section as a typed view.
    pub const fn engine(&self) -> Prim<'a> {
        Prim(self.prim)
    }

    /// Which `IMAG` record holds this unit's animations. 394 of 394.
    pub fn image_id(&self) -> Option<FourCc> {
        self.engine().image_id()
    }

    /// Interface menu category. 394 of 394.
    pub fn menu(&self) -> Option<u32> {
        self.engine().menu()
    }

    /// The stat block, when this is a character.
    pub fn character(&self) -> Option<Character<'a>> {
        (self.kind == UnitKind::Character).then_some(Character(self.secd))
    }

    /// The stat block, when this is a building.
    pub fn building(&self) -> Option<Building<'a>> {
        (self.kind == UnitKind::Building).then_some(Building(self.secd))
    }

    /// The draw-order priority, when this is an overlay. 133 of 133.
    pub fn stack_priority(&self) -> Option<u32> {
        (self.kind == UnitKind::Overlay).then(|| field(self.secd, 0x04))?
    }

    /// The callback's `type`, the XML's `Script type`: the word two before
    /// its name. Bit 0 passes a point for the target, bit 1 no caller.
    ///
    /// **Binary**: script record read reads `type` and `cProc`, then the name.
    pub fn script_kind(&self) -> Option<u32> {
        field(self.prim, 0x14 + engine_block(self.kind) - 8)
    }

    /// The variable part of `PRIM`: callback, attachments and sounds.
    pub const fn tail(&self) -> Tail<'a> {
        self.tail
    }

    /// The particle block, when this is a particle system. 22 of 22.
    pub fn particle_system(&self) -> Option<ParticleSystem<'a>> {
        (self.kind == UnitKind::ParticleSystem)
            .then(|| ParticleSystem(&self.prim[0x14..0x14 + BLOCK_LEN]))
    }

    /// The `ShadowType` field, the second-to-last word of `PRIM`, which
    /// [`Unit::parse`] checks the importer knows.
    ///
    /// # Panics
    ///
    /// Never on a parsed unit.
    ///
    /// **Corpus**: only `WPk1` sets it (`Dynamic`). The last word is zero on
    /// every unit of all four datasets (583).
    ///
    /// **Inferred**: the last word is `ShadowOptions`, registered beside
    /// `ShadowType` in the importer (`Least` = 0, `Max` = 255) and in no
    /// shipped XML. Unconfirmable while nothing sets it.
    pub fn shadow_type(&self) -> ShadowType {
        let raw = word(self.prim, self.prim.len() - 8);

        ShadowType::from_word(raw).unwrap_or_else(|| panic!("shadow type {raw} past Unit::parse"))
    }

    /// Whether `user` may use this unit, as the engine holds it after loading.
    ///
    /// The file word is zero on every shipped unit; the loader fills a zero
    /// from [`User::default_for`], on every subtype but particle systems
    /// (can use default, inlined in unit prim read common). The SDK XML prints the
    /// result. A record made from XML keeps its zero ([`Origin::Xml`]).
    pub fn can_use(&self, user: User) -> bool {
        let word = self.engine().can_use().unwrap_or(0);
        let filled = self.kind != UnitKind::ParticleSystem && self.origin == Origin::Archive;

        if word != 0 {
            word >> user as u32 & 1 == 1
        } else {
            filled && User::default_for(self.head.id) == Some(user)
        }
    }

    /// An overlay's `Flags` word. Bit 0 is the only named one.
    pub fn overlay_flags(&self) -> Option<u32> {
        (self.kind == UnitKind::Overlay).then(|| field(self.secd, 0x08))?
    }

    /// Whether an overlay ignores the cursor. 119 of the 207 overlays.
    ///
    /// The only overlay `Flags` bit the corpus separates.
    pub fn transparent_to_mouse(&self) -> bool {
        self.kind == UnitKind::Overlay && bit(self.secd, 0x08, 0)
    }
}

/// The `<Engine>` section, shared by every unit subtype.
#[derive(Debug, Clone, Copy)]
pub struct Prim<'a>(&'a [u8]);

impl Prim<'_> {
    /// The `Info` bitfield. 568 of 568.
    pub fn info(&self) -> Option<u32> {
        field(self.0, 0x00)
    }

    pub fn has(&self, flag: Info) -> bool {
        bit(self.0, 0x00, flag as u32)
    }

    /// Every `Info` bit that is set and named.
    pub fn info_flags(&self) -> impl Iterator<Item = Info> + '_ {
        Info::ALL.into_iter().filter(|f| self.has(*f))
    }

    /// `Category`. 1 on the four reward flags `ARA1`–`ARA4`, else 0.
    ///
    /// **Binary**: read to descriptor in unit prim read common, where the
    /// importer's `Category` goes.
    pub fn category(&self) -> Option<u32> {
        field(self.0, 0x04)
    }

    /// The raw `CanUse` mask of [`User`] bits; zero on every shipped unit.
    /// [`Unit::can_use`] applies the loader's default.
    ///
    /// **Binary**: read to descriptor in unit prim read common, where the
    /// importer's `CanUse` goes.
    pub fn can_use(&self) -> Option<u32> {
        field(self.0, 0x08)
    }

    pub fn menu(&self) -> Option<u32> {
        field(self.0, 0x0c)
    } // 394
    pub fn image_id(&self) -> Option<FourCc> {
        id_field(self.0, 0x10)
    } // 394

    /// Where an overlay hangs off its owner. Overlays only, 207 of 207.
    ///
    /// A union: attachment point 0 to 3, or a four-character id (three
    /// records).
    pub fn attach_point(&self) -> Option<u32> {
        field(self.0, 0x18)
    }
}

// --------------------------------------------------------------------------
// PRIM's tail
// --------------------------------------------------------------------------
//
// Past the fixed fields `PRIM` is variable-length:
//
//     0x14 + engine_block(kind)          subtype-specific <Engine> payload
//     cstr                               the GPL callback's name, often empty
//     ATCH ...                           the attachment tree, subtypes 0-3 only
//     20 + 8 * overrides                 the sound tail
//
// That sums to the declared length on all 583 HD records, so the tail is
// walked forward and the total checked. `ATCH` is usually at `PRIM + 0x21`
// (empty callback), but takes fifteen offsets across the corpus.
//
// `ATCH`'s children carry a slot count, not a child count, hence
// [`atch::walk`] rather than `Part::parse`.

/// Bytes of subtype-specific `<Engine>` payload at `PRIM + 0x14`.
const fn engine_block(kind: UnitKind) -> usize {
    match kind {
        UnitKind::Overlay => 16,
        UnitKind::ParticleSystem => 272,
        _ => 12,
    }
}

/// The fixed part of the sound tail: version, override count, then (after the
/// overrides) default sound, `ShadowType` and a zero word.
const SOUND_TAIL_LEN: usize = 20;

/// Walk `PRIM` past its fixed fields: callback, attachments and sounds.
///
/// # Errors
///
/// If the callback string is unterminated, `ATCH` is malformed, or the
/// parts do not sum to the declared length (they do on all 583 HD
/// records).
fn tail(prim: &[u8], kind: UnitKind) -> Result<Tail<'_>> {
    let at = 0x14 + engine_block(kind);
    let script = cstr(&prim[at..], "unit script name")?;

    let (attachments, pos) = attachments_at(prim, at + script.len() + 1)?;

    let count = word_in(prim, pos + 4).ok_or(Error::Truncated {
        what: "sound override count",
        need: pos + 8,
        have: prim.len(),
    })? as usize;
    let overrides = slice(prim, "sound overrides", pos + 8, count * PAIR_LEN)?;
    let end = pos + SOUND_TAIL_LEN + count * PAIR_LEN;

    (end == prim.len())
        .then(|| Tail {
            script,
            attachments,
            overrides,
            default_sound: id_field(prim, prim.len() - 12),
        })
        .ok_or(Error::BrokenInvariant {
            what: "unit PRIM tail length",
            expected: prim.len() as u64,
            found: end as u64,
        })
}

/// Read the `ATCH` subtree at `at`, if one starts there.
///
/// Returns it and where the sound tail begins. Picks out the three tags a
/// unit uses from [`atch::walk`].
fn attachments_at(prim: &[u8], at: usize) -> Result<(Option<Attachments<'_>>, usize)> {
    let (tables, end) = atch::walk(prim, at)?;

    let found = tables.map(|tables| {
        let find = |tag: &[u8; 4]| tables.iter().find(|(t, _)| *t == tag).map(|(_, s)| *s);

        Attachments {
            explosion: find(b"EXPL").unwrap_or(Slots(&[])),
            movement: find(b"MOVE").unwrap_or(Slots(&[])),
            weapon: find(b"WEAP"),
        }
    });

    Ok((found, end))
}

/// How a unit moves. The leading word of a `MOVE` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Movement {
    Walk = 1, // 130
    Fly = 4,  // 3
}

impl Movement {
    const fn from_word(word: u32) -> Option<Self> {
        match word {
            1 => Some(Movement::Walk),
            4 => Some(Movement::Fly),
            _ => None,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Movement::Walk => "Walk",
            Movement::Fly => "Fly",
        }
    }
}

/// `ATCH` — what a unit has hanging off it.
///
/// Three fixed tables. Only movement is used: every `EXPL` (1,356) and `WEAP`
/// (598) slot in both editions is zero, so their meaning is unknown.
#[derive(Debug, Clone, Copy)]
pub struct Attachments<'a> {
    pub explosion: Slots<'a>,
    pub movement: Slots<'a>,
    /// Absent on the 40 projectiles, present on the other 299.
    pub weapon: Option<Slots<'a>>,
}

impl Attachments<'_> {
    /// The movement class this unit uses, and how. Slot 0 in all 133 cases.
    pub fn movement_class(&self) -> Option<(Option<Movement>, FourCc)> {
        self.movement
            .filled()
            .next()
            .map(|(_, word, id)| (Movement::from_word(word), id))
    }
}

/// A sound that replaces the default for one action.
#[derive(Debug, Clone, Copy)]
pub struct SoundOverride {
    /// A `DACT` record.
    pub action: FourCc,
    /// A `DSND` record.
    pub sound: FourCc,
}

/// The variable part of `PRIM`.
#[derive(Debug, Clone, Copy)]
pub struct Tail<'a> {
    /// The GPL function the engine calls back into. Empty when there is none.
    pub script: &'a [u8],
    pub attachments: Option<Attachments<'a>>,
    /// `8 * n` bytes of `(action, sound)` pairs. Only five records carry one.
    overrides: &'a [u8],
    /// A `DSND` record. `None` on the 273 units that declare no sound.
    pub default_sound: Option<FourCc>,
}

impl Tail<'_> {
    pub fn overrides(&self) -> impl Iterator<Item = SoundOverride> + '_ {
        (0..self.overrides.len() / PAIR_LEN).filter_map(|i| {
            let at = i * PAIR_LEN;

            Some(SoundOverride {
                action: id_field(self.overrides, at)?,
                sound: id_field(self.overrides, at + 4)?,
            })
        })
    }
}

/// A hero's or monster's `<Game>` section. Sample counts in brackets.
#[derive(Debug, Clone, Copy)]
pub struct Character<'a>(&'a [u8]);

/// A character's fixed `<Game>` fields: 140 bytes, the size of 57 of the
/// install's `SECD`s; longer ones add 8-byte entries.
const CHARACTER_FIXED: usize = 0x8c;

impl Character<'_> {
    pub fn dialog_id(&self) -> Option<FourCc> {
        id_field(self.0, 0x00)
    } // 65
    pub fn cost(&self) -> Option<u32> {
        field(self.0, 0x04)
    } // 16
    pub fn experience(&self) -> Option<u32> {
        field(self.0, 0x08)
    } // 52
    pub fn max_hp(&self) -> Option<u32> {
        field(self.0, 0x0c)
    } // 64
    pub fn sight_range(&self) -> Option<u32> {
        field(self.0, 0x10)
    } // 61
    pub fn speed(&self) -> Option<u32> {
        field(self.0, 0x14)
    } // 55
    pub fn attack_range_min(&self) -> Option<u32> {
        field(self.0, 0x18)
    } // 70
    pub fn attack_range_max(&self) -> Option<u32> {
        field(self.0, 0x1c)
    } // 70
    pub fn vitality(&self) -> Option<u32> {
        field(self.0, 0x20)
    } // 17
    /// **Binary**: `VehicleRec` copies it to `APV:17`.
    pub fn charisma(&self) -> Option<u32> {
        field(self.0, 0x24)
    }
    pub fn artifice(&self) -> Option<u32> {
        field(self.0, 0x28)
    } // 16
    pub fn will_power(&self) -> Option<u32> {
        field(self.0, 0x2c)
    } // 17
    pub fn intelligence(&self) -> Option<u32> {
        field(self.0, 0x30)
    } // 56
    pub fn strength(&self) -> Option<u32> {
        field(self.0, 0x34)
    } // 56
    pub fn magic_resistance(&self) -> Option<u32> {
        field(self.0, 0x38)
    } // 31
    pub fn attack(&self) -> Option<u32> {
        field(self.0, 0x3c)
    } // 56
    pub fn ranged_attack(&self) -> Option<u32> {
        field(self.0, 0x40)
    } // 11
    pub fn parry(&self) -> Option<u32> {
        field(self.0, 0x44)
    } // 60
    pub fn dodge(&self) -> Option<u32> {
        field(self.0, 0x48)
    } // 60
    pub fn weapon_damage(&self) -> Option<u32> {
        field(self.0, 0x4c)
    } // 17
    pub fn armor_damage(&self) -> Option<u32> {
        field(self.0, 0x50)
    } // 35
    pub fn gold(&self) -> Option<u32> {
        field(self.0, 0x54)
    } // 36
    pub fn recruit_delay(&self) -> Option<u32> {
        field(self.0, 0x58)
    } // 76
    pub fn name_gen_type(&self) -> Option<FourCc> {
        id_field(self.0, 0x5c)
    } // 16
    pub fn help_id(&self) -> Option<FourCc> {
        id_field(self.0, 0x64)
    } // 59
    pub fn primary_stat(&self) -> Option<u32> {
        field(self.0, 0x68)
    } // 13

    /// Whether this character has a price, which marks a hero class. In the base
    /// dataset exactly the sixteen player classes do.
    ///
    /// Needed by [`Character::primary_stat`], which reads 0 for both Artifice
    /// and no primary (Rogue vs City Guard).
    ///
    /// Not `NotHero` (the Gnome has it and is a class), nor "recruitable": the
    /// expansion's seventeenth payer is `AVO1 Gnomechamp`, which
    /// `$ChangeUnitType` in `mx_give_exp.gpl` makes from a level 8 Gnome; its
    /// price is never charged.
    ///
    /// See test `every_character_that_costs_gold_is_a_hero_class`.
    pub fn costs_gold(&self) -> bool {
        self.cost().is_some_and(|gold| gold > 0)
    }

    /// The `Flags` bitfield; bits differ from a building's.
    pub fn flags(&self) -> Option<u32> {
        field(self.0, 0x60)
    } // 104

    /// Bytes past the fixed fields: what `VehicleRec` reads its three
    /// lists from (**Inferred**; spells among them, in memory).
    /// Most characters have none.
    #[must_use]
    pub fn lists(&self) -> &[u8] {
        &self.0[CHARACTER_FIXED..]
    }

    pub fn has(&self, flag: CharacterFlag) -> bool {
        bit(self.0, 0x60, flag as u32)
    }

    pub fn flag_names(&self) -> impl Iterator<Item = CharacterFlag> + '_ {
        CharacterFlag::ALL.into_iter().filter(|f| self.has(*f))
    }

    /// The four lists past `PrimaryStat`, `VehicleDesc`,
    /// `AllowedWeapon`, `AllowedArmor`, `AllowedSpells`.
    /// Each is a word 1, a count, then that many
    /// index and value pairs; a spell's value is its `DACT` id.
    ///
    /// **Corpus**: every character's `SECD` in all four datasets ends with
    /// its fourth list, which [`Unit::parse`] checks. The leading 1 is
    /// **Inferred** a version: no list has another.
    ///
    /// # Panics
    ///
    /// Never on a parsed unit's character.
    #[must_use]
    pub fn allowed(&self) -> Allowed {
        lists(self.0).unwrap_or_else(|| panic!("character lists past Unit::parse"))
    }
}

/// A character's four lists, or `None` where the bytes do not end with
/// the fourth.
fn lists(secd: &[u8]) -> Option<Allowed> {
    let mut at = LISTS_AT;
    let mut lists: [Vec<(u32, u32)>; LISTS] = Default::default();

    for list in &mut lists {
        word_in(secd, at).filter(|v| *v == LIST_VERSION)?;

        let count = word_in(secd, at + 4)? as usize;

        at += 8;

        for _ in 0..count {
            list.push((word_in(secd, at)?, word_in(secd, at + 4)?));
            at += 8;
        }
    }

    let [first, weapons, armor, spells] = lists;

    (at == secd.len()).then(|| Allowed {
        first,
        weapons,
        armor,
        spells: spells
            .into_iter()
            .map(|(i, id)| (i, FourCc::from_word(id)))
            .collect(),
    })
}

/// Where a character's lists start: past `PrimaryStat`.
const LISTS_AT: usize = 0x6c;

/// Lists a character's `SECD` holds.
const LISTS: usize = 4;

/// The word each list opens with.
const LIST_VERSION: u32 = 1;

/// A character's four lists, each index and value pairs in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowed {
    /// `VehicleDesc`: empty on every HD character.
    pub first: Vec<(u32, u32)>,
    /// `AllowedWeapon`: an index into the weapon types.
    pub weapons: Vec<(u32, u32)>,
    /// `AllowedArmor`, the same over armour types.
    pub armor: Vec<(u32, u32)>,
    /// `AllowedSpells`: `DACT` ids.
    pub spells: Vec<(u32, FourCc)>,
}

/// A building's `<Game>` section. Sample counts in brackets.
#[derive(Debug, Clone, Copy)]
pub struct Building<'a>(&'a [u8]);

impl Building<'_> {
    pub fn dialog_id(&self) -> Option<FourCc> {
        id_field(self.0, 0x00)
    } // 90
    pub fn cost(&self) -> Option<u32> {
        field(self.0, 0x04)
    } // 46
    /// `UpgradeTo`: the building an upgrade turns it into. The upgrade reads
    /// it from the in-memory `BuildingDesc`.
    pub fn upgrade_to(&self) -> Option<FourCc> {
        id_field(self.0, 0x08)
    }
    /// `UpgradeFrom`: the building this one is an upgrade of. **Binary**: the
    /// XML importer stores it in the in-memory `BuildingDesc`; the panel's
    /// name walks it.
    pub fn upgrade_from(&self) -> Option<FourCc> {
        id_field(self.0, 0x0c)
    }
    /// `Multiplier`, an `f32`: each building of the same code the owner
    /// already has multiplies the price by it (`BuildingDesc`,
    /// building price). The Inn's is 1.1.
    pub fn multiplier(&self) -> Option<f32> {
        field(self.0, 0x10).map(f32::from_bits)
    }
    pub fn income_type(&self) -> Option<u32> {
        field(self.0, 0x14)
    } // 48
    /// Signed: some buildings cost upkeep rather than earning.
    pub fn income_amount(&self) -> Option<i32> {
        field(self.0, 0x18).map(u32::cast_signed)
    } // 36
    pub fn max_hp(&self) -> Option<u32> {
        field(self.0, 0x1c)
    } // 88
    pub fn max_guild_members(&self) -> Option<u32> {
        field(self.0, 0x20)
    } // 27
    pub fn sight_range(&self) -> Option<u32> {
        field(self.0, 0x24)
    } // 67
    pub fn gold(&self) -> Option<u32> {
        field(self.0, 0x2c)
    } // 22
    /// The name generator: `NM01` to `NM17`. `BuildingDesc`, read by
    /// `BuildingRec`.
    pub fn name_gen_type(&self) -> Option<FourCc> {
        id_field(self.0, 0x30)
    }
    pub fn help_id(&self) -> Option<FourCc> {
        id_field(self.0, 0x34)
    } // 83

    /// The `Flags` bitfield; bits differ from a character's.
    pub fn flags(&self) -> Option<u32> {
        field(self.0, 0x28)
    } // 117

    pub fn has(&self, flag: BuildingFlag) -> bool {
        bit(self.0, 0x28, flag as u32)
    }

    pub fn flag_names(&self) -> impl Iterator<Item = BuildingFlag> + '_ {
        BuildingFlag::ALL.into_iter().filter(|f| self.has(*f))
    }

    /// `Produces`: the characters it makes, in file order. Past the
    /// fixed fields two lists, each a word 1 and a count: `Item`
    /// (`BuildingDesc`), empty on every shipped building, then
    /// `Produces`, the codes (building xml read,
    /// building xml read). A guild recruits by index into it.
    ///
    /// **Corpus**: on every building of all four datasets `Item` is empty
    /// and `Produces` ends `SECD`; [`Unit::parse`] checks both.
    ///
    /// # Panics
    ///
    /// Never on a parsed unit's building.
    #[must_use]
    pub fn produces(&self) -> Vec<FourCc> {
        produced(self.0).unwrap_or_else(|| panic!("building lists past Unit::parse"))
    }
}

/// A building's `Produces` codes, or `None` unless an empty `Item` list
/// and then `Produces` end the bytes.
fn produced(secd: &[u8]) -> Option<Vec<FourCc>> {
    let opens = |at: usize| word_in(secd, at).filter(|v| *v == LIST_VERSION);

    opens(BUILDING_LISTS_AT)?;
    word_in(secd, BUILDING_LISTS_AT + 4).filter(|n| *n == 0)?;

    let at = BUILDING_LISTS_AT + 8;

    opens(at)?;

    let count = word_in(secd, at + 4)? as usize;
    let codes = (0..count)
        .map(|n| word_in(secd, at + 8 + 4 * n).map(FourCc::from_word))
        .collect::<Option<Vec<_>>>()?;

    (at + 8 + 4 * count == secd.len()).then_some(codes)
}

/// What a number field of `SECD` holds, and so which values fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Number {
    /// A count, price, time or index: 0 to `i32::MAX`, as the engine
    /// reads it into signed words.
    Count,
    /// A signed word: [`Building::income_amount`].
    Signed,
    /// An `f32`, finite and not negative: [`Building::multiplier`].
    Float,
}

/// A number field of `SECD`: its XML name, its offset, its kind. Each
/// offset is its reader's ([`Building`], [`Character`]); ids and flag
/// words are left out.
pub type NumberField = (&'static str, usize, Number);

/// A building's number fields.
pub const BUILDING_NUMBERS: [NumberField; 8] = [
    ("Cost", 0x04, Number::Count),
    ("Multiplier", 0x10, Number::Float),
    ("IncomeType", 0x14, Number::Count),
    ("IncomeAmount", 0x18, Number::Signed),
    ("MaxHP", 0x1c, Number::Count),
    ("MaxGuildMembers", 0x20, Number::Count),
    ("SightRange", 0x24, Number::Count),
    ("Gold", 0x2c, Number::Count),
];

/// A character's number fields. `AttackRange`'s two halves are named
/// as its XML attributes are.
pub const CHARACTER_NUMBERS: [NumberField; 23] = [
    ("Cost", 0x04, Number::Count),
    ("Experience", 0x08, Number::Count),
    ("MaxHP", 0x0c, Number::Count),
    ("SightRange", 0x10, Number::Count),
    ("Speed", 0x14, Number::Count),
    ("AttackRangeMin", 0x18, Number::Count),
    ("AttackRangeMax", 0x1c, Number::Count),
    ("Vitality", 0x20, Number::Count),
    ("Charisma", 0x24, Number::Count),
    ("Artifice", 0x28, Number::Count),
    ("WillPower", 0x2c, Number::Count),
    ("Intelligence", 0x30, Number::Count),
    ("Strength", 0x34, Number::Count),
    ("MagicResistance", 0x38, Number::Count),
    ("Attack", 0x3c, Number::Count),
    ("RangedAttack", 0x40, Number::Count),
    ("Parry", 0x44, Number::Count),
    ("Dodge", 0x48, Number::Count),
    ("WeaponBasicDamage", 0x4c, Number::Count),
    ("ArmorBasicDamage", 0x50, Number::Count),
    ("Gold", 0x54, Number::Count),
    ("RecruitDelay", 0x58, Number::Count),
    ("PrimaryStat", 0x68, Number::Count),
];

/// Where a building's lists start: past `HelpID`.
const BUILDING_LISTS_AT: usize = 0x38;

/// A building's fixed fields and its two list headers.
const BUILDING_FIXED: usize = BUILDING_LISTS_AT + 16;

/// An overlay's fixed fields: up to its `Flags` word.
const OVERLAY_FIXED: usize = 0x0c;

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(tag: [u8; 4], body: &'static [u8]) -> Part<'static> {
        Part {
            tag: FourCc(tag),
            version: 0,
            declared: body.len(),
            body,
            parts: Vec::new(),
        }
    }

    #[test]
    fn a_prim_shorter_than_its_engine_block_is_refused() {
        // A character's engine block ends at 0x20; eight bytes fall short.
        let data = Part {
            tag: FourCc::new(b"DATA"),
            version: 0,
            declared: 0,
            body: &[],
            parts: vec![
                leaf(*b"HEAD", b"AVB1\x03\0\0\0\0\0"),
                leaf(*b"PRIM", &[0; 8]),
            ],
        };

        assert!(Unit::parse(&data).is_err());
    }

    #[test]
    fn a_unit_without_a_prim_is_missing_it() {
        let data = Part {
            tag: FourCc::new(b"DATA"),
            version: 0,
            declared: 0,
            body: &[],
            parts: vec![leaf(*b"HEAD", b"AVB1\x03\0\0\0\0\0")],
        };

        assert!(matches!(
            Unit::parse(&data),
            Err(Error::Missing { tag }) if tag == FourCc::new(b"PRIM")
        ));
    }
}
