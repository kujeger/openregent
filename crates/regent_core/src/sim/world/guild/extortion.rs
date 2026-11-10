//! The rogues' guild's extortion. **Binary** unless marked.
//!
//! ```text
//!  panel AP48, control 8009 (hot key E)
//!    extortion button      (RoguesGuild_Controller,):
//!      text GMTX 193 "EXTORT (%d)" of total - cut; disabled while the
//!      total is below 1. total: ShowExtortionTotal(guild), cut:
//!      ShowExtortionCut(guild), interface runs
//!         sound EN01 Extortion; total above 0: chat GMTX 194 "Extorted
//!      %d gold, the guild kept %d gold." of total - cut and cut, then
//!      GUILD_ACTION 2, the guild's handle. No price, no timer
//!  decode the extortion: the guild with an agent:
//!    DoExtortion(guild) at once
//! ```
//!
//! `DoExtortion` is the effect: every owned building's and the guild's
//! `APU:2`, 20% of each hero's purse and all of a tax collector's; the rest
//! of a purse to its `StoredGold`; 35% kept, the rest to `AdjustPlayerData`;
//! the palace's tax routes cleared, tax collectors' tasks reset, a
//! `got_gold_bldg` effector on the palace. **Game**: the market probe's,
//! pressed in frame 1442.

use crate::data::fourcc::FourCc;
use crate::sim::world::World;

/// What case 2 dispatches: the string `"DoExtortion"`.
pub const EXTORTION: &str = "DoExtortion";

/// The panel's interface runs: the amount and the rogues' share.
pub const EXTORTION_TOTAL: &str = "ShowExtortionTotal";
pub const EXTORTION_CUT: &str = "ShowExtortionCut";

/// `GMTX` 193, `EXTORT (%d)`: control 8009's text.
pub const EXTORT_TEXT: usize = 193;

/// `GMTX` 194, `Extorted %d gold, the guild kept %d gold.`: the press's
/// line, the interface's, not a decode's.
pub const EXTORTED: usize = 194;

/// `EN01 Extortion`, played at the press (`GE_SoundManager`, phase
/// `EBE0`).
pub const EXTORTION_SOUND: FourCc = FourCc(*b"EN01");

/// What the panel shows: the two interface runs' answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extortion {
    pub total: i32,
    pub cut: i32,
}

impl Extortion {
    /// What 8009 names and the press's line says: the player's share.
    #[must_use]
    pub const fn amount(&self) -> i32 {
        self.total - self.cut
    }

    /// Whether 8009 is enabled: a total of at least 1.
    #[must_use]
    pub const fn ready(&self) -> bool {
        self.total >= 1
    }
}

impl World {
    /// guild action apply case 2 on agent `id`: whether it
    /// dispatches [`EXTORTION`], the guild having a piece and an agent.
    #[must_use]
    pub fn extorts(&self, id: u32) -> bool {
        self.piece(id).is_some() && self.agent(id).is_some()
    }
}
