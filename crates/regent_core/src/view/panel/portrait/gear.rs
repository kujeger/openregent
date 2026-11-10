//! A hero's weapon and armour as its panels name and draw them.
//! **Binary** unless marked.
//!
//! Gear tables fills four maps at start-up, keyed by
//! the value of the descriptor's first `AllowedWeapon` or `AllowedArmor`
//! pair (`desc::Allowed`): a type's name table, an `EN` `STRT` in
//! `gpltext.cam`, and its picture's set.
//!
//! ```text
//!  weapon tooltip       EN table [APV:19, at most the last], then
//!                                   " (%d)" APV:18 + APV:19 when above 0,
//!                                   " +%d" APV:20 when above 0
//!  weapon picture       INBw set, frame APV:19
//!  armour tooltip       the same with APV:21, 22, 23
//!  armour picture       INBa set
//! ```
//!
//! An empty list names `GMTX` 350 "Bare Hands" or 322 "No Armor", and
//! draws set 1013 or 1004.
//!
//! A Blacksmith's panel shows its research instead ([`Forge`]):
//!
//! ```text
//!  smithy gear       7004 frame and 20 "+n": APB:28, 26,
//!                                   24 as 3, 2, 1; none hides both.
//!                                   7005 and 4001: APB:29, 27, 25
//! ```

use crate::data::desc::Character;
use crate::data::fourcc::FourCc;
use crate::data::text::{self, Ref, Text};
use crate::sim::world::{Piece, research};

/// Which of the two a [`Gear`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Worn {
    Weapon,
    Armour,
}

/// The weapon types' name tables, by `AllowedWeapon` value.
pub const WEAPON_NAMES: [(u32, [u8; 4]); 10] = [
    (0, *b"EN15"),
    (1, *b"EN06"),
    (2, *b"EN07"),
    (3, *b"EN08"),
    (4, *b"EN09"),
    (5, *b"EN10"),
    (6, *b"EN11"),
    (7, *b"EN12"),
    (8, *b"EN13"),
    (9, *b"EN05"),
];

/// The armour types' name tables, by `AllowedArmor` value.
pub const ARMOUR_NAMES: [(u32, [u8; 4]); 5] = [
    (0, *b"EN14"),
    (1, *b"EN02"),
    (2, *b"EN03"),
    (3, *b"EN04"),
    (4, *b"EN01"),
];

/// The weapon pictures' sets in [`WEAPON_RECORD`], by type.
pub const WEAPON_SETS: [(u32, u16); 9] = [
    (1, 1006),
    (2, 1007),
    (3, 1008),
    (4, 1009),
    (5, 1005),
    (6, 1012),
    (7, 1010),
    (8, 1011),
    (9, 1004),
];

/// The armour pictures' sets in [`ARMOUR_RECORD`], by type.
pub const ARMOUR_SETS: [(u32, u16); 4] = [(1, 1000), (2, 1001), (3, 1002), (4, 1003)];

/// The weapon pictures: the hero panels' controls 5001 and 7004 name it.
pub const WEAPON_RECORD: FourCc = FourCc(*b"INBw");

/// The armour pictures: controls 5004 and 7005.
pub const ARMOUR_RECORD: FourCc = FourCc(*b"INBa");

/// What an empty weapon list draws (`0x3f5`).
pub const NO_WEAPON_SET: u16 = 1013;

/// What an empty armour list draws (`0x3ec`).
pub const NO_ARMOUR_SET: u16 = 1004;

/// Frames in each [`WEAPON_RECORD`] and [`ARMOUR_RECORD`] set: levels 1
/// to 4, one picture in four colours. No set has a fifth.
pub const FRAMES: i32 = 4;

/// "Bare Hands": `GMTX` 350 (`0x15e`).
pub const NO_WEAPON_TEXT: Ref = Ref::gmtx(350);

/// "No Armor": `GMTX` 322 (`0x142`).
pub const NO_ARMOUR_TEXT: Ref = Ref::gmtx(322);

/// A weapon or armour as the hero panels draw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gear {
    pub worn: Worn,
    /// The value of the descriptor's first `AllowedWeapon` or
    /// `AllowedArmor` pair; `None` for an empty list.
    pub kind: Option<u32>,
    /// `APV:18` or `APV:21`, `Weapon_Basic_Damage`, `Armor_Basic_Damage`.
    pub basic: i32,
    /// Blacksmith upgrades, `APV:19` or `APV:22` (`Weapon_Struct_Bonus`,
    /// `Armor_Struct_Bonus`): the picture's frame and the name's index.
    /// `Upgrade_Equipment.gpl` sets 1 to 3.
    pub upgrades: i32,
    /// The magic bonus, `APV:20` or `APV:23`.
    pub enchant: i32,
}

impl Gear {
    /// The level the game names: one more than the upgrades.
    /// **Game**: two labelled saves (`crate::data::stat::Stat::WeaponUpgrades`); research
    /// `GMTX` 195 "Second Level Weapons" allows the first upgrade.
    #[must_use]
    pub const fn level(&self) -> i32 {
        self.upgrades + 1
    }

    /// The picture's frame as the engine draws it: [`Gear::upgrades`],
    /// unclamped, and widget draw
    /// draws 0 for one at or past [`FRAMES`]. So level 5, from a lair or
    /// a chest, shows level 1's picture. Below 0 is 0 here.
    #[must_use]
    pub const fn frame(&self) -> usize {
        if self.upgrades >= 0 && self.upgrades < FRAMES {
            self.upgrades.unsigned_abs() as usize
        } else {
            0
        }
    }

    /// Whether the level is past the pictures' frames (level 5).
    #[must_use]
    pub const fn past_frames(&self) -> bool {
        self.upgrades >= FRAMES
    }

    /// The `kind` of `character`'s `worn` list: its first pair's value.
    #[must_use]
    pub fn kind_of(character: &Character<'_>, worn: Worn) -> Option<u32> {
        let allowed = character.allowed();
        let list = match worn {
            Worn::Weapon => allowed.weapons,
            Worn::Armour => allowed.armor,
        };

        list.first().map(|&(_, value)| value)
    }

    /// "Longsword (12) +1": the type's name at the upgrade, the damage
    /// or armour value, the bonus. `None` where the tables lack it.
    ///
    /// Upgrades past the table name its last; below 0 none, as
    /// weapon tooltip clamps only the top.
    #[must_use]
    pub fn name(&self, text: &Text) -> Option<String> {
        let (names, none) = match self.worn {
            Worn::Weapon => (&WEAPON_NAMES[..], NO_WEAPON_TEXT),
            Worn::Armour => (&ARMOUR_NAMES[..], NO_ARMOUR_TEXT),
        };
        let base = match self.kind {
            None => text::plain(&text.get(none)?).into_owned(),
            Some(kind) => {
                let (_, id) = names.iter().find(|(k, _)| *k == kind)?;
                let count = text.table(id)?.len();
                let last = i32::try_from(count).ok()? - 1;
                let at = usize::try_from(self.upgrades.min(last)).ok()?;

                text::plain(&text.at(id, at)?).trim().to_owned()
            }
        };

        Some(format!("{base}{}", self.suffix()))
    }

    /// " (12) +1", " (12)", " +1" or nothing.
    fn suffix(&self) -> String {
        let total = self.basic + self.upgrades;

        match (total > 0, self.enchant > 0) {
            (true, true) => format!(" ({total}) +{}", self.enchant),
            (true, false) => format!(" ({total})"),
            (false, true) => format!(" +{}", self.enchant),
            (false, false) => String::new(),
        }
    }

    /// The picture's record and set; its frame is [`Gear::upgrades`].
    /// `None` for a type the start-up maps leave out.
    #[must_use]
    pub fn picture(&self) -> Option<(FourCc, u16)> {
        let (record, sets, none) = match self.worn {
            Worn::Weapon => (WEAPON_RECORD, &WEAPON_SETS[..], NO_WEAPON_SET),
            Worn::Armour => (ARMOUR_RECORD, &ARMOUR_SETS[..], NO_ARMOUR_SET),
        };
        let set = match self.kind {
            None => Some(none),
            Some(kind) => sets.iter().find(|(k, _)| *k == kind).map(|(_, s)| *s),
        };

        set.map(|s| (record, s))
    }
}

/// The Blacksmith's panel, `BlacksmithController`'s.
pub const FORGE_PANEL: FourCc = FourCc(*b"AP02");

/// The Blacksmith's weapon picture and its "+n": controls 7004 and 20.
pub const FORGE_WEAPON: (i32, i32) = (7004, 20);

/// The armour's: controls 7005 and 4001.
pub const FORGE_ARMOUR: (i32, i32) = (7005, 4001);

/// What a Blacksmith's panel shows of its research, as
/// smithy gear reads it: weapon and armour each
/// the highest level researched, 1 to 3, its picture's frame and its
/// "+n" (`GMTX` 191); `None`, nothing researched, hides both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Forge {
    pub weapon: Option<i32>,
    pub armour: Option<i32>,
}

impl Forge {
    /// Blacksmith `piece`'s.
    #[must_use]
    pub fn of(piece: &Piece) -> Forge {
        // The engine asks level 4 first, then 3, then 2.
        let level = |ids: [u32; 3]| {
            ids.iter()
                .rposition(|id| research::done(piece, *id))
                .and_then(|n| i32::try_from(n + 1).ok())
        };

        Forge {
            weapon: level(research::WEAPON_LEVELS),
            armour: level(research::ARMOUR_LEVELS),
        }
    }

    /// Level `level`'s "+2": `GMTX` 191.
    #[must_use]
    pub fn bonus(text: &Text, level: i32) -> Option<String> {
        Some(text::printf(
            &text.get(super::details::BONUS_TEXT)?,
            &[level.into()],
        ))
    }
}

/// The wizards' guild's panel, `WizardsGuildController`'s: its staff,
/// control 7004, and [`STAFF_BONUS`].
pub const STAFF_PANEL: FourCc = FourCc(*b"AP53");

/// The staff's "+n" box.
pub const STAFF_BONUS: i32 = 17;

/// The "+n" a wizards' guild's staff shows, as
/// wizards guild staff reads it: its code's last
/// byte `2` 2, `3` 3, else 1. **Inferred**: the staff's own frame, the
/// picture item given 0, stays 0.
#[must_use]
pub const fn staff(code: FourCc) -> i32 {
    match code.0[3] {
        b'2' => 2,
        b'3' => 3,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wizards_guilds_staff_is_its_level() {
        assert_eq!(staff(FourCc(*b"ABY1")), 1);
        assert_eq!(staff(FourCc(*b"ABY2")), 2);
        assert_eq!(staff(FourCc(*b"ABY3")), 3);
    }

    /// A Blacksmith with researches `done`.
    fn smithy(done: &[u32]) -> Piece {
        Piece {
            attributes: done
                .iter()
                .map(|&id| (research::attribute(id).packed(), 1))
                .collect(),
            ..Piece::default()
        }
    }

    #[test]
    fn a_forge_with_nothing_researched_shows_no_gear() {
        let none = Forge {
            weapon: None,
            armour: None,
        };

        assert_eq!(Forge::of(&smithy(&[])), none);
    }

    #[test]
    fn a_forge_shows_its_highest_level_researched() {
        let forge = Forge::of(&smithy(&research::WEAPON_LEVELS[..2]));

        assert_eq!(forge.weapon, Some(2));
        assert_eq!(forge.armour, None);
        assert_eq!(
            Forge::of(&smithy(&[research::ARMOUR_LEVELS[2]])).armour,
            Some(3)
        );
    }

    fn sword(upgrades: i32, enchant: i32) -> Gear {
        Gear {
            worn: Worn::Weapon,
            kind: Some(6),
            basic: 10,
            upgrades,
            enchant,
        }
    }

    #[test]
    fn a_gears_level_is_one_more_than_its_upgrades() {
        assert_eq!(sword(2, 0).level(), 3);
    }

    #[test]
    fn a_frame_past_the_sets_last_draws_frame_0() {
        assert_eq!(sword(3, 0).frame(), 3);
        assert_eq!(sword(4, 0).frame(), 0);
        assert_eq!(sword(-1, 0).frame(), 0);
        assert!(sword(4, 0).past_frames());
        assert!(!sword(3, 0).past_frames());
    }

    #[test]
    fn the_suffix_shows_the_total_and_any_bonus() {
        assert_eq!(sword(0, 0).suffix(), " (10)");
        assert_eq!(sword(2, 1).suffix(), " (12) +1");

        let bare = Gear {
            basic: 0,
            ..sword(0, 2)
        };

        assert_eq!(bare.suffix(), " +2");
        assert_eq!(Gear { enchant: 0, ..bare }.suffix(), "");
    }

    #[test]
    fn an_empty_list_draws_its_own_picture() {
        let none = Gear {
            kind: None,
            ..sword(0, 0)
        };

        assert_eq!(none.picture(), Some((WEAPON_RECORD, NO_WEAPON_SET)));
        assert_eq!(sword(0, 0).picture(), Some((WEAPON_RECORD, 1012)));
    }
}
