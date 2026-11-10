//! The lockstep delay: fixed, or picked from the lobby's pings and
//! steered by the game's.
//!
//! ```text
//!   guest A ──a── host ──b── guest B      a, b: one way, half a round trip
//!   path A to B  = leg(A) + leg(B)        leg = a + JITTER_WEIGHT * jitter
//!   delay        = ceil((worst path + HEADROOM) / FRAME) + 1
//! ```
//!
//! The host is a leg of zero, so with one guest the worst path is its
//! leg alone.

use std::cmp::Ordering;
use std::time::{Duration, Instant};

use super::{DELAY, LEAST_DELAY, MOST_DELAY};

/// Round trips a guest's window keeps.
pub const SAMPLES: usize = 8;

/// Round trips a guest needs before its pings are trusted.
pub const LEAST_SAMPLES: usize = 3;

/// A frame of game time: what one frame of delay is worth.
const FRAME: Duration = Duration::from_millis(50);

/// Jitters added to a leg. **Inferred**: a chosen margin; the simulated
/// network's jitter is uniform, and its late packets stay within two
/// standard deviations of the mean.
const JITTER_WEIGHT: u32 = 2;

/// Time added to the worst path for the stepping thread's own pace.
/// **Measured**: a delay of `d` frames kept real time up to a one-way
/// path of `(d - 1)` frames less about 5 ms (see
/// `delay_absorbs_the_path_it_is_picked_for`); 10 ms is that gap, doubled.
const HEADROOM: Duration = Duration::from_millis(10);

/// The frames of delay beyond those that cover the path. **Measured**:
/// the first frame's orders are the peer's own; only `d - 1` cover
/// the wire.
const OWN_FRAME: u32 = 1;

/// Round trips of the newest a raise looks at: a rise in the path shows
/// in them before it moves the whole window.
const RAISE_SAMPLES: usize = LEAST_SAMPLES;

/// The least time between a change and a raise. **Inferred**: two ping
/// rounds (1 s each), so a raise sees pings taken since the last change.
pub const RAISE_GAP: Duration = Duration::from_secs(2);

/// How long the path must stay below the delay in force, with no change,
/// before the delay is lowered. A stall costs more than a few frames of
/// lag, so lowering is slow. **Inferred**: a chosen margin, over three
/// windows of samples.
pub const LOWER_HOLD: Duration = Duration::from_secs(30);

/// How the host sets the delay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delay {
    /// From the pings, as they stand.
    Auto,
    /// As told (`--net-delay`).
    Fixed(u32),
}

impl Delay {
    /// The delay for guests whose latest round trips are `guests`: the
    /// told one, else [`DELAY`] while any guest has too few.
    pub fn pick(self, guests: &[Vec<Duration>]) -> u32 {
        match self {
            Delay::Fixed(frames) => frames,
            Delay::Auto if enough(guests) => frames(worst(guests)),
            Delay::Auto => DELAY,
        }
    }

    /// `frames` as the lobby shows them: "3 frames, 150 ms".
    pub fn shown(frames: u32) -> String {
        let ms = u128::from(frames) * FRAME.as_millis();

        format!("{frames} frames, {ms} ms")
    }

    /// Whether `guests` have the samples [`Delay::pick`] needs.
    pub fn sampled(self, guests: &[Vec<Duration>]) -> bool {
        self != Delay::Auto || enough(guests)
    }
}

/// The host's decisions during play: when the delay in force is changed
/// for the path the pings show.
///
/// ```text
///  path above the delay ──> raise to the pick, once RAISE_GAP past the last change
///  path below it, all LOWER_HOLD ──> lower to the highest pick seen meanwhile
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Steer {
    mode: Delay,
    /// The last change, or the start.
    changed: Instant,
    /// Since when the path has stood below the delay, and the highest
    /// pick since.
    below: Option<(Instant, u32)>,
}

impl Steer {
    pub const fn new(mode: Delay, now: Instant) -> Steer {
        Steer {
            mode,
            changed: now,
            below: None,
        }
    }

    /// The delay to change to, if `guests`' round trips at `now` call
    /// for it while `current` is in force; the change counts as made.
    pub fn decide(&mut self, now: Instant, current: u32, guests: &[Vec<Duration>]) -> Option<u32> {
        let ready = self.mode == Delay::Auto && enough(guests);
        let up = if ready { self.pick(guests) } else { current };

        match up.cmp(&current) {
            Ordering::Greater => {
                self.below = None;

                // The newest round trip must call for it too: a window that
                // holds a fall (300 ms, then 20) has a wide spread, and its
                // pick would rise as the path falls (Measured).
                let rising = self.latest(guests) > current;
                let gap = now.saturating_duration_since(self.changed) >= RAISE_GAP;

                (rising && gap).then(|| self.made(now, up))
            }
            Ordering::Less => {
                let (since, most) = self.below.get_or_insert((now, up));

                *most = (*most).max(up);

                let (since, most) = (*since, *most);
                let held = now.saturating_duration_since(since.max(self.changed));

                (held >= LOWER_HOLD).then(|| self.made(now, most))
            }
            Ordering::Equal => {
                self.below = None;
                None
            }
        }
    }

    /// The pick over the whole windows and over their newest samples:
    /// the higher.
    fn pick(&self, guests: &[Vec<Duration>]) -> u32 {
        let newest: Vec<Vec<Duration>> = guests
            .iter()
            .map(|g| g[g.len().saturating_sub(RAISE_SAMPLES)..].to_vec())
            .collect();

        self.mode.pick(guests).max(self.mode.pick(&newest))
    }

    /// The pick if each guest's newest round trip were steady.
    fn latest(&self, guests: &[Vec<Duration>]) -> u32 {
        let steady: Vec<Vec<Duration>> = guests
            .iter()
            .map(|g| vec![g.last().copied().unwrap_or_default(); LEAST_SAMPLES])
            .collect();

        self.mode.pick(&steady)
    }

    const fn made(&mut self, now: Instant, delay: u32) -> u32 {
        self.changed = now;
        self.below = None;
        delay
    }
}

/// Whether there is a guest, and every one has [`LEAST_SAMPLES`].
fn enough(guests: &[Vec<Duration>]) -> bool {
    !guests.is_empty() && guests.iter().all(|g| g.len() >= LEAST_SAMPLES)
}

/// A guest's leg: half its mean round trip, and [`JITTER_WEIGHT`]
/// standard deviations of it.
fn leg(trips: &[Duration]) -> Duration {
    let count = f64::from(u32::try_from(trips.len()).unwrap_or(u32::MAX)).max(1.0);
    let secs = trips.iter().map(Duration::as_secs_f64);
    let mean = secs.clone().sum::<f64>() / count;
    let var = secs.map(|s| (s - mean).powi(2)).sum::<f64>() / count;

    Duration::from_secs_f64(mean / 2.0 + f64::from(JITTER_WEIGHT) * var.sqrt())
}

/// The slowest path between two peers: the two longest legs added.
fn worst(guests: &[Vec<Duration>]) -> Duration {
    let mut legs: Vec<Duration> = guests.iter().map(|g| leg(g)).collect();

    legs.sort_unstable_by(|a, b| b.cmp(a));
    legs.iter().take(2).sum()
}

/// The delay that covers a path of `path`, within the limits.
fn frames(path: Duration) -> u32 {
    let covered = (path + HEADROOM).as_millis().div_ceil(FRAME.as_millis());

    u32::try_from(covered)
        .unwrap_or(MOST_DELAY)
        .saturating_add(OWN_FRAME)
        .clamp(LEAST_DELAY, MOST_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` round trips of `ms` each.
    fn steady(ms: u64, n: usize) -> Vec<Duration> {
        vec![Duration::from_millis(ms); n]
    }

    /// With no jitter, a path of one way `ms` needs its frames plus one.
    #[test]
    fn a_steady_guest_is_picked_by_its_half_round_trip() {
        // 100 ms round trip: 50 ms + 10 ms headroom = 2 frames, + 1.
        assert_eq!(Delay::Auto.pick(&[steady(100, 5)]), 3);
        // 20 ms: 10 + 10 = 1 frame, + 1.
        assert_eq!(Delay::Auto.pick(&[steady(20, 5)]), 2);
        // 180 ms: 90 + 10 = 2 frames; 190 ms: 105 = 3 frames.
        assert_eq!(Delay::Auto.pick(&[steady(180, 5)]), 3);
        assert_eq!(Delay::Auto.pick(&[steady(190, 5)]), 4);
    }

    /// Two guests talk through the host: their legs add.
    #[test]
    fn two_guests_add_their_legs_through_the_host() {
        // 2 x 60 ms one way = 120 + 10 = 3 frames, + 1.
        let guests = [steady(120, 4), steady(120, 4)];

        assert_eq!(Delay::Auto.pick(&guests), 4);
        // A slow and a fast guest: the two longest legs only.
        let three = [steady(120, 4), steady(40, 4), steady(100, 4)];

        // 60 + 50 = 110 + 10 = 3 frames, + 1.
        assert_eq!(Delay::Auto.pick(&three), 4);
    }

    /// Jitter widens the leg.
    #[test]
    fn jitter_adds_a_margin() {
        let jittery: Vec<Duration> = [60, 100, 60, 100, 60, 100]
            .map(Duration::from_millis)
            .to_vec();

        // Mean 80, deviation 20: leg 40 + 2 x 20 = 80, + 10 = 2 frames.
        assert_eq!(Delay::Auto.pick(&[jittery]), 3);
        assert!(Delay::Auto.pick(&[steady(80, 6)]) < 3);
    }

    /// The delay stays within the limits.
    #[test]
    fn the_pick_is_clamped() {
        assert_eq!(Delay::Auto.pick(&[steady(0, 4)]), LEAST_DELAY);
        assert_eq!(Delay::Auto.pick(&[steady(60_000, 4)]), MOST_DELAY);
    }

    /// Too few samples, or no guest: the default.
    #[test]
    fn too_few_samples_fall_back_to_the_default() {
        assert_eq!(Delay::Auto.pick(&[]), DELAY);
        assert_eq!(Delay::Auto.pick(&[steady(400, LEAST_SAMPLES - 1)]), DELAY);
        // One guest short holds the other back.
        assert_eq!(
            Delay::Auto.pick(&[steady(400, 5), steady(400, LEAST_SAMPLES - 1)]),
            DELAY
        );
        assert!(!Delay::Auto.sampled(&[steady(400, 1)]));
        assert!(Delay::Auto.sampled(&[steady(400, LEAST_SAMPLES)]));
    }

    /// The lobby's words for a delay.
    #[test]
    fn a_delay_is_shown_in_frames_and_milliseconds() {
        assert_eq!(Delay::shown(3), "3 frames, 150 ms");
    }

    /// A told delay is used as is, samples or not.
    #[test]
    fn a_told_delay_is_used_as_is() {
        assert_eq!(Delay::Fixed(7).pick(&[]), 7);
        assert_eq!(Delay::Fixed(7).pick(&[steady(400, 5)]), 7);
        assert!(Delay::Fixed(7).sampled(&[]));
    }
}

#[cfg(test)]
mod steer_tests {
    use super::*;

    fn steady(ms: u64) -> Vec<Vec<Duration>> {
        vec![vec![Duration::from_millis(ms); SAMPLES]]
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    /// A path above the delay raises it, but not before the gap.
    #[test]
    fn a_rise_raises_after_the_gap_since_the_last_change() {
        let start = Instant::now();
        let mut steer = Steer::new(Delay::Auto, start);

        // 300 ms round trip: 150 + 10 = 4 frames, + 1.
        assert_eq!(steer.decide(start + RAISE_GAP / 2, 3, &steady(300)), None);
        assert_eq!(steer.decide(start + RAISE_GAP, 3, &steady(300)), Some(5));
        // The raise counts as a change: the next waits too.
        assert_eq!(steer.decide(start + RAISE_GAP, 3, &steady(600)), None);
        assert_eq!(
            steer.decide(start + RAISE_GAP * 2, 5, &steady(600)),
            Some(8)
        );
    }

    /// A rise shown by the newest samples raises it.
    #[test]
    fn a_rise_in_the_newest_samples_raises() {
        let start = Instant::now();
        let mut steer = Steer::new(Delay::Auto, start);
        let mut trips = vec![Duration::from_millis(40); SAMPLES - RAISE_SAMPLES];

        trips.extend([Duration::from_millis(800); RAISE_SAMPLES]);

        assert!(
            steer
                .decide(start + RAISE_GAP, 3, &[trips])
                .is_some_and(|d| d > 3)
        );
    }

    /// A lower path lowers it only after it held for the whole time, to
    /// the highest pick met meanwhile; a rise in between starts it over.
    #[test]
    fn a_fall_lowers_only_after_the_hold_to_the_highest_pick() {
        let start = Instant::now();
        let mut steer = Steer::new(Delay::Auto, start);
        // 100 ms: 3; 180 ms: 3 (see `pick`); 190 ms: 4.
        let at = |n| start + secs(n);

        assert_eq!(steer.decide(at(10), 8, &steady(100)), None);
        assert_eq!(steer.decide(at(20), 8, &steady(190)), None);
        assert_eq!(
            steer.decide(at(10) + LOWER_HOLD.saturating_sub(secs(1)), 8, &steady(100)),
            None
        );
        assert_eq!(steer.decide(at(10) + LOWER_HOLD, 8, &steady(100)), Some(4));

        // Just changed: a new fall waits a hold again.
        assert_eq!(steer.decide(at(41), 4, &steady(20)), None);
        assert_eq!(steer.decide(at(41) + LOWER_HOLD, 4, &steady(20)), Some(2));

        // A pick at the delay in force breaks the hold.
        let mut steer = Steer::new(Delay::Auto, start);

        assert_eq!(steer.decide(at(0), 4, &steady(20)), None);
        assert_eq!(steer.decide(at(20), 4, &steady(190)), None);
        assert_eq!(steer.decide(at(40), 4, &steady(20)), None);
        assert_eq!(
            steer.decide(at(40) + LOWER_HOLD.saturating_sub(secs(1)), 4, &steady(20)),
            None
        );
    }

    /// A path that falls does not raise the delay while the window holds
    /// both: the newest round trip is low.
    #[test]
    fn a_falling_path_is_no_reason_to_raise() {
        let start = Instant::now();
        let mut steer = Steer::new(Delay::Auto, start);
        let mut trips = vec![Duration::from_millis(300); SAMPLES - 1];

        trips.push(Duration::from_millis(20));

        assert_eq!(steer.decide(start + RAISE_GAP, 5, &[trips]), None);
    }

    /// Never past the limits.
    #[test]
    fn a_change_stays_within_the_limits() {
        let start = Instant::now();
        let mut steer = Steer::new(Delay::Auto, start);

        assert_eq!(
            steer.decide(start + RAISE_GAP, 3, &steady(60_000)),
            Some(MOST_DELAY)
        );
        assert_eq!(
            steer.decide(start + RAISE_GAP * 2, MOST_DELAY, &steady(60_000)),
            None
        );
        assert_eq!(
            steer.decide(start + RAISE_GAP * 2, LEAST_DELAY, &steady(0)),
            None
        );
    }

    /// A told delay, too few samples, or no guest: no change.
    #[test]
    fn a_fixed_delay_or_too_few_samples_never_change() {
        let start = Instant::now();
        let later = start + LOWER_HOLD * 3;
        let mut fixed = Steer::new(Delay::Fixed(3), start);
        let mut auto = Steer::new(Delay::Auto, start);

        assert_eq!(fixed.decide(later, 3, &steady(900)), None);
        assert_eq!(fixed.decide(later, 9, &steady(20)), None);
        assert_eq!(auto.decide(later, 3, &[]), None);
        assert_eq!(auto.decide(later, 3, &[steady(900)[0][..2].to_vec()]), None);
    }
}
