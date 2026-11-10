//! What the player's input costs. **Binary** unless marked.
//!
//! ```text
//!  building_price(owner, code, out, free, flag)     a building's price
//!    p = Cost (BuildingDesc 0x08); m = Multiplier (0x14) in scale 1024
//!    each building of the owner standing:
//!      the first Blacksmith (ABC) with FirstStageBuilt: p -= p x GameValue1
//!      flag 0, the same code prefix, past `free` of them: p = m x p / 1024
//!    at most the gold cap
//!  a placed building           (owner, code, out, 0, 0): every standing one
//!  a building upgrade          (owner, UpgradeTo, out, 1, 1): none does
//!  a recruit                   vehicle price: the character's Cost
//! ```
//!
//! `GameValue1` is a settings key no shipped `GOPT` sets; a static
//! initialiser makes it `str2fixed("0.05")`, so a
//! built Blacksmith takes 5% off every building and upgrade. **Game**:
//! play5's treasury at the save is the world's with it, 130 short without.
//! The loop's order is the unit list's; the world takes the discount
//! first (**Inferred**: play5 builds nothing more after its Blacksmith).
//! Every price is taken by `GE_Player`, which refuses a short
//! treasury.
//!
//! `GenUnit` is 3 where `BuildingRec` holds: ruined, or `AE:0`
//! 0. So a building stops counting at `building_death`'s `DeleteAgent`, not
//! when its rubble goes, and a fallen Blacksmith discounts nothing.
//!
//! Under Last Palace (level victory index 3, building price) five
//! codes take `m` from an `#LPS_` constant instead ([`LPS`]), read once:
//! `(c << 20) / 10240`, the constant tenths in scale 1024.
//! The Inn's 12 is 1228.

use super::{AGENT_ID, Piece, World, bodies::Class, offer::SPELLS, spell::cast::SPELL_UNITS};
use crate::base::digest::Map;
use crate::base::fixed::Fixed;
use crate::data::attrib::FIRST_STAGE_BUILT;
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::fixed_div;
use crate::sim::world::treasury::GOLD_CAP;

/// `GameValue1`, 16.16: `str2fixed("0.05")` (`"0.05"`) at
/// a static initialiser.
pub const GAME_VALUE_1: Fixed = Fixed::from_bits(3276);

/// `GameValue2`, 16.16: `str2fixed("0.125")` (`"0.125"`) at
/// a static initialiser; building refund's multiplier.
pub const GAME_VALUE_2: Fixed = Fixed::from_bits(8192);

/// 16.16's unit, 2^-16 (`0x37800000`): `GameValue1` scaled into an `f32`.
const FIXED_16: f32 = 1.0 / 65_536.0;

/// The Blacksmith's code prefix, whose presence discounts a price.
pub const BLACKSMITH: [u8; 3] = *b"ABC";

/// Last Palace's multipliers: code prefix and the `#LPS_` constant, in
/// the order building price reads them.
pub const LPS: [([u8; 3], &str); 5] = [
    (*b"ABB", "#LPS_BallistaMultiplier"),
    (*b"ABM", "#LPS_WizTowerMultiplier"),
    (*b"ABE", "#LPS_GuardhseMultiplier"),
    (*b"ABh", "#LPS_RylGardnMultiplier"),
    (*b"ABF", "#LPS_InnMultiplier"),
];

/// What building price divides an `#LPS_` constant by, shifted
/// by 10 twice: tenths (`0x2800`).
pub const LPS_DIVISOR: i32 = 0x2800;

/// An `#LPS_` constant as a multiplier in scale 1024: `c << 10` by
/// fixed10 div. Where a 32-bit signed divide faults the engine stops; here 0.
#[must_use]
pub fn lps_multiplier(c: i32) -> i32 {
    fixed_div(c.wrapping_shl(10), LPS_DIVISOR).unwrap_or_default()
}

/// [`LPS`]'s multipliers, in scale 1024, as the program gives them:
/// `None` before [`World::read_lps`]. A constant the program lacks is 0
/// (**Inferred**: gpl expression int of no definition).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Lps(Option<[i32; 5]>);

/// Float to fixed10: an `f32` to scale 1024, truncated toward zero; under
/// 1/1024 in size, 0. The Inn's 1.1 is 1126.
#[must_use]
pub fn fixed10(value: f32) -> i32 {
    let bits = value.to_bits().cast_signed();
    let shift = 0x8c - ((bits >> 23) & 0xff);
    let mantissa = (bits & 0x7f_ffff) | 0x80_0000;
    let magnitude = if value.abs() < 1.0 / 1024.0 {
        0
    } else if shift < 1 {
        mantissa << (-shift & 0x1f)
    } else {
        mantissa >> (shift & 0x1f)
    };

    if bits < 0 {
        magnitude.wrapping_neg()
    } else {
        magnitude
    }
}

/// The Blacksmith's discount: `p - p x GameValue1`, `GameValue1` scaled
/// by 2^-16 into an `f32`, the rest in x87 precision, truncated.
/// 1000 is 950, 600 is 570.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "an f32 constant, then float to int's truncation"
)]
pub fn discount(p: i32) -> i32 {
    let g = f64::from(GAME_VALUE_1.to_bits()) * f64::from(FIXED_16);
    let g = f64::from(g as f32);
    let p = f64::from(p);

    (p - p * g) as i32
}

/// One more building of a code: `p` times the multiplier `m`, scale 1024,
/// truncated (as floats, float to int at
/// building price). 400 at 1126 is 439.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "float to int keeps the low word"
)]
pub const fn again(p: i32, m: i32) -> i32 {
    ((m as i64 * p as i64) >> 10) as i32
}

/// The price of the spell that makes unit `code`: its GPL constant, 0 for
/// a unit no spell in [`SPELL_UNITS`] makes.
pub(super) fn spell_price(program: &Program, code: FourCc) -> i32 {
    SPELL_UNITS
        .iter()
        .find(|(unit, _, _)| *unit == code.0)
        .map_or(0, |(_, spell, _)| cost(program, FourCc(*spell)))
}

/// Spell `spell`'s price: its [`SPELLS`] row's GPL constant, else 0.
pub(super) fn cost(program: &Program, spell: FourCc) -> i32 {
    let name = SPELLS.iter().find(|r| r.spell == spell).map(|r| r.cost);

    match name.and_then(|n| program.constant(n)) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

/// Whether an owner's prices take the Blacksmith's discount
/// ([`World::smith`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Smith {
    Built,
    None,
}

impl Smith {
    /// `p`, discounted where the Blacksmith is built.
    fn off(self, p: i32) -> i32 {
        match self {
            Smith::Built => discount(p),
            Smith::None => p,
        }
    }
}

/// `GenUnit` under 3 for a building: `BuildingRec` false, so
/// neither ruined nor without `AE:0`.
pub(super) fn stands(piece: &Piece) -> bool {
    piece.word(AGENT_ID) != 0 && !piece.place.is_some_and(|place| place.ruined())
}

impl World {
    /// [`LPS`]'s constants from `program`, as building price reads
    /// them on first use.
    pub fn read_lps(&mut self, program: &Program) {
        self.lps = Lps(Some(LPS.map(|(_, name)| match program.constant(name) {
            Some(Literal::Int(c)) => lps_multiplier(c),
            _ => 0,
        })));
    }

    /// `code`'s multiplier: [`LPS`]'s under Last Palace, else `m`.
    fn multiplier(&self, code: FourCc, m: i32) -> i32 {
        let last = self.level.victory == super::research::LAST_PALACE;
        let lps = self.lps.0.filter(|_| last).and_then(|ms| {
            LPS.iter()
                .zip(ms)
                .find(|((prefix, _), _)| code.0.starts_with(prefix))
                .map(|(_, m)| m)
        });

        lps.unwrap_or(m)
    }

    /// `building_price(owner, code, _, 0, 0)`: the price of one more
    /// building of `code` for `owner`, its cost multiplied once for each
    /// of the owner's standing buildings of the same code prefix.
    #[must_use]
    pub fn building_price(&self, owner: u32, code: FourCc) -> i32 {
        self.price_past(owner, code, 0)
    }

    /// `building_price(owner, code, _, free, 0)`: the cost multiplied once
    /// for each of the owner's standing buildings of the code's prefix
    /// past the first `free`.
    pub(super) fn price_past(&self, owner: u32, code: FourCc, free: usize) -> i32 {
        let prefix = &code.0[..3];
        let same = self
            .pieces
            .values()
            .filter(|p| p.owner == owner && p.code.0.starts_with(prefix))
            .filter(|p| stands(p) && self.building_code(p.code.0))
            .count();

        self.price_of(code, (free, same), self.smith(owner))
    }

    /// `owner`'s buildings by code prefix, as [`World::price_past`]
    /// counts them: once for a whole menu, not a walk of every piece
    /// per code.
    pub(super) fn buildings_by_prefix(&self, owner: u32) -> Map<[u8; 3], usize> {
        let mut out: Map<[u8; 3], usize> = Map::default();
        let prefixes = self
            .pieces
            .values()
            .filter(|p| p.owner == owner && stands(p) && self.building_code(p.code.0))
            .map(|p| [p.code.0[0], p.code.0[1], p.code.0[2]]);

        for prefix in prefixes {
            *out.entry(prefix).or_default() += 1;
        }

        out
    }

    /// Whether `code`'s descriptor is a building's.
    fn building_code(&self, code: [u8; 4]) -> bool {
        self.descriptors.class(FourCc(code)) == Some(Class::Building)
    }

    /// [`World::price_past`] with the free and the owner's same-prefix
    /// buildings counted, and its [`Smith`].
    pub(super) fn price_of(&self, code: FourCc, (free, same): (usize, usize), smith: Smith) -> i32 {
        let body = self.descriptors.body(code);
        let (cost, m) = body.map_or((0, 0), |b| (b.cost, b.multiplier));
        let m = self.multiplier(code, m);

        (free..same)
            .fold(smith.off(cost), |p, _| again(p, m))
            .min(GOLD_CAP)
    }

    /// `building_price(owner, code, _, 1, 1)`: the price of the upgrade to
    /// `code`, its cost, no multiplier.
    pub(super) fn upgrade_price(&self, owner: u32, code: FourCc) -> i32 {
        let cost = self.descriptors.body(code).map_or(0, |b| b.cost);

        self.discounted(owner, cost).min(GOLD_CAP)
    }

    /// `p` less the Blacksmith's discount where `owner` has a standing
    /// Blacksmith with `FirstStageBuilt`.
    fn discounted(&self, owner: u32, p: i32) -> i32 {
        self.smith(owner).off(p)
    }

    /// Whether `owner` has a standing Blacksmith with `FirstStageBuilt`.
    pub(super) fn smith(&self, owner: u32) -> Smith {
        let built = self.pieces.values().any(|b| {
            b.owner == owner
                && stands(b)
                && b.code.0.starts_with(&BLACKSMITH)
                && b.get(FIRST_STAGE_BUILT).is_some_and(|v| v != 0)
        });

        if built { Smith::Built } else { Smith::None }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        AGENT_ID,
        board::Board,
        places::Motion,
        ruin::{
            RUBBLE_MS,
            tests::{body, standing},
        },
    };
    use super::*;
    use crate::sim::clearing::Time;
    use crate::sim::world::kind;

    const INN: [u8; 4] = *b"ABF1";
    const SMITH: [u8; 4] = *b"ABC1";

    /// The Inn's cost, and that cost with one more Inn or a Blacksmith.
    const COST: i32 = 400;
    const SECOND: i32 = 439;
    const OFF: i32 = 380;

    /// A world holding a standing building of `code` at agent 12, half
    /// cell (20, 20), built, with an agent; an Inn's descriptor beside it.
    fn with_standing(code: [u8; 4]) -> World {
        let mut world = World {
            board: Board::sized((64, 64)),
            ..World::default()
        };
        let mut inn = body(Class::Building, 0x30, vec![(0, 0)]);
        let mut piece = standing(code, 100, kind::Kind::BUILDING, (20, 20));

        inn.cost = COST;
        inn.multiplier = fixed10(1.1);
        world.descriptors_mut().add_body(FourCc(INN), inn);

        if code != INN {
            world
                .descriptors_mut()
                .add_body(FourCc(code), body(Class::Building, 0x30, vec![(0, 0)]));
        }

        piece.attributes.insert(AGENT_ID.packed(), 12);
        piece.attributes.insert(FIRST_STAGE_BUILT.packed(), 1);
        world.pieces.insert(12, piece);
        world.units.set_motion(12, Motion::Still);
        world
    }

    /// Building price skips `GenUnit` 3 and over: a ruin is
    /// no building of its prefix, in the price or the menu's count.
    #[test]
    fn a_ruin_does_not_raise_a_price() {
        let mut world = with_standing(INN);

        assert_eq!(world.building_price(0, FourCc(INN)), SECOND);

        world.collapse(12);

        assert_eq!(world.building_price(0, FourCc(INN)), COST);
        assert!(world.buildings_by_prefix(0).is_empty());
    }

    /// `BuildingRec` holds as soon as `DeleteAgent` writes `AE:0` 0,
    /// so the price falls as the building dies, before its `Crumble` peaks.
    #[test]
    fn an_agentless_building_does_not_raise_a_price() {
        let mut world = with_standing(INN);

        world.orphan(12);

        assert_eq!(world.building_price(0, FourCc(INN)), COST);
    }

    /// The Blacksmith's test sits behind the same skip: its ruin discounts
    /// nothing, though `APB:1` stays set.
    #[test]
    fn a_ruined_blacksmith_gives_no_discount() {
        let mut world = with_standing(SMITH);

        assert_eq!(world.building_price(0, FourCc(INN)), OFF);

        world.collapse(12);

        assert_eq!(world.building_price(0, FourCc(INN)), COST);
    }

    /// A placement's ring and the rubble order's end each delete the ruin,
    /// with its order.
    #[test]
    fn a_cleared_ruin_leaves_the_price_alone() {
        let mut built_over = with_standing(INN);
        let mut hall = body(Class::Building, 0x30, vec![(0, 0)]);

        hall.mask = Some(crate::sim::placement::Pad {
            origin: (0, 0),
            size: (1, 1),
            covered: vec![true],
        });

        // As `building_death` goes: `DeleteAgent`, then the ruin.
        built_over.orphan(12);
        built_over.collapse(12);

        let mut timed = built_over.clone();

        built_over.clear(&hall, (20, 20), Time::Playing);
        assert!(!built_over.pieces.contains(12));
        assert_eq!(built_over.rubble_end(12), None);
        assert_eq!(built_over.building_price(0, FourCc(INN)), COST);

        timed.now += RUBBLE_MS;
        timed.fall_step();
        assert!(!timed.pieces.contains(12));
        assert_eq!(timed.building_price(0, FourCc(INN)), COST);
    }

    /// `globals.gpl`'s five `#LPS_` constants.
    const LPS_SOURCE: &str = "expression #LPS_BallistaMultiplier 25\n\
         expression #LPS_WizTowerMultiplier 18\n\
         expression #LPS_GuardhseMultiplier 15\n\
         expression #LPS_RylGardnMultiplier 12\n\
         expression #LPS_InnMultiplier 12\n";

    /// A second Inn under Last Palace: 400 at `#LPS_InnMultiplier` 12.
    const SECOND_LPS: i32 = 479;

    /// Under Last Palace a second Inn costs by `#LPS_InnMultiplier`, not
    /// its descriptor's 1.1; in any other game, by the descriptor.
    #[test]
    fn last_palace_prices_by_the_lps_constant() {
        let bytes = crate::gpl::emit::compile_source(LPS_SOURCE, "test.gpl").expect("compiles");
        let program = Program::load(&[bytes]).expect("loads");
        let mut world = with_standing(INN);

        world.read_lps(&program);
        assert_eq!(world.building_price(0, FourCc(INN)), SECOND);

        world.level.victory = super::super::research::LAST_PALACE;
        assert_eq!(world.building_price(0, FourCc(INN)), SECOND_LPS);
    }

    #[test]
    fn an_lps_constant_is_tenths_in_scale_1024() {
        assert_eq!(lps_multiplier(25), 2560);
        assert_eq!(lps_multiplier(18), 1843);
        assert_eq!(lps_multiplier(15), 1536);
        assert_eq!(lps_multiplier(12), 1228);
    }

    #[test]
    fn a_multiplier_is_truncated_to_scale_1024() {
        assert_eq!(fixed10(1.1), 1126);
        assert_eq!(fixed10(1.0), 1024);
        assert_eq!(fixed10(0.0), 0);
        assert_eq!(fixed10(-1.5), -1536);
    }

    #[test]
    fn the_16_16_unit_is_2_to_the_minus_16_exactly() {
        assert_eq!(FIXED_16.to_bits(), 0x3780_0000);
        assert_eq!(FIXED_16.to_bits(), 2.0f32.powi(-16).to_bits());
    }

    #[test]
    fn a_blacksmith_takes_a_twentieth_off() {
        assert_eq!(discount(1000), 950);
        assert_eq!(discount(600), 570);
    }

    #[test]
    fn a_second_inn_costs_a_tenth_more_truncated() {
        assert_eq!(again(400, fixed10(1.1)), 439);
    }
}
