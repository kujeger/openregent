//! The attributes whose meaning is known, and the primary-stat codes.

/// An attribute whose meaning is known, by the key a save stores it under.
///
/// Most are matched against the `DUNT` descriptor's value; each of the ten
/// character stats is the unique best match for one field. Below 100% is
/// levelling.
///
/// Level, experience, purses and potions rest on one or two **Game** labels
/// each (weaker); [`Stat::ALL`]'s last column says which. **Binary**: the
/// hero panels read these keys ([`crate::view::panel::portrait::details`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stat {
    /// Current hit points, negative on something just killed.
    HitPoints,
    /// The maximum; an undamaged building sits at it.
    MaxHitPoints,
    SightRange,
    /// A guild's roster limit.
    MaxGuildMembers,
    Strength,
    Intelligence,
    Artifice,
    Vitality,
    WillPower,
    Attack,
    RangedAttack,
    Parry,
    MagicResistance,
    Dodge,
    /// How much experience a level costs — the "of 2000" in "347 of 2000".
    ExperienceNeeded,
    /// Experience earned toward the next level.
    ExperienceEarned,
    Level,
    /// Gold carried that the crown has already taxed.
    TaxedGold,
    /// Untaxed gold: a hero's purse, a tax collector's load, a building's
    /// coffer. 696 of 1,707 corpus buildings hold some, up to 5,000.
    Gold,
    /// Absent rather than zero when none.
    HealingPotions,
    /// Weapon upgrades: the displayed "level" minus one. Absent when none.
    WeaponUpgrades,
    /// The same for armour.
    ArmorUpgrades,
    /// What the character is doing. See [`crate::data::text::Text::activity`].
    Activity,
    /// A tax collector's threshold for returning to the palace.
    TaxMinimumReturn,
    /// A tax collector's threshold for collecting at all.
    TaxMinimumPickup,
    Speed,
    AttackRangeMin,
    AttackRangeMax,
    WeaponDamage,
    ArmorDamage,
    /// The class's primary stat, a code; see [`PRIMARY_STATS`].
    ///
    /// Names hardcoded: game data has them only inside tooltips (`AP21`
    /// "Artifice - how stealthy this hero is").
    PrimaryStat,
    /// Whether Stoneskin is active, a boolean.
    ///
    /// **Game**: 1 under Stoneskin, 0 after. **Binary**: `HasEffectStoneSkin`,
    /// one of fifteen boolean `HasEffect*` attributes; see
    /// [`crate::data::attrib`]. `APV:40` in both games, but the expansion
    /// shifts its neighbours.
    StoneSkin,
}

impl Stat {
    /// Every stat, its key, and its evidence: the share of objects matching
    /// the descriptor, or the **Game** labels.
    pub const ALL: [(Stat, &'static [u8; 3], u8, &'static str); 32] = [
        (
            Stat::HitPoints,
            b"APU",
            0,
            "the field +0x58 was reading positionally",
        ),
        (Stat::MaxHitPoints, b"APU", 1, "97% of buildings"),
        (
            Stat::SightRange,
            b"APU",
            3,
            "100% of buildings, 93% of characters",
        ),
        (Stat::MaxGuildMembers, b"APB", 0, "98%"),
        (Stat::Strength, b"APV", 0, "79%"),
        (Stat::Intelligence, b"APV", 1, "76%"),
        (Stat::Artifice, b"APV", 2, "81%"),
        (Stat::Vitality, b"APV", 3, "79%"),
        (Stat::WillPower, b"APV", 4, "100%"),
        (Stat::Attack, b"APV", 5, "88%"),
        (Stat::RangedAttack, b"APV", 6, "89%"),
        (Stat::Parry, b"APV", 7, "78%"),
        (Stat::MagicResistance, b"APV", 8, "97%"),
        (Stat::Dodge, b"APV", 9, "78%"),
        (Stat::ExperienceNeeded, b"APV", 10, "100%"),
        (Stat::ExperienceEarned, b"APV", 16, "two labelled saves"),
        (Stat::Level, b"APV", 11, "two labelled saves"),
        (Stat::TaxedGold, b"APV", 24, "two labelled saves"),
        (Stat::Gold, b"APU", 2, "three labelled holders"),
        (Stat::HealingPotions, b"APV", 35, "one labelled save"),
        (Stat::WeaponUpgrades, b"APV", 19, "two labelled saves"),
        (Stat::ArmorUpgrades, b"APV", 22, "two labelled saves"),
        (Stat::Activity, b"APV", 30, "four labels over two heroes"),
        (
            Stat::TaxMinimumReturn,
            b"APV",
            48,
            "one labelled tax collector",
        ),
        (
            Stat::TaxMinimumPickup,
            b"APV",
            49,
            "one labelled tax collector",
        ),
        (Stat::StoneSkin, b"APV", 40, "one labelled pair"),
        (Stat::Speed, b"APV", 12, "100%"),
        (Stat::AttackRangeMin, b"APV", 13, "100%"),
        (Stat::AttackRangeMax, b"APV", 14, "95%"),
        (Stat::WeaponDamage, b"APV", 18, "100%"),
        (Stat::ArmorDamage, b"APV", 21, "98%"),
        (Stat::PrimaryStat, b"APV", 47, "100%"),
    ];

    /// The name and index this stat is stored under.
    ///
    /// # Panics
    ///
    /// Never: [`Stat::ALL`] holds every variant, which
    /// `every_stat_names_a_key` asserts.
    #[must_use]
    pub fn key(self) -> (&'static [u8; 3], u8) {
        let (_, name, index, _) = Self::ALL
            .iter()
            .find(|(stat, _, _, _)| *stat == self)
            .expect("every stat is in ALL");

        (name, *index)
    }
}

/// What [`Stat::PrimaryStat`] names.
///
/// **Game**, all sixteen player hero classes:
///
/// | value | stat | classes |
/// |---|---|---|
/// | 0 | Artifice | Rogue, Cultist, Elf |
/// | 1 | — | none |
/// | 2 | Intelligence | Healer, Paladin, Priestess, Wizard |
/// | 3 | Strength | Adept, Ranger, Solarus, Warrior, Dwarf |
/// | 4 | Vitality | Barbarian, Warrior of Discord |
/// | 5 | Will Power | Monk, Gnome, Gnomechamp |
///
/// Alphabetical, with 1 unused. Not an index into the descriptor's stat block.
///
/// Non-heroes also read 0, so check
/// [`crate::data::desc::Character::costs_gold`] first, or every monster
/// claims Artifice.
pub const PRIMARY_STATS: [(i32, Stat, &str); 5] = [
    (0, Stat::Artifice, "Artifice"),
    (2, Stat::Intelligence, "Intelligence"),
    (3, Stat::Strength, "Strength"),
    (4, Stat::Vitality, "Vitality"),
    (5, Stat::WillPower, "Will Power"),
];

/// Which stat a [`Stat::PrimaryStat`] code names. Only valid for recruitable
/// classes; see [`PRIMARY_STATS`].
#[must_use]
pub fn primary(code: i32) -> Option<Stat> {
    PRIMARY_STATS
        .iter()
        .find(|(value, _, _)| *value == code)
        .map(|(_, stat, _)| *stat)
}
