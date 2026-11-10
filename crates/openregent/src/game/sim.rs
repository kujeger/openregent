//! The game itself: the library's world and VM, stepped a frame at a time.
//!
//! ```text
//!  load       what a game is made from (`game::load`)
//!  Sim        Game::free: setup, then a step per 50 ms frame
//!   ├─ lockstep  the peer, digests, a resync
//!   ├─ replay    orders held for the peer, a recording, a replay
//!   ├─ saves     saves a step takes
//!   ├─ report    what it says of itself
//!   └─ panel     what the panels ask and hand it
//! ```

use std::collections::VecDeque;
use std::fmt::Write as _;

use bevy::log::{error, info, warn};
use regent_core::data::fourcc::FourCc;
use regent_core::gpl::load::Program;
use regent_core::gpl::vm::{Config, Vm};
use regent_core::sim::freestyle::Setup;
use regent_core::sim::game::orders::reward_initial;
use regent_core::sim::game::{Game, Input, Order};
use regent_core::sim::host::{Faults, Reports};
use regent_core::sim::scene::Staging;
use regent_core::sim::world::{
    CROWN, Output, World,
    chat::{self, Said},
    defeat::End,
    flag::Rewards,
    guild::{
        bounty::Term,
        camp::{Camp, CampMove},
        extortion::Extortion,
    },
    offer::{Button, Menu, Placement},
};
use regent_core::view::map::particles::{Mote, Sparks};
use regent_core::view::menu::autosave;

use crate::game::record::{DIGEST_EVERY, Event, Recorder};
use crate::net::{Audience, Given, Payload, Peer, Turn};

mod lockstep;
mod panel;
mod replay;
mod report;
mod saves;

/// A game in progress. Stops at the first run the VM cannot finish.
///
/// ```text
///  live input ──> outbox ──> the peer (net), at the step ──┐
///  a replay's events, by frame ─────────────────────────────┴─> recorded ──> Game::input
///  live input waits for a replay's last
/// ```
pub struct Sim {
    game: Game<'static>,
    /// Why it stopped, if it did: where, and the GPL stack.
    pub stopped: Option<String>,
    /// The recording written and the replay fed back ([`replay`]).
    replay: replay::Replay,
    /// What the log was last told ([`report`]).
    said: report::Said,
    /// The interface's own lines, not yet shown.
    told: Vec<Said>,
    /// The particles, stepped after each frame: no rule reads them, so
    /// the world does not keep them.
    sparks: Sparks,
    /// The reward panel's settings, this machine's: the flags it sends
    /// carry them. The original saves them in `GSGL`.
    rewards: Rewards,
    /// The camp move this machine began and has not sent or left: the
    /// cursor shows its price, and leaving it sends `CampLeave`.
    camp: Option<CampMove>,
    /// This machine's events given since the last frame, held for its
    /// next bundle (`net`): they reach the game with every peer's, as
    /// the next frame steps.
    outbox: Vec<Event>,
    /// The lockstep peer every frame steps through: [`Peer::solo`]
    /// unless [`Sim::networked`].
    peer: Peer,
    /// Saves steps took, and the autosave's setting ([`saves`]).
    saves: saves::Saves,
    /// Resyncs gone on from ([`Sim::resyncs`]).
    resyncs: u32,
    /// This machine's human ([`Sim::local`]): no part of play, so the
    /// world does not keep it.
    local: u32,
    /// The track the music's stop callback chose on this machine, until
    /// the scripts' next `PlayMusic` ([`Sim::music`]).
    heard: Option<i32>,
    /// What the frames left since [`Sim::take_output`], taken from the
    /// game after each.
    output: Output,
    /// Whether this machine's player plays, or surrendered
    /// ([`Sim::surrender`]): then it watches the whole map, gives no
    /// order, and is shown its end once every seat is out ([`Sim::over`]).
    seat: Seat,
}

/// Whether this machine's player plays, or surrendered and watches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Seat {
    #[default]
    Plays,
    Spectates,
}

/// What a game is, beside its simulation: what the interface shows of it
/// and a save needs (`begin::Plan::begin`). A resource any system and
/// run condition reads.
#[derive(bevy::prelude::Resource)]
pub struct Session {
    /// A quest's key and title; `None` for the custom game.
    pub quest: Option<(FourCc, String)>,
    /// A quest's whole id: its victory is kept by it
    /// (`Progress::win_quest`).
    pub quest_id: Option<regent_core::data::guid::Guid>,
    /// A quest's `<Short>`, its goals (`AP88`).
    pub goals: Option<String>,
    /// A quest's own `<Win>` and `<Loss>`, the verdict's second ask
    /// (`regent_core::view::menu::end::verdict`); none for the custom game.
    pub messages: regent_core::content::quest::Messages,
    /// A custom game's setup: the title scroll's name and goals.
    pub setup: Option<Setup>,
    /// What a save of it needs of the install.
    pub needs: crate::save::Needs,
    /// A saved game's interface: for the drawing, once.
    pub resumed: Option<Resumed>,
}

impl Session {
    /// The quest's key; `None` for the custom game.
    #[must_use]
    pub fn quest_key(&self) -> Option<FourCc> {
        self.quest.as_ref().map(|(key, _)| *key)
    }
}

/// What a resumed game hands the drawing.
pub struct Resumed {
    pub interface: crate::save::Interface,
}

impl Sim {
    /// A new game on `world`, its GPL RNG seeded with `seed`, `script` the
    /// quest's function the setup runs (`startup::FREESTYLE`, a quest's
    /// name) as `staging` sets it (a scene's, else the default: the
    /// setup at 0).
    ///
    /// # Errors
    ///
    /// A setup run that fails.
    pub fn on(
        program: &'static Program,
        world: World,
        (seed, script): (u32, &str),
        staging: &Staging,
        config: Config,
    ) -> Result<Sim, String> {
        let vm = Vm::new(program, config);
        let mut game = Game::free(vm, seed, world);

        staging
            .run(&mut game, script)
            .map_err(|d| format!("setup: {d:?}"))?;

        let initial = reward_initial(program);
        let rewards = Rewards {
            attack: initial,
            explore: initial,
        };

        // The engine seeds particles from the clock; the GPL seed stands in.
        Ok(Sim::of(game, seed, rewards))
    }

    /// The game `snapshot` holds going on, on `world`, made as the saved
    /// one was ([`regent_core::save`]), the reward panel at `rewards`: no
    /// setup runs.
    ///
    /// # Errors
    ///
    /// A snapshot naming what this build or program lacks.
    pub fn resume(
        program: &'static Program,
        world: World,
        snapshot: regent_core::save::Snapshot,
        rewards: crate::save::Rewards,
        config: Config,
    ) -> Result<Sim, String> {
        let vm = Vm::new(program, config);
        // Particles are not saved: they start afresh, the session seed
        // standing in for the engine's clock.
        let seed = snapshot.game.host.seed;
        let game = Game::resumed(vm, world, snapshot).map_err(|e| e.to_string())?;

        Ok(Sim::of(game, seed, rewards.into()))
    }

    /// A game just begun, its particles' RNG seeded with `seed`, the
    /// reward panel at `rewards`. A camp move the world holds for the
    /// local player is this machine's: left once play goes on.
    fn of(game: Game<'static>, seed: u32, rewards: Rewards) -> Sim {
        let sparks = Sparks::new(seed, game.world().now());
        let world = game.world();
        let camp = world.camp_held(CROWN);
        let peer = Peer::solo();
        let replay = replay::Replay::from(game.frames);

        Sim {
            game,
            sparks,
            stopped: None,
            replay,
            said: report::Said::default(),
            told: Vec::new(),
            rewards,
            camp,
            outbox: Vec::new(),
            peer,
            saves: saves::Saves::default(),
            resyncs: 0,
            local: CROWN,
            heard: None,
            output: Output::default(),
            seat: Seat::Plays,
        }
    }

    /// An engine dispatch of `function` between frames: a test's.
    #[cfg(test)]
    pub fn call(&mut self, function: &str, args: &[regent_core::gpl::ops::Value]) {
        self.game.call(function, args).expect("the call runs");
    }

    /// A peer's `event`, taken now ([`Sim::take`]): a test's.
    #[cfg(test)]
    pub fn deliver(&mut self, event: Event) {
        self.take(event);
    }

    /// How many of this machine's events wait for the next bundle: a
    /// placement asked again once it grows ([`Sim::placement`]).
    pub const fn held(&self) -> usize {
        self.outbox.len()
    }

    /// One 50 ms frame, unless stopped, through the peer: this machine's
    /// orders sealed, every seat's for the frame taken, a replay's due
    /// next. Only once [`Sim::ready`].
    pub fn step(&mut self) {
        let ran = self.frames();
        let given = self.exchange();

        self.take_given(ran, given);
        self.frame();
        self.digests();
        self.peer.fed();
        self.check();
        self.save_asked();
        self.save_due();
    }

    /// The frame itself, a replay's events due first.
    fn frame(&mut self) {
        self.scripted();

        if self.stopped.is_none()
            && let Err(d) = self.game.frame()
        {
            self.stopped = Some(self.stop_report(&format!("{d:?}")));
        }

        // The scripts' `PlayMusic` ends this machine's own track.
        let output = self.game.take_output();

        if output.music_played {
            self.heard = None;
        }

        self.output.append(output);

        self.sparks.step(self.game.world());
        self.say_reports();
        self.say_lost();
        self.say_dropped();
    }

    /// Take the player's `input` before the next frame, as the engine
    /// takes a click in its pacing wait (`Game::order_frame`); not while a
    /// replay plays, nor once the game has ended (**Inferred**: state 8's
    /// screens send no order). It waits in the outbox for the step.
    pub fn input(&mut self, input: Input) {
        let before = self.order_frame();
        let order = Order {
            player: self.local(),
            input,
        };

        if self.takes_orders() {
            self.give(Event::Input { before, order });
        }
    }

    /// Whether the player's orders reach the game: not while a replay
    /// plays, nor once the game has ended or its player surrendered.
    fn takes_orders(&self) -> bool {
        !self.replaying() && !self.ended() && !self.spectating()
    }

    /// The session's cheat setting.
    pub fn set_cheats(&mut self, cheats: regent_core::sim::world::cheat::Cheats) {
        self.game
            .host_mut()
            .core_mut()
            .world_mut()
            .set_cheats(cheats);
    }

    /// The GPL program: its constants price spells and research.
    pub const fn program(&self) -> &'static Program {
        self.game.vm().program()
    }

    /// What the world left since the last call (`Game::take_output`),
    /// the interface's lines after the world's.
    pub fn take_output(&mut self) -> Output {
        let mut output = std::mem::take(&mut self.output);

        output.append(self.game.take_output());
        output.said.append(&mut self.told);
        output
    }

    /// The interface's own `line`, shown to the local player: no message,
    /// so nothing recorded.
    pub fn say(&mut self, line: chat::Line) {
        let at = self.world().now();

        self.told.push(Said {
            at,
            to: None,
            line,
            from: None,
        });
    }

    /// A music track ended: the scripts' stop callback, unless stopped.
    /// It changes no play, so it is not recorded and runs in a replay too.
    pub fn music_stopped(&mut self) {
        if self.stopped.is_none() {
            self.heard = self.game.music_stopped(self.heard);
        }
    }

    /// The track this machine plays: the stop callback's, until the
    /// scripts' next `PlayMusic`; else theirs (`World::music`).
    pub fn music(&self) -> Option<i32> {
        self.heard.or(self.world().music())
    }

    /// The most records one run took, and its function.
    pub const fn longest(&self) -> Option<(u64, &'static str)> {
        self.game.vm().longest()
    }

    /// Whether the local player's game has ended, as shown: won or lost
    /// ([`Sim::end`]), and no CONTINUE PLAYING given since the last frame.
    pub fn ended(&self) -> bool {
        let local = self.local();
        let played_on = self
            .outbox
            .iter()
            .any(|e| matches!(e, Event::PlayOn { player, .. } if *player == local));

        self.end().is_some() && !played_on
    }

    /// How the local player's game ended, as the interface shows it: a
    /// spectator's loss waits until the game is [`Sim::over`].
    pub fn end(&self) -> Option<End> {
        let held = self.spectating() && !self.over();

        self.world().ended(self.local()).filter(|_| !held)
    }

    /// Whether every human seat is out: its game ended (won, lost or
    /// surrendered), or its player dropped, which counts as a surrender.
    /// A seat plays its own player, this machine's the local one.
    pub fn over(&self) -> bool {
        let world = self.world();
        let local = self.local();

        world
            .humans()
            .all(|p| world.ended(p).is_some() || (p != local && self.peer.gone(p)))
    }

    /// Whether this machine's player plays or watches.
    pub const fn seat(&self) -> Seat {
        self.seat
    }

    /// Whether this machine's player surrendered and watches.
    pub const fn spectating(&self) -> bool {
        matches!(self.seat, Seat::Spectates)
    }

    /// The local player surrenders: from now it watches, its orders
    /// refused, and its game ends lost as every peer steps the
    /// surrender. A LAN game's alone, once, and not in a replay.
    pub fn surrender(&mut self) {
        let after = self.game.frames;
        let player = self.local();
        let asked = self.peer.players() > 1 && !self.spectating() && !self.replaying();

        if asked {
            self.seat = Seat::Spectates;
            self.give(Event::Surrender { after, player });
        }
    }

    /// This machine's chat `line` to the other peers: a spectator's for
    /// the spectators alone.
    pub fn chat(&mut self, line: &str) {
        let to = if self.spectating() {
            Audience::Spectators
        } else {
            Audience::All
        };

        self.peer.say(line, to);
    }

    /// The local player's ended game goes on: the end panel's CONTINUE PLAYING,
    /// recorded; not while a replay plays, which has its own.
    pub fn play_on(&mut self) {
        let after = self.game.frames;
        let player = self.local();

        if !self.replaying() {
            self.give(Event::PlayOn { after, player });
        }
    }

    /// Frames run.
    pub const fn frames(&self) -> u32 {
        self.game.frames
    }

    /// The frame an order given now comes before (`Game::order_frame`).
    pub const fn order_frame(&self) -> u32 {
        self.game.order_frame()
    }

    pub fn world(&self) -> &World {
        self.game.world()
    }

    /// `player` this machine's human ([`Sim::local`]): one of the
    /// game's human seats, else an error naming them.
    ///
    /// # Errors
    ///
    /// `player` no human seat.
    pub fn set_local(&mut self, player: u32) -> Result<(), String> {
        let humans = self.world().humans();

        if humans.contains(&player) {
            self.local = player;
            // A camp move the world holds is the new seat's to place or leave.
            self.camp = self.world().camp_held(player);
            Ok(())
        } else {
            Err(format!(
                "--local {player}: the game's human seats are {} to {}",
                humans.start,
                humans.end.saturating_sub(1)
            ))
        }
    }

    /// The local player, app: this machine's
    /// human, whose panels, sounds and lines it shows. Play does the
    /// same for every human seat ([`World::humans`]). [`CROWN`] unless
    /// [`Sim::set_local`] says otherwise.
    pub const fn local(&self) -> u32 {
        self.local
    }

    /// Each live particle, as the last frame left them.
    pub fn motes(&self) -> Vec<Mote> {
        self.sparks.motes(self.game.world())
    }

    /// Where the world gave no answer and the default stood in.
    pub const fn faults(&self) -> &Faults {
        self.game.faults()
    }

    pub const fn reports(&self) -> &Reports {
        self.game.reports()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::compiled::tests as compiled;
    use crate::game::load::{archives, images, randoms};
    use regent_core::data::text::Text;
    use regent_core::sim::freestyle::List;
    use std::path::Path;

    /// A mod with sources and no `.bcd` compiles; its program list
    /// names the cached image, and it composes over the dataset.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_source_only_mod_compiles_and_composes() {
        let (_dir, mods, cache) = compiled::fixture("composes");
        let (library, builds) = compiled::scanned(&mods, &cache);

        assert_eq!(
            compiled::image(&builds).expect("compiled").1,
            crate::files::compiled::Cached::Fresh
        );

        let m = library.find(compiled::guid()).expect("found");
        let (_, program) = &m.load.programs[0];

        assert!(Path::new(program).starts_with(&cache), "{program}");

        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let sdk = hd.join("SDK");
        let files = regent_core::content::dataset::Install::new(
            &hd,
            regent_core::content::dataset::Bytecode::Sdk(&sdk),
        )
        .files_with(regent_core::content::dataset::Datasets::Expansion, &[m])
        .expect("composed");

        assert_eq!(files.programs[0], Path::new(program));
        assert!(images(&files.programs).is_ok());
    }

    /// A build given and not yet stepped, held for the peer, shows and
    /// saves as single player showed and saved one taken as given: its
    /// placement pending, its order among those to come.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_build_given_before_the_frame_is_pending_and_saved() {
        use regent_core::sim::world::build::BuildOrder;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let mut sim = begun.sim;
        let menu = Menu::of(&catalogue);
        let local = sim.local();

        sim.step();

        let world = sim.world();
        let palace = world
            .palaces()
            .first()
            .and_then(|id| world.shown().iter().find(|s| s.agent == *id).map(|s| s.at))
            .expect("a palace");
        let code = world
            .build_menu(&menu, local)
            .iter()
            .find(|o| o.offered)
            .map(|o| o.code)
            .expect("something to build");
        let at = (-8..=8)
            .flat_map(|dx| (-8..=8).map(move |dy| (palace.0 + dx * 64, palace.1 + dy * 64)))
            .find(|at| sim.placement(&menu, code, *at) == Placement::Fits)
            .expect("a place to build");
        let order = Order {
            player: local,
            input: Input::Build(BuildOrder {
                code,
                at,
                owner: local,
            }),
        };

        sim.input(order.input);

        assert_eq!(sim.placement(&menu, code, at), Placement::Pending);
        assert_eq!(
            sim.snapshot().expect("snapshot").game.inputs,
            [(sim.order_frame(), order.to_string())]
        );
    }

    /// What `input::send` hands the game each drawn update besides its
    /// inputs, the reward panel's settings and the interface's lines,
    /// goes unrecorded: it reaches no world or VM state, so a game given
    /// them any number of times plays as one never given them. The
    /// settings are kept for a save, nothing else.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_rewards_and_told_lines_reach_no_play() {
        const FRAMES: u32 = 300;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let mut told = crate::game::begin::test_setup(&cli).0.sim;
        let mut quiet = crate::game::begin::test_setup(&cli).0.sim;

        for frame in 0..FRAMES {
            let n = i32::try_from(frame).expect("small");

            for _ in 0..frame % 3 {
                told.set_rewards(Rewards {
                    attack: n * 10,
                    explore: 500 - n,
                });
                told.say(chat::Line::Text(1));
            }

            told.step();
            quiet.step();
        }

        assert_ne!(told.rewards(), quiet.rewards());
        assert!(
            told.snapshot().expect("snapshot") == quiet.snapshot().expect("snapshot"),
            "the same world and VM"
        );
    }

    /// A replay's order for a frame already run stops the game: it would
    /// never be decoded, and play would part from the recording.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_late_order_stops_the_game() {
        const FRAMES: u32 = 10;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let mut sim = crate::game::begin::test_setup(&cli).0.sim;

        for _ in 0..FRAMES {
            sim.step();
        }

        sim.replay(vec![Event::Input {
            before: FRAMES,
            order: Input::Dismiss(0).into(),
        }]);
        sim.step();

        assert_eq!(sim.frames(), FRAMES);
        assert!(
            sim.stopped
                .as_ref()
                .is_some_and(|why| why.contains("after frame 10 ran"))
        );
    }

    /// Alone, CONTINUE PLAYING is the local player's, whichever it is:
    /// the solo peer's one seat is 0.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn play_on_alone_is_the_local_players() {
        let scratch = crate::testdata::scratch("sim-play-on");
        let dir = scratch.path().to_path_buf();
        let cli = crate::cli::tests::parsed(["openregent", "--players", "2"]);
        let mut sim = crate::game::begin::test_setup(&cli).0.sim;

        sim.set_local(1).expect("a human seat");
        crate::game::begin::Recording::In(dir).record(&mut sim, &cli.recorded(), Vec::new());
        sim.play_on();
        sim.step();
        sim.step();

        let players: Vec<u32> = sim
            .session()
            .expect("recorded")
            .events
            .iter()
            .filter_map(|e| match e {
                Event::PlayOn { player, .. } => Some(*player),
                _ => None,
            })
            .collect();

        assert_eq!(players, [1]);
    }

    /// The Random picks are named by the base game's `GMTX`, as the
    /// title scroll names a custom game.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_random_picks_are_named_by_the_games_texts() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let catalogue =
            regent_draw::data::catalogue(&archives(&hd).expect("the dataset")).expect("catalogue");
        let text = Text::build(&catalogue);
        let names = |list| -> Vec<String> {
            randoms(list, &text)
                .into_iter()
                .map(|(code, name)| format!("{code} {name}"))
                .collect()
        };

        assert_eq!(names(List::Land), ["RAND Random"]);
        assert_eq!(
            names(List::Monsters),
            [
                "BEGI Random (Beginner)",
                "ADVA Random (Advanced)",
                "EXPE Random (Expert)"
            ]
        );
    }

    /// A resync begins the host's faults afresh: the next summary counts
    /// from them, not back from the faults this game had before.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_resync_sums_losses_from_the_new_game() {
        let cli = crate::cli::tests::parsed(["openregent"]);
        let mut sim = crate::game::begin::test_setup(&cli).0.sim;

        // As though this game's host had taken defaults before.
        sim.said.summed.unknown = 5;

        let bytes = sim.payload(0).expect("a payload");

        sim.resync(sim.peer.epoch() + 1, &bytes).expect("resynced");

        assert_eq!(
            sim.said.summed,
            report::Losses::of(sim.world(), sim.game.faults())
        );
    }
}
