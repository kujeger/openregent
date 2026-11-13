//! The game clock: each drawn frame handed to the sim thread, and what
//! it published taken for the drawing.
//!
//! ```text
//!  tick  the frame to the sim thread, its newest state taken ──> Snapshot, Reel
//! ```
//!
//! The simulation runs in fixed 50 ms frames, as many as the sim
//! thread's clock owes; a piece is drawn where the last one left it, or
//! between its last two places with `[enhancements] smooth_position`
//! ([`Motion`]).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use regent_core::base::fixed::Fixed;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::sim::world::{draw_view::Shown, missile::Flown};
use regent_draw::map::reel::Reel;

use crate::files::config::Settings;
use crate::game::seen::{Frame, Interest, News};
use crate::game::speed::Speedup;
use crate::game::thread::{Pace, SimThread, Slide, Start};
use crate::map::mirror::Was;
use crate::menu::Drape;
use crate::ui;

/// The world as the last two frames left it: what [`crate::map::mirror::sync`]
/// mirrors, and the missiles: the sim thread's [`Steps`].
#[derive(Resource, Default)]
pub struct Snapshot {
    steps: Arc<Steps>,
    /// Where the sim thread's clock stood as it published them.
    slide: Slide,
    /// How far from its place a frame before to its place now a piece
    /// draws, 0 to 1 ([`Motion::between`]).
    pub(crate) between: f32,
}

impl Snapshot {
    /// `steps`, drawn at their last frame's place.
    #[cfg(test)]
    pub(crate) fn of(steps: Steps) -> Snapshot {
        Snapshot {
            steps: Arc::new(steps),
            slide: Slide::default(),
            between: 1.0,
        }
    }
}

impl std::ops::Deref for Snapshot {
    type Target = Steps;

    fn deref(&self) -> &Steps {
        &self.steps
    }
}

/// What the last two game frames showed, taken on the sim thread after
/// each step (`game::thread`).
#[derive(Clone, Default)]
pub struct Steps {
    pub(crate) now: Vec<Shown>,
    /// The missiles in flight.
    pub(crate) flown: Vec<Flown>,
    /// Where each missile was a frame before, by handle.
    pub(crate) flown_before: HashMap<u32, [Fixed; 3]>,
    /// Where each piece was a frame before.
    before: HashMap<u32, (i32, i32)>,
    /// How high each flier was a frame before ([`Shown::aloft`]).
    heights: HashMap<u32, Fixed>,
}

impl Steps {
    /// These, then a step that left `world`.
    pub(crate) fn after(&self, world: &regent_core::sim::world::World) -> Steps {
        let now = world.shown();
        let before = walked(&self.now, &now);
        let heights = self
            .now
            .iter()
            .filter(|s| before.contains_key(&s.agent))
            .filter_map(|s| s.aloft.map(|z| (s.agent, z)))
            .collect();

        Steps {
            now,
            flown: world.flown(),
            flown_before: self.flown.iter().map(|f| (f.handle, f.at)).collect(),
            before,
            heights,
        }
    }

    /// These with `world`'s pieces if none are shown yet, as before the
    /// first step; `None` otherwise.
    pub(crate) fn filled(&self, world: &regent_core::sim::world::World) -> Option<Steps> {
        self.now.is_empty().then(|| Steps {
            now: world.shown(),
            ..self.clone()
        })
    }

    /// `now` shown, nothing before: a test's.
    #[cfg(test)]
    pub(crate) fn showing(now: Vec<Shown>) -> Steps {
        Steps {
            now,
            ..Steps::default()
        }
    }

    /// Where agent `agent` was a frame before, if it walked there.
    pub(crate) fn was(&self, agent: u32) -> Was {
        Was {
            at: self.before.get(&agent).copied(),
            aloft: self.heights.get(&agent).copied(),
        }
    }
}

/// How a piece moves on the screen between game frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Motion {
    /// At its last frame's place, as the original: unit screen place
    /// projects the unit's stored position, no earlier one.
    Stepped,
    /// Slid from its place a frame before (`[enhancements] smooth_position`).
    Smooth,
}

impl Motion {
    pub(crate) const fn of(settings: &Settings) -> Motion {
        if settings.enhancements.smooth_position {
            Motion::Smooth
        } else {
            Motion::Stepped
        }
    }

    /// [`Snapshot::between`] with `owed` ms of a `frame` owed: smooth,
    /// the share of the next frame the clock is into; stepped, all the
    /// way to the last frame's place.
    pub(crate) const fn between(self, owed: f32, frame: f32) -> f32 {
        match self {
            Motion::Stepped => 1.0,
            Motion::Smooth => (owed / frame).clamp(0.0, 1.0),
        }
    }
}

/// Hand the drawn frame to the sim thread (`game::thread`): the changes
/// queued since the last, what the interface asks, pause and speed, and
/// the time passed. Sync mode: it steps as many frames as that time owes
/// and publishes, and this waits for it. Pipelined: its own clock steps
/// it, and this takes the newest state if one came. A state is what the
/// interface reads until the next (`seen`), with the snapshot the
/// drawing reads and the news. The slide is worked out for now: the
/// clock goes on between states. Under the loading page
/// (`menu::drape`) the clock does not start: the game is drawn, not
/// stepped.
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
pub(crate) fn tick(
    time: Res<Time>,
    speed: Res<Speedup>,
    (paused, settings, drape): (Res<ui::Paused>, Res<Settings>, Res<Drape>),
    mut thread: ResMut<SimThread>,
    (mut snapshot, mut reel): (ResMut<Snapshot>, ResMut<Reel>),
    (mut published, mut news): (ResMut<Frame>, ResMut<News>),
    interest: Res<Interest>,
) {
    #[allow(clippy::cast_precision_loss, reason = "50")]
    let frame = MS_PER_FRAME as f32;
    let start = if drape.is_up() {
        Start::Held
    } else {
        Start::Free
    };
    let pace = Pace {
        paused: paused.0,
        speed: speed.0,
        start,
    };
    let waited = crate::perf::start();
    let state = thread.frame(*interest, pace, time.delta_secs());

    crate::perf::stop(crate::perf::Timer::Wait, waited);

    thread.told();

    if let Some(mut state) = state {
        news.append(std::mem::take(&mut state.news));
        snapshot.steps = state.frame.taken.steps.clone();
        snapshot.slide = state.slide;

        // The art's clock: game time. Orders step frames in game frames only.
        reel.ms = state.frame.frames.wrapping_mul(MS_PER_FRAME);

        // The world drawn until now is freed on the sim thread.
        let spent = std::mem::replace(&mut *published, state.frame);

        thread.spend(spent.taken);
        timed(state.steps);
    }

    let owed = snapshot.slide.owed(std::time::Instant::now());

    snapshot.between = Motion::of(&settings).between(owed, frame);
}

/// With the perf timers on: the game's first drawn frame, and the
/// frame's steps counted.
fn timed(steps: u32) {
    use crate::perf::{self, Timer};

    if perf::on() {
        perf::startup("game");
        perf::note(Timer::Steps, f64::from(steps));
    }
}

/// Where each piece was a frame before, if it walked since: one placed
/// outright ([`Shown::jumps`]: a teleport, a door) draws at its new place
/// at once, as the engine draws it.
pub(crate) fn walked(was: &[Shown], now: &[Shown]) -> HashMap<u32, (i32, i32)> {
    let jumps: HashMap<u32, u32> = now.iter().map(|s| (s.agent, s.jumps)).collect();

    was.iter()
        .filter(|s| jumps.get(&s.agent) == Some(&s.jumps))
        .map(|s| (s.agent, s.at))
        .collect()
}
