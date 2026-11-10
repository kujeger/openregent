//! Internet packets carried between local UDP sockets and iroh peers.
//! The local socket keeps the lobby and lockstep protocol shared with LAN.
//!
//! ```text
//!  guest game ──UDP── guest proxy ══iroh══ host proxy (per guest id) ──UDP── host game
//!   127.0.0.1         (Bridge::join)        127.0.0.1:p, kept for the id       lobby socket
//! ```
//!
//! The host's game tells guests apart by address, as on a LAN: each guest
//! has a loopback proxy socket of its own, kept by its iroh endpoint id.
//! A guest that reconnects (same key, [`IDENTITY`]) gets the same socket
//! back, so the game still knows it; the old connection's relay stops.
//! Small packets go as datagrams, larger ones each on a stream, sent off
//! the relay loop. The guest dials again after a lost connection, and
//! stops once the host closes it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::net::{SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ::iroh::endpoint::{Connection, ConnectionError, presets};
use ::iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use bevy::log::warn;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use tokio::task::AbortHandle;

use super::wire::{self, MOST_BYTES, Packet};

/// The protocol iroh connections speak, `openregent/<wire::VERSION>`: a
/// peer of another wire version does not connect.
const ALPN: &[u8] = &alpn();
const ALPN_PREFIX: &[u8] = b"openregent/";

/// [`ALPN`]: its prefix and the version's one digit.
const fn alpn() -> [u8; ALPN_PREFIX.len() + 1] {
    const { assert!(wire::VERSION < 10, "the version is one digit") };

    let mut out = [0; ALPN_PREFIX.len() + 1];
    let mut at = 0;

    while at < ALPN_PREFIX.len() {
        out[at] = ALPN_PREFIX[at];
        at += 1;
    }

    out[at] = b'0' + wire::VERSION;
    out
}

const STATUS_EVERY: Duration = Duration::from_millis(250);
const RETRY_EVERY: Duration = Duration::from_secs(1);
/// How long a host's endpoint forwards after it is let go, before it
/// closes: the lobby's last packets, its guests told it left, get out.
const FLUSH: Duration = Duration::from_millis(250);
/// Stream sends waiting at most, per connection.
const STREAMS_QUEUED: usize = 64;
type Owners = Arc<Mutex<HashMap<String, EndpointId>>>;
type Owner = (EndpointId, Owners);
type Healths = Arc<Mutex<HashMap<SocketAddr, Health>>>;
type Ids = Arc<Mutex<HashMap<SocketAddr, EndpointId>>>;

/// This player's iroh key, in openregent's data directory beside the
/// profiles: one identity across joins, so a guest that rejoins owns
/// the nick it had.
const IDENTITY: &str = "iroh.toml";

/// [`IDENTITY`] as written: `version = 1`, `secret = "<64 hex digits>"`.
#[derive(Serialize, Deserialize)]
struct Identity {
    version: u32,
    secret: String,
}

/// Where [`IDENTITY`] is kept: openregent's data directory.
pub fn home() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(crate::files::env::OWN))
}

/// The key kept in `dir`, made and written there the first time.
fn identity(dir: &Path) -> Result<SecretKey, String> {
    let path = dir.join(IDENTITY);

    if path.exists() {
        let file: Identity = crate::files::store::read(&path)?;

        crate::files::store::checked(file.version, &path)?;
        unhex(&file.secret)
            .map(|bytes| SecretKey::from_bytes(&bytes))
            .ok_or_else(|| format!("{}: not a key", path.display()))
    } else {
        let key = SecretKey::generate();
        let file = Identity {
            version: crate::files::store::VERSION,
            secret: key.to_bytes().iter().fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            }),
        };

        crate::files::store::make_dir(dir)?;
        crate::files::store::write(&path, &file)?;
        Ok(key)
    }
}

/// The key kept in `home`; with no data directory, one for this run.
fn key_of(home: Option<&Path>) -> Result<SecretKey, String> {
    home.map_or_else(
        || {
            warn!("net: no data directory: a new iroh identity for this run");
            Ok(SecretKey::generate())
        },
        identity,
    )
}

/// A key's 64 hex digits read back.
fn unhex(text: &str) -> Option<[u8; 32]> {
    let digits = text.trim().as_bytes();
    let mut key = [0; 32];

    (digits.len() == key.len() * 2).then_some(())?;

    for (byte, pair) in key.iter_mut().zip(digits.chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }

    Some(key)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Direct,
    Relayed,
}

#[derive(Clone, Copy, Default)]
pub struct Health {
    pub ping_ms: Option<u32>,
    pub route: Option<Route>,
    pub connected: bool,
    pub unstable: bool,
}

/// The internet endpoint and its local UDP bridge.
pub struct Bridge {
    stop: Mutex<Option<oneshot::Sender<()>>>,
    health: Arc<Mutex<HashMap<SocketAddr, Health>>>,
    ids: Arc<Mutex<HashMap<SocketAddr, EndpointId>>>,
    owners: Owners,
    error: Arc<Mutex<Option<String>>>,
    invite: Arc<Mutex<Option<String>>>,
    local: Option<SocketAddr>,
}

impl Drop for Bridge {
    fn drop(&mut self) {
        if let Ok(mut stop) = self.stop.lock()
            && let Some(stop) = stop.take()
        {
            let _ = stop.send(());
        }
    }
}

impl Bridge {
    /// Check an invite before enabling Join.
    pub fn check_invite(invite: &str) -> Result<(), String> {
        EndpointId::from_str(invite.trim())
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// An endpoint for guests to dial; `game` is the local lobby socket,
    /// the key kept in `home` ([`home`]). Bound aside: its invite once
    /// ready ([`Bridge::invite`]), or why not ([`Bridge::error`]).
    pub fn host(game: SocketAddr, home: Option<PathBuf>) -> Arc<Bridge> {
        let (stop_tx, stop_rx) = oneshot::channel();
        let health = Arc::new(Mutex::new(HashMap::new()));
        let watched = health.clone();
        let ids = Arc::new(Mutex::new(HashMap::new()));
        let owners = Arc::new(Mutex::new(HashMap::new()));
        let error = Arc::new(Mutex::new(None));
        let invite = Arc::new(Mutex::new(None));
        let known = ids.clone();
        let protected = owners.clone();
        let failed = error.clone();
        let ready = invite.clone();

        std::thread::spawn(move || {
            let result = runtime().and_then(|rt| {
                rt.block_on(async move {
                    let ep = endpoint(key_of(home.as_deref())?).await?;

                    if let Ok(mut ready) = ready.lock() {
                        *ready = Some(ep.id().to_string());
                    }

                    serve(ep, game, watched, known, protected, stop_rx).await;
                    Ok(())
                })
            });

            if let Err(why) = result
                && let Ok(mut error) = failed.lock()
            {
                *error = Some(why);
            }
        });

        Arc::new(Bridge {
            stop: Mutex::new(Some(stop_tx)),
            health,
            ids,
            owners,
            error,
            invite,
            local: None,
        })
    }

    /// A local UDP port for the existing guest lobby to join, the key
    /// kept in `home` ([`home`]).
    pub fn join(invite: &str, home: Option<PathBuf>) -> Result<Arc<Bridge>, String> {
        let id = EndpointId::from_str(invite.trim()).map_err(|e| e.to_string())?;
        let socket = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let local = socket.local_addr().map_err(|e| e.to_string())?;
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        let (stop_tx, stop_rx) = oneshot::channel();
        let health = Arc::new(Mutex::new(HashMap::new()));
        let watched = health.clone();
        let error = Arc::new(Mutex::new(None));
        let failed = error.clone();
        let lost = error.clone();

        std::thread::spawn(move || {
            let result = runtime().and_then(|rt| {
                rt.block_on(async move {
                    let ep = endpoint(key_of(home.as_deref())?).await?;
                    dial(ep, id.into(), socket, local, (watched, lost), stop_rx).await;
                    Ok(())
                })
            });

            if let Err(why) = result
                && let Ok(mut error) = failed.lock()
            {
                *error = Some(why);
            }
        });

        Ok(Arc::new(Bridge {
            stop: Mutex::new(Some(stop_tx)),
            health,
            ids: Arc::new(Mutex::new(HashMap::new())),
            owners: Arc::new(Mutex::new(HashMap::new())),
            error,
            invite: Arc::new(Mutex::new(None)),
            local: Some(local),
        }))
    }

    /// The host's invite, once its endpoint is bound; none on a guest.
    pub fn invite(&self) -> Option<String> {
        self.invite.lock().ok()?.clone()
    }

    pub const fn local(&self) -> Option<SocketAddr> {
        self.local
    }

    pub fn health(&self, addr: SocketAddr) -> Health {
        self.health
            .lock()
            .ok()
            .and_then(|health| health.get(&addr).copied())
            .unwrap_or_default()
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().ok()?.clone()
    }

    /// Keep each seated guest's nickname with its authenticated iroh key:
    /// the given seats only, so a freed seat's nick is free, and an empty
    /// nick is no one's.
    pub fn protect(&self, guests: &[(SocketAddr, String)]) {
        if let (Ok(ids), Ok(mut owners)) = (self.ids.lock(), self.owners.lock()) {
            *owners = guests
                .iter()
                .filter(|(_, nick)| !nick.is_empty())
                .filter_map(|(addr, nick)| Some((nick.clone(), *ids.get(addr)?)))
                .collect();
        }
    }

    /// A bridge with no endpoint behind it: a test's.
    #[cfg(test)]
    pub(crate) fn idle() -> Bridge {
        Bridge {
            stop: Mutex::new(None),
            health: Arc::new(Mutex::new(HashMap::new())),
            ids: Arc::new(Mutex::new(HashMap::new())),
            owners: Arc::new(Mutex::new(HashMap::new())),
            error: Arc::new(Mutex::new(None)),
            invite: Arc::new(Mutex::new(None)),
            local: None,
        }
    }

    /// The guest at `addr` known as key `id`, as if it dialled: a test's.
    #[cfg(test)]
    pub(crate) fn known(&self, addr: SocketAddr, id: EndpointId) {
        self.ids.lock().expect("ids").insert(addr, id);
    }

    /// The key `nick` is kept for: a test's.
    #[cfg(test)]
    pub(crate) fn owner(&self, nick: &str) -> Option<EndpointId> {
        self.owners.lock().expect("owners").get(nick).copied()
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
}

async fn endpoint(key: SecretKey) -> Result<Endpoint, String> {
    Endpoint::builder(presets::N0)
        .secret_key(key)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| e.to_string())
}

async fn serve(
    ep: Endpoint,
    game: SocketAddr,
    health: Arc<Mutex<HashMap<SocketAddr, Health>>>,
    ids: Arc<Mutex<HashMap<SocketAddr, EndpointId>>>,
    owners: Owners,
    mut stop: oneshot::Receiver<()>,
) {
    let proxies = Proxies::default();

    loop {
        tokio::select! {
            _ = &mut stop => {
                // The lobby's last words forwarded before the close.
                tokio::time::sleep(FLUSH).await;
                break;
            }
            incoming = ep.accept() => {
                if let Some(incoming) = incoming {
                    let health = health.clone();
                    let ids = ids.clone();
                    let owners = owners.clone();
                    let proxies = proxies.clone();
                    tokio::spawn(async move {
                        if let Ok(conn) = incoming.await {
                            adopt(conn, game, (health, ids, owners), &proxies);
                        }
                    });
                }
            }
        }
    }
    ep.close().await;
}

/// Each guest's proxy socket by its iroh id, its address, and the task
/// relaying its connection.
type Proxies =
    Arc<Mutex<HashMap<EndpointId, (Arc<tokio::net::UdpSocket>, SocketAddr, AbortHandle)>>>;

/// `conn` relayed to `game` from its guest's proxy socket: the one it
/// had if it reconnects, its old connection's relay stopped, so the game
/// hears it at the address it had; else a new one.
fn adopt(
    conn: Connection,
    game: SocketAddr,
    (health, ids, owners): (Healths, Ids, Owners),
    proxies: &Proxies,
) {
    let id = conn.remote_id();

    if let Ok(mut proxies) = proxies.lock() {
        let socket = match proxies.remove(&id) {
            Some((socket, addr, old)) => {
                old.abort();
                Some((socket, addr))
            }
            None => proxy_socket().ok(),
        };

        if let Some((socket, addr)) = socket {
            if let Ok(mut known) = ids.lock() {
                known.insert(addr, id);
            }

            let task = tokio::spawn(proxy(conn, socket.clone(), game, addr, health, owners));

            proxies.insert(id, (socket, addr, task.abort_handle()));
        }
    }
}

/// A new proxy socket on loopback, and its address.
fn proxy_socket() -> std::io::Result<(Arc<tokio::net::UdpSocket>, SocketAddr)> {
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    let addr = socket.local_addr()?;

    socket.set_nonblocking(true)?;
    Ok((Arc::new(tokio::net::UdpSocket::from_std(socket)?), addr))
}

async fn dial(
    ep: Endpoint,
    host: EndpointAddr,
    socket: UdpSocket,
    local: SocketAddr,
    (health, error): (Healths, Arc<Mutex<Option<String>>>),
    mut stop: oneshot::Receiver<()>,
) {
    let socket = tokio::net::UdpSocket::from_std(socket).expect("nonblocking socket");
    let mut game = None;

    loop {
        tokio::select! {
            _ = &mut stop => break,
            connected = ep.connect(host.clone(), ALPN) => {
                if let Ok(conn) = connected {
                    tokio::select! {
                        _ = &mut stop => break,
                        () = relay(&conn, &socket, &mut game, local, &health, None) => {}
                    }

                    // The host closed it: gone, not lost for a while.
                    if let Some(ConnectionError::ApplicationClosed(_)) = conn.close_reason() {
                        if let Ok(mut error) = error.lock() {
                            *error = Some(super::lobby::HOST_LEFT.to_owned());
                        }
                        break;
                    }
                }
            }
        }
        if let Ok(mut status) = health.lock() {
            status.insert(local, Health::default());
        }
        tokio::select! {
            _ = &mut stop => break,
            () = tokio::time::sleep(RETRY_EVERY) => {}
        }
    }
    ep.close().await;
}

async fn proxy(
    conn: Connection,
    socket: Arc<tokio::net::UdpSocket>,
    game: SocketAddr,
    local: SocketAddr,
    health: Arc<Mutex<HashMap<SocketAddr, Health>>>,
    owners: Owners,
) {
    let mut target = Some(game);
    let id = conn.remote_id();
    relay(
        &conn,
        &socket,
        &mut target,
        local,
        &health,
        Some((id, owners)),
    )
    .await;
    if let Ok(mut status) = health.lock() {
        status.insert(local, Health::default());
    }
}

async fn relay(
    conn: &Connection,
    socket: &tokio::net::UdpSocket,
    target: &mut Option<SocketAddr>,
    local: SocketAddr,
    health: &Arc<Mutex<HashMap<SocketAddr, Health>>>,
    owner: Option<Owner>,
) {
    let mut packet = Box::new([0; MOST_BYTES]);
    let mut status = tokio::time::interval(STATUS_EVERY);
    let (large_tx, mut large_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let (stream_tx, stream_rx) = tokio::sync::mpsc::channel::<Bytes>(STREAMS_QUEUED);

    tokio::spawn(streams(conn.clone(), stream_rx));

    loop {
        tokio::select! {
            received = socket.recv_from(&mut packet[..]) => {
                if let Ok((len, from)) = received {
                    if target.is_none() {
                        *target = Some(from);
                    }
                    send(conn, &stream_tx, &packet[..len]);
                }
            }
            received = conn.read_datagram() => {
                match received {
                    Ok(bytes) => {
                        if let Some(to) = target.as_ref().filter(|_| admitted(&bytes, owner.as_ref())) {
                            let _ = socket.send_to(&bytes, *to).await;
                        }
                    }
                    Err(_) => break,
                }
            }
            incoming = conn.accept_uni() => {
                match incoming {
                    Ok(mut stream) => {
                        let sender = large_tx.clone();
                        tokio::spawn(async move {
                            if let Ok(bytes) = stream.read_to_end(MOST_BYTES).await {
                                let _ = sender.send(bytes).await;
                            }
                        });
                    }
                    Err(_) => break,
                }
            }
            Some(bytes) = large_rx.recv() => {
                if let Some(to) = target.as_ref().filter(|_| admitted(&bytes, owner.as_ref())) {
                    let _ = socket.send_to(&bytes, *to).await;
                }
            }
            _ = status.tick() => {
                let paths = conn.paths();
                let selected = paths.iter().find(::iroh::endpoint::Path::is_selected);
                let seen = Health {
                    ping_ms: selected.as_ref().map(|path| path.rtt().as_millis().try_into().unwrap_or(u32::MAX)),
                    route: selected.map(|path| if path.is_relay() { Route::Relayed } else { Route::Direct }),
                    connected: true,
                    unstable: false,
                };
                if let Ok(mut status) = health.lock() {
                    status.insert(local, seen);
                }
            }
        }
    }
}

fn admitted(bytes: &[u8], owner: Option<&Owner>) -> bool {
    match (Packet::decode(bytes), owner) {
        (Ok(Packet::Hello(hello)), Some((id, owners))) => owners
            .lock()
            .is_ok_and(|owners| owners.get(&hello.nick).is_none_or(|held| held == id)),
        _ => true,
    }
}

/// `bytes` sent as a datagram, or, too large for one, queued for
/// [`streams`]; a full queue drops them, as UDP would.
fn send(conn: &Connection, streams: &tokio::sync::mpsc::Sender<Bytes>, bytes: &[u8]) {
    let bytes = Bytes::copy_from_slice(bytes);

    if conn
        .max_datagram_size()
        .is_some_and(|max| bytes.len() <= max)
    {
        let _ = conn.send_datagram(bytes);
    } else {
        let _ = streams.try_send(bytes);
    }
}

/// Packets too large for a datagram, each on a stream of its own, in
/// the order queued, off the relay loop so a slow stream stalls nothing
/// else; until the relay drops its queue.
async fn streams(conn: Connection, mut queued: tokio::sync::mpsc::Receiver<Bytes>) {
    while let Some(bytes) = queued.recv().await {
        if let Ok(mut stream) = conn.open_uni().await {
            let _ = stream.write_all(&bytes).await;
            let _ = stream.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::wire::{Build, Hello, Holding, Lines, Profile, Ready};
    use ::iroh::endpoint::RelayMode;

    /// The protocol names the wire version.
    #[test]
    fn the_protocol_names_the_wire_version() {
        assert_eq!(ALPN, format!("openregent/{}", wire::VERSION).as_bytes());
    }

    #[test]
    fn invite_is_an_endpoint_id() {
        let id = SecretKey::from_bytes(&[1; 32]).public();

        assert!(Bridge::check_invite(&id.to_string()).is_ok());
        assert!(Bridge::check_invite("not an invite").is_err());
    }

    #[test]
    fn another_key_cannot_claim_a_seat() {
        let first = SecretKey::from_bytes(&[1; 32]).public();
        let other = SecretKey::from_bytes(&[2; 32]).public();
        let owners = Arc::new(Mutex::new(HashMap::from([("Ann".to_owned(), first)])));
        let hello = |nick: &str| {
            Packet::Hello(Hello {
                build: Build {
                    version: "1".to_owned(),
                    profile: Profile::Release,
                    commit: "test".to_owned(),
                    data: 0,
                },
                nick: nick.to_owned(),
                ready: Ready::No,
                heard: 0,
                said: Lines {
                    first: 0,
                    lines: Vec::new(),
                },
                holding: Holding::None,
            })
            .encode()
        };

        assert!(admitted(&hello("Ann"), Some(&(first, owners.clone()))));
        assert!(!admitted(&hello("Ann"), Some(&(other, owners.clone()))));
        assert!(admitted(&hello("Bob"), Some(&(other, owners))));
    }

    /// The key is made once and read back the same on every join.
    #[test]
    fn the_identity_is_kept_across_joins() {
        let scratch = crate::testdata::scratch("iroh-identity");
        let dir = scratch.path().join("identity");

        let made = identity(&dir).expect("made");
        let read = identity(&dir).expect("read back");
        let text = std::fs::read_to_string(dir.join(IDENTITY)).expect("written");

        assert_eq!(made.public(), read.public());
        assert!(text.contains("version = 1"), "{text}");
        assert_eq!(unhex("zz"), None);
    }

    /// Only this game's named seats are protected: an earlier game's
    /// nicks, and empty ones, block no one.
    #[test]
    fn protected_nicks_are_this_games_and_named() {
        let bridge = Bridge::idle();
        let (ann, bob) = (
            SocketAddr::from(([127, 0, 0, 1], 1)),
            SocketAddr::from(([127, 0, 0, 1], 2)),
        );
        let (first, other) = (
            SecretKey::from_bytes(&[1; 32]).public(),
            SecretKey::from_bytes(&[2; 32]).public(),
        );
        let owners = || bridge.owners.lock().expect("owners").clone();

        bridge
            .ids
            .lock()
            .expect("ids")
            .extend([(ann, first), (bob, other)]);
        bridge.protect(&[(ann, "Ann".to_owned()), (bob, String::new())]);
        assert_eq!(owners(), HashMap::from([("Ann".to_owned(), first)]));

        bridge.protect(&[(bob, "Bob".to_owned())]);
        assert_eq!(owners(), HashMap::from([("Bob".to_owned(), other)]));
    }

    /// An endpoint on this machine alone, no relay.
    async fn local(alpns: Vec<Vec<u8>>) -> Endpoint {
        Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .alpns(alpns)
            .bind()
            .await
            .expect("endpoint")
    }

    /// A guest that reconnects reaches the host's game from the address
    /// it had, and the game's packets to that address reach it again.
    #[tokio::test]
    async fn a_reconnected_guest_keeps_its_address() {
        let host = local(vec![ALPN.to_vec()]).await;
        let guest = local(Vec::new()).await;
        let port = host
            .bound_sockets()
            .into_iter()
            .find(SocketAddr::is_ipv4)
            .expect("IPv4 socket")
            .port();
        let addr =
            EndpointAddr::new(host.id()).with_ip_addr(SocketAddr::from(([127, 0, 0, 1], port)));
        let game = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("game");
        let at = game.local_addr().expect("game address");
        let (_stop, stop) = oneshot::channel();
        let served = tokio::spawn(serve(
            host,
            at,
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
            Arc::new(Mutex::new(HashMap::new())),
            stop,
        ));
        let mut seen = Vec::new();

        for _ in 0..2 {
            let conn =
                tokio::time::timeout(Duration::from_secs(10), guest.connect(addr.clone(), ALPN))
                    .await
                    .expect("in time")
                    .expect("connected");
            let mut buf = [0; MOST_BYTES];

            conn.send_datagram(Bytes::from_static(b"hi")).expect("sent");
            let (_, from) = tokio::time::timeout(Duration::from_secs(5), game.recv_from(&mut buf))
                .await
                .expect("arrived")
                .expect("received");

            game.send_to(b"back", from).await.expect("sent back");
            let back = tokio::time::timeout(Duration::from_secs(5), conn.read_datagram())
                .await
                .expect("came back")
                .expect("read");

            assert_eq!(&back[..], b"back");
            seen.push(from);
            conn.close(0u32.into(), b"lost");
        }

        assert_eq!(seen[0], seen[1]);
        served.abort();
        guest.close().await;
    }

    /// A guest whose host closes its endpoint says the host left, and
    /// stops dialling.
    #[tokio::test]
    async fn a_guest_hears_the_host_leave() {
        let host = local(vec![ALPN.to_vec()]).await;
        let guest = local(Vec::new()).await;
        let port = host
            .bound_sockets()
            .into_iter()
            .find(SocketAddr::is_ipv4)
            .expect("IPv4 socket")
            .port();
        let addr =
            EndpointAddr::new(host.id()).with_ip_addr(SocketAddr::from(([127, 0, 0, 1], port)));
        let socket = UdpSocket::bind("127.0.0.1:0").expect("guest socket");
        let at = socket.local_addr().expect("guest address");
        let error = Arc::new(Mutex::new(None));
        let (_stop, stop) = oneshot::channel();

        socket.set_nonblocking(true).expect("non-blocking");

        let dialled = tokio::spawn(dial(
            guest,
            addr,
            socket,
            at,
            (Arc::new(Mutex::new(HashMap::new())), error.clone()),
            stop,
        ));
        let accepted = tokio::time::timeout(Duration::from_secs(10), async {
            host.accept().await.expect("incoming").await
        })
        .await
        .expect("in time")
        .expect("accepted");

        host.close().await;
        tokio::time::timeout(Duration::from_secs(10), dialled)
            .await
            .expect("stopped dialling")
            .expect("dial ran");
        drop(accepted);

        assert_eq!(
            error.lock().expect("error").as_deref(),
            Some(super::super::lobby::HOST_LEFT)
        );
    }

    #[tokio::test]
    async fn packets_cross_a_direct_bridge() {
        let host = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await
            .expect("host endpoint");
        let guest = Endpoint::builder(presets::Minimal)
            .relay_mode(RelayMode::Disabled)
            .bind()
            .await
            .expect("guest endpoint");
        let port = host
            .bound_sockets()
            .into_iter()
            .find(SocketAddr::is_ipv4)
            .expect("IPv4 socket")
            .port();
        let addr =
            EndpointAddr::new(host.id()).with_ip_addr(SocketAddr::from(([127, 0, 0, 1], port)));
        let (dialed, accepted) = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(guest.connect(addr, ALPN), async {
                host.accept().await.expect("incoming").await
            })
        })
        .await
        .expect("local connection");
        let outgoing = dialed.expect("guest connection");
        let incoming = accepted.expect("host connection");

        let host_game = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("host game");
        let guest_game = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("guest game");
        let host_proxy = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("host proxy");
        let guest_proxy = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("guest proxy");
        let host_proxy_addr = host_proxy.local_addr().expect("host address");
        let host_game_addr = host_game.local_addr().expect("host game address");
        let guest_game_addr = guest_game.local_addr().expect("guest game address");
        let status = Arc::new(Mutex::new(HashMap::new()));
        let host_status = status.clone();
        let guest_status = status.clone();
        let sent = tokio::spawn(async move {
            relay(
                &incoming,
                &host_proxy,
                &mut Some(host_game_addr),
                host_proxy_addr,
                &host_status,
                None,
            )
            .await;
        });
        let received = tokio::spawn(async move {
            relay(
                &outgoing,
                &guest_proxy,
                &mut Some(guest_game_addr),
                guest_game_addr,
                &guest_status,
                None,
            )
            .await;
        });

        for size in [16, 4_096] {
            let packet = vec![0x5a; size];
            host_game
                .send_to(&packet, host_proxy_addr)
                .await
                .expect("send");
            let mut buf = [0; MOST_BYTES];
            let (len, _) =
                tokio::time::timeout(Duration::from_secs(5), guest_game.recv_from(&mut buf))
                    .await
                    .expect("packet arrived")
                    .expect("receive");
            assert_eq!(&buf[..len], packet);
        }

        sent.abort();
        received.abort();
        host.close().await;
        guest.close().await;
    }
}
