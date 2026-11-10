//! What a selected unit's panels say besides its name, portrait and hit
//! points: a hero's level, gold, gear, spells and items, a tax collector's
//! or caravan's purse, a building's coffer. **Binary** unless marked.
//!
//! Each value is an attribute the panel's controller reads
//! (default 0 unless noted), keyed as a save writes it:
//!
//! ```text
//!  HeroController (AP20)
//!    7003   APV:11 level
//!    8030   AITX[APV:30], own undismissed hero
//!           else GMTX 85 "Follows %s." of GMTX 251
//!    8002, 4002, 4006, 8030  own undismissed hero only
//!    9901   banner, INDe 1010 + owner (APb5)
//!    5001   weapon picture, frame APV:19
//!    5002   "+%d" GMTX 191, APV:20 when not 0
//!    5004/5003  armour: APV:22, APV:23
//!  HeroAttribController (AP21)
//!    4016   bar, APV:16 (default 1, under 0 is 0) of APV:10 (default 2)
//!    4015   GMTX 8 "%d of %d xp", the same
//!    5002-5011  APV:0 to 9 (STAT_CONTROLS)
//!    26-30  the star of APV:47's primary stat
//!  HeroItemController (AP22)
//!    9000   APU:2 gold        9001 APV:24 taxed gold
//!    9002   APV:35 healing potions
//!    9005   list: each INV:n the unit holds, QITM[n]
//!    9003/9004  weapon and armour names (`gear`)
//!  HeroSpellsController, filling the hero spell list
//!    8730   each spell entry shown, CharacterLevel <= APV:11
//!    8731   each unit hung on it the switch names (`enchant`)
//!  TaxCollectorController (AP74)
//!    5000   APU:2
//!  CaravanController (AP90), on APU:2
//!    5000   APU:2
//!  MonsterController (AP97), 8014 hidden
//!    4001-4003  APV:8, 9, 7 rated, GMTX 245-249
//!  StatsWindowController, on any panel with 8014
//!    8014   APU:2 not 0: GMTX 76 "Coffers hold %d gold"
//!           0, IncomeType 3: blank; else GMTX 77 "Coffers hold no gold"
//!  Mausoleum_Controller, its own 8014
//!    8014   GMTX 185 "%d Heroes buried here", relation 2's count
//!  GraveyardController (AP15), no shared 8014 fill
//!    8014   the local player's: GMTX 185 of its hero list's length
//!           (World::buried); another's blank
//!    8004   the local player's only: opens AP16, its heroes
//!  SignPostController (APa6 chests, APa7 signs)
//!    8005   the first item key's name, to its newline; else the name
//!    8014   APM:0 not 0: AITX[APM:0]; else on APa6 APU:2 as
//!           GMTX 226 "%d Gold in Chest", 0 GMTX 233 "Empty"
//! ```
//!
//! The palace's panel, `AP39`, has no 8014: its gold is the treasury.

use crate::data::stat;
use crate::data::stat::Stat;
use crate::sim::world::piece_state;
use std::borrow::Cow;

use crate::content::unit::Unit;
use crate::data::attrib;
use crate::data::attrib::{HAS_BEEN_DISMISSED, Key, PRIMARY_STAT};
use crate::data::catalogue::Catalogue;
use crate::data::desc::{self, User};
use crate::data::desc::{Action, Part};
use crate::data::fourcc::FourCc;
use crate::data::smnu::{self, Panel};
use crate::data::text::{self, Ref, Text};
use crate::sim::world::{self, Piece, World, message::MESSAGE, players::INVENTORY};

use super::enchant::{self, Enchantment};
use super::gear::{Gear, Worn};

/// A hero's panel, its descriptor's `DialogID`: `HeroController`.
///
/// **Corpus**: every `CostsGold` character but one names it, and no other
/// unit does. **Inferred**: the dialog picks the controller.
pub const HERO_PANEL: FourCc = FourCc(*b"AP20");

/// `AVh1 Hooligan`.
const HOOLIGAN: FourCc = FourCc(*b"AVh1");

/// The coffer line, `0x1f4e` in `StatsWindowController`.
pub const COFFER_CONTROL: i32 = 8014;

/// `Coffers hold %d gold`, coloured: `GMTX` 76 (`0x4c`).
pub const COFFER_TEXT: Ref = Ref::gmtx(76);

/// `Coffers hold no gold`: `GMTX` 77 (`0x4d`).
pub const EMPTY_COFFER_TEXT: Ref = Ref::gmtx(77);

/// The `IncomeType` whose empty coffer shows nothing (a compare with 3):
/// towers, guardhouses, dwellings, the fountain.
pub const SILENT_INCOME: u32 = 3;

/// The Mausoleum's panel: `Mausoleum_Controller`. **Inferred**: the
/// dialog picks the controller; only the Mausoleum names it.
pub const MAUSOLEUM_PANEL: FourCc = FourCc(*b"MX04");

/// `%d Heroes buried here`: `GMTX` 185 (`0xb9`).
pub const BURIED_TEXT: Ref = Ref::gmtx(185);

/// A graveyard's panel: `GraveyardController` (controller factory,
/// a compare with `'AP15'` in controller factory).
pub const GRAVEYARD_PANEL: FourCc = FourCc(*b"AP15");

/// A graveyard's heroes: `GraveyardListController` (a compare with `'AP16'` at
/// controller factory), opened by `AP15`'s 8004.
pub const GRAVES_PANEL: FourCc = FourCc(*b"AP16");

/// A graveyard row with detail: `%s,\nLevel %d`, `GMTX` 221 (`0xdd`,
/// `GraveyardListController`), the hero's name and level.
pub const GRAVE_TEXT: Ref = Ref::gmtx(221);

/// A treasure chest's panel: `SignPostController`, as
/// controller factory makes it.
pub const CHEST_PANEL: FourCc = FourCc(*b"APa6");

/// The panel of a sign, banner, marker, obelisk, tablet or note stump:
/// `SignPostController` too. **Corpus**: the `BBs` codes
/// of the base game's and expansion's descriptors name it, no other.
pub const SIGN_PANEL: FourCc = FourCc(*b"APa7");

/// `%d Gold in Chest`: `GMTX` 226 (`0xe2`).
pub const CHEST_TEXT: Ref = Ref::gmtx(226);

/// `Empty`: `GMTX` 233 (`0xe9`).
pub const EMPTY_CHEST_TEXT: Ref = Ref::gmtx(233);

/// `%d of %d xp`: `GMTX` 8.
pub const EXPERIENCE_TEXT: Ref = Ref::gmtx(8);

/// `+%d`, an enchanted weapon's or armour's bonus: `GMTX` 191 (`0xbf`).
pub const BONUS_TEXT: Ref = Ref::gmtx(191);

/// `APV:16`'s default where the experience bar reads it (`0x01`).
pub const EARNED_DEFAULT: i32 = 1;

/// `APV:10`'s default there (`0x02`).
pub const NEEDED_DEFAULT: i32 = 2;

/// `APV:20`, `Weapon_Magic_Bonus`.
pub const WEAPON_MAGIC: Key = Key::new(*b"APV", 20);

/// `APV:23`, `Armor_Magic_Bonus`.
pub const ARMOUR_MAGIC: Key = Key::new(*b"APV", 23);

/// A hero's panel when it is not the player's own: `APb5`, also
/// `HeroController`'s. Name, portrait, hit
/// points, class, level and a "Follows %s." box; no gear, experience,
/// page buttons or dismiss. Selection panel of
/// gives it to a dismissed hero (`APV:28` not 0) and to another owner's
/// hero of a type `CanUse` `HumanPlayer`; `KillConfirmController`
/// opens it on YES. **Game**: `local/shots/ui-hero-dismissal-flow.png`.
pub const FOLLOWER_PANEL: FourCc = FourCc(*b"APb5");

/// Another player's building: selection panel of.
pub const FOREIGN_BUILDING_PANEL: FourCc = FourCc(*b"APb4");

/// Another player's henchman: selection panel of.
pub const FOREIGN_HENCHMAN_PANEL: FourCc = FourCc(*b"APc5");

/// `Belongs to %s.`: `GMTX` 84.
pub const BELONGS_TEXT: usize = 84;

/// The controls hero buttons shows only on the
/// player's own hero not dismissed: dismiss 8002, ITEMS 4002, STATS 4006,
/// and the activity 8030, which hero activity refills.
pub const OWN_CONTROLS: [i32; 4] = [8002, 4002, 4006, DOING_CONTROL];

/// The activity box.
pub const DOING_CONTROL: i32 = 8030;

/// `APb5`'s banner frame: `INDe` set 1010 plus the owner:
/// blue, red, green, purple.
pub const BANNER_CONTROL: i32 = 9901;
pub const BANNER: (FourCc, i32) = (FourCc(*b"INDe"), 1010);

/// `Follows %s.`: `GMTX` 85 (`0x55`).
pub const FOLLOWS_TEXT: usize = 85;

/// `another monarch`: `GMTX` 251 (`0xfb`), a dismissed hero's
/// leader, and in a one-player game any owner's but the player's.
pub const ANOTHER_MONARCH: usize = 251;

/// `the monsters`: `GMTX` 250.
pub const MONSTERS: usize = 250;

/// Another owner's building or character panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreign {
    Building(u32),
    Henchman(u32),
    Follower(u32),
}

impl Foreign {
    /// The other player's panel, when the original type permits human use;
    /// `local` the player who looks.
    #[must_use]
    pub fn of(
        catalogue: &Catalogue<'_>,
        unit: &desc::Unit<'_>,
        world: &World,
        agent: u32,
        local: u32,
    ) -> Option<Self> {
        let piece = world.piece(agent)?;
        let original = match read(piece, world::retype::ORIGINAL_TYPE, 0) {
            0 => None,
            code => Unit::descriptor(catalogue, FourCc::from_word(code.cast_unsigned()))
                .and_then(Result::ok),
        };
        let human = original.as_ref().unwrap_or(unit).can_use(User::HumanPlayer);
        let original_code = original.as_ref().unwrap_or(unit).head.id;
        let dialog = unit.character().and_then(|c| c.dialog_id());

        if piece.owner == local || !human {
            None
        } else if unit.building().is_some() {
            Some(Self::Building(piece.owner))
        } else if unit.character().is_none() || dialog == Some(HERO_PANEL) {
            None
        } else if original_code == HOOLIGAN || world.henchman(agent) {
            Some(Self::Henchman(piece.owner))
        } else if dialog.is_some_and(|id| [FourCc(*b"APb1"), FourCc(*b"APa8")].contains(&id)) {
            None
        } else {
            Some(Self::Follower(piece.owner))
        }
    }

    #[must_use]
    pub const fn panel(self) -> FourCc {
        match self {
            Self::Building(_) => FOREIGN_BUILDING_PANEL,
            Self::Henchman(_) => FOREIGN_HENCHMAN_PANEL,
            Self::Follower(_) => FOLLOWER_PANEL,
        }
    }

    /// The owner's `INDe` frame, set 1010 plus its player slot.
    #[must_use]
    pub fn banner(self) -> (FourCc, i32) {
        let owner = match self {
            Self::Building(owner) | Self::Henchman(owner) | Self::Follower(owner) => owner,
        };

        (
            BANNER.0,
            BANNER.1 + i32::try_from(owner).unwrap_or_default(),
        )
    }

    /// `APb4` says "Belongs to ..."; `APc5` says "Follows ...".
    #[must_use]
    pub fn line(self, text: &Text) -> Option<String> {
        let (owner, template) = match self {
            Self::Building(owner) if owner != piece_state::UNALIGNED && owner != UNOWNED => {
                (owner, BELONGS_TEXT)
            }
            Self::Building(_) => return None,
            Self::Henchman(owner) | Self::Follower(owner) => (owner, FOLLOWS_TEXT),
        };
        let name = text.gmtx(if owner == piece_state::UNALIGNED {
            MONSTERS
        } else {
            ANOTHER_MONARCH
        })?;
        let template = text.gmtx(template)?;

        Some(text::plain(&text::printf(&template, &[name.as_ref().into()])).into_owned())
    }
}

/// An owner whose hero shows no activity box: none (-1).
const UNOWNED: u32 = u32::MAX;

/// `APV:18`, `Weapon_Basic_Damage`.
pub const WEAPON_BASIC: Key = crate::data::attrib::WEAPON_BASIC_DAMAGE;

/// `APV:21`, `Armor_Basic_Damage`.
pub const ARMOUR_BASIC: Key = crate::data::attrib::ARMOR_BASIC_DAMAGE;

/// A monster's panel: `MonsterController`. **Inferred**: the dialog picks
/// the controller, as [`HERO_PANEL`]'s; every monster names it.
pub const MONSTER_PANEL: FourCc = FourCc(*b"AP97");

/// A caravan's panel: `CaravanController` (controller factory,
/// controller factory), which fills 5000 with `APU:2` as a tax collector's.
pub const CARAVAN_PANEL: FourCc = FourCc(*b"AP90");

/// A rating's words, `GMTX` 245 "Terrible (%d)" to 248 "Good (%d)", each
/// below its bound (monster rating compares with 0x14, 0x28,
/// 0x3c, 0x50).
pub const RATINGS: [(i32, usize); 4] = [(20, 245), (40, 246), (60, 247), (80, 248)];

/// The rating at and above the last bound: `GMTX` 249 "Excellent (%d)".
pub const TOP_RATING: usize = 249;

/// The rows `MonsterController` rates, each an attribute and its control:
/// `APV:8` `MagicResistance` 4001, `APV:9` `Dodge` 4002, `APV:7` `Parry`
/// ("Defense") 4003.
pub const RATED: [(Key, i32); 3] = [
    (Key::new(*b"APV", 8), 4001),
    (Key::new(*b"APV", 9), 4002),
    (Key::new(*b"APV", 7), 4003),
];

/// What a selected unit's panels show beyond hit points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Details {
    Hero(Hero),
    /// A tax collector's or caravan's load, `APU:2`.
    Purse(i32),
    Coffer(Coffer),
    /// A Mausoleum's interred, or the local player's graveyard's heroes,
    /// counted.
    Buried(usize),
    /// A sign's or chest's.
    Posted(Posted),
    /// A monster's ratings, in [`RATED`]'s order.
    Monster([i32; 3]),
}

impl Details {
    /// Agent `agent`'s, as the world keeps it, `local` the player who
    /// looks; `None` for a unit whose panel shows none of these.
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>, world: &World, agent: u32, local: u32) -> Option<Details> {
        let piece = world.piece(agent)?;
        let unit = Unit::descriptor(catalogue, piece.code)?.ok()?;
        let dialog = dialog(&unit)?;
        match dialog {
            HERO_PANEL => Some(Details::Hero(Hero::of(
                catalogue,
                &unit,
                piece,
                enchant::on(world, agent),
                local,
            ))),
            world::guild::taxman::TAX_DIALOG | CARAVAN_PANEL => {
                Some(Details::Purse(stat(piece, Stat::Gold, 0)))
            }
            MAUSOLEUM_PANEL => Some(Details::Buried(world.interred(agent).len())),
            GRAVEYARD_PANEL if piece.owner == local => {
                Some(Details::Buried(world.buried(agent).map_or(0, <[_]>::len)))
            }
            GRAVEYARD_PANEL => None,
            CHEST_PANEL | SIGN_PANEL => Some(Details::Posted(Posted::of(catalogue, piece, dialog))),
            MONSTER_PANEL => Some(Details::Monster(RATED.map(|(key, _)| read(piece, key, 0)))),
            _ if holds(catalogue, dialog, COFFER_CONTROL) => {
                Some(Details::Coffer(Coffer::of(&unit, piece)))
            }
            _ => None,
        }
    }
}

/// A hero's panels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hero {
    /// `APV:11`, `ExperienceLevel`.
    pub level: i32,
    pub experience: Experience,
    /// `APU:2`, untaxed.
    pub gold: i32,
    /// `APV:24`, `StoredGold`: gold the crown has taxed.
    pub taxed: i32,
    pub weapon: Gear,
    pub armour: Gear,
    /// `APV:35`, `NumHealingPotions`.
    pub potions: i32,
    /// `APV:30`, an `AITX` code: what the hero is doing. `None` unless the
    /// local player's and not dismissed; else [`Hero::follows`]. Not modelled:
    /// hiding it by the unit's state.
    pub activity: Option<i32>,
    /// `APV:28` set: dismissed, so its panel's dismiss, items and stats
    /// buttons hide.
    pub dismissed: bool,
    /// The owner.
    pub owner: u32,
    /// The local player's and not dismissed: its
    /// panel's own controls show.
    pub own: bool,
    /// [`HERO_PANEL`], or [`FOLLOWER_PANEL`].
    pub panel: FourCc,
    /// The activity box says "Follows another monarch.": not the
    /// player's undismissed hero, nor the monsters' or no one's.
    pub follows: bool,
    /// The spells the panel lists, in list order.
    pub spells: Vec<FourCc>,
    /// What enchants it: the spells page's second list.
    pub enchantments: Vec<Enchantment>,
    /// Each item key held, in key order (`items`), named by [`item`].
    pub items: Vec<i32>,
    /// `APV:0` to `APV:9`, the totals (`HeroAttribController`'s
    /// hero attrib text): strength, intelligence,
    /// artifice, vitality, will power, hand to hand, ranged, parry, magic
    /// resistance, dodge.
    pub stats: [i32; STATS],
    /// [`PRIMARY_STAT`]'s stat, starred.
    pub primary: Option<Stat>,
}

/// Rows [`Hero::stats`] holds.
pub const STATS: usize = 10;

/// The control each of [`Hero::stats`] fills, in order: `AP21`'s 5002 to
/// 5011, but resistance in 5011 and dodge in 5010 (`0x1393`, `0x1392`).
pub const STAT_CONTROLS: [i32; STATS] =
    [5002, 5003, 5004, 5005, 5006, 5007, 5008, 5009, 5011, 5010];

impl Hero {
    fn of(
        catalogue: &Catalogue<'_>,
        unit: &desc::Unit<'_>,
        piece: &Piece,
        enchantments: Vec<Enchantment>,
        local: u32,
    ) -> Hero {
        let get = |s: Stat| stat(piece, s, 0);
        let level = get(Stat::Level);
        let character = unit.character();
        let kind = |worn| character.as_ref().and_then(|c| Gear::kind_of(c, worn));
        let stats = std::array::from_fn(|n| {
            let n = u8::try_from(n).unwrap_or_default();

            read(piece, Key::new(*b"APV", n), 0)
        });

        let dismissed = read(piece, HAS_BEEN_DISMISSED, 0) != 0;
        let own = piece.owner == local && !dismissed;

        Hero {
            level,
            experience: Experience {
                earned: stat(piece, Stat::ExperienceEarned, EARNED_DEFAULT).max(0),
                needed: stat(piece, Stat::ExperienceNeeded, NEEDED_DEFAULT),
            },
            gold: get(Stat::Gold),
            taxed: get(Stat::TaxedGold),
            weapon: Gear {
                worn: Worn::Weapon,
                kind: kind(Worn::Weapon),
                basic: read(piece, WEAPON_BASIC, 0),
                upgrades: get(Stat::WeaponUpgrades),
                enchant: read(piece, WEAPON_MAGIC, 0),
            },
            armour: Gear {
                worn: Worn::Armour,
                kind: kind(Worn::Armour),
                basic: read(piece, ARMOUR_BASIC, 0),
                upgrades: get(Stat::ArmorUpgrades),
                enchant: read(piece, ARMOUR_MAGIC, 0),
            },
            potions: get(Stat::HealingPotions),
            activity: activity(piece, local),
            dismissed,
            owner: piece.owner,
            own,
            panel: panel(catalogue, unit, piece, dismissed, local),
            follows: !own && ![piece_state::UNALIGNED, UNOWNED].contains(&piece.owner),
            spells: spells(catalogue, piece, level),
            enchantments,
            items: items(catalogue, piece),
            stats,
            primary: stat::primary(read(piece, PRIMARY_STAT, -1)),
        }
    }
}

impl Hero {
    /// Whether its panel shows control `id`: [`OWN_CONTROLS`] only on the
    /// player's undismissed hero, 8030 also when it [`Hero::follows`].
    #[must_use]
    pub fn shows(&self, id: i32) -> bool {
        !OWN_CONTROLS.contains(&id) || self.own || (id == DOING_CONTROL && self.follows)
    }

    /// [`BANNER`]'s set for its owner.
    #[must_use]
    pub fn banner(&self) -> (FourCc, i32) {
        let (record, set) = BANNER;

        (record, set + i32::try_from(self.owner).unwrap_or_default())
    }

    /// The activity box's "Follows another monarch.", where it
    /// [`Hero::follows`].
    #[must_use]
    pub fn leader(&self, text: &Text) -> Option<String> {
        let line = text.gmtx(FOLLOWS_TEXT)?;
        let name = text.gmtx(ANOTHER_MONARCH)?;

        self.follows
            .then(|| text::plain(&text::printf(&line, &[name.as_ref().into()])).into_owned())
    }
}

/// Selection panel of's panel for a hero: [`FOLLOWER_PANEL`] when
/// dismissed, or not `local`'s and its type (`APU:6`, else its own)
/// `CanUse` `HumanPlayer`; else [`HERO_PANEL`].
///
/// Not modelled: `APU:5`, a retyped unit's kept `DialogID`, which the
/// player's own pieces take first.
fn panel(
    catalogue: &Catalogue<'_>,
    unit: &desc::Unit<'_>,
    piece: &Piece,
    dismissed: bool,
    local: u32,
) -> FourCc {
    let original = match read(piece, world::retype::ORIGINAL_TYPE, 0) {
        0 => None,
        code => Unit::descriptor(catalogue, FourCc::from_word(code.cast_unsigned()))
            .and_then(Result::ok),
    };
    let human = original.as_ref().unwrap_or(unit).can_use(User::HumanPlayer);

    if dismissed || (piece.owner != local && human) {
        FOLLOWER_PANEL
    } else {
        HERO_PANEL
    }
}

/// A rating as `MonsterController` writes it: "Poor (35)".
#[must_use]
pub fn rating(text: &Text, value: i32) -> Option<String> {
    let index = RATINGS
        .iter()
        .find(|(bound, _)| value < *bound)
        .map_or(TOP_RATING, |(_, index)| *index);

    text.gmtx(index)
        .map(|s| text::plain(&text::printf(&s, &[value.into()])).into_owned())
}

/// Experience toward the next level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Experience {
    /// `APV:16`, raised to 0.
    pub earned: i32,
    /// `APV:10`, `LevelXP`: what the next level needs.
    pub needed: i32,
}

impl Experience {
    /// `347 of 2000 xp`: [`EXPERIENCE_TEXT`].
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        Some(text::printf(
            &text.get(EXPERIENCE_TEXT)?,
            &[self.earned.into(), self.needed.into()],
        ))
    }
}

/// A building's coffer line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coffer {
    /// `APU:2`.
    pub gold: i32,
    /// The descriptor's `IncomeType`.
    pub income: Option<u32>,
}

impl Coffer {
    /// The generic building panel's `APU:2` and `IncomeType`.
    #[must_use]
    pub fn of(unit: &desc::Unit<'_>, piece: &Piece) -> Self {
        Self {
            gold: stat(piece, Stat::Gold, 0),
            income: unit.building().and_then(|b| b.income_type()),
        }
    }

    /// The line, its colours dropped; `None` where blank.
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        let line = |at| text.get(at);

        match (self.gold, self.income) {
            (0, Some(SILENT_INCOME)) => None,
            (0, _) => line(EMPTY_COFFER_TEXT).map(|s| text::plain(&s).into_owned()),
            (gold, _) => line(COFFER_TEXT)
                .map(|s| text::plain(&text::printf(&s, &[gold.into()])).into_owned()),
        }
    }
}

/// What `SignPostController` shows: a sign's message, a chest's
/// gold, and an item's name over either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posted {
    pub board: Board,
    pub says: Says,
    /// The first item key held (`items`): its name, to the first
    /// newline, takes the name's place. The rest goes to control
    /// `0x1301`, which neither panel holds.
    pub item: Option<i32>,
}

/// Which of the two panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    /// `APa7`: its 8014 box covers the portrait; the panel shows the name
    /// at the top, then the text centred both ways in the box, no portrait
    /// (**Game**, `local/shots/wooden_banner.png`).
    Sign,
    /// `APa6`: one line under the portrait.
    Chest,
}

/// Control 8014's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Says {
    /// [`MESSAGE`], an `AITX` index: what a script posted
    /// (`Post_Message`, `Create_Sign`). A save keeps it in the unit's
    /// property list.
    Message(i32),
    /// A chest with no message: its `APU:2`.
    Gold(i32),
    /// A sign with no message: control 8014 keeps the panel's own text.
    /// **Inferred**: none is shown; `STRT APa7` 1 "scrolling text"
    /// reads as a layout placeholder.
    Nothing,
}

impl Posted {
    fn of(catalogue: &Catalogue<'_>, piece: &Piece, dialog: FourCc) -> Posted {
        let message = read(piece, MESSAGE, 0);
        let says = match (message, dialog) {
            (0, CHEST_PANEL) => Says::Gold(stat(piece, Stat::Gold, 0)),
            (0, _) => Says::Nothing,
            (n, _) => Says::Message(n),
        };

        let board = if dialog == CHEST_PANEL {
            Board::Chest
        } else {
            Board::Sign
        };

        Posted {
            board,
            says,
            item: items(catalogue, piece).first().copied(),
        }
    }

    /// Control 8014's text; `None` where none. A message keeps its
    /// colour markup for the box to draw; the
    /// gold lines' is dropped.
    #[must_use]
    pub fn text(&self, text: &Text) -> Option<String> {
        let line = |at| text.get(at);

        match self.says {
            Says::Message(code) => text.activity(code).map(Cow::into_owned),
            Says::Gold(0) => line(EMPTY_CHEST_TEXT).map(|s| text::plain(&s).into_owned()),
            Says::Gold(gold) => line(CHEST_TEXT)
                .map(|s| text::plain(&text::printf(&s, &[gold.into()])).into_owned()),
            Says::Nothing => None,
        }
    }

    /// The name line: the item's name, else `named`.
    #[must_use]
    pub fn title(&self, catalogue: &Catalogue<'_>, text: &Text, named: String) -> String {
        self.item
            .and_then(|key| item(catalogue, text, key))
            .map_or(named, |(name, _)| name)
    }
}

/// A Mausoleum's line: [`BURIED_TEXT`] of its `count`.
#[must_use]
pub fn buried(text: &Text, count: usize) -> Option<String> {
    let count = i32::try_from(count).unwrap_or(i32::MAX);

    text.get(BURIED_TEXT)
        .map(|s| text::plain(&text::printf(&s, &[count.into()])).into_owned())
}

/// A dead hero's name, its method as it died.
#[must_use]
pub fn grave_name(
    catalogue: &Catalogue<'_>,
    text: &Text,
    grave: &world::graveyard::Grave,
) -> String {
    super::own_name(catalogue, text, &grave.name)
}

/// A graveyard row with detail:
/// [`GRAVE_TEXT`] of the hero's name and level, plain.
#[must_use]
pub fn grave_row(
    catalogue: &Catalogue<'_>,
    text: &Text,
    grave: &world::graveyard::Grave,
) -> Option<String> {
    let name = grave_name(catalogue, text, grave);

    text.get(GRAVE_TEXT)
        .map(|s| text::plain(&text::printf(&s, &[(&name).into(), grave.level.into()])).into_owned())
}

/// A spell's title: its `DACT`'s `HEAD`, which `HeroSpellsController` lists.
#[must_use]
pub fn spell_title(catalogue: &Catalogue<'_>, code: FourCc) -> Option<String> {
    with_action(catalogue, code, |a| a.head.title().into_owned())
}

/// An item key's text, item name: `INV:n` is `QITM[n]`; a
/// descriptor code whose character `Flags` hold `IsInventoryItem` its
/// `HEAD` title, looked up in the XML strings first (as
/// [`super::title`]). Majestic Majesty's potions are titled
/// `IDTXT_ITEM_HEALINGPOTIONS_1`, a text id. Anything else, none.
#[must_use]
pub fn item_name<'a>(catalogue: &Catalogue<'_>, text: &Text<'a>, key: i32) -> Option<Cow<'a, str>> {
    let [a, b, c, n] = key.to_le_bytes();
    let titled = || {
        let unit = Unit::descriptor(catalogue, FourCc::from_word(key.cast_unsigned()))?.ok()?;
        let title = unit
            .character()
            .filter(|c| c.has(desc::CharacterFlag::IsInventoryItem))
            .map(|_| unit.head.title())?;

        Some(Cow::Owned(
            text.strings()
                .of(&title)
                .map_or_else(|| title.into_owned(), str::to_owned),
        ))
    };

    if [a, b, c] == INVENTORY {
        text.item(usize::from(n))
    } else {
        titled()
    }
}

/// An item's name and effect: [`item_name`]'s two lines, the name's
/// colour markup kept for its list row to draw, the effect's dropped.
/// `QITM[n]` holds both; a descriptor's title is a name alone. Majestic
/// Majesty's `[color:light_red]Healing Potions:[/color] 1`.
/// **Inferred**: HD's list 9005 draws its rows through
/// text markup tag, as its other dialog items do.
#[must_use]
pub fn item(catalogue: &Catalogue<'_>, text: &Text, key: i32) -> Option<(String, String)> {
    let entry = item_name(catalogue, text, key)?;
    let (name, effect) = entry.split_once('\n').unwrap_or((&entry, ""));

    Some((
        name.trim().to_owned(),
        text::plain(effect).trim().to_owned(),
    ))
}

/// Attribute `bank:index` of `piece`, else `default`, as attr get.
fn read(piece: &Piece, key: attrib::Key, default: i32) -> i32 {
    attrib::get(&piece.attributes, key).unwrap_or(default)
}

/// [`read`] by a stat's key.
fn stat(piece: &Piece, stat: Stat, default: i32) -> i32 {
    let (bank, index) = stat.key();

    read(piece, Key::new(*bank, index), default)
}

/// `APV:30`, shown for the local player's hero while `APV:28` is 0.
fn activity(piece: &Piece, local: u32) -> Option<i32> {
    let dismissed = read(piece, HAS_BEEN_DISMISSED, 0);

    (piece.owner == local && dismissed == 0).then(|| stat(piece, Stat::Activity, 0))
}

/// The spell entries whose `CharacterLevel` is at most `level`.
///
/// **Inferred**: every entry is shown. The panel skips one whose byte
///  is 0 — `LearnSpell`'s third argument, `VehicleRec`'s 1 —
/// which no save writes and the world does not keep; only expansion quests
/// pass `FALSE`. An entry with no `DACT` is skipped.
fn spells(catalogue: &Catalogue<'_>, piece: &Piece, level: i32) -> Vec<FourCc> {
    let known = |code: FourCc| {
        with_action(catalogue, code, |a| a.character_level().unwrap_or_default())
            .and_then(|needs| i32::try_from(needs).ok())
            .is_some_and(|needs| needs <= level)
    };

    piece
        .spells
        .iter()
        .flatten()
        .map(|c| c.code)
        .filter(|&code| known(code))
        .collect()
}

/// Each item key the piece holds, in key order (hero item list,
/// is item key): an `INV:n` slot, or a character's code whose
/// `Flags` hold `IsInventoryItem`, as a mod's GPL gives (Majestic
/// Majesty's potions). **Inferred**: a building's code is none, as
/// `World::is_item_key` leaves it.
fn items(catalogue: &Catalogue<'_>, piece: &Piece) -> Vec<i32> {
    let item = |k: i32| {
        let [a, b, c, _] = k.to_le_bytes();

        [a, b, c] == INVENTORY
            || Unit::descriptor(catalogue, FourCc::from_word(k.cast_unsigned()))
                .and_then(Result::ok)
                .and_then(|u| u.character())
                .is_some_and(|c| c.has(desc::CharacterFlag::IsInventoryItem))
    };

    piece
        .attributes
        .keys()
        .copied()
        .filter(|k| item(*k))
        .collect()
}

/// `f` on the `DACT` of `code`.
fn with_action<T>(
    catalogue: &Catalogue<'_>,
    code: FourCc,
    f: impl FnOnce(&Action<'_>) -> T,
) -> Option<T> {
    let at = catalogue.find(b"DACT", code)?;
    let part = Part::parse(catalogue.record(b"DACT", at)?.data).ok()?;

    Action::parse(&part).ok().map(|a| f(&a))
}

/// The unit's panel: its descriptor's `DialogID`.
pub(crate) fn dialog(unit: &desc::Unit<'_>) -> Option<FourCc> {
    unit.character()
        .and_then(|c| c.dialog_id())
        .or_else(|| unit.building()?.dialog_id())
}

/// Whether panel `id` holds a control numbered `control`.
pub(crate) fn holds(catalogue: &Catalogue<'_>, id: FourCc, control: i32) -> bool {
    catalogue
        .find(b"SMNU", id)
        .and_then(|at| catalogue.record(b"SMNU", at))
        .and_then(|r| Panel::parse(r.data).ok())
        .is_some_and(|panel| {
            panel
                .items()
                .any(|i| i.key == smnu::CONTROL && i.value.first() == Some(&control))
        })
}
