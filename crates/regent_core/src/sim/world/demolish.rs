//! The player's demolition. **Binary** unless marked.
//!
//! ```text
//!  KillConfirmController,      panel AP62, YES on a building
//!    sends DESTROY_BUILDING_MESSAGE, the building's handle
//!  GSMessageManager case 0x3ea, before the clock step
//!    decodes it: the agent by handle, then the refund:
//!      p = building_price(owner, code, _, 1, 0)       the price past one
//!      refund = fixed_mul(GameValue2, p << 16) >> 16, an eighth
//!      refund not 0: the owner's GS_Player adds it to APP:0
//!      GSUnit writes (APU:0, 0): the death script, nested
//! ```

use super::{Dispatch, World};
use crate::base::fixed::{from_whole, mul, whole};
use crate::data::attrib::HP;

/// The buildings building refund's price counts free: the one
/// demolished.
const FREE: usize = 1;

/// A refund: `p` times `GameValue2`, 16.16, the high word.
/// 600 gives 75.
#[must_use]
pub const fn refund(p: i32) -> i32 {
    whole(mul(super::price::GAME_VALUE_2, from_whole(p)))
}

impl World {
    /// `DESTROY_BUILDING_MESSAGE` on building `id`: the owner refunded an
    /// eighth of its price, then its hit points 0, so its death script
    /// runs. The dispatches to run, in order; none without the piece.
    pub fn demolish(&mut self, id: u32) -> Vec<Dispatch> {
        if let Some(piece) = self.pieces.get(id) {
            let (owner, code) = (piece.owner, piece.code);
            let back = refund(self.price_past(owner, code, FREE));

            if back != 0 {
                self.earn(owner, back);
            }

            self.write(id, HP.packed(), 0);
        }

        self.take_queued()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refund_is_an_eighth_of_the_price() {
        assert_eq!(refund(600), 75);
        assert_eq!(refund(439), 54);
        assert_eq!(refund(7), 0);
    }
}
