//! Opt-in timers for the perf runs (`tools/perf/`). Off unless
//! [`ENV`] is set: then each timed spot adds a sample to its series,
//! and a report logs every series at target [`TARGET`], info, and
//! empties them. Off, a spot costs one load of a cached flag.
//!
//! ```text
//!  MAJ_PERF=1 RUST_LOG=warn,maj_perf=info openregent ...
//!  drawn     every WINDOW drawn frames: main, interval, wait; on the sim
//!            thread step, steps, publish, asked, clone (each world copy);
//!            LAN: snapshot (sim thread), fold (its worker); entities,
//!            meshes and visible meshes counted
//!  headless  at the end: step, and every PROBE_EVERY frames clone, deep, henchmen
//!  startup   once each: ms from start to the first drawn frame, and to the game's
//! ```
//!
//! Drawn runs take two more switches, read once:
//!
//! ```text
//!  MAJ_PERF_UNCAPPED=1  no vsync: the frame interval is the frame's cost, not 16.7 ms
//!  MAJ_PERF_GPU=1       with MAJ_PERF: GPU time per pass and per frame, draw calls,
//!                       GPU memory (gpu)
//!  MAJ_PERF_HIDE=a,b    with MAJ_PERF: classes of world meshes hidden (hide)
//!  MAJ_PERF_PIPELINED=1 a screenshot's game in pipelined mode, as played
//!                       (`game::thread::Mode`): game frames by the wall clock
//! ```

mod gpu;
mod hide;

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use bevy::log::info;
use bevy::prelude::*;
use bevy::window::PresentMode;

use crate::draw::shot::Capture;

/// The environment variable that turns the timers on, e.g. `MAJ_PERF=1`.
pub const ENV: &str = "MAJ_PERF";

/// The environment variable that turns vsync off, e.g. `MAJ_PERF_UNCAPPED=1`.
pub const UNCAPPED: &str = "MAJ_PERF_UNCAPPED";

/// The environment variable that runs a screenshot's game pipelined,
/// e.g. `MAJ_PERF_PIPELINED=1`.
pub const PIPELINED: &str = "MAJ_PERF_PIPELINED";

/// The log target the reports go to: `RUST_LOG=warn,maj_perf=info`
/// shows them.
pub const TARGET: &str = "maj_perf";

/// Drawn frames a drawn report covers.
pub const WINDOW: u32 = 600;

/// A sample past this many ms is a spike: a frame and a half at 60 Hz,
/// what a pan shows as a stutter. Each report counts them (`over`).
pub const SPIKE: f64 = 25.0;

/// Headless frames between probes of the copies and digests.
pub const PROBE_EVERY: u32 = 500;

/// What a sample times, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// The main schedule, `First` to `Last`.
    Main,
    /// Wall time between drawn frames.
    Interval,
    /// The steps of one drawn frame, or of one headless frame.
    Step,
    /// A drawn frame's wait for the sim thread's state (`game::clock::tick`).
    Wait,
    /// Game frames stepped in one drawn frame (a count, not ms).
    Steps,
    /// The read model published after the steps, on the sim thread.
    Publish,
    /// One panel query run aside (`seen::run`): the henchmen's with a
    /// palace selected.
    Asked,
    /// One `World` clone: the sim thread's copy for the drawing.
    Clone,
    /// One fog take on the sim thread (`fog::Seen::after`).
    Fog,
    /// One `Game::deep_digest`.
    Deep,
    /// One LAN deep digest's snapshot, on the stepping thread.
    Snapshot,
    /// One LAN deep digest's fold, on its worker (`net::fold`).
    Fold,
    /// One `Sim::henchmen` on the local palace (headless probe).
    Henchmen,
    /// A drawn frame's ground: `map::redraw` and `map::redraw_inset`,
    /// most frames nothing, a recompose when the view leaves the sheet.
    Ground,
}

impl Timer {
    const ALL: [Timer; 14] = [
        Timer::Main,
        Timer::Interval,
        Timer::Step,
        Timer::Wait,
        Timer::Steps,
        Timer::Publish,
        Timer::Asked,
        Timer::Clone,
        Timer::Fog,
        Timer::Deep,
        Timer::Snapshot,
        Timer::Fold,
        Timer::Henchmen,
        Timer::Ground,
    ];

    const fn name(self) -> &'static str {
        match self {
            Timer::Main => "main",
            Timer::Interval => "interval",
            Timer::Step => "step",
            Timer::Wait => "wait",
            Timer::Steps => "steps",
            Timer::Publish => "publish",
            Timer::Asked => "asked",
            Timer::Clone => "clone",
            Timer::Fog => "fog",
            Timer::Deep => "deep",
            Timer::Snapshot => "snapshot",
            Timer::Fold => "fold",
            Timer::Henchmen => "henchmen",
            Timer::Ground => "ground",
        }
    }
}

/// Each timer's samples since the last report, in [`Timer::ALL`]'s order.
static SERIES: Mutex<[Vec<f64>; Timer::ALL.len()]> =
    Mutex::new([const { Vec::new() }; Timer::ALL.len()]);

/// Series named at run time (GPU passes, draw counts), as [`SERIES`].
static NAMED: Mutex<BTreeMap<String, Vec<f64>>> = Mutex::new(BTreeMap::new());

/// When the process started: set by [`init`].
static STARTED: OnceLock<Instant> = OnceLock::new();

/// Whether [`ENV`] is set, read once.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();

    *ON.get_or_init(|| std::env::var_os(ENV).is_some())
}

/// The window's present mode: without vsync when [`UNCAPPED`] is set,
/// else the default.
pub fn present_mode() -> PresentMode {
    if std::env::var_os(UNCAPPED).is_some() {
        PresentMode::AutoNoVsync
    } else {
        Window::default().present_mode
    }
}

/// The sim thread's mode for a game: pipelined, as played, but for a
/// screenshot's, unless [`PIPELINED`] is set.
pub fn mode(run: Capture) -> crate::game::thread::Mode {
    use crate::game::thread::Mode;

    if run == Capture::Shot && std::env::var_os(PIPELINED).is_none() {
        Mode::Sync
    } else {
        Mode::Pipelined
    }
}

/// The process's start, for [`startup`].
pub fn init() {
    STARTED.get_or_init(Instant::now);
}

/// A timer's start: `None` when off.
pub fn start() -> Option<Instant> {
    on().then(Instant::now)
}

/// The ms since `start` added to `timer`'s samples, if timing.
pub fn stop(timer: Timer, start: Option<Instant>) {
    if let Some(t) = start {
        note(timer, t.elapsed().as_secs_f64() * 1e3);
    }
}

/// `value` added to `timer`'s samples, if timing.
pub fn note(timer: Timer, value: f64) {
    if on()
        && let Ok(mut series) = SERIES.lock()
    {
        series[timer as usize].push(value);
    }
}

/// `value` added to the series `name`, if timing. Names are one word
/// each, as the summary reads them: `gpu_frame`, `draws`.
pub fn named(name: &str, value: f64) {
    if on()
        && let Ok(mut named) = NAMED.lock()
    {
        named.entry(name.to_owned()).or_default().push(value);
    }
}

/// `f` run, timed as `timer`.
pub fn timed<T>(timer: Timer, f: impl FnOnce() -> T) -> T {
    let t = start();
    let out = f();

    stop(timer, t);
    out
}

/// One line, `what`'s ms since the process started: once per `what`.
pub fn startup(what: &'static str) {
    static SAID: Mutex<Vec<&str>> = Mutex::new(Vec::new());

    if on()
        && let (Some(t), Ok(mut said)) = (STARTED.get(), SAID.lock())
        && !said.contains(&what)
    {
        said.push(what);
        info!(target: TARGET, "startup {what} {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    }
}

/// Each non-empty series logged, `what` the run's part, and emptied:
///
/// ```text
///  drawn step n 600 mean 2.026 p50 1.9 p99 13.136 max 26.985 over 1
/// ```
///
/// `over`: the samples past [`SPIKE`].
pub fn report(what: &str) {
    let Ok(mut series) = SERIES.lock() else {
        return;
    };

    for (timer, samples) in Timer::ALL.iter().zip(series.iter_mut()) {
        if let Some((mean, p50, p99, max)) = stats(samples) {
            info!(
                target: TARGET,
                "{what} {} n {} mean {mean:.3} p50 {p50:.3} p99 {p99:.3} max {max:.3} over {}",
                timer.name(),
                samples.len(),
                spikes(samples)
            );
        }
        samples.clear();
    }
    drop(series);

    if let Ok(mut named) = NAMED.lock() {
        for (name, samples) in named.iter_mut() {
            // A count has no spikes.
            let over = if COUNTS.contains(&name.as_str()) {
                String::new()
            } else {
                format!(" over {}", spikes(samples))
            };

            if let Some((mean, p50, p99, max)) = stats(samples) {
                info!(
                    target: TARGET,
                    "{what} {name} n {} mean {mean:.3} p50 {p50:.3} p99 {p99:.3} max {max:.3}{over}",
                    samples.len(),
                );
            }
            samples.clear();
        }
    }
}

/// The drawn frame's clock.
#[derive(Resource, Default)]
struct Drawn {
    /// This frame's `First`.
    began: Option<Instant>,
    /// The last frame's `Last`.
    ended: Option<Instant>,
    /// Drawn frames since the last report.
    frames: u32,
}

/// With the timers on, the main schedule and the drawn frames timed.
pub fn install(app: &mut App) {
    if on() {
        app.init_resource::<Drawn>()
            .init_resource::<GroundClock>()
            .add_systems(First, began)
            .add_systems(Last, ended);
        gpu::install(app);
        hide::install(app);
    }
}

/// This frame's ground time: each [`ground_began`] to [`ground_ended`]
/// span summed, noted by [`ground_noted`].
#[derive(Resource, Default)]
pub struct GroundClock {
    began: Option<Instant>,
    spent: Option<f64>,
}

/// A ground span started: a system before `map::redraw` or
/// `map::redraw_inset`.
pub fn ground_began(clock: Option<ResMut<GroundClock>>) {
    if let Some(mut clock) = clock {
        clock.began = Some(Instant::now());
    }
}

/// A ground span added: a system after `map::redraw` or
/// `map::redraw_inset`.
pub fn ground_ended(clock: Option<ResMut<GroundClock>>) {
    if let Some(mut clock) = clock
        && let Some(t) = clock.began.take()
    {
        let ms = t.elapsed().as_secs_f64() * 1e3;

        clock.spent = Some(clock.spent.unwrap_or(0.0) + ms);
    }
}

/// The frame's ground time noted: a system after the last span.
pub fn ground_noted(clock: Option<ResMut<GroundClock>>) {
    if let Some(mut clock) = clock
        && let Some(ms) = clock.spent.take()
    {
        note(Timer::Ground, ms);
    }
}

fn began(mut drawn: ResMut<Drawn>) {
    drawn.began = Some(Instant::now());
}

/// The frame's main schedule and interval noted; each [`WINDOW`]
/// frames, the entity counts and a report.
fn ended(mut drawn: ResMut<Drawn>, entities: Query<()>, meshes: Meshes) {
    let now = Instant::now();
    let ms = |t: Instant| now.duration_since(t).as_secs_f64() * 1e3;

    startup("drawn");

    if let Some(t) = drawn.began {
        note(Timer::Main, ms(t));
    }
    if let Some(t) = drawn.ended {
        note(Timer::Interval, ms(t));
    }

    drawn.ended = Some(now);
    drawn.frames += 1;

    if drawn.frames == WINDOW {
        drawn.frames = 0;
        counted(&entities, &meshes);
        report("drawn");
    }
}

/// The named series that count things, not ms: [`counted`]'s, and
/// `fog_moved`, 1 for a fog take whose explored ground changed.
const COUNTS: [&str; 4] = ["entities", "meshes", "visible", "fog_moved"];

/// Every mesh, and whether a camera drew it last frame.
type Meshes<'w, 's> = Query<'w, 's, &'static ViewVisibility, With<Mesh3d>>;

/// The world's size, one sample each: entities, meshes, and the meshes
/// a camera drew. Bevy's visibility passes visit every entity, drawn or
/// not.
#[allow(clippy::cast_precision_loss, reason = "a few hundred thousand")]
fn counted(entities: &Query<()>, meshes: &Meshes) {
    let visible = meshes.iter().filter(|v| v.get()).count();

    named("entities", entities.iter().count() as f64);
    named("meshes", meshes.iter().count() as f64);
    named("visible", visible as f64);
}

/// Mean, median, 99th percentile and maximum; `None` for no samples.
/// The median shows the typical sample: one hitch moves the mean.
fn stats(samples: &mut [f64]) -> Option<(f64, f64, f64, f64)> {
    samples.sort_by(f64::total_cmp);

    let max = *samples.last()?;
    #[allow(clippy::cast_precision_loss, reason = "a few thousand samples")]
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let p99 = samples[(samples.len() - 1) * 99 / 100];

    // Even counts: the middle two averaged.
    let low = samples[(samples.len() - 1) / 2];
    let high = samples[samples.len() / 2];
    let p50 = f64::midpoint(low, high);

    Some((mean, p50, p99, max))
}

/// How many samples are past [`SPIKE`].
fn spikes(samples: &[f64]) -> usize {
    samples.iter().filter(|s| **s > SPIKE).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spike_is_a_sample_past_25_ms() {
        assert_eq!(spikes(&[1.0, 25.0, 25.5, 80.0]), 2);
    }

    #[test]
    fn stats_are_mean_median_p99_and_max() {
        let mut samples: Vec<f64> = (1..=100).rev().map(f64::from).collect();

        assert_eq!(stats(&mut samples), Some((50.5, 50.5, 99.0, 100.0)));
        assert_eq!(stats(&mut [3.0, 1.0, 2.0]), Some((2.0, 2.0, 2.0, 3.0)));
        assert_eq!(stats(&mut []), None);
    }
}
