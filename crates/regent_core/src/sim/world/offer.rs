//! What the player may order now: what the interface's panels offer, and
//! where a building may go. A front end asks these before it sends a
//! [`crate::sim::game::Input`]; the decodes refuse a short
//! treasury themselves. **Binary** unless marked.
//!
//! ```text
//!  build menu   the building list: kind 2, CanUse HumanPlayer, not
//!               NotBuildable; refused when disabled (DisableUnitType),
//!               at its cap (SetBuildingLimit), or BDEP unmet
//!               (BDEP met); a refused one unlisted unless the
//!               player lists them, then red; sorted by name
//!  preview      the preview test: disabled (6), BDEP (5), a
//!               decode pending (7), the cap (8), then the PRIM rect on
//!               the map, 85% of its cells explored, the pad free, no
//!               building within ConstructData1 (Placement); a decode
//!               pending (7) and CanIBuildThisBuilding (4) are the
//!               game's (Game::placement)
//!  guild        Base_GuildController: Produces[0] while finished,
//!               not recruiting, not full, the hero's type not disabled
//!               (WarGuildController: three)
//!  research     the research button: stage reached, the earlier stage's done
//!  upgrade      the upgrade status: UpgradeTo, not disabled nor capped,
//!               finished, whole, researched, Upgrade_HerosNeeded and
//!               Upgrade_MemberLevelNeeded met
//!  spells       the spell button state: the owner's best level of the row's
//!               building at least the row's stage (SPELLS)
//!  reward flag  the select click: explore at a point; attack on a unit
//!               while the treasury holds the reward
//! ```

use crate::sim::world::piece_state;
use std::collections::BTreeMap;

use crate::base::digest::{Map, Set};
use crate::data::attrib::{
    BUTTON_DELAY_TIME, CURRENT_STAGE_BUILT, ENCHANT_DURATION, ENCHANTMENT_LEVEL, EXPERIENCE_LEVEL,
    FIRST_STAGE_BUILT, HP, Key, MAX_HP, UPGRADE_HEROS_NEEDED,
};

use super::flag::TARGET_ID;
use super::flag::reward_steps;
use super::interface::Stride;
use super::research::{self, Item};
use super::{Piece, World, bodies::Class};
use crate::data::catalogue::Catalogue;
use crate::data::deps::{self, Expr, Stand};
use crate::data::desc::{self, BuildingFlag, User};
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::sim::space;

/// The `DATA` record of building rules.
pub const BDEP: FourCc = FourCc::new(b"BDEP");

/// The GPL constant build preview test reads: the percent of the
/// preview's `PRIM` rect cells that must be explored (85 in
/// `globals.gpl`).
pub const MIN_EXPLORED: &str = "#construction_minpercent_notblocked";

/// Every building's level while `GIVE ME POWER` is on (3,
/// player building level).
pub const EVERY_LEVEL: i32 = 3;

/// `APB:19`, `Upgrade_MemberLevelNeeded`: with [`UPGRADE_HEROS_NEEDED`],
/// what upgrade status asks of the roster.
const LEVEL_NEEDED: Key = Key::new(*b"APB", 19);

/// The `subtype` unit class of calls a hero.
const HERO: &str = "hero";

/// `APU:9`, `NotFlaggable`.
const NOT_FLAGGABLE: Key = crate::data::attrib::NOT_FLAGGABLE;

/// The wizards' tower's code prefix, whose panel enchants.
pub const WIZARDS_TOWER: [u8; 3] = *b"ABM";

/// The wizards' guild stage a tower's Enchant button wants (a compare
/// with 2, wiz tower panel state).
pub const ENCHANT_GUILD: i32 = 2;

/// The reward flags the reward panel places (`Reward1Controller`): its
/// two settings are attack and explore.
pub const ATTACK_FLAG: FourCc = FourCc::new(b"ARA2");
pub const EXPLORE_FLAG: FourCc = FourCc::new(b"ARA4");

/// Whether `code` is a reward flag's: [`ATTACK_FLAG`] or [`EXPLORE_FLAG`].
#[must_use]
pub fn is_reward_flag(code: FourCc) -> bool {
    code == ATTACK_FLAG || code == EXPLORE_FLAG
}

/// `#RewardInitial` (`globals.gpl`, 0): both panel settings at a new game.
pub const REWARD_INITIAL: &str = "#RewardInitial";

/// The warriors' guild's three recruits by `Produces` index:
/// the warrior always; the
/// paladin and the warrior of discord by temple.
const PALADIN: usize = 0;
const WARRIOR: usize = 1;
const DISCORD: usize = 2;

/// The temples war guild temples looks for among
/// the owner's finished buildings for the warriors' guild: Dauros (`ABP`)
/// and Fervus (`ABQ`). **Binary**: Fervus
/// opens the warrior of discord, else Dauros the paladin, in one slot, so
/// with both the original offers the warrior and the warrior of discord
/// only, an interface bug (**Game**, the user); here both temples offer
/// both, a deliberate departure (`docs/openregent/enhancements.md`, interface fixes).
const DAUROS: [u8; 3] = *b"ABP";
const FERVUS: [u8; 3] = *b"ABQ";

/// What a warriors' guild's panel counts beside its portrait:
/// its living members of each
/// kind (guild living count of the code). A temple hero's count is
/// shown only while its temple stands finished or the guild has one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WarRoster {
    /// Warriors, always shown (control 7949).
    pub warriors: usize,
    /// Paladins, by Dauros.
    pub paladins: Option<usize>,
    /// Warriors of discord, by Fervus.
    pub discord: Option<usize>,
}

/// The spell table, spell table build, in its
/// order: the building's code prefix, the stage it needs, the spell code,
/// its price's GPL constant.
pub const SPELLS: [SpellRow; 28] = [
    row(*b"ABY", 1, *b"Sp10", "#SpellCost_Farseeing"),
    row(*b"ABY", 1, *b"Sp11", "#SpellCost_Invisibility"),
    row(*b"ABY", 2, *b"Sp12", "#SpellCost_LightningBolt"),
    row(*b"ABY", 2, *b"Sp13", "#SpellCost_AntiMagicShield"),
    row(*b"ABY", 3, *b"Sp14", "#SpellCost_LightningStorm"),
    row(*b"ABY", 3, *b"Sp15", "#SpellCost_SuperCharge"),
    row(*b"ABO", 1, *b"Sp16", "#SpellCost_AgrelaHealing"),
    row(*b"ABO", 2, *b"Sp17", "#SpellCost_Blessing"),
    row(*b"ABO", 3, *b"Sp18", "#SpellCost_Resurrection"),
    row(*b"ABP", 1, *b"Sp19", "#SpellCost_StoneSkin"),
    row(*b"ABP", 2, *b"Sp21", "#SpellCost_Vigilance"),
    row(*b"ABP", 3, *b"Sp20", "#SpellCost_Petrify"),
    row(*b"ABQ", 1, *b"Sp22", "#SpellCost_FervusHealing"),
    row(*b"ABQ", 2, *b"Sp24", "#SpellCost_IllusionaryHero"),
    row(*b"ABQ", 3, *b"Sp23", "#SpellCost_Vines"),
    row(*b"ABT", 1, *b"Sp26", "#SpellCost_Wither"),
    row(*b"ABT", 2, *b"Sp25", "#SpellCost_AnimateBones"),
    row(*b"ABT", 3, *b"Sp27", "#SpellCost_ReAnimate"),
    row(*b"ABU", 1, *b"Sp28", "#SpellCost_WingedFeet"),
    row(*b"ABU", 2, *b"Sp29", "#SpellCost_WindStorm"),
    row(*b"ABR", 1, *b"Sp30", "#SpellCost_FireStrike"),
    row(*b"ABR", 2, *b"Sp31", "#SpellCost_SunScorch"),
    row(*b"ABq", 1, *b"Sp32", "#SpellCost_ChangeOfHeart"),
    row(*b"ABq", 1, *b"Sp33", "#SpellCost_FrostField"),
    row(*b"ABq", 2, *b"Sp34", "#SpellCost_EarthQuake"),
    row(*b"ABq", 2, *b"Sp35", "#SpellCost_ChainLightning"),
    row(*b"ABq", 3, *b"Sp36", "#SpellCost_Gate"),
    row(*b"ABq", 3, *b"Sp37", "#SpellCost_Dismiss"),
];

/// One row of [`SPELLS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellRow {
    pub building: [u8; 3],
    pub stage: i32,
    pub spell: FourCc,
    pub cost: &'static str,
}

impl SpellRow {
    /// The building it needs at its stage: `ABQ3` for Vines
    /// (spell tip: the stage plus `'0'` over the prefix).
    #[must_use]
    pub fn needs(&self) -> FourCc {
        let [a, b, c] = self.building;
        let stage = u8::try_from(self.stage).unwrap_or(0);

        FourCc([a, b, c, b'0' + stage])
    }

    /// Its price, `program`'s `#SpellCost_` constant; 0 without one.
    #[must_use]
    pub fn price(&self, program: &Program) -> i32 {
        match program.constant(self.cost) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        }
    }
}

const fn row(building: [u8; 3], stage: i32, spell: [u8; 4], cost: &'static str) -> SpellRow {
    SpellRow {
        building,
        stage,
        spell: FourCc(spell),
        cost,
    }
}

/// What a spell's button says after its own text (spell tip,
/// spell tip), by the building `N` its row needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spell<N> {
    /// Enabled: its price, none where free. No treasury asked.
    Ready(i32),
    /// `SuperCharge` disabled while it runs.
    InEffect,
    /// Disabled: the row's building at its stage, `ABQ3` for Vines.
    Needs(N),
}

impl<N> Spell<N> {
    /// Borrowing its name.
    pub const fn as_ref(&self) -> Spell<&N> {
        match self {
            Spell::Ready(price) => Spell::Ready(*price),
            Spell::InEffect => Spell::InEffect,
            Spell::Needs(n) => Spell::Needs(n),
        }
    }

    /// The building named by `f`.
    pub fn map<M>(self, f: impl FnOnce(N) -> M) -> Spell<M> {
        match self {
            Spell::Ready(price) => Spell::Ready(price),
            Spell::InEffect => Spell::InEffect,
            Spell::Needs(n) => Spell::Needs(f(n)),
        }
    }
}

/// What the upgrade button says,
/// checked in this order; `N` names the upgrade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade<N> {
    /// Hit points short of the most.
    Damaged,
    /// `CurrentStageBuilt` 0.
    Building,
    /// No `UpgradeTo`, but one it came from (`BuildingDesc`).
    Full,
    /// No `UpgradeTo` and none it came from, or the upgrade disabled.
    None,
    /// The upgrade's cap reached.
    Capped,
    /// `ResearchNotCompleted` set.
    Research,
    /// The owner's heroes, of `Upgrade_HerosNeeded`.
    Heroes { have: usize, need: i32 },
    /// The best member's level, of `Upgrade_MemberLevelNeeded`.
    Level { best: i32, need: i32 },
    /// Ready to ask: the upgrade and its price.
    Offer { to: N, price: i32 },
}

impl<N> Upgrade<N> {
    /// Borrowing its name.
    pub const fn as_ref(&self) -> Upgrade<&N> {
        match self {
            Upgrade::Damaged => Upgrade::Damaged,
            Upgrade::Building => Upgrade::Building,
            Upgrade::Full => Upgrade::Full,
            Upgrade::None => Upgrade::None,
            Upgrade::Capped => Upgrade::Capped,
            Upgrade::Research => Upgrade::Research,
            Upgrade::Heroes { have, need } => Upgrade::Heroes {
                have: *have,
                need: *need,
            },
            Upgrade::Level { best, need } => Upgrade::Level {
                best: *best,
                need: *need,
            },
            Upgrade::Offer { to, price } => Upgrade::Offer { to, price: *price },
        }
    }

    /// The upgrade named by `f`.
    pub fn map<M>(self, f: impl FnOnce(N) -> M) -> Upgrade<M> {
        match self {
            Upgrade::Damaged => Upgrade::Damaged,
            Upgrade::Building => Upgrade::Building,
            Upgrade::Full => Upgrade::Full,
            Upgrade::None => Upgrade::None,
            Upgrade::Capped => Upgrade::Capped,
            Upgrade::Research => Upgrade::Research,
            Upgrade::Heroes { have, need } => Upgrade::Heroes { have, need },
            Upgrade::Level { best, need } => Upgrade::Level { best, need },
            Upgrade::Offer { to, price } => Upgrade::Offer { to: f(to), price },
        }
    }
}

/// Whether deathmatch rules hold: `GplAIRoot`'s `Deathmatch_Rules`, which
/// the victory conditions set (`victory_conditions.gpl`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rules {
    #[default]
    Standard,
    Deathmatch,
}

/// The cap a hero's skill row names (hero skill cap, by its argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// 0: `#max_tohit`.
    ToHit,
    /// 1: `#max_toavoid`, `#deathmatch_max_toavoid` under
    /// [`Rules::Deathmatch`].
    ToAvoid,
    /// 2: `#deathmatch_hero_magic_resist`, whatever the rules.
    MagicResist,
}

impl Cap {
    /// The GPL constant it reads under `rules`.
    #[must_use]
    pub const fn constant(self, rules: Rules) -> &'static str {
        match (self, rules) {
            (Cap::ToHit, _) => "#max_tohit",
            (Cap::ToAvoid, Rules::Standard) => "#max_toavoid",
            (Cap::ToAvoid, Rules::Deathmatch) => "#deathmatch_max_toavoid",
            (Cap::MagicResist, _) => "#deathmatch_hero_magic_resist",
        }
    }

    /// Its value in `program`, 0 where none.
    #[must_use]
    pub fn value(self, program: &Program, rules: Rules) -> i32 {
        match program.constant(self.constant(rules)) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        }
    }
}

/// A row of a hero's skills that `HeroAttribController`
/// explains: its control, the `GMTX` line naming the
/// skill, and its cap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Skill {
    pub control: i32,
    pub name: usize,
    pub cap: Cap,
}

/// The rows, by control: hand to hand, ranged, parry, dodge, magic
/// resistance (`0x138f` to `0x1393`).
pub const SKILLS: [Skill; 5] = [
    skill(5007, 382, Cap::ToHit),
    skill(5008, 383, Cap::ToHit),
    skill(5009, 384, Cap::ToAvoid),
    skill(5010, 386, Cap::ToAvoid),
    skill(5011, 385, Cap::MagicResist),
];

const fn skill(control: i32, name: usize, cap: Cap) -> Skill {
    Skill { control, name, cap }
}

/// How a spell is aimed, as the world sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    /// A point: [`super::spell::cast::Cast`] of this unit ([`super::spell::cast::SPELL_UNITS`]).
    Point(FourCc),
    /// A unit: [`super::spell::cast::Bless`] on it ([`World::spell_target`]).
    Unit,
    /// No target: `SuperCharge`'s button sends at once
    /// (spell button, `0x1146`), a [`super::spell::cast::Bless`] of target 0.
    Now,
}

/// What the build menu and the panels need of the data: built once.
#[derive(Debug, Default)]
pub struct Menu {
    /// building list fill's buildings, by code.
    listed: Vec<FourCc>,
    /// The player's buildings flagged `NotBuildable`: `BUILD ANYTHING`
    /// lists them too.
    unbuildable: Vec<FourCc>,
    /// `BDEP`'s rule per code; a code it lacks needs nothing.
    rules: Map<FourCc, Expr>,
    /// Each guild's `Produces` (`BuildingFlag::IsGuild`).
    guilds: Map<FourCc, Vec<FourCc>>,
}

impl Menu {
    /// The menu from `catalogue`'s descriptors, the last loaded of each
    /// code, and its `BDEP` (the last loaded: HD's is the expansion's).
    #[must_use]
    pub fn of(catalogue: &Catalogue<'_>) -> Menu {
        let units: BTreeMap<FourCc, desc::Unit<'_>> = catalogue
            .tagged(b"DUNT")
            .filter_map(|(_, record)| {
                let part = desc::Part::parse(record.data).ok()?;

                Some((record.id, desc::Unit::parse(&part).ok()?))
            })
            .collect();
        let buildings = || {
            units
                .iter()
                .filter_map(|(code, u)| Some((*code, u, u.building()?)))
        };
        let (listed, unbuildable) = buildings()
            .filter(|(_, u, _)| u.can_use(User::HumanPlayer))
            .partition::<Vec<_>, _>(|(.., b)| !b.has(BuildingFlag::NotBuildable));
        let codes = |v: Vec<(FourCc, _, _)>| v.into_iter().map(|(code, ..)| code).collect();
        let (listed, unbuildable) = (codes(listed), codes(unbuildable));
        let guilds = buildings()
            .filter(|(.., b)| b.has(BuildingFlag::IsGuild))
            .map(|(code, _, b)| (code, b.produces()))
            .filter(|(_, p)| !p.is_empty())
            .collect();
        let text = catalogue
            .find(b"DATA", BDEP)
            .and_then(|at| catalogue.record(b"DATA", at))
            .map(|r| String::from_utf8_lossy(r.data).into_owned())
            .unwrap_or_default();
        let rules = deps::buildings(&text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| Some((r.id, r.requires?)))
            .collect();

        Menu {
            listed,
            unbuildable,
            rules,
            guilds,
        }
    }

    /// The ids `code`'s `BDEP` rule names, repeats included; none without
    /// a rule. Order construction asks `CanIBuildThisBuilding` only
    /// where there are some (bdep tokens of: the rule's tokens).
    #[must_use]
    pub fn needs(&self, code: FourCc) -> Vec<FourCc> {
        self.rules.get(&code).map(Expr::ids).unwrap_or_default()
    }

    /// `code`'s `BDEP` rule, if it has one.
    #[must_use]
    pub fn rule(&self, code: FourCc) -> Option<&Expr> {
        self.rules.get(&code)
    }
}

/// One building of the build menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub code: FourCc,
    /// The price, building price.
    pub price: i32,
    /// Whether it may be ordered: not disabled ([`World::disabled`]),
    /// under its cap ([`World::under_cap`]), its `BDEP` rule met.
    pub offered: bool,
}

/// Whether the build menu lists what it does not offer
/// (building list fill: bit 1 of
/// building list flags). The player's `BuildingListShowUnavailable`,
/// prefs record 6, default 0; `AP03`'s control 5405
/// toggles it. Unaffordable is not
/// unavailable: the fill asks no treasury.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unavailable {
    #[default]
    Hidden,
    Listed,
}

/// The markup a listed unavailable building's name opens with, red.
pub const UNAVAILABLE_MARK: &str = "\u{1}FF0000";

/// Sorts build menu rows by name as building list fill sorts its
/// list (`std::list::sort`, comparator building name less):
/// ascending, byte by byte, case kept (string compare with no
/// folding), so "Wizard" before "inn"; the red mark is left out, and the
/// sort is stable.
pub fn by_title<T>(rows: &mut [(String, T)]) {
    rows.sort_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
}

/// Where the build preview stands, placement state's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// 0: it may be placed.
    Fits,
    /// 6: a script disabled its type (`DisableUnitType`).
    Disabled,
    /// 8: the owner has its cap of its code's prefix.
    Capped,
    /// 5: its `BDEP` rule is not met.
    Unmet,
    /// 2: its `PRIM` rect leaves the map, or under
    /// [`MIN_EXPLORED`] of its cells are explored.
    Unseen,
    /// 1: its pad is taken, or another building stands within
    /// `ConstructData1` ([`crate::sim::clearing::MARGIN`]).
    Taken,
    /// 4: `CanIBuildThisBuilding` answered not 0: the wizards' tower out
    /// of a tower's or guild's range, a marketplace or trading post too
    /// close to another, an outpost too close to a palace or outpost.
    Refused,
    /// 7: the last placed building is not decoded yet: the level's
    ///  bit 2, set as create building send makes its
    /// marker `ABA0`, cleared as the decode
    /// deletes it.
    Pending,
}

/// One button of a building's panel: what it orders, its price, whether
/// it may be pressed now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Button<T> {
    pub what: T,
    pub price: i32,
    pub ready: bool,
}

/// One spell of the spell panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellOffer {
    pub row: SpellRow,
    pub price: i32,
    pub aim: Aim,
    /// Whether its button sends: an Abode spell waits out the Abode's
    /// timer (`FastBar`, [`World::abode_ready`]).
    pub ready: bool,
}

/// A building's stage for [`World::stage_of`] and `GetBuildingLevel`:
/// its code's fourth letter, one less while an upgrade is built.
pub(super) fn stage(p: &Piece) -> i32 {
    let level = i32::from(p.code.0[3]) - i32::from(b'0');

    if p.word(CURRENT_STAGE_BUILT) == 0 {
        level - 1
    } else {
        level
    }
}

/// Whether buildings standing as `stands` ([`World::build_stands`])
/// meet `rule`; none is met.
fn stood_by(stands: &Map<FourCc, Stand>, rule: Option<&Expr>) -> bool {
    rule.is_none_or(|rule| rule.holds(&|id| stands.get(&id).copied().unwrap_or(Stand::Absent)))
}

impl World {
    /// The build menu for `owner`: every listed code, its price, and
    /// whether it is offered. `BUILD
    /// ANYTHING` lists the `NotBuildable` too, and offers all.
    #[must_use]
    pub fn build_menu(&self, menu: &Menu, owner: u32) -> Vec<Offer> {
        let mut codes = menu.listed.clone();

        if self.build_cheat(owner) {
            codes.extend(&menu.unbuildable);
            codes.sort_unstable();
        }

        // The owner's buildings gathered once, not again for each code.
        let stands = self.build_stands(owner);
        let counts = self.buildings_by_prefix(owner);
        let smith = self.smith(owner);
        let same = |code: FourCc| {
            counts
                .get(&[code.0[0], code.0[1], code.0[2]])
                .copied()
                .unwrap_or(0)
        };

        codes
            .iter()
            .filter(|code| self.descriptors.class(**code) == Some(Class::Building))
            .map(|code| Offer {
                code: *code,
                price: self.price_of(*code, (0, same(*code)), smith),
                offered: self.build_cheat(owner)
                    || !self.disabled(*code)
                        && self.under_cap(owner, *code)
                        && stood_by(&stands, menu.rules.get(code)),
            })
            .collect()
    }

    /// The build menu as the palace's list shows it:
    /// what is offered, and with
    /// [`Unavailable::Listed`] what is not, a disabled type, a capped one
    /// or an unmet `BDEP` rule (statuses 1, 4, 2), its name marked red.
    /// The `NotBuildable` never, but under `BUILD ANYTHING`.
    #[must_use]
    pub fn build_list(&self, menu: &Menu, owner: u32, unavailable: Unavailable) -> Vec<Offer> {
        self.build_menu(menu, owner)
            .into_iter()
            .filter(|o| o.offered || unavailable == Unavailable::Listed)
            .collect()
    }

    /// Whether `owner`'s buildings meet `code`'s `BDEP` rule;
    /// a code with none is met.
    #[must_use]
    pub fn meets(&self, menu: &Menu, owner: u32, code: FourCc) -> bool {
        self.rule_met(owner, menu.rules.get(&code))
    }

    /// Whether `owner`'s buildings meet `rule`; none is met.
    pub(super) fn rule_met(&self, owner: u32, rule: Option<&Expr>) -> bool {
        stood_by(&self.build_stands(owner), rule)
    }

    /// How far each code stands for `owner`: its
    /// standing buildings, not ruined, finished where `CurrentStageBuilt`
    /// is set.
    fn build_stands(&self, owner: u32) -> Map<FourCc, Stand> {
        let mut out: Map<FourCc, Stand> = Map::default();

        for (_, piece, _) in self.kingdom(owner) {
            let stand = if piece.word(CURRENT_STAGE_BUILT) == 0 {
                Stand::Placed
            } else {
                Stand::Finished
            };
            let at = out.entry(piece.code).or_insert(stand);

            *at = (*at).max(stand);
        }

        out
    }

    /// `owner`'s buildings that stand, neither ruined nor agentless
    /// (`GenUnit` under 3): agent, piece and place.
    pub(super) fn kingdom(&self, owner: u32) -> impl Iterator<Item = (u32, &Piece, &super::Place)> {
        self.pieces.iter().filter_map(move |(id, p)| {
            let place = self.known(*id)?;
            let building = place.is_building() && super::price::stands(p);

            (building && p.owner == owner).then_some((*id, p, place))
        })
    }

    /// Player building level: the highest stage of
    /// `owner`'s standing buildings of code prefix `prefix` with
    /// `FirstStageBuilt`, the code's fourth letter, one less while an
    /// upgrade is built; 0 with none. [`EVERY_LEVEL`] while
    /// every level flag is set ([`World::every_level`]).
    #[must_use]
    pub fn building_level(&self, owner: u32, prefix: [u8; 3]) -> i32 {
        if self.every_level(owner) {
            EVERY_LEVEL
        } else {
            self.stage_of(owner, prefix)
        }
    }

    /// [`World::building_level`] as the buildings stand, flag or not:
    /// the building player building level hands back.
    pub(super) fn stage_of(&self, owner: u32, prefix: [u8; 3]) -> i32 {
        self.kingdom(owner)
            .filter(|(_, p, _)| p.code.0.starts_with(&prefix) && p.word(FIRST_STAGE_BUILT) != 0)
            .map(|(_, p, _)| stage(p))
            .max()
            .unwrap_or_default()
    }

    /// [`World::stage_of`] for every prefix `owner` has, from one walk;
    /// a prefix not here is 0.
    fn stages(&self, owner: u32) -> Map<[u8; 3], i32> {
        let mut out: Map<[u8; 3], i32> = Map::default();
        let built = self
            .kingdom(owner)
            .filter(|(_, p, _)| p.word(FIRST_STAGE_BUILT) != 0)
            .map(|(_, p, _)| ([p.code.0[0], p.code.0[1], p.code.0[2]], stage(p)));

        for (prefix, level) in built {
            out.entry(prefix)
                .and_modify(|l| *l = (*l).max(level))
                .or_insert(level);
        }

        out
    }

    /// Build preview test for a building of `code` placed by
    /// `owner` at `at`, 32nds of a cell: the preview snaps to its half
    /// cell. The world's part: states 7 and 4, a decode pending and
    /// `CanIBuildThisBuilding`, are the game's
    /// (`sim::game::Game::placement`).
    #[must_use]
    pub fn placement(
        &self,
        program: &Program,
        menu: &Menu,
        owner: u32,
        code: FourCc,
        at: (i32, i32),
    ) -> Placement {
        self.placer(program, menu, owner, code)(at)
    }

    /// [`World::placement`] as a function of the place, for many places:
    /// what does not depend on it is worked out once.
    pub fn placer<'s>(
        &'s self,
        program: &Program,
        menu: &Menu,
        owner: u32,
        code: FourCc,
    ) -> impl Fn((i32, i32)) -> Placement + 's {
        // `BUILD ANYTHING` skips the type and the rule, not the cap or
        // the ground.
        let cheat = self.build_cheat(owner);
        let gate = if !cheat && self.disabled(code) {
            Some(Placement::Disabled)
        } else if !cheat && !self.meets(menu, owner, code) {
            Some(Placement::Unmet)
        } else if !self.under_cap(owner, code) {
            Some(Placement::Capped)
        } else {
            None
        };
        let site = self.siter(program, owner, code);

        move |at| gate.unwrap_or_else(|| site(at))
    }

    /// Build preview test's tests of the ground alone: as it runs
    /// with its skip argument set, for a camp move's preview,
    /// which asks no rule, no pending decode
    /// and no `CanIBuildThisBuilding`.
    #[must_use]
    pub fn site(&self, program: &Program, owner: u32, code: FourCc, at: (i32, i32)) -> Placement {
        self.siter(program, owner, code)(at)
    }

    /// [`World::site`] as a function of the place.
    fn siter<'s>(
        &'s self,
        program: &Program,
        owner: u32,
        code: FourCc,
    ) -> impl Fn((i32, i32)) -> Placement + 's {
        let body = self.descriptors.body(code);
        let rect = self.descriptors.shape(code).map(|s| s.bounds);
        let percent = match program.constant(MIN_EXPLORED) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };
        // Its pad free of what stands, its clearing ring of buildings.
        let free = body.map(|b| {
            (
                self.blocked(b, (&b.cells, b.edge), super::placing::Blockers::Standing),
                self.crowder(b),
            )
        });

        move |at| {
            let half = (
                at.0 / crate::sim::units::PER_HALF,
                at.1 / crate::sim::units::PER_HALF,
            );
            let snapped = (
                half.0 * crate::sim::units::PER_HALF,
                half.1 * crate::sim::units::PER_HALF,
            );

            if !rect.is_some_and(|r| self.seen_enough(r, snapped, owner, percent)) {
                Placement::Unseen
            } else if !free
                .as_ref()
                .is_some_and(|(blocked, crowded)| !blocked(half) && !crowded(half))
            {
                Placement::Taken
            } else {
                Placement::Fits
            }
        }
    }

    /// `CanIBuildThisBuilding`'s second argument as
    /// order construction gathers it: each building of unit list
    /// 0, every owner's, once per id of `ids` it matches by code
    /// (**Inferred**: list 0 in agent order). No shipped script reads it.
    #[must_use]
    pub fn dependents(&self, ids: &[FourCc]) -> Vec<u32> {
        let mut buildings: Vec<(u32, [u8; 4])> = self
            .pieces
            .iter()
            .filter(|(id, _)| self.agents.alive(**id))
            .filter(|(id, _)| self.known(**id).is_some_and(super::Place::is_building))
            .map(|(id, p)| (*id, p.code.0))
            .collect();

        buildings.sort_unstable();

        buildings
            .iter()
            .flat_map(|(id, code)| ids.iter().filter(|i| i.0 == *code).map(|_| *id))
            .collect()
    }

    /// Whether `rect`, 32nds about `at`, lies on the map's cells at both
    /// corners and at least `percent` of its cells have a
    /// corner `owner` explored. The test runs in
    /// `f32` cells, `x / 32`, each cell from `x0` stepping 1.0 while not
    /// past `x1`; the share is 16.16 against
    /// `float_to_fixed(percent / 100.0)`.
    fn seen_enough(&self, rect: space::HalfRect, at: (i32, i32), owner: u32, percent: i32) -> bool {
        let cells = self.fog.explored().cells();
        let corner = |x: i32, y: i32| {
            let cell = (
                x.div_euclid(crate::sim::units::PER_CELL),
                y.div_euclid(crate::sim::units::PER_CELL),
            );

            (0..cells).contains(&cell.0) && (0..cells).contains(&cell.1)
        };
        let [x0, y0, x1, y1] = [
            at.0 + rect[0],
            at.1 + rect[1],
            at.0 + rect[2],
            at.1 + rect[3],
        ];
        let on = corner(x0, y0) && corner(x1, y1);

        #[allow(clippy::cast_precision_loss, reason = "32nds of a 512 cell map")]
        let f = |v: i32| v as f32 / crate::sim::units::PER_CELL as f32;

        let (fx0, fy0, fx1, fy1) = (f(x0), f(y0), f(x1), f(y1));
        let steps = |from: f32, to: f32| {
            std::iter::successors(Some(from), |v| Some(v + 1.0)).take_while(move |v| *v <= to)
        };

        #[allow(clippy::cast_possible_truncation, reason = "a cell on the map")]
        let unseen = steps(fx0, fx1)
            .flat_map(|x| steps(fy0, fy1).map(move |y| (x as i32, y as i32)))
            .filter(|cell| !self.fog.explored().cell_seen(*cell, owner))
            .count();

        #[allow(clippy::cast_possible_truncation, reason = "float to int")]
        let area = ((fx1 - fx0 + 1.0) as i32) * ((fy1 - fy0 + 1.0) as i32);

        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "a percent"
        )]
        let need = (percent as f32 / 100.0 * 65536.0) as i32;

        let unseen = i32::try_from(unseen).unwrap_or(i32::MAX);
        #[allow(clippy::cast_possible_truncation, reason = "a share under 1.0")]
        let share = if area > 0 {
            ((i64::from(unseen) << 16) / i64::from(area)) as i32
        } else {
            0
        };

        on && 0x1_0000 - share >= need
    }

    /// Footprint blocked with margin `ConstructData1` and
    /// flags `0x1100`, as a function of half cell: its clearing ring off the map,
    /// or over the pad of a standing building on a shared layer
    /// (blocker default: `0x1000` lets what `GSUnit` clears
    /// through, `0x100` asks the other `GSUnit` method `(1)`, a building not
    /// a chest).
    fn crowder<'s>(&'s self, body: &super::bodies::Body) -> impl Fn((i32, i32)) -> bool + 's {
        let ring: Vec<(i32, i32)> = body
            .ring
            .as_ref()
            .map_or_else(|| body.cells.clone(), |r| r.halves().collect());
        let taken: Set<(i32, i32)> = self
            .pieces
            .iter()
            .filter_map(|(id, p)| {
                let place = self.known(*id)?;
                let other = self.descriptors.body(p.code)?;
                // `GSUnit` method `(1)` is false for a chest, full or not.
                let blocks = place.is_building()
                    && !super::walking::is_chest(p.code.0)
                    && !Self::clearable(p, place)
                    && !place.ruined()
                    && place.holder == 0
                    && other.layer & body.layer != 0;

                blocks.then(|| {
                    other
                        .cells
                        .iter()
                        .map(move |(x, y)| (place.standing.0 + x, place.standing.1 + y))
                })
            })
            .flatten()
            .collect();
        let off = |(x, y): (i32, i32)| !self.board.holds((x, y));

        move |half| {
            ring.iter()
                .map(|(x, y)| (half.0 + x, half.1 + y))
                .any(|at| off(at) || taken.contains(&at))
        }
    }

    /// A guild's recruit buttons: its
    /// `Produces[0]` at the hero's `Cost`, ready while the guild is
    /// finished (`FirstStageBuilt`), recruits nothing (`APB:13` 0) and
    /// its living do not fill it. The warriors'
    /// guild's three (`DAUROS`, `FERVUS`). Empty for any other
    /// building.
    #[must_use]
    pub fn recruits(&self, menu: &Menu, id: u32) -> Vec<Button<FourCc>> {
        self.pieces
            .get(id)
            .map(|piece| self.guild_buttons(menu, id, piece))
            .unwrap_or_default()
    }

    /// [`World::recruits`] of guild `id`, `piece`.
    fn guild_buttons(&self, menu: &Menu, id: u32, piece: &Piece) -> Vec<Button<FourCc>> {
        let produces = menu.guilds.get(&piece.code).cloned().unwrap_or_default();
        let ready = piece.word(FIRST_STAGE_BUILT) != 0
            && piece.word(BUTTON_DELAY_TIME) == 0
            && !self.filled(id);
        let finished = |prefix: [u8; 3]| self.building_level(piece.owner, prefix) > 0;
        let picks: Vec<usize> = if produces.len() > DISCORD {
            let (dauros, fervus) = (finished(DAUROS), finished(FERVUS));

            std::iter::once(WARRIOR)
                .chain(dauros.then_some(PALADIN))
                .chain(fervus.then_some(DISCORD))
                .collect()
        } else {
            vec![0]
        };

        picks
            .into_iter()
            .filter_map(|i| produces.get(i))
            .map(|code| Button {
                what: *code,
                price: self.descriptors.body(*code).map_or(0, |b| b.cost),
                ready: ready && !self.disabled(*code),
            })
            .collect()
    }

    /// Warriors' guild `id`'s [`WarRoster`]; none for any other building.
    #[must_use]
    pub fn war_roster(&self, menu: &Menu, id: u32) -> Option<WarRoster> {
        let piece = self.pieces.get(id)?;
        let produces = menu.guilds.get(&piece.code)?;
        let living = |i: usize| {
            let code = produces.get(i).copied();

            self.roster(id)
                .into_iter()
                .filter(|(member, dead)| !dead && self.pieces.code(*member) == code)
                .count()
        };
        let finished = |prefix: [u8; 3]| self.building_level(piece.owner, prefix) > 0;
        let shown = |temple: bool, n: usize| (temple || n > 0).then_some(n);

        (produces.len() > DISCORD).then(|| WarRoster {
            warriors: living(WARRIOR),
            paladins: shown(finished(DAUROS), living(PALADIN)),
            discord: shown(finished(FERVUS), living(DISCORD)),
        })
    }

    /// A researcher's buttons, the researches its
    /// stage shows: each whose stage the building's code reaches, past
    /// stage 1 only once the earlier stage's are done, none while
    /// another runs. Ready while not done, the stage finished
    /// (`CurrentStageBuilt`), the building whole and idle. The price
    /// less the owner's Library discount.
    #[must_use]
    pub fn researches(&self, program: &Program, id: u32) -> Vec<Button<Item>> {
        self.pieces
            .get(id)
            .map(|piece| self.research_buttons(program, piece))
            .unwrap_or_default()
    }

    /// [`World::researches`] of `piece`.
    fn research_buttons(&self, program: &Program, piece: &Piece) -> Vec<Button<Item>> {
        let stage = research::stage(piece.code.0);
        let running = piece.word(research::CURRENT);
        let library = self.library_level(piece.owner);
        let idle = piece.word(CURRENT_STAGE_BUILT) != 0
            && !damaged(piece)
            && piece.word(BUTTON_DELAY_TIME) == 0;

        research::items(piece.code.0)
            .filter(|item| item.stage <= stage)
            .filter(|item| item.stage <= 1 || self.done_by(piece, item.stage - 1))
            .filter(|item| running == 0 || running == item.id.cast_signed())
            .map(|item| {
                let done = piece.word(item.key) != 0;

                Button {
                    what: item,
                    price: research::price(research::cost(program, item), library),
                    ready: idle && !done && running == 0,
                }
            })
            .collect()
    }

    /// A building's upgrade button: its
    /// `UpgradeTo` at `World::upgrade_price`. Ready once finished
    /// (`CurrentStageBuilt`), whole, its researches done
    /// (`ResearchNotCompleted` 0), the owner holding
    /// `Upgrade_HerosNeeded` heroes ([`World::heroes`]: the palace's
    /// `heroes_to_upgrade`, the only building that sets it), and
    /// `Upgrade_MemberLevelNeeded` met. That last loops over the roster
    /// (relation 100) but reads the building's own `APV:11` each pass;
    /// nothing sets the key, so it passes. `BDEP` is not
    /// asked: upgrade uncapped reads the `BTDT` cap table.
    #[must_use]
    pub fn upgrade_offer(&self, id: u32) -> Option<Button<FourCc>> {
        let piece = self.pieces.get(id)?;
        let to = self.descriptors.body(piece.code)?.upgrade?;
        let finished = piece.word(CURRENT_STAGE_BUILT) != 0;

        // Disabled or capped, a finished building offers none (state 3);
        // the cheat asks neither.
        let barred = self.disabled(to) || !self.uncapped(to);
        let shown = !finished || !barred || self.build_cheat(piece.owner);

        // The engine's slip: the building's level, once per member.
        let best = if self.roster(id).is_empty() {
            0
        } else {
            piece.word(EXPERIENCE_LEVEL).max(0)
        };

        // `BUILD ANYTHING` asks only that it is finished.
        let heroes = usize::try_from(piece.word(UPGRADE_HEROS_NEEDED)).unwrap_or(0);
        let ready = finished
            && (self.build_cheat(piece.owner)
                || !damaged(piece)
                    && piece.word(research::NOT_COMPLETED) == 0
                    && self.heroes(piece.owner) >= heroes
                    && best >= piece.word(LEVEL_NEEDED));

        shown.then_some(Button {
            what: to,
            price: self.upgrade_price(piece.owner, to),
            ready,
        })
    }

    /// What building `id`'s upgrade button says (upgrade tooltip,
    /// upgrade tooltip), checked in the engine's order. No cheat asked.
    /// **Inferred**: the best member's level as [`World::upgrade_offer`]
    /// reads it; nothing sets `Upgrade_MemberLevelNeeded`.
    #[must_use]
    pub fn upgrade_tip(&self, id: u32) -> Option<Upgrade<FourCc>> {
        let piece = self.pieces.get(id)?;
        let body = self.descriptors.body(piece.code)?;
        let heroes = self.heroes(piece.owner);
        let best = if self.roster(id).is_empty() {
            0
        } else {
            piece.word(EXPERIENCE_LEVEL).max(0)
        };

        Some(match body.upgrade {
            _ if damaged(piece) => Upgrade::Damaged,
            _ if piece.word(CURRENT_STAGE_BUILT) == 0 => Upgrade::Building,
            None if body.upgraded.is_some() => Upgrade::Full,
            None => Upgrade::None,
            Some(to) if self.disabled(to) => Upgrade::None,
            Some(to) if !self.uncapped(to) => Upgrade::Capped,
            Some(_) if piece.word(research::NOT_COMPLETED) != 0 => Upgrade::Research,
            Some(_)
                if usize::try_from(piece.word(UPGRADE_HEROS_NEEDED)).is_ok_and(|n| heroes < n) =>
            {
                Upgrade::Heroes {
                    have: heroes,
                    need: piece.word(UPGRADE_HEROS_NEEDED),
                }
            }
            Some(_) if best < piece.word(LEVEL_NEEDED) => Upgrade::Level {
                best,
                need: piece.word(LEVEL_NEEDED),
            },
            Some(to) => Upgrade::Offer {
                to,
                price: self.upgrade_price(piece.owner, to),
            },
        })
    }

    /// Whether building `id` is short of its hit points.
    #[must_use]
    pub fn needs_repair(&self, id: u32) -> bool {
        self.pieces.get(id).is_some_and(damaged)
    }

    /// What `owner`'s button for `row` says after its own text:
    /// its price where `offers`
    /// ([`World::spells`]) holds it, else why not.
    #[must_use]
    pub fn spell_tip(&self, offers: &[SpellOffer], row: &SpellRow, owner: u32) -> Spell<FourCc> {
        match offers.iter().find(|o| o.row.spell == row.spell) {
            Some(o) => Spell::Ready(o.price),
            None if row.spell.0 == super::spell::cast::SUPERCHARGE && self.supercharged(owner) => {
                Spell::InEffect
            }
            None => Spell::Needs(row.needs()),
        }
    }

    /// `owner`'s heroes as upgrade status counts them: every
    /// mobile of unit list 1 (`VehicleList`) of the owner, standing
    /// (`GenUnit` under 3: not lying dead), its agent's `subtype`
    /// `hero` (unit class of 1) and not `NotHero`.
    /// Wherever it is: walking, in a guild or inn.
    #[must_use]
    pub fn heroes(&self, owner: u32) -> usize {
        let hero = |id: u32| {
            self.agents.get(id).is_some_and(|a| {
                a.values.iter().any(|m| {
                    m.name.eq_ignore_ascii_case(super::search::SUBTYPE)
                        && matches!(&m.value, Value::String(s) if s.eq_ignore_ascii_case(HERO))
                })
            })
        };

        self.pieces
            .iter()
            .filter(|(_, p)| p.owner == owner)
            .filter(|(_, p)| {
                self.descriptors
                    .body(p.code)
                    .is_some_and(|b| b.class == Class::Character && !b.not_hero)
            })
            .filter(|(_, p)| p.place.is_none_or(|at| at.state & piece_state::RUINED == 0))
            .filter(|(id, _)| hero(**id))
            .count()
    }

    /// Tower `id`, where its panel shows its enchantment, button or bar:
    /// finished, and not under Last
    /// Palace (level victory index 3, wiz tower panel state).
    fn enchant_panel(&self, id: u32) -> Option<&Piece> {
        let last_palace = self.level.victory == super::research::LAST_PALACE;

        self.pieces
            .get(id)
            .filter(|p| p.code.0.starts_with(&WIZARDS_TOWER))
            .filter(|p| p.word(FIRST_STAGE_BUILT) != 0 && !last_palace)
    }

    /// A wizards' tower's enchantment button: `#WizTowerUpgradeCost`,
    /// ready while `EnchantmentLevel` is not above 0 and none runs.
    /// Shown
    /// on `World::enchant_panel`'s towers with a wizards' guild of
    /// stage 2 or better.
    /// The engine asks the local player's guild; the panel shows on the
    /// local player's towers alone, so the tower's owner's.
    #[must_use]
    pub fn enchant_offer(&self, program: &Program, id: u32) -> Option<Button<()>> {
        let piece = self.enchant_panel(id).filter(|p| {
            self.building_level(p.owner, super::spell::target::WIZARDS_GUILD) >= ENCHANT_GUILD
        })?;
        let price = match program.constant(super::guild::ENCHANT_COST) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };

        Some(Button {
            what: (),
            price,
            ready: piece.word(ENCHANTMENT_LEVEL) <= 0 && piece.word(ENCHANT_DURATION) == 0,
        })
    }

    /// A wizards' tower's enchantment as its panel shows it:
    /// ms since `APB:45`, of
    /// `#Wiz_Tower_Enchantment_Duration`, on
    /// `World::enchant_panel`'s towers while `EnchantmentLevel` is 1
    /// or more. Bar 8202 and its words 8723 take the enchant button's
    /// place.
    #[must_use]
    pub fn enchanting(&self, program: &Program, id: u32) -> Option<(i32, i32)> {
        let piece = self
            .enchant_panel(id)
            .filter(|p| p.word(ENCHANTMENT_LEVEL) >= 1)?;
        let length = match program.constant(super::guild::ENCHANT_DURATION) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };
        let elapsed = self
            .now
            .cast_signed()
            .wrapping_sub(piece.word(ENCHANT_DURATION));

        Some((elapsed, length))
    }

    /// The spells `owner` may cast: each row whose
    /// building it has at the row's stage or better, at its price.
    /// `SuperCharge` not while the palace's `SuperChargeActive` is set.
    #[must_use]
    pub fn spells(&self, program: &Program, owner: u32) -> Vec<SpellOffer> {
        let abode = self.abode_ready(owner);
        let charged = self.supercharged(owner);

        // Each prefix's level from one walk of the kingdom, not one a row.
        let every = self.every_level(owner);
        let stages = self.stages(owner);
        let level = |prefix: &[u8; 3]| {
            if every {
                EVERY_LEVEL
            } else {
                stages.get(prefix).copied().unwrap_or_default()
            }
        };

        SPELLS
            .iter()
            .filter(|row| level(&row.building) >= row.stage)
            .filter(|row| !(charged && row.spell.0 == super::spell::cast::SUPERCHARGE))
            .map(|row| SpellOffer {
                row: *row,
                price: row.price(program),
                aim: aim(row.spell),
                ready: row.building != super::spell::cast::ABODE || abode,
            })
            .collect()
    }

    /// Whether `owner`'s palace has `SuperChargeActive` set.
    #[must_use]
    pub fn supercharged(&self, owner: u32) -> bool {
        self.pieces
            .get(self.palace_of(owner))
            .is_some_and(|p| p.word(super::spell::cast::SUPERCHARGE_ACTIVE) != 0)
    }

    /// Whether `owner` may hang an attack flag on agent `target`:
    /// a building, or a
    /// character character usable takes that is no resource pile;
    /// it stands (`GenUnit` under 3: no ruin, none
    /// lying dead), is not
    /// `NotFlaggable`, carries no attack flag of `owner`'s, and the
    /// treasury holds `reward`. Any owner's: **Game**, the player's own
    /// palace takes one.
    #[must_use]
    pub fn flaggable(&self, owner: u32, target: u32, reward: i32) -> bool {
        let standing = self
            .known(target)
            .is_some_and(|p| p.state & piece_state::RUINED == 0 && p.holder == 0);
        let flagged = self.pieces.values().any(|p| {
            p.code == ATTACK_FLAG && p.owner == owner && p.word(TARGET_ID) == target.cast_signed()
        });
        let rich = self.treasury(owner).is_none_or(|g| g >= reward);

        self.flag_kind(target) && standing && !flagged && rich
    }

    /// [`World::flaggable`]'s tests of agent `target` alone: a building,
    /// or a usable character no resource pile, not `NotFlaggable`. Not
    /// the engine's: [`crate::sim::game::check`] holds an attack flag to
    /// it, as the decode refuses only a flag the
    /// target holds already. Standing and the treasury it leaves to the
    /// decode: either may change between the click and the decode.
    #[must_use]
    pub fn flag_kind(&self, target: u32) -> bool {
        let building = |p: &Piece| self.descriptors.class(p.code) == Some(Class::Building);

        self.pieces.get(target).is_some_and(|p| {
            let kind = building(p)
                || (self.usable(p) && !super::spell::target::RESOURCES.contains(&p.code.0));

            kind && p.word(NOT_FLAGGABLE) == 0
        })
    }

    /// Whether a focus key selects piece `id` (`FastHotKeyList`,
    /// [`crate::view::map::map_keys::Jump`]): it stands (`GenUnit` under
    /// 3), or is no building, so a ruin is passed over
    /// and a body lying dead is not.
    #[must_use]
    pub fn focusable(&self, id: u32) -> bool {
        let building = self
            .pieces
            .get(id)
            .is_some_and(|p| self.descriptors.class(p.code) == Some(Class::Building));
        let standing = self
            .known(id)
            .is_some_and(|p| p.state & piece_state::RUINED == 0);

        self.pieces.contains(id) && (standing || !building)
    }

    /// Whether `owner` may raise reward flag `id` by
    /// [`super::flag::REWARD_DELTA`]: its own, the treasury holding that much.
    /// Its reward is not asked: a flag
    /// at 0 is raised too.
    #[must_use]
    pub fn raisable(&self, owner: u32, id: u32) -> bool {
        let own = self.pieces.owner(id) == Some(owner);

        own && self.reward(id).is_some()
            && self
                .treasury(owner)
                .is_none_or(|g| g >= super::flag::REWARD_DELTA)
    }

    /// How many raises of flag `id` a press sends: while the treasury,
    /// less those before, holds one ([`World::raisable`]). Each pays on
    /// landing, so the setting's own check at 0 spent asks the same.
    #[must_use]
    pub fn flag_raises(&self, owner: u32, id: u32, stride: Stride) -> usize {
        if self.raisable(owner, id) {
            reward_steps(stride, |gone| self.raises_reward(owner, gone)).count()
        } else {
            0
        }
    }
}

/// How spell `spell` is aimed: `SuperCharge` at nothing; a target's
/// ([`super::spell::cast::SPELL_SCRIPTS`], Illusionary Hero); else its unit's at a
/// point ([`super::spell::cast::SPELL_UNITS`]). Lightning bolt and fire strike hit a
/// unit.
#[must_use]
pub fn aim(spell: FourCc) -> Aim {
    let unit = super::spell::cast::SPELL_UNITS
        .iter()
        .find(|(_, s, _)| *s == spell.0)
        .map_or(FourCc(*b"    "), |(u, ..)| FourCc(*u));
    let blessing = super::spell::cast::SPELL_SCRIPTS
        .iter()
        .any(|(s, ..)| *s == spell.0);

    match spell.0 {
        super::spell::cast::SUPERCHARGE => Aim::Now,
        super::spell::cast::ILLUSION => Aim::Unit,
        _ if blessing => Aim::Unit,
        _ => Aim::Point(unit),
    }
}

/// Unit damaged: hit points, raised to 0, under the maximum.
fn damaged(piece: &Piece) -> bool {
    piece.word(HP).max(0) < piece.word(MAX_HP)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::kind;

    #[test]
    fn the_spell_table_has_three_stages_per_temple_at_most() {
        assert!(SPELLS.iter().all(|r| (1..=3).contains(&r.stage)));
        assert_eq!(SPELLS.iter().filter(|r| r.building == *b"ABY").count(), 6);
        assert_eq!(SPELLS[0].spell, FourCc::new(b"Sp10"));
        assert_eq!(SPELLS[27].spell, FourCc::new(b"Sp37"));
    }

    #[test]
    fn a_spell_is_aimed_at_a_point_a_unit_or_nothing() {
        assert_eq!(aim(FourCc::new(b"Sp10")), Aim::Point(FourCc::new(b"WVA1")));
        assert_eq!(aim(FourCc::new(b"Sp12")), Aim::Unit);
        assert_eq!(aim(FourCc::new(b"Sp30")), Aim::Unit);
        assert_eq!(aim(FourCc::new(b"Sp24")), Aim::Unit);
        assert_eq!(aim(FourCc::new(b"Sp15")), Aim::Now);
        assert_eq!(aim(FourCc::new(b"Sp36")), Aim::Point(FourCc::new(b"WVh1")));
    }

    /// `GenUnit` is 3 from `DeleteAgent` on, before the ruin: a
    /// building falling is out of the owner's kingdom.
    #[test]
    fn an_agentless_building_leaves_the_kingdom() {
        use crate::sim::world::ruin::tests::standing;

        const LIBRARY: u32 = 21;

        let mut world = World::default();
        let mut library = standing(*b"ABG1", LIBRARY, kind::Kind::BUILDING, (0, 0));

        library.attributes.insert(FIRST_STAGE_BUILT.packed(), 1);
        library.attributes.insert(CURRENT_STAGE_BUILT.packed(), 1);
        library
            .attributes
            .insert(super::super::AGENT_ID.packed(), LIBRARY.cast_signed());
        world.pieces.insert(LIBRARY, library);
        world
            .units
            .set_motion(LIBRARY, super::super::places::Motion::Still);
        assert_eq!(world.stage_of(0, *b"ABG"), 1);

        world.orphan(LIBRARY);
        assert_eq!(world.stage_of(0, *b"ABG"), 0);
    }

    /// Under Last Palace a tower's panel shows neither its button nor
    /// its bar.
    #[test]
    fn last_palace_hides_the_towers_enchantment() {
        use crate::sim::world::cheat::{Cheat, CheatCode};
        use crate::sim::world::ruin::tests::standing;

        const TOWER: u32 = 21;

        let program = Program::default();
        let mut world = World::default();
        let mut tower = standing(*b"ABM1", TOWER, kind::Kind::BUILDING, (0, 0));

        tower.attributes.insert(FIRST_STAGE_BUILT.packed(), 1);
        world.pieces.insert(TOWER, tower);
        world.cheat(&CheatCode {
            cheat: Cheat::Spells,
            arg: 0,
        });

        assert!(world.enchant_offer(&program, TOWER).is_some());

        world.write(TOWER, ENCHANTMENT_LEVEL.packed(), 1);
        assert!(world.enchanting(&program, TOWER).is_some());

        world.level.victory = super::super::research::LAST_PALACE;
        assert_eq!(world.enchanting(&program, TOWER), None);

        world.write(TOWER, ENCHANTMENT_LEVEL.packed(), 0);
        assert_eq!(world.enchant_offer(&program, TOWER), None);
    }
}
