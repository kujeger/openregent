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
//!            meshes and visible meshes counted; each frame the entities
//!            spawned and despawned, pieces and missiles drawn anew, its
//!            start-up work (regent_draw::settle; uploads) (maj_perf=debug:
//!            a line a frame)
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
//!  MAJ_PERF_HELD=<file> with MAJ_PERF: the art service's pictures written out (held)
//!  MAJ_PERF_PIPELINED=1 a screenshot's game in pipelined mode, as played
//!                       (`game::thread::Mode`): game frames by the wall clock
//! ```

mod gpu;
mod held;
mod hide;
mod repaint;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use bevy::log::{debug, info};
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

/// What a drawn frame counts of the entities' churn, noted at its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Churn {
    /// Entities with a `Transform` spawned.
    Spawned,
    /// Entities with a `Transform` despawned.
    Despawned,
    /// Pieces despawned and drawn anew as their key changed: a tinted
    /// one's (`map::pieces`).
    Respawns,
    /// Drawings given a new key's frames in their own entities: pieces,
    /// missiles and effects (`map::pieces::lay`).
    Rekeys,
    /// Missiles drawn the first time (`map::pieces::missiles`).
    Missiles,
    /// Pieces whose new key takes a body material the old did not:
    /// another sheet page, palette or pass (`map::pieces`).
    Swaps,
}

impl Churn {
    const ALL: [Churn; 6] = [
        Churn::Spawned,
        Churn::Despawned,
        Churn::Respawns,
        Churn::Rekeys,
        Churn::Missiles,
        Churn::Swaps,
    ];

    const fn name(self) -> &'static str {
        match self {
            Churn::Spawned => "spawned",
            Churn::Despawned => "despawned",
            Churn::Respawns => "respawns",
            Churn::Rekeys => "rekeys",
            Churn::Missiles => "missiles",
            Churn::Swaps => "swaps",
        }
    }
}

/// This frame's churn, in [`Churn::ALL`]'s order.
static CHURN: [AtomicU32; Churn::ALL.len()] = [const { AtomicU32::new(0) }; Churn::ALL.len()];

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

/// `n` added to this frame's `what`, if timing.
pub fn churn(what: Churn, n: u32) {
    if on() {
        CHURN[what as usize].fetch_add(n, Ordering::Relaxed);
    }
}

/// `f` run, timed as `timer`.
pub fn timed<T>(timer: Timer, f: impl FnOnce() -> T) -> T {
    let t = start();
    let out = f();

    stop(timer, t);
    out
}

/// What [`startup`] has said.
static SAID: Mutex<Vec<&str>> = Mutex::new(Vec::new());

/// One line, `what`'s ms since the process started: once per `what`.
pub fn startup(what: &'static str) {
    if on()
        && let (Some(t), Ok(mut said)) = (STARTED.get(), SAID.lock())
        && !said.contains(&what)
    {
        said.push(what);
        info!(target: TARGET, "startup {what} {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    }
}

/// The loading page's lift ([`lifted`]): ms up and drawn frames, for the
/// frame's line.
static LIFTED: Mutex<Option<(f64, u32)>> = Mutex::new(None);

/// A new game's loading page (`menu::drape`) gone after `ms` and
/// `frames` drawn frames: logged with the frame's number, as
/// `drape frame 61 ms 210 frames 13`.
pub fn lifted(ms: f64, frames: u32) {
    if on()
        && let Ok(mut lifted) = LIFTED.lock()
    {
        *lifted = Some((ms, frames));
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
    /// Drawn frames since the start, this one counted: a spike's number.
    total: u32,
    /// The run's part: its reports' name.
    part: Part,
}

/// A drawn run's part. A game's first frames compose the ground and read
/// art, sounds and pipelines for the first time: reported apart, so
/// `drawn` holds play alone.
///
/// ```text
///  menus, loading ──> the game's first frame + OPENING frames ──> play
///  └──────────────── "opening" ─────────────────────────────┘     "drawn"
/// ```
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Part {
    /// No game drawn yet.
    #[default]
    Loading,
    /// The game drawn; this many frames of its opening left.
    Opening(u32),
    Play,
}

/// The drawn frames after a game's first that its opening takes in: the
/// render thread draws that frame while the next ones wait.
const OPENING: u32 = 4;

/// Whether a game's first frame has drawn ([`startup`]'s `game`).
#[derive(Clone, Copy)]
enum Game {
    Unseen,
    Drawn,
}

impl Part {
    /// The part after one more frame.
    const fn after(self, game: Game) -> Part {
        match (self, game) {
            (Part::Loading, Game::Drawn) => Part::Opening(OPENING),
            (Part::Opening(0), _) => Part::Play,
            (Part::Opening(left), _) => Part::Opening(left - 1),
            (Part::Loading | Part::Play, _) => self,
        }
    }

    /// The name its reports go by.
    const fn name(self) -> &'static str {
        match self {
            Part::Loading | Part::Opening(_) => "opening",
            Part::Play => "drawn",
        }
    }
}

/// With the timers on, the main schedule and the drawn frames timed.
pub fn install(app: &mut App) {
    if on() {
        app.init_resource::<Drawn>()
            .init_resource::<GroundClock>()
            .init_resource::<Uploads>()
            .add_systems(First, began)
            .add_systems(
                Last,
                (uploads, ended)
                    .chain()
                    .after(regent_draw::settle::Noted)
                    .after(crate::menu::lift),
            )
            .add_observer(|_: On<Add<Transform>>| churn(Churn::Spawned, 1))
            .add_observer(|_: On<Despawn<Transform>>| churn(Churn::Despawned, 1));
        gpu::install(app);
        hide::install(app);
        held::install(app);
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

/// Bytes of images given to the render world this frame, new or
/// changed: what it uploads next.
#[derive(Resource, Default)]
struct Uploads(u64);

/// Bytes in a KiB.
const KIB: f64 = 1024.0;

/// This frame's new and changed images' bytes added to [`Uploads`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "a Bevy system takes its parameters by value"
)]
fn uploads(
    mut events: MessageReader<AssetEvent<Image>>,
    images: Res<Assets<Image>>,
    mut uploaded: ResMut<Uploads>,
) {
    use bevy::asset::RenderAssetUsages;

    let bytes: usize = events
        .read()
        .filter_map(|e| match e {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => images.get(*id),
            _ => None,
        })
        .filter(|i| i.asset_usage.contains(RenderAssetUsages::RENDER_WORLD))
        .filter_map(|i| i.data.as_ref().map(Vec::len))
        .sum();

    uploaded.0 += bytes as u64;
}

/// The frame's start-up work, noted as series and returned for its
/// lines: sprite pages put on, sprite page jobs waiting, pipelines
/// waiting, ground patches pending, KiB of images uploaded.
fn work(settle: Option<&regent_draw::settle::Settle>, uploads: &mut Uploads) -> [f64; 5] {
    let pending = settle
        .map(regent_draw::settle::Settle::pending)
        .unwrap_or_default();
    #[allow(clippy::cast_precision_loss, reason = "counts and bytes of a frame")]
    let work = [
        f64::from(pending.pages),
        pending.jobs as f64,
        pending.pipelines as f64,
        pending.patches as f64,
        std::mem::take(&mut uploads.0) as f64 / KIB,
    ];

    for (name, value) in WORK.iter().zip(work) {
        named(name, value);
    }

    work
}

/// [`work`]'s series, in its order.
const WORK: [&str; 5] = ["pages", "jobs", "compiling", "patches", "upload_kb"];

/// The frame's main schedule, interval and churn noted; each [`WINDOW`]
/// frames, the entity counts and a report, named by the run's [`Part`].
fn ended(
    mut drawn: ResMut<Drawn>,
    (entities, meshes): (Query<()>, Meshes),
    (settle, mut uploads): (Option<Res<regent_draw::settle::Settle>>, ResMut<Uploads>),
) {
    let now = Instant::now();
    let ms = |t: Instant| now.duration_since(t).as_secs_f64() * 1e3;

    startup("drawn");

    let main = drawn.began.map(ms);
    let interval = drawn.ended.map(ms);

    if let Some(main) = main {
        note(Timer::Main, main);
    }
    if let Some(interval) = interval {
        note(Timer::Interval, interval);
    }

    drawn.ended = Some(now);
    drawn.frames += 1;
    drawn.total += 1;

    // A frame's line, to set slow frames beside their churn:
    // `frame 812 main 4.1 interval 16.7 spawned 120 despawned 118 respawns 3 missiles 2`.
    let churned = Churn::ALL.map(|c| CHURN[c as usize].swap(0, Ordering::Relaxed));

    for (what, n) in Churn::ALL.iter().zip(churned) {
        named(what.name(), f64::from(n));
    }

    let [spawned, despawned, respawns, rekeys, missiles, swaps] = churned;
    let [pages, jobs, pipelines, patches, upload_kb] = work(settle.as_deref(), &mut uploads);

    debug!(
        target: TARGET,
        "frame {} main {:.3} interval {:.3} spawned {spawned} despawned {despawned} respawns {respawns} rekeys {rekeys} missiles {missiles} swaps {swaps} pages {pages} jobs {jobs} compiling {pipelines} patches {patches} upload_kb {upload_kb:.0}",
        drawn.total,
        main.unwrap_or(0.0),
        interval.unwrap_or(0.0)
    );

    // A spike's frame named, so runs can be compared frame by frame, with
    // the start-up work it did.
    if main.into_iter().chain(interval).any(|t| t > SPIKE) {
        info!(
            target: TARGET,
            "spike frame {} main {:.1} interval {:.1} wait {:.1} step {:.1} ground {:.1} pages {pages} jobs {jobs} compiling {pipelines} patches {patches} upload_kb {upload_kb:.0}",
            drawn.total,
            main.unwrap_or(0.0),
            interval.unwrap_or(0.0),
            latest(Timer::Wait),
            latest(Timer::Step),
            latest(Timer::Ground)
        );
    }

    if let Some((ms, frames)) = LIFTED.lock().ok().and_then(|mut l| l.take()) {
        info!(target: TARGET, "drape frame {} ms {ms:.0} frames {frames}", drawn.total);
    }

    let was = drawn.part;
    let game = if SAID.lock().is_ok_and(|said| said.contains(&"game")) {
        Game::Drawn
    } else {
        Game::Unseen
    };

    drawn.part = was.after(game);

    // A window's report, or the opening's as play begins.
    if drawn.frames == WINDOW || (was != Part::Play && drawn.part == Part::Play) {
        drawn.frames = 0;
        counted(&entities, &meshes);
        report(was.name());
    }
}

/// The named series that count things, not ms: [`counted`]'s, [`Churn`]'s,
/// `fog_moved`, 1 for a fog take whose explored ground changed, and
/// [`WORK`]'s.
const COUNTS: [&str; 13] = [
    "entities",
    "meshes",
    "visible",
    "fog_moved",
    "spawned",
    "despawned",
    "respawns",
    "missiles",
    "pages",
    "jobs",
    "compiling",
    "patches",
    "upload_kb",
];

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

/// `timer`'s last sample since the report, 0 for none: a drawn frame's
/// own in sync mode.
fn latest(timer: Timer) -> f64 {
    SERIES
        .lock()
        .ok()
        .and_then(|series| series[timer as usize].last().copied())
        .unwrap_or(0.0)
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

    /// The opening ends OPENING frames after a game's first; play stays.
    #[test]
    fn a_games_opening_ends_after_its_frames() {
        assert_eq!(Part::Loading.after(Game::Unseen).name(), "opening");

        let mut part = Part::Loading.after(Game::Drawn);

        for _ in 0..OPENING {
            assert_eq!(part.name(), "opening");
            part = part.after(Game::Drawn);
        }

        assert_eq!(part.after(Game::Drawn).name(), "drawn");
        assert_eq!(Part::Play.after(Game::Drawn).name(), "drawn");
    }
}
