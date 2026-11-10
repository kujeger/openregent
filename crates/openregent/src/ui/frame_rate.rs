//! `FRAME IT`'s frame rate in the day counter's place
//! (`regent_core::view::panel::day::Frames`).
//!
//! ```text
//!  Output::frame_cheats ──> Frames::cheat
//!  frames run, real time ─────> Meter ──> Frames::tick at the clock
//!  a click on the counter ────> Frames::click (panel::bars::labels)
//! ```
//!
//! The rate is game frames a real second, as game frame step
//! counts its own runs.

use std::time::Instant;

use bevy::prelude::*;
use regent_core::view::panel::day::Meter;

use crate::game::seen::{News, Watch};
use crate::panel::Panels;

/// The meter and where it last read the game: frames run, clock, when.
#[derive(Resource, Default)]
pub struct FrameRate {
    meter: Meter,
    last: Option<(u32, u32, Instant)>,
}

/// The cheat's runs taken, the frames since the last pass measured, the
/// counter ticked to the clock.
#[allow(clippy::needless_pass_by_value, reason = "a Bevy system")]
pub fn step(
    mut news: ResMut<News>,
    watch: Watch,
    mut rate: ResMut<FrameRate>,
    mut panels: ResMut<Panels>,
) {
    let frames = &mut panels.state.frames;
    let now = Instant::now();

    for _ in 0..std::mem::take(&mut news.frame_cheats) {
        frames.cheat();
    }

    if let Some((run, clock, at)) = rate.last {
        let ms = u32::try_from(now.duration_since(at).as_millis()).unwrap_or(u32::MAX);

        rate.meter.step(watch.frames.saturating_sub(run), ms);
        frames.tick((clock, watch.now), rate.meter.rate());
    }

    rate.last = Some((watch.frames, watch.now, now));
}

/// The meter, a game's.
pub fn plugin(app: &mut App) {
    use crate::game::scope::Scoped as _;

    app.per_game::<FrameRate>();
}
