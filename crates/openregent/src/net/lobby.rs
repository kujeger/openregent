//! Before the game: the host waits for a guest a seat, checks it runs
//! the same build on the same data, shows every guest the session's
//! name, setup, seats and chat, and once every guest is ready and the
//! host starts, welcomes them all with the game's setup and every
//! seat's nick. Both ends are polled ([`Hosting`], [`Joining`]), so the
//! menus wait without blocking; the command line's wrappers block on
//! them, the host starting as every seat is taken.
//!
//! ```text
//!  guest                                  host (seat 0)
//!  Hello(build, nick, ready, chat held,   same build? a seat free?
//!        own chat) ── every 250 ms ──>
//!       <── Lobby(seat, players, title,   yes: its seat, else the next;
//!           revision, setup, seats,       its chat lines taken
//!           own chat taken, chat)
//!       <── Refused(why)                  no; or the host gave up
//!  each second, the lobby announced to the LAN (`discover`)
//!  a guest silent GONE_AFTER is let go, the seats after it moving up;
//!  a setup changed counts a revision, and a guest's ready lapses
//! .. every seat taken, every guest ready, the host starts:
//!       <── Welcome(seat, players, delay, setup, nicks), until the
//!           guest's first game packet (`node`)
//! ```
//!
//! A LAN save the host loads ([`Hosting::choose`]) is offered in the
//! lobby by its game's hash; a guest with a copy of its own says so, one
//! without is sent the host's (`transfer`), into its profile's LAN saves:
//!
//! ```text
//!  Hello(.., want game from chunk n) ──>  ──> Chunk × WINDOW from n
//!  Hello(.., have game) ──>               every guest has it, ready:
//!                                         startable; each seat the saved
//!                                         seat its nick had, unless moved
//!       <── Welcome(.., saved seat, host's seat, game)
//! ```
//!
//! A setup naming mods is played only once every seat holds each, its
//! files hashed alike (`files::mod_hash`): each guest says its hash of
//! each, the host marks every seat's match in the lobby.
//!
//! ```text
//!  Hello(.., mods: (GUID, hash?)...) ──>  each seat's match of each mod:
//!       <── Lobby(.., titles, seats       same, missing, differs, unknown;
//!           with each mod's match)        startable when all are same
//! ```
//!
//! After a game, each peer may go back to the lobby on its game's socket
//! ([`super::Regroup`], the end panel's Return to Lobby):
//!
//! ```text
//!  a guest first: Bye to the host's game; Hello ──> Wait, while the host
//!                 plays on (no timeout runs)
//!  the host:      Regroup × 3 to each guest in the game; hosting again,
//!                 those and the ones waiting seated first, by seat
//!       <── Regroup, again, to a seated guest's game packet
//!  each guest:    Hello ──> Lobby, as at the start; one gone stays gone
//! ```

use std::collections::VecDeque;
use std::io::ErrorKind;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::log::{info, warn};
use regent_core::content::dataset::{self, Bytecode, Datasets};
use regent_core::data::fnv;

use super::delay::{self, Delay};
use super::discover::{Beacon, Targets};
use super::node::Back;
use super::transfer::{Incoming, Outgoing, WINDOW};
use super::wire::{
    Build, Hello, Holding, Lines, MOST_BYTES, ModMatch, Offer, Open, Packet, Profile, Ready, Room,
    Seat, Talk, Welcome,
};
use crate::files::env::Install;
use crate::files::mod_hash::Holdings;
use crate::setup::Setup;
use regent_core::data::guid::Guid;

/// The port a host listens on unless told: this game's own pick.
pub const PORT: u16 = 47_101;

/// [`PORT`] as the command line spells it.
pub const PORT_TEXT: &str = "47101";

/// A guest's hello again this often, until welcomed.
const HELLO_EVERY: Duration = Duration::from_millis(250);
const PING_EVERY: Duration = Duration::from_secs(1);
const PING_STALE: Duration = Duration::from_secs(3);

/// A guest gives up on a host that answers nothing this long.
const JOIN_TIMEOUT: Duration = Duration::from_secs(60);

/// A host lets go a seated guest silent this long: eight hellos missed.
const GONE_AFTER: Duration = Duration::from_secs(2);

/// How long the command line's host waits, once every seat is taken,
/// for its guests' pings to pick a delay from: [`delay::LEAST_SAMPLES`]
/// pings a second apart are in by then. Past it, the default stands.
const SAMPLE_WAIT: Duration = Duration::from_secs(3);

/// A blocking lobby's nap between polls.
const NAP: Duration = Duration::from_millis(10);

/// Packets read at most in one poll.
const MOST_READS: usize = 64;

/// Chat lines a lobby packet carries at most, each way.
const CHAT_LINES: usize = 16;

/// A nick's characters at most.
pub const NICK_MOST: usize = 32;

/// Why a guest is refused once every seat is taken.
const FULL: &str = "every seat is taken";

/// Why a guest is refused whose nick another player has.
const NICK_TAKEN: &str = "another player has that nick";

/// Where a lobby is announced (`discover`): the local network; in
/// tests, nowhere unless told (`Hosting::announce_to`), so no test
/// game shows on the LAN.
const ANNOUNCED: Targets = if cfg!(test) {
    Targets::Given(Vec::new())
} else {
    Targets::Broadcast
};

/// What a host that gives up tells its guests.
pub(super) const HOST_LEFT: &str = "the host left";

/// Times a host that gives up tells each guest: one lost leaves none
/// waiting (as the game's `node::BYE_COPIES`).
const LEAVE_COPIES: usize = 3;

/// `bytes` folded: FNV-1a's start and prime over little-endian words,
/// the tail padded; not FNV-1a itself, which folds a byte at a time.
fn fold(mut h: u64, bytes: &[u8]) -> u64 {
    for chunk in bytes.chunks(8) {
        let mut word = [0; 8];

        word[..chunk.len()].copy_from_slice(chunk);
        h = (h ^ u64::from_le_bytes(word)).wrapping_mul(fnv::PRIME);
    }

    h
}

/// This build, on `install`'s data.
///
/// # Errors
///
/// A data file that will not read.
pub fn build(install: &Install) -> Result<Build, String> {
    Ok(Build {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        profile: Profile::this(),
        commit: env!("MAJ_COMMIT").to_owned(),
        data: data_hash(&install.hd, &install.sdk)?,
    })
}

/// The data a freestyle game reads, hashed: the GPL images' and the
/// freestyle quest's bytes, each archive's name and length. The
/// archives' bytes are not read, for time (**Inferred**: an archive
/// edited to the same length is rare).
fn data_hash(hd: &Path, sdk: &Path) -> Result<u64, String> {
    let files = dataset::Install::new(hd, Bytecode::Sdk(sdk))
        .files(Datasets::Expansion)
        .map_err(|e| e.to_string())?;
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
    let quest = regent_core::content::on_disk(&hd.join(crate::files::env::QUEST));
    let mut h = fnv::START;

    for path in files.programs.iter().chain([&files.chart]).chain(&quest) {
        h = fold(h, &read(path)?);
    }

    for path in &files.archives {
        let len = std::fs::metadata(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .len();
        // One spelling on every system: `/` between parts, lower case
        // (Windows finds `FOO.CAM` as the definitions' `Foo.cam`; Linux
        // falls back to the disk's spelling).
        let name = path
            .strip_prefix(hd)
            .unwrap_or(path)
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
            .to_ascii_lowercase();

        h = fold(fold(h, name.as_bytes()), &len.to_le_bytes());
    }

    Ok(h)
}

/// Whether to play with a peer built from another commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builds {
    /// The same commit only, on any system.
    Same,
    /// Any, of the same version and profile.
    Any,
}

/// What a peer told of differing game data sees: warned, not refused.
pub const DATA_DIFFER: &str = "the game data differ: the game may desync";

/// [`DATA_DIFFER`] if `theirs` reads other game data than `ours`.
pub fn data_warning(ours: &Build, theirs: &Build) -> Option<&'static str> {
    (ours.data != theirs.data).then_some(DATA_DIFFER)
}

/// Why `theirs` may not play with `ours`; `None` if it may. Differing
/// data does not refuse ([`data_warning`]).
pub fn mismatch(ours: &Build, theirs: &Build, builds: Builds) -> Option<String> {
    if ours.version != theirs.version {
        Some(format!(
            "version {} here, {} there",
            ours.version, theirs.version
        ))
    } else if ours.profile != theirs.profile {
        Some(format!(
            "a {:?} build here, {:?} there: debug assertions differ",
            ours.profile, theirs.profile
        ))
    } else if ours.commit != theirs.commit && builds == Builds::Same {
        Some(format!(
            "commit {} here, {} there (the host may allow it: --net-any-build)",
            ours.commit, theirs.commit
        ))
    } else {
        None
    }
}

/// A host's socket, bound, before its guests are in.
pub struct Lobby {
    socket: UdpSocket,
}

/// The guests in, and what the host's game needs to begin.
pub struct Seated {
    pub socket: UdpSocket,
    /// Each guest: address, seat, and the welcome it is sent.
    pub guests: Vec<(SocketAddr, u32, Vec<u8>)>,
    pub players: u32,
    /// The delay the game starts with.
    pub delay: u32,
    /// Whether the host steers it in play.
    pub mode: Delay,
    /// Each seat's nick, by seat.
    pub nicks: Vec<String>,
    /// The host's seat: 0 but in a loaded game.
    pub seat: u32,
    /// The save the game goes on from.
    pub save: Option<std::path::PathBuf>,
    /// The host's build and what it lets play: a guest that rejoins is
    /// held to it.
    pub build: (Build, Builds),
}

/// A guest seated: where it is, its nick and build, whether ready, when
/// last heard, its chat lines taken, and what it holds of the save.
struct Guest {
    addr: SocketAddr,
    place: Place,
    nick: String,
    build: String,
    ready: Ready,
    heard: Instant,
    taken: u32,
    holding: Holding,
    /// Its hash of each mod it was asked of; `None`, not held.
    mods: Vec<(Guid, Option<String>)>,
    probe: Probe,
}

/// Where a seated guest is: in the lobby, or still in the game after a
/// regroup, where it answers no ping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Place {
    Lobby,
    Game,
}

/// One link's answered probe: a lobby's, and the game's (`node`).
#[derive(Default)]
pub(super) struct Probe {
    next: u32,
    started: Option<Instant>,
    sent: Option<(u32, Instant)>,
    answer: Option<(Duration, Instant)>,
    /// The latest round trips, oldest first, at most [`delay::SAMPLES`].
    recent: VecDeque<Duration>,
}

impl Probe {
    pub(super) fn ask(&mut self, now: Instant) -> Option<Packet> {
        let due = self
            .sent
            .is_none_or(|(_, at)| now.saturating_duration_since(at) >= PING_EVERY);

        due.then(|| {
            self.started.get_or_insert(now);
            self.next = self.next.wrapping_add(1);
            self.sent = Some((self.next, now));
            Packet::Ping(self.next)
        })
    }

    pub(super) fn answered(&mut self, id: u32, now: Instant) {
        if let Some((_, at)) = self.sent.filter(|(sent, _)| *sent == id) {
            let trip = now.saturating_duration_since(at);

            self.answer = Some((trip, now));
            self.record(trip);
        }
    }

    /// `trip` joins the window of latest round trips.
    pub(super) fn record(&mut self, trip: Duration) {
        self.recent.push_back(trip);

        while self.recent.len() > delay::SAMPLES {
            self.recent.pop_front();
        }
    }

    /// The latest round trips, oldest first.
    pub(super) fn samples(&self) -> Vec<Duration> {
        self.recent.iter().copied().collect()
    }

    pub(super) fn ping(&self, now: Instant) -> Option<Duration> {
        self.answer
            .filter(|(_, at)| now.saturating_duration_since(*at) < PING_STALE)
            .map(|(ping, _)| ping)
    }

    fn stale(&self, now: Instant) -> bool {
        let latest = self.answer.map_or(self.started, |(_, at)| Some(at));

        latest.is_some_and(|at| now.saturating_duration_since(at) >= PING_STALE)
    }
}

/// The save the host loads: as offered, its file to send, where it is,
/// and the lobby's nicks its seats were last fitted to.
struct Chosen {
    offer: Offer,
    sent: Outgoing,
    path: std::path::PathBuf,
    fitted: Vec<String>,
}

/// A LAN save the host loads, read by the caller: net reads no save.
pub struct Loaded {
    pub path: std::path::PathBuf,
    /// The file, as sent to a guest that lacks it.
    pub bytes: Vec<u8>,
    pub name: String,
    /// Its game's hash, as its meta names it.
    pub game: String,
    /// Each saved seat's nick.
    pub seats: Vec<String>,
    /// Its recording's setup.
    pub setup: Setup,
}

/// Where a guest keeps the saves a host loads, the profile's LAN saves:
/// the caller's, as net reads no save.
pub trait Saves: Send + Sync {
    /// Where they are kept: a test's.
    #[cfg(test)]
    fn dir(&self) -> &Path;

    /// The save whose meta names game `game`.
    fn find(&self, game: &str) -> Option<std::path::PathBuf>;

    /// A save sent, `bytes`, kept as `name` or one free, and read back:
    /// its path.
    ///
    /// # Errors
    ///
    /// A file that will not write, or not of game `game`.
    fn keep(&self, name: &str, game: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String>;
}

/// Why the host's game may not start yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unready {
    /// A seat is free.
    Seats,
    /// A guest is not ready for the setup as it stands.
    Guests,
    /// A guest lacks the save the host loads.
    Save,
    /// A seat lacks a mod the setup names, or holds another version.
    Mods,
}

/// A host's lobby, polled: each guest seated as its hello comes, the
/// host seat 0; the session's name, setup and chat shown to every guest.
pub struct Hosting {
    socket: UdpSocket,
    build: (Build, Builds),
    players: u32,
    delay: Delay,
    /// The delay the game would start with now: told, or picked from
    /// the guests' pings at the last poll.
    picked: u32,
    setup: Setup,
    /// Counts each change of the setup: a guest's ready lapses with it.
    revision: u32,
    /// The session's name.
    title: String,
    /// The host's nick.
    nick: String,
    /// Each guest's seat is its place here, plus one.
    guests: Vec<Guest>,
    chat: Vec<Talk>,
    /// The save the game goes on from; none, a new game.
    chosen: Option<Chosen>,
    /// Announces the lobby to LAN discovery; none where its socket
    /// will not bind.
    beacon: Option<Beacon>,
    /// The host's mods, hashed; none until hashed.
    holdings: Option<Holdings>,
}

impl Lobby {
    /// A lobby on `port` of every interface; 0 picks one.
    ///
    /// # Errors
    ///
    /// A port that will not bind.
    pub fn bind(port: u16) -> Result<Lobby, String> {
        let socket = UdpSocket::bind(("0.0.0.0", port)).map_err(|e| format!("port {port}: {e}"))?;

        Ok(Lobby { socket })
    }

    /// A lobby reached only through an internet bridge.
    pub fn bind_local() -> Result<Lobby, String> {
        let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;

        Ok(Lobby { socket })
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.socket.local_addr().map_or(0, |a| a.port())
    }

    /// Wait for `players - 1` guests running `build`, the host seat 0
    /// and named `nick`, and welcome each to the game of `setup`, orders
    /// `delay` frames ahead, as soon as every seat is taken, ready or
    /// not, and every seat holds the setup's mods as `holdings` does.
    /// Blocks: the command line's. An automatic delay waits up to
    /// [`SAMPLE_WAIT`] after the last seat for every guest's pings,
    /// else the default stands ([`Delay::pick`]). A mod that does not
    /// match is logged, once each change, and waited on.
    ///
    /// # Errors
    ///
    /// A socket that will not be made non-blocking.
    pub fn seat(
        self,
        build: (&Build, Builds),
        (players, delay): (u32, Delay),
        (setup, nick): (&Setup, &str),
        holdings: &Holdings,
    ) -> Result<Seated, String> {
        let mut hosting = self.host(build, (players, delay), (setup, nick))?;
        let mut full = None;
        let mut start = false;
        let mut said = Vec::new();

        hosting.set_holdings(holdings);

        // Full at `full`; started once the pings are in, or SAMPLE_WAIT
        // on, and the mods match.
        while !start {
            let now = Instant::now();

            if hosting.poll(now) {
                let since = now.saturating_duration_since(*full.get_or_insert(now));
                let problems = hosting.mod_problems();

                if problems != said {
                    for problem in &problems {
                        warn!("net: waiting on a mod: {problem}");
                    }

                    said = problems;
                }

                start = (hosting.sampled() || since >= SAMPLE_WAIT) && said.is_empty();
            }

            if !start {
                std::thread::sleep(NAP);
            }
        }

        Ok(hosting.seat())
    }

    /// The lobby, polled from here on ([`Hosting::poll`]).
    ///
    /// # Errors
    ///
    /// A socket that will not be made non-blocking.
    pub fn host(
        self,
        (build, builds): (&Build, Builds),
        (players, delay): (u32, Delay),
        (setup, nick): (&Setup, &str),
    ) -> Result<Hosting, String> {
        let port = self.port();

        self.socket
            .set_nonblocking(true)
            .map_err(|e| e.to_string())?;
        info!(
            "net: hosting on port {port}, waiting for {} players",
            players - 1
        );
        let local = self
            .socket
            .local_addr()
            .is_ok_and(|at| at.ip().is_loopback());

        Ok(Hosting {
            socket: self.socket,
            build: (build.clone(), builds),
            players,
            delay,
            picked: super::DELAY,
            setup: setup.clone(),
            revision: 0,
            title: String::new(),
            nick: nick_of(nick),
            guests: Vec::new(),
            chat: Vec::new(),
            chosen: None,
            beacon: if local {
                None
            } else {
                Beacon::new(ANNOUNCED)
                    .inspect_err(|why| warn!("net: not announced to LAN discovery: {why}"))
                    .ok()
            },
            holdings: None,
        })
    }
}

impl Hosting {
    /// The socket address of a seated guest, for lobby connection health.
    pub fn guest_addr(&self, seat: usize) -> Option<SocketAddr> {
        self.guests
            .get(seat.checked_sub(1)?)
            .map(|guest| guest.addr)
    }

    /// Each guest's latest round trips.
    fn trips(&self) -> Vec<Vec<Duration>> {
        self.guests.iter().map(|g| g.probe.samples()).collect()
    }

    /// Whether the delay has what it is picked from ([`Delay::sampled`]).
    pub fn sampled(&self) -> bool {
        self.delay.sampled(&self.trips())
    }

    /// Seat `seat`'s guest answered a ping in `trip`: a test's.
    #[cfg(test)]
    pub fn sample(&mut self, seat: usize, trip: Duration) {
        if let Some(guest) = self.guests.get_mut(seat - 1) {
            guest.probe.record(trip);
        }
    }

    pub fn ping(&self, seat: usize, now: Instant) -> Option<Duration> {
        self.guests.get(seat.checked_sub(1)?)?.probe.ping(now)
    }

    /// Whether seat `seat`'s guest has answered no ping for a while; one
    /// still in the game is asked none.
    pub fn ping_stale(&self, seat: usize, now: Instant) -> bool {
        self.guests
            .get(seat.saturating_sub(1))
            .is_some_and(|guest| guest.place == Place::Lobby && guest.probe.stale(now))
    }

    pub fn identities(&self) -> Vec<(SocketAddr, String)> {
        self.guests
            .iter()
            .map(|guest| (guest.addr, guest.nick.clone()))
            .collect()
    }

    /// The port it listens on.
    pub fn port(&self) -> u16 {
        self.socket.local_addr().map_or(0, |a| a.port())
    }

    /// The session named `title`, as every guest's lobby shows it.
    pub fn set_title(&mut self, title: &str) {
        title.clone_into(&mut self.title);
    }

    /// The game's setup now `setup`: a change counts a revision, so each
    /// guest's ready lapses, and says so in the chat, as the original's
    /// lobby does as the options come.
    /// **Inferred**: the original's host gets no options of its own, so
    /// may not say it; here every peer does.
    pub fn set_setup(&mut self, setup: &Setup) {
        if self.setup != *setup {
            self.setup = setup.clone();
            self.revise();
        }
    }

    /// The host's mods, hashed: each guest's are matched to them.
    pub fn set_holdings(&mut self, holdings: &Holdings) {
        if self.holdings.as_ref() != Some(holdings) {
            self.holdings = Some(holdings.clone());
        }
    }

    /// The host's hash of each mod the setup names: `None` where not
    /// held, or not hashed yet.
    fn own_mods(&self) -> Vec<(Guid, Option<String>)> {
        self.setup
            .mods
            .iter()
            .map(|w| {
                let hash = self
                    .holdings
                    .as_ref()
                    .and_then(|h| h.get(&w.guid))
                    .map(|held| held.hash.clone());

                (w.guid, hash)
            })
            .collect()
    }

    /// The host seat's match of each mod the setup names.
    fn own_matches(&self) -> Vec<ModMatch> {
        self.own_mods()
            .iter()
            .map(|(_, hash)| match (&self.holdings, hash) {
                (None, _) => ModMatch::Unknown,
                (Some(_), None) => ModMatch::Missing,
                (Some(_), Some(_)) => ModMatch::Same,
            })
            .collect()
    }

    /// A guest's match of each mod the setup names, `theirs` its hashes:
    /// unknown until both it and the host have said.
    fn matches(&self, theirs: &[(Guid, Option<String>)]) -> Vec<ModMatch> {
        self.own_mods()
            .iter()
            .map(|(guid, ours)| {
                let said = theirs.iter().find(|(g, _)| g == guid).map(|(_, h)| h);

                match (ours, said) {
                    (_, None) | (None, Some(Some(_))) => ModMatch::Unknown,
                    (_, Some(None)) => ModMatch::Missing,
                    (Some(ours), Some(Some(hash))) if ours == hash => ModMatch::Same,
                    (Some(_), Some(Some(_))) => ModMatch::Differs,
                }
            })
            .collect()
    }

    /// Each mod the setup names, its title as the host has it, else its
    /// GUID.
    fn mod_titles(&self) -> Vec<String> {
        self.setup
            .mods
            .iter()
            .map(|w| {
                self.holdings
                    .as_ref()
                    .and_then(|h| h.get(&w.guid))
                    .map_or_else(|| w.guid.to_string(), |held| held.title.clone())
            })
            .collect()
    }

    /// Whether every seat holds every mod the setup names, as the host's.
    fn mods_match(&self) -> bool {
        let same = |m: &ModMatch| *m == ModMatch::Same;

        self.own_matches().iter().all(same)
            && self
                .guests
                .iter()
                .all(|g| self.matches(&g.mods).iter().all(same))
    }

    /// Each seat whose mods do not match, and how: for the log.
    ///
    /// ```text
    ///  Bob: New Game+ missing
    /// ```
    pub fn mod_problems(&self) -> Vec<String> {
        let titles = self.mod_titles();
        let seats = std::iter::once((self.nick.clone(), self.own_matches())).chain(
            self.guests
                .iter()
                .map(|g| (g.nick.clone(), self.matches(&g.mods))),
        );

        seats
            .flat_map(|(nick, matches)| {
                titles
                    .iter()
                    .zip(matches)
                    .filter(|(_, m)| *m != ModMatch::Same)
                    .map(move |(title, m)| format!("{nick}: {title} {}", said_match(m)))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// A change every guest must ready for again, said in the chat.
    fn revise(&mut self) {
        self.revision += 1;
        self.chat.push(Talk::Changed);
    }

    /// The LAN save `loaded` loaded: offered to every guest, its setup
    /// the lobby's, each seat the saved seat its nick had.
    ///
    /// # Errors
    ///
    /// A game of another number of players.
    pub fn choose(&mut self, loaded: Loaded) -> Result<(), String> {
        if loaded.seats.len() != self.players as usize {
            Err(format!(
                "a game of {} players; this one seats {}",
                loaded.seats.len(),
                self.players
            ))?;
        }

        let sent = Outgoing::new(loaded.bytes);

        info!("net: loading {}, game {}", loaded.name, loaded.game);
        self.chosen = Some(Chosen {
            offer: Offer {
                name: loaded.name,
                game: loaded.game,
                file: sent.file.clone(),
                size: sent.size(),
                seats: loaded.seats,
                sits: Vec::new(),
            },
            sent,
            path: loaded.path,
            fitted: Vec::new(),
        });
        self.setup = loaded.setup;
        self.revise();
        self.fit();
        Ok(())
    }

    /// The save loaded let go: a new game of the setup.
    pub fn unchoose(&mut self) {
        if self.chosen.take().is_some() {
            self.revise();
        }
    }

    /// The nicks here, by lobby seat: the host's, then each guest's.
    fn nicks(&self) -> Vec<String> {
        std::iter::once(self.nick.clone())
            .chain(self.guests.iter().map(|g| g.nick.clone()))
            .collect()
    }

    /// Each lobby seat the saved seat its nick had, the rest the free
    /// ones in order: again once the lobby's nicks change.
    fn fit(&mut self) {
        let nicks = self.nicks();

        if let Some(chosen) = &mut self.chosen
            && chosen.fitted != nicks
        {
            chosen.offer.sits = fitted(&chosen.offer.seats, &nicks);
            chosen.fitted = nicks;
        }
    }

    /// Lobby seat `at` moved to the next saved seat, with whoever had
    /// it moved to its old one: the host's reassignment.
    pub fn reseat(&mut self, at: usize) {
        let moved = self.chosen.as_mut().and_then(|chosen| {
            let sits = &mut chosen.offer.sits;
            let seats = u32::try_from(chosen.offer.seats.len()).ok()?;
            let was = *sits.get(at)?;
            let next = (was + 1) % seats;

            if let Some(other) = sits.iter().position(|s| *s == next) {
                sits[other] = was;
            }

            sits[at] = next;
            Some(())
        });

        if moved.is_some() {
            self.revise();
        }
    }

    /// The host's chat `line`, to every guest.
    pub fn say(&mut self, line: &str) {
        let line = line.chars().take(super::CHAT_MOST).collect();

        self.chat.push(Talk::Said(self.nick.clone(), line));
    }

    /// Every hello come answered at `now`, and a guest silent
    /// [`GONE_AFTER`] let go; whether every seat is taken.
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut buf = [0; MOST_BYTES];

        // An error but "nothing waiting" is an unreachable guest's,
        // read past.
        for _ in 0..MOST_READS {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => self.heard(&buf[..n], from, now),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {}
            }
        }

        self.guests.retain(|g| {
            let here = now.saturating_duration_since(g.heard) < GONE_AFTER;

            if !here {
                info!("net: {} left the lobby", g.addr);
            }

            here
        });
        for guest in self.guests.iter_mut().filter(|g| g.place == Place::Lobby) {
            if let Some(packet) = guest.probe.ask(now) {
                let _ = self.socket.send_to(&packet.encode(), guest.addr);
            }
        }
        self.fit();
        self.picked = self.delay.pick(&self.trips());

        if let Some(mut beacon) = self.beacon.take() {
            beacon.poll(now, |id| self.open(id));
            self.beacon = Some(beacon);
        }

        self.guests.len() + 1 >= self.players as usize
    }

    /// The lobby as LAN discovery shows it, its id `id`.
    fn open(&self, id: u64) -> Open {
        let (build, builds) = &self.build;
        // The map options alone: an announcement stays short.
        let setup = Setup {
            land: self.setup.land,
            map_seed: self.setup.map_seed,
            ..Setup::default()
        };

        Open {
            id,
            port: self.port(),
            title: self.title.clone(),
            nick: self.nick.clone(),
            seated: u32::try_from(self.guests.len() + 1).unwrap_or(u32::MAX),
            players: self.players,
            setup,
            save: self.chosen.as_ref().map(|c| c.offer.name.clone()),
            build: build.clone(),
            builds: *builds,
        }
    }

    /// The lobby announced to `to` alone: a test's.
    ///
    /// # Errors
    ///
    /// A socket that will not bind.
    #[cfg(test)]
    pub fn announce_to(&mut self, to: Vec<SocketAddr>) -> Result<(), String> {
        self.beacon = Some(Beacon::new(Targets::Given(to))?);
        Ok(())
    }

    /// Why the game may not start yet; `None` once every seat is taken
    /// and every guest ready for the setup as it stands.
    pub fn unready(&self) -> Option<Unready> {
        if self.guests.len() + 1 < self.players as usize {
            Some(Unready::Seats)
        } else if self.guests.iter().any(|g| !self.ready(g.ready)) {
            Some(Unready::Guests)
        } else if !self.mods_match() {
            Some(Unready::Mods)
        } else if self.chosen.as_ref().is_some_and(|c| {
            let have = Holding::Have(c.offer.game.clone());

            self.guests.iter().any(|g| g.holding != have)
        }) {
            Some(Unready::Save)
        } else {
            None
        }
    }

    /// Whether `ready` holds for the setup as it stands.
    const fn ready(&self, ready: Ready) -> bool {
        match ready {
            Ready::No => false,
            Ready::For(revision) => revision == self.revision,
            Ready::Always => true,
        }
    }

    /// Each seat's nick, by seat; `None` for a seat not taken.
    #[cfg(test)]
    pub fn seats(&self) -> Vec<Option<&str>> {
        let taken = std::iter::once(self.nick.as_str())
            .chain(self.guests.iter().map(|g| g.nick.as_str()))
            .map(Some);

        taken
            .chain(std::iter::repeat(None))
            .take(self.players as usize)
            .collect()
    }

    /// The lobby as seat `seat` sees it, `taken` its chat lines taken,
    /// the chat from line `heard` on.
    fn room(&self, (seat, taken): (u32, u32), heard: u32) -> Room {
        let host = Seat {
            nick: self.nick.clone(),
            build: self.build.0.shown(),
            ready: true,
            mods: self.own_matches(),
        };
        let guests = self.guests.iter().map(|g| Seat {
            nick: g.nick.clone(),
            build: g.build.clone(),
            ready: self.ready(g.ready),
            mods: self.matches(&g.mods),
        });
        let first = heard.min(u32::try_from(self.chat.len()).unwrap_or(u32::MAX));
        let mut room = Room {
            seat,
            players: self.players,
            delay: self.picked,
            title: self.title.clone(),
            revision: self.revision,
            setup: self.setup.clone(),
            mods: self.mod_titles(),
            seats: std::iter::once(host).chain(guests).collect(),
            taken,
            chat: Lines {
                first,
                lines: self
                    .chat
                    .iter()
                    .skip(first as usize)
                    .take(CHAT_LINES)
                    .cloned()
                    .collect(),
            },
            save: self.chosen.as_ref().map(|c| c.offer.clone()),
        };

        // The lines that fit the datagram: one over MOST_BYTES is not
        // read whole. The rest go with the next.
        while !room.chat.lines.is_empty() && Packet::Lobby(room.clone()).encode().len() > MOST_BYTES
        {
            room.chat.lines.pop();
        }

        room
    }

    /// The lobby as the host sees it: every seat, the whole chat.
    pub fn view(&self) -> Room {
        Room {
            chat: Lines {
                first: 0,
                lines: self.chat.clone(),
            },
            ..self.room((0, 0), u32::MAX)
        }
    }

    /// Each guest welcomed to the game: the host's game may begin. A
    /// loaded game's seats are the saved seats each lobby seat sits in.
    pub fn seat(self) -> Seated {
        info!(
            "net: lockstep delay {} frames ({:?})",
            self.picked, self.delay
        );

        let lobby = self.nicks();
        let sits = self
            .chosen
            .as_ref()
            .map_or_else(|| (0..self.players).collect(), |c| c.offer.sits.clone());
        let mut nicks = vec![String::new(); self.players as usize];

        for (nick, &sit) in lobby.iter().zip(&sits) {
            if let Some(at) = nicks.get_mut(sit as usize) {
                nick.clone_into(at);
            }
        }

        let host = sits.first().copied().unwrap_or_default();
        let game = self.chosen.as_ref().map(|c| c.offer.game.clone());
        let guests = self
            .guests
            .iter()
            .zip(sits.iter().skip(1))
            .map(|(guest, &seat)| {
                let welcome = Packet::Welcome(Welcome {
                    seat,
                    players: self.players,
                    delay: self.picked,
                    setup: self.setup.clone(),
                    nicks: nicks.clone(),
                    host,
                    save: game.clone(),
                });

                (guest.addr, seat, welcome.encode())
            })
            .collect::<Vec<_>>();

        for (addr, _, welcome) in &guests {
            let _ = self.socket.send_to(welcome, addr);
        }

        Seated {
            socket: self.socket,
            guests,
            players: self.players,
            delay: self.picked,
            mode: self.delay,
            nicks,
            seat: host,
            save: self.chosen.map(|c| c.path),
            build: self.build,
        }
    }

    /// The lobby given up: each guest told, [`LEAVE_COPIES`] times, the
    /// socket let go.
    pub fn leave(self) {
        let gone = Packet::Refused(HOST_LEFT.to_owned()).encode();

        for guest in &self.guests {
            for _ in 0..LEAVE_COPIES {
                let _ = self.socket.send_to(&gone, guest.addr);
            }
        }

        info!("net: hosting given up");
    }

    /// `bytes` from `from` at `now`: a hello answered, and a guest that
    /// wants the save sent the chunks it asks.
    fn heard(&mut self, bytes: &[u8], from: SocketAddr, now: Instant) {
        let replies = match Packet::decode(bytes) {
            Ok(Packet::Hello(hello)) => {
                let reply = self.hello(&hello, from, now);
                let chunks = match (&reply, &hello.holding, &self.chosen) {
                    (Packet::Lobby(_), Holding::Want { game, from }, Some(c))
                        if *game == c.offer.game =>
                    {
                        c.sent.chunks(*from)
                    }
                    _ => Vec::new(),
                };

                std::iter::once(reply)
                    .chain(chunks.into_iter().map(Packet::Chunk))
                    .collect()
            }
            // A seated guest still in the game missed the regroup.
            Ok(Packet::Game(_)) => match self.guests.iter_mut().find(|g| g.addr == from) {
                Some(guest) => {
                    guest.heard = now;
                    guest.place = Place::Game;
                    vec![Packet::Regroup]
                }
                None => Vec::new(),
            },
            Ok(Packet::Ping(id)) if self.guests.iter().any(|g| g.addr == from) => {
                vec![Packet::Pong(id)]
            }
            Ok(Packet::Pong(id)) => {
                if let Some(guest) = self.guests.iter_mut().find(|g| g.addr == from) {
                    guest.probe.answered(id, now);
                }
                Vec::new()
            }
            Ok(_) => Vec::new(),
            Err(why) => {
                warn!("net: {from}: {why}");
                Vec::new()
            }
        };

        for reply in replies {
            let _ = self.socket.send_to(&reply.encode(), from);
        }
    }

    /// The answer to `from`'s `hello`: the lobby from its seat, or why
    /// not.
    fn hello(&mut self, hello: &Hello, from: SocketAddr, now: Instant) -> Packet {
        let (ours, builds) = &self.build;
        let seat = match mismatch(ours, &hello.build, *builds) {
            Some(why) => Err(why),
            None => self.seated(from, hello, now),
        };

        match seat {
            Ok(seat) => {
                if let Some(why) = data_warning(&self.build.0, &hello.build) {
                    warn!("net: {from}: {why}");
                }

                let taken = self.told(seat, hello);

                Packet::Lobby(self.room((seat, taken), hello.heard))
            }
            Err(why) => {
                warn!("net: {from} refused: {why}");
                Packet::Refused(why)
            }
        }
    }

    /// The seat of the guest at `from`, as its `hello` names it: its own
    /// if in, else the next; or why not: none free, or its nick taken.
    /// Its ready and heard kept.
    fn seated(&mut self, from: SocketAddr, hello: &Hello, now: Instant) -> Result<u32, String> {
        let known = self.guests.iter().position(|g| g.addr == from);
        let nick = nick_of(&hello.nick);
        let at = match known {
            Some(at) => at,
            None if self.guests.len() + 1 >= self.players as usize => Err(FULL)?,
            None if self.taken(&nick) => Err(format!("{nick}: {NICK_TAKEN}"))?,
            None => {
                self.guests.push(Guest {
                    addr: from,
                    place: Place::Lobby,
                    nick,
                    build: hello.build.shown(),
                    ready: Ready::No,
                    heard: now,
                    // A guest let go and seated again counts on: its
                    // lines before these were taken, or are lost.
                    taken: hello.said.first,
                    holding: Holding::None,
                    mods: Vec::new(),
                    probe: Probe::default(),
                });
                info!("net: {from} joined as player {}", self.guests.len());
                self.guests.len() - 1
            }
        };
        let guest = &mut self.guests[at];

        guest.heard = now;
        guest.place = Place::Lobby;
        guest.build = hello.build.shown();
        guest.ready = hello.ready;
        guest.holding.clone_from(&hello.holding);
        guest.mods.clone_from(&hello.mods);
        u32::try_from(at + 1).map_err(|e| e.to_string())
    }

    /// Whether `nick` is the host's or a seated guest's, in any case; an
    /// empty nick is no one's.
    fn taken(&self, nick: &str) -> bool {
        let nick = nick.to_lowercase();

        !nick.is_empty() && self.nicks().iter().any(|n| n.to_lowercase() == nick)
    }

    /// Seat `seat`'s chat lines in `hello` taken, those not taken before
    /// added to the chat; how many of its lines are taken.
    fn told(&mut self, seat: u32, hello: &Hello) -> u32 {
        let Some(guest) = (seat as usize)
            .checked_sub(1)
            .and_then(|at| self.guests.get_mut(at))
        else {
            return 0;
        };
        let fresh = numbered(hello.said.first, &hello.said.lines);
        let mut said = Vec::new();

        for (n, line) in fresh {
            let next = guest.taken.checked_add(1);

            if let (true, Some(next)) = (n == guest.taken, next) {
                said.push(Talk::Said(
                    guest.nick.clone(),
                    line.chars().take(super::CHAT_MOST).collect(),
                ));
                guest.taken = next;
            }
        }

        let taken = guest.taken;

        self.chat.extend(said);
        taken
    }
}

/// A mod's match, as the log says it.
const fn said_match(m: ModMatch) -> &'static str {
    match m {
        ModMatch::Unknown => "not checked yet",
        ModMatch::Same => "the same",
        ModMatch::Missing => "missing",
        ModMatch::Differs => "another version",
    }
}

/// `lines` numbered from `first`, those past `u32::MAX` dropped: a
/// peer's numbers are its word.
fn numbered<T>(first: u32, lines: &[T]) -> impl Iterator<Item = (u32, &T)> {
    (0..).map_while(move |i| first.checked_add(i)).zip(lines)
}

/// Each of `nicks`, by lobby seat, the saved seat of `seats` it names,
/// the rest the saved seats left, in order.
///
/// ```text
///  saved [Ann, Bob, Cat], lobby [Cat, Dan, Ann] ─> [2, 1, 0]
/// ```
fn fitted(seats: &[String], nicks: &[String]) -> Vec<u32> {
    let mut free: Vec<u32> = (0..u32::try_from(seats.len()).unwrap_or_default()).collect();
    let mut named: Vec<Option<u32>> = nicks
        .iter()
        .map(|nick| {
            let at = free.iter().position(|&s| seats[s as usize] == *nick)?;

            Some(free.remove(at))
        })
        .collect();

    for sit in &mut named {
        if sit.is_none() && !free.is_empty() {
            *sit = Some(free.remove(0));
        }
    }

    named.into_iter().flatten().collect()
}

/// `nick` as sent: cut to [`NICK_MOST`] characters.
fn nick_of(nick: &str) -> String {
    nick.chars().take(NICK_MOST).collect()
}

/// `HOST[:PORT]` as typed: the host and port, [`PORT`] if none is
/// given. Not resolved: whether it names a machine is the join's to
/// find.
///
/// ```text
///  "192.168.1.5"        192.168.1.5, 47101
///  "player1-pc:4000"    player1-pc, 4000
///  "[::1]:4000", "::1"  ::1, 4000 / 47101
/// ```
///
/// # Errors
///
/// Nothing typed, a space in it, or a port that is not a number from 1
/// to 65535.
pub fn parse_address(text: &str) -> Result<(String, u16), String> {
    let text = text.trim();
    let port_of = |p: &str| {
        p.parse::<u16>()
            .ok()
            .filter(|&p| p != 0)
            .ok_or_else(|| format!("port {p}: not a number from 1 to 65535"))
    };
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        // A bracketed IPv6 address, a port after it or not.
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| format!("{text}: no closing ]"))?;

        match after.strip_prefix(':') {
            Some(p) => (host, port_of(p)?),
            None if after.is_empty() => (host, PORT),
            None => Err(format!("{text}: what follows ] is not :PORT"))?,
        }
    } else {
        match text.split_once(':') {
            // Two colons or more: an IPv6 address, no port.
            Some((_, rest)) if rest.contains(':') => (text, PORT),
            Some((host, p)) => (host, port_of(p)?),
            None => (text, PORT),
        }
    };

    if host.is_empty() {
        Err("name the host: HOST or HOST:PORT".to_owned())
    } else if host.contains(char::is_whitespace) {
        Err(format!("{host}: a host has no spaces"))
    } else {
        Ok((host.to_owned(), port))
    }
}

/// `HOST[:PORT]` as an address, [`PORT`] if none is given: an IPv4 one,
/// as the lobby's sockets are, though the name resolves to IPv6 first
/// (`localhost` to `::1` on some systems).
///
/// # Errors
///
/// An address that does not read ([`parse_address`]), a host that does
/// not resolve, or one with IPv6 addresses only.
pub fn address(text: &str) -> Result<SocketAddr, String> {
    let (host, port) = parse_address(text)?;
    let all: Vec<SocketAddr> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("{host}: {e}"))?
        .collect();

    match (all.iter().find(|a| a.is_ipv4()), all.is_empty()) {
        (Some(v4), _) => Ok(*v4),
        (None, true) => Err(format!("{host}: no address")),
        (None, false) => Err(format!("{host}: IPv6 only; LAN games take an IPv4 address")),
    }
}

/// A guest's lobby, polled: its hello sent again until the host's
/// welcome, the host's lobby as last heard kept.
pub struct Joining {
    socket: UdpSocket,
    host: SocketAddr,
    build: Build,
    nick: String,
    ready: Ready,
    /// When the host was last heard, or the asking began.
    heard: Instant,
    /// When the hello was last sent.
    sent: Option<Instant>,
    /// The chunk of the save it asked from.
    asked: u32,
    probe: Probe,
    /// The host's lobby, as last heard.
    room: Option<Room>,
    /// The chat as heard.
    chat: Vec<Talk>,
    /// This player's chat lines, and how many the host has taken.
    said: Vec<String>,
    taken: u32,
    /// Where a save the host loads is looked for, and one sent goes:
    /// the profile's LAN saves. None, no save is taken.
    saves: Option<Arc<dyn Saves>>,
    /// The save the host loads, as this guest holds it.
    save: Save,
    /// The host plays on: asked from the lobby after a game, this guest
    /// waits for it.
    held: bool,
    /// This machine's mods, hashed; none until hashed.
    holdings: Option<Holdings>,
}

/// What a guest holds of the save the host loads.
#[derive(Clone, Debug, Default)]
enum Save {
    /// None offered, or none to be taken.
    #[default]
    None,
    /// Looked for in the saves: none there with the game; being sent.
    Coming(Incoming, String),
    /// A copy, of the game: its own or sent.
    Have(String, std::path::PathBuf),
    /// Sent but not taken: the file, and why. Not asked again unless the
    /// host offers another file.
    Failed(String, String),
}

impl Joining {
    /// Asking `host` for a seat at `now`, running `build`, named `nick`,
    /// not ready.
    ///
    /// # Errors
    ///
    /// A socket that will not bind or be made non-blocking.
    pub fn new(
        host: SocketAddr,
        (build, nick): (&Build, &str),
        now: Instant,
    ) -> Result<Joining, String> {
        let socket = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;

        Joining::on(socket, host, (build, nick), now)
    }

    /// [`Joining::new`] on `socket`.
    ///
    /// # Errors
    ///
    /// A socket that will not be made non-blocking.
    fn on(
        socket: UdpSocket,
        host: SocketAddr,
        (build, nick): (&Build, &str),
        now: Instant,
    ) -> Result<Joining, String> {
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        info!("net: joining {host}");

        Ok(Joining {
            socket,
            host,
            build: build.clone(),
            nick: nick_of(nick),
            ready: Ready::No,
            heard: now,
            sent: None,
            asked: 0,
            probe: Probe::default(),
            room: None,
            chat: Vec::new(),
            said: Vec::new(),
            taken: 0,
            saves: None,
            save: Save::None,
            held: false,
            holdings: None,
        })
    }

    /// Whether the host plays on, this guest back in the lobby first.
    pub const fn held(&self) -> bool {
        self.held
    }

    pub const fn host(&self) -> SocketAddr {
        self.host
    }

    pub fn ping(&self, now: Instant) -> Option<Duration> {
        self.probe.ping(now)
    }

    pub fn ping_stale(&self, now: Instant) -> bool {
        self.probe.stale(now)
    }

    /// A save the host loads looked for in `saves`, and one sent kept
    /// there: the profile's LAN saves.
    pub fn set_saves(&mut self, saves: Option<Arc<dyn Saves>>) {
        self.saves = saves;
    }

    /// The copy of the save of game `game`, if this guest has it.
    pub fn save(&self, game: &str) -> Option<&Path> {
        match &self.save {
            Save::Have(have, path) if have == game => Some(path),
            _ => None,
        }
    }

    /// A copy of the save of game `game`: the lobby's, else one in the
    /// saves, as a guest that rejoins a loaded game has.
    pub fn copy(&self, game: &str) -> Option<std::path::PathBuf> {
        self.save(game)
            .map(Path::to_path_buf)
            .or_else(|| self.saves.as_ref()?.find(game))
    }

    /// The save being sent: chunks in, of all.
    pub fn receiving(&self) -> Option<(u32, u32)> {
        match &self.save {
            Save::Coming(incoming, _) => Some(incoming.progress()),
            _ => None,
        }
    }

    /// Why the save the host loads did not come.
    pub fn failed(&self) -> Option<&str> {
        match &self.save {
            Save::Failed(_, why) => Some(why),
            _ => None,
        }
    }

    /// What this guest holds of the save, as its hello says.
    fn holding(&self) -> Holding {
        match &self.save {
            Save::None | Save::Failed(..) => Holding::None,
            Save::Coming(incoming, game) => Holding::Want {
                game: game.clone(),
                from: incoming.from(),
            },
            Save::Have(game, _) => Holding::Have(game.clone()),
        }
    }

    /// The save `offer` names, looked for in the saves once offered: a
    /// copy whose meta names its game held, else sent.
    fn offered(&mut self, offer: Option<&Offer>) {
        // A copy of the game will do; a file being sent must be the one
        // offered, whose chunks come.
        let held = match (&self.save, offer) {
            (Save::Have(game, _), Some(o)) => *game == o.game,
            (Save::Coming(incoming, game), Some(o)) => *game == o.game && incoming.file == o.file,
            (Save::Failed(file, _), Some(o)) => *file == o.file,
            (Save::None, None) => true,
            _ => false,
        };

        if !held {
            self.save = match (offer, &self.saves) {
                (Some(offer), Some(saves)) => saves.find(&offer.game).map_or_else(
                    || {
                        info!("net: {} is sent, {} bytes", offer.name, offer.size);
                        Save::Coming(
                            Incoming::new(offer.file.clone(), offer.size),
                            offer.game.clone(),
                        )
                    },
                    |path| {
                        info!("net: {} here as {}", offer.name, path.display());
                        Save::Have(offer.game.clone(), path)
                    },
                ),
                _ => Save::None,
            };
        }
    }

    /// A chunk of the save being sent; once all are in, the save
    /// written to the saves under its name or one free, read back. Bytes
    /// not those offered, or a save not of the game offered, fail it:
    /// dropped, said, the seat kept.
    fn chunk(&mut self, chunk: super::transfer::Chunk) {
        let Save::Coming(incoming, game) = &mut self.save else {
            return;
        };

        incoming.take(chunk);

        let name = self
            .room
            .as_ref()
            .and_then(|r| r.save.as_ref())
            .map_or_else(String::new, |o| o.name.clone());
        let done = incoming.done().map(|bytes| {
            let saves = self.saves.as_ref().ok_or("no saves")?;

            saves.keep(&name, game, &bytes?)
        });

        match done {
            Some(Ok(path)) => {
                info!("net: {name} came: {}", path.display());
                self.save = Save::Have(game.clone(), path);
            }
            Some(Err(why)) => {
                let why = format!("the host's save {name}: {why}");

                warn!("net: {why}");
                self.save = Save::Failed(incoming.file.clone(), why);
            }
            None => {}
        }
    }

    /// The seat and players the host last said, while the rest join.
    pub fn seated(&self) -> Option<(u32, u32)> {
        self.room.as_ref().map(|r| (r.seat, r.players))
    }

    /// The host's lobby as last heard: none before its first answer.
    pub const fn room(&self) -> Option<&Room> {
        self.room.as_ref()
    }

    /// The chat as heard.
    pub fn chat(&self) -> &[Talk] {
        &self.chat
    }

    /// Ready, or not, for the host's setup as last heard; a change of it
    /// lapses this. Set [`Ready::Always`], it holds whatever the setup.
    pub const fn set_ready(&mut self, ready: Ready) {
        self.ready = ready;
    }

    /// Whether the host counts this guest ready for the setup as it
    /// stands, as last heard.
    pub const fn ready(&self) -> bool {
        match (self.ready, &self.room) {
            (Ready::Always, _) => true,
            (Ready::For(revision), Some(room)) => revision == room.revision,
            _ => false,
        }
    }

    /// This player's chat `line`, to the host and through it the rest.
    pub fn say(&mut self, line: &str) {
        self.said
            .push(line.chars().take(super::CHAT_MOST).collect());
    }

    /// The hello as it stands: ready, the chat held, the lines the host
    /// has not taken.
    /// This machine's mods, hashed: the host is told its hash of each
    /// the setup names.
    pub fn set_holdings(&mut self, holdings: &Holdings) {
        if self.holdings.as_ref() != Some(holdings) {
            self.holdings = Some(holdings.clone());
        }
    }

    /// This machine's hash of each mod the host's setup names; none
    /// before the lobby is heard or the mods hashed.
    fn held_mods(&self) -> Vec<(Guid, Option<String>)> {
        match (&self.room, &self.holdings) {
            (Some(room), Some(holdings)) => room
                .setup
                .mods
                .iter()
                .map(|w| (w.guid, holdings.get(&w.guid).map(|h| h.hash.clone())))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn hello(&self) -> Vec<u8> {
        let held = u32::try_from(self.chat.len()).unwrap_or(u32::MAX);

        Packet::Hello(Hello {
            build: self.build.clone(),
            nick: self.nick.clone(),
            ready: self.ready,
            holding: self.holding(),
            mods: self.held_mods(),
            heard: held,
            said: Lines {
                first: self.taken,
                lines: self
                    .said
                    .iter()
                    .skip(self.taken as usize)
                    .take(CHAT_LINES)
                    .cloned()
                    .collect(),
            },
        })
        .encode()
    }

    /// The hello sent again if due at `now`, and what the host said: its
    /// welcome, once the host starts the game.
    ///
    /// # Errors
    ///
    /// A refusal, or a host silent for [`JOIN_TIMEOUT`].
    pub fn poll(&mut self, now: Instant) -> Result<Option<Welcome>, String> {
        let host = self.host;
        let mut buf = [0; MOST_BYTES];
        // A save being sent asks on as soon as the window asked is in;
        // a chunk lost waits for the next hello.
        let window_in = match &self.save {
            Save::Coming(incoming, _) => incoming.from() >= self.asked.saturating_add(WINDOW),
            _ => false,
        };
        let due = window_in
            || self
                .sent
                .is_none_or(|s| now.saturating_duration_since(s) >= HELLO_EVERY);

        if due {
            let _ = self.socket.send_to(&self.hello(), host);
            self.sent = Some(now);
            self.asked = match &self.save {
                Save::Coming(incoming, _) => incoming.from(),
                _ => 0,
            };
        }
        if let Some(packet) = self.probe.ask(now) {
            let _ = self.socket.send_to(&packet.encode(), host);
        }

        for _ in 0..MOST_READS {
            let (n, from) = match self.socket.recv_from(&mut buf) {
                Ok(got) => got,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => continue,
            };

            if from != host {
                continue;
            }

            self.heard = now;

            match Packet::decode(&buf[..n]) {
                Ok(Packet::Welcome(welcome)) => {
                    info!(
                        "net: welcomed as player {} of {}",
                        welcome.seat, welcome.players
                    );
                    return Ok(Some(welcome));
                }
                Ok(Packet::Lobby(room)) => {
                    self.held = false;
                    self.lobby(room);
                }
                Ok(Packet::Chunk(chunk)) => self.chunk(chunk),
                Ok(Packet::Refused(why)) => Err(format!("{host} refused: {why}"))?,
                Ok(Packet::Wait) => self.held = true,
                Ok(Packet::Ping(id)) => {
                    let _ = self.socket.send_to(&Packet::Pong(id).encode(), host);
                }
                Ok(Packet::Pong(id)) => self.probe.answered(id, now),
                Ok(Packet::Bye(_)) => Err(format!("{host}: {HOST_LEFT}"))?,
                Ok(_) => {}
                Err(why) => warn!("net: {host}: {why}"),
            }
        }

        if now.saturating_duration_since(self.heard) > JOIN_TIMEOUT {
            Err(format!("{host}: no answer"))
        } else {
            Ok(None)
        }
    }

    /// The host's lobby `room` heard: its chat lines not held added, this
    /// player's taken let go of.
    fn lobby(&mut self, room: Room) {
        if self.seated() != Some((room.seat, room.players)) {
            info!(
                "net: player {} of {}; waiting for the host",
                room.seat, room.players
            );
        }

        for (n, line) in numbered(room.chat.first, &room.chat.lines) {
            if n as usize == self.chat.len() {
                self.chat.push(line.clone());
            }
        }

        self.taken = self.taken.max(room.taken);
        self.offered(room.save.as_ref());
        self.room = Some(room);
    }

    /// The socket, for the game.
    pub fn socket(self) -> UdpSocket {
        self.socket
    }
}

impl super::Regroup {
    /// The host's lobby again on its game's socket, running `build`, the
    /// setup `setup`, named `title`: each guest of the game in it or
    /// waiting seated first, by seat, heard `now`, not ready.
    ///
    /// # Errors
    ///
    /// A guest's regroup, or a socket that will not be made
    /// non-blocking.
    pub fn host(
        self,
        build: (&Build, Builds),
        (setup, title): (&Setup, &str),
        now: Instant,
    ) -> Result<Hosting, String> {
        let guests = match self.back {
            Back::Host(guests) => guests,
            Back::Guest(_) => Err("a guest hosts no lobby")?,
        };
        let lobby = Lobby {
            socket: self.socket,
        };
        let mut hosting = lobby.host(build, (self.players, Delay::Auto), (setup, &self.nick))?;
        let shown = hosting.build.0.shown();

        hosting.set_title(title);
        hosting.guests = guests
            .into_iter()
            .map(|(addr, seat)| Guest {
                addr,
                place: Place::Game,
                nick: nick_of(self.nicks.get(seat as usize).map_or("", String::as_str)),
                build: shown.clone(),
                ready: Ready::No,
                heard: now,
                taken: 0,
                holding: Holding::None,
                mods: Vec::new(),
                probe: Probe::default(),
            })
            .collect();
        Ok(hosting)
    }

    /// The guest's lobby again on its game's socket, running `build`,
    /// asking its host from `now`.
    ///
    /// # Errors
    ///
    /// The host's regroup, or a socket that will not be made
    /// non-blocking.
    pub fn join(self, build: &Build, now: Instant) -> Result<Joining, String> {
        match self.back {
            Back::Guest(host) => Joining::on(self.socket, host, (build, &self.nick), now),
            Back::Host(_) => Err("the host joins no lobby".to_owned()),
        }
    }
}

/// Join the host at `host` running `build`, named `nick`, always ready,
/// its mods `holdings`: its welcome, and the socket. Blocks: the command
/// line's.
///
/// # Errors
///
/// A refusal, or a host silent for [`JOIN_TIMEOUT`].
pub fn join(
    host: SocketAddr,
    build: (&Build, &str),
    holdings: &Holdings,
) -> Result<(UdpSocket, Welcome), String> {
    let mut joining = Joining::new(host, build, Instant::now())?;

    joining.set_ready(Ready::Always);
    joining.set_holdings(holdings);

    loop {
        if let Some(welcome) = joining.poll(Instant::now())? {
            break Ok((joining.socket(), welcome));
        }

        std::thread::sleep(NAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_tracks_answer_and_silence() {
        let now = Instant::now();
        let mut probe = Probe::default();
        let id = match probe.ask(now) {
            Some(Packet::Ping(id)) => id,
            other => panic!("expected ping, got {other:?}"),
        };

        probe.answered(id + 1, now + Duration::from_millis(20));
        assert_eq!(probe.ping(now), None);

        probe.answered(id, now + Duration::from_millis(40));
        assert_eq!(
            probe.ping(now + Duration::from_secs(1)),
            Some(Duration::from_millis(40))
        );
        assert!(probe.stale(now + PING_STALE + Duration::from_millis(40)));
    }

    fn build() -> Build {
        Build {
            version: "1".to_owned(),
            profile: Profile::Release,
            commit: "c0ffee".to_owned(),
            data: 2,
        }
    }

    #[test]
    fn a_differing_build_is_refused_with_its_reason() {
        let ours = build();
        let commit = Build {
            commit: "beef".to_owned(),
            ..build()
        };
        let data = Build { data: 9, ..build() };
        let debug = Build {
            profile: Profile::Debug,
            ..build()
        };

        assert_eq!(mismatch(&ours, &build(), Builds::Same), None);
        assert!(mismatch(&ours, &commit, Builds::Same).is_some_and(|w| w.contains("commit")));
        assert_eq!(mismatch(&ours, &commit, Builds::Any), None);
        assert_eq!(mismatch(&ours, &data, Builds::Same), None);
        assert_eq!(data_warning(&ours, &data), Some(DATA_DIFFER));
        assert_eq!(data_warning(&ours, &build()), None);
        assert!(mismatch(&ours, &debug, Builds::Any).is_some_and(|w| w.contains("debug")));
    }

    #[test]
    fn a_host_without_a_port_gets_the_default() {
        assert_eq!(address("127.0.0.1").map(|a| a.port()), Ok(PORT));
        assert_eq!(address("127.0.0.1:9").map(|a| a.port()), Ok(9));
        assert_eq!(PORT_TEXT.parse(), Ok(PORT));
    }

    /// A name's IPv4 address is taken over its IPv6 one, and an IPv6
    /// address is refused at once: the lobby's sockets are IPv4.
    #[test]
    fn an_address_is_ipv4_or_refused() {
        assert!(address("localhost").is_ok_and(|a| a.is_ipv4()));
        assert!(address("[::1]:4000").is_err_and(|why| why.contains("IPv6")));
    }

    #[test]
    fn an_address_reads_as_host_and_port_or_says_why_not() {
        let read = |text| parse_address(text);
        let ok = |host: &str, port| Ok((host.to_owned(), port));

        assert_eq!(read("192.168.1.5"), ok("192.168.1.5", PORT));
        assert_eq!(read(" player1-pc:4000 "), ok("player1-pc", 4000));
        assert_eq!(read("[::1]:4000"), ok("::1", 4000));
        assert_eq!(read("[::1]"), ok("::1", PORT));
        assert_eq!(read("fe80::1"), ok("fe80::1", PORT));

        for bad in [
            "",
            "  ",
            ":4000",
            "host:",
            "host:0",
            "host:70000",
            "a b",
            "[::1",
        ] {
            assert!(read(bad).is_err(), "{bad:?}");
        }
    }

    /// A lobby of `players` on a free port.
    fn hosting(players: u32) -> Hosting {
        Lobby::bind(0)
            .expect("a port")
            .host(
                (&build(), Builds::Same),
                (players, Delay::Fixed(super::super::DELAY)),
                (&Setup::default(), "Host"),
            )
            .expect("hosting")
    }

    /// A guest's hello, its own lines from `first`.
    fn hello(first: u32, lines: &[&str]) -> Hello {
        Hello {
            build: build(),
            nick: "Ann".to_owned(),
            ready: Ready::No,
            heard: 0,
            said: Lines {
                first,
                lines: lines.iter().map(|l| (*l).to_owned()).collect(),
            },
            holding: Holding::None,
            mods: Vec::new(),
        }
    }

    /// The players' lines in `chat`.
    fn said(chat: &[Talk]) -> Vec<&str> {
        chat.iter()
            .filter_map(|t| match t {
                Talk::Said(_, line) => Some(line.as_str()),
                Talk::Changed => None,
            })
            .collect()
    }

    /// Line numbers at the top of their range, from a guest or the host,
    /// break neither lobby.
    #[test]
    fn line_numbers_at_the_top_of_their_range_break_no_lobby() {
        let now = Instant::now();
        let mut host = hosting(2);
        let from = SocketAddr::from(([127, 0, 0, 1], 9));

        host.hello(&hello(u32::MAX, &["a", "b"]), from, now);

        let mut guest = Joining::new(from, (&build(), "Ann"), now).expect("joining");
        let lines = vec![Talk::Changed, Talk::Changed];

        guest.lobby(Room {
            seat: 1,
            players: 2,
            chat: Lines {
                first: u32::MAX,
                lines,
            },
            ..Room::default()
        });
    }

    /// Two mods' GUIDs: the tests' setup names both.
    const MODS: [&str; 2] = [
        "{17BF53DE-722F-4683-8E63-6B93D1606DFF}",
        "{D666FCBE-9906-4582-A6A0-DC96F812441E}",
    ];

    fn guid(at: usize) -> Guid {
        Guid::parse(MODS[at]).expect("a GUID")
    }

    /// The mods `held` names hashed as given, titled by their place.
    fn holdings(held: &[(usize, &str)]) -> Holdings {
        held.iter()
            .map(|&(at, hash)| {
                let held = crate::files::mod_hash::Held {
                    title: format!("Mod {at}"),
                    hash: hash.to_owned(),
                };

                (guid(at), held)
            })
            .collect()
    }

    /// A setup naming both mods.
    fn modded() -> Setup {
        Setup {
            mods: (0..MODS.len())
                .map(|at| crate::setup::Wanted::from(guid(at)))
                .collect(),
            ..Setup::default()
        }
    }

    /// A guest that lacks a mod, or holds another version, holds the
    /// start, marked on its seat; once it holds each as the host does,
    /// the game may start.
    #[test]
    fn a_guest_missing_a_mod_or_of_another_version_holds_the_start() {
        use ModMatch::{Differs, Missing, Same};

        let now = Instant::now();
        let mut host = hosting(2);
        let from = SocketAddr::from(([127, 0, 0, 1], 9));

        host.set_setup(&modded());
        host.set_holdings(&holdings(&[(0, "aa"), (1, "bb")]));

        let said = |host: &mut Hosting, second: Option<&str>| {
            let hello = Hello {
                ready: Ready::For(host.revision),
                mods: vec![
                    (guid(0), Some("aa".to_owned())),
                    (guid(1), second.map(str::to_owned)),
                ],
                ..hello(0, &[])
            };

            host.hello(&hello, from, now);
            (host.view().seats[1].mods.clone(), host.unready())
        };

        assert_eq!(
            said(&mut host, None),
            (vec![Same, Missing], Some(Unready::Mods))
        );
        assert_eq!(host.mod_problems(), ["Ann: Mod 1 missing"]);
        assert_eq!(
            said(&mut host, Some("xx")),
            (vec![Same, Differs], Some(Unready::Mods))
        );
        assert_eq!(said(&mut host, Some("bb")), (vec![Same, Same], None));
        assert_eq!(host.view().mods, ["Mod 0", "Mod 1"]);
    }

    /// The host's own seat: unknown until its mods are hashed, then
    /// missing where it holds none.
    #[test]
    fn a_host_unhashed_or_lacking_a_mod_holds_the_start() {
        use ModMatch::{Missing, Same, Unknown};

        let mut host = hosting(1);

        host.set_setup(&modded());

        assert_eq!(host.view().seats[0].mods, [Unknown, Unknown]);
        assert_eq!(host.unready(), Some(Unready::Mods));

        host.set_holdings(&holdings(&[(0, "aa")]));

        assert_eq!(host.view().seats[0].mods, [Same, Missing]);
        assert_eq!(host.unready(), Some(Unready::Mods));
    }

    /// A guest tells the host its hash of each mod the host's setup
    /// names, none for one it lacks; nothing before its mods are hashed.
    #[test]
    fn a_guest_says_its_hash_of_each_mod_the_setup_names() {
        let now = Instant::now();
        let from = SocketAddr::from(([127, 0, 0, 1], 9));
        let mut guest = Joining::new(from, (&build(), "Ann"), now).expect("joining");

        guest.lobby(Room {
            seat: 1,
            players: 2,
            setup: modded(),
            ..Room::default()
        });

        assert_eq!(guest.held_mods(), []);

        guest.set_holdings(&holdings(&[(1, "bb")]));

        assert_eq!(
            guest.held_mods(),
            [(guid(0), None), (guid(1), Some("bb".to_owned()))]
        );
    }

    /// A guest let go for its silence and seated again goes on chatting:
    /// its lines count on from those taken before.
    #[test]
    fn a_guest_seated_again_is_heard() {
        let now = Instant::now();
        let mut host = hosting(2);
        let from = SocketAddr::from(([127, 0, 0, 1], 9));

        host.hello(&hello(0, &["one"]), from, now);
        host.poll(now + GONE_AFTER * 2);

        assert!(host.guests.is_empty(), "let go");

        host.hello(&hello(1, &["two"]), from, now + GONE_AFTER * 2);

        assert_eq!(said(&host.chat), ["one", "two"]);
    }

    /// A guest whose nick the host or a seated guest has, in any case,
    /// is refused, and told why; empty nicks clash with none.
    #[test]
    fn a_nick_taken_is_refused() {
        let now = Instant::now();
        let mut host = hosting(4);
        let at = |port| SocketAddr::from(([127, 0, 0, 1], port));
        let named = |nick: &str| Hello {
            nick: nick.to_owned(),
            ..hello(0, &[])
        };
        let refused =
            |packet: Packet| matches!(packet, Packet::Refused(why) if why.contains(NICK_TAKEN));

        assert!(matches!(
            host.hello(&named("Ann"), at(1), now),
            Packet::Lobby(_)
        ));
        assert!(matches!(
            host.hello(&named("Ann"), at(1), now),
            Packet::Lobby(_)
        ));
        assert!(refused(host.hello(&named("ann"), at(2), now)));
        assert!(refused(host.hello(&named("HOST"), at(3), now)));
        assert!(matches!(
            host.hello(&named(""), at(4), now),
            Packet::Lobby(_)
        ));
        assert!(matches!(
            host.hello(&named(""), at(5), now),
            Packet::Lobby(_)
        ));
    }

    /// A lobby packet fits a datagram read whole however long the chat's
    /// lines: as many as fit go, the rest with the next.
    #[test]
    fn a_lobby_packet_fits_its_datagram() {
        let now = Instant::now();
        let mut host = hosting(2);
        let from = SocketAddr::from(([127, 0, 0, 1], 9));
        let long = "€".repeat(super::super::CHAT_MOST);

        // 1,000 caps: some 7,000 bytes.
        let caps = (0..1_000u16)
            .map(|n| {
                let [low, high] = n.to_le_bytes();
                let letter = |b: u8| b'A' + b % 26;

                let code = [letter(low), letter(low / 26), letter(high), b'1'];

                (regent_core::data::fourcc::FourCc(code), 0)
            })
            .collect();

        host.set_setup(&Setup {
            limits: Some(caps),
            ..Setup::default()
        });

        for _ in 0..CHAT_LINES {
            host.say(&long);
        }

        let reply = host.hello(&hello(0, &[]), from, now);
        let lines = match &reply {
            Packet::Lobby(room) => room.chat.lines.len(),
            other => panic!("expected a lobby, got {other:?}"),
        };

        assert!(reply.encode().len() <= MOST_BYTES);
        assert!(lines > 0);
    }

    /// A save that comes wrong is dropped and said; the guest stays
    /// seated, and is not sent the same game again.
    #[test]
    fn a_save_that_comes_wrong_keeps_the_seat() {
        let host = UdpSocket::bind("127.0.0.1:0").expect("host");
        let at = host.local_addr().expect("host address");
        let now = Instant::now();
        let mut guest = Joining::new(at, (&build(), "Ann"), now).expect("joining");
        let port = guest.socket.local_addr().expect("guest address").port();
        let sent = Outgoing::new(vec![1, 2, 3]);
        let mut chunk = sent.chunks(0).remove(0);
        let offer = Offer {
            game: "g".to_owned(),
            file: sent.file.clone(),
            size: sent.size(),
            ..Offer::default()
        };

        chunk.bytes[1] = 9;
        guest.save = Save::Coming(
            Incoming::new(sent.file.clone(), sent.size()),
            "g".to_owned(),
        );
        host.send_to(&Packet::Chunk(chunk).encode(), ("127.0.0.1", port))
            .expect("sent");
        std::thread::sleep(Duration::from_millis(20));

        assert!(guest.poll(now).is_ok());
        assert!(guest.failed().is_some());
        assert_eq!(guest.holding(), Holding::None);

        guest.offered(Some(&offer));
        assert!(guest.failed().is_some(), "not sent again");
    }

    /// A host that gives up tells each guest more than once: one packet
    /// lost does not leave it waiting.
    #[test]
    fn a_host_leaving_says_so_more_than_once() {
        let now = Instant::now();
        let mut host = hosting(2);
        let guest = UdpSocket::bind("127.0.0.1:0").expect("guest");
        let mut buf = [0; MOST_BYTES];

        host.hello(&hello(0, &[]), guest.local_addr().expect("address"), now);
        host.leave();
        std::thread::sleep(Duration::from_millis(20));
        guest.set_nonblocking(true).expect("non-blocking");

        let told = std::iter::from_fn(|| {
            let (n, _) = guest.recv_from(&mut buf).ok()?;

            Some(matches!(Packet::decode(&buf[..n]), Ok(Packet::Refused(_))))
        })
        .filter(|&refused| refused)
        .count();

        assert!(told > 1, "{told}");
    }

    /// A seated guest still in the game, which answers no ping, is not
    /// flagged as unstable.
    #[test]
    fn a_guest_still_in_the_game_is_not_unstable() {
        let now = Instant::now();
        let mut host = hosting(2);
        let from = SocketAddr::from(([127, 0, 0, 1], 9));
        let game = Packet::Game(super::super::wire::Game::default()).encode();

        host.hello(&hello(0, &[]), from, now);

        for s in 0..5 {
            let at = now + Duration::from_secs(s);

            host.heard(&game, from, at);
            host.poll(at);
        }

        let at = now + Duration::from_secs(4);

        assert!(host.guest_addr(1).is_some(), "seated");
        assert!(!host.ping_stale(1, at));
    }

    /// Hellos `socket` has had, read now.
    fn hellos(socket: &UdpSocket) -> usize {
        let mut buf = [0; MOST_BYTES];

        socket.set_nonblocking(true).expect("non-blocking");
        std::iter::from_fn(|| {
            let (n, _) = socket.recv_from(&mut buf).ok()?;

            Some(matches!(Packet::decode(&buf[..n]), Ok(Packet::Hello(_))))
        })
        .filter(|&hello| hello)
        .count()
    }

    /// A guest being sent the save asks again once a window is in, or
    /// after a pause, not at every poll.
    #[test]
    fn a_save_being_sent_is_asked_for_at_a_pace() {
        use super::super::transfer::{CHUNK, Chunk};

        let host = UdpSocket::bind("127.0.0.1:0").expect("host");
        let at = host.local_addr().expect("host address");
        let now = Instant::now();
        let mut guest = Joining::new(at, (&build(), "Ann"), now).expect("joining");
        let size = u32::try_from(CHUNK * 100).expect("size");

        guest.save = Save::Coming(Incoming::new("f".to_owned(), size), "g".to_owned());

        for ms in 0..3 {
            let _ = guest.poll(now + Duration::from_millis(ms));
        }

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(hellos(&host), 1);

        for at in 0..WINDOW {
            guest.chunk(Chunk {
                file: "f".to_owned(),
                at,
                bytes: vec![0; CHUNK],
            });
        }

        let _ = guest.poll(now + Duration::from_millis(4));
        let _ = guest.poll(now + HELLO_EVERY);

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(hellos(&host), 1, "the next window asked, once");
    }

    /// The host loads another file of the same game while one is being
    /// sent: the guest takes the new file's chunks.
    #[test]
    fn another_file_of_the_game_is_sent_in_its_place() {
        let scratch = crate::testdata::scratch("lobby-another-file");
        let dir = scratch.path();

        let now = Instant::now();
        let host = SocketAddr::from(([127, 0, 0, 1], 9));
        let mut guest = Joining::new(host, (&build(), "Ann"), now).expect("joining");
        let offer = |file: &str| Offer {
            name: "game".to_owned(),
            game: "g".to_owned(),
            file: file.to_owned(),
            size: 10,
            ..Offer::default()
        };

        guest.set_saves(Some(Arc::new(crate::save::lan::Shelf::new(
            dir.to_path_buf(),
        ))));
        guest.offered(Some(&offer("f1")));
        guest.offered(Some(&offer("f2")));

        assert!(matches!(&guest.save, Save::Coming(incoming, _) if incoming.file == "f2"));
    }

    #[test]
    fn a_fold_sees_every_byte_and_the_tail() {
        assert_ne!(
            fold(fnv::START, b"abcdefgh1"),
            fold(fnv::START, b"abcdefgh2")
        );
        assert_ne!(fold(fnv::START, b"a"), fold(fnv::START, b"b"));
    }
}
