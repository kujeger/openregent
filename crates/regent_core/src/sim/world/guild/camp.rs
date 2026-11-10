//! The rangers' camp move: `GUILD_ACTION_MESSAGE` action 0, and the
//! `PlayerResourceMessage` its panel pays with. **Binary** unless marked;
//! no trace holds either.
//!
//! ```text
//!  panel   RangersGuild_Controller, control 8009: the camp standing
//!          (APU:0 not below 0), #MoveCampCost in the treasury:
//!          PlayerResourceMessage, spend; mode Move; the guild's preview
//!          (order construction)
//!  click   the camp move: the preview test of the ground alone;
//!          fits: GUILD_ACTION 0, the guild's handle, the preview's point
//!  cancel  the cost back: PlayerResourceMessage, add
//!  here    the press and the cancel are inputs of their own, CampBegin
//!          and CampLeave: the price held per player in the world, so a
//!          refund is only ever of a price paid ([`CampMove`])
//!  decode  guild action apply, case 0, the guild with an agent:
//!          place search from the point, margin ConstructData1,
//!          flags 0x1000; found: KickoutRangers(agent), unit move to,
//!          building clear, MoveCamp(agent); Place_Building's
//!          Begin at the guild, as its owner's (guild action apply)
//! ```
//!
//! No flatten and no pad: the decode calls neither. **Inferred**: the
//! ground under the old and new camp stays as it was.

use crate::base::digest;
use crate::data::attrib::HP;
use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::sim::clearing::Time;
use crate::sim::space;
use crate::sim::world::{World, offer::Button};

/// What guild action apply dispatches before the move, `"KickoutRangers"`.
pub const KICKOUT: &str = "KickoutRangers";

/// What it dispatches after, `"MoveCamp"`.
pub const MOVE_CAMP: &str = "MoveCamp";

/// The camp move's price, read once into move camp cost.
pub const MOVE_CAMP_COST: &str = "#MoveCampCost";

/// The rangers' guild's code prefix, whose panel moves the camp.
pub const RANGERS_GUILD: [u8; 3] = *b"ABW";

/// `GUILD_ACTION_MESSAGE` action 0: guild agent `guild` to `at`, 32nds,
/// the preview's point at the click.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Camp {
    pub guild: u32,
    pub at: (i32, i32),
}

/// Which way a `PlayerResourceMessage` moves its amount: message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// 1: `GS_Player`, taken if the treasury holds it.
    Spend,
    /// 0: `GS_Player`, added.
    Add,
}

/// A `PlayerResourceMessage` (type `0xb`, `MessageManager`) on
/// `APP:0`, the treasury: its decode logs `Decoding
/// PlayerResourceMessage: Amount = %d`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource {
    pub owner: u32,
    pub amount: i32,
    pub flow: Flow,
}

/// A camp move begun and not yet sent or left: its guild, and the price
/// paid as it began, which comes back unless the move is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CampMove {
    pub guild: u32,
    pub price: i32,
}

/// Each player's camp move begun, by player. In the engine the panel's
/// controller holds it, on one machine; here the world, so every machine
/// knows what a cancel may give back.
#[derive(Clone, Debug, Default)]
pub(in crate::sim::world) struct Moves(digest::Map<u32, CampMove>);

/// `#MoveCampCost`, 0 where the program lacks it.
fn move_cost(program: &Program) -> i32 {
    match program.constant(MOVE_CAMP_COST) {
        Some(Literal::Int(v)) => v,
        _ => 0,
    }
}

impl World {
    /// `MessageManager` on the treasury.
    pub fn resource(&mut self, r: &Resource) {
        match r.flow {
            Flow::Spend => {
                self.spend(r.owner, r.amount);
            }
            Flow::Add => self.earn(r.owner, r.amount),
        }
    }

    /// Where camp move `camp` puts its guild, 32nds: the place search
    /// from its point with the guild's clearing ring
    /// (`World::place_ringed`); `None` without the guild's agent or a
    /// free spot. The guild's own pad counts, as the grid holds it.
    #[must_use]
    pub fn camp_spot(&self, camp: &Camp) -> Option<(i32, i32)> {
        let body = self
            .pieces
            .get(camp.guild)
            .filter(|_| self.agents.alive(camp.guild))
            .and_then(|p| self.descriptors.body(p.code))?;

        self.place_ringed(body, camp.at)
    }

    /// The move, between the two dispatches: unit move to to
    /// `spot`, 32nds, then the clearing ring.
    pub fn move_camp(&mut self, guild: u32, spot: (i32, i32)) {
        let fixed = (
            crate::sim::units::from_pos32(spot.0),
            crate::sim::units::from_pos32(spot.1),
        );
        let standing = (space::half(fixed.0), space::half(fixed.1));
        let body = self.piece_body(guild).cloned();

        if let Some(place) = self.pieces.place_mut(guild) {
            place.fixed = fixed;
            place.standing = standing;
        }

        self.jumped(guild);

        if let Some(body) = body {
            self.clear(&body, standing, Time::Playing);
        }
    }

    /// After `MoveCamp`: `Place_Building`'s `Begin` at guild `id`, as its
    /// owner's.
    pub fn camp_placed(&mut self, id: u32) {
        if let Some(owner) = self.pieces.owner(id) {
            self.cue_owned(
                crate::sim::world::sound::From::Agent(id),
                crate::sim::world::sound::PLACE_BUILDING,
                crate::sim::world::sound::BEGIN,
                owner,
            );
        }
    }

    /// The camp move button of `owner`'s rangers' guild `id`
    /// (`RangersGuild_Controller`, control 8009): at
    /// [`MOVE_CAMP_COST`], ready while the camp stands (`APU:0` not below
    /// 0, else "Camp has no hit points!") and the treasury holds the price
    /// (else "Can't afford to Move Camp."). `None` for any other piece.
    #[must_use]
    pub fn camp_offer(&self, program: &Program, owner: u32, id: u32) -> Option<Button<()>> {
        let guild = self.pieces.owned(id, owner, &RANGERS_GUILD)?;
        let price = move_cost(program);
        let standing = guild.get(HP).unwrap_or_default() >= 0;

        Some(Button {
            what: (),
            price,
            ready: standing && self.treasury(owner).is_none_or(|g| g >= price),
        })
    }

    /// `player`'s camp move begun and not yet sent or left, if any.
    #[must_use]
    pub fn camp_held(&self, player: u32) -> Option<CampMove> {
        self.pending.camps.0.get(&player).copied()
    }

    /// `player` begins moving `guild`'s camp (the panel's press): its
    /// `#MoveCampCost` spent, as the panel's `PlayerResourceMessage`
    /// spends, and held. A move held and not sent is left first, its
    /// price back (**Inferred**: one move at a time).
    pub fn camp_begin(&mut self, program: &Program, player: u32, guild: u32) {
        let price = move_cost(program);

        self.camp_leave(player);
        self.spend(player, price);
        self.pending
            .camps
            .0
            .insert(player, CampMove { guild, price });
    }

    /// `player`'s camp move left without its move (camp move place,
    /// button 2): its price back. Nothing without one. **Binary**: the
    /// kept price, though the spend found the treasury short and took
    /// nothing (camp move place, cursor amount; `MessageManager`
    /// spends only what is held), so a short spend then a cancel gains
    /// gold.
    pub fn camp_leave(&mut self, player: u32) {
        if let Some(m) = self.pending.camps.0.remove(&player) {
            self.earn(player, m.price);
        }
    }

    /// `player`'s camp move sent: its price kept, the move no longer held.
    pub fn camp_sent(&mut self, player: u32) -> Option<CampMove> {
        self.pending.camps.0.remove(&player)
    }

    /// `player` holding `held`, as a begin would leave it, for tests.
    #[cfg(test)]
    pub(crate) fn hold_camp(&mut self, player: u32, held: CampMove) {
        self.pending.camps.0.insert(player, held);
    }
}

/// To and from the save model.
mod persist {
    use super::*;
    use crate::save::{self as m, Refused, Saved};
    use std::result::Result;

    impl Saved for Moves {
        type Model = std::collections::BTreeMap<u32, m::CampMove>;

        fn save(&self) -> Self::Model {
            self.0
                .iter()
                .map(|(p, c)| {
                    let model = m::CampMove {
                        guild: c.guild,
                        price: c.price,
                    };

                    (*p, model)
                })
                .collect()
        }

        fn load(model: Self::Model) -> Result<Moves, Refused> {
            Ok(Moves(
                model
                    .into_iter()
                    .map(|(p, c)| {
                        let held = CampMove {
                            guild: c.guild,
                            price: c.price,
                        };

                        (p, held)
                    })
                    .collect(),
            ))
        }
    }
}
