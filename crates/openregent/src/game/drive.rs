//! The one loop that steps the game, pumps the network and keeps the
//! game clock: the sim thread's, in either mode, and every headless
//! run's, alone or on the LAN.
//!
//! ```text
//!  drive:  wait (at most the nap: a command, the next step due, a peer's
//!          bundle) ─> pump ─> steps ─> turned ─> again, until one says Quit
//!  steps:  while the clock owes a frame, at most MOST_STEPS, and every
//!          seat's orders for it are in: before ─> Sim::step ─> after ─> pump
//! ```
//!
//! The clock's [`Pacing`] is where the runs differ in time:
//!
//! ```text
//!  Drawn    sync mode: each drawn frame's seconds owed
//!  Own      pipelined mode: the thread's own Instants owed
//!  FlatOut  headless: a number of steps, as fast as every peer allows
//! ```
//!
//! The hooks ([`Drive`]) are where they differ in what they do: the sim
//! thread takes commands and publishes; a headless run gives the bot's
//! orders and tallies.

use std::time::{Duration, Instant};

use bevy::log::info;

use regent_core::gpl::runtime::MS_PER_FRAME;

use crate::game::bot::Bot;
use crate::game::sim::Sim;
use crate::perf::{self, Timer};

/// Game frames at most per turn: a slow machine falls behind rather
/// than stalls.
const MOST_STEPS: u8 = 10;

/// One game frame, ms.
#[allow(clippy::cast_precision_loss, reason = "50")]
const FRAME: f32 = MS_PER_FRAME as f32;

/// Paced, with peers: the longest between pumps. A bundle waited for is
/// taken this soon; the peer's own resends are 25 ms apart.
const PUMP: Duration = Duration::from_millis(2);

/// Paced, alone and with nothing owed (paused, stopped): the longest
/// nap. A command wakes the sim thread sooner.
const IDLE: Duration = Duration::from_millis(250);

/// Flat out: the nap while a bundle is waited for, or a finished run's
/// peers settle.
const NAP: Duration = Duration::from_micros(500);

/// A finished headless run waits at most this long for its peers to
/// have all it sends.
const LINGER: Duration = Duration::from_secs(5);

/// Whether the loop goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    On,
    Quit,
}

impl Flow {
    /// `next`'s flow, run only while on.
    fn and(self, next: impl FnOnce() -> Flow) -> Flow {
        match self {
            Flow::On => next(),
            Flow::Quit => Flow::Quit,
        }
    }
}

/// Whether the game's clock runs: a game the VM stopped owes no more
/// time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Run {
    #[default]
    Going,
    Stopped,
}

/// Where the clock's time comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pacing {
    /// Each drawn frame's seconds, ms not yet owed.
    Drawn { pending: f32 },
    /// The real time since the clock last owed.
    Own,
    /// Steps left, owed at once.
    FlatOut { left: u32 },
}

/// Game time owed, ms, while the game runs.
pub struct Clock {
    pub owed: f32,
    run: Run,
    /// Paced: when time was last owed; `None` before the first
    /// hand-over, so a game made but not yet drawn waits.
    pub last: Option<Instant>,
    pacing: Pacing,
}

impl Clock {
    pub const fn new(pacing: Pacing) -> Clock {
        Clock {
            owed: 0.0,
            run: Run::Going,
            last: None,
            pacing,
        }
    }

    /// Times the engine's speed the game runs at now, at `speed`.
    const fn at(&self, speed: f32) -> f32 {
        match self.run {
            Run::Going => speed,
            Run::Stopped => 0.0,
        }
    }

    /// Game ms owed a real ms now: paused while any peer is, at the
    /// host's speed; alone, this machine's pause and speed. None before
    /// the first drawn frame.
    pub fn rate(&self, sim: &Sim) -> f32 {
        let peer = sim.peer();

        if peer.paused() || self.last.is_none() {
            0.0
        } else {
            self.at(peer.speed())
        }
    }

    /// The game stopped: no more time is owed.
    pub const fn stop(&mut self) {
        self.run = Run::Stopped;
    }

    /// Whether the game stopped.
    pub fn stopped(&self) -> bool {
        self.run == Run::Stopped
    }

    /// The clock's start, at the first hand-over.
    pub fn start(&mut self) {
        self.last.get_or_insert_with(Instant::now);
    }

    /// A drawn frame's `seconds`, owed at the next steps (sync mode's).
    pub fn drawn(&mut self, seconds: f32) {
        if let Pacing::Drawn { pending } = &mut self.pacing {
            *pending += seconds * 1000.0;
        }
    }

    /// `ms` of real time passed: game time owed at the `rate`, up to the
    /// catch-up's most.
    fn owe(&mut self, ms: f32, rate: f32) {
        self.owed = (self.owed + ms * rate).min(FRAME * f32::from(MOST_STEPS));
    }

    /// The time passed owed, as the pacing has it, at `rate`.
    fn tick(&mut self, rate: f32) {
        match &mut self.pacing {
            Pacing::Drawn { pending } => {
                let ms = std::mem::take(pending);

                self.owe(ms, rate);
            }
            Pacing::Own => {
                if let Some(last) = self.last {
                    let now = Instant::now();

                    self.last = Some(now);
                    self.owe(now.duration_since(last).as_secs_f32() * 1000.0, rate);
                }
            }
            Pacing::FlatOut { .. } => {}
        }
    }

    /// Whether a frame is owed.
    fn due(&self) -> bool {
        match self.pacing {
            Pacing::Drawn { .. } | Pacing::Own => self.owed >= FRAME,
            Pacing::FlatOut { left } => left > 0,
        }
    }

    /// A frame stepped: owed no more.
    fn spent(&mut self) {
        match &mut self.pacing {
            Pacing::Drawn { .. } | Pacing::Own => self.owed -= FRAME,
            Pacing::FlatOut { left } => *left -= 1,
        }
    }

    /// How long to wait before the next step is due, or the next pump
    /// with peers.
    fn nap(&self, sim: &Sim) -> Duration {
        let ready = sim.ready();
        let alone = sim.peer().players() == 1;

        match self.pacing {
            Pacing::FlatOut { left } if left > 0 && ready => Duration::ZERO,
            Pacing::FlatOut { .. } => NAP,
            Pacing::Drawn { .. } | Pacing::Own => {
                let rate = self.rate(sim);
                let due = (rate > 0.0 && ready).then(|| {
                    let ms = (FRAME - self.owed).max(0.0) / rate;

                    Duration::from_secs_f32(ms / 1000.0)
                });

                match (due, alone) {
                    (Some(due), true) => due,
                    (Some(due), false) => due.min(PUMP),
                    (None, true) => IDLE,
                    (None, false) => PUMP,
                }
            }
        }
    }
}

/// A run of the loop: its game and clock, and what it does around them.
pub trait Drive {
    /// The game and its clock.
    fn parts(&mut self) -> (&mut Sim, &mut Clock);

    /// At most `nap` waited for something to do.
    fn wait(&mut self, nap: Duration) -> Flow;

    /// Before each step.
    fn before(&mut self) -> Flow;

    /// After each step, before the pump.
    fn after(&mut self) -> Flow;

    /// After a turn's steps.
    fn turned(&mut self) -> Flow;
}

/// `run` driven until one of its hooks says quit.
pub fn drive(run: &mut impl Drive) {
    loop {
        let nap = {
            let (sim, clock) = run.parts();

            clock.nap(sim)
        };
        let flow = run
            .wait(nap)
            .and(|| {
                pump(run);
                steps(run)
            })
            .and(|| run.turned());

        if flow == Flow::Quit {
            break;
        }
    }
}

/// The peer's packets in and out.
fn pump(run: &mut impl Drive) {
    run.parts().0.pump(Instant::now());
}

/// As many frames as the clock owes, through the peer: a frame once
/// every seat's orders for it are in, at most [`MOST_STEPS`]; the
/// network pumped between.
fn steps(run: &mut impl Drive) -> Flow {
    let mut steps = 0;

    loop {
        let (sim, clock) = run.parts();
        let rate = clock.rate(sim);

        clock.tick(rate);

        if !clock.due() || steps >= MOST_STEPS || !sim.ready() {
            break Flow::On;
        }

        if run.before() == Flow::Quit {
            break Flow::Quit;
        }

        let (sim, clock) = run.parts();

        sim.step();
        clock.spent();
        steps += 1;

        if run.after() == Flow::Quit {
            break Flow::Quit;
        }

        pump(run);
    }
}

/// What a headless run measured.
#[derive(Clone, Copy, Debug)]
pub struct Ran {
    pub frames: u32,
    /// From the first frame to the last.
    pub took: Duration,
    /// Of that, time spent waiting for a peer's bundle.
    pub waited: Duration,
}

/// A headless run's: flat out, the bot's orders before each step, `each`
/// after.
struct Headless<'a, F> {
    sim: &'a mut Sim,
    clock: Clock,
    bot: &'a mut Option<Bot>,
    each: F,
    /// The first step's start.
    began: Option<Instant>,
    /// When the last step was done.
    reached: Option<Instant>,
    waited: Duration,
    /// The step running's start, timed.
    stepping: Option<Instant>,
}

impl<F: FnMut(&mut Sim)> Drive for Headless<'_, F> {
    fn parts(&mut self) -> (&mut Sim, &mut Clock) {
        (self.sim, &mut self.clock)
    }

    /// The nap slept: waiting for a peer's bundle, once begun, counted.
    fn wait(&mut self, nap: Duration) -> Flow {
        if !nap.is_zero() {
            let napping = Instant::now();

            std::thread::sleep(nap);

            if self.began.is_some() && self.reached.is_none() {
                self.waited += napping.elapsed();
            }
        }

        Flow::On
    }

    /// A stopped game ends the run; alone, an ended one too, unless a
    /// replay plays on. Else the bot's orders.
    fn before(&mut self) -> Flow {
        let sim = &mut *self.sim;
        let alone = sim.peer().players() == 1;
        let stands = alone && sim.ended() && !sim.replaying();

        if stands || sim.stopped.is_some() {
            Flow::Quit
        } else {
            if let Some(bot) = self.bot {
                for input in bot.moves(sim) {
                    sim.input(input);
                }
            }

            self.began.get_or_insert_with(Instant::now);
            self.stepping = perf::start();
            Flow::On
        }
    }

    fn after(&mut self) -> Flow {
        perf::stop(Timer::Step, self.stepping.take());
        (self.each)(self.sim);
        Flow::On
    }

    /// A stopped game ends the run; so does the last step, once every
    /// peer has what it needs, or after [`LINGER`].
    fn turned(&mut self) -> Flow {
        let done = matches!(self.clock.pacing, Pacing::FlatOut { left: 0 });

        if done {
            self.reached.get_or_insert_with(Instant::now);
        }

        let settled = |at: Instant| self.sim.peer().settled() || at.elapsed() > LINGER;

        if self.sim.stopped.is_some() || self.reached.is_some_and(settled) {
            Flow::Quit
        } else {
            Flow::On
        }
    }
}

/// `sim` run `steps` steps as fast as every peer allows, `bot` giving
/// orders, `each` after each step; then on, unstepped, until the others
/// have what they need. Alone, an ended game ends the run.
pub fn headless(
    sim: &mut Sim,
    steps: u32,
    bot: &mut Option<Bot>,
    each: impl FnMut(&mut Sim),
) -> Ran {
    let mut run = Headless {
        sim,
        clock: Clock::new(Pacing::FlatOut { left: steps }),
        bot,
        each,
        began: None,
        reached: None,
        waited: Duration::ZERO,
        stepping: None,
    };

    drive(&mut run);

    let took = match (run.began, run.reached) {
        (Some(b), Some(r)) => r.duration_since(b),
        (Some(b), None) => b.elapsed(),
        _ => Duration::ZERO,
    };
    let sim = run.sim;

    if !sim.peer().settled() && sim.stopped.is_none() {
        info!("net: left before every peer had all it needed");
    }

    Ran {
        frames: sim.frames(),
        took,
        waited: run.waited,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stopped game's clock owes no more time, whatever the speed.
    #[test]
    #[allow(clippy::float_cmp, reason = "the speed set, not worked out")]
    fn a_stopped_clock_owes_nothing() {
        let mut stopped = Clock::new(Pacing::Own);

        stopped.stop();

        assert_eq!(stopped.at(2.0), 0.0);
        assert_eq!(Clock::new(Pacing::Own).at(2.0), 2.0);
    }

    /// Flat out, a headless run steps the steps asked, and alone an
    /// ended game ends it early.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_headless_run_steps_as_asked() {
        use regent_core::sim::game::Input;
        use regent_core::sim::world::cheat::{Cheat, CheatCode};

        // The cheat's end needs a few frames to stand.
        const ENDING: u32 = 30;

        let mut sim = crate::game::begin::test_game(&[]).0.sim;
        let mut each = 0;
        let ran = headless(&mut sim, 20, &mut None, |_| each += 1);

        assert_eq!((ran.frames, each), (20, 20));

        let arg = sim.local();

        sim.input(Input::Cheat(CheatCode {
            cheat: Cheat::Win,
            arg,
        }));

        let ran = headless(&mut sim, 1_000, &mut None, |_| ());

        assert!(sim.ended(), "won");
        assert!(ran.frames < 20 + ENDING, "{}", ran.frames);
    }
}
