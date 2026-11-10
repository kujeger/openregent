//! The player's spells: `PLAYER_SPELL_MESSAGE`. **Binary** unless marked.
//!
//! ```text
//!  frame f  GSMessageManager       case 0x3f1, player spell decode:
//!   the spell applies (spell, _, player, x, y, target, price)
//!    price above APP:0      a chat line (0x22), nothing more
//!    Sp15 `SuperCharge`       the palace: refused while APB:9; APB:51 the
//!                           clock; SuperCharge_Begin(palace)
//!    a unit                 made at the point (Gate
//!                           searches), NewUnitInit, its PRIM callback
//!                           (unit, target); Sp12 and Sp30 on a building
//!    else, a target         none: refused
//!     APV:44, not ours      refused (chat 0x2a): its magic mirror
//!     Sp24                  APU:0 above 0: an illusion (World::illusion)
//!     SPELL_SCRIPTS         its guard, then its function(target)
//!    not refused            the price (GE_Player)
//!    Sp32 to Sp37           refused too: the ABq's ASS:0 the clock,
//!                           ASP:0 #SpellDuration_… (ABODE_SPELLS)
//! ```
//!
//! The price rides in the message: the spell table's (spell table build,
//! records of guild code, panel, stage, `#SpellCost_…`, spell code) as the
//! spell panel set it. Spell unit code names each unit a spell makes.
//!
//! **Game**: play5's four `farseeing` units stand where the probe first
//! prints them until `UnitDelete` ends each; with 200 each the treasury at
//! the save is the save's. Its `DontBlock` keeps it off the grid; it
//! reveals its descriptor's `SightRange` as it is placed.
//!
//! **Measured**, whole-log replays of every spell a trace casts:
//! invisibility, anti-magic shield, lightning bolt on a unit, Farseeing,
//! Lightning Storm, Stone Skin, Petrify, Vigilance, Gate, `SuperCharge`
//! and Dismiss. The rest, Illusionary Hero among them, no trace casts.

use crate::data::attrib::{
    HAS_EFFECT_ANTI_MAGIC, HAS_EFFECT_INVISIBILITY, HAS_EFFECT_MAGIC_MIRROR, Key,
    SORCERERS_ABODE_SPELL_START_TIME,
};
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::vm::Fault;
use crate::sim::space;
use crate::sim::world::chat::{Line, MIRRORED, NO_GOLD_SPELL};
use crate::sim::world::kind;
use crate::sim::world::price::{cost, spell_price};
use crate::sim::world::{Piece, Place, World};

/// The spells that make a unit: the unit, the spell
/// code, and the name of its price's GPL constant.
/// Lightning bolt and fire strike make theirs only on a building
/// ([`BUILDING_UNITS`]).
pub const SPELL_UNITS: [([u8; 4], [u8; 4], &str); 8] = [
    (*b"WVA1", *b"Sp10", "#SpellCost_Farseeing"),
    (*b"WVC1", *b"Sp12", "#SpellCost_LightningBolt"),
    (*b"WVE1", *b"Sp14", "#SpellCost_LightningStorm"),
    (*b"DVB1", *b"Sp29", "#SpellCost_WindStorm"),
    (*b"SVA1", *b"Sp30", "#SpellCost_FireStrike"),
    (*b"SVB1", *b"Sp31", "#SpellCost_SunScorch"),
    (*b"WVi1", *b"Sp34", "#SpellCost_EarthQuake"),
    (*b"WVh1", *b"Sp36", "#SpellCost_Gate"),
];

/// The spells whose unit spell unit code makes only for a target that is a
/// building (`GSUnit` method `(0)`, spell unit code); on any other target their
/// function runs ([`SPELL_SCRIPTS`]).
pub const BUILDING_UNITS: [[u8; 4]; 2] = [*b"Sp12", *b"Sp30"];

/// What refuses a spell cast on a unit besides the treasury.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Guard {
    /// Nothing.
    Free,
    /// The target's key not 0: the spell is on it already.
    Key(Key),
    /// Hit points at their maximum, unless plagued ([`PLAGUE`]): the
    /// healings.
    Hurt,
}

/// The spells cast on a unit that make no unit of their own:
/// the spell code, the function
/// dispatched with the target's agent, its price's GPL constant, and its
/// [`Guard`].
pub const SPELL_SCRIPTS: [Blessing; 20] = [
    (
        *b"Sp11",
        "Invisibility_Begin",
        "#SpellCost_Invisibility",
        Guard::Key(HAS_EFFECT_INVISIBILITY),
    ),
    (
        *b"Sp12",
        "Lightning_Bolt_Hit",
        "#SpellCost_LightningBolt",
        Guard::Free,
    ),
    (
        *b"Sp13",
        "Anti_Magic_Shield_Begin",
        "#SpellCost_AntiMagicShield",
        Guard::Key(HAS_EFFECT_ANTI_MAGIC),
    ),
    (
        *b"Sp16",
        "Agrela_Heal_Effect",
        "#SpellCost_AgrelaHealing",
        Guard::Hurt,
    ),
    (
        *b"Sp17",
        "Blessing_Begin",
        "#SpellCost_Blessing",
        Guard::Key(Key::new(*b"APV", 36)),
    ),
    (
        *b"Sp18",
        "Resurrection_Begin",
        "#SpellCost_Resurrection",
        Guard::Free,
    ),
    (
        *b"Sp19",
        "Dauros_Stone_Skin_Begin",
        "#SpellCost_StoneSkin",
        Guard::Key(Key::new(*b"APV", 40)),
    ),
    (
        *b"Sp20",
        "Petrify_Begin",
        "#SpellCost_Petrify",
        Guard::Key(Key::new(*b"APV", 38)),
    ),
    (
        *b"Sp21",
        "Vigilance_Begin",
        "#SpellCost_Vigilance",
        Guard::Key(Key::new(*b"APV", 41)),
    ),
    (
        *b"Sp22",
        "Fervus_Heal_Effect",
        "#SpellCost_FervusHealing",
        Guard::Hurt,
    ),
    (
        *b"Sp23",
        "Vines_Begin",
        "#SpellCost_Vines",
        Guard::Key(Key::new(*b"APV", 39)),
    ),
    (
        *b"Sp25",
        "Animate_Bones_Effect",
        "#SpellCost_AnimateBones",
        Guard::Key(Key::new(*b"APV", 62)),
    ),
    (
        *b"Sp26",
        "Wither_Begin",
        "#SpellCost_Wither",
        Guard::Key(Key::new(*b"APV", 43)),
    ),
    (
        *b"Sp27",
        "Reanimate_Begin",
        "#SpellCost_ReAnimate",
        Guard::Free,
    ),
    (
        *b"Sp28",
        "Winged_Feet_Begin",
        "#SpellCost_WingedFeet",
        Guard::Key(Key::new(*b"APV", 42)),
    ),
    (
        *b"Sp30",
        "Fire_Strike_Hit",
        "#SpellCost_FireStrike",
        Guard::Free,
    ),
    (
        *b"Sp32",
        "ChangeOfHeart_Begin",
        "#SpellCost_ChangeOfHeart",
        Guard::Free,
    ),
    (
        *b"Sp33",
        "FrostField_Begin",
        "#SpellCost_FrostField",
        Guard::Free,
    ),
    (
        *b"Sp35",
        "ChainLightning_Begin",
        "#SpellCost_ChainLightning",
        Guard::Free,
    ),
    (*b"Sp37", "Dismiss_Begin", "#SpellCost_Dismiss", Guard::Free),
];

/// One of [`SPELL_SCRIPTS`]: spell code, function, price, guard.
pub type Blessing = ([u8; 4], &'static str, &'static str, Guard);

/// `SuperCharge`: no target; its function takes the caster's palace.
pub const SUPERCHARGE: [u8; 4] = *b"Sp15";

/// What `SuperCharge` dispatches with the palace.
pub const SUPERCHARGE_BEGIN: &str = "SuperCharge_Begin";

/// `SuperChargeActive`, `APB:9`: the palace refuses `SuperCharge` while set.
/// `SuperCharge_Begin` sets it, `SuperCharge_End` clears.
pub const SUPERCHARGE_ACTIVE: Key = Key::new(*b"APB", 9);

/// `SuperChargeBeginTime`, `APB:51`: the clock as `SuperCharge` is cast.
pub const SUPERCHARGE_BEGUN: Key = Key::new(*b"APB", 51);

/// Illusionary Hero: its unit is the engine's (`World::illusion`).
pub const ILLUSION: [u8; 4] = *b"Sp24";

/// `HasEffectRatmanPlague`, `APV:60`: a plagued unit takes a healing at
/// full hit points.
pub const PLAGUE: Key = Key::new(*b"APV", 60);

/// The Sorcerer's Abode, whose spells share one timer.
pub const ABODE: [u8; 3] = *b"ABq";

/// `SorcerersAbodeSpellDuration`, `ASP:0`: how long the timer runs, ms.
pub const ABODE_LASTS: Key = Key::new(*b"ASP", 0);

/// The Abode's spells and their timer's GPL constant (player spell apply on,
/// player spell apply on).
pub const ABODE_SPELLS: [([u8; 4], &str); 6] = [
    (*b"Sp32", "#SpellDuration_ChangeOfHeart"),
    (*b"Sp33", "#SpellDuration_FrostField"),
    (*b"Sp34", "#SpellDuration_Earthquake"),
    (*b"Sp35", "#SpellDuration_ChainLightning"),
    (*b"Sp36", "#SpellDuration_Gate"),
    (*b"Sp37", "#SpellDuration_Dismiss"),
];

/// A `PLAYER_SPELL_MESSAGE` that is not [`Cast`]: the spell code, the
/// target's agent (0 for `SuperCharge`), the caster's player, and the point
/// clicked, 32nds, where a spell's unit stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bless {
    pub spell: FourCc,
    pub target: u32,
    pub owner: u32,
    pub at: (i32, i32),
}

/// What a [`Bless`] does once paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blessed {
    /// This function dispatched with this agent.
    Script(&'static str, u32),
    /// A unit made: its `NewUnitInit`, then its `PRIM` callback with this
    /// target where given.
    Made(u32, Option<u32>),
}

/// `PLAYER_SPELL_MESSAGE`'s unit: its descriptor, point, 32nds of a
/// cell, and owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cast {
    pub code: FourCc,
    pub at: (i32, i32),
    pub owner: u32,
}

impl World {
    /// Descriptor `code`'s `PRIM` callback, where it has one: a spell
    /// unit's runs after its `NewUnitInit`.
    #[must_use]
    pub fn script_of(&self, code: FourCc) -> Option<crate::sim::world::missile::Script> {
        self.descriptors.body(code).and_then(|b| b.script.clone())
    }

    /// The spell's unit and agent: the next agent id and handle, at its
    /// point. The caller dispatches `NewUnitInit`.
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub fn cast(&mut self, program: &Program, cast: &Cast) -> Result<Option<u32>, Fault> {
        let price = spell_price(program, cast.code);
        let short = !self.affords(cast.owner, price);

        // A short treasury: a chat line, nothing more.
        if short {
            self.say(cast.owner, Line::Text(NO_GOLD_SPELL));
        }

        match self.descriptors.body(cast.code).cloned().filter(|_| !short) {
            Some(body) => {
                let id = self.stand_spell(program, cast, &body)?;

                // Spell unit animate: its own `Effector Begin`.
                self.cue(
                    crate::sim::world::sound::From::Agent(id),
                    crate::sim::world::sound::Voice::Own,
                    crate::sim::world::sound::Phase::Id(crate::sim::world::sound::EFFECTOR_BEGIN),
                    0.0,
                );
                self.spend(cast.owner, price);
                self.count_paid(cast.owner, crate::sim::world::ledger::Spend::Spells, price);

                // Earthquake's and Gate's timer on the Abode.
                if let Some((_, spell, _)) = SPELL_UNITS.iter().find(|(u, ..)| *u == cast.code.0) {
                    self.abode_timer(program, cast.owner, FourCc(*spell));
                }

                Ok(Some(id))
            }
            None => Ok(None),
        }
    }

    /// Player spell apply for a spell that is not [`Cast`]: what it
    /// does, the price taken; `None` where the treasury is short or the
    /// spell refused (**Inferred**: refused, nothing paid; the jumps skip
    /// player spell apply). An Abode spell starts its timer refused or not.
    ///
    /// # Errors
    ///
    /// A unit's agent's `.dat` instance is missing.
    pub fn bless(&mut self, program: &Program, bless: &Bless) -> Result<Option<Blessed>, Fault> {
        let price = cost(program, bless.spell);
        let short = !self.affords(bless.owner, price);
        let on_building = self
            .piece_body(bless.target)
            .is_some_and(|b| b.class == crate::sim::world::bodies::Class::Building);
        let unit = SPELL_UNITS
            .iter()
            .find(|(_, s, _)| *s == bless.spell.0)
            .filter(|_| on_building && BUILDING_UNITS.contains(&bless.spell.0));

        match (short, unit) {
            (true, _) => {
                self.say(bless.owner, Line::Text(NO_GOLD_SPELL));
                Ok(None)
            }

            // Lightning bolt or fire strike on a building: its unit at the
            // point, its callback given the building.
            (false, Some((code, ..))) => {
                let cast = Cast {
                    code: FourCc(*code),
                    at: bless.at,
                    owner: bless.owner,
                };
                let made = self.cast(program, &cast)?;

                Ok(made.map(|id| Blessed::Made(id, Some(bless.target))))
            }
            (false, None) => {
                let blessed = self.blessed(program, bless)?;

                if blessed.is_some() {
                    self.spend(bless.owner, price);
                    self.count_paid(bless.owner, crate::sim::world::ledger::Spend::Spells, price);
                }

                self.abode_timer(program, bless.owner, bless.spell);
                Ok(blessed)
            }
        }
    }

    /// The switch on the spell code, the treasury met: what it does, or
    /// `None` where refused.
    fn blessed(&mut self, program: &Program, bless: &Bless) -> Result<Option<Blessed>, Fault> {
        let target = self.pieces.get(bless.target);
        let mirrored =
            target.is_some_and(|p| p.word(HAS_EFFECT_MAGIC_MIRROR) != 0 && p.owner != bless.owner);
        let row = SPELL_SCRIPTS
            .iter()
            .find(|(code, ..)| *code == bless.spell.0);

        match (bless.spell.0, target) {
            (SUPERCHARGE, _) => Ok(self.supercharge(bless.owner)),
            (_, None) => Ok(None),
            _ if mirrored => {
                self.say(bless.owner, Line::Text(MIRRORED));
                Ok(None)
            }
            (ILLUSION, Some(t)) if t.word(crate::data::attrib::HP) > 0 => {
                let made = self.illusion(program, bless.target)?;

                Ok(made.map(|id| Blessed::Made(id, None)))
            }
            (_, Some(t)) => Ok(row
                .filter(|(.., guard)| !refuses(*guard, t))
                .map(|(_, function, ..)| Blessed::Script(function, bless.target))),
        }
    }

    /// `SuperCharge`: on the owner's palace, unless running; its
    /// `SuperChargeBeginTime` the clock.
    fn supercharge(&mut self, owner: u32) -> Option<Blessed> {
        let palace = self.palace_of(owner);
        let piece = self.pieces.get_mut(palace)?;
        let running = piece.word(SUPERCHARGE_ACTIVE) != 0;

        if running {
            None
        } else {
            piece
                .attributes
                .insert(SUPERCHARGE_BEGUN.packed(), self.now.cast_signed());
            Some(Blessed::Script(SUPERCHARGE_BEGIN, palace))
        }
    }

    /// An Abode spell's timer: on the owner's Abode (one a player, its
    /// `BDEP` rule), player building level's, its start the clock
    /// and its length the spell's constant; none without an Abode.
    fn abode_timer(&mut self, program: &Program, owner: u32, spell: FourCc) {
        let lasts = ABODE_SPELLS
            .iter()
            .find(|(s, _)| *s == spell.0)
            .map(|(_, name)| match program.constant(name) {
                Some(Literal::Int(v)) => v,
                _ => 0,
            });
        let abode = self.best_building(owner, ABODE);
        let now = self.now.cast_signed();

        if let (Some(lasts), Some(piece)) = (lasts, abode.and_then(|a| self.pieces.get_mut(a))) {
            piece
                .attributes
                .insert(SORCERERS_ABODE_SPELL_START_TIME.packed(), now);
            piece.attributes.insert(ABODE_LASTS.packed(), lasts);
        }
    }

    /// The spell's unit of `body` at the cast's point, off the grid.
    fn stand_spell(
        &mut self,
        program: &Program,
        cast: &Cast,
        body: &crate::sim::world::bodies::Body,
    ) -> Result<u32, Fault> {
        let id = self.agents.next_id();
        let agent = crate::sim::world::Agent::made(program, &body.name)?;
        let fixed = (
            crate::sim::units::from_pos32(cast.at.0),
            crate::sim::units::from_pos32(cast.at.1),
        );

        self.agents.advance();
        self.handle += 1;
        self.agents.insert(id, agent);
        self.pieces.insert(
            id,
            Piece {
                owner: cast.owner,
                attributes: spell_attributes(id, &body.attributes),
                handle: self.handle,
                code: cast.code,
                name: body.name.clone(),
                place: Some(Place {
                    kind: kind::Kind::VEHICLE,
                    fixed,
                    standing: (space::half(fixed.0), space::half(fixed.1)),
                    holder: 0,
                    // `DontBlock`: off the grid, never nudged
                    //.
                    state: body.state,
                }),
                spells: Some(Vec::new()),
            },
        );
        self.units
            .set_motion(id, crate::sim::world::places::Motion::Still);
        self.agents.note_made(id);
        self.units.entry(id).spelled = Some(self.now);

        // Its first set's particle systems, each the next handle.
        self.first_vents(self.handle, id, cast.code);
        Ok(id)
    }
}

/// Whether `guard` refuses a spell on `piece`.
fn refuses(guard: Guard, piece: &Piece) -> bool {
    match guard {
        Guard::Free => false,
        Guard::Key(key) => piece.word(key) != 0,
        Guard::Hurt => {
            let whole =
                piece.word(crate::data::attrib::MAX_HP) <= piece.word(crate::data::attrib::HP);

            whole && piece.word(PLAGUE) == 0
        }
    }
}

/// A spell unit's attributes as made, from its descriptor's `made`: its
/// agent id, and the descriptor's `SightRange`, which
/// player spell apply writes before placing it, so
/// its reveal has a radius. The rest `VehicleRec` sets is not
/// followed.
fn spell_attributes(id: u32, made: &[(i32, i32)]) -> std::collections::BTreeMap<i32, i32> {
    let sight = crate::sim::world::reveal::SIGHT_RANGE.packed();

    made.iter()
        .filter(|(k, _)| *k == sight)
        .copied()
        .chain(std::iter::once((
            crate::sim::world::AGENT_ID.packed(),
            id.cast_signed(),
        )))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::MAX_HP;
    use crate::sim::world::offer::SPELLS;

    /// `farseeing`'s unit reveals its descriptor's `SightRange`:
    /// Player spell apply writes it before placing the unit.
    #[test]
    fn a_spell_unit_takes_its_descriptors_sight_range() {
        let sight = crate::sim::world::reveal::SIGHT_RANGE.packed();
        let made = spell_attributes(40, &[(sight, 288), (MAX_HP.packed(), 10)]);

        assert_eq!(made.get(&sight), Some(&288));
        assert_eq!(made.len(), 2);
    }

    fn piece(words: &[(Key, i32)]) -> Piece {
        Piece {
            owner: 0,
            attributes: words.iter().map(|(k, v)| (k.packed(), *v)).collect(),
            handle: 1,
            code: FourCc(*b"AVa1"),
            name: String::new(),
            place: None,
            spells: None,
        }
    }

    /// A healing takes a wounded or plagued target, a key spell one
    /// without it.
    #[test]
    fn a_healing_refuses_a_whole_unplagued_target() {
        let (hp, max) = (crate::data::attrib::HP, crate::data::attrib::MAX_HP);

        assert!(refuses(Guard::Hurt, &piece(&[(hp, 10), (max, 10)])));
        assert!(!refuses(Guard::Hurt, &piece(&[(hp, 9), (max, 10)])));
        assert!(!refuses(
            Guard::Hurt,
            &piece(&[(hp, 10), (max, 10), (PLAGUE, 1)])
        ));
        assert!(refuses(
            Guard::Key(HAS_EFFECT_INVISIBILITY),
            &piece(&[(HAS_EFFECT_INVISIBILITY, 1)])
        ));
        assert!(!refuses(Guard::Free, &piece(&[])));
    }

    /// Every spell of the table but `SuperCharge`, Illusionary Hero and the
    /// eight that make a unit dispatches a function.
    #[test]
    fn every_spell_is_a_unit_a_function_or_one_of_two() {
        let units = SPELL_UNITS.iter().map(|(_, s, _)| *s);
        let scripts = SPELL_SCRIPTS.iter().map(|(s, ..)| *s);
        let mut all: Vec<[u8; 4]> = units
            .chain(scripts)
            .chain([SUPERCHARGE, ILLUSION])
            .collect();

        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), SPELLS.len());
    }
}
