//! The warriors' guild's Call to Arms. **Binary** unless marked.
//!
//! ```text
//!  panel AP52, control 8009 CALL TO ARMS (hot key C), price box 4481
//!    assembly state (WarGuildController): 4481 the
//!      price, red under it; 8009 disabled (message 10) with no living
//!      member (guild living count)
//!         the tooltip: GMTX 28 "- (%d) gold" of the price; none living
//!      GMTX 260 "No members to Call to Arms."
//!         #AssemblyCost in the treasury: PlayerResourceMessage spend,
//!      then GUILD_ACTION 3, the guild's handle and point; else GMTX 37
//!      "Can't afford a Call to Arms." to the chat
//!  decode guild action apply, case 3: the
//!    guild with an agent: DoAssembly(guild) at once
//! ```
//!
//! `DoAssembly` is the effect: each living, unfrozen member out of its
//! building, else its tasks reset; a 1,200 ms `teleport_effector` on it,
//! `TeleportToUnit` to the guild, `ActiveScript` `berserk`; then the
//! guild's `Call_to_arms` sound. No timer: the call can be pressed again
//! at once.

use crate::gpl::exec::Literal;
use crate::gpl::load::Program;
use crate::sim::world::{World, offer::Button};

/// What case 3 dispatches: the string `"DoAssembly"`.
pub const ASSEMBLY: &str = "DoAssembly";

/// The call's price, read once into assembly price.
pub const ASSEMBLY_COST: &str = "#AssemblyCost";

/// `GMTX` 37, `Can't afford a Call to Arms.`: the press's line, the
/// interface's, not a decode's.
pub const NO_GOLD_ASSEMBLY: usize = 37;

impl World {
    /// The Call to Arms button of `owner`'s guild `id` (control 8009 on
    /// `AP52`): at [`ASSEMBLY_COST`], ready while the guild has a living
    /// member. A short treasury does not disable it: the press refuses
    /// with [`NO_GOLD_ASSEMBLY`]. `None` for another's piece or no guild.
    #[must_use]
    pub fn assembly_offer(&self, program: &Program, owner: u32, id: u32) -> Option<Button<()>> {
        let members = self
            .members(id)
            .filter(|_| self.pieces.owner(id) == Some(owner))?;
        let price = match program.constant(ASSEMBLY_COST) {
            Some(Literal::Int(v)) => v,
            _ => 0,
        };

        Some(Button {
            what: (),
            price,
            ready: members.living > 0,
        })
    }

    /// Guild action apply case 3 on agent `id`: whether it
    /// dispatches [`ASSEMBLY`], the guild having a piece and an agent.
    #[must_use]
    pub fn assembles(&self, id: u32) -> bool {
        self.piece(id).is_some() && self.agent(id).is_some()
    }
}
