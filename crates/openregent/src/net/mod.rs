//! Network play: every peer runs the whole game. Orders use lockstep
//! over UDP; internet play carries its packets through iroh. The
//! original's protocol is not followed.
//!
//! ```text
//!  openregent --server [PORT] --players N     host, seat 0
//!  openregent --client HOST[:PORT]            guest, seat 1 up
//!
//!  openregent, Multiplayer                    either, from the menus
//!
//!  lobby   Hello (build, nick, ready, chat) ─> Lobby (title, setup, seats,
//!          chat) / Refused; every guest ready, the host starts (the
//!          command line's: every seat taken) ─> Welcome (seat, players,
//!          delay, the recording's setup, nicks)
//!  play    at step n a peer seals its bundle for step n + delay: its
//!          orders, its digest after step n, and a deep digest folded
//!          since (fold.rs: of a snapshot every DEEP_EVERY steps, on a
//!          worker); step n + 1 runs once every seat's bundle for it is in
//!  each step:  pump ─> seal ─> feed each seat's items, seat order ─>
//!              the frame ─> let go what every peer holds   (Sim::step)
//!
//!  a step is a frame, but where the game stands still at its end

//! ```
//!
//! A host's lobby announces itself to the local network each second
//! ([`Finder`], `discover`): Multiplayer Setup lists the games it hears.
//! The menus host or join a lobby through its client, [`Session`]: polled,
//! it never blocks, and comes to a game started ([`Started`]) or a failure.
//!
//! Each peer records every seat's orders, in one order, to its
//! `play.input`: any peer's replays the game. Pause is everyone's: a
//! peer paused (the key, the options menu) holds every peer. The host's
//! speed paces all. A digest that differs holds every peer, each
//! writing its snapshot beside its log (a deep digest's, the snapshot
//! it folded); the host sends its game over and every peer goes on from
//! it ([`Peer::turn`], `resync`). Another desync within [`RESYNC_GAP`]
//! frames stops the game. A peer silent `node::WAIT_AFTER` holds the
//! game: each peer shows whom it waits on ([`Peer::waiting`]), and the
//! host may drop it ([`Peer::drop_player`]). Back in time, play goes on;
//! silent `node::DROP_AFTER`, it is dropped, a leaving one at its
//! goodbye (`Bye`), as is one whose own game stopped; its kingdom plays
//! on, uncontrolled, and the others' chat says so ([`Left`]). A dropped player joining again by its nick
//! takes its seat back: the host resyncs every peer to its game, the
//! seat in play again ([`Notice::Rejoined`]).
//!
//! A surrender is an item of a bundle ([`Given::Surrender`]): every peer
//! ends the seat's game lost at that step, and its player watches. A
//! spectator's chat lines are for spectators ([`Audience`]).
//!
//! A SAVE GAME is an item of a bundle: every peer saves the game as that
//! step leaves it ([`Peer::take_saves`]). A LAN save loads in the lobby:
//! offered by its game's hash, sent to a guest that lacks it
//! (`transfer`), each seat in its saved seat ([`Hosting::choose`]).
//!
//! After a game, Return to Lobby takes a peer back to the lobby on its
//! socket ([`Peer::regroup`], `lobby`); the host's going back takes each
//! guest still in the game with it.
//!
//! `net` reads no save and makes no game; `game` owns a [`Peer`]. The save
//! the host loads comes read ([`Loaded`], `save::lan`), a guest's copies
//! are kept through [`Saves`], and orders cross as [`Given`].
//!
//! Every game runs so: single player is a game of one peer
//! ([`Peer::solo`]) over a loopback link, its orders a frame ahead. Its
//! own bundle is sealed as the step it is for begins: it never waits.

mod delay;
mod discover;
mod fold;
pub mod iroh;
mod link;
mod lobby;
mod node;
mod resync;
mod session;
#[cfg(test)]
mod tests;
mod transfer;
mod wire;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::log::{debug, error, info, warn};
use regent_core::data::text::{Text, printf};
use regent_core::save::Snapshot;
use regent_core::sim::freestyle::MOST_PLAYERS;
use regent_core::sim::game::{Order, SOLO_DELAY};

pub use delay::Delay;
pub use discover::{Finder, Found};
use fold::Folder;
use link::{Link, Loopback, Udp};
pub use lobby::{
    Builds, Hosting, Joining, Loaded, Lobby, NICK_MOST, PORT, PORT_TEXT, Saves, Unready, address,
    build, parse_address,
};
use node::{Back, Depth, Node, Role};
pub use resync::Payload;
pub use session::{Phase, Polled, Service, Session, Started};
#[cfg(test)]
pub use wire::Profile;
pub use wire::{Build, Leave, Lines, Open, Ready, Room, Talk, Welcome};
use wire::{Bundle, Item, Packet};

/// Frames from an order's giving to the frame it comes before, unless
/// told: 150 ms, room for a LAN's round trip and a lost packet's resend.
pub const DELAY: u32 = 3;

/// The least delay: with one, each peer's next frame would wait on the
/// other's, which waits on its own.
pub const LEAST_DELAY: u32 = 2;

/// Frames between deep digests: on 512 cells a snapshot of about 1 ms
/// on the stepping thread, and its fold on a worker.
const DEEP_EVERY: u32 = 100;

/// A chat line's characters at most: it must fit a packet.
pub const CHAT_MOST: usize = 200;

/// `GMTX` 298 "%s quit the game." and 295 "%s was dropped.": a player's
/// goodbye, and its silence. HD formats 298 with a name in its lobby's
/// network handler; 295 has no such site found.
/// **Inferred**: both said as a player leaves.
pub const QUIT_TEXT: usize = 298;
pub const DROPPED_TEXT: usize = 295;

/// Either text's words without the data.
const LEFT_FALLBACK: &str = "%s left the game.";

/// Said after the host's leaving. A placeholder: the original ends the
/// game instead (`GMTX` 308 "The host cancelled out of the game - game
/// aborted!").
const ALONE: &str = "You play on alone.";

/// A player who left the game, for the others' chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Left {
    pub nick: String,
    pub how: Leave,
    pub who: Who,
}

/// Whose seat left: a guest's, or the host's, after which a guest plays
/// on alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Player,
    Host,
}

impl Left {
    /// As the chat says it, in the game's words where it has them:
    ///
    /// ```text
    ///  Ann quit the game.            Ann was dropped.
    ///  Host quit the game. You play on alone.
    /// ```
    #[must_use]
    pub fn line(&self, text: &Text<'_>) -> String {
        let at = match self.how {
            Leave::Quit => QUIT_TEXT,
            Leave::Silent => DROPPED_TEXT,
        };
        let format = text
            .gmtx(at)
            .map_or_else(|| LEFT_FALLBACK.to_owned(), |t| t.trim().to_owned());
        let said = printf(&format, &[self.nick.as_str().into()]);

        match self.who {
            Who::Player => said,
            Who::Host => format!("{said} {ALONE}"),
        }
    }
}

/// Frames after a resync within which another desync stops the game:
/// one that comes back so soon from the host's own state would come
/// back forever. A game minute, a dozen deep digests (**Inferred**: a
/// bound picked, not measured).
pub const RESYNC_GAP: u32 = 1_200;

/// `GMTX` 329 "The multiplayer game cannot continue for the following
/// reason:", before why a game stops. HD has no text of a desync or a
/// resync; the others are placeholders.
pub const CANNOT_CONTINUE_TEXT: usize = 329;

/// What the chat says of a desync, as a resync begins and ends.
const RESYNCING: &str = "Desync at frame %d: resyncing.";
const RESYNCED: &str = "Resynced.";

/// What the chat says of a player back in its seat: a placeholder, HD
/// has no such text.
const REJOINED: &str = "%s rejoined the game.";

/// What the host says to a hello in play that takes no seat.
const BEGUN: &str = "the game has begun";

/// A resync, for the chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A desync found at this frame: every peer holds while the host's
    /// game is sent.
    Resyncing(u32),
    /// Every peer goes on from the host's game: the host's once every
    /// guest has, a guest's once it has.
    Resynced,
    /// The game stops, and why.
    Stopped(String),
    /// A dropped player is back in its seat, by nick.
    Rejoined(String),
}

impl Notice {
    /// As the chat says it:
    ///
    /// ```text
    ///  Desync at frame 1200: resyncing.        Resynced.
    ///  The multiplayer game cannot continue for the following reason: desync again ...
    /// ```
    #[must_use]
    pub fn line(&self, text: &Text<'_>) -> String {
        match self {
            Notice::Resyncing(frame) => RESYNCING.replace("%d", &frame.to_string()),
            Notice::Resynced => RESYNCED.to_owned(),
            Notice::Rejoined(nick) => printf(REJOINED, &[nick.as_str().into()]),
            Notice::Stopped(why) => {
                let lead = text
                    .gmtx(CANNOT_CONTINUE_TEXT)
                    .map(|t| t.trim().to_owned())
                    .unwrap_or_default();

                format!("{lead} {why}").trim().to_owned()
            }
        }
    }
}

/// A seat the game holds for: silent, or not yet heard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wait {
    pub seat: u32,
    pub nick: String,
    /// Since last heard.
    pub silent: Duration,
    /// Until it is dropped.
    pub left: Duration,
}

/// What the game does next for a resync ([`Peer::turn`]).
pub enum Turn {
    /// The host: send its game as it stands, the desync at this frame
    /// ([`Peer::offer`]), and go on from it.
    Offer(u32),
    /// The host's file whole, of this epoch: go on from it.
    Resume(u32, Vec<u8>),
    /// Stop, for this.
    Stop(String),
}

/// A peer gone back to the lobby with its game's socket: the host to
/// host again, its guests seated first; a guest to join its host again
/// (`lobby`'s [`Regroup::host`], [`Regroup::join`]).
pub struct Regroup {
    socket: std::net::UdpSocket,
    /// This seat's nick, as typed.
    nick: String,
    players: u32,
    /// Each guest's nick, by seat, on the host.
    nicks: Vec<String>,
    back: Back,
}

impl Regroup {
    /// The game's players: the lobby's seats.
    pub const fn players(&self) -> u32 {
        self.players
    }

    /// Whether this peer hosts the lobby.
    pub const fn hosts(&self) -> bool {
        matches!(self.back, Back::Host(_))
    }
}

/// What a seat gives for a step, as the game records it: an order, its
/// ended game played on, or its surrender.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Given {
    Order(Order),
    PlayOn,
    Surrender,
}

/// Who may read a chat line: every player, or the spectators alone. A
/// spectator's lines are for spectators; it reads every line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audience {
    All,
    Spectators,
}

/// Opens a line for [`Audience::Spectators`] as it crosses the wire: a
/// control character, which typed chat never holds.
const SPECTATORS_ONLY: char = '\u{1f}';

/// A save's name's characters at most, as it crosses the wire.
const SAVE_NAME_MOST: usize = 64;

/// One peer of a game: its lockstep state and link. Every game has one,
/// in its `Sim`: a LAN game's talks UDP, any other's is [`Peer::solo`].
pub struct Peer {
    node: Node,
    link: Box<dyn Link>,
    bridge: Option<Arc<iroh::Bridge>>,
    /// Where a desync's snapshot is written; `None`, nowhere.
    dumps: Option<PathBuf>,
    /// The desync said, once.
    told: bool,
    /// Steps taken: the frames the lockstep counts. A frame each, but
    /// for the steps an ended game stands still in (`Game::frame`).
    steps: u32,
    /// Each seat's nick, by seat: empty, none.
    nicks: Vec<String>,
    /// The deep digests' worker, from the first; never alone.
    folder: Option<Folder>,
    /// Snapshots folded, by frame, kept to write until every seat's
    /// deep digest of the frame is compared, or it is too old to be.
    held: BTreeMap<u32, Snapshot>,
    /// Saves this machine asked, for its next bundle.
    asked: Vec<String>,
    /// Saves the last step took: who asked, and the name.
    saves: Vec<(u32, String)>,
    /// The last pump's time.
    now: Instant,
    /// The host's resync turn, for the game.
    due: Option<Turn>,
    /// The step the last resync went on from.
    resynced: Option<u32>,
    /// What the chat says of a resync, not yet taken.
    notices: Vec<Notice>,
    /// The host's build and what it lets play: a rejoining guest's is
    /// checked as the lobby checks it. `None`, unchecked.
    lobby: Option<(wire::Build, Builds)>,
}

/// The longest delay a guest keeps, frames: 2.5 s, `--net-delay`'s
/// most.
pub const MOST_DELAY: u32 = 50;

/// Whether the guest's lockstep can keep `welcome`: the lockstep
/// indexes its seats by the seats and players the host sent; a delay
/// past [`MOST_DELAY`] would overflow its frames.
fn keepable(welcome: &Welcome) -> Result<(), String> {
    let seats = 1..=MOST_PLAYERS;
    let kept = seats.contains(&welcome.players)
        && welcome.seat < welcome.players
        && welcome.host < welcome.players
        && (LEAST_DELAY..=MOST_DELAY).contains(&welcome.delay);

    if kept {
        Ok(())
    } else {
        Err(format!(
            "the host's welcome: seat {} and host {} of {} players, delay {}",
            welcome.seat, welcome.host, welcome.players, welcome.delay
        ))
    }
}

/// A desync found: the snapshot to write, by frame.
pub struct Parted {
    /// A deep digest's: the snapshot it folded, its frame. A quick
    /// one's: none, the game's now is written.
    pub held: Option<(u32, Snapshot)>,
}

impl Peer {
    fn new(node: Node, link: Box<dyn Link>, dumps: Option<PathBuf>) -> Peer {
        Peer {
            node,
            link,
            bridge: None,
            dumps,
            told: false,
            steps: 0,
            nicks: Vec::new(),
            folder: None,
            held: BTreeMap::new(),
            asked: Vec::new(),
            saves: Vec::new(),
            now: Instant::now(),
            due: None,
            resynced: None,
            notices: Vec::new(),
            lobby: None,
        }
    }

    /// The same peer, its seats named `nicks`.
    fn named(mut self, nicks: Vec<String>) -> Peer {
        self.nicks = nicks;
        self
    }

    /// Keep the internet bridge through the game and its return to lobby.
    pub fn bridged(mut self, bridge: Arc<iroh::Bridge>) -> Peer {
        self.bridge = Some(bridge);
        self
    }

    /// The peer of a game of one: one seat, orders a frame ahead as the
    /// engine's single player gives them ([`SOLO_DELAY`]), a loopback
    /// link.
    pub fn solo() -> Peer {
        let node = Node::new((Role::Host, 0), (1, SOLO_DELAY), Vec::new(), Instant::now());

        Peer::new(node, Box::new(Loopback::default()), None)
    }

    /// Host a game of `players` on `port`, named `nick`: wait for the
    /// guests, then welcome them to `setup`. Desyncs are written to
    /// `dumps`. Blocks: the command line's.
    ///
    /// # Errors
    ///
    /// A port that will not bind, a socket that fails.
    pub fn host(
        port: u16,
        build: (&wire::Build, Builds),
        (players, delay): (u32, Delay),
        (setup, nick): (&crate::setup::Setup, &str),
        dumps: Option<PathBuf>,
    ) -> Result<Peer, String> {
        let seated = Lobby::bind(port)?.seat(build, (players, delay), (setup, nick))?;

        Peer::hosted(seated, dumps)
    }

    /// The host's peer once `hosting` has every seat taken: each guest
    /// welcomed; and the save the game goes on from, if one is loaded.
    ///
    /// # Errors
    ///
    /// A socket that fails.
    pub fn seated(
        hosting: Hosting,
        dumps: Option<PathBuf>,
    ) -> Result<(Peer, Option<PathBuf>), String> {
        let seated = hosting.seat();
        let save = seated.save.clone();

        Ok((Peer::hosted(seated, dumps)?, save))
    }

    fn hosted(seated: lobby::Seated, dumps: Option<PathBuf>) -> Result<Peer, String> {
        let guests = seated
            .guests
            .into_iter()
            .map(|(addr, seat, welcome)| (addr, seat, Some(welcome)))
            .collect();
        let node = Node::new(
            (Role::Host, seated.seat),
            (seated.players, seated.delay),
            guests,
            Instant::now(),
        )
        .steered(seated.mode);
        let link = Box::new(Udp::new(seated.socket)?);
        let mut peer = Peer::new(node, link, dumps).named(seated.nicks);

        peer.lobby = Some(seated.build);
        Ok(peer)
    }

    /// Join the host at `host`, named `nick`: the peer, and the host's
    /// welcome. Blocks: the command line's.
    ///
    /// # Errors
    ///
    /// A host that does not resolve, refuses, or does not answer.
    pub fn join(
        host: &str,
        build: (&wire::Build, &str),
        dumps: Option<PathBuf>,
    ) -> Result<(Peer, Welcome), String> {
        let addr = lobby::address(host)?;
        let (socket, welcome) = lobby::join(addr, build)?;

        Ok((Peer::guest(socket, addr, &welcome, dumps)?, welcome))
    }

    /// The guest's peer once `joining` is welcomed with `welcome`.
    ///
    /// # Errors
    ///
    /// A socket that fails, or a welcome the lockstep cannot keep.
    pub fn welcomed(
        joining: Joining,
        welcome: &Welcome,
        dumps: Option<PathBuf>,
    ) -> Result<Peer, String> {
        let host = joining.host();

        Peer::guest(joining.socket(), host, welcome, dumps)
    }

    fn guest(
        socket: std::net::UdpSocket,
        host: SocketAddr,
        welcome: &Welcome,
        dumps: Option<PathBuf>,
    ) -> Result<Peer, String> {
        keepable(welcome)?;

        let node = Node::new(
            (Role::Guest, welcome.seat),
            (welcome.players, welcome.delay),
            vec![(host, welcome.host, None)],
            Instant::now(),
        );
        let link = Box::new(Udp::new(socket)?);

        Ok(Peer::new(node, link, dumps).named(welcome.nicks.clone()))
    }

    pub const fn seat(&self) -> u32 {
        self.node.seat()
    }

    pub const fn delay(&self) -> u32 {
        self.node.delay()
    }

    /// The game's peers: 1 alone.
    pub const fn players(&self) -> u32 {
        self.node.players()
    }

    /// Seat `seat`'s nick; none, `p<seat>`.
    pub fn nick(&self, seat: u32) -> String {
        usize::try_from(seat)
            .ok()
            .and_then(|at| self.nicks.get(at))
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("p{seat}"))
    }

    /// Packets in and out at `now`; deep digests folded taken.
    pub fn pump(&mut self, now: Instant) {
        self.now = now;
        self.collect();

        while let Some((bytes, from)) = self.link.recv(now) {
            match Packet::decode(&bytes) {
                // A hello from no remote in the game: a dropped player?
                Ok(Packet::Hello(hello))
                    if self.node.role() == Role::Host && self.node.rejoins(from) =>
                {
                    for (to, reply) in self.rejoin(from, &hello, now) {
                        self.link.send(to, &reply);
                    }
                }
                Ok(packet) => {
                    for (to, reply) in self.node.receive(from, packet, now) {
                        self.link.send(to, &reply.encode());
                    }
                }
                Err(why) => debug!("net: {from}: {why}"),
            }
        }

        for (to, bytes) in self.node.packets(now) {
            self.link.send(to, &bytes);
        }

        self.release();
    }

    /// The host's answer to `hello` from `from`, no remote in the game:
    /// a guest of the host's build named as a dropped seat is welcomed
    /// back to it, from a new address or, dropped silent, its old
    /// ([`Node::rejoins`]), and every peer held for the resync that
    /// brings it in ([`Turn::Offer`]); while another runs, asked to
    /// wait. Any other is refused.
    fn rejoin(
        &mut self,
        from: SocketAddr,
        hello: &wire::Hello,
        now: Instant,
    ) -> Vec<(SocketAddr, Vec<u8>)> {
        let why = self
            .lobby
            .as_ref()
            .and_then(|(ours, builds)| lobby::mismatch(ours, &hello.build, *builds));
        let seat = (0..self.players()).find(|&s| {
            self.node.gone(s)
                && !hello.nick.is_empty()
                && self.nicks.get(s as usize) == Some(&hello.nick)
        });
        let refused = |why: &str| vec![(from, Packet::Refused(why.to_owned()).encode())];

        if let Some(why) = self
            .lobby
            .as_ref()
            .and_then(|(ours, _)| lobby::data_warning(ours, &hello.build))
        {
            warn!("net: {from}: {why}");
        }

        match (why, seat) {
            (Some(why), _) => refused(&why),
            (None, Some(seat)) => match self.node.rejoin(from, seat, now) {
                Some(out) => {
                    info!("net: {} rejoins as player {seat}", hello.nick);
                    self.due = Some(Turn::Offer(self.steps));
                    out
                }
                None => vec![(from, Packet::Wait.encode())],
            },
            (None, None) => refused(BEGUN),
        }
    }

    /// The seats the game holds for, as of the last pump: on the host,
    /// its guests silent past `node::WAIT_AFTER`, each dropped at
    /// `node::DROP_AFTER`; on a guest, its host so, else those the host
    /// waits on.
    pub fn waiting(&self) -> Vec<Wait> {
        self.node
            .waiting(self.now)
            .into_iter()
            .map(|(seat, silent, left)| Wait {
                seat,
                nick: self.nick(seat),
                silent,
                left,
            })
            .collect()
    }

    /// The host drops seat `seat`, waited on or not; its kingdom plays
    /// on uncontrolled. A guest's does nothing.
    pub fn drop_player(&mut self, seat: u32) {
        self.node.toss(seat);
    }

    /// Whether this peer hosts the game.
    pub fn hosts(&self) -> bool {
        self.node.role() == Role::Host
    }

    /// Whether the next step's [`Peer::exchange`] wants a snapshot: with
    /// peers, every [`DEEP_EVERY`] steps.
    pub const fn deep_due(&self) -> bool {
        self.node.players() > 1 && self.steps.is_multiple_of(DEEP_EVERY)
    }

    /// The worker's folds done: each digest for the next bundles, each
    /// snapshot held.
    fn collect(&mut self) {
        let done = self.folder.as_ref().map(Folder::take).unwrap_or_default();

        for folded in done {
            self.node.folded(folded.frame, folded.deep);
            self.held.insert(folded.frame, folded.snapshot);
        }
    }

    /// The snapshots every seat's deep digest was compared against let
    /// go, and those too old to be: a peer that sends none would keep
    /// each; after a desync, kept.
    fn release(&mut self) {
        if self.node.desync().is_none() {
            let node = &self.node;
            let oldest = self.steps.saturating_sub(node::KEPT_DIGESTS);

            self.held
                .retain(|frame, _| *frame >= oldest && !node.compared(*frame));
        }
    }

    /// This machine's pause and speed, which the others hear.
    pub fn set_local(&mut self, paused: bool, speed: f32) {
        self.node.set_local(paused, speed);
    }

    /// The seats in play that hold the game paused, in order.
    pub fn pausers(&self) -> Vec<u32> {
        self.node.pausers()
    }

    /// The round trip from this peer to each other seat in play, where
    /// known ([`Node::pings`]).
    pub fn pings(&self) -> Vec<(u32, std::time::Duration)> {
        self.node.pings(self.now)
    }

    /// Whether seat `seat` left the game.
    pub fn gone(&self, seat: u32) -> bool {
        self.node.gone(seat)
    }

    /// Whether any peer holds the game paused, and no save waits: a save
    /// asked steps every peer on to its frame, at most the delay.
    pub fn paused(&self) -> bool {
        self.node.paused() && self.asked.is_empty() && !self.node.save_ahead(self.steps)
    }

    /// A save named `name` asked: every peer saves the game as the step
    /// the delay ahead leaves it ([`Peer::take_saves`]).
    pub fn ask_save(&mut self, name: &str) {
        self.asked.push(name.chars().take(SAVE_NAME_MOST).collect());
    }

    /// The saves the last step took, in seat order: who asked, the name.
    pub fn take_saves(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.saves)
    }

    /// The host's speed, everyone's.
    pub const fn speed(&self) -> f32 {
        self.node.speed()
    }

    /// Whether the next step may run: every seat's orders for it are in.
    pub fn ready(&self) -> bool {
        self.node.ready(self.steps + 1)
    }

    /// The next step: this machine's `sent` sealed for the step the
    /// delay ahead, with the `quick` digest and a deep one folded since;
    /// the `snapshot` taken where [`Peer::deep_due`], to fold; what each
    /// seat gave for the next step back, by seat, in seat order. Only
    /// once [`Peer::ready`].
    pub fn exchange(
        &mut self,
        sent: Vec<Given>,
        quick: impl FnOnce() -> u64,
        snapshot: Option<Snapshot>,
    ) -> Vec<(u32, Given)> {
        let ran = self.steps;
        let next = ran + 1;
        let target = ran + self.node.delay();
        // After a lowering some steps seal nothing: the orders wait.
        let seals = self.node.seals(ran);

        if let Some(snapshot) = snapshot {
            self.folder
                .get_or_insert_with(Folder::new)
                .fold(ran, snapshot);
        }

        self.collect();

        if seals {
            let items = sent
                .into_iter()
                .map(|given| match given {
                    Given::Order(order) => Item::Order(order),
                    Given::PlayOn => Item::PlayOn,
                    Given::Surrender => Item::Surrender,
                })
                .chain(self.asked.drain(..).map(Item::Save))
                .chain(self.node.announce(self.now).map(Item::Delay))
                .collect();
            // The quick digest only where a peer compares it: alone, 0.
            let shared = self.node.players() > 1;

            self.node.seal(
                target,
                Bundle {
                    items,
                    step: ran,
                    digest: shared.then(quick).unwrap_or_default(),
                    deep: None,
                },
            );
        }

        self.release();

        // A save is not given: the game is not told, the step's end saved.
        let mut given = Vec::new();

        for (seat, items) in self.node.items(next) {
            for item in items {
                match item {
                    Item::Order(order) => given.push((seat, Given::Order(order))),
                    Item::PlayOn => given.push((seat, Given::PlayOn)),
                    Item::Surrender => given.push((seat, Given::Surrender)),
                    Item::Save(name) => self.saves.push((seat, name)),
                    Item::Delay(frames) => self.node.change(ran, frames),
                }
            }
        }

        given
    }

    /// Seals refused for being out of order: a test's.
    #[cfg(test)]
    pub const fn misseals(&self) -> u32 {
        self.node.misseals()
    }

    /// Whether a delay change is in flight on the host: a test's.
    #[cfg(test)]
    pub const fn changing(&self) -> bool {
        self.node.changing()
    }

    /// Whether the next step seals a bundle: if not, the caller keeps its
    /// orders for the step that does ([`Node::seals`]).
    pub fn seals(&self) -> bool {
        self.node.seals(self.steps)
    }

    /// The step ran: what every peer holds of it is let go.
    pub fn fed(&mut self) {
        self.steps += 1;
        self.node.fed(self.steps);
    }

    /// A desync, said once: what it is, and a deep digest's snapshot.
    /// The host holds every peer and makes its file next
    /// ([`Peer::turn`]), or, the last resync too close, stops the game.
    pub fn desync(&mut self) -> Option<Parted> {
        let d = self.node.desync().filter(|_| !self.told)?;
        let depth = match d.kind {
            Depth::Quick => "digest",
            Depth::Deep => "deep digest",
        };
        let why = format!(
            "desync: frame {}'s {depth} is {:016x} here, {:016x} at player {}",
            d.frame, d.mine, d.theirs, d.seat
        );

        error!("net: {why}");
        self.told = true;

        if self.node.role() == Role::Host {
            self.verdict(d.frame);
        }

        let held = match d.kind {
            Depth::Quick => None,
            Depth::Deep => self.held.remove(&d.frame).map(|s| (d.frame, s)),
        };

        Some(Parted { held })
    }

    /// The host's turn after a desync at `frame`: a resync, unless the
    /// last went on from within [`RESYNC_GAP`] frames of it.
    fn verdict(&mut self, frame: u32) {
        let since = self.resynced.map(|at| frame.saturating_sub(at));

        if let Some(since) = since.filter(|s| *s <= RESYNC_GAP) {
            let why = format!("desync again at frame {frame}, {since} frames after a resync");

            error!("net: {why}: the game stops");
            self.node.give_up(why.clone());
            self.note(Notice::Stopped(why.clone()));
            self.due = Some(Turn::Stop(why));
        } else {
            for (to, bytes) in self.node.hold(self.now) {
                self.link.send(to, &bytes);
            }

            self.note(Notice::Resyncing(frame));
            self.due = Some(Turn::Offer(frame));
        }
    }

    /// What the game does next for a resync, once each: the host's
    /// offer or stop, a guest's file come, or the host's stop heard.
    pub fn turn(&mut self) -> Option<Turn> {
        self.due
            .take()
            .or_else(|| {
                self.node
                    .take_came()
                    .map(|(epoch, bytes)| Turn::Resume(epoch, bytes))
            })
            .or_else(|| {
                let why = self.node.take_stop()?;

                error!("net: the game stops: {why}");
                self.note(Notice::Stopped(why.clone()));
                Some(Turn::Stop(why))
            })
    }

    /// The host's file, `bytes`, offered to every guest; `frame` the
    /// desync's.
    pub fn offer(&mut self, frame: u32, bytes: Vec<u8>) {
        self.node.offer(frame, bytes);
    }

    /// The host's file not made: the game stops on every peer, for
    /// `why`.
    pub fn give_up(&mut self, why: &str) {
        error!("net: {why}: the game stops");
        self.node.give_up(why.to_owned());
        self.note(Notice::Stopped(why.to_owned()));
    }

    /// This machine's game stopped, not by the host's word: it leaves as
    /// one that quits, or every peer would wait on a seat heard but never
    /// stepping again.
    pub fn stopped(&mut self) {
        if !self.node.stopping() {
            for (to, bytes) in self.node.bye() {
                self.link.send(to, &bytes);
            }
        }
    }

    /// Steps taken ([`Peer::exchange`]).
    pub const fn steps(&self) -> u32 {
        self.steps
    }

    /// The resyncs begun or gone on from: the host's file's epoch.
    pub const fn epoch(&self) -> u32 {
        self.node.epoch()
    }

    /// The game went on from the file of `epoch` at step `steps`: the
    /// lockstep begun again there, folds and snapshots let go.
    pub fn resumed(&mut self, steps: u32, epoch: u32) {
        let back = self.node.resume(steps, epoch);

        self.steps = steps;
        self.folder = None;
        self.held.clear();
        self.told = false;

        // A rejoin's resync is no desync's: the next desync may resync.
        if back.is_none() {
            self.resynced = Some(steps);

            if self.node.role() == Role::Guest {
                self.note(Notice::Resynced);
            }
        }
    }

    /// What the chat says of a resync since the last call, in order.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.gather();
        std::mem::take(&mut self.notices)
    }

    /// `notice` for the chat, after the lockstep's own so far.
    fn note(&mut self, notice: Notice) {
        self.gather();
        self.notices.push(notice);
    }

    /// The lockstep's notices so far, and the seats back in play, by
    /// nick.
    fn gather(&mut self) {
        let back: Vec<Notice> = self
            .node
            .take_rejoined()
            .into_iter()
            .map(|seat| Notice::Rejoined(self.nick(seat)))
            .collect();

        self.notices.append(&mut self.node.take_notices());
        self.notices.extend(back);
    }

    /// Where a desync's files go; `None`, nowhere.
    pub fn dumps(&self) -> Option<&std::path::Path> {
        self.dumps.as_deref()
    }

    /// Whether a guest is being sent the host's file: a test's.
    #[cfg(test)]
    pub const fn taking(&self) -> bool {
        self.node.taking()
    }

    /// `snapshot`, of frame `frames`, as RON in [`Peer::dumps`].
    pub fn dump(&self, snapshot: Result<regent_core::save::Snapshot, String>, frames: u32) {
        let Some(dir) = &self.dumps else {
            return;
        };
        let path = dir.join(format!("desync-player{}-frame{frames}.ron", self.seat()));

        dump(&path, snapshot);
    }

    /// This machine's chat `line`, to every peer, for `to` to read.
    pub fn say(&mut self, line: &str, to: Audience) {
        let marked = match to {
            Audience::All => line.chars().take(CHAT_MOST).collect(),
            Audience::Spectators => std::iter::once(SPECTATORS_ONLY)
                .chain(line.chars().take(CHAT_MOST - 1))
                .collect(),
        };

        self.node.say(marked);
    }

    /// The other players' chat lines come since the last call, and whom
    /// each is for.
    pub fn take_chat(&mut self) -> Vec<(u32, String, Audience)> {
        self.node
            .take_chat()
            .into_iter()
            .map(|(seat, line)| match line.strip_prefix(SPECTATORS_ONLY) {
                Some(rest) => (seat, rest.to_owned(), Audience::Spectators),
                None => (seat, line, Audience::All),
            })
            .collect()
    }

    /// The players gone since the last call: a goodbye heard, or a
    /// silence. Alone, none.
    pub fn take_left(&mut self) -> Vec<Left> {
        self.node
            .take_left()
            .into_iter()
            .map(|(seat, how)| Left {
                nick: self.nick(seat),
                how,
                who: if seat == self.node.host() {
                    Who::Host
                } else {
                    Who::Player
                },
            })
            .collect()
    }

    /// Whether a run that stops here may leave: every bundle the others
    /// need to reach this step delivered, and theirs held.
    pub fn settled(&self) -> bool {
        self.node.settled(self.steps + self.node.delay() - 1)
    }

    /// Whether a guest's host is back in its lobby: the game is left
    /// for it ([`Peer::regroup`]).
    pub const fn regrouped(&self) -> bool {
        self.node.regrouped()
    }

    /// This peer back to the lobby, its socket kept: the host tells each
    /// guest in the game to come, a guest whose host plays on says
    /// goodbye. The game stands still after. `None` alone.
    pub fn regroup(&mut self) -> Option<Regroup> {
        let socket = self.link.socket()?;
        let (back, out) = self.node.regroup();

        for (to, bytes) in out {
            self.link.send(to, &bytes);
        }

        // The game's handle let go: the lobby's is the one left.
        self.link = Box::new(Loopback::default());

        let own = |seat: u32| self.nicks.get(seat as usize).cloned().unwrap_or_default();

        Some(Regroup {
            socket,
            nick: own(self.seat()),
            players: self.players(),
            nicks: (0..self.players()).map(own).collect(),
            back,
        })
    }

    /// Whether any seat was dropped: a test's.
    #[cfg(test)]
    pub fn dropped(&self) -> bool {
        self.node.dropped()
    }
}

/// A game left, by any way (QUIT, the menus, the window closed): the
/// others told at once, not after the silence.
impl Drop for Peer {
    fn drop(&mut self) {
        for (to, bytes) in self.node.bye() {
            self.link.send(to, &bytes);
        }
    }
}

/// `snapshot` as RON at `path`, said either way.
pub fn dump(path: &std::path::Path, snapshot: Result<regent_core::save::Snapshot, String>) {
    let written = snapshot
        .and_then(|s| ron::to_string(&s).map_err(|e| e.to_string()))
        .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));

    match written {
        Ok(()) => error!("snapshot in {}", path.display()),
        Err(e) => error!("no snapshot: {e}"),
    }
}
