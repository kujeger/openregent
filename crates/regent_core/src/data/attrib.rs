//! The engine's own name for an attribute a save stores by number.
//!
//! A save's property list is `[three-byte bank][index][u32]`: `APV:9` on a
//! hero, `APU:0` on anything with hit points. Both halves of the key are copied
//! from the executable.
//!
//! ## The binary registers every attribute by bank and index
//!
//! One straight-line function, one block per attribute, bank and index as
//! immediates:
//!
//! ```text
//! the name     "ATTRIB_AgentID"
//! the index    0
//! the bank     one character per argument, backwards: 0, 'E', 'A'
//! ```
//!
//! That block is `AE:0` `AgentID`. The same scan yields 178 in the base game
//! and 207 in the expansion, one call site per string: [`EXPANSION`], in
//! file order. No test reruns the scan; `tests/attrib.rs` checks the table
//! against the saves.
//!
//! ## A bank is a class
//!
//! The strings run class by class; a bank is one class's numbering:
//!
//! | Bank | Whose attributes | Base | Expansion |
//! |---|---|---|---|
//! | `AE` | an agent: its id, its environmental sounds | 5 | 5 |
//! | `AP` | an agent's name, rubble and controller | 3 | 3 |
//! | `APR` | a reward flag | 3 | 3 |
//! | `APB` | a building: stages, tax, repair, research, the fair | 56 | 56 |
//! | `APV` | a vehicle, the engine's word for a moving thing, so the hero statistics | 66 | 74 |
//! | `APM` | a message | 3 | 3 |
//! | `APU` | a unit: hit points, gold, sight, clones | 15 | 15 |
//! | `LUC` | a lair: what it spawns and how fast | 11 | 11 |
//! | `APP` | the player — `APP:0` `TotalGold` is the treasury | 2 | 5 |
//! | `XSC` | the script callbacks a GPL agent can be woken by | 14 | 15 |
//! | `AZ0`, `ABB`, `ARB`, `AEM`, `ASP`, `ASS` | the expansion's zoo, bazaar, embassy and Sorcerer's Abode | — | 17 |
//!
//! `AP` is the project's initials: PDB paths give `Source\AP\`, also the
//! source of `SMNU` panel ids `AP##`.
//!
//! Consequences:
//!
//! - Indices do not move between editions. The expansion adds 29 (the table's
//!   difference); its eight mid-run inserts fill `APV:54` to `APV:61`, empty
//!   in the base game, so [`name`] answers from the expansion's superset.
//! - A bank is not contiguous: `APB` skips 38, `APM` runs 0, 32, 64, and `AP`
//!   has no index 0.
//! - `APB:56` is `ButtonStartTime`, the last of the building bank.
//!
//! ## What it agrees with
//!
//! The 32 [`Stat::ALL`] keys, found from save data alone, match this table
//! (test `the_engines_names_agree_with_the_joins`). Engine names differ:
//! `Attack` is `HtoH`, `ExperienceNeeded` `LevelXP`, `TaxedGold` `StoredGold`,
//! `Activity` `AIIntentionString`.
//!
//! Exactly ten `MagDelta*` modifiers, `APV:100` to `APV:109`, mirror the ten
//! statistics at `APV:0` to `APV:9`. So only base statistics take modifiers;
//! no `APV:121` backs a Ring of Protection's two armour points.
//!
//! [`Stat::ALL`]: crate::data::stat::Stat::ALL
//!
//! ## What is still not named
//!
//! `INV:n` is an inventory slot, not an attribute; its index names the item in
//! `STRT QITM`. Across the 65-save corpus's 234,802 property entries, `INV:8`
//! to `INV:12` are the only unresolved keys; the other 113 resolve.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One attribute: the bank and index a save writes it under, and its name.
///
/// The bank is three bytes, zero-padded: `AE` is `b"AE\0"`.
pub type Attrib = (&'static [u8; 3], u8, &'static str);

/// An attribute's key: bank and index. Saved as the pair `(bank, index)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(from = "([u8; 3], u8)", into = "([u8; 3], u8)")]
pub struct Key {
    pub bank: [u8; 3],
    pub index: u8,
}

impl Key {
    #[must_use]
    pub const fn new(bank: [u8; 3], index: u8) -> Self {
        Key { bank, index }
    }

    /// The key as a property list packs it: the bank's bytes, then the
    /// index, little-endian.
    #[must_use]
    pub const fn packed(self) -> i32 {
        let Key { bank, index } = self;
        i32::from_le_bytes([bank[0], bank[1], bank[2], index])
    }
}

impl From<([u8; 3], u8)> for Key {
    fn from((bank, index): ([u8; 3], u8)) -> Self {
        Key { bank, index }
    }
}

impl From<Key> for ([u8; 3], u8) {
    fn from(Key { bank, index }: Key) -> Self {
        (bank, index)
    }
}

// The keys the engine code names, by bank, then index. Each is the
// attribute its name says (test `the_named_keys_are_their_attributes`).

/// `APB:0 MaxGuildMembers`.
pub const MAX_GUILD_MEMBERS: Key = Key::new(*b"APB", 0);

/// `APB:1 FirstStageBuilt`: 0 while a building is a site.
pub const FIRST_STAGE_BUILT: Key = Key::new(*b"APB", 1);

/// `APB:2 CurrentStageBuilt`: 0 while an upgrade is built.
pub const CURRENT_STAGE_BUILT: Key = Key::new(*b"APB", 2);

/// `APB:4 EnchantmentLevel`.
pub const ENCHANTMENT_LEVEL: Key = Key::new(*b"APB", 4);

/// `APB:5 CurrentEvent`.
pub const CURRENT_EVENT: Key = Key::new(*b"APB", 5);

/// `APB:10 CheatWizGuildRange`, on the palace: every unit in reach.
pub const CHEAT_WIZ_GUILD_RANGE: Key = Key::new(*b"APB", 10);

/// `APB:12 PendingUnitName`: the name a recruit's hero takes, drawn at its
/// decode.
pub const PENDING_UNIT_NAME: Key = Key::new(*b"APB", 12);

/// `APB:13 ButtonDelayTime`, the key `BuildingIsRecruiting` reads:
/// `0xd425041` in gpl building is recruiting, default 0. Not 0 while a recruit or research
/// runs.
pub const BUTTON_DELAY_TIME: Key = Key::new(*b"APB", 13);

/// `APB:14 RecruitType`: the recruit's descriptor.
pub const RECRUIT_TYPE: Key = Key::new(*b"APB", 14);

/// `APB:16 IsTaxed`.
pub const IS_TAXED: Key = Key::new(*b"APB", 16);

/// `APB:17 QuickTax`.
pub const QUICK_TAX: Key = Key::new(*b"APB", 17);

/// `APB:18 Upgrade_HerosNeeded`: what upgrade status asks of the roster.
pub const UPGRADE_HEROS_NEEDED: Key = Key::new(*b"APB", 18);

/// `APB:20 IsRepaired`.
pub const IS_REPAIRED: Key = Key::new(*b"APB", 20);

/// `APB:21 QuickRepair`.
pub const QUICK_REPAIR: Key = Key::new(*b"APB", 21);

/// `APB:45 EnchantDuration`: holds the enchantment's start, the clock.
pub const ENCHANT_DURATION: Key = Key::new(*b"APB", 45);

/// `APB:54 MagicalBirth`: a building raised by magic.
pub const MAGICAL_BIRTH: Key = Key::new(*b"APB", 54);

/// `APB:56 ButtonStartTime`: the clock at a recruit's or research's decode.
pub const BUTTON_START_TIME: Key = Key::new(*b"APB", 56);

/// `APV:11 ExperienceLevel`: a hero's level (`0xb565041`).
pub const EXPERIENCE_LEVEL: Key = Key::new(*b"APV", 11);

/// `APV:27 PoisonCounter`.
pub const POISON_COUNTER: Key = Key::new(*b"APV", 27);

/// `APV:28 HasBeenDismissed`: hides the activity.
pub const HAS_BEEN_DISMISSED: Key = Key::new(*b"APV", 28);

/// `APV:37 HasEffectInvisibility`: no spell but the healings and their like
/// finds an invisible unit (`0x25565041`).
pub const HAS_EFFECT_INVISIBILITY: Key = Key::new(*b"APV", 37);

/// `APV:44 HasEffectMagicMirror`: a target holding it refuses another
/// player's spell and turns a missile (`0x2c565041` at
/// missile impact).
pub const HAS_EFFECT_MAGIC_MIRROR: Key = Key::new(*b"APV", 44);

/// `APV:46 HasEffectIllusionaryHero`: set on the clone; an
/// illusion never heals.
pub const HAS_EFFECT_ILLUSIONARY_HERO: Key = Key::new(*b"APV", 46);

/// `APV:47 PrimaryStat`: `HeroAttribController` lights
/// one row's star by it (`crate::data::stat::PRIMARY_STATS`).
pub const PRIMARY_STAT: Key = Key::new(*b"APV", 47);

/// `APV:48 ReturnAmount`.
pub const RETURN_AMOUNT: Key = Key::new(*b"APV", 48);

/// `APV:49 PickupAmount`.
pub const PICKUP_AMOUNT: Key = Key::new(*b"APV", 49);

/// `APU:0 HP`.
pub const HP: Key = Key::new(*b"APU", 0);

/// `APU:1 MaxHP`: `BuildingRec` sets it from `BuildingDesc`.
pub const MAX_HP: Key = Key::new(*b"APU", 1);

/// `APU:2 Gold`.
pub const GOLD: Key = Key::new(*b"APU", 2);

/// `APU:4 NameGenTable`: the generator a name is spelled with, written only
/// by `ChangeUnitType` to keep
/// the old descriptor's `NameGenType` (`BuildingDesc`, `VehicleDesc`).
pub const NAME_GEN_TABLE: Key = Key::new(*b"APU", 4);

/// `APU:7 HasEffectAntiMagic`: anti-magic shield's own refusal.
pub const HAS_EFFECT_ANTI_MAGIC: Key = Key::new(*b"APU", 7);

/// `APU:11 StartedWithThisUnit`: a start hero counts as none alive, and is
/// no champion.
pub const STARTED_WITH_THIS_UNIT: Key = Key::new(*b"APU", 11);

/// `APU:14 InhibitSelectionSound`.
pub const INHIBIT_SELECTION_SOUND: Key = Key::new(*b"APU", 14);

/// `LUC:8 Lair_BaseSpawnRate`.
pub const LAIR_BASE_SPAWN_RATE: Key = Key::new(*b"LUC", 8);

/// `LUC:10 Lair_History_Mod`.
pub const LAIR_HISTORY_MOD: Key = Key::new(*b"LUC", 10);

/// `APP:0 TotalGold`: the player's treasury.
pub const TOTAL_GOLD: Key = Key::new(*b"APP", 0);

/// `ASS:0 SorcerersAbodeSpellStartTime`: the clock at the cast. The fast bar
/// sends no Abode spell while it is set.
pub const SORCERERS_ABODE_SPELL_START_TIME: Key = Key::new(*b"ASS", 0);

/// `APU:3 SightRange`.
pub const SIGHT_RANGE: Key = Key::new(*b"APU", 3);

/// `APU:9 NotFlaggable`.
pub const NOT_FLAGGABLE: Key = Key::new(*b"APU", 9);

/// `APU:10 NotSpellTarget`.
pub const NOT_SPELL_TARGET: Key = Key::new(*b"APU", 10);

/// `APV:0 Strength`.
pub const STRENGTH: Key = Key::new(*b"APV", 0);

/// `APV:1 Intelligence`.
pub const INTELLIGENCE: Key = Key::new(*b"APV", 1);

/// `APV:2 Artifice`.
pub const ARTIFICE: Key = Key::new(*b"APV", 2);

/// `APV:3 Vitality`.
pub const VITALITY: Key = Key::new(*b"APV", 3);

/// `APV:4 WillPower`.
pub const WILL_POWER: Key = Key::new(*b"APV", 4);

/// `APV:5 HtoH`.
pub const HTOH: Key = Key::new(*b"APV", 5);

/// `APV:6 Ranged`.
pub const RANGED: Key = Key::new(*b"APV", 6);

/// `APV:7 Parry`.
pub const PARRY: Key = Key::new(*b"APV", 7);

/// `APV:8 MagicResistance`.
pub const MAGIC_RESISTANCE: Key = Key::new(*b"APV", 8);

/// `APV:9 Dodge`.
pub const DODGE: Key = Key::new(*b"APV", 9);

/// `APV:10 LevelXP`.
pub const LEVEL_XP: Key = Key::new(*b"APV", 10);

/// `APV:12 Speed`.
pub const SPEED: Key = Key::new(*b"APV", 12);

/// `APV:13 MinAttackRange`.
pub const MIN_ATTACK_RANGE: Key = Key::new(*b"APV", 13);

/// `APV:14 MaxAttackRange`.
pub const MAX_ATTACK_RANGE: Key = Key::new(*b"APV", 14);

/// `APV:15 Team`.
pub const TEAM: Key = Key::new(*b"APV", 15);

/// `APV:16 Experience`.
pub const EXPERIENCE: Key = Key::new(*b"APV", 16);

/// `APV:17 Charisma`.
pub const CHARISMA: Key = Key::new(*b"APV", 17);

/// `APV:18 Weapon_Basic_Damage`.
pub const WEAPON_BASIC_DAMAGE: Key = Key::new(*b"APV", 18);

/// `APV:21 Armor_Basic_Damage`.
pub const ARMOR_BASIC_DAMAGE: Key = Key::new(*b"APV", 21);

/// `APV:25 WeaponTypeIndex`.
pub const WEAPON_TYPE_INDEX: Key = Key::new(*b"APV", 25);

/// `APV:26 ArmorTypeIndex`.
pub const ARMOR_TYPE_INDEX: Key = Key::new(*b"APV", 26);

/// `APV:29 WeaponPoisoned`.
pub const WEAPON_POISONED: Key = Key::new(*b"APV", 29);

/// `APB:3 TaxRate`.
pub const TAX_RATE: Key = Key::new(*b"APB", 3);

/// `key`'s value in packed `attributes`; `None` where unset.
#[must_use]
pub fn get(attributes: &BTreeMap<i32, i32>, key: Key) -> Option<i32> {
    attributes.get(&key.packed()).copied()
}

/// What the engine calls the attribute a save stores as `bank:index`.
///
/// Answered from [`EXPANSION`], a superset of the base game's keys.
///
/// ```
/// use regent_core::data::attrib;
///
/// assert_eq!(attrib::name(*b"APV", 30), Some("AIIntentionString"));
/// assert_eq!(attrib::name(*b"APV", 107), Some("MagDeltaParry"));
/// assert_eq!(attrib::name(*b"APU", 0), Some("HP"));
/// assert_eq!(attrib::name(*b"AE\0", 0), Some("AgentID"));
/// assert_eq!(attrib::name(*b"APB", 56), Some("ButtonStartTime"));
///
/// // Not an attribute, and gaps in a bank stay gaps.
/// assert_eq!(attrib::name(*b"INV", 11), None);
/// assert_eq!(attrib::name(*b"APB", 38), None);
/// ```
#[must_use]
pub fn name(bank: [u8; 3], index: u8) -> Option<&'static str> {
    EXPANSION
        .iter()
        .find(|&&(held, at, _)| *held == bank && at == index)
        .map(|&(_, _, name)| name)
}

/// The key of the attribute the engine calls `name`, in any case: [`name`]
/// backwards. Names are unique in any case.
///
/// ```
/// use regent_core::data::attrib;
///
/// assert_eq!(attrib::key_of("hp"), Some(attrib::Key::new(*b"APU", 0)));
/// assert_eq!(attrib::key_of("ExperienceLevel"), Some(attrib::EXPERIENCE_LEVEL));
/// assert_eq!(attrib::key_of("Health"), None);
/// ```
#[must_use]
pub fn key_of(name: &str) -> Option<Key> {
    EXPANSION
        .iter()
        .find(|&&(_, _, held)| held.eq_ignore_ascii_case(name))
        .map(|&(bank, index, _)| Key::new(*bank, index))
}

/// The expansion's attributes: the base game's 178 and 29 more, in the
/// binary's string order. [`name`] answers from it.
pub const EXPANSION: [Attrib; 207] = [
    (b"AE\0", 0, "AgentID"),
    (b"AE\0", 1, "PlayingEnvironmentalSound1"),
    (b"AE\0", 2, "PlayingEnvironmentalSound2"),
    (b"AE\0", 3, "PlayingEnvironmentalSound3"),
    (b"AE\0", 4, "PlayingEnvironmentalSound4"),
    (b"AP\0", 1, "NameComponents"),
    (b"AP\0", 2, "RubbleCleanupCount"),
    (b"AP\0", 3, "Controller"),
    (b"APR", 0, "FlagSeekers"),
    (b"APR", 1, "RewardCost"),
    (b"APR", 2, "TargetID"),
    (b"APB", 0, "MaxGuildMembers"),
    (b"APB", 1, "FirstStageBuilt"),
    (b"APB", 2, "CurrentStageBuilt"),
    (b"APB", 3, "TaxRate"),
    (b"APB", 4, "EnchantmentLevel"),
    (b"APB", 5, "CurrentEvent"),
    (b"APB", 6, "MaxContestant"),
    (b"APB", 7, "RageOfKrolmCount"),
    (b"APB", 8, "GravesIndex"),
    (b"APB", 9, "SuperChargeActive"),
    (b"APB", 10, "CheatWizGuildRange"),
    (b"APB", 11, "ForceBuildingState"),
    (b"APB", 12, "PendingUnitName"),
    (b"APB", 13, "ButtonDelayTime"),
    (b"APB", 14, "RecruitType"),
    (b"APB", 15, "RecruitFlags"),
    (b"APB", 16, "IsTaxed"),
    (b"APB", 17, "QuickTax"),
    (b"APB", 18, "Upgrade_HerosNeeded"),
    (b"APB", 19, "Upgrade_MemberLevelNeeded"),
    (b"APB", 20, "IsRepaired"),
    (b"APB", 21, "QuickRepair"),
    (b"APB", 22, "MarketTime"),
    (b"APB", 23, "ResearchNotCompleted"),
    (b"APB", 24, "ResearchWeaponLevel_2"),
    (b"APB", 25, "ResearchArmorLevel_2"),
    (b"APB", 26, "ResearchWeaponLevel_3"),
    (b"APB", 27, "ResearchArmorLevel_3"),
    (b"APB", 28, "ResearchWeaponLevel_4"),
    (b"APB", 29, "ResearchArmorLevel_4"),
    (b"APB", 30, "ResearchTournament"),
    (b"APB", 31, "ResearchWizardFireBlast"),
    (b"APB", 32, "ResearchMagicResistance"),
    (b"APB", 33, "ResearchTrainIntelligence"),
    (b"APB", 34, "ResearchWizardMeteorStorm"),
    (b"APB", 35, "ResearchEnergyBlast"),
    (b"APB", 36, "ResearchFireShield"),
    (b"APB", 37, "ResearchMarketDay"),
    (b"APB", 39, "ResearchHealingPotions"),
    (b"APB", 40, "ResearchRingsofProtection"),
    (b"APB", 41, "ResearchPowerfulItem"),
    (b"APB", 42, "ResearchArrows"),
    (b"APB", 43, "ResearchGoodGuard"),
    (b"APB", 44, "CurrentResearchItem"),
    (b"APB", 45, "EnchantDuration"),
    (b"APB", 46, "GambleColor"),
    (b"APB", 47, "GambleAmount"),
    (b"APB", 48, "GambleClose"),
    (b"APB", 49, "GambleTime"),
    (b"APB", 50, "GambleWinner"),
    (b"APB", 51, "SuperChargeBeginTime"),
    (b"APB", 52, "RageBeginTime"),
    (b"APB", 53, "GambleWinnings"),
    (b"APB", 54, "MagicalBirth"),
    (b"APB", 55, "AlwaysView"),
    (b"APB", 56, "ButtonStartTime"),
    (b"APV", 0, "Strength"),
    (b"APV", 1, "Intelligence"),
    (b"APV", 2, "Artifice"),
    (b"APV", 3, "Vitality"),
    (b"APV", 4, "WillPower"),
    (b"APV", 5, "HtoH"),
    (b"APV", 6, "Ranged"),
    (b"APV", 7, "Parry"),
    (b"APV", 8, "MagicResistance"),
    (b"APV", 9, "Dodge"),
    (b"APV", 100, "MagDeltaStrength"),
    (b"APV", 101, "MagDeltaIntelligence"),
    (b"APV", 102, "MagDeltaArtifice"),
    (b"APV", 103, "MagDeltaVitality"),
    (b"APV", 104, "MagDeltaWillPower"),
    (b"APV", 105, "MagDeltaHtoH"),
    (b"APV", 106, "MagDeltaRanged"),
    (b"APV", 107, "MagDeltaParry"),
    (b"APV", 108, "MagDeltaMagicResistance"),
    (b"APV", 109, "MagDeltaDodge"),
    (b"APV", 10, "LevelXP"),
    (b"APV", 11, "ExperienceLevel"),
    (b"APV", 12, "Speed"),
    (b"APV", 13, "MinAttackRange"),
    (b"APV", 14, "MaxAttackRange"),
    (b"APV", 15, "Team"),
    (b"APV", 16, "Experience"),
    (b"APV", 17, "Charisma"),
    (b"APV", 18, "Weapon_Basic_Damage"),
    (b"APV", 19, "Weapon_Struct_Bonus"),
    (b"APV", 20, "Weapon_Magic_Bonus"),
    (b"APV", 21, "Armor_Basic_Damage"),
    (b"APV", 22, "Armor_Struct_Bonus"),
    (b"APV", 23, "Armor_Magic_Bonus"),
    (b"APV", 24, "StoredGold"),
    (b"APV", 25, "WeaponTypeIndex"),
    (b"APV", 26, "ArmorTypeIndex"),
    (b"APV", 27, "PoisonCounter"),
    (b"APV", 28, "HasBeenDismissed"),
    (b"APV", 29, "WeaponPoisoned"),
    (b"APV", 30, "AIIntentionString"),
    (b"APV", 31, "ContestantInFair"),
    (b"APV", 32, "ContestantRank"),
    (b"APV", 33, "HealingRateModifier"),
    (b"APV", 34, "MovementRateModifier"),
    (b"APV", 35, "NumHealingPotions"),
    (b"APV", 36, "HasEffectBlessing"),
    (b"APV", 37, "HasEffectInvisibility"),
    (b"APV", 38, "HasEffectPetrify"),
    (b"APV", 39, "HasEffectVines"),
    (b"APV", 40, "HasEffectStoneSkin"),
    (b"APV", 41, "HasEffectVigilance"),
    (b"APV", 42, "HasEffectWingedFeet"),
    (b"APV", 43, "HasEffectWither"),
    (b"APV", 44, "HasEffectMagicMirror"),
    (b"APV", 45, "HasEffectCamouflage"),
    (b"APV", 46, "HasEffectIllusionaryHero"),
    (b"APV", 47, "PrimaryStat"),
    (b"APV", 48, "ReturnAmount"),
    (b"APV", 49, "PickupAmount"),
    (b"APV", 50, "ActionRateModifier"),
    (b"APV", 51, "TurnRateModifier"),
    (b"APV", 52, "HasEffectParalyticGaze"),
    (b"APV", 53, "HasEffectLevelLeach"),
    (b"APV", 54, "HasEffectStrength"),
    (b"APV", 55, "HasEffectFireBalm"),
    (b"APV", 56, "HasEffectRegeneration"),
    (b"APV", 57, "HasEffectDamageShield"),
    (b"APV", 58, "HasEffectMedusa_Stare"),
    (b"APV", 59, "HasEffectMedusa_Slow"),
    (b"APV", 60, "HasEffectRatmanPlague"),
    (b"APV", 61, "StruckByChainLightning"),
    (b"APV", 62, "HasEffectAnimateBones"),
    (b"APV", 63, "IsFrozen"),
    (b"APM", 0, "Message_Text"),
    (b"APM", 32, "Message_Text_Replace_Int"),
    (b"APM", 64, "Message_Text_Replace_Str"),
    (b"APU", 0, "HP"),
    (b"APU", 1, "MaxHP"),
    (b"APU", 2, "Gold"),
    (b"APU", 3, "SightRange"),
    (b"APU", 4, "NameGenTable"),
    (b"APU", 5, "DialogID"),
    (b"APU", 6, "OriginalType"),
    (b"APU", 7, "HasEffectAntiMagic"),
    (b"APU", 8, "CloneParentID"),
    (b"APU", 9, "NotFlaggable"),
    (b"APU", 10, "NotSpellTarget"),
    (b"APU", 11, "StartedWithThisUnit"),
    (b"APU", 12, "WhichClone"),
    (b"APU", 13, "NumClones"),
    (b"APU", 14, "InhibitSelectionSound"),
    (b"LUC", 0, "Lair_Type1"),
    (b"LUC", 1, "Lair_Percent1"),
    (b"LUC", 2, "Lair_Type2"),
    (b"LUC", 3, "Lair_Percent2"),
    (b"LUC", 4, "Lair_Type3"),
    (b"LUC", 5, "Lair_Percent3"),
    (b"LUC", 6, "Lair_Type4"),
    (b"LUC", 7, "Lair_Percent4"),
    (b"LUC", 8, "Lair_BaseSpawnRate"),
    (b"LUC", 9, "Lair_Dispersion"),
    (b"LUC", 10, "Lair_History_Mod"),
    (b"APP", 0, "TotalGold"),
    (b"APP", 1, "ShareVisibility"),
    (b"APP", 2, "SpecialMonster"),
    (b"APP", 3, "SpecialMonsterDuration"),
    (b"APP", 4, "SpecialMonsterStart"),
    (b"XSC", 0, "SC_CHILD_CREATED"),
    (b"XSC", 1, "SC_CHILD_DESTROYED"),
    (b"XSC", 2, "SC_BLDG_TYPES_CHANGED"),
    (b"XSC", 3, "SC_BECAME_GRAVESTONE"),
    (b"XSC", 4, "SC_UNITTYPE_CHANGED"),
    (b"XSC", 5, "SC_UNIT_ENTERED"),
    (b"XSC", 6, "SC_UNIT_EXITED"),
    (b"XSC", 7, "SC_UNIT_DELETED"),
    (b"XSC", 8, "SC_UNIT_CREATED"),
    (b"XSC", 9, "SC_FRAME_TICK"),
    (b"XSC", 10, "SC_ITEM_LOST"),
    (b"XSC", 11, "SC_HERO_COUNT_CHANGED"),
    (b"XSC", 12, "SC_COMPLETED_CAMP_MOVE"),
    (b"XSC", 13, "SC_NAME_CHANGED"),
    (b"XSC", 14, "SC_MONSTER_SLAIN"),
    (b"ARB", 0, "ResearchBazaar_Item_One"),
    (b"ARB", 1, "ResearchBazaar_Item_Two"),
    (b"ABB", 2, "ResearchBazaar_Item_Three"),
    (b"ABB", 3, "ResearchBazaar_Item_Four"),
    (b"ABB", 4, "ResearchBazaar_Item_Five"),
    (b"ABB", 5, "ResearchBazaar_Item_Six"),
    (b"ARB", 10, "ResearchBazaar_Level_One"),
    (b"ARB", 11, "ResearchBazaar_Level_Two"),
    (b"ARB", 12, "ResearchBazaar_Level_Three"),
    (b"AZ0", 0, "Zoo_Legal_Target"),
    (b"ASS", 0, "SorcerersAbodeSpellStartTime"),
    (b"ASP", 0, "SorcerersAbodeSpellDuration"),
    (b"AEM", 1, "EmbassyRecruitCost"),
    (b"AEM", 2, "EmbassyTimerProgress"),
    (b"AEM", 3, "EmbassyTimerDuration"),
    (b"AEM", 4, "EmbassyActiveFlag"),
    (b"AEM", 5, "EmbassyHasSlot"),
];

/// The one name HD appends to [`EXPANSION`]: HD's 208 are the expansion's 207
/// then this. Comes with HD's eight added inventory-item builtins.
///
/// No key: HD (MSVC) registers it through a different helper without the
/// key's constant. Nothing in the corpus writes it.
pub const HD_EXTRA: &str = "CanDropItem";

#[cfg(test)]
mod tests {
    use super::*;

    /// Each named key is the attribute its name says.
    #[test]
    fn the_named_keys_are_their_attributes() {
        let named = [
            (MAX_GUILD_MEMBERS, "MaxGuildMembers"),
            (FIRST_STAGE_BUILT, "FirstStageBuilt"),
            (CURRENT_STAGE_BUILT, "CurrentStageBuilt"),
            (ENCHANTMENT_LEVEL, "EnchantmentLevel"),
            (CURRENT_EVENT, "CurrentEvent"),
            (CHEAT_WIZ_GUILD_RANGE, "CheatWizGuildRange"),
            (PENDING_UNIT_NAME, "PendingUnitName"),
            (BUTTON_DELAY_TIME, "ButtonDelayTime"),
            (RECRUIT_TYPE, "RecruitType"),
            (IS_TAXED, "IsTaxed"),
            (QUICK_TAX, "QuickTax"),
            (UPGRADE_HEROS_NEEDED, "Upgrade_HerosNeeded"),
            (IS_REPAIRED, "IsRepaired"),
            (QUICK_REPAIR, "QuickRepair"),
            (ENCHANT_DURATION, "EnchantDuration"),
            (MAGICAL_BIRTH, "MagicalBirth"),
            (BUTTON_START_TIME, "ButtonStartTime"),
            (EXPERIENCE_LEVEL, "ExperienceLevel"),
            (POISON_COUNTER, "PoisonCounter"),
            (HAS_BEEN_DISMISSED, "HasBeenDismissed"),
            (HAS_EFFECT_INVISIBILITY, "HasEffectInvisibility"),
            (HAS_EFFECT_MAGIC_MIRROR, "HasEffectMagicMirror"),
            (HAS_EFFECT_ILLUSIONARY_HERO, "HasEffectIllusionaryHero"),
            (PRIMARY_STAT, "PrimaryStat"),
            (RETURN_AMOUNT, "ReturnAmount"),
            (PICKUP_AMOUNT, "PickupAmount"),
            (HP, "HP"),
            (MAX_HP, "MaxHP"),
            (GOLD, "Gold"),
            (NAME_GEN_TABLE, "NameGenTable"),
            (HAS_EFFECT_ANTI_MAGIC, "HasEffectAntiMagic"),
            (STARTED_WITH_THIS_UNIT, "StartedWithThisUnit"),
            (INHIBIT_SELECTION_SOUND, "InhibitSelectionSound"),
            (LAIR_BASE_SPAWN_RATE, "Lair_BaseSpawnRate"),
            (LAIR_HISTORY_MOD, "Lair_History_Mod"),
            (TOTAL_GOLD, "TotalGold"),
            (
                SORCERERS_ABODE_SPELL_START_TIME,
                "SorcerersAbodeSpellStartTime",
            ),
        ];

        for (Key { bank, index }, want) in named {
            assert_eq!(name(bank, index), Some(want));
        }
    }
}
