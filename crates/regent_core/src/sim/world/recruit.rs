//! A hero the player recruits: `CREATE_VEHICLE_MESSAGE`. **Binary** unless
//! marked.
//!
//! ```text
//!  frame f  GSMessageManager case 0x3eb, create vehicle decode:
//!    flags bit 1 ── the recruit decode, on the guild (message):
//!       nothing while its APB:13 is set; the price against APP:0
//!       the hero's NameGenType draws its name into APB:12 (world::Hire)
//!       APB:14 the code, APB:56 the clock, APB:13 the recruit delay
//!       GS_OrderProcessor: a timed order on the guild, callback 0x2006
//!  the delay later  recruit done: the hero is placed, flags 8,
//!       at the guild's exit (GenUnit), HOME the guild, AP:1 the
//!       guild's APB:12, VehicleRec drawing none:
//!       Generate_Character_Attributes, NewUnitInit
//! ```
//!
//! **Game**, play3's four wizards: each is made its descriptor's 14,000 ms
//! after `50 (f + 1)`, the save's `ButtonStartTime` for the last, and every
//! one's spells date from then.

use super::chat::Line;
use super::naming::NAME;
use super::{World, placing::Blockers};
use crate::data::attrib::PENDING_UNIT_NAME;
use crate::data::fourcc::FourCc;
use crate::gpl::load::Program;
use crate::gpl::vm::Fault;

/// `CREATE_VEHICLE_MESSAGE`'s content for a guild's recruit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recruit {
    /// The guild, by agent id; the message names its handle.
    pub guild: u32,
    /// The hero's descriptor.
    pub code: FourCc,
}

impl World {
    /// recruit done for `recruit`: the hero at the place search's
    /// cell from its guild's exit, the guild's owner's, its `HOME` the
    /// guild; its place unknown where the guild's place, or a piece that
    /// could block, is. `None` without its guild or descriptor, or with its
    /// guild ruined or agentless since the decode. The caller
    /// dispatches.
    ///
    /// Its name is the one the decode drew, the guild's `APB:12`
    /// ([`super::hire::Hire`]); the price, the hero's `Cost`, the decode's too.
    /// Without decode frames both are taken here: the name drawn as
    /// `VehicleRec` draws for a spawn.
    ///
    /// # Errors
    ///
    /// The agent's `.dat` instance is missing.
    pub fn recruit(&mut self, program: &Program, recruit: &Recruit) -> Result<Option<u32>, Fault> {
        let body = self.descriptors.body(recruit.code).cloned();
        // A guild that fell since the decode makes none (`GenUnit`
        // under 3).
        let guild = self
            .pieces
            .get(recruit.guild)
            .filter(|g| super::price::stands(g))
            .cloned();
        let spot = body
            .as_ref()
            .zip(self.exit(recruit.guild))
            .filter(|_| !self.unplaced())
            .and_then(|(b, at)| self.place_among(b, at, Blockers::All));

        match (body, guild) {
            (Some(body), Some(guild)) => {
                // "Recruited: Ranger", before the hero.
                let said = self.say_of(guild.owner, Line::Recruited(recruit.code), None);

                let pending = PENDING_UNIT_NAME.packed();
                // Paid at the decode (`hire`): the guild's words cleared,
                // the name it drew given.
                self.hired(recruit.guild);
                let named = guild.attributes.get(&pending).copied().filter(|n| *n != 0);

                // Set before `VehicleRec`'s draw, so none is drawn; no
                // script reads `AP:1` in between.
                let given: Vec<(i32, i32)> =
                    named.map(|n| (NAME.packed(), n)).into_iter().collect();
                let id = self.stand(program, (recruit.code, &body), guild.owner, &given, spot)?;

                // The line names the hero, now it stands.
                self.said_of(said, id);

                self.relations.set_home(self.handle, vec![guild.handle]);
                Ok(Some(id))
            }
            _ => Ok(None),
        }
    }
}
