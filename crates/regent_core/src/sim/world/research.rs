//! Research the player orders: `UPGRADE_BUILDING_MESSAGE` naming a
//! research. **Binary** unless marked.
//!
//! ```text
//!  frame f  GSMessageManager       case 0x3ed, upgrade decode:
//!    message 0x0c not 0 ── GS_OrderProcessor (building, research id)
//!       nothing while a research order runs on it (1, 0x4005)
//!       price: #ResearchCost… less 5% per Library level, against APP:0
//!       paid (GE_Player): APB:44 the id, APB:56 the clock,
//!       APB:13 #ResearchTime…; GS_OrderProcessor, callback 0x2009
//!  the time later, in the orders: research done
//!       the research's attribute 1; the advisor's word
//!       (world::ADVISORS); APB:23 0 once research is done; APB:44 0,
//!       APB:13 0; a chat line. No GPL dispatch
//! ```
//!
//! The table (research table build, built on first use) reads each research's
//! time and cost from the loaded GPL's `#ResearchTime…` and
//! `#ResearchCost…` constants through `GplExpressionHandle`.
//!
//! **Game**, play5: the eleven researches' attributes, `ButtonStartTime`
//! `50 (f + 1)` for each building's last, and `ResearchNotCompleted` are
//! the save's.

use std::collections::BTreeMap;

use super::chat::{Line, NO_GOLD_RESEARCH};
use super::timed::Timer;
use super::{Piece, World};
use crate::data::attrib::{BUTTON_DELAY_TIME, BUTTON_START_TIME, Key};
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::runtime::MS_PER_FRAME;

/// A research the player orders: the building, by agent id; the research
/// id (message `0x0c`); the frame err.log decodes it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Research {
    pub building: u32,
    pub item: u32,
    pub frame: u32,
}

/// One row of the research table, research table build's calls to
/// research row add: the id, the stage the building needs, the attribute set
/// when done, a chat text id, and the name its two constants end in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: u32,
    pub stage: u8,
    pub key: Key,
    pub text: u32,
    pub name: &'static str,
}

const fn item(id: u32, stage: u8, key: Key, text: u32, name: &'static str) -> Item {
    Item {
        id,
        stage,
        key,
        text,
        name,
    }
}

/// The research table, in research table build's order.
pub const ITEMS: [Item; 26] = [
    item(5000, 1, Key::new(*b"APB", 24), 0xc3, "WeaponLevel_2"),
    item(5001, 1, Key::new(*b"APB", 25), 0xc4, "ArmorLevel_2"),
    item(5002, 2, Key::new(*b"APB", 26), 0xc5, "WeaponLevel_3"),
    item(5003, 2, Key::new(*b"APB", 27), 0xc6, "ArmorLevel_3"),
    item(5004, 3, Key::new(*b"APB", 28), 0xc7, "WeaponLevel_4"),
    item(5005, 3, Key::new(*b"APB", 29), 0xc8, "ArmorLevel_4"),
    item(5010, 1, Key::new(*b"APB", 30), 0xc9, "Tournament"),
    item(5020, 1, Key::new(*b"APB", 42), 0xca, "Arrows"),
    item(5021, 2, Key::new(*b"APB", 43), 0xcb, "GoodGuard"),
    item(5030, 1, Key::new(*b"APB", 31), 0xcc, "WizardFireBlast"),
    item(5031, 1, Key::new(*b"APB", 32), 0xcd, "MagicResistance"),
    item(5032, 1, Key::new(*b"APB", 33), 0xce, "TrainIntelligence"),
    item(5033, 2, Key::new(*b"APB", 34), 0xcf, "WizardMeteorStorm"),
    item(5034, 2, Key::new(*b"APB", 35), 0xd0, "EnergyBlast"),
    item(5035, 2, Key::new(*b"APB", 36), 0xd1, "FireShield"),
    item(5040, 1, Key::new(*b"APB", 37), 0xd2, "MarketDay"),
    item(5041, 1, Key::new(*b"APB", 39), 0xd3, "HealPotions"),
    item(5042, 2, Key::new(*b"APB", 40), 0xd4, "RingsofProtection"),
    item(5043, 3, Key::new(*b"APB", 41), 0xd5, "PowerfulItem"),
    item(5050, 1, Key::new(*b"APB", 39), 0xd6, "TradePostHealing"),
    item(5061, 1, Key::new(*b"ARB", 0), 0x184, "MagicBazaar_Item_One"),
    item(5062, 1, Key::new(*b"ARB", 1), 0x185, "MagicBazaar_Item_Two"),
    item(
        5063,
        2,
        Key::new(*b"ABB", 2),
        0x186,
        "MagicBazaar_Item_Three",
    ),
    item(
        5064,
        2,
        Key::new(*b"ABB", 3),
        0x187,
        "MagicBazaar_Item_Four",
    ),
    item(
        5065,
        3,
        Key::new(*b"ABB", 4),
        0x188,
        "MagicBazaar_Item_Five",
    ),
    item(5066, 3, Key::new(*b"ABB", 5), 0x189, "MagicBazaar_Item_Six"),
];

/// Each researching building's research ids by code prefix: the records
/// of `0x1c` bytes at research table, research done's walk; 0 ends a
/// list.
pub const BY_BUILDING: [([u8; 3], [u32; 6]); 7] = [
    (*b"ABC", [5000, 5001, 5002, 5003, 5004, 5005]),
    (*b"ABD", [5010, 0, 0, 0, 0, 0]),
    (*b"ABE", [5020, 5021, 0, 0, 0, 0]),
    (*b"ABG", [5030, 5031, 5032, 5033, 5034, 5035]),
    (*b"ABH", [5040, 5041, 5042, 5043, 0, 0]),
    (*b"ABL", [5050, 0, 0, 0, 0, 0]),
    (*b"ABl", [5061, 5062, 5063, 5064, 5065, 5066]),
];

/// The researches play reads beyond research itself: the Blacksmith's
/// weapon and armour levels 2 to 4, the
/// Fairgrounds' tournament (its panel's contests), the Marketplace's
/// Market Day.
pub const WEAPON_LEVELS: [u32; 3] = [5000, 5002, 5004];
pub const ARMOUR_LEVELS: [u32; 3] = [5001, 5003, 5005];
pub const TOURNAMENT: u32 = 5010;
pub const MARKET_DAY: u32 = 5040;

/// The prefixes of a research's two GPL constants.
pub const TIME_PREFIX: &str = "#ResearchTime";
pub const COST_PREFIX: &str = "#ResearchCost";

/// A Library's discount per level: str2fixed of `"0.05"`
/// (`"0.05"`, static initialiser a static initialiser) is `3276` in 16.16, which
/// research price shifts right by 6 to scale 1024.
pub const LIBRARY_DISCOUNT: i32 = 51;

/// The Library's code prefix, whose level player building level finds.
pub const LIBRARY: [u8; 3] = *b"ABG";

/// `APB:44`, `CurrentResearchItem`.
pub const CURRENT: Key = Key::new(*b"APB", 44);

/// `APB:23`, `ResearchNotCompleted`.
pub(super) const NOT_COMPLETED: Key = Key::new(*b"APB", 23);

/// The Guardhouse's code prefix, compared with `0x454241` in
/// research done.
const GUARDHOUSE: [u8; 3] = *b"ABE";

/// The victory index research done tests (a compare with 3): Last Palace.
pub(super) const LAST_PALACE: i32 = crate::sim::freestyle::Victory::LastPalace
    .word()
    .cast_signed();

/// Research the player orders, by frame, and running.
#[derive(Clone, Debug, Default)]
pub(super) struct Researching(Vec<Pending>);

/// A research ordered, its time and cost read from the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pending {
    order: Research,
    item: Item,
    time: i32,
    cost: i32,
    /// The clock at the decode, once paid.
    started: Option<u32>,
}

/// Where a research stands after a frame's step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stand {
    Waiting,
    Running,
    Over,
}

/// The research table's row for `id`.
#[must_use]
pub fn of(id: u32) -> Option<Item> {
    ITEMS.iter().find(|i| i.id == id).copied()
}

/// Research `id`'s attribute, [`ITEMS`]'. In a `const`, an id the table
/// lacks fails the build.
#[must_use]
pub const fn attribute(id: u32) -> Key {
    let mut i = 0;

    while ITEMS[i].id != id {
        i += 1;
    }

    ITEMS[i].key
}

/// Whether `piece` has research `id` done: its attribute not 0.
#[must_use]
pub fn done(piece: &Piece, id: u32) -> bool {
    of(id).is_some_and(|i| piece.word(i.key) != 0)
}

/// The researches of a building of `code`, by its prefix
/// ([`BY_BUILDING`]), in the table's order; none for another building.
pub fn items(code: [u8; 4]) -> impl Iterator<Item = Item> {
    BY_BUILDING
        .iter()
        .filter(move |(p, _)| code.starts_with(p))
        .flat_map(|(_, ids)| ids.iter().filter_map(|i| of(*i)))
}

/// Whether a building of `code` researches: its prefix is one of
/// [`BY_BUILDING`]'s.
#[must_use]
pub fn researcher(code: [u8; 4]) -> bool {
    BY_BUILDING.iter().any(|(p, _)| code.starts_with(p))
}

/// `item`'s `#ResearchCost…` in `program`, 0 when absent.
#[must_use]
pub fn cost(program: &Program, item: Item) -> i32 {
    constant(program, &format!("{COST_PREFIX}{}", item.name))
}

/// `item`'s `#ResearchTime…` in `program`, 0 when absent.
#[must_use]
pub fn time(program: &Program, item: Item) -> i32 {
    constant(program, &format!("{TIME_PREFIX}{}", item.name))
}

/// Research price: `cost` less [`LIBRARY_DISCOUNT`] per Library level,
/// in scale 1024, truncated. A level-1 Library makes 250 into 237.
#[must_use]
pub const fn price(cost: i32, library: i32) -> i32 {
    let off = crate::gpl::ops::fixed_mul(library << 10, LIBRARY_DISCOUNT);
    let off = crate::gpl::ops::fixed_mul(off, cost << 10);

    ((cost << 10) - off) >> 10
}

/// A building's stage from its code's fourth letter: `'2'` or `'3'`,
/// else 1.
pub(super) const fn stage(code: [u8; 4]) -> u8 {
    match code[3] {
        b'2' => 2,
        b'3' => 3,
        _ => 1,
    }
}

/// Whether every research of `piece`'s building up to `stage` is done:
/// its attribute not 0.
pub(super) fn done_through(piece: &Piece, stage: u8) -> bool {
    items(piece.code.0)
        .filter(|i| i.stage <= stage)
        .all(|i| piece.word(i.key) != 0)
}

/// Research done's first test: in a game of victory
/// index `victory` 3, Last Palace, a Guardhouse's research is all done,
/// whatever its attributes.
pub(super) fn always_done(code: [u8; 4], victory: i32) -> bool {
    victory == LAST_PALACE && code.starts_with(&GUARDHOUSE)
}

/// An integer constant of the program, 0 when absent.
fn constant(program: &Program, name: &str) -> i32 {
    match program.constant(name) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

/// `order` waiting for its frame, its time and cost from `program`'s
/// constants; `None` for an id the table lacks.
fn pending(program: &Program, order: Research) -> Option<Pending> {
    let item = of(order.item)?;

    Some(Pending {
        order,
        item,
        time: time(program, item),
        cost: cost(program, item),
        started: None,
    })
}

impl World {
    /// Research `order` in its frame, as [`World::with_research`] does.
    pub fn research_later(&mut self, program: &Program, order: Research) {
        self.pending.research.0.extend(pending(program, order));
    }

    /// The frame's research: each order decoded the frame before, then
    /// each whose time is up. Called from [`World::step`], after the clock
    /// step.
    pub(super) fn research_step(&mut self) {
        let mut over = Vec::new();

        for i in 0..self.pending.research.0.len() {
            let mut p = self.pending.research.0[i];

            if self.advance(&mut p) == Stand::Over {
                over.push(i);
            }

            self.pending.research.0[i] = p;
        }

        for i in over.into_iter().rev() {
            self.pending.research.0.remove(i);
        }
    }

    /// One research at the frame's clock.
    fn advance(&mut self, p: &mut Pending) -> Stand {
        let decoded = (p.order.frame + 1) * MS_PER_FRAME;

        match p.started {
            None if self.now >= decoded => self.order_research(p),
            None => Stand::Waiting,
            Some(_) => Stand::Running,
        }
    }

    /// `GS_OrderProcessor`: refused while another research runs on
    /// the building, or when the owner is short; else paid and started.
    fn order_research(&mut self, p: &mut Pending) -> Stand {
        let id = p.order.building;
        let busy = self
            .pending
            .research
            .0
            .iter()
            .any(|q| q.order.building == id && q.started.is_some());
        let owner = self.pieces.owner(id);
        let price = price(p.cost, owner.map_or(0, |o| self.library_level(o)));
        let paid = owner
            .filter(|_| !busy)
            .is_some_and(|o| self.spend(o, price));

        // Busy, nothing; short, a chat line.
        if let (Some(o), false, false) = (owner, busy, paid) {
            self.say(o, Line::Text(NO_GOLD_RESEARCH));
        }

        if let (Some(o), true) = (owner, paid) {
            let now = self.now.cast_signed();

            // `GS_OrderProcessor`: the price counted as paid for research.
            self.count_paid(o, super::ledger::Spend::Research, price);
            self.write(id, CURRENT.packed(), p.item.id.cast_signed());
            self.write(id, BUTTON_START_TIME.packed(), now);
            self.write(id, BUTTON_DELAY_TIME.packed(), p.time);
            p.started = Some(self.now);
            self.time(id, Timer::Research, p.time.cast_unsigned());
            Stand::Running
        } else {
            Stand::Over
        }
    }

    /// Building `id`'s research timer ended ([`super::timed`]): its
    /// running research done.
    pub(super) fn research_over(&mut self, id: u32) {
        let at = self
            .pending
            .research
            .0
            .iter()
            .position(|p| p.order.building == id && p.started.is_some());

        if let Some(at) = at {
            let p = self.pending.research.0.remove(at);

            self.research_done(&p);
        }
    }

    /// Research done callback, callback `0x2009`.
    fn research_done(&mut self, p: &Pending) {
        let id = p.order.building;

        self.write(id, p.item.key.packed(), 1);
        self.advise(id);

        if self.researched(id) {
            self.write(id, NOT_COMPLETED.packed(), 0);
        }

        // "Library #1 Research Complete: Fire Blast".
        let owner = self.pieces.owner(id);

        if let Some((owner, building)) = owner.zip(self.who(id)) {
            let text = usize::try_from(p.item.text).unwrap_or_default();

            self.say_of(owner, Line::Researched { building, text }, Some(id));
        }

        self.write(id, CURRENT.packed(), 0);
        self.write(id, BUTTON_DELAY_TIME.packed(), 0);
    }

    /// The advisor's word that building `id`'s research is done, unplaced,
    /// its owner's.
    fn advise(&mut self, id: u32) {
        let piece = self.pieces.get(id).map(|p| (p.code.0, p.owner));
        let advisor = piece.and_then(|(code, owner)| {
            super::sound::ADVISORS
                .iter()
                .find(|(prefix, _)| code.starts_with(*prefix))
                .map(|(_, sound)| (*sound, owner))
        });

        if let Some((sound, owner)) = advisor {
            self.cue_owned(
                super::sound::From::Nowhere,
                sound,
                super::sound::ADVISOR,
                owner,
            );
        }
    }

    /// `research_done(unit, 0)`: every research of the building's
    /// code up to its stage has its attribute set.
    pub(super) fn researched(&self, id: u32) -> bool {
        self.pieces
            .get(id)
            .is_some_and(|piece| self.done_by(piece, stage(piece.code.0)))
    }

    /// Research done of `piece` through `stage`: [`always_done`] in
    /// this game, else [`done_through`].
    pub(super) fn done_by(&self, piece: &Piece, stage: u8) -> bool {
        always_done(piece.code.0, self.level.victory) || done_through(piece, stage)
    }

    /// `BuildingRec`'s `ResearchNotCompleted`: 1 only where
    /// research done says no, so none on a
    /// Guardhouse [`always_done`]. `attributes` are the body's, which set it
    /// for every researcher.
    pub(super) fn vf3_research(&self, code: [u8; 4], attributes: &mut BTreeMap<i32, i32>) {
        if always_done(code, self.level.victory) {
            attributes.remove(&NOT_COMPLETED.packed());
        }
    }

    /// `player_building_level(LIBRARY, 0)` on player `owner`: the highest level of
    /// its Libraries ([`World::building_level`]).
    pub(super) fn library_level(&self, owner: u32) -> i32 {
        self.building_level(owner, LIBRARY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::fourcc::FourCc;

    /// **Binary**: in a Last Palace game (victory index 3) a Guardhouse's
    /// research is done before any is (research done,
    /// research done): no `ResearchNotCompleted` as it is made, and its
    /// stage 2 research shown with stage 1's undone.
    /// Another researcher is
    /// as before.
    #[test]
    fn a_last_palace_guardhouse_has_done_its_research() {
        let program = Program::default();
        let guardhouse = Piece {
            code: FourCc(*b"ABE2"),
            ..Piece::default()
        };
        let library = Piece {
            code: FourCc(*b"ABG1"),
            ..Piece::default()
        };
        let mut world = World::default();

        world.pieces.insert(10, guardhouse);
        world.pieces.insert(11, library);

        let shown = |world: &World| -> Vec<u32> {
            world
                .researches(&program, 10)
                .iter()
                .map(|b| b.what.id)
                .collect()
        };
        let made = |world: &World, code: &[u8; 4]| {
            let mut attributes = BTreeMap::from([(NOT_COMPLETED.packed(), 1)]);

            world.vf3_research(*code, &mut attributes);
            attributes.is_empty()
        };

        assert!(!world.researched(10));
        assert_eq!(shown(&world), [5020]);
        assert!(!made(&world, b"ABE1"));

        world.level.victory = LAST_PALACE;

        assert!(world.researched(10));
        assert!(!world.researched(11));
        assert_eq!(shown(&world), [5020, 5021]);
        assert!(made(&world, b"ABE1"));
        assert!(!made(&world, b"ABG1"));
    }

    #[test]
    fn a_library_takes_five_percent_off() {
        assert_eq!(price(250, 0), 250);
        assert_eq!(price(250, 1), 237);
        assert_eq!(price(300, 1), 285);
    }

    #[test]
    fn every_researcher_lists_ids_the_table_has() {
        assert!(
            BY_BUILDING
                .iter()
                .flat_map(|(_, ids)| ids.iter())
                .all(|i| *i == 0 || of(*i).is_some())
        );
    }

    #[test]
    fn every_effect_is_a_research_of_the_table() {
        use crate::data::attrib;

        let effects = WEAPON_LEVELS
            .iter()
            .chain(&ARMOUR_LEVELS)
            .chain(&[TOURNAMENT, MARKET_DAY]);

        for id in effects {
            let Key { bank, index } = of(*id).expect("a research").key;
            let name = attrib::name(bank, index).unwrap_or_default();

            assert!(name.starts_with("Research"), "{id}: {name}");
        }

        assert_eq!(attribute(TOURNAMENT), Key::new(*b"APB", 30));
        assert_eq!(attribute(MARKET_DAY), Key::new(*b"APB", 37));
    }

    #[test]
    fn a_stage_is_the_codes_fourth_letter() {
        assert_eq!(stage(*b"ABE2"), 2);
        assert_eq!(stage(FourCc(*b"ABG1").0), 1);
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Research {
        type Model = m::Research;

        fn save(&self) -> m::Research {
            m::Research {
                building: self.building,
                item: self.item,
                frame: self.frame,
            }
        }

        fn load(model: m::Research) -> Result<Research, Refused> {
            Ok(Research {
                building: model.building,
                item: model.item,
                frame: model.frame,
            })
        }
    }

    /// An item by its id in [`ITEMS`].
    impl Saved for Pending {
        type Model = m::PendingResearch;

        fn save(&self) -> m::PendingResearch {
            let Pending {
                order,
                item,
                time,
                cost,
                started,
            } = self;

            m::PendingResearch {
                order: order.save(),
                item: item.id,
                time: *time,
                cost: *cost,
                started: *started,
            }
        }

        fn load(model: m::PendingResearch) -> Result<Pending, Refused> {
            let item = ITEMS
                .iter()
                .find(|i| i.id == model.item)
                .copied()
                .ok_or_else(|| Refused::Missing(format!("research item {}", model.item)))?;

            Ok(Pending {
                order: Research::load(model.order)?,
                item,
                time: model.time,
                cost: model.cost,
                started: model.started,
            })
        }
    }

    impl Saved for Researching {
        type Model = Vec<m::PendingResearch>;

        fn save(&self) -> Self::Model {
            self.0.save()
        }

        fn load(model: Self::Model) -> Result<Researching, Refused> {
            Vec::load(model).map(Researching)
        }
    }
}
