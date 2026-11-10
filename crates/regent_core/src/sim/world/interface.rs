//! What the interface asks of the world before it sends: whether an
//! owner affords a price.
//!
//! ```text
//!  treasury known     affords: treasury >= price
//!  treasury unknown   affords anything (UNKNOWN_AFFORDS)
//! ```

use super::{
    World,
    flag::{REWARD_DELTA, Rewards, reward_steps},
};

/// What an owner with no treasury affords: anything. Only a world without
/// its players' state has none (`World::default`, a bare test world), so
/// its panels stay pressable.
pub const UNKNOWN_AFFORDS: bool = true;

/// How far one press of a panel's up or down button goes. A departure:
/// the original has only [`Stride::One`]. A press that goes far sends the
/// single-step order several times, so the orders stay the engine's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stride {
    /// One step.
    One,
    /// As many steps as the button's multiplier allows: Shift held.
    Far,
}

impl Stride {
    /// The most steps a press may take, `far` being its button's multiplier.
    #[must_use]
    pub const fn steps(self, far: usize) -> usize {
        match self {
            Stride::One => 1,
            Stride::Far => far,
        }
    }

    /// What a whole press moves: its steps of `step` each.
    #[must_use]
    pub fn amount(self, step: i32, far: usize) -> i32 {
        std::iter::repeat_n(step, self.steps(far)).sum()
    }
}

impl World {
    /// Whether `owner`'s treasury holds `price` ([`UNKNOWN_AFFORDS`]
    /// without one).
    #[must_use]
    pub fn affords(&self, owner: u32, price: i32) -> bool {
        self.treasury(owner).map_or(UNKNOWN_AFFORDS, |g| g >= price)
    }

    /// Whether the reward panel's raise button is enabled for `setting`:
    /// `owner` affords it raised by `#RewardDelta` ([`Rewards::raises`]).
    #[must_use]
    pub fn raises_reward(&self, owner: u32, setting: i32) -> bool {
        self.treasury(owner)
            .map_or(UNKNOWN_AFFORDS, |g| Rewards::raises(setting, g))
    }

    /// `setting` after its raise button's press: each step while
    /// [`World::raises_reward`] holds of the running value. E.g. a
    /// treasury holding 300 over it, far: up 300.
    #[must_use]
    pub fn raised_reward(&self, owner: u32, setting: i32, stride: Stride) -> i32 {
        let steps = reward_steps(stride, |gone| self.raises_reward(owner, setting + gone));

        setting + steps.map(|_| REWARD_DELTA).sum::<i32>()
    }

    /// `owner`'s treasury at `gold`: a test's.
    #[cfg(test)]
    pub(crate) fn set_treasury(&mut self, owner: u32, gold: i32) {
        self.players.set_gold(owner, gold);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{CROWN, flag::REWARD_DELTA};

    /// A world without a treasury affords any price; with one, what it
    /// holds and no more.
    #[test]
    fn an_unknown_treasury_affords_anything() {
        let world = World::default();

        assert!(world.treasury(CROWN).is_none());
        assert!(world.affords(CROWN, i32::MAX));
        assert!(world.raises_reward(CROWN, i32::MAX - REWARD_DELTA));
    }

    #[test]
    fn a_known_treasury_affords_what_it_holds() {
        let mut world = World::default();

        world.set_treasury(CROWN, 100);

        assert!(world.affords(CROWN, 100));
        assert!(!world.affords(CROWN, 101));
        assert!(world.raises_reward(CROWN, 100 - REWARD_DELTA));
        assert!(!world.raises_reward(CROWN, 100));
    }
}
