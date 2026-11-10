//! A game run with no window (`--headless`), and its printed summary.

use std::time::Instant;

use bevy::log::warn;
use regent_core::data::text::Text;
use regent_core::gpl::runtime::MS_PER_FRAME;
use regent_core::sim::world::{defeat::End, draw_view::Doing, loss_log};
use regent_core::view::panel::day::Day;

use crate::game::sim::Sim;
use crate::{game::bot, game::drive, game::tune, map::effects, perf, ui};

/// A headless LAN game to `frames`, and what it measured: frames a
/// second, time waiting for peers, and the digests to compare.
pub(crate) fn lan_headless(mut sim: Sim, frames: u32, mut bot: Option<bot::Bot>) {
    let steps = frames.saturating_sub(sim.frames());
    let ran = drive::headless(&mut sim, steps, &mut bot, |_| ());
    let rate = f64::from(ran.frames) / ran.took.as_secs_f64().max(f64::EPSILON);

    println!(
        "net: player {}, {} frames in {:.1} s ({rate:.0} a second), {:.1} s waiting for peers",
        sim.peer().seat(),
        ran.frames,
        ran.took.as_secs_f32(),
        ran.waited.as_secs_f32()
    );
    println!(
        "net: frame {} digest {:016x}, deep {}",
        sim.frames(),
        sim.digest(),
        sim.deep_digest()
            .map_or_else(|e| e, |d| format!("{d:016x}"))
    );

    if let Some(why) = &sim.stopped {
        warn!("stopped at {why}");
    }

    crate::perf::report("lan");
}

/// After a headless step: a track's end in game time ([`tune`]).
fn tuned(sim: &mut Sim, tunes: &mut tune::Tunes) {
    let now = sim.world().now();

    if tunes.ended(sim.music(), now) {
        sim.music_stopped();
        tunes.next(sim.music(), now);
    }
}

/// With the perf timers on, every [`perf::PROBE_EVERY`] frames: a
/// world copy, a deep digest and the local palace's henchmen timed.
fn probed(sim: &mut Sim) {
    if perf::on() && sim.frames().is_multiple_of(perf::PROBE_EVERY) {
        drop(perf::timed(perf::Timer::Clone, || sim.world().clone()));
        drop(perf::timed(perf::Timer::Deep, || sim.deep_digest()));

        if let Some(palace) = sim.world().player_palace(sim.local()) {
            perf::timed(perf::Timer::Henchmen, || sim.henchmen(palace));
        }
    }
}

/// Step `frames` frames and print what happened; the game after.
pub(crate) fn headless(
    mut sim: Sim,
    frames: u32,
    text: &Text,
    (mut bot, mut tunes): (Option<bot::Bot>, tune::Tunes),
) -> Sim {
    let began = Instant::now();
    let mut acted = 0;
    let mut shots = std::collections::BTreeSet::new();
    let mut sparks = 0;
    let mut first_spark = None;
    let mut drawn = effects::Tally::default();
    let mut cues = 0;

    // What a drawing would show of combat: action frames, missiles.
    let each = |sim: &mut Sim| {
        tuned(sim, &mut tunes);
        probed(sim);

        cues += sim.take_output().cues.len();

        let world = sim.world();

        acted += world.framed();
        shots.extend(world.flown().iter().map(|f| (f.launched, f.code)));
        sparks = sparks.max(sim.motes().len());

        if first_spark.is_none() {
            first_spark = sim.motes().first().map(|m| (world.now(), m.code));
        }
        drawn.add(world);
    };

    drive::headless(&mut sim, frames, &mut bot, each);

    let world = sim.world();
    let shown = world.shown();
    let count = |d: Doing| shown.iter().filter(|s| s.doing == d).count();

    println!(
        "{} frames, {} ms of game, in {:.1} s",
        sim.frames(),
        sim.frames() * MS_PER_FRAME,
        began.elapsed().as_secs_f32()
    );
    println!("digest {:016x}", sim.digest());

    let day = Day::of(world.now());

    println!(
        "{}, {}% into the next",
        day.text(text)
            .unwrap_or_else(|| format!("day {}", day.number)),
        day.percent
    );
    println!(
        "treasury: {}",
        ui::gold_text(world.treasury(sim.local()), text)
    );
    println!(
        "{} pieces placed: {} standing, {} walking, {} waiting, {} acting",
        shown.len(),
        count(Doing::Standing),
        count(Doing::Walking),
        count(Doing::Waiting),
        count(Doing::Acting)
    );
    println!(
        "{acted} piece frames in an action's set; {} missiles flown; {} buildings fell, {} in ruins",
        shots.len(),
        world.collapses().len(),
        world.rubble().len()
    );
    println!(
        "particles: {} live in {} systems, at most {sparks}, the first {}",
        sim.motes().len(),
        world.plumes().iter().filter(|p| !p.spent).count(),
        first_spark.map_or_else(
            || "none".to_owned(),
            |(ms, code)| format!("{code} at {ms} ms")
        )
    );

    drawn.print();
    perf::report("headless");

    println!("{cues} sound cues; music track {:?}", sim.music());

    let tally = |what: &str, calls: &std::collections::BTreeMap<&'static str, usize>| {
        let total: usize = calls.values().sum();

        println!("{what}: {total} calls of {} builtins", calls.len());

        for (name, n) in calls {
            println!("  {n:>8}  {name}");
        }
    };

    let faults = sim.faults();
    tally("unknown (the world cannot tell here)", &faults.unknown);

    let loss_log::Counts { places, declined } = world.losses();

    println!("places lost: {places}; orders declined: {declined}");

    let reports = sim.reports();

    // Each kept one was said as it came, with its frame.
    println!("script errors: {}", reports.count);

    if let Some((records, name)) = sim.longest() {
        println!("longest run: {records} records, {name}");
    }

    if let Some(end) = world.ended(sim.local()) {
        println!("{}", ended(end, sim.frames()));
    }

    if let Some(why) = &sim.stopped {
        warn!("stopped at {why}");
    }

    if sim.replaying() {
        println!("the replay's events go on past the last frame run");
    }

    sim
}

/// Whether `sim` matched every recorded digest in `digests` (frames)
/// up to its last frame: a perf run that changed the game fails
/// (`tools/perf/`). E.g. digests at 100 and 200, stopped at 250: 200
/// must have matched.
pub(crate) fn checked(sim: &Sim, digests: &[u32]) -> Result<(), String> {
    let due = digests.iter().copied().filter(|&f| f <= sim.frames()).max();

    match (due, &sim.stopped) {
        (_, Some(why)) => Err(why.clone()),
        (Some(due), None) if sim.matched() != Some(due) => Err(format!(
            "replay: digests matched to frame {:?}, not {due}",
            sim.matched()
        )),
        _ => {
            if let Some(due) = due {
                println!("replay: every digest matched, to frame {due}");
            }
            Ok(())
        }
    }
}

/// The headless line for the crown's end at frame `frame`.
fn ended(end: End, frame: u32) -> String {
    let how = match end {
        End::Won => "won",
        End::Lost => "lost",
    };

    format!("{how}: the game stood still from frame {frame}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use crate::cli::tests::parsed;
    use crate::{game::begin, game::record};

    /// A headless game's tracks end in game time and the callback runs,
    /// as a drawn game's do in real time: the tracks go on, nothing is
    /// recorded, and the game is the one with no track's end.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_headless_tracks_end_changes_no_play() {
        const FRAMES: u32 = 200;

        // Short tracks: lines 3 to 5, early, mid and end game, a second.
        let tunes = || tune::Tunes::of(vec![None, None, Some(1_000), Some(1_000), Some(1_000)]);
        let dir = crate::testdata::scratch("headless_music");
        let path = dir.path().join("play.input");
        let cli = parsed(["openregent"]);

        let mut played = begin::test_setup(&cli).0.sim;
        let mut played_tunes = tunes();

        played.record(record::Recorder::open(
            &path,
            &dir.join("play.input.1"),
            &cli.recorded(),
        ));

        let mut asked = std::collections::BTreeSet::new();

        drive::headless(&mut played, FRAMES, &mut None, |sim| {
            tuned(sim, &mut played_tunes);
            asked.insert(sim.music());
        });

        let session = record::read(&path).expect("recorded");
        let mut quiet = begin::test_setup(&cli).0.sim;
        let mut silent = tune::Tunes::of(Vec::new());

        drive::headless(&mut quiet, FRAMES, &mut None, |sim| {
            tuned(sim, &mut silent);
        });

        assert!(
            session
                .events
                .iter()
                .all(|e| matches!(e, record::Event::Digest { .. })),
            "no input recorded"
        );
        assert_eq!(
            asked.into_iter().collect::<Vec<_>>(),
            [Some(3), Some(4), Some(5)],
            "the callback's tracks"
        );
        assert!(
            quiet.snapshot().expect("snapshot") == played.snapshot().expect("snapshot"),
            "the same game"
        );
    }

    /// A replay run headless passes the check while its digests match,
    /// and fails it once one differs.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_headless_replay_fails_on_a_differing_digest() {
        const FRAMES: u32 = 300;
        const TAMPERED: u32 = 200;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/input_logs/the_forsaken_land.input");
        let session = record::read(&path).expect("the recording");
        let digests = record::digested(&session.events);
        let tampered = session
            .events
            .iter()
            .map(|e| match *e {
                record::Event::Digest { after, digest } if after == TAMPERED => {
                    record::Event::Digest {
                        after,
                        digest: !digest,
                    }
                }
                e => e,
            })
            .collect();

        let run = |events| {
            let mut sim = begin::test_setup(&session.setup).0.sim;

            sim.replay(events);
            drive::headless(&mut sim, FRAMES, &mut None, |_| ());
            checked(&sim, &digests)
        };

        assert_eq!(run(session.events.clone()), Ok(()));
        assert!(run(tampered).is_err());
    }

    #[test]
    fn the_headless_line_says_won_or_lost() {
        assert_eq!(ended(End::Won, 7), "won: the game stood still from frame 7");
        assert_eq!(
            ended(End::Lost, 9),
            "lost: the game stood still from frame 9"
        );
    }
}
