//! The lockstep protocol, free of sockets and games: what a peer holds
//! of every seat, what it sends whom, and when a frame may run.
//!
//! ```text
//!  guest 1 ──┐                 a star: guests talk to the host alone,
//!  guest 2 ──┼── host (seat 0)  which relays each seat's bundles and
//!  guest 3 ──┘                 chat to the others
//!
//!  each packet: my acks (per seat, the last frame and chat line held
//!  without a gap), and every bundle and line the receiver has not
//!  acked, so a lost packet's are in the next
//! ```
//!
//! A frame `t` runs once every seat's bundle for it is in, or the seat
//! was dropped before it. Bundles for frames below the delay are empty
//! on every peer: no order can name them.
//!
//! A desync holds every peer while the host sends its game over
//! (`transfer`); each goes on from it in a new epoch, its streams begun
//! again at the host's step as at the start. A packet of another epoch
//! is heard, not read.
//!
//! ```text
//!  host                                  guest
//!  desync found: epoch + 1, holds ──>    a packet of a later epoch: holds
//!  its game as a file: Offer ──────>     Want(file, from), again once
//!                                        the window is in or after a pause
//!  Chunk × WINDOW from `from` ──────>    ... whole: the game goes on from
//!  every guest of its epoch: on    <──   it, of the host's epoch
//! ```
//!
//! A remote silent [`WAIT_AFTER`] is waited on: the game holds, as its
//! bundles stop, and the host may drop it ([`Node::toss`]); silent
//! [`DROP_AFTER`], it is dropped. One that comes back before resumes.
//! A dropped guest may ask its seat back from a new address, or its old
//! one dropped silent: the host welcomes it again and resyncs every
//! peer, the seat back in play.
//!
//! ```text
//!  host                                  guest, dropped
//!                                  <──   Hello (nick of a dropped seat)
//!  Welcome (its seat), epoch + 1 ──>     makes its game, then as a resync
//!  Offer (back: seat) ──────────────>    Want ... whole: back in play
//! ```

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bevy::log::{error, info, warn};

use super::delay::{Delay, Steer};
use super::lobby::Probe;
use super::transfer::{self, Incoming, Outgoing};
use super::wire::{Ack, Bundle, Game, Item, Leave, Packet, Resync, Welcome};
use super::{CHAT_MOST, LEAST_DELAY, MOST_DELAY, Notice};

/// A packet's bytes at most, about: under any LAN's datagram.
const BUDGET: usize = 1_200;

/// An order's bytes, about: its text and its length.
const ITEM_BYTES: usize = 48;

/// An item's bytes, about: an order's [`ITEM_BYTES`], a save's name and
/// its tag and length.
const fn item_bytes(item: &Item) -> usize {
    match item {
        Item::Order(_) => ITEM_BYTES,
        Item::PlayOn | Item::Surrender => 1,
        Item::Save(name) => 3 + name.len(),
        Item::Delay(_) => 5,
    }
}

/// A bundle's bytes without items, at most.
const BUNDLE_BYTES: usize = 32;

/// Chat lines a packet carries at most: a remote's line numbered further
/// past those held is not held.
#[allow(clippy::cast_possible_truncation, reason = "a few dozen")]
const LINES_AHEAD: u32 = (BUDGET / BUNDLE_BYTES) as u32;

/// Packets to a peer at least this often, new data or not: the
/// heartbeat, and the resend of what was lost.
const SEND_EVERY: Duration = Duration::from_millis(25);

/// The host's welcome again this often, until the guest's first game
/// packet.
const WELCOME_EVERY: Duration = Duration::from_millis(250);

/// A guest's ask for the host's file again this often, where a chunk
/// of the window asked was lost.
const ASK_AGAIN: Duration = Duration::from_millis(250);

/// A peer silent this long is waited on: the game holds for it, and
/// the host may drop it, as HD's "Drop Player?" asks.
/// **Inferred**: a bound picked; HD's own
/// not found.
pub(super) const WAIT_AFTER: Duration = Duration::from_secs(2);

/// HD's `TossPlayerAfter`, ms: its file value
/// (`data::settings::cases`).
pub(super) const TOSS_PLAYER_AFTER: u64 = 60_000;

/// A peer heard from silent this long is dropped: HD compares each
/// player's silence with `TossPlayerAfter`.
/// **Inferred**: past it, HD drops the seat.
pub(super) const DROP_AFTER: Duration = Duration::from_millis(TOSS_PLAYER_AFTER);

/// A peer not yet heard from in the game, making its world, is given
/// this long.
const FIRST_GRACE: Duration = Duration::from_secs(60);

/// Copies of a goodbye sent: one may be lost. Lost all, the silence
/// drops the seat.
const BYE_COPIES: usize = 3;

/// Frames past the last held a bundle may name: further is no peer's.
const WINDOW: u32 = 1_000;

/// Frames of digests kept to compare.
pub(super) const KEPT_DIGESTS: u32 = 2_000;

/// Which end of the star this peer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

/// A peer this one talks to: the host's guests, or a guest's host.
struct Remote {
    addr: SocketAddr,
    seat: u32,
    /// What it holds of each seat, by its latest acks.
    acked: Vec<Ack>,
    /// When it was last heard in the game; `None` before its first.
    heard: Option<Instant>,
    /// When this peer began listening for it.
    since: Instant,
    sent: Option<Instant>,
    /// The host's welcome, sent until it is heard in the game.
    welcome: Option<(Vec<u8>, Option<Instant>)>,
    /// The host's welcome as first sent: sent again to a guest that
    /// rejoins.
    greeting: Option<Vec<u8>>,
    live: bool,
    /// Its epoch, as last heard.
    epoch: u32,
    /// Gone from the game and asking from the lobby: the host's lobby
    /// seats it again ([`Node::regroup`]).
    waiting: bool,
    /// Its round trip, pinged once a second.
    probe: Probe,
}

/// How long a remote may be silent: in play, or while a resync holds
/// the game, as a file is made, sent and loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Patience {
    Play,
    Resync,
}

impl Remote {
    /// A remote at `addr` in seat `seat` of `seats`, not yet heard at
    /// `now`, its streams held to the frames below `delay`; sent
    /// `welcome` until heard.
    fn new(
        (addr, seat): (SocketAddr, u32),
        (seats, delay): (usize, u32),
        welcome: Option<Vec<u8>>,
        now: Instant,
    ) -> Remote {
        let start = Ack {
            frames: delay - 1,
            chat: 0,
        };

        Remote {
            addr,
            seat,
            acked: vec![start; seats],
            heard: None,
            since: now,
            sent: None,
            welcome: welcome.clone().map(|w| (w, None)),
            greeting: welcome,
            live: true,
            epoch: 0,
            waiting: false,
            probe: Probe::default(),
        }
    }

    fn silent(&self, now: Instant, patience: Patience) -> bool {
        let most = match patience {
            Patience::Play => DROP_AFTER,
            Patience::Resync => FIRST_GRACE,
        };

        match self.heard {
            Some(heard) => now.duration_since(heard) > most,
            None => now.duration_since(self.since) > FIRST_GRACE,
        }
    }
}

/// Where this peer stands in a resync.
enum Sync {
    /// In play.
    Idle,
    /// The host: a desync found, its file not yet made.
    Holding,
    /// The host: its file, offered to each guest not yet of its epoch;
    /// the desync's frame; the seat it brings back, if a rejoin's.
    Sending {
        frame: u32,
        file: Outgoing,
        back: Option<u32>,
    },
    /// A guest: the host's file of `epoch` coming, the seat it brings
    /// back, and the chunk last asked from, when.
    Taking {
        epoch: u32,
        file: Incoming,
        back: Option<u32>,
        delay: u32,
        asked: Option<(u32, Instant)>,
    },
    /// A guest: the file whole, until the game takes it, then until it
    /// goes on from it.
    Came {
        epoch: u32,
        bytes: Option<Vec<u8>>,
        back: Option<u32>,
        delay: u32,
    },
}

/// Two peers' digests of one frame that differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Desync {
    pub frame: u32,
    pub seat: u32,
    pub mine: u64,
    pub theirs: u64,
    pub kind: Depth,
}

/// Which digest parted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Depth {
    Quick,
    Deep,
}

impl Depth {
    const ALL: [Depth; 2] = [Depth::Quick, Depth::Deep];
}

/// A digest's kind and frame.
type Of = (Depth, u32);

/// One peer's lockstep state.
pub struct Node {
    role: Role,
    seat: u32,
    players: u32,
    /// The frames ahead this peer seals: the same on every peer at every
    /// step ([`Node::change`]).
    delay: u32,
    /// The host: a delay sealed in a bundle and not yet taken.
    pending: Option<u32>,
    /// The host's: when the delay is changed in play.
    steer: Steer,
    /// Runs of frames no seat sealed, a raise's, by first frame to last.
    skips: BTreeMap<u32, u32>,
    /// Seals refused for being out of order: none in a sound game.
    misseals: u32,
    remotes: Vec<Remote>,
    /// Each seat's bundles held, by frame.
    bundles: Vec<BTreeMap<u32, Bundle>>,
    /// Each seat's chat lines held, by number.
    chat: Vec<BTreeMap<u32, String>>,
    /// What this peer holds of each seat, without a gap.
    have: Vec<Ack>,
    /// Chat lines handed on, a seat.
    shown: Vec<u32>,
    /// Each dropped seat's last frame played, and how it left.
    dropped: Vec<Option<(u32, Leave)>>,
    /// Seats gone not yet taken by the interface: a guest that loses the
    /// host has the host's alone.
    left: Vec<(u32, Leave)>,
    /// Each seat paused, as last heard; this peer's own as set.
    paused: Vec<bool>,
    /// The host's speed: this peer's own on the host.
    speed: f32,
    /// New data since the last send.
    fresh: bool,
    /// This peer's digests sent, and the others' not yet compared.
    mine: BTreeMap<Of, u64>,
    theirs: BTreeMap<Of, Vec<(u32, u64)>>,
    /// This peer's deep digests folded, frame and digest, for the next
    /// bundles: one each.
    folded: std::collections::VecDeque<(u32, u64)>,
    /// The seats whose deep digest of a frame was compared, a bit each.
    compared: BTreeMap<u32, u32>,
    desync: Option<Desync>,
    /// Resyncs begun (the host) or gone on from (a guest).
    epoch: u32,
    sync: Sync,
    /// Why the game stops: the host's, sent; a guest's, heard. Taken
    /// once.
    stop: Option<String>,
    stop_taken: bool,
    /// What the chat says of a resync, not yet taken.
    notices: Vec<Notice>,
    /// A guest's host is back in its lobby: the game is left for it.
    regrouped: bool,
    /// The host: the seat rejoining, until the game goes on with it.
    back: Option<u32>,
    /// Seats back in play not yet taken by the interface.
    rejoined: Vec<u32>,
    /// A guest: the seats the host waits on, and when each is dropped,
    /// as last heard.
    reported: Vec<(u32, Instant)>,
    /// A guest: each other guest's round trip to the host, as last
    /// heard.
    relayed: Vec<(u32, Duration)>,
    /// The last time heard or sent at: a packet's waits count from it.
    now: Instant,
}

/// A seat waited on: its seat, silence, and the time left before it is
/// dropped.
pub type Waited = (u32, Duration, Duration);

/// Where a peer goes back to the lobby ([`Node::regroup`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Back {
    /// The host: its guests to seat again, by seat.
    Host(Vec<(SocketAddr, u32)>),
    /// A guest: its host.
    Guest(SocketAddr),
}

impl Node {
    /// A peer in seat `seat` of `players`, orders `delay` frames ahead,
    /// talking to `remotes`: address, seat and, on the host, the welcome
    /// each is sent until heard.
    pub fn new(
        (role, seat): (Role, u32),
        (players, delay): (u32, u32),
        remotes: Vec<(SocketAddr, u32, Option<Vec<u8>>)>,
        now: Instant,
    ) -> Node {
        let seats = players as usize;
        // The frames below the delay: empty everywhere.
        let start = Ack {
            frames: delay - 1,
            chat: 0,
        };

        Node {
            role,
            seat,
            players,
            delay,
            pending: None,
            steer: Steer::new(Delay::Fixed(delay), now),
            skips: BTreeMap::new(),
            misseals: 0,
            remotes: remotes
                .into_iter()
                .map(|(addr, seat, welcome)| {
                    Remote::new((addr, seat), (seats, delay), welcome, now)
                })
                .collect(),
            bundles: vec![BTreeMap::new(); seats],
            chat: vec![BTreeMap::new(); seats],
            have: vec![start; seats],
            shown: vec![0; seats],
            dropped: vec![None; seats],
            left: Vec::new(),
            paused: vec![false; seats],
            speed: 1.0,
            fresh: false,
            mine: BTreeMap::new(),
            theirs: BTreeMap::new(),
            folded: std::collections::VecDeque::new(),
            compared: BTreeMap::new(),
            desync: None,
            epoch: 0,
            sync: Sync::Idle,
            stop: None,
            stop_taken: false,
            notices: Vec::new(),
            regrouped: false,
            back: None,
            rejoined: Vec::new(),
            reported: Vec::new(),
            relayed: Vec::new(),
            now,
        }
    }

    /// The host's delay steered in play as `mode` says.
    #[must_use]
    pub const fn steered(mut self, mode: Delay) -> Node {
        self.steer = Steer::new(mode, self.now);
        self
    }

    pub const fn seat(&self) -> u32 {
        self.seat
    }

    pub const fn delay(&self) -> u32 {
        self.delay
    }

    /// The host's seat: this peer's on the host, its one remote's on a
    /// guest. Seat 0 in a new game, any in a loaded one.
    pub fn host(&self) -> u32 {
        match (self.role, self.remotes.first()) {
            (Role::Guest, Some(host)) => host.seat,
            _ => self.seat,
        }
    }

    pub const fn players(&self) -> u32 {
        self.players
    }

    /// Whether any seat was dropped: a test's.
    #[cfg(test)]
    pub fn dropped(&self) -> bool {
        self.dropped.iter().any(Option::is_some)
    }

    pub const fn desync(&self) -> Option<Desync> {
        self.desync
    }

    pub const fn epoch(&self) -> u32 {
        self.epoch
    }

    pub const fn role(&self) -> Role {
        self.role
    }

    /// Whether a resync holds this peer: a desync found, a later epoch
    /// heard and not yet gone on from, the host's file not yet made, or
    /// the game stopping.
    fn holding(&self) -> bool {
        self.desync.is_some()
            || self.stop.is_some()
            || matches!(
                self.sync,
                Sync::Holding | Sync::Taking { .. } | Sync::Came { .. }
            )
            || self.remotes.iter().any(|r| r.live && r.epoch > self.epoch)
    }

    /// How long `remote` may be silent: a resync's patience while one
    /// holds this peer or the remote is of another epoch.
    fn patience(&self, remote: &Remote) -> Patience {
        if self.holding() || remote.epoch != self.epoch {
            Patience::Resync
        } else {
            Patience::Play
        }
    }

    /// The host's desync: a new epoch, every peer held until its file
    /// is offered. The packets that say so, a few copies each: a guest
    /// that missed them would think the host silent while the file is
    /// made.
    pub fn hold(&mut self, now: Instant) -> Vec<(SocketAddr, Vec<u8>)> {
        self.epoch += 1;
        self.sync = Sync::Holding;
        self.fresh = true;

        self.packets(now)
            .into_iter()
            .flat_map(|sent| std::iter::repeat_n(sent, BYE_COPIES))
            .collect()
    }

    /// The host's file for its epoch, `bytes`, offered to every guest;
    /// `frame` the desync's.
    pub fn offer(&mut self, frame: u32, bytes: Vec<u8>) {
        self.sync = Sync::Sending {
            frame,
            file: Outgoing::new(bytes),
            back: self.back,
        };
        self.fresh = true;
    }

    /// Whether the game stops on every peer: the host's word, sent or
    /// heard.
    pub const fn stopping(&self) -> bool {
        self.stop.is_some()
    }

    /// The host gives up: the game stops on every peer, for `why`.
    pub fn give_up(&mut self, why: String) {
        self.stop = Some(why);
        self.stop_taken = true;
        self.fresh = true;
    }

    /// The game went on from the file of `epoch`, at step `step`: every
    /// stream begun again there, as at the start, its frames to the
    /// delay empty everywhere; chat goes on, but for the seat brought
    /// back. A seat dropped stays so, but for one the file brings back:
    /// that seat.
    pub fn resume(&mut self, step: u32, epoch: u32) -> Option<u32> {
        // The host's delay at the file's step, which every peer takes.
        if let Sync::Taking { delay, .. } | Sync::Came { delay, .. } = &self.sync {
            self.delay = *delay;
        }

        self.pending = None;
        self.skips.clear();

        let start = step + self.delay - 1;
        let back = match &self.sync {
            Sync::Taking { back, .. } | Sync::Came { back, .. } => *back,
            _ => self.back.take(),
        };

        // The seat rejoining plays again, from the step as every seat.
        if let Some(seat) = back.filter(|s| *s < self.players) {
            self.dropped[Self::index(seat)] = None;

            // Not paused until it says so: its pause from before it was
            // dropped would show till its first packet.
            if seat != self.seat {
                self.paused[Self::index(seat)] = false;
            }

            // The host says so once every guest has the file.
            if self.role == Role::Guest && seat != self.seat {
                self.rejoined.push(seat);
            }

            // Its chat begun again: the machine back numbers its lines
            // from 1.
            if seat != self.seat {
                let i = Self::index(seat);

                self.chat[i].clear();
                self.have[i].chat = 0;
                self.shown[i] = 0;

                for remote in &mut self.remotes {
                    remote.acked[i].chat = 0;
                }
            }
        }

        for s in self.seats() {
            let i = Self::index(s);

            self.bundles[i].clear();
            self.have[i].frames = start;
            self.dropped[i] = self.dropped[i].map(|(last, how)| (last.min(start), how));
        }

        for remote in &mut self.remotes {
            for ack in &mut remote.acked {
                ack.frames = start;
            }
        }

        self.mine.clear();
        self.theirs.clear();
        self.folded.clear();
        self.compared.clear();
        self.desync = None;
        self.epoch = epoch;
        self.fresh = true;

        if !matches!(self.sync, Sync::Sending { .. }) {
            self.sync = Sync::Idle;
        }

        info!("net: resynced at frame {step}, epoch {epoch}");
        back
    }

    /// A guest's file whole, once: its epoch and bytes.
    pub fn take_came(&mut self) -> Option<(u32, Vec<u8>)> {
        match &mut self.sync {
            Sync::Came { epoch, bytes, .. } => bytes.take().map(|b| (*epoch, b)),
            _ => None,
        }
    }

    /// Why the game stops, once: the host's word, or the host gone while
    /// a resync held this guest.
    pub fn take_stop(&mut self) -> Option<String> {
        let stranded =
            self.role == Role::Guest && self.holding() && self.remotes.iter().all(|r| !r.live);

        if stranded && self.stop.is_none() {
            self.stop = Some("the host left during a resync".to_owned());
        }

        let taken = std::mem::replace(&mut self.stop_taken, self.stop.is_some());

        self.stop.clone().filter(|_| !taken)
    }

    /// What the chat says of a resync since the last call.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    /// The seats back in play since the last call.
    pub fn take_rejoined(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.rejoined)
    }

    /// Whether a hello from `addr` may ask a dropped seat back: from no
    /// remote's address, or from one dropped by its silence or the host.
    /// One that said goodbye went back to its lobby: from its old address
    /// it waits there for the host's ([`Node::regroup`]).
    pub fn rejoins(&self, addr: SocketAddr) -> bool {
        self.remotes.iter().filter(|r| r.addr == addr).all(|r| {
            let dropped = self.dropped[Self::index(r.seat)];

            !r.live && dropped.is_some_and(|(_, how)| how == Leave::Silent)
        })
    }

    /// Whether seat `seat` was dropped.
    pub fn gone(&self, seat: u32) -> bool {
        seat < self.players && self.dropped[Self::index(seat)].is_some()
    }

    /// The host drops seat `seat`'s guest, silent or not, as a silence
    /// past [`DROP_AFTER`] would.
    pub fn toss(&mut self, seat: u32) {
        let at = self.remotes.iter().position(|r| r.live && r.seat == seat);

        if let (Role::Host, Some(at)) = (self.role, at) {
            warn!("net: player {seat} dropped by the host");
            self.lost(at, Leave::Silent);
        }
    }

    /// The host takes the guest at `from` back into dropped seat
    /// `seat`, if no resync runs: its welcome sent again, every peer
    /// held for a resync that brings the seat back. The packets that
    /// say so; `None`, not now.
    pub fn rejoin(
        &mut self,
        from: SocketAddr,
        seat: u32,
        now: Instant,
    ) -> Option<Vec<(SocketAddr, Vec<u8>)>> {
        let idle = self.role == Role::Host
            && matches!(self.sync, Sync::Idle)
            && self.desync.is_none()
            && self.stop.is_none()
            && self.back.is_none()
            && self.gone(seat);
        let at = self
            .remotes
            .iter()
            .position(|r| r.seat == seat && !r.live)
            .filter(|_| idle)?;
        // The welcome again names the delay in force, not the start's.
        let greeting =
            self.remotes[at]
                .greeting
                .as_ref()
                .map(|bytes| match Packet::decode(bytes) {
                    Ok(Packet::Welcome(w)) => Packet::Welcome(Welcome {
                        delay: self.delay,
                        ..w
                    })
                    .encode(),
                    _ => bytes.clone(),
                });

        info!("net: player {seat} rejoins from {from}");
        self.remotes[at] = Remote::new(
            (from, seat),
            (self.players as usize, self.delay),
            greeting,
            now,
        );
        self.back = Some(seat);

        Some(self.hold(now))
    }

    /// The seats waited on at `now`: a live remote silent past
    /// [`WAIT_AFTER`] in play, or not yet heard; on a guest whose host
    /// is heard, those the host waits on.
    pub fn waiting(&self, now: Instant) -> Vec<Waited> {
        let own: Vec<Waited> = self
            .remotes
            .iter()
            .filter(|r| r.live)
            .filter_map(|r| self.wait_of(r, now))
            .collect();

        if self.role == Role::Guest && own.is_empty() {
            self.reported
                .iter()
                .map(|(seat, until)| {
                    let left = until.saturating_duration_since(now);

                    (*seat, DROP_AFTER.saturating_sub(left), left)
                })
                .collect()
        } else {
            own
        }
    }

    /// `remote`'s wait at `now`: its silence and the time left, by the
    /// patience it has; none while heard, or during a resync.
    fn wait_of(&self, remote: &Remote, now: Instant) -> Option<Waited> {
        let (since, most) = match (remote.heard, self.patience(remote)) {
            (Some(heard), Patience::Play) => Some((heard, DROP_AFTER)),
            (Some(_), Patience::Resync) => None,
            (None, _) => Some((remote.since, FIRST_GRACE)),
        }?;
        let silent = now.saturating_duration_since(since);

        (silent > WAIT_AFTER).then(|| (remote.seat, silent, most.saturating_sub(silent)))
    }

    /// Whether a guest is being sent the host's file: a test's.
    #[cfg(test)]
    pub const fn taking(&self) -> bool {
        matches!(self.sync, Sync::Taking { .. })
    }

    /// Seats in play, in order.
    const fn seats(&self) -> std::ops::Range<u32> {
        0..self.players
    }

    const fn index(seat: u32) -> usize {
        seat as usize
    }

    /// This peer's local pause and, on the host, speed.
    pub fn set_local(&mut self, paused: bool, speed: f32) {
        self.paused[Self::index(self.seat)] = paused;

        if self.role == Role::Host {
            self.speed = speed;
        }
    }

    /// Whether any seat in play is paused.
    pub fn paused(&self) -> bool {
        self.seats().any(|s| self.holds(s))
    }

    /// The seats in play paused, in order: whom the pause line names.
    pub fn pausers(&self) -> Vec<u32> {
        self.seats().filter(|s| self.holds(*s)).collect()
    }

    /// Whether seat `s` is in play and paused.
    fn holds(&self, s: u32) -> bool {
        self.paused[Self::index(s)] && self.dropped[Self::index(s)].is_none()
    }

    /// The round trip from this peer to each other seat in play, at
    /// `now`, where known: a guest's to another guest goes by the host,
    /// so it is its own to the host and the host's to it.
    ///
    /// ```text
    ///  guest 1 ──20 ms── host ──30 ms── guest 2   guest 1 sees 50 ms
    /// ```
    pub fn pings(&self, now: Instant) -> Vec<(u32, Duration)> {
        let direct = self
            .remotes
            .iter()
            .filter(|r| r.live)
            .filter_map(|r| r.probe.ping(now).map(|p| (r.seat, p)));

        match self.role {
            Role::Host => direct.collect(),
            Role::Guest => {
                let host = direct.collect::<Vec<_>>();
                let to_host = host.first().map(|(_, p)| *p);
                let others = self
                    .relayed
                    .iter()
                    .filter(|(s, _)| *s != self.seat && !self.gone(*s))
                    .filter_map(|(s, p)| to_host.map(|h| (*s, h + *p)));

                host.iter().copied().chain(others).collect()
            }
        }
    }

    /// The speed everyone runs at: the host's.
    pub const fn speed(&self) -> f32 {
        self.speed
    }

    /// Whether the seal after step `ran` makes a bundle: its frame is
    /// past the last sealed. After a lowering it is not, for a few
    /// steps: the orders wait.
    ///
    /// ```text
    ///  delay 5 -> 3 after step n:  steps n+1, n+2 seal nothing,
    ///  last frame sealed:  n+5     n+5     n+5     n+6 (step n+3 seals)
    /// ```
    ///
    /// Each frame has one bundle of each seat, or is in a raise's gap.
    /// Let L be the last frame a peer sealed. Step `n` seals frame
    /// `n + d` only if `n + d > L`, and then `n + d == L + 1`: it holds at
    /// the start (`L = d - 1`), after a resync (`L = step + d - 1`), after
    /// a raise (`change` sets `L = n + d'`, so the next step seals
    /// `n + 1 + d'`) and while a lowering waits (`L` is not passed, so no
    /// step seals). So the frames sealed are consecutive but for the
    /// gap, none twice. `L` follows the same `d` at the same steps on
    /// every peer, so the gap is the same frames everywhere and no peer
    /// waits for a bundle that does not exist.
    pub fn seals(&self, ran: u32) -> bool {
        ran + self.delay > self.have[Self::index(self.seat)].frames
    }

    /// Seals refused for being out of order: a test's.
    #[cfg(test)]
    pub const fn misseals(&self) -> u32 {
        self.misseals
    }

    /// Whether the host has a delay sealed and not yet taken: a test's.
    #[cfg(test)]
    pub const fn changing(&self) -> bool {
        self.pending.is_some()
    }

    /// The host: a new delay for the game, if the pings call for one and
    /// the game is calm; it goes in the bundle sealed next, and counts as
    /// in flight until [`Node::change`] takes it.
    pub fn announce(&mut self, now: Instant) -> Option<u32> {
        let live = || self.remotes.iter().filter(|r| r.live);
        let calm = self.role == Role::Host
            && self.pending.is_none()
            && !self.holding()
            && matches!(self.sync, Sync::Idle)
            && self.back.is_none()
            && !self.paused()
            && self.waiting(now).is_empty()
            && live().all(|r| r.heard.is_some() && r.probe.ping(now).is_some());

        if calm {
            let guests: Vec<Vec<Duration>> = live().map(|r| r.probe.samples()).collect();
            let new = self.steer.decide(now, self.delay, &guests)?;

            self.pending = Some(new);
            Some(new)
        } else {
            None
        }
    }

    /// The delay `new` taken after step `ran`, by every peer alike: the
    /// seals from step `ran + 1` on reach `new` frames ahead. A raise
    /// leaves the frames between the last sealed and the next empty for
    /// every seat; a lowering seals nothing until the frames pass the
    /// last sealed ([`Node::seals`]), so each frame has one bundle of
    /// each seat, or is in a run no seat sealed.
    ///
    /// ```text
    ///  delay 3 -> 5 after step n:   sealed  ... n+3 |gap n+4, n+5| n+6 ...
    ///                               step n+1 seals n+6 (= n+1 + 5)
    /// ```
    pub fn change(&mut self, ran: u32, new: u32) {
        let me = Self::index(self.seat);
        let first = self.have[me].frames + 1;
        let last = ran + new;
        let old = std::mem::replace(&mut self.delay, new);

        self.pending = None;

        if last >= first {
            self.skips.insert(first, last);
            self.have[me].frames = last;

            let own = self.seat;

            for s in self.seats().filter(|s| *s != own) {
                self.advance(Self::index(s));
            }

            self.fresh = true;
        }

        info!("net: lockstep delay {old} -> {new} at step {}", ran + 1);
    }

    /// Seat `s`'s held frames counted on, over runs no seat sealed.
    fn advance(&mut self, s: usize) {
        loop {
            let next = self.have[s].frames + 1;

            if let Some(last) = self.skips.get(&next) {
                self.have[s].frames = *last;
            } else if self.bundles[s].contains_key(&next) {
                self.have[s].frames = next;
            } else {
                break;
            }

            self.fresh = true;
        }
    }

    /// Whether frame `frame` may run: every other seat's bundle for it
    /// is held, or the seat was dropped before it. This peer's own is
    /// sealed before the frame steps, a delay ahead: with one frame's
    /// delay, as it steps (`Peer::exchange`).
    pub fn ready(&self, frame: u32) -> bool {
        !self.holding()
            && self.seats().filter(|s| *s != self.seat).all(|s| {
                let s = Self::index(s);

                self.have[s].frames >= frame
                    || self.dropped[s].is_some_and(|(last, _)| frame > last)
            })
    }

    /// Each seat's items for frame `frame`, in seat order: a dropped
    /// seat's after its last are none.
    pub fn items(&self, frame: u32) -> Vec<(u32, Vec<Item>)> {
        self.seats()
            .filter_map(|s| {
                let bundle = self.bundles[Self::index(s)].get(&frame)?;

                Some((s, bundle.items.clone()))
            })
            .collect()
    }

    /// Whether a bundle held for a frame past `after`, any seat's, asks
    /// a save: the game steps on to it, paused or not.
    pub fn save_ahead(&self, after: u32) -> bool {
        self.bundles.iter().any(|held| {
            held.range(after + 1..)
                .any(|(_, b)| b.items.iter().any(|i| matches!(i, Item::Save(_))))
        })
    }

    /// This peer's bundle for `frame`: the next of its own, given the
    /// oldest deep digest folded and not yet sent.
    pub fn seal(&mut self, frame: u32, mut bundle: Bundle) {
        let me = Self::index(self.seat);

        if frame == self.have[me].frames + 1 {
            bundle.deep = self.folded.pop_front();

            // Compared only once sent: a peer that finds a desync must
            // have sent what lets the others find it too.
            self.mine((Depth::Quick, bundle.step), bundle.digest);

            if let Some((at, deep)) = bundle.deep {
                self.mine((Depth::Deep, at), deep);
            }

            self.bundles[me].insert(frame, bundle);
            self.have[me].frames = frame;
            self.fresh = true;
        } else {
            self.misseals += 1;
            error!(
                "net: bundle for frame {frame} sealed after {}",
                self.have[me].frames
            );
        }
    }

    /// This peer's deep digest of `frame`, folded: sent with the next
    /// bundle free of one. The host compares it at once: a guest that
    /// found a desync holds, and with it the host's next seal, while the
    /// host's finding it is what resyncs.
    pub fn folded(&mut self, frame: u32, deep: u64) {
        self.folded.push_back((frame, deep));

        if self.role == Role::Host {
            self.mine((Depth::Deep, frame), deep);
        }
    }

    /// Whether every other seat's deep digest of `frame` was compared
    /// with this peer's, or the seat dropped.
    pub fn compared(&self, frame: u32) -> bool {
        let heard = self.compared.get(&frame).copied().unwrap_or_default();

        self.seats()
            .filter(|s| *s != self.seat)
            .all(|s| heard >> s & 1 == 1 || self.dropped[Self::index(s)].is_some())
    }

    /// This peer's chat `line`, sent to every seat.
    pub fn say(&mut self, line: String) {
        let me = Self::index(self.seat);
        let seq = self.have[me].chat + 1;

        self.chat[me].insert(seq, line);
        self.have[me].chat = seq;
        self.shown[me] = seq;
        self.fresh = true;
    }

    /// The other seats' chat lines come since the last call, in order.
    pub fn take_chat(&mut self) -> Vec<(u32, String)> {
        let mut lines = Vec::new();

        for seat in self.seats() {
            let s = Self::index(seat);

            while self.shown[s] < self.have[s].chat {
                self.shown[s] += 1;

                if let Some(line) = self.chat[s].get(&self.shown[s]) {
                    lines.push((seat, line.clone()));
                }
            }
        }

        lines
    }

    /// The seats gone since the last call, in order.
    pub fn take_left(&mut self) -> Vec<(u32, Leave)> {
        std::mem::take(&mut self.left)
    }

    /// This peer leaves: every remote let go, and told so.
    pub fn bye(&mut self) -> Vec<(SocketAddr, Vec<u8>)> {
        let bytes = Packet::Bye(self.seat).encode();
        let mut out = Vec::new();

        // The host's stop goes first: a guest told only of the goodbye
        // would play on alone.
        if self.role == Role::Host && self.stop.is_some() {
            for remote in self.remotes.iter().filter(|r| r.live) {
                let stop = Packet::Game(self.game_for(remote)).encode();

                out.extend(std::iter::repeat_n((remote.addr, stop), BYE_COPIES));
            }
        }

        // A guest waiting in the lobby is told too: no game comes.
        for remote in self.remotes.iter_mut().filter(|r| r.live || r.waiting) {
            remote.live = false;
            remote.waiting = false;
            out.extend(std::iter::repeat_n(
                (remote.addr, bytes.clone()),
                BYE_COPIES,
            ));
        }

        out
    }

    /// Whether a guest's host is back in its lobby.
    pub const fn regrouped(&self) -> bool {
        self.regrouped
    }

    /// This peer back to the lobby: every remote let go. The host tells
    /// each guest in the game to come ([`Packet::Regroup`]) and seats
    /// them and those already asking ([`Remote::waiting`]); a guest says
    /// goodbye, unless its host went first.
    pub fn regroup(&mut self) -> (Back, Vec<(SocketAddr, Vec<u8>)>) {
        let out = match (self.role, self.regrouped) {
            (Role::Host, _) => {
                let come = Packet::Regroup.encode();

                self.remotes
                    .iter()
                    .filter(|r| r.live)
                    .flat_map(|r| std::iter::repeat_n((r.addr, come.clone()), BYE_COPIES))
                    .collect()
            }
            (Role::Guest, false) => self.bye(),
            (Role::Guest, true) => Vec::new(),
        };
        let back = if let (Role::Guest, Some(host)) = (self.role, self.remotes.first()) {
            Back::Guest(host.addr)
        } else {
            let mut guests: Vec<(SocketAddr, u32)> = self
                .remotes
                .iter()
                .filter(|r| r.live || r.waiting)
                .map(|r| (r.addr, r.seat))
                .collect();

            guests.sort_by_key(|(_, seat)| *seat);
            Back::Host(guests)
        };

        for remote in &mut self.remotes {
            remote.live = false;
            remote.waiting = false;
        }

        (back, out)
    }

    /// Frame `frame` ran: what every peer has of it and before is let go.
    pub fn fed(&mut self, frame: u32) {
        for s in self.seats() {
            let i = Self::index(s);
            let acked = self
                .remotes
                .iter()
                .filter(|r| r.live && self.relays(r, s))
                .map(|r| r.acked[i].frames)
                .min()
                .unwrap_or(u32::MAX);
            let keep = frame.min(acked);

            self.bundles[i] = self.bundles[i].split_off(&(keep + 1));
        }

        self.skips.retain(|_, last| *last > frame);

        let oldest = frame.saturating_sub(KEPT_DIGESTS);

        // Only those past the window, a kind at a time: a retain walked
        // all 2,000 each frame, 4 % of a late game's step. The others'
        // digests too: one this peer never sent is never compared.
        for depth in Depth::ALL {
            Self::forget(&mut self.mine, (depth, oldest));
            Self::forget(&mut self.theirs, (depth, oldest));
        }

        self.compared = self.compared.split_off(&oldest);
    }

    /// `digests` of the kind and frames before `(depth, oldest)` let go.
    fn forget<T>(digests: &mut BTreeMap<Of, T>, (depth, oldest): Of) {
        while let Some(of) = digests
            .range((depth, 0)..(depth, oldest))
            .next()
            .map(|(of, _)| *of)
        {
            digests.remove(&of);
        }
    }

    /// Whether this peer sends seat `seat`'s streams to `remote`: the
    /// host every seat but the remote's, a guest its own.
    const fn relays(&self, remote: &Remote, seat: u32) -> bool {
        match self.role {
            Role::Host => seat != remote.seat,
            Role::Guest => seat == self.seat,
        }
    }

    /// Whether `remote` may send seat `seat`'s streams: a guest its own,
    /// the host any seat but this peer's.
    const fn accepts(&self, remote: &Remote, seat: u32) -> bool {
        match self.role {
            Role::Host => seat == remote.seat,
            Role::Guest => seat != self.seat && seat < self.players,
        }
    }

    /// Whether every seat's bundles through frame `last` are held, and
    /// every remote has all this peer sends it: a finished run may leave.
    pub fn settled(&self, last: u32) -> bool {
        let held = self.seats().all(|s| {
            let s = Self::index(s);

            self.have[s].frames >= last || self.dropped[s].is_some()
        });
        let told = self.remotes.iter().filter(|r| r.live).all(|r| {
            self.seats()
                .filter(|s| self.relays(r, *s))
                .all(|s| r.acked[Self::index(s)] == self.have[Self::index(s)])
        });

        held && told
    }

    /// `packet` from `from`, at `now`; what to send back at once.
    pub fn receive(
        &mut self,
        from: SocketAddr,
        packet: Packet,
        now: Instant,
    ) -> Vec<(SocketAddr, Packet)> {
        let at = self.remotes.iter().position(|r| r.addr == from);

        self.now = now;

        match (packet, at) {
            (Packet::Game(game), Some(at)) => self.game(at, game, now),
            (Packet::Chunk(chunk), Some(_)) if self.role == Role::Guest => {
                self.chunk(chunk);
                Vec::new()
            }
            // A guest gone back to the lobby waits for the host's.
            (Packet::Hello(_), Some(at)) if self.role == Role::Host && !self.remotes[at].live => {
                self.remotes[at].waiting = true;
                vec![(from, Packet::Wait)]
            }
            // A guest that missed its welcome asks again.
            (Packet::Hello(_), Some(at)) => self.remotes[at]
                .welcome
                .as_ref()
                .and_then(|(bytes, _)| Packet::decode(bytes).ok())
                .map(|w| vec![(from, w)])
                .unwrap_or_default(),
            (Packet::Bye(seat), Some(at))
                if self.remotes[at].live && seat == self.remotes[at].seat =>
            {
                self.lost(at, Leave::Quit);
                Vec::new()
            }
            (Packet::Regroup, Some(at)) if self.role == Role::Guest && self.remotes[at].live => {
                self.regrouped = true;
                Vec::new()
            }
            (Packet::Hello(_), None) if self.role == Role::Host => {
                vec![(from, Packet::Refused("the game has begun".to_owned()))]
            }
            (Packet::Ping(id), Some(at)) if self.remotes[at].live => {
                vec![(from, Packet::Pong(id))]
            }
            (Packet::Pong(id), Some(at)) => {
                self.remotes[at].probe.answered(id, now);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A game packet from remote `at`; what to send back at once: a
    /// window of the host's file, where a guest wants it.
    fn game(&mut self, at: usize, game: Game, now: Instant) -> Vec<(SocketAddr, Packet)> {
        let remote = &self.remotes[at];

        if !remote.live || game.from != remote.seat {
            return Vec::new();
        }

        let seat = remote.seat;
        let remote = &mut self.remotes[at];

        // Heard in any epoch: no drop while a resync holds the game.
        remote.heard = Some(now);
        remote.welcome = None;
        remote.epoch = remote.epoch.max(game.epoch);

        let replies = self.resync(at, game.epoch, game.resync);

        if game.epoch != self.epoch {
            return replies;
        }

        let remote = &mut self.remotes[at];

        // Acks only grow: a late packet's are old.
        for (held, ack) in remote.acked.iter_mut().zip(&game.acks) {
            held.frames = held.frames.max(ack.frames);
            held.chat = held.chat.max(ack.chat);
        }

        self.pauses(seat, game.paused);

        if self.role == Role::Guest {
            self.speed = game.speed;
            self.reported = game
                .waiting
                .iter()
                .map(|(s, ms)| (*s, now + Duration::from_millis((*ms).into())))
                .collect();
            self.relayed = game
                .pings
                .iter()
                .map(|(s, ms)| (*s, Duration::from_millis((*ms).into())))
                .collect();

            for (s, last, how) in game.dropped {
                if self.drop_seat(s, Some(last), how) {
                    self.left.push((s, how));
                }
            }
        }

        for (s, frame, bundle) in game.bundles {
            if self.accepts(&self.remotes[at], s) {
                self.bundle(s, frame, bundle);
            }
        }

        for (s, seq, line) in game.chat {
            if self.accepts(&self.remotes[at], s) {
                self.line(s, seq, &line);
            }
        }

        replies
    }

    /// A packet's resync word from remote `at`, of `epoch`: the host
    /// answers a want with a window of its file; a guest holds for an
    /// offer of a later epoch, and takes a stop.
    fn resync(&mut self, at: usize, epoch: u32, resync: Resync) -> Vec<(SocketAddr, Packet)> {
        let addr = self.remotes[at].addr;

        match (resync, &self.sync) {
            (Resync::Want { file, from }, Sync::Sending { file: out, .. })
                if self.role == Role::Host && out.file == file =>
            {
                out.chunks(from)
                    .into_iter()
                    .map(|c| (addr, Packet::Chunk(c)))
                    .collect()
            }
            (
                Resync::Offer {
                    frame,
                    file,
                    size,
                    delay,
                    back,
                },
                sync,
            ) if self.role == Role::Guest && epoch > self.epoch => {
                let had = match sync {
                    Sync::Taking { epoch: e, .. } | Sync::Came { epoch: e, .. } => *e == epoch,
                    _ => false,
                };

                if !had {
                    info!("net: the host resyncs from frame {frame}: {size} bytes");
                    self.sync = Sync::Taking {
                        epoch,
                        file: Incoming::new(file, size),
                        back,
                        delay: delay.clamp(LEAST_DELAY, MOST_DELAY),
                        asked: None,
                    };

                    // A rejoin is said as it ends, not as a desync.
                    if back.is_none() {
                        self.notices.push(Notice::Resyncing(frame));
                    }
                }

                Vec::new()
            }
            (Resync::Stop(why), _) if self.role == Role::Guest && self.stop.is_none() => {
                self.stop = Some(why);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// A chunk of the host's file, kept; whole, the file came, or is
    /// asked again where it is not what was offered.
    fn chunk(&mut self, chunk: super::transfer::Chunk) {
        if let Sync::Taking {
            epoch,
            file,
            back,
            delay,
            asked,
        } = &mut self.sync
        {
            file.take(chunk);

            match file.done() {
                Some(Ok(bytes)) => {
                    self.sync = Sync::Came {
                        epoch: *epoch,
                        bytes: Some(bytes),
                        back: *back,
                        delay: *delay,
                    };
                }
                Some(Err(why)) => {
                    error!("net: the resync's file: {why}; asked again");
                    *file = Incoming::new(file.file.clone(), file.size());
                    *asked = None;
                }
                None => {}
            }
        }
    }

    /// The pause bits a packet from seat `from` carries: a guest's own,
    /// the host's every seat's but this peer's.
    fn pauses(&mut self, from: u32, mask: u8) {
        let bit = |s: u32| mask >> s & 1 == 1;
        let heard: Vec<u32> = match self.role {
            Role::Host => vec![from],
            Role::Guest => self.seats().filter(|s| *s != self.seat).collect(),
        };

        for s in heard {
            let was = std::mem::replace(&mut self.paused[Self::index(s)], bit(s));

            if was != bit(s) {
                let how = if bit(s) { "paused" } else { "resumed" };

                info!("net: player {s} {how} the game");
            }
        }
    }

    /// Seat `seat`'s bundle for `frame`, its orders checked to be the
    /// seat's own.
    fn bundle(&mut self, seat: u32, frame: u32, mut bundle: Bundle) {
        let s = Self::index(seat);
        let held = self.have[s].frames;
        let past_drop = self.dropped[s].is_some_and(|(last, _)| frame > last);
        let host = self.host();

        if frame > held && frame <= held + WINDOW && !past_drop {
            bundle.items.retain(|item| match item {
                Item::Order(order) if order.player != seat => {
                    warn!("net: seat {seat} sent an order for player {}", order.player);
                    false
                }
                // Only the host sets the delay, and within the limits.
                Item::Delay(frames)
                    if seat != host || !(LEAST_DELAY..=MOST_DELAY).contains(frames) =>
                {
                    warn!("net: seat {seat} sent a delay of {frames}");
                    false
                }
                _ => true,
            });

            self.theirs
                .entry((Depth::Quick, bundle.step))
                .or_default()
                .push((seat, bundle.digest));

            if let Some((at, deep)) = bundle.deep {
                self.theirs
                    .entry((Depth::Deep, at))
                    .or_default()
                    .push((seat, deep));
            }

            self.bundles[s].insert(frame, bundle);
            self.compare();

            self.advance(s);
        }
    }

    /// A remote's chat line: cut to [`CHAT_MOST`] characters, as this
    /// peer's own, so it fits a packet; one further ahead than a packet
    /// carries is not held.
    fn line(&mut self, seat: u32, seq: u32, line: &str) {
        let s = Self::index(seat);
        let next = self.have[s].chat;

        if seq > next && seq - next <= LINES_AHEAD {
            self.chat[s].insert(seq, line.chars().take(CHAT_MOST).collect());

            while self.chat[s].contains_key(&(self.have[s].chat + 1)) {
                self.have[s].chat += 1;
                self.fresh = true;
            }
        }
    }

    /// One of this peer's digests, sent.
    fn mine(&mut self, of: Of, digest: u64) {
        self.mine.insert(of, digest);
        self.compare();
    }

    /// Every other seat's digests this peer has its own of, compared:
    /// the first that differs stops play.
    fn compare(&mut self) {
        let ready: Vec<Of> = self
            .theirs
            .keys()
            .copied()
            .filter(|of| self.mine.contains_key(of))
            .collect();

        for of in ready {
            let (kind, frame) = of;
            let mine = self.mine[&of];

            for (seat, theirs) in self.theirs.remove(&of).unwrap_or_default() {
                if kind == Depth::Deep {
                    *self.compared.entry(frame).or_default() |= 1 << seat;
                }

                if theirs != mine && self.desync.is_none() {
                    self.desync = Some(Desync {
                        frame,
                        seat,
                        mine,
                        theirs,
                        kind,
                    });
                }
            }
        }
    }

    /// Seat `seat` out of play after frame `last`, its own bundles held
    /// so far by default; whether it was in play.
    fn drop_seat(&mut self, seat: u32, last: Option<u32>, how: Leave) -> bool {
        let s = Self::index(seat);
        let was = seat != self.seat && seat < self.players && self.dropped[s].is_none();

        if was {
            let last = last.unwrap_or(self.have[s].frames);

            warn!(
                "net: player {seat} dropped after frame {last} ({how:?}); its kingdom plays on uncontrolled"
            );
            self.dropped[s] = Some((last, how));
            self.fresh = true;
        }

        was
    }

    /// Remote `n` gone, as `how` says: the host drops a guest's seat, a
    /// guest that loses the host every other seat, and plays on alone.
    /// Either way one seat is told gone: the remote's.
    fn lost(&mut self, n: usize, how: Leave) {
        let seat = self.remotes[n].seat;

        self.remotes[n].live = false;
        self.reported.clear();

        let gone = match self.role {
            Role::Host => self.drop_seat(seat, None, how),
            Role::Guest => {
                warn!("net: the host left ({how:?}); playing on alone");

                for s in self.seats().filter(|s| *s != seat) {
                    self.drop_seat(s, None, how);
                }

                self.drop_seat(seat, None, how)
            }
        };

        if gone {
            self.left.push((seat, how));
        }
    }

    /// Silent remotes dropped.
    fn timeouts(&mut self, now: Instant) {
        let silent: Vec<usize> = (0..self.remotes.len())
            .filter(|&n| {
                let remote = &self.remotes[n];

                remote.live && remote.silent(now, self.patience(remote))
            })
            .collect();

        for n in silent {
            self.lost(n, Leave::Silent);
        }
    }

    /// The packets due at `now`: timeouts first, then each live remote's
    /// game packet, when there is news or the heartbeat is due, and a
    /// pending welcome.
    pub fn packets(&mut self, now: Instant) -> Vec<(SocketAddr, Vec<u8>)> {
        self.now = now;
        self.timeouts(now);
        self.sent_all();
        self.ask(now);

        let fresh = std::mem::take(&mut self.fresh);
        let mut out = Vec::new();

        for n in 0..self.remotes.len() {
            let remote = &self.remotes[n];
            let due = remote
                .sent
                .is_none_or(|t| now.duration_since(t) >= SEND_EVERY);

            if remote.live && (fresh || due) {
                out.push((remote.addr, Packet::Game(self.game_for(remote)).encode()));
                self.remotes[n].sent = Some(now);
            }

            let remote = &mut self.remotes[n];

            if remote.live
                && let Some(ping) = remote.probe.ask(now)
            {
                out.push((remote.addr, ping.encode()));
            }

            let addr = self.remotes[n].addr;

            if let Some((bytes, sent)) = &mut self.remotes[n].welcome
                && sent.is_none_or(|t| now.duration_since(t) >= WELCOME_EVERY)
            {
                out.push((addr, bytes.clone()));
                *sent = Some(now);
            }
        }

        out
    }

    /// A guest's ask for the host's file due at `now`: the first, once
    /// the window asked is in, or after [`ASK_AGAIN`] where a chunk was
    /// lost. Asked in every packet, the host would send each window
    /// many times over.
    fn ask(&mut self, now: Instant) {
        if let Sync::Taking { file, asked, .. } = &mut self.sync {
            let from = file.from();
            let due = asked.is_none_or(|(last, at)| {
                from >= last.saturating_add(transfer::WINDOW) || now.duration_since(at) >= ASK_AGAIN
            });

            if due {
                *asked = Some((from, now));
                self.fresh = true;
            }
        }
    }

    /// The host's file, once every guest in play is of its epoch: sent.
    fn sent_all(&mut self) {
        let all = self
            .remotes
            .iter()
            .filter(|r| r.live)
            .all(|r| r.epoch == self.epoch);

        if let (Sync::Sending { back, .. }, true) = (&self.sync, all) {
            // A rejoining seat dropped again before it came is no news.
            match *back {
                Some(seat) if !self.gone(seat) => self.rejoined.push(seat),
                Some(_) => {}
                None => self.notices.push(Notice::Resynced),
            }

            self.sync = Sync::Idle;
        }
    }

    /// The resync word for `remote`: the host's stop, or its offer to a
    /// guest not yet of its epoch; a guest's want.
    fn resync_for(&self, remote: &Remote) -> Resync {
        match (&self.stop, &self.sync) {
            (Some(why), _) if self.role == Role::Host => Resync::Stop(why.clone()),
            (_, Sync::Sending { frame, file, back }) if remote.epoch < self.epoch => {
                Resync::Offer {
                    frame: *frame,
                    file: file.file.clone(),
                    size: file.size(),
                    delay: self.delay,
                    back: *back,
                }
            }
            (
                _,
                Sync::Taking {
                    file,
                    asked: Some((from, at)),
                    ..
                },
            ) if *at == self.now => Resync::Want {
                file: file.file.clone(),
                from: *from,
            },
            _ => Resync::None,
        }
    }

    /// `held`'s entries from `from` to `to`, none where `from` is past.
    fn unacked<T>(held: &BTreeMap<u32, T>, from: u32, to: u32) -> impl Iterator<Item = (&u32, &T)> {
        held.range(from..).take_while(move |(n, _)| **n <= to)
    }

    /// The game packet for `remote`: this peer's state, and the bundles
    /// and lines it lacks, oldest first, within [`BUDGET`] (but for a
    /// first bundle over it).
    fn game_for(&self, remote: &Remote) -> Game {
        let mut room = BUDGET;
        let mut bundles = Vec::new();
        let mut chat = Vec::new();

        for s in self.seats().filter(|s| self.relays(remote, *s)) {
            let i = Self::index(s);
            // A remote's acks are its word: one at `u32::MAX` must not
            // overflow.
            let from = remote.acked[i].frames.saturating_add(1);

            // In order, to the first that does not fit: the rest go next.
            // A packet's first bundle goes whatever its size, or a frame
            // of many orders would never go.
            for (frame, bundle) in Self::unacked(&self.bundles[i], from, self.have[i].frames) {
                let size = BUNDLE_BYTES + bundle.items.iter().map(item_bytes).sum::<usize>();

                if size > room && !bundles.is_empty() {
                    break;
                }

                room = room.saturating_sub(size);
                bundles.push((s, *frame, bundle.clone()));
            }

            let first = remote.acked[i].chat.saturating_add(1);

            for (seq, line) in Self::unacked(&self.chat[i], first, self.have[i].chat) {
                let size = BUNDLE_BYTES + line.len();

                if size > room {
                    break;
                }

                room -= size;
                chat.push((s, *seq, line.clone()));
            }
        }

        let paused = self
            .seats()
            .filter(|s| self.paused[Self::index(*s)])
            .filter(|s| self.role == Role::Host || *s == self.seat)
            .fold(0, |mask, s| mask | 1 << s);
        let dropped = match self.role {
            Role::Host => self
                .seats()
                .filter_map(|s| self.dropped[Self::index(s)].map(|(last, how)| (s, last, how)))
                .collect(),
            Role::Guest => Vec::new(),
        };
        let waiting = match self.role {
            Role::Host => self
                .waiting(self.now)
                .into_iter()
                .map(|(s, _, left)| (s, u32::try_from(left.as_millis()).unwrap_or(u32::MAX)))
                .collect(),
            Role::Guest => Vec::new(),
        };
        let pings = match self.role {
            Role::Host => self
                .pings(self.now)
                .into_iter()
                .map(|(s, p)| (s, u32::try_from(p.as_millis()).unwrap_or(u32::MAX)))
                .collect(),
            Role::Guest => Vec::new(),
        };

        Game {
            from: self.seat,
            epoch: self.epoch,
            paused,
            speed: self.speed,
            dropped,
            waiting,
            pings,
            acks: self.have.clone(),
            bundles,
            chat,
            resync: self.resync_for(remote),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: u32 = 3;

    fn addr(n: u16) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, 1], n))
    }

    fn pair(now: Instant) -> (Node, Node) {
        let host = Node::new((Role::Host, 0), (2, DELAY), vec![(addr(2), 1, None)], now);
        let guest = Node::new((Role::Guest, 1), (2, DELAY), vec![(addr(1), 0, None)], now);

        (host, guest)
    }

    fn empty(digest: u64) -> Bundle {
        Bundle {
            items: Vec::new(),
            step: 0,
            digest,
            deep: None,
        }
    }

    /// Every packet `from` sends, received by `to`.
    fn carry(from: &mut Node, from_addr: SocketAddr, to: &mut Node, now: Instant) {
        for (_, bytes) in from.packets(now) {
            let packet = Packet::decode(&bytes).expect("decoded");

            to.receive(from_addr, packet, now);
        }
    }

    #[test]
    fn the_frames_below_the_delay_run_at_once_and_the_next_waits() {
        let (host, _) = pair(Instant::now());

        assert!(host.ready(DELAY - 1));
        assert!(!host.ready(DELAY));
    }

    /// A game of one, a frame's delay: each frame runs at once, its
    /// bundle sealed as it steps, and nothing is sent or kept.
    #[test]
    fn a_peer_alone_never_waits() {
        let now = Instant::now();
        let mut solo = Node::new((Role::Host, 0), (1, 1), Vec::new(), now);
        let order = Item::Order("p0 dismiss 3".parse().expect("order"));

        for frame in 1..=3 {
            assert!(solo.ready(frame));

            solo.seal(
                frame,
                Bundle {
                    items: vec![order.clone()],
                    step: 0,
                    digest: 1,
                    deep: None,
                },
            );

            assert_eq!(solo.items(frame), [(0, vec![order.clone()])]);

            solo.fed(frame);
        }

        assert_eq!(solo.packets(now).len(), 0);
        assert!(solo.bundles[0].is_empty());
    }

    /// A peer's own digests stay [`KEPT_DIGESTS`] frames, of each kind:
    /// the older let go as frames run.
    #[test]
    fn a_peers_own_digests_stay_for_a_window() {
        const LAST: u32 = KEPT_DIGESTS + 10;

        let mut solo = Node::new((Role::Host, 0), (1, 1), Vec::new(), Instant::now());

        for frame in 1..=LAST {
            if frame % 100 == 5 {
                solo.folded(frame, 7);
            }

            // Sealed after step `frame - 1`, a delay of 1.
            solo.seal(
                frame,
                Bundle {
                    step: frame - 1,
                    ..empty(1)
                },
            );
            solo.fed(frame);
        }

        let kept = |depth: Depth| {
            let frames: Vec<u32> = solo
                .mine
                .keys()
                .filter(|(d, _)| *d == depth)
                .map(|(_, f)| *f)
                .collect();

            (frames.len(), frames.first().copied())
        };

        // Quick: frames 10 to 2,009 of 0 to 2,009. Deep: 105 to 2,005, not 5.
        assert_eq!(kept(Depth::Quick), (2_000, Some(10)));
        assert_eq!(kept(Depth::Deep), (20, Some(105)));
    }

    #[test]
    fn a_frame_runs_once_both_seats_bundles_are_in() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        host.seal(DELAY, empty(1));
        carry(&mut host, addr(1), &mut guest, now);

        assert!(!host.ready(DELAY), "the guest's is missing");

        guest.seal(DELAY, empty(1));
        carry(&mut guest, addr(2), &mut host, now);

        assert!(host.ready(DELAY) && guest.ready(DELAY));
        assert_eq!(host.desync(), None);
    }

    #[test]
    fn differing_digests_of_a_frame_are_a_desync() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        host.seal(DELAY, empty(1));
        guest.seal(DELAY, empty(2));
        carry(&mut guest, addr(2), &mut host, now);

        assert_eq!(
            host.desync().map(|d| (d.frame, d.seat, d.kind)),
            Some((0, 1, Depth::Quick))
        );
        assert!(!host.ready(DELAY));
    }

    /// A deep digest folded rides the next bundle sealed, whatever its
    /// frame: equal, the frame is compared; differing, a desync of it.
    #[test]
    fn a_deep_digest_folded_goes_with_the_next_bundle() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        host.folded(0, 5);
        guest.folded(0, 5);
        host.seal(DELAY, empty(1));
        guest.seal(DELAY, empty(1));
        carry(&mut guest, addr(2), &mut host, now);

        assert!(host.compared(0));
        assert_eq!(host.desync(), None);

        host.folded(1, 6);
        guest.folded(1, 7);

        assert!(!host.compared(1), "not yet sent");

        host.seal(DELAY + 1, empty(1));
        guest.seal(DELAY + 1, empty(1));
        carry(&mut guest, addr(2), &mut host, now + SEND_EVERY);

        assert_eq!(
            host.desync().map(|d| (d.frame, d.kind, d.mine, d.theirs)),
            Some((1, Depth::Deep, 6, 7))
        );
    }

    /// A raise leaves the frames between the last sealed and the next
    /// empty for both seats, a lowering seals nothing until the frames
    /// pass the last sealed: each frame has one bundle of each seat, or
    /// is in the run no seat sealed; no step waits on either.
    #[test]
    fn a_raise_leaves_a_gap_and_a_lowering_seals_nothing() {
        const RAISED_AT: u32 = 2;
        const LOWERED_AT: u32 = 10;
        const STEPS: u32 = 30;

        let now = Instant::now();
        let (mut host, mut guest) = pair(now);
        let mut sealed: [Vec<u32>; 2] = [Vec::new(), Vec::new()];

        for ran in 0..STEPS {
            for (node, kept) in [&mut host, &mut guest].into_iter().zip(&mut sealed) {
                if node.seals(ran) {
                    let frame = ran + node.delay();

                    node.seal(
                        frame,
                        Bundle {
                            step: ran,
                            ..empty(1)
                        },
                    );
                    kept.push(frame);
                }

                match ran {
                    RAISED_AT => node.change(ran, 5),
                    LOWERED_AT => node.change(ran, 3),
                    _ => {}
                }
            }

            carry(&mut host, addr(1), &mut guest, now);
            carry(&mut guest, addr(2), &mut host, now);

            assert!(host.ready(ran + 1) && guest.ready(ran + 1), "step {ran}");
            host.fed(ran + 1);
            guest.fed(ran + 1);
        }

        let [mine, theirs] = sealed;
        let gap = 6..=7;
        let reached = *mine.last().expect("sealed");
        let covered: Vec<u32> = (DELAY..=reached).filter(|f| !gap.contains(f)).collect();

        assert_eq!(mine, theirs, "both seat the same frames");
        assert_eq!(mine, covered, "once each, but the gap");
        assert_eq!((host.misseals(), guest.misseals()), (0, 0));
        assert_eq!(host.delay(), 3);
        assert_eq!(guest.delay(), 3);
        assert_eq!(host.have[1].frames, reached);
    }

    /// A guest that rejoins is welcomed with the delay in force, not the
    /// start's; a resync's offer says it too.
    #[test]
    fn a_rejoin_welcome_and_an_offer_name_the_delay_in_force() {
        let now = Instant::now();
        let welcome = Packet::Welcome(Welcome {
            seat: 1,
            players: 2,
            delay: DELAY,
            setup: crate::setup::Setup::default(),
            nicks: vec![String::new(); 2],
            host: 0,
            save: None,
        })
        .encode();
        let mut host = Node::new(
            (Role::Host, 0),
            (2, DELAY),
            vec![(addr(2), 1, Some(welcome))],
            now,
        );

        host.change(0, 6);
        host.toss(1);

        let sent = host.rejoin(addr(3), 1, now);

        assert!(sent.is_some(), "a dropped seat is taken back");

        let again = host.remotes[0].welcome.as_ref().expect("welcome");

        assert!(
            matches!(Packet::decode(&again.0), Ok(Packet::Welcome(w)) if w.delay == 6),
            "{:?}",
            Packet::decode(&again.0)
        );

        host.offer(7, vec![1, 2, 3]);

        let game = host.game_for(&host.remotes[0]);

        assert!(matches!(game.resync, Resync::Offer { delay: 6, .. }));
    }

    /// A digest of the others' this peer never sent its own of (a
    /// lowering skipped that step) is let go past the window.
    #[test]
    fn an_unmatched_digest_of_theirs_is_let_go_past_the_window() {
        const LAST: u32 = KEPT_DIGESTS + 10;

        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        // The guest sealed after step 0, the host never did.
        guest.seal(DELAY, empty(1));
        carry(&mut guest, addr(2), &mut host, now);
        assert_eq!(host.theirs.len(), 1);

        host.fed(KEPT_DIGESTS);
        assert_eq!(host.theirs.len(), 1, "inside the window");

        host.fed(LAST);
        assert!(host.theirs.is_empty(), "past it");
    }

    /// Only the host sets the delay, within the limits.
    #[test]
    fn a_delay_from_a_guest_or_past_the_limits_is_dropped() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        guest.seal(
            DELAY,
            Bundle {
                items: vec![Item::Delay(9)],
                ..empty(1)
            },
        );
        carry(&mut guest, addr(2), &mut host, now);
        assert_eq!(host.items(DELAY), [(1, Vec::new())], "a guest's");

        host.seal(
            DELAY,
            Bundle {
                items: vec![Item::Delay(MOST_DELAY + 1)],
                ..empty(1)
            },
        );
        carry(&mut host, addr(1), &mut guest, now);
        assert_eq!(
            guest.items(DELAY).first(),
            Some(&(0, Vec::new())),
            "past the limit"
        );
    }

    #[test]
    fn a_guests_order_for_another_player_is_dropped() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);
        let forged = Item::Order("p0 dismiss 3".parse().expect("order"));
        let own = Item::Order("p1 dismiss 4".parse().expect("order"));

        guest.seal(
            DELAY,
            Bundle {
                items: vec![forged, own.clone()],
                step: 0,
                digest: 1,
                deep: None,
            },
        );
        carry(&mut guest, addr(2), &mut host, now);

        assert_eq!(host.items(DELAY), [(1, vec![own])]);
    }

    #[test]
    fn a_silent_guest_is_dropped_after_its_last_frame() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        host.seal(DELAY, empty(1));
        host.seal(DELAY + 1, empty(1));
        guest.seal(DELAY, empty(1));
        carry(&mut guest, addr(2), &mut host, now);
        host.packets(now + DROP_AFTER * 2);

        assert!(host.ready(DELAY + 1), "the seat is out after frame {DELAY}");
        assert_eq!(host.dropped[1], Some((DELAY, Leave::Silent)));
    }

    /// The silence that drops a peer is HD's `TossPlayerAfter`, its
    /// file value.
    #[test]
    fn a_peer_is_dropped_after_hds_toss_player_after() {
        use regent_core::data::settings::cases::{self, Target};

        let Some(Target::Int { initial: Some(ms) }) =
            cases::find("TossPlayerAfter").map(|c| c.target)
        else {
            panic!("an int");
        };

        assert_eq!(u64::try_from(ms), Ok(TOSS_PLAYER_AFTER));
        assert_eq!(DROP_AFTER, Duration::from_secs(60));
    }

    /// A guest silent past [`WAIT_AFTER`] is waited on, with its silence
    /// and the time left; the host's drop takes it out at once.
    #[test]
    fn a_silent_guest_is_waited_on_and_the_host_may_drop_it() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        guest.seal(DELAY, empty(1));
        carry(&mut guest, addr(2), &mut host, now);

        let later = now + WAIT_AFTER * 2;

        assert_eq!(host.waiting(now + WAIT_AFTER / 2), []);
        assert_eq!(
            host.waiting(later),
            [(1, WAIT_AFTER * 2, DROP_AFTER.saturating_sub(WAIT_AFTER * 2))]
        );

        guest.toss(0);
        host.toss(1);

        assert_eq!(host.dropped[1], Some((DELAY, Leave::Silent)));
        assert_eq!(host.take_left(), [(1, Leave::Silent)]);
        assert_eq!(host.waiting(later), []);
        assert!(
            guest.dropped.iter().all(Option::is_none),
            "a guest drops none"
        );
    }

    /// Three players: each guest's bundle and chat reach the other guest
    /// through the host.
    #[test]
    fn the_host_relays_a_guests_streams_to_the_others() {
        let now = Instant::now();
        let host_remotes = vec![(addr(2), 1, None), (addr(3), 2, None)];
        let mut host = Node::new((Role::Host, 0), (3, DELAY), host_remotes, now);
        let mut one = Node::new((Role::Guest, 1), (3, DELAY), vec![(addr(1), 0, None)], now);
        let mut two = Node::new((Role::Guest, 2), (3, DELAY), vec![(addr(1), 0, None)], now);

        host.seal(DELAY, empty(1));
        one.seal(DELAY, empty(1));
        one.say("hi".to_owned());
        two.seal(DELAY, empty(1));
        carry(&mut one, addr(2), &mut host, now);
        carry(&mut two, addr(3), &mut host, now);

        for (at, bytes) in host.packets(now + SEND_EVERY) {
            let to = if at == addr(2) { &mut one } else { &mut two };

            to.receive(addr(1), Packet::decode(&bytes).expect("decoded"), now);
        }

        assert!(host.ready(DELAY) && one.ready(DELAY) && two.ready(DELAY));
        assert_eq!(two.take_chat(), [(1, "hi".to_owned())]);
        assert_eq!(one.take_chat(), []);
    }

    /// Three players, the second guest's deep digest parting. The first
    /// guest finds it and holds, so the host steps no further and seals
    /// no bundle more: its own fold, come late, is compared as it comes.
    #[test]
    fn a_hosts_late_fold_finds_the_desync_a_guest_holds_on() {
        let now = Instant::now();
        let host_remotes = vec![(addr(2), 1, None), (addr(3), 2, None)];
        let mut host = Node::new((Role::Host, 0), (3, DELAY), host_remotes, now);
        let mut one = Node::new((Role::Guest, 1), (3, DELAY), vec![(addr(1), 0, None)], now);
        let mut two = Node::new((Role::Guest, 2), (3, DELAY), vec![(addr(1), 0, None)], now);

        one.folded(0, 5);
        two.folded(0, 6);
        host.seal(DELAY, empty(1));
        one.seal(DELAY, empty(1));
        two.seal(DELAY, empty(1));
        carry(&mut one, addr(2), &mut host, now);
        carry(&mut two, addr(3), &mut host, now);

        for (at, bytes) in host.packets(now + SEND_EVERY) {
            let to = if at == addr(2) { &mut one } else { &mut two };

            to.receive(addr(1), Packet::decode(&bytes).expect("decoded"), now);
        }

        assert!(one.desync().is_some() && !one.ready(DELAY + 1), "held");
        assert_eq!(host.desync(), None);

        host.folded(0, 5);

        assert_eq!(
            host.desync().map(|d| (d.frame, d.seat, d.kind)),
            Some((0, 2, Depth::Deep))
        );
    }

    /// A guest dropped while paused, back in play: its old pause holds
    /// no one until it says otherwise.
    #[test]
    fn a_seat_back_in_play_is_not_paused() {
        let now = Instant::now();
        let (mut host, _) = pair(now);

        host.paused[1] = true;
        host.toss(1);

        assert!(!host.paused(), "dropped");

        host.back = Some(1);
        host.resume(DELAY, 1);

        assert!(!host.paused(), "back");
    }

    /// A guest's pause reaches the host, which names it; a dropped
    /// seat's pause names no one.
    #[test]
    fn the_host_names_a_guest_that_paused() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        guest.set_local(true, 1.0);
        carry(&mut guest, addr(2), &mut host, now);

        assert_eq!(host.pausers(), [1], "heard");

        host.toss(1);

        assert!(host.pausers().is_empty(), "dropped");
    }

    /// Each ping `from` (at `from_addr`) sends at `now`, answered by
    /// the remote at its address in `to` after its lateness.
    fn ping_pong(
        (from, from_addr): (&mut Node, SocketAddr),
        to: &mut [(&mut Node, SocketAddr, Duration)],
        now: Instant,
    ) {
        for (at, bytes) in from.packets(now) {
            let ping = Packet::decode(&bytes)
                .ok()
                .filter(|p| matches!(p, Packet::Ping(_)));

            if let (Some(ping), Some((remote, addr, late))) =
                (ping, to.iter_mut().find(|(_, a, _)| *a == at))
            {
                for (_, pong) in remote.receive(from_addr, ping, now) {
                    from.receive(*addr, pong, now + *late);
                }
            }
        }
    }

    /// The host times each guest; a guest its host, and another guest
    /// as its own trip and the host's, relayed.
    #[test]
    fn a_guest_sees_another_guests_ping_through_the_host() {
        let now = Instant::now();
        let ms = Duration::from_millis;
        let host_remotes = vec![(addr(2), 1, None), (addr(3), 2, None)];
        let mut host = Node::new((Role::Host, 0), (3, DELAY), host_remotes, now);
        let mut one = Node::new((Role::Guest, 1), (3, DELAY), vec![(addr(1), 0, None)], now);
        let mut two = Node::new((Role::Guest, 2), (3, DELAY), vec![(addr(1), 0, None)], now);

        ping_pong(
            (&mut host, addr(1)),
            &mut [(&mut one, addr(2), ms(20)), (&mut two, addr(3), ms(30))],
            now,
        );
        ping_pong(
            (&mut one, addr(2)),
            &mut [(&mut host, addr(1), ms(10))],
            now,
        );

        let later = now + ms(40);

        assert_eq!(host.pings(later), [(1, ms(20)), (2, ms(30))]);

        carry(&mut host, addr(1), &mut one, later);

        assert_eq!(one.pings(later), [(0, ms(10)), (2, ms(40))]);
    }

    #[test]
    fn chat_lines_arrive_once_in_order() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        guest.say("one".to_owned());
        guest.say("two".to_owned());
        carry(&mut guest, addr(2), &mut host, now);
        carry(&mut guest, addr(2), &mut host, now + SEND_EVERY);

        assert_eq!(
            host.take_chat(),
            [(1, "one".to_owned()), (1, "two".to_owned())]
        );
        assert_eq!(host.take_chat(), []);
    }

    /// Orders piled up in one frame (given while paused) make a bundle
    /// over [`BUDGET`]: it still goes, alone in its packet.
    #[test]
    fn a_bundle_over_the_budget_is_sent_alone() {
        const ORDERS: u32 = 40;

        let now = Instant::now();
        let (mut host, mut guest) = pair(now);
        let items: Vec<Item> = (0..ORDERS)
            .map(|n| Item::Order(format!("p1 dismiss {n}").parse().expect("order")))
            .collect();
        let size = BUNDLE_BYTES + items.iter().map(item_bytes).sum::<usize>();

        assert!(size > BUDGET, "{size}");

        guest.seal(
            DELAY,
            Bundle {
                items: items.clone(),
                step: 0,
                digest: 1,
                deep: None,
            },
        );
        carry(&mut guest, addr(2), &mut host, now);

        assert_eq!(host.items(DELAY), [(1, items)]);
    }

    /// A packet acking past `u32::MAX`'s frame and line: the next send
    /// does not overflow.
    #[test]
    fn an_ack_at_the_top_of_its_range_breaks_nothing() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);

        host.seal(DELAY, empty(1));
        host.say("hi".to_owned());
        guest.seal(DELAY, empty(1));

        for (_, bytes) in guest.packets(now) {
            let mut game = match Packet::decode(&bytes) {
                Ok(Packet::Game(game)) => game,
                Ok(Packet::Ping(_)) => continue,
                _ => panic!("a game packet"),
            };

            game.acks.fill(Ack {
                frames: u32::MAX,
                chat: u32::MAX,
            });

            host.receive(addr(2), Packet::Game(game), now);
        }

        assert_ne!(host.packets(now + SEND_EVERY).len(), 0);
    }

    /// A remote's chat line is cut to [`super::super::CHAT_MOST`]
    /// characters, and lines far past the next are not held.
    #[test]
    fn a_remote_line_is_cut_and_one_far_ahead_dropped() {
        let now = Instant::now();
        let (mut host, _) = pair(now);
        let long = "x".repeat(2_000);

        host.line(1, 1, &long);
        host.line(1, 1_000_000, "far");

        assert_eq!(host.take_chat(), [(1, "x".repeat(super::super::CHAT_MOST))]);
        assert_eq!(host.chat[1].len(), 1);
    }

    /// A guest taken the host's file asks for a window once, not in
    /// every packet: the next once that window is in, or a chunk lost,
    /// after [`ASK_AGAIN`].
    #[test]
    fn a_resync_file_is_asked_for_at_a_pace() {
        let now = Instant::now();
        let (mut host, mut guest) = pair(now);
        let wants = |packets: Vec<(SocketAddr, Vec<u8>)>| -> Vec<u32> {
            packets
                .iter()
                .filter_map(|(_, bytes)| match Packet::decode(bytes) {
                    Ok(Packet::Game(Game {
                        resync: Resync::Want { from, .. },
                        ..
                    })) => Some(from),
                    _ => None,
                })
                .collect()
        };

        host.hold(now);
        host.offer(0, vec![0; transfer::CHUNK * 40]);
        carry(&mut host, addr(1), &mut guest, now);

        // Heartbeats with nothing come: one ask.
        let asked: Vec<u32> = (1..=5)
            .flat_map(|k| wants(guest.packets(now + SEND_EVERY * k)))
            .collect();

        assert_eq!(asked, [0]);

        // The window in: asked on at once, from its end.
        let later = now + SEND_EVERY * 6;
        let chunks = host.resync(
            0,
            guest.epoch,
            Resync::Want {
                file: host_file(&host),
                from: 0,
            },
        );

        for (_, chunk) in chunks {
            guest.receive(addr(1), chunk, later);
        }

        assert_eq!(wants(guest.packets(later)), [transfer::WINDOW]);

        // A chunk lost: asked again, but only after a pause.
        assert_eq!(wants(guest.packets(later + SEND_EVERY)).len(), 0);
        assert_eq!(wants(guest.packets(later + ASK_AGAIN)), [transfer::WINDOW]);
    }

    /// The hash of the file `host` sends.
    fn host_file(host: &Node) -> String {
        match &host.sync {
            Sync::Sending { file, .. } => file.file.clone(),
            _ => String::new(),
        }
    }
}
