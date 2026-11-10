//! The player's dismissal of a hero. **Binary** unless marked.
//!
//! ```text
//!  hero panel AP20, control 8002 (hot key X), HeroController:
//!    the ask: a unit state above 2 (**Inferred**: which word) sends
//!    DESTROY_VEHICLE_MESSAGE at once;
//!    else panel AP62, GMTX 20 + 22 "Are you sure you want to dismiss
//!    this character?"
//!  AP62 YES 5501, KillConfirmController:
//!    GUILD_ACTION_MESSAGE action 5, the unit's handle and its point;
//!    then panel APb5, HeroController's follower panel (details::FOLLOWER_PANEL)
//!  GSMessageManager case 0x3f2, before the clock step
//!    guild action apply, case 5: the unit by handle,
//!    with an agent: APV:28 HasBeenDismissed 1,
//!    then Unit_Dismissed(agent) dispatched at once
//! ```
//!
//! Nothing else: no gold back, no roster change but the script's.
//! `Unit_Dismissed` takes it off its guild's `Members`, homes it on the
//! palace and gives it `flee_map`, which walks it off the map. With
//! `APV:28` set the hero panel's buttons hide.
//!
//! A dead hero's dismissal, and a double click on an own ruin or body,
//! send `DESTROY_VEHICLE_MESSAGE` by handle;
//! its decode (destroy vehicle decode, case
//! `0x3ec`) dispatches [`GRAVESTONE`] on the unit at once, nothing else.
//! The script runs the unit's `ActiveScript` thread at once.

use super::World;
use crate::data::attrib::HAS_BEEN_DISMISSED;

/// What case 5 dispatches: the string `"Unit_Dismissed"`.
pub const DISMISSED: &str = "Unit_Dismissed";

/// What `DESTROY_VEHICLE_MESSAGE`'s decode dispatches: the string at
/// `"Unit_Dismiss_Gravestone"`.
pub const GRAVESTONE: &str = "Unit_Dismiss_Gravestone";

impl World {
    /// The guild action apply, case 5, on agent `id`, before its
    /// dispatch: whether it stands to be dismissed.
    #[must_use]
    pub fn dismissable(&self, id: u32) -> bool {
        self.piece(id).is_some() && self.agent(id).is_some()
    }

    /// The agent `DESTROY_VEHICLE_MESSAGE` on `handle` dispatches
    /// [`GRAVESTONE`] on; none, nothing runs.
    #[must_use]
    pub fn gravestone(&self, handle: u32) -> Option<u32> {
        self.holder(handle).filter(|id| self.agent(*id).is_some())
    }

    /// Case 5's write before [`DISMISSED`] runs on `id`.
    pub(super) fn dismiss(&mut self, id: u32) {
        self.write(id, HAS_BEEN_DISMISSED.packed(), 1);
    }
}
