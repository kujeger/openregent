//! A lobby's client, as a front end drives it: the host's port bound
//! and its build hashed, a guest's host looked up, the lobby polled,
//! the game started. Nothing here draws or waits: what may block
//! (hashing this build, resolving a name) runs on a thread aside, and
//! [`Session::poll`] steps the rest.
//!
//! ```text
//!  host ──> Bound ──build hashed──> Hosting ──every guest ready, start──> Started::Hosted
//!  join ──> Resolving ──host and build──> Joining ──welcome──> Started::Welcomed
//!  regroup ──> Back ──build hashed──> Hosting (the host's) | Joining (a guest's)
//!  any ──failure──> Failed, idle;  cancel ──> idle, the guests told
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

use crate::files::env::Install;
use crate::files::mod_hash::{self, Holdings};
use crate::setup::Setup;

use super::{
    Build, Builds, Delay, Hosting, Joining, Loaded, Lobby, Peer, Ready, Regroup, Room, Saves,
    Unready, Welcome, build, iroh,
};

/// A thread's work, its result kept once done.
struct Aside<T> {
    running: Option<JoinHandle<Result<T, String>>>,
    done: Option<Result<T, String>>,
}

impl<T: Send + 'static> Aside<T> {
    fn spawn(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Aside<T> {
        Aside {
            running: Some(std::thread::spawn(work)),
            done: None,
        }
    }

    /// The result, once done.
    fn poll(&mut self) -> Option<&Result<T, String>> {
        let finished = self.running.as_ref().is_some_and(JoinHandle::is_finished);

        if finished && let Some(handle) = self.running.take() {
            let done = handle
                .join()
                .unwrap_or_else(|_| Err("a lobby thread panicked".to_owned()));

            self.done = Some(done);
        }

        self.done.as_ref()
    }
}

/// What a lobby waits on.
enum Wait {
    /// The host's port bound, the build still hashing; then `setup`'s game
    /// hosted, named `nick`, the session `title`.
    Bound {
        lobby: Lobby,
        setup: Box<Setup>,
        nick: String,
        title: String,
    },
    /// The host's lobby, `setup` the game it shows.
    Hosting {
        hosting: Hosting,
        setup: Box<Setup>,
    },
    /// The host's name resolving, or the build hashing.
    Resolving {
        address: Aside<SocketAddr>,
        nick: String,
    },
    Joining(Joining),
    /// Back from a game, the build still hashing: then the host's lobby
    /// again, `setup` its setup, named `title`, or the guest's.
    Back {
        regroup: Regroup,
        setup: Box<Setup>,
        title: String,
    },
}

/// When the host's game starts, and whether a guest is ready at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Starts {
    /// At [`Session::start`]; a guest at [`Session::flip_ready`].
    Pressed,
    /// As soon as every guest is ready, a guest ready from the first: a
    /// test's.
    #[cfg(test)]
    Ready,
}

/// How a game is reached: UDP on the LAN, or iroh's internet bridge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Service {
    #[default]
    Lan,
    Internet,
}

/// A game started: its peer, bridged if the lobby was.
pub enum Started {
    /// The host's: `setup` as the lobby showed it, going on from
    /// `saved`, if a save is loaded.
    Hosted {
        peer: Peer,
        setup: Setup,
        saved: Option<PathBuf>,
    },
    /// A guest's: as `welcome` asks, going on from this machine's copy
    /// of the host's save, `saved`.
    Welcomed {
        peer: Peer,
        welcome: Welcome,
        saved: Option<PathBuf>,
    },
}

/// What a poll came to.
pub enum Polled {
    Started(Box<Started>),
    /// The lobby stopped, and why: the session is idle again.
    Failed(String),
}

/// Where the session stands, for a front end to show.
pub enum Phase<'a> {
    Idle,
    /// The host's port bound, its build hashing.
    Bound {
        port: u16,
    },
    Hosting(&'a Hosting),
    /// The host's name resolving, or the build hashing.
    Resolving,
    Joining(&'a Joining),
    /// Back from a game, the build hashing.
    Back,
}

/// A lobby's client: hosts or joins one lobby at a time.
pub struct Session {
    /// The install whose build the lobby checks.
    install: Option<Install>,
    /// This build, hashed once.
    built: Option<Aside<Build>>,
    /// The install's mods, hashed: again after a rescan
    /// ([`Session::rehash`]).
    held: Option<Aside<Holdings>>,
    bridge: Option<Arc<iroh::Bridge>>,
    wait: Option<Wait>,
    starts: Starts,
    /// The profile's LAN saves: a save the host loads is looked for, and
    /// one sent goes, there.
    saves: Option<Arc<dyn Saves>>,
    /// Where a game's desyncs are written, asked as it starts.
    dumps: fn() -> Option<PathBuf>,
}

impl Session {
    /// An idle session, a game's desyncs written to `dumps`'s.
    pub fn new(dumps: fn() -> Option<PathBuf>) -> Session {
        Session {
            install: None,
            built: None,
            held: None,
            bridge: None,
            wait: None,
            starts: Starts::Pressed,
            saves: None,
            dumps,
        }
    }

    /// Games hosted and joined on `install`: its build is what the
    /// lobby checks.
    pub fn on(&mut self, install: Install) {
        self.install = Some(install);
        self.built = None;
        self.held = None;
    }

    /// The install's mods hashed again, as a rescan may change them.
    pub fn rehash(&mut self) {
        self.held = None;
    }

    /// The install's mods, hashed aside on the first ask; `None` while
    /// they are, or with no install.
    fn holdings(&mut self) -> Option<Holdings> {
        let install = self.install.clone();
        let aside = self.held.get_or_insert_with(|| {
            Aside::spawn(move || {
                let install = install.ok_or("no install to hash")?;

                Ok(mod_hash::holdings(&install.mods()))
            })
        });

        aside.poll().and_then(|done| done.as_ref().ok()).cloned()
    }

    /// This build, hashed aside on the first ask; `None` while it is.
    ///
    /// # Errors
    ///
    /// No install, or one that will not read: kept, not hashed again at
    /// every ask, until another install ([`Session::on`]).
    pub fn build(&mut self) -> Result<Option<Build>, String> {
        let install = self.install.clone();
        let aside = self.built.get_or_insert_with(|| {
            Aside::spawn(move || build(&install.ok_or("no install to check")?))
        });

        aside.poll().cloned().transpose()
    }

    /// The build and the mods hashed ahead, as a page that needs them
    /// opens.
    pub fn warm(&mut self) {
        let _ = self.build();
        let _ = self.holdings();
    }

    /// The profile's LAN saves, where a LAN save is looked for and one
    /// sent goes.
    pub fn set_saves(&mut self, saves: Option<Arc<dyn Saves>>) {
        self.saves = saves;
    }

    /// The saves directory [`Session::set_saves`] set: a test's.
    #[cfg(test)]
    pub fn saves(&self) -> Option<&std::path::Path> {
        self.saves.as_deref().map(Saves::dir)
    }

    /// Where the session stands.
    pub fn phase(&self) -> Phase<'_> {
        match &self.wait {
            None => Phase::Idle,
            Some(Wait::Bound { lobby, .. }) => Phase::Bound { port: lobby.port() },
            Some(Wait::Hosting { hosting, .. }) => Phase::Hosting(hosting),
            Some(Wait::Resolving { .. }) => Phase::Resolving,
            Some(Wait::Joining(joining)) => Phase::Joining(joining),
            Some(Wait::Back { .. }) => Phase::Back,
        }
    }

    /// Host `setup`'s game over `service` on `port`, 0 a port the system
    /// picks, named `nick`, the session `title`; an internet game's lobby
    /// is on loopback, any port. The guests are waited for from here on.
    ///
    /// # Errors
    ///
    /// A port that will not bind: the session as it was.
    pub fn host(
        &mut self,
        (service, port): (Service, u16),
        setup: Setup,
        (nick, title): (String, String),
    ) -> Result<(), String> {
        let (lobby, bridge) = match service {
            Service::Lan => (Lobby::bind(port)?, None),
            Service::Internet => {
                let lobby = Lobby::bind_local()?;
                let game = SocketAddr::from(([127, 0, 0, 1], lobby.port()));
                let bridge = iroh::Bridge::host(game, iroh::home());
                (lobby, Some(bridge))
            }
        };

        self.bridge = bridge;
        self.wait = Some(Wait::Bound {
            lobby,
            setup: Box::new(setup),
            nick,
            title,
        });
        Ok(())
    }

    /// Join the host at `address` over `service`, named `nick`: a LAN
    /// host's name resolved aside, then asked for a seat from here on.
    ///
    /// # Errors
    ///
    /// An internet invite that will not open: the session as it was.
    pub fn join(&mut self, service: Service, address: &str, nick: String) -> Result<(), String> {
        match service {
            Service::Lan => {
                let host = address.trim().to_owned();
                let address = Aside::spawn(move || super::address(&host));
                self.wait = Some(Wait::Resolving { address, nick });
                Ok(())
            }
            Service::Internet => {
                let bridge = iroh::Bridge::join(address, iroh::home())?;
                let local = bridge.local().expect("guest bridge has a local address");
                let address = Aside::spawn(move || Ok(local));
                self.bridge = Some(bridge);
                self.wait = Some(Wait::Resolving { address, nick });
                Ok(())
            }
        }
    }

    /// Back from a LAN game: its lobby again on the game's socket, the
    /// host's showing `setup`, named `title`.
    pub fn regroup(&mut self, regroup: Regroup, setup: Setup, title: String) {
        self.wait = Some(Wait::Back {
            regroup,
            setup: Box::new(setup),
            title,
        });
    }

    /// The lobby given up: its socket let go, the guests told.
    pub fn cancel(&mut self) {
        if let Some(Wait::Hosting { hosting, .. }) = self.wait.take() {
            hosting.leave();
        }

        self.bridge = None;
    }

    /// The host's LAN save `loaded` loaded: offered to the guests.
    ///
    /// # Errors
    ///
    /// Not the host, or a save that will not load into this lobby
    /// ([`Hosting::choose`]).
    pub fn choose(&mut self, loaded: Loaded) -> Result<(), String> {
        match &mut self.wait {
            Some(Wait::Hosting { hosting, .. }) => hosting.choose(loaded),
            _ => Err("only the host loads a game".to_owned()),
        }
    }

    /// Lobby seat `at` moved to the next saved seat: the host's.
    pub fn reseat(&mut self, at: usize) {
        if let Some(Wait::Hosting { hosting, .. }) = &mut self.wait {
            hosting.reseat(at);
        }
    }

    /// The host's setup now `setup`: the one every guest sees, a change
    /// lapsing their ready.
    pub fn share(&mut self, new: Setup) {
        match &mut self.wait {
            Some(Wait::Hosting { hosting, setup }) => {
                **setup = new;
                hosting.unchoose();
                hosting.set_setup(&setup.recorded());
            }
            Some(Wait::Bound { setup, .. }) => **setup = new,
            _ => {}
        }
    }

    /// The host's game started: every guest welcomed.
    ///
    /// # Errors
    ///
    /// Not the host, a seat free, a guest not ready: the session as it
    /// was. A socket that fails: the lobby gone.
    pub fn start(&mut self) -> Result<Started, String> {
        match self.wait.take() {
            Some(Wait::Hosting { hosting, setup }) => match hosting.unready() {
                None => self.hosted(hosting, *setup),
                Some(unready) => {
                    self.wait = Some(Wait::Hosting { hosting, setup });
                    Err(said(unready).to_owned())
                }
            },
            wait => {
                self.wait = wait;
                Err("only the host starts the game".to_owned())
            }
        }
    }

    /// Whether [`Session::start`] would start the game.
    pub fn startable(&self) -> bool {
        matches!(&self.wait, Some(Wait::Hosting { hosting, .. }) if hosting.unready().is_none())
    }

    /// A guest's ready flipped: for the host's setup as it stands.
    pub const fn flip_ready(&mut self) {
        if let Some(Wait::Joining(joining)) = &mut self.wait {
            let ready = match (joining.ready(), joining.room()) {
                (false, Some(room)) => Ready::For(room.revision),
                _ => Ready::No,
            };

            joining.set_ready(ready);
        }
    }

    /// This player's chat `line`, to the lobby.
    pub fn say(&mut self, line: &str) {
        let line = line.trim();

        if !line.is_empty() {
            match &mut self.wait {
                Some(Wait::Hosting { hosting, .. }) => hosting.say(line),
                Some(Wait::Joining(joining)) => joining.say(line),
                _ => {}
            }
        }
    }

    /// A guest's players, once seated.
    pub fn seated_players(&self) -> Option<u32> {
        match &self.wait {
            Some(Wait::Joining(joining)) => joining.seated().map(|(_, n)| n),
            _ => None,
        }
    }

    /// Whether this machine hosts the lobby.
    pub const fn hosts(&self) -> bool {
        match &self.wait {
            Some(Wait::Bound { .. } | Wait::Hosting { .. }) => true,
            Some(Wait::Back { regroup, .. }) => regroup.hosts(),
            _ => false,
        }
    }

    /// The lobby as last heard: the host's own, or a guest's of it.
    pub fn room(&self) -> Option<Room> {
        match &self.wait {
            Some(Wait::Hosting { hosting, .. }) => Some(hosting.view()),
            Some(Wait::Joining(joining)) => joining.room().map(|room| Room {
                chat: super::Lines {
                    first: 0,
                    lines: joining.chat().to_vec(),
                },
                ..room.clone()
            }),
            _ => None,
        }
    }

    /// The internet host's invite, once its bridge has one.
    pub fn invite(&self) -> Option<String> {
        self.hosts()
            .then_some(self.bridge.as_ref())?
            .and_then(|bridge| bridge.invite())
    }

    /// Seat `seat`'s link as this machine sees it: a guest sees only the
    /// host's, seat 0.
    pub fn health(&self, seat: usize) -> Option<iroh::Health> {
        let now = Instant::now();
        let (addr, ping, unstable) = match &self.wait {
            Some(Wait::Hosting { hosting, .. }) => (
                hosting.guest_addr(seat)?,
                hosting.ping(seat, now),
                hosting.ping_stale(seat, now),
            ),
            Some(Wait::Joining(joining)) if seat == 0 => (
                self.bridge
                    .as_ref()
                    .and_then(|b| b.local())
                    .unwrap_or(joining.host()),
                joining.ping(now),
                joining.ping_stale(now),
            ),
            _ => return None,
        };
        let mut status = self.bridge.as_ref().map_or(
            iroh::Health {
                connected: true,
                ..iroh::Health::default()
            },
            |bridge| bridge.health(addr),
        );
        status.ping_ms = ping.map(|rtt| rtt.as_millis().try_into().unwrap_or(u32::MAX));
        status.unstable = unstable;
        Some(status)
    }

    /// The session's name, once a lobby is begun.
    pub fn title(&self) -> Option<String> {
        match &self.wait {
            Some(Wait::Bound { title, .. } | Wait::Back { title, .. }) => Some(title.clone()),
            Some(Wait::Hosting { .. } | Wait::Joining(_)) => {
                Some(self.room().map(|r| r.title).unwrap_or_default())
            }
            _ => None,
        }
    }

    /// The lobby polled at `now`: its game started, or why it stopped.
    pub fn poll(&mut self, now: Instant) -> Option<Polled> {
        if let Some(why) = self.bridge.as_ref().and_then(|bridge| bridge.error()) {
            self.wait = None;
            self.bridge = None;
            Some(Polled::Failed(why))
        } else {
            let wait = self.wait.take()?;

            match self.step(wait, now) {
                Ok(Step::Waiting(wait)) => {
                    self.wait = Some(*wait);
                    None
                }
                Ok(Step::Started(started)) => Some(Polled::Started(started)),
                Err(why) => Some(Polled::Failed(why)),
            }
        }
    }

    /// `wait` a step on at `now`.
    fn step(&mut self, wait: Wait, now: Instant) -> Result<Step, String> {
        match wait {
            Wait::Bound {
                lobby,
                setup,
                nick,
                title,
            } => match self.build()? {
                Some(build) => {
                    let recorded = setup.recorded();
                    let players = (setup.players, Delay::Auto);
                    let builds = (&build, Builds::Same);
                    let mut hosting = lobby.host(builds, players, (&recorded, &nick))?;

                    hosting.set_title(&title);
                    Ok(Step::waiting(Wait::Hosting { hosting, setup }))
                }
                None => Ok(Step::waiting(Wait::Bound {
                    lobby,
                    setup,
                    nick,
                    title,
                })),
            },
            Wait::Hosting { mut hosting, setup } => {
                if let Some(holdings) = self.holdings() {
                    hosting.set_holdings(&holdings);
                }

                hosting.poll(now);

                // Nicks follow the lobby's seats: kept as a guest sits,
                // freed as it leaves.
                if let Some(bridge) = &self.bridge {
                    bridge.protect(&hosting.identities());
                }

                if self.starts != Starts::Pressed && hosting.unready().is_none() {
                    let started = self.hosted(hosting, *setup)?;

                    Ok(Step::Started(Box::new(started)))
                } else {
                    Ok(Step::waiting(Wait::Hosting { hosting, setup }))
                }
            }
            Wait::Resolving { mut address, nick } => {
                let resolved = address.poll().cloned().transpose()?;

                match (resolved, self.build()?) {
                    (Some(host), Some(build)) => {
                        let joining = Joining::new(host, (&build, &nick), now)?;

                        Ok(Step::waiting(Wait::Joining(self.asking(joining))))
                    }
                    _ => Ok(Step::waiting(Wait::Resolving { address, nick })),
                }
            }
            Wait::Back {
                regroup,
                setup,
                title,
            } => match (self.build()?, regroup.hosts()) {
                (Some(build), true) => {
                    let recorded = setup.recorded();
                    let builds = (&build, Builds::Same);
                    let hosting = regroup.host(builds, (&recorded, &title), now)?;

                    Ok(Step::waiting(Wait::Hosting { hosting, setup }))
                }
                (Some(build), false) => {
                    let joining = regroup.join(&build, now)?;

                    Ok(Step::waiting(Wait::Joining(self.asking(joining))))
                }
                (None, _) => Ok(Step::waiting(Wait::Back {
                    regroup,
                    setup,
                    title,
                })),
            },
            Wait::Joining(mut joining) => {
                if let Some(holdings) = self.holdings() {
                    joining.set_holdings(&holdings);
                }

                match joining.poll(now)? {
                    Some(welcome) => {
                        let saved = welcome
                            .save
                            .as_ref()
                            .map(|game| joining.copy(game).ok_or("the host's save is not here"))
                            .transpose()?;
                        let peer = Peer::welcomed(joining, &welcome, (self.dumps)())?;

                        Ok(Step::Started(Box::new(Started::Welcomed {
                            peer: self.keep_bridge(peer),
                            welcome,
                            saved,
                        })))
                    }
                    None => Ok(Step::waiting(Wait::Joining(joining))),
                }
            }
        }
    }

    /// A guest's lobby, its saves looked for where the profile keeps
    /// them; a test's guest ready from the first.
    fn asking(&self, mut joining: Joining) -> Joining {
        joining.set_saves(self.saves.clone());

        if self.starts != Starts::Pressed {
            joining.set_ready(Ready::Always);
        }

        joining
    }

    /// The host's game once `hosting` starts: `setup`'s, or the save
    /// loaded.
    ///
    /// # Errors
    ///
    /// A socket that fails.
    fn hosted(&self, hosting: Hosting, setup: Setup) -> Result<Started, String> {
        let (peer, saved) = Peer::seated(hosting, (self.dumps)())?;

        Ok(Started::Hosted {
            peer: self.keep_bridge(peer),
            setup,
            saved,
        })
    }

    /// `peer` bridged as the lobby is: a LAN game keeps its own link.
    fn keep_bridge(&self, peer: Peer) -> Peer {
        match &self.bridge {
            Some(bridge) => peer.bridged(bridge.clone()),
            None => peer,
        }
    }

    /// The host's game started as soon as every guest is ready, and a
    /// guest ready from the first: a test's.
    #[cfg(test)]
    pub const fn start_at_once(&mut self) {
        self.starts = Starts::Ready;
    }

    /// The host's lobby's port while its build hashes: a test's.
    #[cfg(test)]
    pub fn bound_port(&self) -> Option<u16> {
        match &self.wait {
            Some(Wait::Bound { lobby, .. }) => Some(lobby.port()),
            _ => None,
        }
    }

    /// These mods taken as hashed already: a test's.
    #[cfg(test)]
    pub(crate) fn held(&mut self, holdings: Holdings) {
        self.held = Some(Aside {
            running: None,
            done: Some(Ok(holdings)),
        });
    }

    /// This build taken as hashed already: a test's.
    #[cfg(test)]
    pub(crate) fn built(&mut self, build: Build) {
        self.built = Some(Aside {
            running: None,
            done: Some(Ok(build)),
        });
    }
}

/// Why the host may not start, as said.
const fn said(unready: Unready) -> &'static str {
    match unready {
        Unready::Seats => "a seat is free",
        Unready::Guests => "a player is not ready",
        Unready::Save => "a player lacks the save",
        Unready::Mods => "a player lacks a mod, or has another version",
    }
}

/// Where a lobby's step leaves it.
enum Step {
    Waiting(Box<Wait>),
    Started(Box<Started>),
}

impl Step {
    fn waiting(wait: Wait) -> Step {
        Step::Waiting(Box::new(wait))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::wire::{ModMatch, Profile};
    use super::*;

    /// How long a lobby's step may take: generous, as a loaded machine
    /// may not run a thread for a while.
    const SETTLE: Duration = Duration::from_secs(30);

    /// What each session's polls came to, by session.
    type Came = Vec<Option<Polled>>;

    /// Each session polled until `done` holds of what its polls came
    /// to; panics past [`SETTLE`].
    fn until(
        what: &str,
        sessions: &mut [&mut Session],
        mut done: impl FnMut(&Came, &[&mut Session]) -> bool,
    ) -> Came {
        let deadline = Instant::now() + SETTLE;
        let mut came: Came = sessions.iter().map(|_| None).collect();

        loop {
            for (session, came) in sessions.iter_mut().zip(&mut came) {
                if came.is_none() {
                    *came = session.poll(Instant::now());
                }
            }

            if done(&came, sessions) {
                break came;
            }

            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Every session polled until each has started or failed.
    fn all(what: &str, sessions: &mut [&mut Session]) -> Came {
        until(what, sessions, |came, _| came.iter().all(Option::is_some))
    }

    /// A session whose build is hashed already.
    fn session() -> Session {
        let mut session = Session::new(|| None);

        session.built(Build {
            version: "test".to_owned(),
            profile: Profile::this(),
            commit: "test".to_owned(),
            data: 2,
        });
        session
    }

    /// A game of two.
    fn two() -> Setup {
        Setup {
            players: 2,
            ..Setup::default()
        }
    }

    /// A host bound on a port the system picks, and a guest joining it.
    fn hosted_and_joined(starts: Starts) -> (Session, Session) {
        let (mut host, mut guest) = (session(), session());

        host.starts = starts;
        guest.starts = starts;
        host.host(
            (Service::Lan, 0),
            two(),
            ("Host".to_owned(), "Ann's".to_owned()),
        )
        .expect("bound");

        let port = host.bound_port().expect("bound");

        assert!(host.hosts());
        assert_eq!(host.title().as_deref(), Some("Ann's"));

        guest
            .join(
                Service::Lan,
                &format!(" 127.0.0.1:{port} "),
                "Ann".to_owned(),
            )
            .expect("joining");
        assert!(matches!(guest.phase(), Phase::Resolving));
        assert!(!guest.hosts());
        (host, guest)
    }

    /// A guest's game started: its peer and welcome.
    fn welcomed(came: Option<Polled>) -> (Peer, Welcome) {
        match came {
            Some(Polled::Started(started)) => match *started {
                Started::Welcomed { peer, welcome, .. } => (peer, welcome),
                Started::Hosted { .. } => panic!("a guest hosts"),
            },
            _ => panic!("not started"),
        }
    }

    /// The host's game started, no save loaded: its peer and setup.
    fn hosted(came: Option<Polled>) -> (Peer, Setup) {
        match came {
            Some(Polled::Started(started)) => match *started {
                Started::Hosted { peer, setup, saved } => {
                    assert!(saved.is_none());
                    (peer, setup)
                }
                Started::Welcomed { .. } => panic!("the host is welcomed"),
            },
            _ => panic!("not started"),
        }
    }

    /// The two games a host's and a guest's polls came to.
    fn both(came: Came) -> ((Peer, Setup), (Peer, Welcome)) {
        let mut came = came.into_iter();
        let host = hosted(came.next().flatten());

        (host, welcomed(came.next().flatten()))
    }

    /// Host and join: bound, then hosting; resolving, then joining; once
    /// the guest is ready the host's game starts, the guest welcomed to
    /// it.
    #[test]
    fn a_host_and_a_guest_both_start_the_game() {
        let (mut host, mut guest) = hosted_and_joined(Starts::Ready);
        let ((peer, setup), (guest_peer, welcome)) =
            both(all("started", &mut [&mut host, &mut guest]));

        assert_eq!(setup, two());
        assert_eq!((peer.seat(), guest_peer.seat()), (0, 1));
        assert_eq!((welcome.seat, welcome.players), (1, 2));
        assert_eq!(welcome.setup, two().recorded());
        assert!(matches!(host.phase(), Phase::Idle));
        assert!(matches!(guest.phase(), Phase::Idle));
    }

    /// The host's start waits for START GAME and a ready guest: refused
    /// before, the lobby kept; the guest's ready flipped, it starts.
    #[test]
    fn start_waits_for_a_ready_guest() {
        let (mut host, mut guest) = hosted_and_joined(Starts::Pressed);

        assert!(guest.start().is_err_and(|why| why.contains("host")));
        until("seated", &mut [&mut host, &mut guest], |_, s| {
            s[1].seated_players() == Some(2) && s[1].room().is_some()
        });
        assert_eq!(guest.title().as_deref(), Some("Ann's"));
        assert!(!host.startable());
        assert_eq!(host.start().err().as_deref(), Some("a player is not ready"));
        assert!(matches!(host.phase(), Phase::Hosting(_)));

        guest.flip_ready();
        until("ready", &mut [&mut host, &mut guest], |_, s| {
            s[0].startable()
        });
        hosted(Some(Polled::Started(Box::new(
            host.start().expect("started"),
        ))));
        welcomed(all("welcomed", &mut [&mut guest]).pop().flatten());
    }

    /// A guest lacking a mod the host's setup names holds the start, its
    /// seat marked; once it holds the host's version, the game starts,
    /// the welcome naming the mod.
    #[test]
    fn a_guest_lacking_a_mod_holds_the_start() {
        let guid = regent_core::data::guid::Guid::parse("{17BF53DE-722F-4683-8E63-6B93D1606DFF}")
            .expect("a GUID");
        let held: Holdings = [(
            guid,
            mod_hash::Held {
                title: "New Game+".to_owned(),
                hash: "aa".to_owned(),
            },
        )]
        .into();
        let setup = Setup {
            mods: vec![guid.into()],
            ..two()
        };
        let (mut host, mut guest) = (session(), session());

        host.held(held.clone());
        guest.held(Holdings::new());
        host.host((Service::Lan, 0), setup, ("Host".to_owned(), String::new()))
            .expect("bound");

        let port = host.bound_port().expect("bound");

        guest
            .join(Service::Lan, &format!("127.0.0.1:{port}"), "Ann".to_owned())
            .expect("joining");
        until("seated", &mut [&mut host, &mut guest], |_, s| {
            s[1].room().is_some()
        });
        guest.flip_ready();

        let marked = |s: &[&mut Session]| {
            s[0].room().is_some_and(|r| {
                r.seats
                    .get(1)
                    .is_some_and(|seat| seat.mods == [ModMatch::Missing])
            })
        };

        until("marked", &mut [&mut host, &mut guest], |_, s| marked(s));
        assert_eq!(
            host.start().err().as_deref(),
            Some("a player lacks a mod, or has another version")
        );

        guest.held(held);
        until("startable", &mut [&mut host, &mut guest], |_, s| {
            s[0].startable()
        });
        hosted(Some(Polled::Started(Box::new(
            host.start().expect("started"),
        ))));

        let (_, welcome) = welcomed(all("welcomed", &mut [&mut guest]).pop().flatten());

        assert_eq!(welcome.setup.mods, [guid.into()]);
    }

    /// A name that does not resolve fails aside: said, the session idle.
    #[test]
    fn an_unknown_host_fails() {
        let mut guest = session();

        guest
            .join(Service::Lan, "no such host.invalid", String::new())
            .expect("resolving");

        let came = all("failed", &mut [&mut guest]);

        assert!(matches!(&came[0], Some(Polled::Failed(why)) if why.contains("no spaces")));
        assert!(matches!(guest.phase(), Phase::Idle));
    }

    /// The host's port bound while the build hashes, which fails with no
    /// install: said, the port let go; the failure kept, not hashed again
    /// at every ask.
    #[test]
    fn a_host_with_no_build_fails() {
        let mut host = Session::new(|| None);

        host.host((Service::Lan, 0), two(), (String::new(), String::new()))
            .expect("bound");
        assert!(matches!(host.phase(), Phase::Bound { .. }));

        let came = all("failed", &mut [&mut host]);

        assert!(matches!(&came[0], Some(Polled::Failed(why)) if why == "no install to check"));
        assert!(matches!(host.phase(), Phase::Idle));
        assert!(host.build().is_err());
        assert!(host.built.is_some(), "kept");
    }

    /// The host cancels: idle, and the seated guest told, its lobby
    /// stopped.
    #[test]
    fn a_cancelled_lobby_tells_its_guest() {
        let (mut host, mut guest) = hosted_and_joined(Starts::Pressed);

        until("seated", &mut [&mut host, &mut guest], |_, s| {
            s[1].seated_players().is_some()
        });
        host.cancel();
        assert!(matches!(host.phase(), Phase::Idle));

        let came = all("told", &mut [&mut guest]);

        assert!(matches!(&came[0], Some(Polled::Failed(why)) if why.contains("the host left")));
    }

    /// An internet lobby keeps a seated guest's nick for its key before
    /// any game starts, and frees it once the guest has left.
    #[test]
    fn a_lobby_nick_is_kept_for_its_key() {
        let (mut host, mut guest) = hosted_and_joined(Starts::Pressed);
        let bridge = Arc::new(iroh::Bridge::idle());
        let key = ::iroh::SecretKey::from_bytes(&[1; 32]).public();

        host.bridge = Some(bridge.clone());
        until("seated", &mut [&mut host, &mut guest], |_, s| {
            s[1].seated_players() == Some(2)
        });

        let addr = match &host.wait {
            Some(Wait::Hosting { hosting, .. }) => hosting.identities()[0].0,
            _ => panic!("not hosting"),
        };

        bridge.known(addr, key);
        until("kept", &mut [&mut host], |_, _| {
            bridge.owner("Ann") == Some(key)
        });

        drop(guest);
        until("freed", &mut [&mut host], |_, _| {
            bridge.owner("Ann").is_none()
        });
    }

    /// Back from a game: the guest waits while the host plays on, then is
    /// seated in the host's lobby again, and both start the next game.
    #[test]
    fn a_regrouped_lobby_starts_again() {
        let (mut host, mut guest) = hosted_and_joined(Starts::Ready);
        let ((mut host_peer, _), (mut guest_peer, _)) =
            both(all("started", &mut [&mut host, &mut guest]));

        for _ in 0..50 {
            host_peer.pump(Instant::now());
            guest_peer.pump(Instant::now());
            std::thread::sleep(Duration::from_millis(2));
        }

        let back = guest_peer.regroup().expect("a socket");

        guest.regroup(back, two(), String::new());
        assert!(matches!(guest.phase(), Phase::Back));
        assert!(!guest.hosts());
        until("held", &mut [&mut guest], |_, s| {
            host_peer.pump(Instant::now());
            matches!(s[0].phase(), Phase::Joining(j) if j.held())
        });

        let back = host_peer.regroup().expect("a socket");

        host.regroup(back, two(), "Again".to_owned());
        assert!(host.hosts());
        assert_eq!(host.title().as_deref(), Some("Again"));

        let (_, (_, welcome)) = both(all("started again", &mut [&mut host, &mut guest]));

        assert_eq!(welcome.seat, 1);
    }
}
