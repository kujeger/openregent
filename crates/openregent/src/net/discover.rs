//! LAN discovery: each host's lobby announces itself on the local
//! network, and a guest's Multiplayer Setup lists what it hears. Our own
//! protocol, as the rest of `net`; the original lists `DirectPlay`'s
//! sessions instead.
//!
//! ```text
//!  host (Beacon)                              guest (Finder, on DISCOVERY_PORT)
//!  Announce(id, port, title, nick, seats, ──> every guest on a machine
//!    map, build) to each interface's          hears it: the port is
//!    broadcast address and this machine,      shared
//!    every ANNOUNCE_EVERY
//!  a game unheard GONE_AFTER leaves the list; one heard twice (two
//!  networks, or this machine and a network) is one game, by its id
//! ```
//!
//! Announced, not asked: a guest's firewall need only let in
//! [`DISCOVERY_PORT`], where an open to a broadcast query would come
//! to a port of the moment. Nothing leaves the LAN: a broadcast is not
//! routed, and the packets live one hop ([`HOPS`]). IPv4 only: no IPv6
//! multicast.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use bevy::log::{debug, warn};
use socket2::{Domain, Protocol, Socket, Type};

use super::lobby::mismatch;
use super::wire::{Build, Open, Packet};

/// The port every guest's finder shares: [`super::PORT`]'s neighbour.
pub const DISCOVERY_PORT: u16 = 47_100;

/// A host announces its lobby this often.
const ANNOUNCE_EVERY: Duration = Duration::from_secs(1);

/// A game unheard this long leaves the list: three announcements missed
/// (**Inferred**: a bound picked, not measured).
const GONE_AFTER: Duration = Duration::from_secs(3);

/// Routers a discovery packet may cross: none.
const HOPS: u32 = 1;

/// The largest datagram read: an announcement is well under it.
const MOST_BYTES: usize = 2 * 1024;

/// Packets read at most in one poll.
const MOST_READS: usize = 64;

/// Whether a socket's port is its own, or shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Share {
    Own,
    Port,
}

/// A UDP socket on `port` of every interface, 0 picking one, not
/// blocking, its packets kept on the LAN; with `Share::Port`, the port
/// shared with other sockets so: every one hears a broadcast.
fn socket(port: u16, share: Share) -> Result<UdpSocket, String> {
    let said = |e: std::io::Error| format!("discovery port {port}: {e}");
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).map_err(said)?;

    if share == Share::Port {
        socket.set_reuse_address(true).map_err(said)?;

        // BSD and macOS share a port for broadcasts by this one.
        #[cfg(all(
            unix,
            not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))
        ))]
        socket.set_reuse_port(true).map_err(said)?;
    }

    socket.set_broadcast(true).map_err(said)?;
    socket.set_ttl_v4(HOPS).map_err(said)?;
    socket.set_nonblocking(true).map_err(said)?;
    socket
        .bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())
        .map_err(said)?;

    Ok(socket.into())
}

/// Where a host's announcements go.
#[derive(Clone, Debug)]
pub enum Targets {
    /// Every interface's broadcast address and this machine, at
    /// [`DISCOVERY_PORT`].
    Broadcast,
    /// These alone: a test's, none by default.
    Given(Vec<SocketAddr>),
}

/// A host's voice: it announces its lobby.
pub struct Beacon {
    socket: UdpSocket,
    targets: Targets,
    /// The lobby's, picked as it opens.
    id: u64,
    said: Option<Instant>,
}

impl Beacon {
    /// A beacon announcing to `targets`.
    ///
    /// # Errors
    ///
    /// A socket that will not bind.
    pub fn new(targets: Targets) -> Result<Beacon, String> {
        let socket = socket(0, Share::Own)?;
        let port = socket.local_addr().map_or(0, |a| a.port());

        Ok(Beacon {
            socket,
            targets,
            id: RandomState::new().hash_one(port),
            said: None,
        })
    }

    /// The lobby's id its announcements carry: a test's.
    #[cfg(test)]
    pub const fn id(&self) -> u64 {
        self.id
    }

    /// At `now`, the lobby `open` makes announced, if due.
    pub fn poll(&mut self, now: Instant, open: impl FnOnce(u64) -> Open) {
        let due = self
            .said
            .is_none_or(|at| now.saturating_duration_since(at) >= ANNOUNCE_EVERY);

        if due {
            let bytes = Packet::Open(open(self.id)).encode();
            let targets = match &self.targets {
                Targets::Broadcast => broadcasts(),
                Targets::Given(given) => given.clone(),
            };

            for to in targets {
                if let Err(e) = self.socket.send_to(&bytes, to) {
                    debug!("net: announce to {to}: {e}");
                }
            }

            self.said = Some(now);
        }
    }
}

/// A game found: where its lobby is, and what it said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Its lobby's address: the announcement's sender, at its port.
    pub host: SocketAddr,
    pub open: Open,
    heard: Instant,
}

impl Found {
    /// Why this machine's `ours` may not join it; `None` if it may.
    #[must_use]
    pub fn misfit(&self, ours: &Build) -> Option<String> {
        mismatch(ours, &self.open.build, self.open.builds)
    }

    /// Whether every seat is taken.
    #[must_use]
    pub const fn full(&self) -> bool {
        self.open.seated >= self.open.players
    }
}

/// A guest's ear for games, polled: it keeps the games announced, by
/// when first heard.
pub struct Finder {
    socket: UdpSocket,
    found: Vec<Found>,
}

impl Finder {
    /// A finder on [`DISCOVERY_PORT`], shared with every other on this
    /// machine.
    ///
    /// # Errors
    ///
    /// A port that will not bind, or not be shared.
    pub fn new() -> Result<Finder, String> {
        Finder::on(DISCOVERY_PORT)
    }

    /// A finder on `port`; 0 picks one, a test's.
    ///
    /// # Errors
    ///
    /// A port that will not bind, or not be shared.
    pub fn on(port: u16) -> Result<Finder, String> {
        Ok(Finder {
            socket: socket(port, Share::Port)?,
            found: Vec::new(),
        })
    }

    /// The port it listens on.
    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.socket.local_addr().map_or(0, |a| a.port())
    }

    /// The games heard, by when first heard.
    pub fn found(&self) -> &[Found] {
        &self.found
    }

    /// At `now`: each announcement taken, and a game unheard
    /// [`GONE_AFTER`] let go.
    pub fn poll(&mut self, now: Instant) {
        let mut buf = [0; MOST_BYTES];

        for _ in 0..MOST_READS {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => match Packet::decode(&buf[..n]) {
                    Ok(Packet::Open(open)) => self.heard(open, from, now),
                    Ok(_) => {}
                    Err(why) => debug!("net: {from}: {why}"),
                },
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => {}
            }
        }

        self.found
            .retain(|f| now.saturating_duration_since(f.heard) < GONE_AFTER);
    }

    /// `open` from `from` at `now`: a game found, or one known heard
    /// again, at the address first heard.
    fn heard(&mut self, open: Open, from: SocketAddr, now: Instant) {
        match self.found.iter_mut().find(|f| f.open.id == open.id) {
            Some(found) => {
                found.open = open;
                found.heard = now;
            }
            None => self.found.push(Found {
                host: SocketAddr::new(from.ip(), open.port),
                open,
                heard: now,
            }),
        }
    }
}

/// Where a host announces: each IPv4 interface's broadcast address, the
/// limited broadcast (the default route's network), and this machine,
/// with no network up; at [`DISCOVERY_PORT`].
fn broadcasts() -> Vec<SocketAddr> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_else(|e| {
        warn!("net: the interfaces: {e}");
        Vec::new()
    });
    let each = interfaces.into_iter().filter_map(|i| match i.addr {
        if_addrs::IfAddr::V4(v4) if !v4.is_loopback() => v4.broadcast,
        _ => None,
    });
    let mut all: Vec<Ipv4Addr> = each
        .chain([Ipv4Addr::BROADCAST, Ipv4Addr::LOCALHOST])
        .collect();

    all.sort_unstable();
    all.dedup();
    all.into_iter()
        .map(|ip| SocketAddr::V4(SocketAddrV4::new(ip, DISCOVERY_PORT)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_announces_to_the_limited_broadcast_and_this_machine() {
        let targets = broadcasts();
        let at = |ip| SocketAddr::V4(SocketAddrV4::new(ip, DISCOVERY_PORT));

        assert!(targets.contains(&at(Ipv4Addr::BROADCAST)));
        assert!(targets.contains(&at(Ipv4Addr::LOCALHOST)));
        assert_eq!(DISCOVERY_PORT + 1, super::super::PORT);
    }
}
