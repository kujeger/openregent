//! A session's setup and every input, recorded as they happen, so a played
//! game plays again (`--replay`): play is a function of setup, GPL seed
//! and input.
//!
//! ```text
//!  $XDG_DATA_HOME/openregent/play.input   (play.input.1 the previous)
//!
//!  # openregent session                   a comment
//!  setup (seed:190881324,...)           the game's setup (`crate::setup`),
//!                                         RON on one line
//!  input 121 p0 build Gld1 3200 4500 0  player 0's order, before frame 121
//!  play-on 412 p0                       player 0's CONTINUE PLAYING, after
//!                                         frame 412
//!  surrender 600 p1                     player 1 surrendered, after frame 600
//!  digest 500 3f2a9c0d1e4b5a67          Game::digest after frame 500
//! ```
//!
//! The end panel's CONTINUE PLAYING, pressed after real-time fades, is
//! input too. A track's end is not: its callback changes no play
//! (`Game::music_stopped`).
//!
//! Every [`DIGEST_EVERY`] frames the game's digest is written too, and a
//! replay compares its own with it: a replay that parts from its
//! recording stops at the first frame that differs. A recording without
//! digest lines replays unchecked.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::log::warn;
use regent_core::sim::game::Order;

use crate::setup::Setup;

/// The recording in the log's directory.
const FILE: &str = "play.input";

/// The previous session's recording.
const PREVIOUS: &str = "play.input.1";

/// The first line.
const HEADER: &str = "# openregent session: setup, then inputs by frame";

/// The setup's line: [`Setup`] as RON follows.
const SETUP: &str = "setup";

/// An order's line: its frame and [`Order`]'s text follow.
const INPUT: &str = "input";

/// CONTINUE PLAYING: the frames run before it and the player follow.
const PLAY_ON: &str = "play-on";

/// A surrender: the frames run before it and the player follow.
const SURRENDER: &str = "surrender";

/// The game's digest: the frame run and the digest, hex, follow.
const DIGEST: &str = "digest";

/// Frames between digests: 5 game seconds. A replay that parts is caught
/// within this many frames of it; a game hour adds 720 short lines.
/// [`Game::digest`](regent_core::sim::game::Game::digest) costs microseconds,
/// so its cost decides nothing. No deep digest: a part shows in the cheap
/// one's world or RNG soon after, and the replay has every frame to look.
pub const DIGEST_EVERY: u32 = 100;

/// Opens a play-on's player: `p0`, as an [`Order`]'s.
const PLAYER: char = 'p';

/// Marks a comment.
const COMMENT: char = '#';

/// A recorded line past the setup: what a player did, by frame, or the
/// game's digest, a replay's check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Taken before frame `before`.
    Input { before: u32, order: Order },
    /// `player`'s ended game went on after frame `after` ran: the end
    /// panel's CONTINUE PLAYING. An ended game runs no frame, so `after`
    /// is the end's.
    PlayOn { after: u32, player: u32 },
    /// `player` surrendered after frame `after` ran.
    Surrender { after: u32, player: u32 },
    /// [`Game::digest`](regent_core::sim::game::Game::digest) once frame
    /// `after` ran: no input, only checked.
    Digest { after: u32, digest: u64 },
}

impl Event {
    /// The frame it names.
    pub const fn frame(&self) -> u32 {
        match *self {
            Event::Input { before, .. } => before,
            Event::PlayOn { after, .. }
            | Event::Surrender { after, .. }
            | Event::Digest { after, .. } => after,
        }
    }

    fn line(&self) -> String {
        match self {
            Event::Input { before, order } => format!("{INPUT} {before} {order}"),
            Event::PlayOn { after, player } => format!("{PLAY_ON} {after} {PLAYER}{player}"),
            Event::Surrender { after, player } => {
                format!("{SURRENDER} {after} {PLAYER}{player}")
            }
            Event::Digest { after, digest } => format!("{DIGEST} {after} {digest:016x}"),
        }
    }
}

/// The frames `events` hold a digest after, in order.
pub fn digested(events: &[Event]) -> Vec<u32> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Digest { after, .. } => Some(*after),
            Event::Input { .. } | Event::PlayOn { .. } | Event::Surrender { .. } => None,
        })
        .collect()
}

/// A recorded session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub setup: Setup,
    pub events: Vec<Event>,
}

impl Session {
    /// As `play.input` holds it: the header, the setup, the events.
    pub fn text(&self) -> String {
        [HEADER.to_owned(), setup_line(&self.setup)]
            .into_iter()
            .chain(self.events.iter().map(Event::line))
            .map(|l| l + "\n")
            .collect()
    }
}

/// `setup`'s line. A setup that will not write (a path not UTF-8) is
/// left empty, and will not read back.
fn setup_line(setup: &Setup) -> String {
    let ron = ron::to_string(setup)
        .inspect_err(|e| warn!("setup not recorded: {e}"))
        .unwrap_or_default();

    format!("{SETUP} {ron}")
}

/// The session in `text`.
///
/// # Errors
///
/// A line that is no setup, input, CONTINUE PLAYING, surrender or
/// digest; a setup that does not read.
pub fn parse(text: &str) -> Result<Session, String> {
    let mut session = Session::default();

    for (n, line) in text.lines().map(str::trim).enumerate() {
        let shown = |e: String| format!("line {}: {e}", n + 1);
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let frame = |text: &str| {
            text.parse::<u32>()
                .map_err(|e| shown(format!("{text}: {e}")))
        };

        match word {
            "" => {}
            _ if word.starts_with(COMMENT) => {}
            INPUT => {
                let (before, order) = rest.split_once(' ').unwrap_or((rest, ""));
                let order = order.parse().map_err(shown)?;

                session.events.push(Event::Input {
                    before: frame(before)?,
                    order,
                });
            }
            PLAY_ON | SURRENDER => {
                let (after, player) = rest.split_once(' ').unwrap_or((rest, ""));
                let player = player
                    .strip_prefix(PLAYER)
                    .and_then(|n| n.parse().ok())
                    .ok_or_else(|| shown(format!("{player} is no player")))?;
                let after = frame(after)?;

                session.events.push(if word == PLAY_ON {
                    Event::PlayOn { after, player }
                } else {
                    Event::Surrender { after, player }
                });
            }
            DIGEST => {
                let (after, digest) = rest.split_once(' ').unwrap_or((rest, ""));
                let digest =
                    u64::from_str_radix(digest, 16).map_err(|e| shown(format!("{digest}: {e}")))?;

                session.events.push(Event::Digest {
                    after: frame(after)?,
                    digest,
                });
            }
            SETUP => {
                session.setup = ron::from_str(rest).map_err(|e| shown(format!("setup: {e}")))?;
            }
            _ => Err(shown(format!("{word}: no setup, input, play-on or digest")))?,
        }
    }

    Ok(session)
}

/// The session at `path`.
///
/// # Errors
///
/// A file that will not read or parse.
pub fn read(path: &Path) -> Result<Session, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;

    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a session as it is played; each line at once, so a crash keeps
/// it. Keeps it too, for a save.
pub struct Recorder {
    file: Option<File>,
    /// The file; its directory is where a replay that parts writes its
    /// snapshot.
    path: Option<PathBuf>,
    session: Session,
}

impl Recorder {
    /// A recording at `path` of `setup`: the file there before it moves to
    /// `previous`. With no file, a recorder writing nothing, said once.
    pub fn open(path: &Path, previous: &Path, setup: &Setup) -> Recorder {
        let _ = std::fs::rename(path, previous);

        let mut recorder = Recorder {
            file: File::create(path)
                .inspect_err(|e| warn!("no recording: {e}"))
                .ok(),
            path: Some(path.to_path_buf()),
            session: Session {
                setup: setup.clone(),
                events: Vec::new(),
            },
        };

        recorder.line(HEADER);
        recorder.line(&setup_line(setup));
        recorder
    }

    /// A recording at `path` going on from `session`, a saved game's:
    /// its setup and events written first, so the file replays from the
    /// game's start.
    pub fn resume(path: &Path, previous: &Path, session: &Session) -> Recorder {
        let mut recorder = Recorder::open(path, previous, &session.setup);

        for event in &session.events {
            recorder.event(event);
        }

        recorder
    }

    /// [`Recorder::resume`] to [`FILE`] in `dir`.
    pub fn in_dir(dir: &Path, session: &Session) -> Recorder {
        Recorder::resume(&dir.join(FILE), &dir.join(PREVIOUS), session)
    }

    /// [`Recorder::resume`] in the log's directory, else kept only.
    pub fn default_resumed(session: &Session) -> Recorder {
        match crate::files::log::dir() {
            Some(dir) => Recorder::in_dir(&dir, session),
            None => Recorder {
                file: None,
                path: None,
                session: session.clone(),
            },
        }
    }

    pub fn event(&mut self, event: &Event) {
        self.session.events.push(*event);
        self.line(&event.line());
    }

    /// The recording's directory.
    pub fn dir(&self) -> Option<&Path> {
        self.path.as_deref().and_then(Path::parent)
    }

    /// The recording begun again as `session`, a LAN resync's: the file
    /// written afresh, so it replays as `session` does, and goes on.
    pub fn restart(&mut self, session: &Session) {
        self.file = self.path.as_deref().and_then(|path| {
            File::create(path)
                .inspect_err(|e| warn!("no recording: {e}"))
                .ok()
        });
        self.session = Session {
            setup: session.setup.clone(),
            events: Vec::new(),
        };
        self.line(HEADER);
        self.line(&setup_line(&session.setup));

        for event in &session.events {
            self.event(event);
        }
    }

    /// The session so far.
    pub const fn session(&self) -> &Session {
        &self.session
    }

    fn line(&mut self, line: &str) {
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Where a recording goes by default.
pub fn default_path() -> Option<PathBuf> {
    crate::files::log::dir().map(|d| d.join(FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::fourcc::FourCc;
    use regent_core::sim::game::Input;
    use regent_core::sim::world::{
        CROWN,
        build::BuildOrder,
        defeat::End,
        flag::Flag,
        offer::{EXPLORE_FLAG, Menu, Placement},
    };

    #[test]
    fn a_session_reads_back_from_its_lines() {
        let events = vec![
            Event::Input {
                before: 121,
                order: Input::Unpreview(FourCc::new(b"Gld1")).into(),
            },
            Event::PlayOn {
                after: 350,
                player: 2,
            },
            Event::Surrender {
                after: 380,
                player: 1,
            },
            Event::Input {
                before: 400,
                order: Order {
                    player: 1,
                    input: Input::Upgrade(7),
                },
            },
        ];
        let setup = Setup {
            scene: Some("/a path/with spaces.toml".into()),
            mods: vec![
                crate::setup::Wanted::parse("{D666FCBE-9906-4582-A6A0-DC96F812441E}@00ff")
                    .expect("a mod"),
            ],
            limits: Some([(FourCc::new(b"ABB1"), 0)].into_iter().collect()),
            ..Setup::default()
        };
        let text = Session {
            setup: setup.clone(),
            events: events.clone(),
        }
        .text();
        let session = parse(&text).expect("parses");

        assert_eq!(session.events, events);
        assert_eq!(session.setup, setup);
        assert!(text.contains("play-on 350 p2\nsurrender 380 p1\ninput 400 p1 upgrade 7\n"));
        assert!(parse("seed 0x0b609e2c\n").is_err());
    }

    /// The world as a replay must match it: every piece as drawn, the
    /// treasury, the clock, the script errors.
    fn state(sim: &crate::game::sim::Sim) -> impl PartialEq + std::fmt::Debug {
        let world = sim.world();

        (
            world.shown(),
            world.treasury(CROWN),
            world.now(),
            sim.reports().count,
            sim.stopped.clone(),
        )
    }

    /// The events a player gave, digests left out.
    fn given(session: &Session) -> usize {
        session
            .events
            .iter()
            .filter(|e| !matches!(e, Event::Digest { .. }))
            .count()
    }

    /// A fresh scratch folder of its own.
    fn scratch(name: &str) -> crate::testdata::Scratch {
        crate::testdata::scratch(&format!("record-{name}"))
    }

    /// A new game played with a building and an explore flag, recorded,
    /// replays from its file to the same world.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_recorded_session_replays_to_the_same_world() {
        const FRAMES: u32 = 400;
        const BUILD_AT: u32 = 40;
        const FLAG_AT: u32 = 120;
        const REWARD: i32 = 200;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let dir = scratch("replays");
        let path = dir.join(FILE);

        std::fs::create_dir_all(&dir).expect("scratch");

        // Played: a building where it fits near the palace, then a flag.
        let (begun, catalogue) = crate::game::begin::test_setup(&cli);
        let mut played = begun.sim;
        let program = played.program();
        let menu = Menu::of(&catalogue);

        played.record(Recorder::open(&path, &dir.join(PREVIOUS), &cli.recorded()));

        for frame in 0..FRAMES {
            let world = played.world();
            let palace = world
                .palaces()
                .first()
                .and_then(|id| world.shown().iter().find(|s| s.agent == *id).map(|s| s.at))
                .expect("a palace");

            if frame == BUILD_AT {
                let code = world
                    .build_menu(&menu, CROWN)
                    .iter()
                    .find(|o| o.offered)
                    .map(|o| o.code)
                    .expect("something to build");
                let at = (-8..=8)
                    .flat_map(|dx| (-8..=8).map(move |dy| (dx, dy)))
                    .map(|(dx, dy)| (palace.0 + dx * 64, palace.1 + dy * 64))
                    .find(|at| world.placement(program, &menu, CROWN, code, *at) == Placement::Fits)
                    .expect("a place to build");

                played.input(Input::Unpreview(code));
                played.input(Input::Build(BuildOrder {
                    code,
                    at,
                    owner: CROWN,
                }));
            }

            if frame == FLAG_AT {
                played.input(Input::Flag(Flag {
                    code: EXPLORE_FLAG,
                    at: (palace.0 + 320, palace.1),
                    owner: CROWN,
                    reward: REWARD,
                    target: 0,
                }));
            }

            played.step();
        }

        // Replayed from the file.
        let session = read(&path).expect("recorded");
        let setup = session.setup.clone();
        let mut replayed = crate::game::begin::test_setup(&setup).0.sim;
        let again = dir.join("again");

        std::fs::create_dir_all(&again).expect("scratch");
        replayed.record(Recorder::in_dir(
            &again,
            &Session {
                setup: session.setup.clone(),
                events: Vec::new(),
            },
        ));
        replayed.replay(session.events.clone());

        for _ in 0..FRAMES {
            replayed.step();
        }

        let rerecorded = std::fs::read_to_string(again.join(FILE));

        // The input took: the flag stands.
        let flags = |sim: &crate::game::sim::Sim| {
            let shown = sim.world().shown();

            shown.iter().filter(|s| s.code == EXPLORE_FLAG).count()
        };

        assert_eq!(flags(&played), 1);
        assert_eq!(given(&session), 3);
        assert_eq!(
            session.events.len() - 3,
            (FRAMES / DIGEST_EVERY) as usize,
            "a digest every {DIGEST_EVERY} frames"
        );
        assert_eq!(
            rerecorded.ok(),
            Some(session.text()),
            "a replay records itself"
        );
        assert_eq!(session.setup, cli.recorded());
        assert!(!replayed.replaying());
        assert_eq!(state(&replayed), state(&played));
    }

    /// An order given between frames is recorded before the next and
    /// stands once that frame runs, as the engine takes a click in its
    /// pacing wait (**Measured**).
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_order_between_frames_is_recorded_before_the_next() {
        const FRAMES: u32 = 40;
        const REWARD: i32 = 100;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let dir = scratch("between");
        let mut sim = crate::game::begin::test_setup(&cli).0.sim;

        std::fs::create_dir_all(&dir).expect("scratch");
        sim.record(Recorder::open(
            &dir.join(FILE),
            &dir.join(PREVIOUS),
            &cli.recorded(),
        ));

        for _ in 0..FRAMES {
            sim.step();
        }

        let world = sim.world();
        let palace = world
            .palaces()
            .first()
            .and_then(|id| world.shown().iter().find(|s| s.agent == *id).map(|s| s.at))
            .expect("a palace");
        let flags = |sim: &crate::game::sim::Sim| {
            let shown = sim.world().shown();

            shown.iter().filter(|s| s.code == EXPLORE_FLAG).count()
        };

        sim.input(Input::Flag(Flag {
            code: EXPLORE_FLAG,
            at: (palace.0 + 320, palace.1),
            owner: CROWN,
            reward: REWARD,
            target: 0,
        }));

        let recorded = sim.session().map(|s| s.events.clone());
        let unmade = flags(&sim);

        sim.step();

        assert_eq!(
            recorded.and_then(|e| e.first().map(Event::frame)),
            Some(FRAMES + 1)
        );
        assert_eq!(unmade, 0);
        assert_eq!(flags(&sim), 1);
    }

    /// A game won, held at its end for any number of drawn updates as the
    /// curtain fades, then played on and flagged, replays from its file
    /// to the same world: CONTINUE PLAYING is recorded.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_game_played_on_after_its_end_replays() {
        use regent_core::sim::world::cheat::{Cheat, CheatCode};

        const WIN_AT: u32 = 20;
        const HELD: u32 = 37;
        const FRAMES: u32 = 120;
        const REWARD: i32 = 100;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let dir = scratch("played_on");
        let path = dir.join(FILE);

        std::fs::create_dir_all(&dir).expect("scratch");

        let mut played = crate::game::begin::test_setup(&cli).0.sim;

        played.record(Recorder::open(&path, &dir.join(PREVIOUS), &cli.recorded()));

        for _ in 0..WIN_AT {
            played.step();
        }

        played.input(Input::Cheat(CheatCode {
            cheat: Cheat::Win,
            arg: CROWN,
        }));
        played.step();

        // The end stands still while the curtain fades in real time.
        for _ in 0..HELD {
            played.step();
        }

        let ended_at = played.frames();

        assert!(played.ended(), "won");

        played.play_on();

        let world = played.world();
        let palace = world
            .palaces()
            .first()
            .and_then(|id| world.shown().iter().find(|s| s.agent == *id).map(|s| s.at))
            .expect("a palace");

        played.input(Input::Flag(Flag {
            code: EXPLORE_FLAG,
            at: (palace.0 + 320, palace.1),
            owner: CROWN,
            reward: REWARD,
            target: 0,
        }));

        while played.frames() < ended_at + FRAMES {
            played.step();
        }

        let session = read(&path).expect("recorded");
        let setup = session.setup.clone();
        let mut replayed = crate::game::begin::test_setup(&setup).0.sim;

        replayed.replay(session.events.clone());

        // Held at the end for fewer updates; a stand-still bounded.
        for _ in 0..WIN_AT + 1 + HELD + FRAMES {
            if replayed.frames() < played.frames() {
                replayed.step();
            }
        }

        assert!(!played.ended(), "played on");
        assert_eq!(replayed.frames(), played.frames());
        assert!(!replayed.replaying());
        assert_eq!(state(&replayed), state(&played));
    }

    /// A new game on Building Limitations records its caps with its
    /// setup and replays from its file to the same world, digests
    /// checked: the library banned, one wizards' guild at most.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_game_on_building_limits_replays_with_its_caps() {
        const FRAMES: u32 = 300;
        const CAPS: &str = "ABG1=0,ABY1=1";
        const LIBRARY: FourCc = FourCc(*b"ABG1");

        let cli = crate::cli::tests::parsed(["openregent", "--limits", CAPS]);
        let dir = scratch("limits");
        let path = dir.join(FILE);

        std::fs::create_dir_all(&dir).expect("scratch");

        let mut played = crate::game::begin::test_setup(&cli).0.sim;

        played.record(Recorder::open(&path, &dir.join(PREVIOUS), &cli.recorded()));

        assert_eq!(played.world().caps().len(), 2);
        assert!(!played.world().under_cap(CROWN, LIBRARY));

        for _ in 0..FRAMES {
            played.step();
        }

        let session = read(&path).expect("recorded");
        let setup = session.setup.clone();
        let mut replayed = crate::game::begin::test_setup(&setup).0.sim;

        replayed.replay(session.events.clone());

        for _ in 0..FRAMES {
            replayed.step();
        }

        assert!(session.setup.limits.as_ref().is_some_and(|c| c.len() == 2));
        assert_eq!(session.setup.limits, cli.limits);
        assert_eq!(replayed.world().caps(), played.world().caps());
        assert!(replayed.stopped.is_none(), "{:?}", replayed.stopped);
        assert_eq!(state(&replayed), state(&played));
    }

    /// A recording replayed with one digest changed, or one input left
    /// out, stops once the first digest after it is due, naming that
    /// frame and the last that matched, its snapshot beside the replay's
    /// recording.
    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn a_replay_that_parts_stops_at_the_digest() {
        const FRAMES: u32 = 300;
        const FLAG_AT: u32 = 120;
        const PARTED: u32 = 200;
        const REWARD: i32 = 100;

        let cli = crate::cli::tests::parsed(["openregent"]);
        let dir = scratch("parts");
        let path = dir.join(FILE);
        let begun = || crate::game::begin::test_setup(&cli).0.sim;

        std::fs::create_dir_all(&dir).expect("scratch");

        let mut played = begun();

        played.record(Recorder::open(&path, &dir.join(PREVIOUS), &cli.recorded()));

        for frame in 0..FRAMES {
            if frame == FLAG_AT {
                let world = played.world();
                let palace = world
                    .palaces()
                    .first()
                    .and_then(|id| world.shown().iter().find(|s| s.agent == *id).map(|s| s.at))
                    .expect("a palace");

                played.input(Input::Flag(Flag {
                    code: EXPLORE_FLAG,
                    at: (palace.0 + 320, palace.1),
                    owner: CROWN,
                    reward: REWARD,
                    target: 0,
                }));
            }

            played.step();
        }

        let session = read(&path).expect("recorded");

        // Replayed from `events`, recorded in its own directory: why it
        // stopped, its frames, and whether a snapshot was written.
        let replay = |name: &str, events: Vec<Event>| {
            let again = dir.join(name);
            let mut sim = begun();

            std::fs::create_dir_all(&again).expect("scratch");
            sim.record(Recorder::in_dir(&again, &Session::default()));
            sim.replay(events);

            for _ in 0..FRAMES {
                sim.step();
            }

            let dumped = again.join(format!("replay-frame{PARTED}.ron")).exists();

            (sim.stopped.clone(), sim.frames(), dumped)
        };

        let tampered = session
            .events
            .iter()
            .map(|e| match *e {
                Event::Digest { after, digest } if after == PARTED => Event::Digest {
                    after,
                    digest: !digest,
                },
                e => e,
            })
            .collect();
        let unflagged = session
            .events
            .iter()
            .filter(|e| !matches!(e, Event::Input { .. }))
            .copied()
            .collect();
        let tampered = replay("tampered", tampered);
        let unflagged = replay("unflagged", unflagged);

        for (stopped, frames, dumped) in [tampered, unflagged] {
            let why = stopped.expect("stopped");

            assert!(
                why.starts_with(&format!("replay: frame {PARTED}'s digest")),
                "{why}"
            );
            assert!(why.ends_with("last match frame 100"), "{why}");
            assert_eq!(frames, PARTED);
            assert!(dumped, "a snapshot");
        }
    }

    /// Digest lines read back; a recording without them still parses.
    #[test]
    fn a_digest_reads_back_from_its_line() {
        let event = Event::Digest {
            after: 500,
            digest: 0x0123_4567_89ab_cdef,
        };
        let session = parse(&format!("{}\n", event.line())).expect("parses");

        assert_eq!(event.line(), "digest 500 0123456789abcdef");
        assert_eq!(session.events, [event]);
        assert!(parse("digest 500 xyz\n").is_err());
    }

    #[test]
    fn a_bad_input_names_its_line() {
        let e = parse("# a comment\ninput 5 dance\n").expect_err("no such input");

        assert!(e.starts_with("line 2:"), "{e}");
    }

    /// A test that `assets/input_logs/<name>.input`, a played quest
    /// recorded by hand, replays to every recorded digest:
    /// `replay_test!(the_siege)` is `replays::the_siege`.
    macro_rules! replay_test {
        ($name:ident) => {
            #[test]
            #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
            fn $name() {
                super::replays_to_digests(concat!(stringify!($name), ".input"));
            }
        };
    }

    /// One test a log in `assets/input_logs/`.
    mod replays {
        replay_test!(the_forsaken_land);
        replay_test!(urban_renewal);
        replay_test!(the_siege);
        replay_test!(day_of_reckoning);
    }

    /// Frames after a recording's last line within which its quest is
    /// won: one digest interval. The five recorded quests win 2 to 63
    /// frames after it.
    const WON_WITHIN: u32 = DIGEST_EVERY;

    /// Replays `assets/input_logs/{name}` to its last digest, then checks
    /// that the quest is won within [`WON_WITHIN`] frames.
    fn replays_to_digests(name: &str) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/input_logs")
            .join(name);
        let session = read(&path).expect("the recording");
        let last = session
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Digest { after, .. } => Some(*after),
                Event::Input { .. } | Event::PlayOn { .. } | Event::Surrender { .. } => None,
            })
            .max()
            .expect("digests to check");
        let mut sim = crate::game::begin::test_setup(&session.setup).0.sim;

        sim.replay(session.events.clone());

        // A digest that differs stops the game, naming the frame. Past the
        // last digest, the inputs after it (The Siege: one at 54319).
        while (sim.frames() < last || sim.replaying()) && sim.stopped.is_none() {
            sim.step();
        }

        assert_eq!(sim.stopped, None);
        assert!(!sim.replaying());
        assert_eq!(sim.matched(), Some(last));

        // The recording stops where the player won: the win follows soon.
        let done = sim.frames();
        while sim.world().ended(0).is_none() && sim.frames() < done + WON_WITHIN {
            sim.step();
        }

        assert_eq!(
            sim.world().ended(0),
            Some(End::Won),
            "won within {WON_WITHIN} frames of {done}"
        );
    }
}
