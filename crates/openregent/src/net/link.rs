//! Where packets go: a UDP socket, a loopback for a game of one, or,
//! for the tests, a made-up network with latency, loss and reordering.
//!
//! UDP from `std::net`: a LAN game sends a few small packets each frame,
//! and the lockstep layer resends what is not acknowledged (`node`), so
//! no crate's reliability or ordering is needed.

use std::collections::VecDeque;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Mutex;
use std::time::Instant;

use bevy::log::debug;

use super::wire::MOST_BYTES;

/// Errors read past at most in one call before giving up: Linux reports
/// an unreachable peer on the next read.
const MOST_ERRORS: usize = 8;

/// A way to send and receive datagrams.
pub trait Link: Send + Sync {
    fn send(&self, to: SocketAddr, bytes: &[u8]);

    /// The next datagram arrived by `now`, and its sender; none waiting,
    /// `None`.
    fn recv(&self, now: Instant) -> Option<(Vec<u8>, SocketAddr)>;

    /// Its socket, another handle of it, for the lobby a game goes back
    /// to; `None` with no socket.
    fn socket(&self) -> Option<UdpSocket> {
        None
    }
}

/// A UDP socket, not blocking.
pub struct Udp(UdpSocket);

impl Udp {
    /// # Errors
    ///
    /// A socket that will not be made non-blocking.
    pub fn new(socket: UdpSocket) -> Result<Udp, String> {
        socket.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok(Udp(socket))
    }
}

impl Link for Udp {
    fn send(&self, to: SocketAddr, bytes: &[u8]) {
        if let Err(e) = self.0.send_to(bytes, to) {
            debug!("net: send to {to}: {e}");
        }
    }

    fn recv(&self, _: Instant) -> Option<(Vec<u8>, SocketAddr)> {
        let mut buf = [0; MOST_BYTES];

        (0..MOST_ERRORS).find_map(|_| match self.0.recv_from(&mut buf) {
            Ok((n, from)) => Some(Some((buf[..n].to_vec(), from))),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Some(None),
            Err(e) => {
                debug!("net: receive: {e}");
                None
            }
        })?
    }

    fn socket(&self) -> Option<UdpSocket> {
        self.0.try_clone().ok()
    }
}

/// A link to itself, no socket: a game of one peer sends nothing, as it
/// has no remote, but its peer is built as any other's.
#[derive(Default)]
pub struct Loopback(Mutex<VecDeque<(Vec<u8>, SocketAddr)>>);

impl Link for Loopback {
    fn send(&self, to: SocketAddr, bytes: &[u8]) {
        if let Ok(mut queue) = self.0.lock() {
            queue.push_back((bytes.to_vec(), to));
        }
    }

    fn recv(&self, _: Instant) -> Option<(Vec<u8>, SocketAddr)> {
        self.0.lock().ok()?.pop_front()
    }
}

#[cfg(test)]
pub use fake::{Fake, Weather};

#[cfg(test)]
mod fake {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;

    /// How the made-up network treats each datagram.
    #[derive(Clone, Copy, Debug)]
    pub struct Weather {
        /// The least time on the way.
        pub latency: Duration,
        /// Up to this more, drawn each datagram: later ones overtake.
        pub jitter: Duration,
        /// Datagrams lost in a hundred.
        pub loss: u32,
    }

    /// A datagram on its way.
    struct Flying {
        at: Instant,
        to: SocketAddr,
        from: SocketAddr,
        bytes: Vec<u8>,
    }

    /// The network all of a test's peers share.
    struct Air {
        weather: Weather,
        flying: Vec<Flying>,
        /// An LCG: losses and jitter alike every run.
        state: u32,
        /// Addresses cut off: nothing they send or are sent arrives.
        cut: Vec<SocketAddr>,
        /// The test's clock, which sends are stamped with.
        now: Instant,
    }

    impl Air {
        fn draw(&mut self, n: u32) -> u32 {
            self.state = self.state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (self.state >> 8) % n.max(1)
        }
    }

    /// One peer's end of the made-up network.
    #[derive(Clone)]
    pub struct Fake {
        air: Arc<Mutex<Air>>,
        pub addr: SocketAddr,
    }

    impl Fake {
        /// A network in `weather`, with `peers` ends, at `now`.
        pub fn network(weather: Weather, peers: u16, now: Instant) -> Vec<Fake> {
            let air = Arc::new(Mutex::new(Air {
                weather,
                flying: Vec::new(),
                state: 7,
                cut: Vec::new(),
                now,
            }));

            (0..peers)
                .map(|n| Fake {
                    air: air.clone(),
                    addr: SocketAddr::from(([10, 0, 0, 1], 1000 + n)),
                })
                .collect()
        }

        /// The test's clock moves to `now`.
        pub fn set_now(&self, now: Instant) {
            if let Ok(mut air) = self.air.lock() {
                air.now = now;
            }
        }

        /// The network's weather changes: datagrams sent from now on.
        pub fn set_weather(&self, weather: Weather) {
            if let Ok(mut air) = self.air.lock() {
                air.weather = weather;
            }
        }

        /// Nothing this end sends or is sent arrives from now on.
        pub fn cut(&self) {
            if let Ok(mut air) = self.air.lock() {
                air.cut.push(self.addr);
            }
        }

        /// This end cut off no more: what it sends and is sent from now
        /// on arrives.
        pub fn mend(&self) {
            if let Ok(mut air) = self.air.lock() {
                air.cut.retain(|a| *a != self.addr);
            }
        }

        /// Another end of the same network, at `port`: a machine come
        /// later.
        pub fn end(&self, port: u16) -> Fake {
            Fake {
                air: self.air.clone(),
                addr: SocketAddr::from(([10, 0, 0, 1], port)),
            }
        }
    }

    impl Link for Fake {
        fn send(&self, to: SocketAddr, bytes: &[u8]) {
            if let Ok(mut air) = self.air.lock() {
                let weather = air.weather;
                let lost = air.draw(100) < weather.loss;
                let jitter = u32::try_from(weather.jitter.as_micros()).unwrap_or(u32::MAX);
                let late = Duration::from_micros(air.draw(jitter).into());
                let cut = air.cut.contains(&to) || air.cut.contains(&self.addr);

                if !lost && !cut {
                    let at = air.now + weather.latency + late;

                    air.flying.push(Flying {
                        at,
                        to,
                        from: self.addr,
                        bytes: bytes.to_vec(),
                    });
                }
            }
        }

        fn recv(&self, now: Instant) -> Option<(Vec<u8>, SocketAddr)> {
            let mut air = self.air.lock().ok()?;
            let next = air
                .flying
                .iter()
                .enumerate()
                .filter(|(_, f)| f.to == self.addr && f.at <= now)
                .min_by_key(|(_, f)| f.at)
                .map(|(n, _)| n)?;
            let flying = air.flying.remove(next);

            Some((flying.bytes, flying.from))
        }
    }
}
