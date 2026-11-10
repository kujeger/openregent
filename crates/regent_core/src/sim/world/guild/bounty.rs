//! The Hall of Champions' bounty: `GUILD_ACTION_MESSAGE` action `0x14`,
//! the player's hall words and the `Champion_Icon` it hangs. **Binary**
//! unless marked.
//!
//! ```text
//!  panel MX00 8851 Place Bounty (B) opens MX01: list 5000, the slain
//!    types (bounty list fill); 2050 short (S), 2051 long (L), each
//!    its price, disabled under it (bounty buttons state); a press
//!    sends GUILD_ACTION 0x14 (code, index 1 or 2, the hall's handle)
//!  decode guild action apply, case 0x14, the hall with an agent:
//!    period = HallOfChampions_Bounty_Period(index), price =
//!    HallOfChampions_Bounty_Cost(index), interface runs; the owner's
//!    APP:0 holds the price: taken (GE_Player), APP:2 the code,
//!    APP:3 period - 350, APP:4 the clock; Champion_Icon for period -
//!    350 on every monster of the code (unit list 1); a Wait of period -
//!    350 on the owner's palace, callback 0x200c
//!  end   hall timer end: ended: APP:2 and APP:3 0; APP:4 stays
//!  SetHallFlag(unit), a slot 7 unit: players 0 to 3, the first whose
//!    APP:2 is its code, Champion_Icon for APP:3 + APP:4 - the clock
//!    where above 0
//! ```
//!
//! Heroes chase a marked monster: `Check_Champion_Rewards` asks
//! `CheckEffector(monster, "Champion_Icon")`.

use crate::data::fourcc::FourCc;
use crate::gpl::vm::{Fault, Handle, Vm};
use crate::sim::world::timed::Timer;
use crate::sim::world::{Answer, Found, World, agent_ref};

/// What the decode and the panel run for a bounty's length, ms
/// (bounty period, the string `"HallOfChampions_Bounty_Period"`).
pub const BOUNTY_PERIOD: &str = "HallOfChampions_Bounty_Period";

/// What the decode and the panel run for a bounty's price
/// (bounty cost, `"HallOfChampions_Bounty_Cost"`).
pub const BOUNTY_COST: &str = "HallOfChampions_Bounty_Cost";

/// The effector a bounty hangs on its monsters (`"Champion_Icon"`).
pub const CHAMPION_ICON: &str = "Champion_Icon";

/// Taken off the period for the icons, the palace's `Wait` and `APP:3`
/// (`-0x15e` added, guild action apply).
pub const BOUNTY_LEAD: i32 = 350;

/// The players `SetHallFlag` walks: 0 to 3 (a compare with 3, gpl set hall flag).
const HALL_PLAYERS: u32 = 4;

/// A bounty's term: the panel's two buttons, the GPL functions' index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Term {
    /// 2050, hot key S: index 1.
    Short,
    /// 2051, hot key L: index 2.
    Long,
}

/// The player slot that is no kingdom: `SetHallFlag` acts only on its
/// units (a compare with 7 in gpl set hall flag).
pub const NO_KINGDOM: u32 = 7;

impl Term {
    /// Both, in the panel's order.
    pub const ALL: [Term; 2] = [Term::Short, Term::Long];

    /// The index the message and the GPL functions take.
    #[must_use]
    pub const fn index(self) -> i32 {
        match self {
            Term::Short => 1,
            Term::Long => 2,
        }
    }

    /// The term of `index`, where one is.
    #[must_use]
    pub const fn of(index: i32) -> Option<Term> {
        match index {
            1 => Some(Term::Short),
            2 => Some(Term::Long),
            _ => None,
        }
    }
}

/// `GUILD_ACTION_MESSAGE` action `0x14`: a bounty on `code` from `hall`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounty {
    pub hall: u32,
    pub code: FourCc,
    pub term: Term,
}

/// A player's hall words: `APP:2` the code (0 none), `APP:3` the length,
/// `APP:4` the clock it began.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hall {
    pub code: u32,
    pub length: i32,
    pub began: i32,
}

/// `GMTX` 412, `Calling Champions`: the hall's text over its bar while a
/// bounty runs.
pub const CALLING: usize = 412;

impl World {
    /// What the hall's panel (`MX00`) shows for `owner`'s
    /// bounty: `elapsed` of `length` ms while its code is set and the
    /// clock less its start is within its length; then 8851 hides, 4500
    /// is that bar and 4501 says [`CALLING`].
    #[must_use]
    pub fn bounty_running(&self, owner: u32) -> Option<(i32, i32)> {
        let now = self.now.cast_signed();

        self.players
            .hall(owner)
            .filter(|h| h.code != 0 && now.wrapping_sub(h.began) <= h.length)
            .map(|h| (now.wrapping_sub(h.began), h.length))
    }

    /// Player `player`'s hall words, where a bounty ever wrote them.
    #[must_use]
    pub fn hall(&self, player: u32) -> Option<Hall> {
        self.players.hall(player)
    }

    /// Case `0x14` after its two interface runs gave `period` and
    /// `price`: whether the bounty is placed. The hall needs an agent and
    /// its owner the price.
    pub fn place_bounty(&mut self, b: &Bounty, period: i32, price: i32) -> bool {
        let owner = self
            .agent(b.hall)
            .and(self.pieces.get(b.hall))
            .map(|p| p.owner);
        let paid = owner.filter(|o| self.spend(*o, price));

        if let Some(owner) = paid {
            let length = period - BOUNTY_LEAD;
            let code = b.code.word();
            let mut marked: Vec<u32> = self
                .pieces
                .iter()
                .filter(|(_, p)| p.code == b.code)
                .map(|(id, _)| *id)
                .filter(|id| self.monster(*id))
                .collect();

            marked.sort_unstable();

            self.players.set_hall(
                owner,
                Hall {
                    code,
                    length,
                    began: self.now.cast_signed(),
                },
            );

            for id in marked {
                self.hang_for(id, CHAMPION_ICON, length);
            }

            if let Some(palace) = self.player_palace(owner) {
                self.time(palace, Timer::Hall(owner), length.cast_unsigned());
            }
        }

        paid.is_some()
    }

    /// Callback `0x200c` on `player`'s palace's `Wait`, ended: its code
    /// and length 0.
    pub(in crate::sim::world) fn bounty_over(&mut self, player: u32) {
        if let Some(h) = self.players.hall_mut(player) {
            h.code = 0;
            h.length = 0;
        }
    }

    /// `SetHallFlag` on piece `id`, a slot 7 unit: a `Champion_Icon` for
    /// what is left of the first bounty on its code among players 0 to 3.
    pub(in crate::sim::world) fn flag_champion(&mut self, id: u32) {
        let code = self
            .pieces
            .get(id)
            .filter(|p| p.owner == NO_KINGDOM)
            .map(|p| p.code.word());
        let now = self.now.cast_signed();
        let left = code.and_then(|code| {
            (0..HALL_PLAYERS)
                .filter_map(|p| self.players.hall(p))
                .find(|h| h.code != 0 && h.code == code)
                .map(|h| h.length.wrapping_add(h.began).wrapping_sub(now))
        });

        if let Some(ms) = left.filter(|ms| *ms > 0) {
            self.hang_for(id, CHAMPION_ICON, ms);
        }
    }

    /// `SetHallFlag(unit)`, gpl set hall flag: a unit or a throw; only a unit
    /// of [`NO_KINGDOM`] is marked, with a `Champion_Icon` effector
    /// for the time left on the first player among 0–3
    /// whose `APP:2` names its descriptor, `APP:3` plus `APP:4` less the
    /// clock, where above 0 ([`World::place_bounty`] writes them).
    pub(in crate::sim::world) fn hall_flag(
        &mut self,
        vm: &mut Vm<'_>,
        args: &[Handle],
    ) -> std::result::Result<Answer, Fault> {
        let id = agent_ref(vm, args, 1)?;

        match self.found(id) {
            Found::Piece(_) => {
                self.flag_champion(id);
                Ok(Answer::Given)
            }
            Found::Gone => Err(Fault::throw("SetHallFlag() unit is null")),
            Found::Unsure => Ok(Answer::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A term's index is the GPL functions' bounty index.
    #[test]
    fn a_short_bounty_is_index_one_and_a_long_two() {
        assert_eq!(Term::ALL.map(Term::index), [1, 2]);
        assert_eq!(Term::of(2), Some(Term::Long));
        assert_eq!(Term::of(3), None);
    }

    /// What is left of a bounty is a 32-bit sum: past `i32::MAX` it
    /// wraps below 0, and no icon hangs.
    #[test]
    fn a_bounty_left_past_the_most_wraps() {
        const CODE: [u8; 4] = *b"AVH1";

        let mut world = World::default();
        let piece = crate::sim::world::Piece {
            owner: NO_KINGDOM,
            code: FourCc(CODE),
            ..crate::sim::world::Piece::default()
        };

        world.add_test_piece(1, piece);
        world.players.set_hall(
            0,
            Hall {
                code: u32::from_le_bytes(CODE),
                length: i32::MAX,
                began: 10,
            },
        );
        world.flag_champion(1);

        assert_eq!(world.effectors.iter().count(), 0);
    }

    /// The hall's end clears the code and length, keeping the start.
    #[test]
    fn a_bounty_ends_clearing_its_code_and_length() {
        let mut world = World::default();

        world.players.set_hall(
            0,
            Hall {
                code: 7,
                length: 119_650,
                began: 269_750,
            },
        );
        world.bounty_over(0);

        assert_eq!(
            world.hall(0),
            Some(Hall {
                code: 0,
                length: 0,
                began: 269_750
            })
        );
    }
}
