//! The packets, as bytes: little-endian words, strings with a `u16`
//! length, orders as their text (`Order`'s `Display`, `p1 build ...`).
//!
//! ```text
//!  "MJLN" version kind  body
//!  kind 1 Hello    build: version, profile, commit, data hash; nick,
//!                  ready, chat held, own chat (first, lines), the save
//!                  loaded (0 none | 1 have game | 2 want game, from),
//!                  the setup's mods held (GUID, hash?)
//!       2 Lobby    seat, players, delay, title, revision, setup (RON),
//!                  the setup's mods' titles, seats (nick, build, ready,
//!                  each mod's match), own chat taken, chat
//!                  (first, (0 who line | 1 settings changed)...), the
//!                  save loaded (name, game, file, size, saved nicks,
//!                  each seat's saved seat)?: in the lobby, waiting
//!       3 Welcome  seat, players, delay, setup (RON), nicks,
//!                  the host's seat, the save's game?
//!       4 Refused  why
//!       5 Game     from, epoch, paused mask, speed, dropped (seat, after,
//!                  how)..., waited on (seat, ms to its drop)..., round
//!                  trips (seat, ms)..., acks
//!                  (frames, chat) a seat, bundles (seat, frame, step, digest,
//!                  deep (frame, digest)?, items: order, play on, save
//!                  (name), delay (frames), surrender), chat (seat, seq,
//!                  line: a spectator's opens SPECTATORS_ONLY), resync
//!                  (0 none | 1 offer: frame, file, size, delay in force,
//!                  seat rejoining? | 2 want:
//!                  file, from | 3 stop: why)
//!       6 Bye      from: a peer leaving, sent a few times
//!       7 Chunk    file, at, bytes: of the save loaded, or of a resync
//!                  (`transfer`)
//!       8 Wait     the host plays on: a guest back in the lobby asks
//!                  again (`lobby`)
//!       9 Regroup  the host is back in its lobby: each guest comes
//!      10 Open     a host's lobby, announced (`discover`): id, port,
//!                  title, nick, seats taken, players, setup (RON),
//!                  the save loaded?, build, any build
//!      11 Ping    probe number
//!      12 Pong    the probe number answered
//! ```

use regent_core::sim::game::Order;

use regent_core::data::guid::Guid;

use crate::setup::Setup;

/// Opens every packet.
const MAGIC: [u8; 4] = *b"MJLN";

/// The protocol's version: a peer of another is not understood.
pub(super) const VERSION: u8 = 1;

/// The largest packet read, on every link: a lobby packet carries the
/// setup, building caps included, and some chat; game packets are kept
/// well under it (`node::BUDGET`); a file's chunk (`transfer::CHUNK`)
/// fits.
pub(super) const MOST_BYTES: usize = 16 * 1024;

const HELLO: u8 = 1;
const LOBBY: u8 = 2;
const WELCOME: u8 = 3;
const REFUSED: u8 = 4;
const GAME: u8 = 5;
const BYE: u8 = 6;
const CHUNK: u8 = 7;
const WAIT: u8 = 8;
const REGROUP: u8 = 9;
const OPEN: u8 = 10;
const PING: u8 = 11;
const PONG: u8 = 12;

/// An open lobby's builds tag: the same commit only, or any.
const BUILDS_SAME: u8 = 0;
const BUILDS_ANY: u8 = 1;

/// A hello's tag for the save loaded: none, held, wanted.
const HOLD_NONE: u8 = 0;
const HOLD_HAVE: u8 = 1;
const HOLD_WANT: u8 = 2;

/// A hello's ready tag: not, for a revision, always.
const NOT_READY: u8 = 0;
const READY_FOR: u8 = 1;
const READY_ALWAYS: u8 = 2;

/// A seat's match of a mod the setup names, as sent.
const MATCH_UNKNOWN: u8 = 0;
const MATCH_SAME: u8 = 1;
const MATCH_MISSING: u8 = 2;
const MATCH_DIFFERS: u8 = 3;

/// A lobby chat line's tag: a player's line, or the settings changed.
const SAID: u8 = 0;
const CHANGED: u8 = 1;

/// How a seat left: a goodbye, or silence.
const QUIT: u8 = 0;
const SILENT: u8 = 1;

/// A game packet's resync tag: none, the host's offer, a guest's want,
/// the host's stop.
const SYNC_NONE: u8 = 0;
const SYNC_OFFER: u8 = 1;
const SYNC_WANT: u8 = 2;
const SYNC_STOP: u8 = 3;

/// An item's tag: an order, CONTINUE PLAYING, a save, a delay, or a
/// surrender.
const ORDER: u8 = 0;
const PLAY_ON: u8 = 1;
const SAVE: u8 = 2;
const DELAY: u8 = 3;
const SURRENDER: u8 = 4;

/// A build's profile: peers must agree, as the sim's `debug_assert!`s
/// panic in debug alone. Both profiles check overflow (`Cargo.toml`;
/// `regent_core`'s `overflow_panics` test); stable Rust tells a crate only
/// `debug_assertions`, not its overflow checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Release,
    Debug,
}

impl Profile {
    /// This build's.
    pub const fn this() -> Profile {
        if cfg!(debug_assertions) {
            Profile::Debug
        } else {
            Profile::Release
        }
    }
}

/// What a peer runs: it must be what the host runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    /// The crate's version.
    pub version: String,
    pub profile: Profile,
    /// The git commit built (`build.rs`). Stands in for the version
    /// while that stays 0.1 through development; then the version alone.
    pub commit: String,
    /// The game data a freestyle game reads, hashed (`lobby::data_hash`).
    pub data: u64,
}

impl Build {
    /// As the lobby shows it beside a nick: version and short commit.
    ///
    /// ```text
    ///  v0.1.0 1a2b3c4, v0.1.0 1a2b3c4-dirty
    /// ```
    #[must_use]
    pub fn shown(&self) -> String {
        let (commit, mark) = self
            .commit
            .split_once('-')
            .map_or((self.commit.as_str(), String::new()), |(c, m)| {
                (c, format!("-{m}"))
            });
        let short: String = commit.chars().take(SHORT_COMMIT).collect();

        format!("v{} {short}{mark}", self.version)
    }
}

/// A commit's characters as the lobby shows them.
const SHORT_COMMIT: usize = 7;

/// Whether a guest is ready to start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ready {
    No,
    /// For the host's setup of this revision: a change lapses it.
    For(u32),
    /// Whatever the setup: the command line's guest.
    Always,
}

/// Chat lines of a stream from its `first`, numbered from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lines<T> {
    pub first: u32,
    pub lines: Vec<T>,
}

impl<T> Default for Lines<T> {
    fn default() -> Lines<T> {
        Lines {
            first: 0,
            lines: Vec::new(),
        }
    }
}

/// A line of the lobby's chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Talk {
    /// A player's: who, the line.
    Said(String, String),
    /// The host changed the settings: each peer says so in its own
    /// words, `GMTX` 314.
    Changed,
}

/// A guest's ask for a seat, sent again as its heartbeat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hello {
    pub build: Build,
    /// The player's name, its profile's; empty, none.
    pub nick: String,
    pub ready: Ready,
    /// The host's chat lines it holds.
    pub heard: u32,
    /// Its own chat lines the host has not taken.
    pub said: Lines<String>,
    /// What it holds of the save the host loads.
    pub holding: Holding,
    /// Each mod the host's setup names, by GUID: its files' hash here
    /// (`files::mod_hash`), `None` where no folder holds it.
    pub mods: Vec<(Guid, Option<String>)>,
}

/// What a guest holds of the save the host loads, by its game's hash.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Holding {
    #[default]
    None,
    /// A copy, its own or sent.
    Have(String),
    /// Being sent: the first chunk it lacks.
    Want { game: String, from: u32 },
}

/// The save the host loads, as the lobby offers it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Offer {
    pub name: String,
    /// Its game's hash (`save::Lan::game`): every peer's copy has it.
    pub game: String,
    /// The host's file's hash and length: what a guest that lacks it
    /// is sent.
    pub file: String,
    pub size: u32,
    /// Each saved seat's nick.
    pub seats: Vec<String>,
    /// Each lobby seat's saved seat, by lobby seat.
    pub sits: Vec<u32>,
}

/// A seat as the lobby shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub nick: String,
    /// Its build, [`Build::shown`].
    pub build: String,
    /// The host's seat always is.
    pub ready: bool,
    /// Its match of each mod the setup names, in the setup's order.
    pub mods: Vec<ModMatch>,
}

/// How a seat's copy of a mod the setup names matches the host's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ModMatch {
    /// Not said yet.
    #[default]
    Unknown,
    Same,
    /// No mod folder holds it.
    Missing,
    /// Its files hash otherwise: another version.
    Differs,
}

/// The host's lobby, as it answers a guest's hello.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Room {
    /// The guest's seat.
    pub seat: u32,
    pub players: u32,
    /// The delay a game started now would have, in frames; 0 if unsaid.
    pub delay: u32,
    /// The session's name.
    pub title: String,
    /// The setup's revision: each change counts one up.
    pub revision: u32,
    /// The recording's setup the game is made from ([`Setup::recorded`]).
    pub setup: Setup,
    /// Each mod the setup names, its title as the host has it.
    pub mods: Vec<String>,
    /// Each seat taken, by seat.
    pub seats: Vec<Seat>,
    /// The guest's own chat lines taken so far.
    pub taken: u32,
    pub chat: Lines<Talk>,
    /// The save the host loads; none, a new game of the setup.
    pub save: Option<Offer>,
}

/// The host's answer once every seat is taken: what to make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Welcome {
    pub seat: u32,
    pub players: u32,
    /// Frames from an order's giving to the frame it comes before.
    pub delay: u32,
    /// The recording's setup ([`Setup::recorded`]): the game's options.
    pub setup: Setup,
    /// Each seat's nick, by seat: empty, none.
    pub nicks: Vec<String>,
    /// The host's seat: 0 but in a loaded game.
    pub host: u32,
    /// The loaded save's game hash: the game goes on from it.
    pub save: Option<String>,
}

/// How a seat left the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leave {
    /// It said goodbye: QUIT, or back to the menus.
    Quit,
    /// It fell silent (`node::DROP_AFTER`), or the host dropped it.
    Silent,
}

/// What a peer has of each seat's streams, contiguous from the start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ack {
    /// The last frame whose bundle it holds.
    pub frames: u32,
    /// Chat lines it holds.
    pub chat: u32,
}

/// Something a seat did, for a frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Order(Order),
    /// The seat's ended game goes on.
    PlayOn,
    /// Every peer saves the game as this frame leaves it, by this name.
    Save(String),
    /// The host's: from the step after this frame's, orders are sealed
    /// this many frames ahead. Changes no game state.
    Delay(u32),
    /// The seat's player surrenders: its game ends lost.
    Surrender,
}

/// A seat's items for one frame, its digest of the game after the step
/// it was sealed at, and a deep digest folded since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub items: Vec<Item>,
    /// The step `digest` is of: the one the bundle was sealed after.
    pub step: u32,
    /// `Game::digest`.
    pub digest: u64,
    /// `Game::deep_digest` of an earlier frame, every `DEEP_EVERY`
    /// frames: frame, digest. Folded on a worker, so it comes later.
    pub deep: Option<(u32, u64)>,
}

/// Where a game packet's sender stands in a resync after a desync
/// (`node`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Resync {
    #[default]
    None,
    /// The host's, to a guest not yet of its epoch: the desync's frame,
    /// the file every peer goes on from, by hash and length, and the
    /// seat that rejoins with it, if that is why.
    Offer {
        frame: u32,
        file: String,
        size: u32,
        /// The lockstep delay in force at the file's step.
        delay: u32,
        back: Option<u32>,
    },
    /// A guest's: the host's file, from this chunk on.
    Want { file: String, from: u32 },
    /// The host's: the game stops, and why.
    Stop(String),
}

/// One game packet: the sender's state, and what the receiver lacks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Game {
    /// The sender's seat.
    pub from: u32,
    /// The resyncs the sender has begun (the host) or gone on from (a
    /// guest): streams of another epoch are not read.
    pub epoch: u32,
    /// Seats paused, a bit each: a guest sends its own, the host all.
    pub paused: u8,
    /// The sender's speed: the host's paces everyone.
    pub speed: f32,
    /// Seats dropped, the last frame of theirs played, and how they left.
    pub dropped: Vec<(u32, u32, Leave)>,
    /// The host's: seats silent and waited on, and ms until each is
    /// dropped.
    pub waiting: Vec<(u32, u32)>,
    /// The host's: each guest's round trip to it, ms (`Node::pings`).
    pub pings: Vec<(u32, u32)>,
    /// What the sender has of each seat, by seat.
    pub acks: Vec<Ack>,
    /// Bundles: seat, frame, bundle.
    pub bundles: Vec<(u32, u32, Bundle)>,
    /// Chat lines: seat, number from 1, line.
    pub chat: Vec<(u32, u32, String)>,
    pub resync: Resync,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Packet {
    Hello(Hello),
    Lobby(Room),
    Welcome(Welcome),
    Refused(String),
    Game(Game),
    /// The sender's seat leaves the game.
    Bye(u32),
    Chunk(super::transfer::Chunk),
    /// The host plays on: a hello from a guest back in the lobby
    /// answered.
    Wait,
    /// The host is back in its lobby, the guests asked there.
    Regroup,
    /// A host's lobby, announced.
    Open(Open),
    Ping(u32),
    Pong(u32),
}

/// A host's lobby as LAN discovery shows it (`discover`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    /// The lobby's, picked as it opens: one heard on two networks is
    /// one game.
    pub id: u64,
    /// The port its lobby listens on.
    pub port: u16,
    /// The session's name.
    pub title: String,
    /// The host's nick.
    pub nick: String,
    /// Seats taken, the host's too, of `players`.
    pub seated: u32,
    pub players: u32,
    /// The setup's map options, the rest the defaults.
    pub setup: Setup,
    /// The name of the save the host loads.
    pub save: Option<String>,
    pub build: Build,
    /// Whether it seats guests of another commit.
    pub builds: super::Builds,
}

/// Bytes being written.
#[derive(Default)]
struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    /// A seat or count that fits a byte: seats are at most four.
    fn small(&mut self, v: impl TryInto<u8>) {
        self.u8(v.try_into().unwrap_or(u8::MAX));
    }

    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    /// A string, cut to what a `u16` length counts.
    fn str(&mut self, s: &str) {
        let bytes = &s.as_bytes()[..s.len().min(usize::from(u16::MAX))];

        self.u16(u16::try_from(bytes.len()).unwrap_or(u16::MAX));
        self.0.extend_from_slice(bytes);
    }

    /// A count of what follows, as a `u16`.
    fn count(&mut self, n: usize) {
        self.u16(u16::try_from(n).unwrap_or(u16::MAX));
    }

    /// Bytes, a `u32` length first.
    fn bytes(&mut self, bytes: &[u8]) {
        self.u32(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
        self.0.extend_from_slice(bytes);
    }

    /// A setup, as RON. One that will not write (a path not UTF-8) goes
    /// empty, and will not read.
    fn setup(&mut self, setup: &Setup) {
        self.str(&ron::to_string(setup).unwrap_or_default());
    }

    fn strings(&mut self, strings: &[String]) {
        self.count(strings.len());

        for s in strings {
            self.str(s);
        }
    }

    /// A string or none, a tag first.
    fn maybe(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
            None => self.u8(0),
        }
    }

    fn build(&mut self, b: &Build) {
        self.str(&b.version);
        self.u8(match b.profile {
            Profile::Release => 0,
            Profile::Debug => 1,
        });
        self.str(&b.commit);
        self.u64(b.data);
    }

    fn holding(&mut self, holding: &Holding) {
        match holding {
            Holding::None => self.u8(HOLD_NONE),
            Holding::Have(game) => {
                self.u8(HOLD_HAVE);
                self.str(game);
            }
            Holding::Want { game, from } => {
                self.u8(HOLD_WANT);
                self.str(game);
                self.u32(*from);
            }
        }
    }

    fn offer(&mut self, offer: Option<&Offer>) {
        match offer {
            Some(o) => {
                self.u8(1);
                self.str(&o.name);
                self.str(&o.game);
                self.str(&o.file);
                self.u32(o.size);
                self.strings(&o.seats);
                self.count(o.sits.len());

                for sit in &o.sits {
                    self.small(*sit);
                }
            }
            None => self.u8(0),
        }
    }
}

/// Bytes being read.
struct In<'a>(&'a [u8]);

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.0.len() >= n {
            let (head, rest) = self.0.split_at(n);

            self.0 = rest;
            Ok(head)
        } else {
            Err("short packet".to_owned())
        }
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().map_err(|_| "u16")?,
        ))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| "u32")?,
        ))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| "u64")?,
        ))
    }

    fn f32(&mut self) -> Result<f32, String> {
        self.u32().map(f32::from_bits)
    }

    fn str(&mut self) -> Result<String, String> {
        let n = usize::from(self.u16()?);

        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }

    fn setup(&mut self) -> Result<Setup, String> {
        ron::from_str(&self.str()?).map_err(|e| format!("setup: {e}"))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let n = self.u32()? as usize;

        Ok(self.take(n)?.to_vec())
    }

    fn build(&mut self) -> Result<Build, String> {
        Ok(Build {
            version: self.str()?,
            profile: match self.u8()? {
                0 => Profile::Release,
                _ => Profile::Debug,
            },
            commit: self.str()?,
            data: self.u64()?,
        })
    }

    fn offer(&mut self) -> Result<Option<Offer>, String> {
        match self.u8()? {
            0 => Ok(None),
            _ => Ok(Some(Offer {
                name: self.str()?,
                game: self.str()?,
                file: self.str()?,
                size: self.u32()?,
                seats: self.list(In::str)?,
                sits: self.list(|at| at.u8().map(u32::from))?,
            })),
        }
    }

    /// A guest's hash of a mod: its GUID, and the hash if held.
    fn held(&mut self) -> Result<(Guid, Option<String>), String> {
        let text = self.str()?;
        let guid = Guid::parse(&text).ok_or_else(|| format!("mod {text}: no GUID"))?;
        let hash = match self.u8()? {
            0 => None,
            _ => Some(self.str()?),
        };

        Ok((guid, hash))
    }

    /// A seat's match of a mod.
    fn matched(&mut self) -> Result<ModMatch, String> {
        Ok(match self.u8()? {
            MATCH_SAME => ModMatch::Same,
            MATCH_MISSING => ModMatch::Missing,
            MATCH_DIFFERS => ModMatch::Differs,
            _ => ModMatch::Unknown,
        })
    }

    /// `n` of what `one` reads, `n` read first.
    fn list<T>(
        &mut self,
        mut one: impl FnMut(&mut Self) -> Result<T, String>,
    ) -> Result<Vec<T>, String> {
        let n = self.u16()?;

        (0..n).map(|_| one(self)).collect()
    }
}

impl Packet {
    #[allow(clippy::too_many_lines, reason = "one arm per packet kind")]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Out::default();

        out.0.extend_from_slice(&MAGIC);
        out.u8(VERSION);

        match self {
            Packet::Hello(h) => {
                out.u8(HELLO);
                out.build(&h.build);
                out.str(&h.nick);

                match h.ready {
                    Ready::No => out.u8(NOT_READY),
                    Ready::For(revision) => {
                        out.u8(READY_FOR);
                        out.u32(revision);
                    }
                    Ready::Always => out.u8(READY_ALWAYS),
                }

                out.u32(h.heard);
                out.u32(h.said.first);
                out.count(h.said.lines.len());

                for line in &h.said.lines {
                    out.str(line);
                }

                out.holding(&h.holding);
                out.count(h.mods.len());

                for (guid, hash) in &h.mods {
                    out.str(&guid.to_string());
                    out.maybe(hash.as_deref());
                }
            }
            Packet::Lobby(r) => {
                out.u8(LOBBY);
                out.small(r.seat);
                out.small(r.players);
                out.u32(r.delay);
                out.str(&r.title);
                out.u32(r.revision);
                out.setup(&r.setup);
                out.strings(&r.mods);

                out.count(r.seats.len());

                for seat in &r.seats {
                    out.str(&seat.nick);
                    out.str(&seat.build);
                    out.u8(u8::from(seat.ready));
                    out.count(seat.mods.len());

                    for m in &seat.mods {
                        out.u8(match m {
                            ModMatch::Unknown => MATCH_UNKNOWN,
                            ModMatch::Same => MATCH_SAME,
                            ModMatch::Missing => MATCH_MISSING,
                            ModMatch::Differs => MATCH_DIFFERS,
                        });
                    }
                }

                out.u32(r.taken);
                out.u32(r.chat.first);
                out.count(r.chat.lines.len());

                for talk in &r.chat.lines {
                    match talk {
                        Talk::Said(who, line) => {
                            out.u8(SAID);
                            out.str(who);
                            out.str(line);
                        }
                        Talk::Changed => out.u8(CHANGED),
                    }
                }

                out.offer(r.save.as_ref());
            }
            Packet::Welcome(w) => {
                out.u8(WELCOME);
                out.small(w.seat);
                out.small(w.players);
                out.u32(w.delay);
                out.setup(&w.setup);

                out.strings(&w.nicks);
                out.small(w.host);
                out.maybe(w.save.as_deref());
            }
            Packet::Refused(why) => {
                out.u8(REFUSED);
                out.str(why);
            }
            Packet::Game(g) => {
                out.u8(GAME);
                game(&mut out, g);
            }
            Packet::Bye(seat) => {
                out.u8(BYE);
                out.small(*seat);
            }
            Packet::Chunk(c) => {
                out.u8(CHUNK);
                out.str(&c.file);
                out.u32(c.at);
                out.bytes(&c.bytes);
            }
            Packet::Wait => out.u8(WAIT),
            Packet::Regroup => out.u8(REGROUP),
            Packet::Open(o) => {
                out.u8(OPEN);
                open(&mut out, o);
            }
            Packet::Ping(id) => {
                out.u8(PING);
                out.u32(*id);
            }
            Packet::Pong(id) => {
                out.u8(PONG);
                out.u32(*id);
            }
        }

        out.0
    }

    /// # Errors
    ///
    /// Bytes no peer of this version sent.
    pub fn decode(bytes: &[u8]) -> Result<Packet, String> {
        let mut at = In(bytes);

        if at.take(MAGIC.len())? != MAGIC {
            Err("not a openregent packet".to_owned())?;
        }

        let version = at.u8()?;

        if version != VERSION {
            Err(format!("protocol version {version}, not {VERSION}"))?;
        }

        match at.u8()? {
            HELLO => Ok(Packet::Hello(Hello {
                build: at.build()?,
                nick: at.str()?,
                ready: match at.u8()? {
                    NOT_READY => Ready::No,
                    READY_FOR => Ready::For(at.u32()?),
                    _ => Ready::Always,
                },
                heard: at.u32()?,
                said: Lines {
                    first: at.u32()?,
                    lines: at.list(In::str)?,
                },
                holding: match at.u8()? {
                    HOLD_HAVE => Holding::Have(at.str()?),
                    HOLD_WANT => Holding::Want {
                        game: at.str()?,
                        from: at.u32()?,
                    },
                    _ => Holding::None,
                },
                mods: at.list(In::held)?,
            })),
            LOBBY => Ok(Packet::Lobby(Room {
                seat: at.u8()?.into(),
                players: at.u8()?.into(),
                delay: at.u32()?,
                title: at.str()?,
                revision: at.u32()?,
                setup: at.setup()?,
                mods: at.list(In::str)?,
                seats: at.list(|at| {
                    Ok(Seat {
                        nick: at.str()?,
                        build: at.str()?,
                        ready: at.u8()? != 0,
                        mods: at.list(In::matched)?,
                    })
                })?,
                taken: at.u32()?,
                chat: Lines {
                    first: at.u32()?,
                    lines: at.list(|at| match at.u8()? {
                        SAID => Ok(Talk::Said(at.str()?, at.str()?)),
                        CHANGED => Ok(Talk::Changed),
                        tag => Err(format!("chat tag {tag}")),
                    })?,
                },
                save: at.offer()?,
            })),
            WELCOME => Ok(Packet::Welcome(Welcome {
                seat: at.u8()?.into(),
                players: at.u8()?.into(),
                delay: at.u32()?,
                setup: at.setup()?,
                nicks: at.list(In::str)?,
                host: at.u8()?.into(),
                save: match at.u8()? {
                    0 => None,
                    _ => Some(at.str()?),
                },
            })),
            REFUSED => Ok(Packet::Refused(at.str()?)),
            GAME => game_of(&mut at).map(Packet::Game),
            BYE => Ok(Packet::Bye(at.u8()?.into())),
            CHUNK => Ok(Packet::Chunk(super::transfer::Chunk {
                file: at.str()?,
                at: at.u32()?,
                bytes: at.bytes()?,
            })),
            WAIT => Ok(Packet::Wait),
            REGROUP => Ok(Packet::Regroup),
            OPEN => open_of(&mut at).map(Packet::Open),
            PING => Ok(Packet::Ping(at.u32()?)),
            PONG => Ok(Packet::Pong(at.u32()?)),
            kind => Err(format!("packet kind {kind}")),
        }
    }
}

fn open(out: &mut Out, o: &Open) {
    out.u64(o.id);
    out.u16(o.port);
    out.str(&o.title);
    out.str(&o.nick);
    out.small(o.seated);
    out.small(o.players);
    out.setup(&o.setup);

    out.maybe(o.save.as_deref());
    out.build(&o.build);
    out.u8(match o.builds {
        super::Builds::Same => BUILDS_SAME,
        super::Builds::Any => BUILDS_ANY,
    });
}

fn open_of(at: &mut In<'_>) -> Result<Open, String> {
    Ok(Open {
        id: at.u64()?,
        port: at.u16()?,
        title: at.str()?,
        nick: at.str()?,
        seated: at.u8()?.into(),
        players: at.u8()?.into(),
        setup: at.setup()?,
        save: match at.u8()? {
            0 => None,
            _ => Some(at.str()?),
        },
        build: at.build()?,
        builds: match at.u8()? {
            BUILDS_SAME => super::Builds::Same,
            _ => super::Builds::Any,
        },
    })
}

fn game(out: &mut Out, g: &Game) {
    out.small(g.from);
    out.u32(g.epoch);
    out.u8(g.paused);
    out.f32(g.speed);
    out.count(g.dropped.len());

    for (seat, after, how) in &g.dropped {
        out.small(*seat);
        out.u32(*after);
        out.u8(match how {
            Leave::Quit => QUIT,
            Leave::Silent => SILENT,
        });
    }

    out.count(g.waiting.len());

    for (seat, left) in &g.waiting {
        out.small(*seat);
        out.u32(*left);
    }

    out.count(g.pings.len());

    for (seat, ms) in &g.pings {
        out.small(*seat);
        out.u32(*ms);
    }

    out.count(g.acks.len());

    for ack in &g.acks {
        out.u32(ack.frames);
        out.u32(ack.chat);
    }

    out.count(g.bundles.len());

    for (seat, frame, bundle) in &g.bundles {
        out.small(*seat);
        out.u32(*frame);
        out.u32(bundle.step);
        out.u64(bundle.digest);

        match bundle.deep {
            Some((frame, deep)) => {
                out.u8(1);
                out.u32(frame);
                out.u64(deep);
            }
            None => out.u8(0),
        }

        out.count(bundle.items.len());

        for item in &bundle.items {
            match item {
                Item::Order(order) => {
                    out.u8(ORDER);
                    out.str(&order.to_string());
                }
                Item::PlayOn => out.u8(PLAY_ON),
                Item::Save(name) => {
                    out.u8(SAVE);
                    out.str(name);
                }
                Item::Delay(frames) => {
                    out.u8(DELAY);
                    out.u32(*frames);
                }
                Item::Surrender => out.u8(SURRENDER),
            }
        }
    }

    out.count(g.chat.len());

    for (seat, seq, line) in &g.chat {
        out.small(*seat);
        out.u32(*seq);
        out.str(line);
    }

    match &g.resync {
        Resync::None => out.u8(SYNC_NONE),
        Resync::Offer {
            frame,
            file,
            size,
            delay,
            back,
        } => {
            out.u8(SYNC_OFFER);
            out.u32(*frame);
            out.str(file);
            out.u32(*size);
            out.u32(*delay);

            match back {
                Some(seat) => {
                    out.u8(1);
                    out.small(*seat);
                }
                None => out.u8(0),
            }
        }
        Resync::Want { file, from } => {
            out.u8(SYNC_WANT);
            out.str(file);
            out.u32(*from);
        }
        Resync::Stop(why) => {
            out.u8(SYNC_STOP);
            out.str(why);
        }
    }
}

fn game_of(at: &mut In<'_>) -> Result<Game, String> {
    Ok(Game {
        from: at.u8()?.into(),
        epoch: at.u32()?,
        paused: at.u8()?,
        speed: at.f32()?,
        dropped: at.list(|at| {
            let seat = at.u8()?.into();
            let after = at.u32()?;
            let how = match at.u8()? {
                QUIT => Leave::Quit,
                SILENT => Leave::Silent,
                tag => Err(format!("leave tag {tag}"))?,
            };

            Ok((seat, after, how))
        })?,
        waiting: at.list(|at| Ok((at.u8()?.into(), at.u32()?)))?,
        pings: at.list(|at| Ok((at.u8()?.into(), at.u32()?)))?,
        acks: at.list(|at| {
            Ok(Ack {
                frames: at.u32()?,
                chat: at.u32()?,
            })
        })?,
        bundles: at.list(|at| {
            let seat = at.u8()?.into();
            let frame = at.u32()?;
            let step = at.u32()?;
            let digest = at.u64()?;
            let deep = match at.u8()? {
                0 => None,
                _ => Some((at.u32()?, at.u64()?)),
            };
            let items = at.list(|at| match at.u8()? {
                ORDER => Ok(Item::Order(at.str()?.parse()?)),
                PLAY_ON => Ok(Item::PlayOn),
                SAVE => Ok(Item::Save(at.str()?)),
                DELAY => Ok(Item::Delay(at.u32()?)),
                SURRENDER => Ok(Item::Surrender),
                tag => Err(format!("item tag {tag}")),
            })?;

            Ok((
                seat,
                frame,
                Bundle {
                    items,
                    step,
                    digest,
                    deep,
                },
            ))
        })?,
        chat: at.list(|at| Ok((at.u8()?.into(), at.u32()?, at.str()?)))?,
        resync: match at.u8()? {
            SYNC_NONE => Resync::None,
            SYNC_OFFER => Resync::Offer {
                frame: at.u32()?,
                file: at.str()?,
                size: at.u32()?,
                delay: at.u32()?,
                back: match at.u8()? {
                    0 => None,
                    _ => Some(at.u8()?.into()),
                },
            },
            SYNC_WANT => Resync::Want {
                file: at.str()?,
                from: at.u32()?,
            },
            SYNC_STOP => Resync::Stop(at.str()?),
            tag => Err(format!("resync tag {tag}"))?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::sim::game::Input;

    const MOD: &str = "{17BF53DE-722F-4683-8E63-6B93D1606DFF}";

    #[test]
    #[allow(clippy::too_many_lines, reason = "every kind of packet")]
    fn every_packet_reads_back_as_written() {
        let order: Order = "p1 dismiss 7".parse().expect("order");
        let packets = [
            Packet::Hello(Hello {
                build: Build {
                    version: "0.1.0".to_owned(),
                    profile: Profile::Debug,
                    commit: "c0ffee".to_owned(),
                    data: u64::MAX,
                },
                nick: "Ælfred".to_owned(),
                ready: Ready::For(3),
                heard: 4,
                said: Lines {
                    first: 2,
                    lines: vec!["hi".to_owned()],
                },
                holding: Holding::Want {
                    game: "0123".to_owned(),
                    from: 7,
                },
                mods: vec![
                    (Guid::parse(MOD).expect("a GUID"), Some("89ab".to_owned())),
                    (Guid::parse(MOD).expect("a GUID"), None),
                ],
            }),
            Packet::Lobby(Room {
                seat: 1,
                players: 2,
                delay: 4,
                title: "foobar".to_owned(),
                revision: 3,
                setup: Setup {
                    gold: 30_000,
                    ..Setup::default()
                },
                mods: vec!["New Game+".to_owned()],
                seats: vec![Seat {
                    nick: "Ælfred".to_owned(),
                    build: "v0.1.0 c0ffee".to_owned(),
                    ready: true,
                    mods: vec![
                        ModMatch::Unknown,
                        ModMatch::Same,
                        ModMatch::Missing,
                        ModMatch::Differs,
                    ],
                }],
                taken: 1,
                chat: Lines {
                    first: 0,
                    lines: vec![
                        Talk::Said("Ælfred".to_owned(), "hello".to_owned()),
                        Talk::Changed,
                    ],
                },
                save: Some(Offer {
                    name: "Day 3".to_owned(),
                    game: "0123".to_owned(),
                    file: "4567".to_owned(),
                    size: 9_000,
                    seats: vec!["Ælfred".to_owned(), "p1".to_owned()],
                    sits: vec![1, 0],
                }),
            }),
            Packet::Welcome(Welcome {
                seat: 1,
                players: 2,
                delay: 3,
                setup: Setup {
                    seed: 7,
                    ..Setup::default()
                },
                nicks: vec!["Ælfred".to_owned(), String::new()],
                host: 1,
                save: Some("0123".to_owned()),
            }),
            Packet::Chunk(super::super::transfer::Chunk {
                file: "4567".to_owned(),
                at: 2,
                bytes: vec![0, 1, 255],
            }),
            Packet::Refused("full".to_owned()),
            Packet::Game(Game {
                from: 1,
                epoch: 2,
                paused: 0b10,
                speed: 2.0,
                dropped: vec![(2, 400, Leave::Silent), (3, 410, Leave::Quit)],
                waiting: vec![(1, 57_000)],
                pings: vec![(1, 34), (2, 1_024)],
                acks: vec![Ack { frames: 9, chat: 1 }, Ack::default()],
                bundles: vec![(
                    1,
                    12,
                    Bundle {
                        items: vec![
                            Item::Order(order),
                            Item::PlayOn,
                            Item::Save("Day 3".to_owned()),
                            Item::Delay(5),
                            Item::Surrender,
                        ],
                        step: 9,
                        digest: 0xdead_beef,
                        deep: Some((7, 5)),
                    },
                )],
                chat: vec![(1, 1, "hello".to_owned())],
                resync: Resync::Want {
                    file: "89ab".to_owned(),
                    from: 3,
                },
            }),
            Packet::Bye(2),
            Packet::Wait,
            Packet::Regroup,
            Packet::Ping(42),
            Packet::Pong(42),
            Packet::Open(Open {
                id: u64::MAX,
                port: 47_101,
                title: "Ann's game".to_owned(),
                nick: "Ann".to_owned(),
                seated: 1,
                players: 3,
                setup: Setup {
                    land: Some(regent_core::data::fourcc::FourCc::new(b"East")),
                    ..Setup::default()
                },
                save: Some("saved".to_owned()),
                build: Build {
                    version: "0.1.0".to_owned(),
                    profile: Profile::Release,
                    commit: "c0ffee-dirty".to_owned(),
                    data: 9,
                },
                builds: super::super::Builds::Any,
            }),
        ];

        for packet in packets {
            assert_eq!(Packet::decode(&packet.encode()), Ok(packet));
        }

        assert_eq!(order.input, Input::Dismiss(7));
    }

    /// Each resync word a game packet carries.
    #[test]
    fn a_resync_word_reads_back_as_written() {
        let words = [
            Resync::None,
            Resync::Offer {
                frame: 400,
                file: "89ab".to_owned(),
                size: 9_000,
                delay: 4,
                back: None,
            },
            Resync::Offer {
                frame: 400,
                file: "89ab".to_owned(),
                size: 9_000,
                delay: 4,
                back: Some(2),
            },
            Resync::Stop("desync again".to_owned()),
        ];

        for resync in words {
            let packet = Packet::Game(Game {
                epoch: 1,
                resync,
                ..Game::default()
            });

            assert_eq!(Packet::decode(&packet.encode()), Ok(packet));
        }
    }

    #[test]
    fn a_build_shows_its_version_and_short_commit() {
        let build = |commit: &str| Build {
            version: "0.1.0".to_owned(),
            profile: Profile::Release,
            commit: commit.to_owned(),
            data: 0,
        };

        assert_eq!(build("1a2b3c4d5e6f").shown(), "v0.1.0 1a2b3c4");
        assert_eq!(build("1a2b3c4d5e6f-dirty").shown(), "v0.1.0 1a2b3c4-dirty");
        assert_eq!(build("unknown").shown(), "v0.1.0 unknown");
    }

    #[test]
    fn a_foreign_or_short_packet_is_refused() {
        let mut other = Packet::Refused("x".to_owned()).encode();

        other[4] = VERSION + 1;

        assert!(Packet::decode(b"hello").is_err());
        assert!(Packet::decode(&other).is_err_and(|e| e.contains("version")));
        assert!(Packet::decode(&Packet::Refused("x".to_owned()).encode()[..7]).is_err());
    }
}
