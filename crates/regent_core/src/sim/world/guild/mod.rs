//! A guild's action the player orders: `GUILD_ACTION_MESSAGE`. **Binary**
//! unless marked.
//!
//! ```text
//!  frame f  GSMessageManager case 0x3f2, guild action decode:
//!    guild action apply by action (message 0x08): 0 move a camp, 5 dismiss a
//!    unit, 6–12 building flags, 20 a champion, ...
//!    the enchantment: GS_OrderProcessor, unless busy (0x4006)
//!      the payment: nothing once APB:4 is over 0; else
//!        #WizTowerUpgradeCost off APP:0 (GE_Player), APB:4 + 1
//!      APB:45 the clock; DoWizTowerEnchant(agent) dispatched at once;
//!      a timed order, callback 0x200a, EndWizTowerEnchant
//!  EnchantWizTower(tower): GS_OrderProcessor (tower, 1), unpaid;
//!    busy: the timer's counter 0, a whole duration more from its end
//! ```
//!
//! The world pays and writes at the dispatch.
//! The end is the world's: the timed order's callback dispatches
//! `EndWizTowerEnchant` in the tower's ORDR turn ([`super::timed`];
//! **Game**: play5's, enchanted in frame 2932, ends in 4732, 90,000 ms
//! on).
//! **Game**: play5's one guild action runs `DoWizTowerEnchant` in its
//! decode's frame (**Measured**); with its 400 the treasury at the
//! save is the save's.
//!
//! The other buildings' actions, a module each.

pub mod assembly;
pub mod bounty;
pub mod camp;
pub mod embassy;
pub mod extortion;
pub mod gamble;
pub mod market;
pub mod mausoleum;
pub mod rage;
pub mod taxman;
pub mod upkeep;

use super::chat::{Line, NO_GOLD_ENCHANT};
use super::timed::Timer;
use super::{Answer, Found, World, agent_ref};
use crate::data::attrib::{self, ENCHANTMENT_LEVEL};
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::gpl::ops::Value;
use crate::gpl::vm::{Fault, Handle, Vm};

/// The one code `EnchantWizTower` takes.
pub const ENCHANT_TOWER: [u8; 4] = *b"ABM1";

/// What `GS_OrderProcessor` dispatches: the string `"DoWizTowerEnchant"`.
pub const ENCHANT: &str = "DoWizTowerEnchant";

/// The price's GPL constant, read by wiz tower constants.
pub const ENCHANT_COST: &str = "#WizTowerUpgradeCost";

/// The timed order's length, read by wiz tower constants.
pub const ENCHANT_DURATION: &str = "#Wiz_Tower_Enchantment_Duration";

/// What the timed order's callback `0x200a` dispatches.
pub const ENCHANT_END: &str = "EndWizTowerEnchant";

impl World {
    /// wiz tower enchant pay and the write before the dispatch, on agent `id`.
    pub(super) fn enchant(&mut self, program: &Program, id: u32) {
        let cost = match program.constant(ENCHANT_COST) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };
        let piece = self.pieces.get(id).map(|p| {
            let level = p.get(ENCHANTMENT_LEVEL);

            (p.owner, level.unwrap_or_default())
        });

        let duration = match program.constant(ENCHANT_DURATION) {
            Some(Literal::Int(v)) => v.cast_unsigned(),
            _ => 0,
        };

        let unpaid = piece.filter(|(_, l)| *l <= 0);
        let paid = unpaid.map(|(owner, _)| self.spend(owner, cost));

        // A short treasury: a chat line.
        if let (Some((owner, _)), Some(false)) = (unpaid, paid) {
            self.say(owner, Line::Text(NO_GOLD_ENCHANT));
        }

        if let (Some((owner, level)), Some(true)) = (unpaid, paid) {
            let now = self.now.cast_signed();

            self.count_paid(owner, super::ledger::Spend::Buildings, cost);

            self.write(id, ENCHANTMENT_LEVEL.packed(), level + 1);
            self.write(id, attrib::ENCHANT_DURATION.packed(), now);
            self.time(id, Timer::Enchant, duration);
        }
    }

    /// `EnchantWizTower(agent)`, gpl enchant wiz tower: throws with no unit or
    /// for one not a [`ENCHANT_TOWER`]; else `GS_OrderProcessor`
    /// with the free flag: an enchantment running runs a whole duration
    /// more from its end ([`World::reset_enchant`]); else, `APB:4` 0, the
    /// enchantment, unpaid. Returns nothing.
    pub(super) fn enchant_tower(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(p) if p.code.0 != ENCHANT_TOWER => Err(Fault::throw(
                "EnchantWizTower() unit is not a Wizard Tower".to_owned(),
            )),
            Found::Piece(_) => {
                let duration = match vm.program().constant(ENCHANT_DURATION) {
                    Some(Literal::Int(v)) => v.cast_unsigned(),
                    _ => 0,
                };

                if !self.reset_enchant(id) {
                    self.free_enchant(id, duration);
                }

                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw("EnchantWizTower() unit is null".to_owned())),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }

    /// Wiz tower enchant pay with the free flag, then the rest of
    /// `GS_OrderProcessor`: nothing once `APB:4` is over 0; else
    /// `APB:4` plus 1, `APB:45` the clock, [`ENCHANT`] dispatched, the
    /// timer of `duration` ms pushed.
    fn free_enchant(&mut self, id: u32, duration: u32) {
        let level = self
            .pieces
            .attribute(id, ENCHANTMENT_LEVEL)
            .unwrap_or_default();

        if level <= 0 {
            let now = self.now.cast_signed();

            self.write(id, ENCHANTMENT_LEVEL.packed(), level + 1);
            self.write(id, attrib::ENCHANT_DURATION.packed(), now);
            self.dispatch(ENCHANT, vec![Value::Agent(id)]);
            self.time(id, Timer::Enchant, duration);
        }
    }

    /// Callback `0x200a` on tower `id`, ended normally, in
    /// its ORDR turn: `APB:45` 0, then [`ENCHANT_END`] dispatched.
    pub(super) fn enchant_over(&mut self, id: u32) {
        self.write(id, attrib::ENCHANT_DURATION.packed(), 0);
        self.dispatch(ENCHANT_END, vec![Value::Agent(id)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::attrib::Key;

    const TOWER: u32 = 21;
    const LASTS: u32 = 90_000;

    fn names(world: &mut World) -> Vec<String> {
        world.take_queued().into_iter().map(|(n, _)| n).collect()
    }

    /// `EnchantWizTower`'s enchantment is the player's, unpaid: level 1,
    /// its start, `DoWizTowerEnchant` at once; and its end a duration on.
    #[test]
    fn a_free_enchantment_runs_its_duration() {
        let mut world = World::default();

        world
            .pieces
            .insert(TOWER, crate::sim::world::Piece::default());
        world.now = 1000;
        world.free_enchant(TOWER, LASTS);

        let attribute = |w: &World, k: Key| w.pieces[&TOWER].get(k);

        assert_eq!(attribute(&world, ENCHANTMENT_LEVEL), Some(1));
        assert_eq!(attribute(&world, attrib::ENCHANT_DURATION), Some(1000));
        assert_eq!(names(&mut world), [ENCHANT]);

        // A second, enchanted already: nothing (`APB:4` over 0).
        world.free_enchant(TOWER, LASTS);
        assert_eq!(names(&mut world).len(), 0);

        world.now = 1000 + LASTS;
        world.run_timers(TOWER);
        assert_eq!(names(&mut world), [ENCHANT_END]);
    }

    /// On a tower enchanted, the counter goes to 0: its
    /// due run re-arms the time, so it ends a whole duration later.
    #[test]
    fn enchanting_an_enchanted_tower_runs_a_duration_more() {
        let mut world = World::default();

        world
            .pieces
            .insert(TOWER, crate::sim::world::Piece::default());
        world.free_enchant(TOWER, LASTS);
        names(&mut world);

        world.now = 30_000;
        assert!(world.reset_enchant(TOWER));

        world.now = LASTS;
        world.run_timers(TOWER);
        assert_eq!(names(&mut world).len(), 0);

        world.now = 2 * LASTS - 50;
        world.run_timers(TOWER);
        assert_eq!(names(&mut world).len(), 0);

        world.now = 2 * LASTS;
        world.run_timers(TOWER);
        assert_eq!(names(&mut world), [ENCHANT_END]);
    }
}
