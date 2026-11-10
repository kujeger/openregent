//! Which unit a spell may be cast on: spell target check,
//! the cursor's test, and the wizards' guilds' reach.
//! **Binary** unless marked.
//!
//! ```text
//!  every spell   not NotSpellTarget (APU:10), not held; a spell code
//!  Sp11          in reach, then as a character spell
//!  Sp12, Sp13    in reach, seen, a building or a character, not AVk,
//!                not invisible (APV:37), standing; Sp13 not shielded
//!  Sp32          seen, a character, a hero, not AVk, not invisible,
//!                standing
//!  Sp18, Sp27    seen, a character, lying dead (GenUnit       3)
//!  the rest      a character spell: seen, a character, not AVk, not
//!                invisible, standing, the spell's key unset; a healing
//!                wants it hurt or plagued (cast::Guard)
//!  reach         one of the owner's wizards' guilds or towers, whole
//!                hit points above 0 and finished: the gap from its pad
//!                to the target, 32nds, within #SpellCastRange (doubled
//!                while SuperCharged) plus #LevelSpellRangeBonus a
//!                guild stage past 1; the palace's APB:10 reaches all
//!  a point       the spot check: Sp10 unexplored and in reach; Sp14
//!                explored and in reach; Sp29, Sp31, Sp34, Sp36 explored
//!  a refused     the click: Sp10 to Sp14 with the point out of
//!  click         reach say GMTX 40; nothing sent, no sound
//!  a short       the click, the spot taken: the treasury under
//!  click         the price says GMTX 34; nothing sent, no sound, the
//!                spell still aimed. The button and its key ask no gold
//!  the portrait  StatsWindowController      8011 while aiming: the
//!                panel's unit tested as a click on it, cast at its
//!                place; refused, the reach tested there
//! ```
//!
//! The decode tests no reach: the interface
//! refuses.
//!
//! A character is character usable's: **Inferred**, a character
//! whose code's `CanUse` default (its first letter) is a human player's
//! or a monster's; its descriptor kind test (not 1, 4 or 5) is not read.

use super::cast::{Bless, Guard, SPELL_SCRIPTS, SUPERCHARGE_ACTIVE};
use crate::data::attrib::{
    CHEAT_WIZ_GUILD_RANGE, HAS_EFFECT_ANTI_MAGIC, HAS_EFFECT_INVISIBILITY, Key,
};
use crate::data::desc::User;
use crate::data::fourcc::FourCc;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::sim::space;
use crate::sim::world::chat::{Line as ChatLine, NO_GOLD_SPELL, OUT_OF_REACH};
use crate::sim::world::offer::WIZARDS_TOWER;
use crate::sim::world::piece_state;
use crate::sim::world::{Piece, World, bodies::Class};

/// `NotSpellTarget`, `APU:10` (`0xa555041`, spell target check).
pub const NOT_SPELL_TARGET: Key = crate::data::attrib::NOT_SPELL_TARGET;

/// The wizards' guild's code prefix: it and the tower lend reach.
pub const WIZARDS_GUILD: [u8; 3] = *b"ABY";

/// The reach of a wizards' guild, 32nds.
pub const CAST_RANGE: &str = "#SpellCastRange";

/// The reach each guild stage past 1 adds.
pub const RANGE_BONUS: &str = "#LevelSpellRangeBonus";

/// The spells a refused click tests the reach of:
/// farseeing, invisibility, lightning bolt, anti-magic shield, lightning
/// storm. The minimap draws reach circles while one is aimed
/// ([`World::reach_rings`]).
pub const REACH_SPELLS: [[u8; 4]; 5] = [*b"Sp10", *b"Sp11", *b"Sp12", *b"Sp13", *b"Sp14"];

/// The resource piles no spell takes.
pub(in crate::sim::world) const RESOURCES: [[u8; 4]; 3] = [*b"AVk1", *b"AVk2", *b"AVk3"];

/// How a spell's target is tested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Test {
    /// Invisibility: in reach, then as [`Test::Character`].
    Reached,
    /// Lightning bolt and anti-magic shield: in reach, a building or a
    /// character.
    Struck,
    /// Change of heart: a hero.
    Hero,
    /// Resurrection, reanimate: lying dead.
    Grave,
    /// Any other spell cast on a unit.
    Character,
    /// A spell no unit takes.
    None,
}

/// Spell `spell`'s test.
const fn test(spell: [u8; 4]) -> Test {
    match &spell {
        b"Sp11" => Test::Reached,
        b"Sp12" | b"Sp13" => Test::Struck,
        b"Sp32" => Test::Hero,
        b"Sp18" | b"Sp27" => Test::Grave,
        b"Sp16" | b"Sp17" | b"Sp19" | b"Sp20" | b"Sp21" | b"Sp22" | b"Sp23" | b"Sp24" | b"Sp25"
        | b"Sp26" | b"Sp28" | b"Sp30" | b"Sp33" | b"Sp35" | b"Sp37" => Test::Character,
        _ => Test::None,
    }
}

/// 32nds of a cell.
const CELL: i32 = 32;

/// How a point spell's spot is tested: spell spot check's branches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    /// Farseeing: ground not explored, in reach.
    Hidden,
    /// Lightning storm: in reach, explored.
    Reached,
    /// Wind storm, sun scorch, earthquake, gate: explored.
    Explored,
    /// A unit spell: the cursor tests the unit under it.
    None,
}

/// Point spell `spell`'s test.
const fn point(spell: [u8; 4]) -> Point {
    match &spell {
        b"Sp10" => Point::Hidden,
        b"Sp14" => Point::Reached,
        b"Sp29" | b"Sp31" | b"Sp34" | b"Sp36" => Point::Explored,
        _ => Point::None,
    }
}

/// The key spell target check's table holds for a character spell: the spell
/// already on the target. Illusionary Hero's is the clone's mark.
const fn key_of(spell: [u8; 4]) -> Option<Key> {
    match &spell {
        b"Sp24" => Some(crate::data::attrib::HAS_EFFECT_ILLUSIONARY_HERO),
        _ => None,
    }
}

impl World {
    /// Whether `owner` may cast spell `spell` on agent `target`, as the
    /// cursor tests it.
    #[must_use]
    pub fn spell_target(&self, program: &Program, owner: u32, spell: FourCc, target: u32) -> bool {
        let piece = self.unit(target);
        let place = self.known(target);
        let open = piece.is_some_and(|p| p.word(NOT_SPELL_TARGET) == 0)
            && place.is_some_and(|p| p.holder == 0);

        match (piece, open) {
            (Some(p), true) => self.takes(program, owner, spell, target, p),
            _ => false,
        }
    }

    /// Whether `owner` may cast point spell `spell` at `at`, 32nds, as the
    /// cursor tests it: Farseeing on unexplored ground in
    /// reach, Lightning Storm on explored ground in reach, the other four
    /// on explored ground. A unit spell takes no point.
    #[must_use]
    pub fn spell_point(
        &self,
        program: &Program,
        owner: u32,
        spell: FourCc,
        at: (i32, i32),
    ) -> bool {
        let cell = (at.0.div_euclid(CELL), at.1.div_euclid(CELL));
        let explored = self.fog.explored().cell_seen(cell, owner);
        let reached = || self.reaches(program, owner, Some(at));

        match point(spell.0) {
            Point::Hidden => !explored && reached(),
            Point::Reached => explored && reached(),
            Point::Explored => explored,
            Point::None => false,
        }
    }

    /// What a refused click of spell `spell` at `at`, 32nds, says:
    /// [`OUT_OF_REACH`] for a reach spell, Sp10 to Sp14,
    /// with `at` beyond every wizards' guild's and tower's reach; else
    /// nothing. No sound.
    #[must_use]
    pub fn spell_refusal(
        &self,
        program: &Program,
        owner: u32,
        spell: FourCc,
        at: (i32, i32),
    ) -> Option<ChatLine> {
        let reach_spell = REACH_SPELLS.contains(&spell.0);

        (reach_spell && !self.reaches(program, owner, Some(at)))
            .then_some(ChatLine::Text(OUT_OF_REACH))
    }

    /// What a taken spell click at `price` says (spell click,
    /// spell click): [`NO_GOLD_SPELL`] where `owner`'s treasury is under
    /// it; else nothing. No sound.
    #[must_use]
    pub fn spell_short(&self, owner: u32, price: i32) -> Option<ChatLine> {
        (!self.affords(owner, price)).then_some(ChatLine::Text(NO_GOLD_SPELL))
    }

    /// A press on the selection panel's portrait while `owner` aims spell
    /// `spell` at `price` (`StatsWindowController` 8011,
    /// spell click event 1): a [`Bless`] on agent `target` at its
    /// place where [`World::spell_target`] takes it and the treasury pays
    /// ([`World::spell_short`]); else what [`World::spell_refusal`] says
    /// at its place. A point spell takes no unit, so is refused.
    ///
    /// # Errors
    ///
    /// The refusal's chat line, if any.
    pub fn spell_on(
        &self,
        program: &Program,
        owner: u32,
        (spell, price): (FourCc, i32),
        target: u32,
    ) -> Result<Bless, Option<ChatLine>> {
        let at = self.position(target);
        let taken = self.spell_target(program, owner, spell, target);

        match (at, taken) {
            (Some(at), true) => self.spell_short(owner, price).map_or(
                Ok(Bless {
                    spell,
                    target,
                    owner,
                    at,
                }),
                |short| Err(Some(short)),
            ),
            (Some(at), false) => Err(self.spell_refusal(program, owner, spell, at)),
            (None, _) => Err(None),
        }
    }

    /// [`World::spell_target`] past its first tests.
    fn takes(&self, program: &Program, owner: u32, spell: FourCc, id: u32, p: &Piece) -> bool {
        let seen = self.word(id, p, owner).unwrap_or(false);
        let standing = self
            .known(id)
            .is_some_and(|at| at.state & piece_state::RUINED == 0);
        let character = self.usable(p);
        let plain =
            !RESOURCES.contains(&p.code.0) && p.word(HAS_EFFECT_INVISIBILITY) == 0 && standing;
        let reached = || self.in_reach(program, owner, id);

        match test(spell.0) {
            Test::Reached => reached() && seen && character && plain,
            Test::Struck => {
                let shielded = spell.0 == *b"Sp13" && p.word(HAS_EFFECT_ANTI_MAGIC) != 0;

                reached() && seen && (self.building(p) || character) && plain && !shielded
            }
            Test::Hero => seen && character && self.is_hero(id) && plain,
            Test::Grave => seen && character && !standing,
            Test::Character => seen && character && plain && !spelled_on(spell, p),
            Test::None => false,
        }
    }

    /// Whether agent `target` is of the kind spell `spell` is cast on:
    /// [`World::spell_target`]'s class tests alone, a character, or for
    /// lightning bolt and anti-magic shield a building too. A spell no
    /// unit takes asks nothing. Not the engine's: [`crate::sim::game::check`]
    /// holds a [`Bless`] to it, as its decode tests no class
    /// and a script errs on a building given
    /// a character's spell (`Invisibility_End` reads `original_type`).
    #[must_use]
    pub fn spell_kind(&self, spell: FourCc, target: u32) -> bool {
        let piece = self.unit(target);
        let character = piece.is_some_and(|p| self.usable(p));

        match test(spell.0) {
            Test::Struck => character || piece.is_some_and(|p| self.building(p)),
            Test::None => true,
            Test::Reached | Test::Hero | Test::Grave | Test::Character => character,
        }
    }

    /// Whether `p`'s descriptor is a building's.
    fn building(&self, p: &Piece) -> bool {
        self.descriptors.class(p.code) == Some(Class::Building)
    }

    /// Character usable: a character a human player or a monster
    /// may use (**Inferred**, above).
    pub(in crate::sim::world) fn usable(&self, p: &Piece) -> bool {
        let character = self.descriptors.class(p.code) == Some(Class::Character);

        character && User::default_for(p.code).is_some()
    }

    /// Unit class of 1: the agent's `subtype` is `hero`.
    pub fn is_hero(&self, id: u32) -> bool {
        self.agents.get(id).is_some_and(|a| {
            a.values.iter().any(|m| {
                m.name
                    .eq_ignore_ascii_case(crate::sim::world::search::SUBTYPE)
                    && matches!(&m.value, Value::String(s) if s.eq_ignore_ascii_case("hero"))
            })
        })
    }

    /// Spell reach: whether agent `id` stands of one of
    /// `owner`'s wizards' guilds or towers.
    fn in_reach(&self, program: &Program, owner: u32, id: u32) -> bool {
        self.reaches(program, owner, self.position(id))
    }

    /// Spell reach: whether `at`, 32nds, lies in reach of one of
    /// `owner`'s wizards' guilds or towers.
    fn reaches(&self, program: &Program, owner: u32, at: Option<(i32, i32)>) -> bool {
        let reaches = |(gid, code, reach): (u32, FourCc, i32)| {
            let rect = self.descriptors.shape(code).map(|s| s.rect);
            let gap = rect
                .zip(self.position(gid))
                .zip(at)
                .map(|((r, from), to)| space::gap2(space::placed(r, from), space::point(to)));

            gap.is_some_and(|d| {
                crate::gpl::vm::native::fixed_sqrt(d.cast_unsigned(), 0).cast_signed() <= reach
            })
        };

        self.lenders(program, owner)
            .is_none_or(|mut lenders| lenders.any(reaches))
    }

    /// The reach circles the minimap draws while a reach spell is aimed:
    /// each of `owner`'s wizards' guilds and towers
    /// lending reach, its place and reach in 32nds. `None` where the reach
    /// is unlimited; **Inferred**: no circle then.
    #[must_use]
    pub fn reach_rings(&self, program: &Program, owner: u32) -> Option<Vec<((i32, i32), i32)>> {
        let lenders = self.lenders(program, owner)?;

        Some(
            lenders
                .filter_map(|(id, _, reach)| Some((self.position(id)?, reach)))
                .collect(),
        )
    }

    /// The reach circles shown while `owner` aims spell `spell`:
    /// [`World::reach_rings`] for a reach spell ([`REACH_SPELLS`]), sorted,
    /// as the pieces' order is a hash map's; none for another spell, or
    /// where the reach is unlimited.
    #[must_use]
    pub fn aimed_reach(
        &self,
        program: &Program,
        owner: u32,
        spell: FourCc,
    ) -> Vec<((i32, i32), i32)> {
        let mut rings = if REACH_SPELLS.contains(&spell.0) {
            self.reach_rings(program, owner).unwrap_or_default()
        } else {
            Vec::new()
        };

        rings.sort_unstable();
        rings
    }

    /// The reach circles shown while `owner` places building `code`
    /// with its preview at `at`, 32nds: for a wizards' guild or tower,
    /// [`World::reach_rings`] and the preview's own, sorted; none for
    /// another building, or where the reach is unlimited. Not the
    /// original's: it shows no circles while placing
    /// (minimap reach set is called by the reach spells' cursor
    /// rows alone).
    #[must_use]
    pub fn placing_reach(
        &self,
        program: &Program,
        owner: u32,
        code: FourCc,
        at: Option<(i32, i32)>,
    ) -> Vec<((i32, i32), i32)> {
        let rings = lends_reach(code.0)
            .then(|| self.reach_terms(program, owner))
            .flatten()
            .and_then(|terms| {
                let mut rings = self.reach_rings(program, owner)?;

                rings.extend(at.map(|at| (at, reach_of(code.0, terms))));
                Some(rings)
            });
        let mut rings = rings.unwrap_or_default();

        rings.sort_unstable();
        rings
    }

    /// `owner`'s reach terms, 32nds: `#SpellCastRange` (doubled while
    /// `SuperCharged`) and `#LevelSpellRangeBonus`. `None` where the
    /// reach is unlimited.
    fn reach_terms(&self, program: &Program, owner: u32) -> Option<(i32, i32)> {
        let constant = |name: &str| match program.constant(name) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };
        let palace = self.pieces.get(self.palace_of(owner));
        // every level flag too.
        let unlimited =
            self.every_level(owner) || palace.is_some_and(|p| p.word(CHEAT_WIZ_GUILD_RANGE) != 0);
        let charged = palace.is_some_and(|p| p.word(SUPERCHARGE_ACTIVE) != 0);
        let base = constant(CAST_RANGE) * if charged { 2 } else { 1 };

        (!unlimited).then_some((base, constant(RANGE_BONUS)))
    }

    /// `owner`'s wizards' guilds and towers lending reach, each with its
    /// code and reach in 32nds: whole hit points above 0, finished, `#SpellCastRange`
    /// (doubled while `SuperCharged`) plus `#LevelSpellRangeBonus` a guild
    /// stage past 1. `None` where the reach is unlimited.
    fn lenders(
        &self,
        program: &Program,
        owner: u32,
    ) -> Option<impl Iterator<Item = (u32, FourCc, i32)>> {
        let terms = self.reach_terms(program, owner)?;

        let lends = move |(gid, g): (&u32, &Piece)| {
            let lends = g.owner == owner
                && lends_reach(g.code.0)
                && g.word(crate::data::attrib::HP) > 0
                && g.word(crate::data::attrib::FIRST_STAGE_BUILT) != 0;

            lends.then_some((*gid, g.code, reach_of(g.code.0, terms)))
        };

        Some(self.pieces.iter().filter_map(lends))
    }

    /// Player building level's building: `owner`'s standing
    /// finished building of code prefix `prefix` at the best stage
    /// ([`World::building_level`]), the first in agent order.
    pub(in crate::sim::world) fn best_building(&self, owner: u32, prefix: [u8; 3]) -> Option<u32> {
        let best = self.stage_of(owner, prefix);

        self.pieces
            .iter()
            .filter(|(_, p)| p.owner == owner && p.code.0.starts_with(&prefix))
            .filter(|(_, p)| p.word(crate::data::attrib::FIRST_STAGE_BUILT) != 0)
            .filter(|(id, _)| {
                self.known(**id)
                    .is_some_and(|at| at.state & piece_state::RUINED == 0)
            })
            .filter(|(_, p)| {
                let level = i32::from(p.code.0[3]) - i32::from(b'0');
                let building = p.word(crate::data::attrib::CURRENT_STAGE_BUILT) == 0;

                best > 0 && level - i32::from(building) == best
            })
            .map(|(id, _)| *id)
            .min()
    }

    /// Whether the Abode's timer lets `owner` cast one of its spells:
    /// unset, or run out (**Inferred**: the fast bar sends an
    /// `AlterAttribute` clearing both words once the clock passes the
    /// length, fast bar abode timer; the world clears none).
    #[must_use]
    pub fn abode_ready(&self, owner: u32) -> bool {
        let abode = self
            .best_building(owner, super::cast::ABODE)
            .and_then(|a| self.pieces.get(a));
        let begun = abode.map_or(0, |a| {
            a.word(crate::data::attrib::SORCERERS_ABODE_SPELL_START_TIME)
        });
        let lasts = abode.map_or(0, |a| a.word(super::cast::ABODE_LASTS));

        begun == 0 || self.now.cast_signed().wrapping_sub(begun) > lasts
    }

    /// Abode `id`'s rest, as its own panel shows it:
    /// ms since `ASS:0`, of `ASP:0`, while
    /// the length has not run out; its Spells button hidden meanwhile.
    #[must_use]
    pub fn resting(&self, id: u32) -> Option<(i32, i32)> {
        let abode = self.pieces.get(id)?;
        let begun = abode.word(crate::data::attrib::SORCERERS_ABODE_SPELL_START_TIME);
        let lasts = abode.word(super::cast::ABODE_LASTS);
        let elapsed = self.now.cast_signed().wrapping_sub(begun);

        (begun != 0 && elapsed <= lasts).then_some((elapsed, lasts))
    }

    /// `owner`'s rest as the spell bar's 9500 shows it: its Abode's
    /// [`World::resting`]. **Binary**: one Abode a player (`BDEP`), as
    /// `FastBar` reads it.
    #[must_use]
    pub fn abode_rest(&self, owner: u32) -> Option<(i32, i32)> {
        self.best_building(owner, super::cast::ABODE)
            .and_then(|a| self.resting(a))
    }
}

/// Whether a building of `code` lends spell reach: a wizards' guild or
/// tower.
fn lends_reach(code: [u8; 4]) -> bool {
    code.starts_with(&WIZARDS_GUILD) || code.starts_with(&WIZARDS_TOWER)
}

/// The reach a building of `code` lends, 32nds, from
/// [`World::reach_terms`]: the base, plus the bonus a guild stage past 1.
fn reach_of(code: [u8; 4], (base, bonus): (i32, i32)) -> i32 {
    let stages = match (code.starts_with(&WIZARDS_GUILD), code[3]) {
        (true, b'2') => 1,
        (true, b'3') => 2,
        _ => 0,
    };

    base + bonus * stages
}

/// Whether spell `spell` is on `p` already, or a healing finds it whole.
fn spelled_on(spell: FourCc, p: &Piece) -> bool {
    let guard = SPELL_SCRIPTS
        .iter()
        .find(|(s, ..)| *s == spell.0)
        .map_or(Guard::Free, |(.., g)| *g);
    let own = key_of(spell.0).is_some_and(|k| p.word(k) != 0);

    own || match guard {
        Guard::Free => false,
        Guard::Key(key) => p.word(key) != 0,
        Guard::Hurt => {
            let whole = p.word(crate::data::attrib::MAX_HP) <= p.word(crate::data::attrib::HP);

            whole && p.word(super::cast::PLAGUE) == 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::kind;

    #[test]
    fn a_grave_spell_and_a_point_spell_test_apart() {
        assert_eq!(test(*b"Sp18"), Test::Grave);
        assert_eq!(test(*b"Sp27"), Test::Grave);
        assert_eq!(test(*b"Sp10"), Test::None);
        assert_eq!(test(*b"Sp30"), Test::Character);
        assert_eq!(test(*b"Sp12"), Test::Struck);
    }

    /// The point tests are `SPELL_UNITS` less the two a unit takes
    /// (Lightning Bolt, Fire Strike).
    #[test]
    fn the_point_tests_are_the_spells_cast_at_a_point() {
        let points: Vec<[u8; 4]> = crate::sim::world::spell::cast::SPELL_UNITS
            .iter()
            .map(|(_, s, _)| *s)
            .filter(|s| point(*s) != Point::None)
            .collect();

        assert_eq!(
            points,
            [*b"Sp10", *b"Sp14", *b"Sp29", *b"Sp31", *b"Sp34", *b"Sp36"]
        );
    }

    /// An Abode rests from its spell's cast till its length runs out:
    /// at the length still, past it not.
    #[test]
    fn an_abode_rests_for_its_spells_length() {
        use super::super::cast::ABODE_LASTS;
        use crate::data::attrib::SORCERERS_ABODE_SPELL_START_TIME;

        const ABODE: u32 = 10;

        let mut world = World::default();

        world.pieces.insert(ABODE, Piece::default());

        assert_eq!(world.resting(ABODE), None);

        world.write(ABODE, SORCERERS_ABODE_SPELL_START_TIME.packed(), 110_600);
        world.write(ABODE, ABODE_LASTS.packed(), 80_000);
        world.now = 150_600;

        assert_eq!(world.resting(ABODE), Some((40_000, 80_000)));

        world.now = 190_600;

        assert_eq!(world.resting(ABODE), Some((80_000, 80_000)));

        world.now = 190_650;

        assert_eq!(world.resting(ABODE), None);
    }

    /// Of two Abodes at the best stage, the lower agent's, whatever
    /// order the pieces' hash map hands them in: each round a fresh
    /// world, a fresh hash.
    #[test]
    fn the_best_building_is_the_first_in_agent_order() {
        use crate::data::attrib::{CURRENT_STAGE_BUILT, FIRST_STAGE_BUILT};
        use crate::sim::world::ruin::tests::standing;

        const ROUNDS: usize = 16;
        const ABODES: [u32; 4] = [9, 3, 12, 6];

        for _ in 0..ROUNDS {
            let mut world = World::default();

            for (id, x) in ABODES.into_iter().zip(0..) {
                let mut piece = standing(*b"ABq1", id, kind::Kind::BUILDING, (x, 0));

                for stage in [FIRST_STAGE_BUILT, CURRENT_STAGE_BUILT] {
                    piece.attributes.insert(stage.packed(), 1);
                }

                // A standing building has an agent.
                piece
                    .attributes
                    .insert(crate::sim::world::AGENT_ID.packed(), id.cast_signed());
                world.pieces.insert(id, piece);
            }

            assert_eq!(world.best_building(0, super::super::cast::ABODE), Some(3));
        }
    }

    /// The spell bar's rest is the Abode's: none without one or while it
    /// is not resting, else its own.
    #[test]
    fn the_spell_bar_rests_with_the_abode() {
        use super::super::cast::ABODE_LASTS;
        use crate::data::attrib::{
            CURRENT_STAGE_BUILT, FIRST_STAGE_BUILT, SORCERERS_ABODE_SPELL_START_TIME,
        };
        use crate::sim::world::ruin::tests::standing;

        const ABODE: u32 = 7;

        let mut world = World::default();

        assert_eq!(world.abode_rest(0), None);

        let mut piece = standing(*b"ABq2", ABODE, kind::Kind::BUILDING, (0, 0));

        for stage in [FIRST_STAGE_BUILT, CURRENT_STAGE_BUILT] {
            piece.attributes.insert(stage.packed(), 1);
        }

        piece
            .attributes
            .insert(crate::sim::world::AGENT_ID.packed(), ABODE.cast_signed());
        world.pieces.insert(ABODE, piece);
        world.now = 150_600;

        assert_eq!(world.abode_rest(0), None);

        world.write(ABODE, SORCERERS_ABODE_SPELL_START_TIME.packed(), 130_600);
        world.write(ABODE, ABODE_LASTS.packed(), 60_000);

        assert_eq!(world.abode_rest(0), Some((20_000, 60_000)));
    }
}
