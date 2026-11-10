//! A player's treasury, `APP:0`: what the player's orders and the scripts
//! pay and earn. **Binary** unless marked.
//!
//! ```text
//!  affords(owner, price)  GE_Player's test: the treasury holds price
//!                         (interface)
//!  spend(owner, price)    GE_Player: taken if held
//!  earn(owner, amount)    GS_Player: added, made counted
//!  AdjustPlayerData       moved, kept under GOLD_CAP
//! ```
//!
//! A treasury the world does not know affords any price and is never
//! written.

use super::{Answer, Found, World, agent_ref, int_ref, string_ref};
use crate::gpl::vm::{Fault, Handle, Vm};

/// `AdjustPlayerData`'s one field.
pub(super) const GOLD: &str = "gold";

/// `AdjustPlayerData`'s optional type, the argument after the amount: past
/// four values.
const ADJUST_TYPED: usize = 4;

/// The treasury's cap: gold cap, 2,000,000 in the file.
pub const GOLD_CAP: i32 = 2_000_000;

impl World {
    /// `GE_Player` on `APP:0`: take `amount` from the owner's
    /// treasury if it holds that much. True where the treasury is not
    /// known.
    pub(super) fn spend(&mut self, owner: u32, amount: i32) -> bool {
        match self.players.gold_mut(owner) {
            Some(g) if amount <= *g => {
                // `GE_Player`'s plain `sub`: a refund wraps.
                *g = g.wrapping_sub(amount);
                true
            }
            Some(_) => false,
            None => true,
        }
    }

    /// The owner's treasury, `APP:0`, where known.
    #[must_use]
    pub fn treasury(&self, owner: u32) -> Option<i32> {
        self.players.gold(owner)
    }

    /// `AdjustPlayerData(agent, field, amount, [type])`, gpl adjust player data:
    /// throws with no unit or a field other than `gold`; else the owner's
    /// treasury moves by the amount, kept under [`GOLD_CAP`]. A spend with
    /// a type counts that descriptor as bought: a hero
    /// or a building; only `Embassy.gpl` passes one.
    pub(super) fn adjust(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;
        let field = string_ref(vm, args, 2)?;
        let amount = int_ref(vm, args, 3)?;
        let owner = match self.found(id) {
            Found::Piece(p) => Some(p.owner),
            Found::Unsure => None,
            Found::Gone => Err(Fault::throw("AdjustPlayerData() unit is null"))?,
        };

        // Counted before the field is checked.
        if let Some(o) = owner.filter(|_| amount < 0 && args.len() > ADJUST_TYPED) {
            let ty = string_ref(vm, args, ADJUST_TYPED)?;

            self.count_spend(vm.program(), o, (&ty, -amount));
        }

        if !field.eq_ignore_ascii_case(GOLD) {
            Err(Fault::throw(format!(
                "$AdjustPlayerData() could not set field \"{field}\""
            )))?;
        }

        match owner {
            Some(o) => {
                if let Some(g) = self.players.gold_mut(o) {
                    let old = *g;

                    *g = if *g > GOLD_CAP {
                        GOLD_CAP
                    } else {
                        (*g + amount).min(GOLD_CAP)
                    };

                    // An add goes through `GS_Player`: made, and the
                    // peak.
                    let (added, now) = (*g - old, *g);

                    if amount > 0 && added > 0 {
                        let l = self.ledger_mut(o);

                        l.made += added;
                        l.peak = l.peak.max(now);
                    }
                }

                Ok(Answer::Given)
            }
            None => Ok(Answer::Unknown),
        }
    }

    /// `GS_Player` on `APP:0`: `amount` added where the treasury is
    /// known, counted as made above 0, the treasury the
    /// peak where above it.
    pub(super) fn earn(&mut self, owner: u32, amount: i32) {
        let now = self.players.gold_mut(owner).map(|g| {
            *g += amount;
            *g
        });
        let l = self.ledger_mut(owner);

        if amount > 0 {
            l.made += amount;
        }

        if let Some(now) = now {
            l.peak = l.peak.max(now);
        }
    }
}
