//! The original's autosave. **Binary** unless marked.
//!
//! Game frame step tests it first thing each frame,
//! before the frame's messages run:
//!
//! ```text
//!  the autosave option set and frames per autosave not 0,
//!  game mode (GSMessageManager) not 2,
//!  frames run > 0 and (frames run - 1) % frames per autosave == 0
//!    ──> the autosave: GMTX 335 to the chat in green, the game saved
//!        under the name GSMessageManager holds, "autosave";
//!        "Frame %d: Autosave successful" logged
//! ```
//!
//! So the game is saved as frame 1, `N + 1`, `2N + 1`... has run: a
//! day's second frame, where the interval ends a day. One file, each
//! autosave over the last. The options page (`Option4MenuController`,
//! method) offers off and [`FRAMES`]; a LAN game takes the host's
//! (net game start sends it, net lobby sets it at
//! Net lobby). **Inferred**: mode 2 is a LAN game loaded from a save,
//! the one mode that seeds from rng saved state; no other write of
//!  was looked for.

use crate::data::text::Ref;
use crate::gpl::runtime::MS_PER_FRAME;
use crate::view::panel::day::DAY_MS;

/// The options page's intervals, frames: 6000, 12000 and 24000 written
/// to frames per autosave (`Option4MenuController`,
/// `Option4MenuController`): 5, 10 and 20 game days.
pub const FRAMES: [u32; 3] = [6_000, 12_000, 24_000];

/// The save's name.
pub const NAME: &str = "autosave";

/// The chat line said as it saves (`gmtx_string(0x14f)` in
/// autosave write).
pub const NOTICE: Ref = Ref::gmtx(335);

/// Frames in `days` game days.
#[must_use]
pub const fn frames(days: u32) -> u32 {
    days * DAY_MS / MS_PER_FRAME
}

/// Whether an autosave every `every` frames falls due once `run` frames
/// have run: at 1, then each `every` on.
#[must_use]
pub const fn due(run: u32, every: u32) -> bool {
    every > 0 && run > 0 && (run - 1).is_multiple_of(every)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_options_are_5_10_and_20_days() {
        assert_eq!(FRAMES, [frames(5), frames(10), frames(20)]);
    }

    /// Due as frame 1 has run, and one frame past each interval.
    #[test]
    fn an_autosave_falls_due_one_frame_past_the_interval() {
        let five = FRAMES[0];

        assert!(due(1, five));
        assert!(due(five + 1, five));
        assert!(due(2 * five + 1, five));
        assert!(!due(0, five));
        assert!(!due(five, five));
        assert!(!due(five + 2, five));
        assert!(!due(1, 0));
    }
}
