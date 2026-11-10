//! How `MajestyHD.exe` starts a new game: what it dispatches before the first
//! frame, how map objects become GPL agents, and where every RNG gets its seed.
//!
//! **Binary** unless marked. Prose: `docs/gpl/runtime.md`, "Starting a game".
//!
//! ```text
//!  custom-game dialog        CalculateDifficulty…, map generated from the map number
//!        │
//!  game state loop, state 2
//!        │
//!  game start ── world RNG  <── the session word
//!        │        └─ GSMessageManager: GPL RNG <── the session word
//!        │
//!  new game setup, game time 0
//!        1 InitializeGpl                 GplAIRoot is agent 1
//!        2 GS_Level      (0xc)           an agent per qualifying unit, ids 2…
//!        3 the quest's script            "Freestyle", or the quest's name
//!        4 GS_Level      (0xa)           NewUnitInit per agent, same order
//!        5 Palace_Pre_Birth              per owner, on its palace
//!        │
//!  state 3: game frame step, game time 50
//! ```

// ---------------------------------------------------------------------------
// The sequence
// ---------------------------------------------------------------------------

/// The script a freestyle game dispatches as the quest's script, through a
/// pointer at freestyle name. Not a literal at a dispatch site, so not in
/// [`super::runtime::DISPATCHED`].
pub const FREESTYLE: &str = "Freestyle";

/// Descriptor code prefixes `Palace_Pre_Birth` is dispatched on, compared on the
/// low three bytes in new game setup and new game setup: a palace (`ABJ1`–`ABJ4`)
/// and, where an owner has none, an outpost (`ABm1`). Keyed by owner, so one
/// dispatch per owner, in owner order.
pub const PALACES: [&[u8; 3]; 2] = [b"ABJ", b"ABm"];

/// What new game setup dispatches, in order, for `agents` agents and
/// `owners` owners with a palace. `quest` is the quest's script.
///
/// ```
/// use regent_core::gpl::startup::{FREESTYLE, dispatches};
///
/// assert_eq!(
///     dispatches(FREESTYLE, 2, 1),
///     ["InitializeGpl", "Freestyle", "NewUnitInit", "NewUnitInit", "Palace_Pre_Birth"]);
/// ```
#[must_use]
pub fn dispatches(quest: &str, agents: usize, owners: usize) -> Vec<&str> {
    let init = std::iter::repeat_n("NewUnitInit", agents);
    let palaces = std::iter::repeat_n("Palace_Pre_Birth", owners);

    ["InitializeGpl", quest]
        .into_iter()
        .chain(init)
        .chain(palaces)
        .collect()
}

// ---------------------------------------------------------------------------
// Seeds
// ---------------------------------------------------------------------------
