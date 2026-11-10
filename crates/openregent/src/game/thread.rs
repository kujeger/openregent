//! The game on a thread of its own: the one owner of the [`Sim`] and of
//! the game clock. The interface sends it commands and takes back what
//! it publishes; nothing else reaches the game.
//!
//! ```text
//!  main thread (Bevy)                            sim thread ("sim")
//!  input::send  ──> SimThread::queue (asks)
//!  game::clock::tick   ──> Command::Frame ────────────> the asks handed over, Interest kept,
//!                   asks, interest, pace, time   pause and speed to the peer; pump,
//!                                                Sim::step per 50 ms owed, each step's
//!                                                pieces taken (game::clock::Steps); publish
//!               <── Told: a stop ─────────────── a channel: never dropped
//!               <── State ────────────────────── the slot: one at a time
//!                   Frame (facts, motes, answers, seen::Taken), the slide,
//!                   the news since the last
//!               ──> Command::Spent ────────────> the last frame's Taken, freed here
//!  ui::keep     ──> Command::Save ─────────────> save::FromSim, on its own channel
//!  end::curtain ──> Command::Regroup ──────────> the peer's socket, for the lobby
//!  game's end   ──> SimThread dropped: the commands' channel closes, the
//!                   thread ends, joined; the Sim and its recorder go with it
//! ```
//!
//! Two modes ([`Mode`]), one path; they differ in the clock and the wait:
//!
//! ```text
//!  Sync       the drawn frame's time owed; tick waits for the state answering
//!             its hand-over: play, recordings, shots and digests as on one thread
//!  Pipelined  the thread's own clock: it steps, pumps the network and takes
//!             commands between steps whether or not anything is drawn (a
//!             minimised window's game and LAN peer run on); tick takes the
//!             newest state, if any, and never waits. A state is put only once
//!             the last was taken, so a fast game is not copied each step.
//! ```
//!
//! The loop itself, stepping, pumping and the clock, is
//! [`drive`]'s, shared with headless runs; this module gives it the
//! commands and the publishing.
//!
//! No state is dropped, so the news ride it: a sound or a line never
//! comes before the frame that shows it. Sync, the drawing waits for
//! each hand-over's one state before the next; pipelined, only the sim
//! thread puts, and only into an empty slot, which only the drawing
//! empties, by taking. A state is lost only with the game: the thread
//! ended, or the game left.
//!
//! The world is copied only after a step or a resync: no [`Ask`]
//! changes it. A thread that dies (a panic) stops the game: the drawing
//! keeps its last frame, and [`SimThread::stopped`] says why.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bevy::log::warn;
use bevy::prelude::Resource;

use regent_core::sim::world::{flag::Rewards, offer::Menu};
use regent_core::view::map::particles::Mote;

use crate::game::clock::Steps;
use crate::game::drive::{self, Clock, Drive, Flow, Pacing};
use crate::game::seen::{Answers, Ask, Frame, Interest, News, Taken};
use crate::game::sim::Sim;
use crate::perf::{self, Timer};
use crate::save::FromSim;

/// The thread's name, as a debugger or `top -H` shows it.
const NAME: &str = "sim";

/// What an ask of a thread that has ended gets.
const GONE: &str = "the game's thread has ended";

/// How the drawing and the sim thread share time.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The drawn frame's time; the drawing waits for its state: tests,
    /// screenshots.
    Sync,
    /// The thread's own clock; the drawing takes the newest state: play.
    Pipelined,
}

/// This machine's pause and speed (times the engine's), as the
/// interface sets them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pace {
    pub paused: bool,
    pub speed: f32,
}

/// What the interface may ask of the sim thread: the whole list.
enum Command {
    /// One drawn frame's hand-over.
    Frame(Handed),
    /// A save: the reward panel's settings set, then the game taken.
    Save(Rewards, Sender<FromSim>),
    /// A drawn frame's taken world, done with: freed there.
    Spent(Taken),
    /// Back to the LAN lobby: the peer's socket and seats
    /// (`net::Peer::regroup`); the game stands still after.
    Regroup(Sender<Option<crate::net::Regroup>>),
    /// A test's look at, or change to, the game.
    #[cfg(test)]
    Run(Box<dyn FnOnce(&mut Sim) + Send>),
}

/// A drawn frame's hand-over: its number, its changes in order, what it
/// asks run aside, pause and speed, and the seconds since the last
/// (sync mode's clock).
struct Handed {
    seq: u64,
    asks: Vec<Ask>,
    interest: Interest,
    pace: Pace,
    elapsed: f32,
}

/// What the sim thread says besides its states.
enum Told {
    /// The game stopped: the VM's error, a desync, a replay's mismatch.
    Stopped(String),
}

/// What the sim thread publishes.
pub struct State {
    /// The last hand-over handled.
    seq: u64,
    pub frame: Frame,
    /// Where the clock stands between frames: smooth mode's slide.
    pub slide: Slide,
    /// Game frames stepped since the last state.
    pub steps: u32,
    /// What the game said since the last state.
    pub news: News,
}

/// Game time owed short of the next frame, as of a moment, and how fast
/// it grows: the drawing works out the slide for when it draws.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slide {
    /// Game ms owed.
    owed: f32,
    /// When `owed` was worked out: pipelined mode's. Sync mode's slide
    /// is the drawn frame's, standing.
    at: Option<Instant>,
    /// Game ms owed a real ms since `at`: the speed, 0 paused.
    rate: f32,
}

impl Slide {
    /// Game ms owed at `now`.
    pub fn owed(&self, now: Instant) -> f32 {
        let since = self
            .at
            .map_or(0.0, |at| now.duration_since(at).as_secs_f32());

        self.owed + since * 1000.0 * self.rate
    }
}

/// The latest state, put by the sim thread, taken by the drawing.
#[derive(Default)]
struct Slot {
    latest: Mutex<Latest>,
    put: Condvar,
}

#[derive(Default)]
struct Latest {
    state: Option<State>,
    /// The thread has ended: nothing more comes.
    ended: bool,
}

/// The sim thread has ended.
struct Ended;

impl Slot {
    /// `state`, into the slot: emptied by the drawing before, in either
    /// mode, so no state and no news are lost (said if one were).
    fn put(&self, state: State) {
        if let Ok(mut latest) = self.latest.lock()
            && latest.state.replace(state).is_some()
        {
            warn!("a state the drawing did not take was dropped");
        }

        self.put.notify_all();
    }

    /// Whether the last state put was taken.
    fn taken(&self) -> bool {
        self.latest.lock().is_ok_and(|l| l.state.is_none())
    }

    fn end(&self) {
        if let Ok(mut latest) = self.latest.lock() {
            latest.ended = true;
        }

        self.put.notify_all();
    }

    /// The state answering hand-over `seq`, waited for: sync mode's.
    fn wait(&self, seq: u64) -> Result<State, Ended> {
        let latest = self.latest.lock().map_err(|_| Ended)?;
        let waiting = |l: &mut Latest| !l.ended && l.state.as_ref().is_none_or(|s| s.seq < seq);
        let mut latest = self.put.wait_while(latest, waiting).map_err(|_| Ended)?;

        latest.state.take().filter(|s| s.seq >= seq).ok_or(Ended)
    }

    /// The newest state, if one came since the last: pipelined mode's.
    fn take(&self) -> Result<Option<State>, Ended> {
        let mut latest = self.latest.lock().map_err(|_| Ended)?;

        match latest.state.take() {
            None if latest.ended => Err(Ended),
            state => Ok(state),
        }
    }
}

/// Ends the slot as the thread ends, by a return or a panic: after the
/// `Sim` is dropped.
struct Ends(Arc<Slot>);

impl Drop for Ends {
    fn drop(&mut self) {
        self.0.end();
    }
}

/// The interface's end of the sim thread, a game's: the one way to the
/// game. Dropped, it joins the thread.
#[derive(Resource)]
pub struct SimThread {
    mode: Mode,
    /// `None` only as it drops.
    commands: Option<Sender<Command>>,
    slot: Arc<Slot>,
    told: Mutex<Receiver<Told>>,
    /// `None` once joined.
    thread: Option<JoinHandle<()>>,
    /// The changes queued for the next drawn frame.
    asks: Vec<Ask>,
    /// The last drawn frame handed over.
    sent: u64,
    /// The last hand-over with an order in it.
    ordered: u64,
    /// Why the game stopped, once it did: the game's own reason, or the
    /// thread's end.
    pub stopped: Option<String>,
}

impl SimThread {
    /// `sim` on a thread of its own in `mode`, `menu` the build menu's
    /// data its preview runs need; and the frame it starts at.
    ///
    /// # Errors
    ///
    /// A thread the system would not start.
    pub fn spawn(sim: Sim, menu: Arc<Menu>, mode: Mode) -> Result<(SimThread, Frame), String> {
        let frame = Frame::of(&sim);
        let (commands, inbox) = mpsc::channel();
        let (tell, told) = mpsc::channel();
        let slot = Arc::<Slot>::default();
        let ends = Ends(slot.clone());
        let pacing = match mode {
            Mode::Sync => Pacing::Drawn { pending: 0.0 },
            Mode::Pipelined => Pacing::Own,
        };
        let stepper = Stepper {
            sim,
            menu,
            mode,
            inbox,
            slot: slot.clone(),
            clock: Clock::new(pacing),
            interest: Interest::default(),
            answers: Answers::default(),
            motes: frame.motes.clone(),
            taken: frame.taken.clone(),
            steps: None,
            stepped: 0,
            touched: Touched::No,
            handed: 0,
            ordered: Vec::new(),
            fresh: Fresh::No,
            tell,
            stepping: None,
            turn: 0,
            resyncs: frame.resyncs,
        };
        let thread = std::thread::Builder::new()
            .name(NAME.to_owned())
            .spawn(move || {
                // Dropped last: the slot ends once the game is gone.
                let _ends = ends;

                stepper.run();
            })
            .map_err(|e| format!("the game's thread: {e}"))?;

        let thread = SimThread {
            mode,
            commands: Some(commands),
            slot,
            told: Mutex::new(told),
            thread: Some(thread),
            asks: Vec::new(),
            sent: 0,
            ordered: 0,
            stopped: None,
        };

        Ok((thread, frame))
    }

    /// `ask` for the next drawn frame's hand-over.
    pub fn queue(&mut self, ask: Ask) {
        self.asks.push(ask);
    }

    /// The hand-over the asks queued now go with.
    pub const fn next(&self) -> u64 {
        self.sent + 1
    }

    /// Whether an order handed over is not yet in `frame`: what the
    /// game answered for it may be stale.
    pub const fn in_flight(&self, frame: &Frame) -> bool {
        frame.handed < self.ordered
    }

    /// The drawn frame handed over: the changes queued, then `interest`,
    /// `pace` and `elapsed` seconds. Sync mode: the state answering it,
    /// waited for. Pipelined: the newest state, taken before, if one
    /// came. `None` once the thread has ended.
    pub fn frame(&mut self, interest: Interest, pace: Pace, elapsed: f32) -> Option<State> {
        self.sent += 1;

        if self.asks.iter().any(Ask::orders) {
            self.ordered = self.sent;
        }

        let handed = Command::Frame(Handed {
            seq: self.sent,
            asks: std::mem::take(&mut self.asks),
            interest,
            pace,
            elapsed,
        });
        let state = match self.mode {
            Mode::Sync => self
                .send(handed)
                .ok_or(Ended)
                .and_then(|()| self.slot.wait(self.sent))
                .map(Some),
            // Taken first, so the slot is free for the state answering
            // this hand-over.
            Mode::Pipelined => {
                let taken = self.slot.take();

                self.send(handed).ok_or(Ended).and(taken)
            }
        };

        state.unwrap_or_else(|Ended| {
            self.died();
            None
        })
    }

    /// The game's stop noted, once told.
    pub fn told(&mut self) {
        if let Ok(told) = self.told.get_mut() {
            for Told::Stopped(why) in told.try_iter() {
                self.stopped = Some(why);
            }
        }
    }

    /// `taken`, done with: freed on the sim thread.
    pub fn spend(&self, taken: Taken) {
        // A thread that has ended frees nothing: dropped here.
        let _ = self.send(Command::Spent(taken));
    }

    /// The game's peer back to the LAN lobby (`net::Peer::regroup`);
    /// `None` alone, or once the thread has ended.
    pub fn regroup(&self) -> Option<crate::net::Regroup> {
        let (reply, answer) = mpsc::channel();

        self.send(Command::Regroup(reply))
            .and_then(|()| answer.recv().ok())
            .flatten()
    }

    /// What a save takes of the game, the reward panel at `rewards`.
    ///
    /// # Errors
    ///
    /// A thread that has ended.
    pub fn save(&self, rewards: Rewards) -> Result<FromSim, String> {
        let (reply, answer) = mpsc::channel();

        self.send(Command::Save(rewards, reply))
            .and_then(|()| answer.recv().ok())
            .ok_or_else(|| GONE.to_owned())
    }

    /// `look` run on the game, on its thread, and its answer.
    ///
    /// # Errors
    ///
    /// A thread that has ended, or ends in `look`.
    #[cfg(test)]
    pub fn run<R: Send + 'static>(
        &self,
        look: impl FnOnce(&mut Sim) -> R + Send + 'static,
    ) -> Result<R, String> {
        let (reply, answer) = mpsc::channel();
        let run = Box::new(move |sim: &mut Sim| {
            let _ = reply.send(look(sim));
        });

        self.send(Command::Run(run))
            .and_then(|()| answer.recv().ok())
            .ok_or_else(|| GONE.to_owned())
    }

    /// `command` sent; `None` once the thread has ended.
    fn send(&self, command: Command) -> Option<()> {
        self.commands.as_ref()?.send(command).ok()
    }

    /// The thread ended under the game: joined, its panic the stop's
    /// reason, said once.
    fn died(&mut self) {
        if let Some(thread) = self.thread.take() {
            let why = match thread.join() {
                Ok(()) => "it returned".to_owned(),
                Err(panic) => panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "a panic".to_owned()),
            };
            let why = format!("{GONE}: {why}");

            warn!("the game stopped at {why}");
            self.stopped = Some(why);
        }
    }

    /// Whether the thread has ended: a test's.
    #[cfg(test)]
    pub fn ended(&self) -> Arc<dyn Fn() -> bool + Send + Sync> {
        let slot = self.slot.clone();

        Arc::new(move || slot.latest.lock().map_or(true, |l| l.ended))
    }
}

impl Drop for SimThread {
    /// The commands' channel closed, so the thread's wait ends; then
    /// joined, so its recorder is closed before another game opens the
    /// same file.
    fn drop(&mut self) {
        drop(self.commands.take());

        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            warn!("the game's thread ended in a panic");
        }
    }
}

/// Whether a change since the last publish may have touched the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Touched {
    No,
    Yes,
}

/// Whether something happened since the last state: pipelined mode
/// publishes only then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fresh {
    No,
    Yes,
}

/// The sim thread's own: the game, its clock, and what it last
/// published.
struct Stepper {
    sim: Sim,
    menu: Arc<Menu>,
    mode: Mode,
    inbox: Receiver<Command>,
    slot: Arc<Slot>,
    clock: Clock,
    interest: Interest,
    answers: Answers,
    motes: Vec<Mote>,
    /// What the last publish took of the world.
    taken: Taken,
    /// The pieces since `taken`'s, not yet published.
    steps: Option<Steps>,
    /// Game frames stepped since the last state.
    stepped: u32,
    touched: Touched,
    /// The last hand-over handled.
    handed: u64,
    /// Each hand-over with orders since the last state, and the frame
    /// they come before.
    ordered: Vec<(u64, u32)>,
    fresh: Fresh,
    tell: Sender<Told>,
    /// The turn's start, timed.
    stepping: Option<Instant>,
    /// Game frames stepped this turn.
    turn: u32,
    /// The resyncs `taken` is after ([`Sim::resyncs`]).
    resyncs: u32,
}

impl Stepper {
    /// The loop ([`drive`]), until the interface's end closes the
    /// channel.
    fn run(mut self) {
        drive::drive(&mut self);
    }

    /// Sync mode: each command until a drawn frame's hand-over, handled.
    fn handed(&mut self) -> Flow {
        loop {
            match self.inbox.recv() {
                Ok(Command::Frame(handed)) => {
                    self.hand(handed);
                    break Flow::On;
                }
                Ok(command) => self.command(command),
                Err(_) => break Flow::Quit,
            }
        }
    }

    /// A command; a drawn frame's handed over.
    fn command(&mut self, command: Command) {
        match command {
            Command::Frame(handed) => {
                self.hand(handed);
            }
            Command::Save(rewards, reply) => {
                self.sim.set_rewards(rewards);

                let _ = reply.send(FromSim::of(&self.sim));
            }
            Command::Spent(taken) => drop(taken),
            Command::Regroup(reply) => {
                let _ = reply.send(self.sim.peer_mut().regroup());
            }
            #[cfg(test)]
            Command::Run(look) => {
                look(&mut self.sim);
                self.touched = Touched::Yes;
                self.fresh = Fresh::Yes;
            }
        }
    }

    /// Each command waiting, handled.
    fn commands(&mut self) -> Flow {
        loop {
            match self.inbox.try_recv() {
                Ok(command) => self.command(command),
                Err(TryRecvError::Empty) => return Flow::On,
                Err(TryRecvError::Disconnected) => return Flow::Quit,
            }
        }
    }

    /// A drawn frame's changes, before any step; its interest, pause and
    /// speed kept. Its seconds since the last: sync mode's clock.
    fn hand(&mut self, handed: Handed) {
        let Handed {
            seq,
            asks,
            interest,
            pace,
            elapsed,
        } = handed;

        if asks.iter().any(Ask::orders) {
            self.ordered.push((seq, self.sim.order_frame()));
        }

        // No ask changes the world: orders wait for the next step, the
        // music's stop runs aside (`Game::music_stopped`).
        for ask in asks {
            ask.hand(&mut self.sim);
        }

        self.interest = interest;
        self.handed = seq;
        self.fresh = Fresh::Yes;
        self.sim.peer_mut().set_local(pace.paused, pace.speed);

        // The clock starts with the first drawn frame.
        self.clock.start();
        self.clock.drawn(elapsed);
    }

    /// The game's stop, said once: the clock stops with it.
    fn stop(&mut self) {
        if let Some(why) = &self.sim.stopped
            && !self.clock.stopped()
        {
            warn!("the game stopped at {why}");
            self.clock.stop();

            let _ = self.tell.send(Told::Stopped(why.clone()));
        }
    }

    /// The state to publish, with the news since the last: the slide as
    /// the clock last owed `at` (pipelined), else standing.
    fn state(&mut self, at: Option<Instant>) -> State {
        let frame = self.publish();
        let mut news = News::default();

        news.gather(&mut self.sim);
        news.ordered.append(&mut self.ordered);
        self.fresh = Fresh::No;

        State {
            seq: self.handed,
            frame,
            slide: Slide {
                owed: self.clock.owed,
                at,
                rate: self.clock.rate(&self.sim),
            },
            steps: std::mem::take(&mut self.stepped),
            news,
        }
    }

    /// What the interface reads: the world copied and taken again only
    /// if it changed, the motes only after a step; then what
    /// [`Interest`] asks.
    fn publish(&mut self) -> Frame {
        let published = perf::start();
        let resynced = self.sim.resyncs() != self.resyncs;
        let changed = self.stepped > 0 || self.touched == Touched::Yes || resynced;

        if changed || self.steps.is_some() {
            let world = if changed {
                Arc::new(perf::timed(Timer::Clone, || self.sim.world().clone()))
            } else {
                self.taken.world.clone()
            };

            // A resync's world shares nothing with the last: taken
            // afresh, no piece walked from where the last had it.
            self.taken = if resynced {
                let steps = Steps::default().filled(&world).unwrap_or_default();

                self.steps = None;
                self.resyncs = self.sim.resyncs();
                Taken::of_world(world, Arc::new(steps), self.sim.local())
            } else {
                let steps = self
                    .steps
                    .take()
                    .map_or_else(|| self.taken.steps.clone(), Arc::new);

                self.taken.after(world, steps, self.sim.local())
            };
            self.touched = Touched::No;
        }

        if self.stepped > 0 || resynced {
            self.motes = self.sim.motes();
        }

        self.answers =
            std::mem::take(&mut self.answers).asked(&mut self.sim, &self.interest, &self.menu);

        let frame = Frame {
            handed: self.handed,
            ..Frame::with(
                &self.sim,
                self.motes.clone(),
                self.answers.clone(),
                self.taken.clone(),
            )
        };

        perf::stop(Timer::Publish, published);
        frame
    }
}

/// The sim thread's turn: commands in, steps' pieces taken, a state out.
impl Drive for Stepper {
    fn parts(&mut self) -> (&mut Sim, &mut Clock) {
        (&mut self.sim, &mut self.clock)
    }

    /// Sync: until a drawn frame's hand-over. Pipelined: a command, or
    /// `nap`; then each waiting.
    fn wait(&mut self, nap: Duration) -> Flow {
        let flow = match self.mode {
            Mode::Sync => self.handed(),
            Mode::Pipelined => match self.inbox.recv_timeout(nap) {
                Ok(command) => {
                    self.command(command);
                    self.commands()
                }
                Err(RecvTimeoutError::Timeout) => self.commands(),
                Err(RecvTimeoutError::Disconnected) => Flow::Quit,
            },
        };

        self.stepping = perf::start();
        self.turn = 0;
        flow
    }

    fn before(&mut self) -> Flow {
        Flow::On
    }

    /// The step's pieces taken; the commands come since handled.
    fn after(&mut self) -> Flow {
        let last = self.steps.as_ref().unwrap_or(&self.taken.steps);

        self.steps = Some(last.after(self.sim.world()));
        self.stepped += 1;
        self.turn += 1;
        self.fresh = Fresh::Yes;
        self.commands()
    }

    /// The stop said; a state put. Sync: the hand-over's. Pipelined:
    /// once something happened and the last was taken (back-pressure).
    fn turned(&mut self) -> Flow {
        // Before the first step: the pieces as the world starts.
        let last = self.steps.as_ref().unwrap_or(&self.taken.steps);

        if let Some(filled) = last.filled(self.sim.world()) {
            self.steps = Some(filled);
        }

        if self.turn > 0 || self.mode == Mode::Sync {
            perf::stop(Timer::Step, self.stepping.take());
        }

        self.stop();

        match self.mode {
            Mode::Sync => {
                let state = self.state(None);

                self.slot.put(state);
            }
            Mode::Pipelined => {
                if self.fresh == Fresh::Yes && self.slot.taken() {
                    let state = self.state(self.clock.last);

                    self.slot.put(state);
                }
            }
        }

        Flow::On
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drawn frame's time at 60 Hz, s.
    const DRAWN: f32 = 1.0 / 60.0;

    /// Running, at the engine's speed.
    const RUNNING: Pace = Pace {
        paused: false,
        speed: 1.0,
    };

    /// A new game on its thread, in `mode`.
    fn spawned(mode: Mode) -> SimThread {
        let (begun, catalogue) = crate::game::begin::test_game(&[]);

        SimThread::spawn(begun.sim, Arc::new(Menu::of(&catalogue)), mode)
            .expect("spawned")
            .0
    }

    /// A panic on the sim thread stops the game, its message the
    /// reason: the drawing gets no state, nothing waits forever.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_panic_on_the_sim_thread_stops_the_game() {
        let mut thread = spawned(Mode::Sync);

        assert!(thread.frame(Interest::default(), RUNNING, DRAWN).is_some());
        assert!(thread.run(|_| panic!("a planted panic")).is_err());
        assert!(thread.frame(Interest::default(), RUNNING, DRAWN).is_none());
        assert!(
            thread
                .stopped
                .as_ref()
                .is_some_and(|why| why.contains("a planted panic")),
            "{:?}",
            thread.stopped
        );
        assert!(thread.save(Rewards::default()).is_err());
    }

    /// Each drawn frame's state is the game's as its frame left it: the
    /// clock steps a frame per 50 ms owed, the world is copied as it
    /// steps and shared while it does not.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_frame_is_published_as_the_game_left_it() {
        // 62.5 ms a drawn frame, exact in binary: 8 of them owe 10 game
        // frames.
        const SLOW: f32 = 0.0625;

        let mut thread = spawned(Mode::Sync);
        let paused = Pace {
            paused: true,
            ..RUNNING
        };
        let mut seen = Vec::new();
        let mut copies = 0;
        let mut last: Option<Arc<regent_core::sim::world::World>> = None;

        for pace in [[RUNNING; 8].as_slice(), &[paused; 2]].concat() {
            let state = thread
                .frame(Interest::default(), pace, SLOW)
                .expect("a state");
            let world = state.frame.taken.world.clone();

            if last.as_ref().is_none_or(|l| !Arc::ptr_eq(l, &world)) {
                copies += 1;
            }

            seen.push((state.frame.frames, state.steps));
            last = Some(world);
        }

        assert_eq!(
            seen,
            [
                (1, 1),
                (2, 1),
                (3, 1),
                (5, 2),
                (6, 1),
                (7, 1),
                (8, 1),
                (10, 2),
                (10, 0),
                (10, 0)
            ]
        );
        assert_eq!(copies, 8, "one a stepped frame, shared while paused");
        assert_eq!(thread.run(|sim| sim.frames()), Ok(10), "the thread's own");
    }

    /// An order handed over is reported with the frame the game took it
    /// for, which `input::choose` waits for; one not yet in a state is
    /// in flight.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_order_is_reported_with_its_frame() {
        use regent_core::sim::game::Input;
        use regent_core::sim::world::cheat::{Cheat, CheatCode};

        let mut thread = spawned(Mode::Sync);
        let first = thread
            .frame(Interest::default(), RUNNING, DRAWN)
            .expect("a state");
        let gold = Ask::Input(Input::Cheat(CheatCode {
            cheat: Cheat::Gold,
            arg: first.frame.local,
        }));
        let before = thread.run(|sim| sim.order_frame()).expect("the frame");

        thread.queue(gold);

        let hand = thread.next();

        assert!(!thread.in_flight(&first.frame));

        let state = thread
            .frame(Interest::default(), RUNNING, DRAWN)
            .expect("a state");

        assert!(thread.in_flight(&first.frame), "the first state is stale");
        assert!(!thread.in_flight(&state.frame));
        assert_eq!(state.news.ordered, [(hand, before)]);
    }

    /// A music track's end runs the scripts' callback aside: the world
    /// is not copied again, yet the track it chose is published.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_music_stop_publishes_its_track_without_a_copy() {
        const PAUSED: Pace = Pace {
            paused: true,
            ..RUNNING
        };

        let mut thread = spawned(Mode::Sync);

        // The scripts' start sets the callback and a track.
        for _ in 0..4 {
            thread.frame(Interest::default(), RUNNING, 0.1);
        }

        let before = thread
            .frame(Interest::default(), PAUSED, DRAWN)
            .expect("a state");

        thread.queue(Ask::MusicStopped);

        let after = thread
            .frame(Interest::default(), PAUSED, DRAWN)
            .expect("a state");
        let heard = thread.run(|sim| sim.music()).expect("the track");

        assert!(
            Arc::ptr_eq(&before.frame.taken.world, &after.frame.taken.world),
            "not copied"
        );
        assert_eq!(after.frame.frames, before.frame.frames, "no step");
        assert_eq!(after.frame.music, heard);
        assert!(heard.is_some(), "a track chosen");
    }

    /// Pipelined, the clock is the thread's own: the game runs on with
    /// nothing drawn, from the first hand-over, and a state waiting to
    /// be taken holds every step since the last.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_pipelined_game_runs_without_drawn_frames() {
        const WAIT: Duration = Duration::from_millis(600);

        let mut thread = spawned(Mode::Pipelined);

        std::thread::sleep(WAIT / 2);
        assert_eq!(thread.run(|sim| sim.frames()), Ok(0), "not before drawn");

        // The hand-over starts the clock. A test's look publishes, so a
        // state may be in, but not a step.
        let looked = thread.frame(Interest::default(), RUNNING, DRAWN);

        assert!(looked.is_none_or(|s| s.frame.frames == 0), "not stepped");

        std::thread::sleep(WAIT);

        let frames = thread.run(|sim| sim.frames()).expect("frames");

        // 600 ms: 12 frames, but for a busy machine's lag.
        assert!((8..=13).contains(&frames), "{frames}");

        // The state answering the first hand-over, not taken, stood.
        let first = thread
            .frame(Interest::default(), RUNNING, DRAWN)
            .expect("a state");

        assert_eq!((first.frame.frames, first.steps), (0, 0));

        // Taken, the next comes: every step since.
        let next = (0..1_000).find_map(|_| {
            std::thread::sleep(Duration::from_millis(1));
            thread.frame(Interest::default(), RUNNING, DRAWN)
        });
        let next = next.expect("a state");

        assert!(next.steps >= frames, "{} {frames}", next.steps);
        assert_eq!(next.steps, next.frame.frames, "one state for them all");
    }

    /// Pipelined, a panic on the sim thread stops the game, its message
    /// the reason: the drawing takes no more states.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_panic_on_a_pipelined_sim_thread_stops_the_game() {
        let mut thread = spawned(Mode::Pipelined);

        let _ = thread.frame(Interest::default(), RUNNING, DRAWN);
        assert!(thread.run(|_| panic!("a planted panic")).is_err());

        let stopped = (0..1_000).any(|_| {
            std::thread::sleep(Duration::from_millis(1));
            let _ = thread.frame(Interest::default(), RUNNING, DRAWN);

            thread.stopped.is_some()
        });

        assert!(stopped, "stopped");
        assert!(
            thread
                .stopped
                .as_ref()
                .is_some_and(|why| why.contains("a planted panic")),
            "{:?}",
            thread.stopped
        );
        assert!(thread.frame(Interest::default(), RUNNING, DRAWN).is_none());
    }

    /// Quitting while the thread catches up as fast as it can: the
    /// thread notices between steps, and the drop's join is quick.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn quitting_mid_catch_up_joins_at_once() {
        const FLAT_OUT: Pace = Pace {
            paused: false,
            speed: 1_000.0,
        };

        let mut thread = spawned(Mode::Pipelined);
        let ended = thread.ended();

        let _ = thread.frame(Interest::default(), FLAT_OUT, DRAWN);

        // Stepping: frames run.
        let stepping = (0..2_000).any(|_| {
            std::thread::sleep(Duration::from_millis(1));
            thread.run(|sim| sim.frames()).is_ok_and(|f| f > 20)
        });

        assert!(stepping, "stepping");

        let quit = Instant::now();

        drop(thread);

        assert!(ended(), "the thread ended");
        assert!(
            quit.elapsed() < Duration::from_secs(1),
            "{:?}",
            quit.elapsed()
        );
    }
}
