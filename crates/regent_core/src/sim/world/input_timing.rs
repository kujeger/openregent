//! When the player's input runs. **Binary**, `GSMessageManager`: a
//! message is decoded between two frames' scheduler passes, and err.log
//! names the frame, `Frame f: Decoding NAME`, the pass before.
//!
//! ```text
//!  pass f ── Frame f: Decoding CREATE_BUILDING ── NewUnitInit ── pass f + 1
//! ```
//!
//! **Game**: in play5, probed, the scheduler's model puts every one of
//! the 50 decodes' dispatches right after pass f; a recruit's hero comes
//! its descriptor's delay later. A log without a probe shows no frame
//! between passes that run nothing, so checking against one held each
//! input's dispatch to its frame.
//!
//! Every other engine dispatch between two passes comes after the clock
//! step and the periodic tick, in `GE_Level` before the scheduler:
//! an order's callback in method (an action's function at its last
//! frame, a timed order's), a deleted unit's `UnitDelete` in method
//! ([`Stage::unheld`]).

use super::NEW_UNIT;
use super::build::{PREVIEW_ID, UPGRADED};

/// Where in the next frame the dispatch runs, and so the clock it sees.
///
/// ```text
///  pass f ── messages ── clock to 50 (f + 1), tick ── orders ── deletes ── pass f + 1
///            Message                                  Order
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Decoded by `GSMessageManager` before the clock steps
    /// (game frame step, the step): game time `50 f`.
    Message,
    /// A timed order's callback in `GE_Level`, after the step: game
    /// time `50 (f + 1)`. A recruit's hero; **Game**:
    /// play3's first wizard's `Wizard_tree` first runs 300 ms after that.
    /// An action's function (callback `0x1000`), and a deleted unit's
    /// `UnitDelete` in method after the orders, come here too.
    Order,
}

/// Engine dispatches between passes that come before the clock step with
/// no hold to say so: the player's input a message decodes, the music's
/// callback and the interface's queries.
///
/// - `NewUnitInit` of a building, flag or spell's unit, and
///   `building_upgraded`, by `GSMessageManager`; a guild's action
///   (`DoWizTowerEnchant`, **Game**: play5's decode at 2931;
///   `Mausoleum_Resurrect_Begin`). A hero's
///   `NewUnitInit` follows its `Generate_Character_Attributes`, which is
///   a timed order's, so it runs after the step all the same.
/// - `Farseeing_unit_created`, **Inferred**: it follows its unit's
///   `NewUnitInit` with nothing between (play5).
/// - The music's, music player after `GE_Level` in the frame
///   step (gsgl read keeps the name).
/// - The interface's (order construction, palace henchmen fill,
///   `GSGameView`): **Inferred** before the step; the world does
///   not answer them. So is `UnitDelete` of the build menu's preview,
///   [`PREVIEW_ID`]: **Game**, play3, each comes just before its
///   building's `NewUnitInit`.
pub const BEFORE_STEP: [&str; 10] = [
    NEW_UNIT,
    UPGRADED,
    "DoWizTowerEnchant",
    super::guild::mausoleum::RESURRECT_BEGIN,
    "Farseeing_unit_created",
    "FreeStyle_Music_Stopped",
    "Quest_Music_Stopped",
    "CanIBuildThisBuilding",
    "GetSupportedHenchmen",
    "IsValidMiniViewTarget",
];

impl Stage {
    /// The stage of an engine dispatch between passes that no hold names,
    /// `agent` its first argument's id: [`BEFORE_STEP`]'s and the
    /// preview's before the clock step, every other after it.
    #[must_use]
    pub fn unheld(function: &str, agent: Option<i32>) -> Stage {
        let before = BEFORE_STEP.iter().any(|f| f.eq_ignore_ascii_case(function));

        if before || agent == Some(PREVIEW_ID) {
            Stage::Message
        } else {
            Stage::Order
        }
    }
}
