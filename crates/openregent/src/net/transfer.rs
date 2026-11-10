//! A file sent to a peer that lacks it, in chunks over UDP: a save the
//! host loads (`lobby`), or the host's game after a desync (`node`).
//! Free of sockets: the lobby carries the chunks and the asks, and the
//! game's link alike.
//!
//! ```text
//!  sender                                  receiver
//!                <── want(file, from) ──   the first chunk it lacks,
//!                                          asked again once a window is
//!                                          in or after a pause
//!  chunks from..from + WINDOW ──>          each kept; a gap asked again
//!  ...                                     all in: the bytes hashed, as
//!                                          offered, or refused
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

/// A chunk's bytes at most: with its header, under the lobby's largest
/// datagram (`wire::MOST_BYTES`).
pub const CHUNK: usize = 4_096;

/// Chunks sent at most for one ask.
pub const WINDOW: u32 = 32;

/// A chunk of a file: the file's hash, its place, its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub file: String,
    pub at: u32,
    pub bytes: Vec<u8>,
}

/// A file being sent: its hash ([`crate::files::store::hash`]) and bytes.
#[derive(Clone, Debug)]
pub struct Outgoing {
    pub file: String,
    bytes: Arc<Vec<u8>>,
}

impl Outgoing {
    pub fn new(bytes: Vec<u8>) -> Outgoing {
        Outgoing {
            file: crate::files::store::hash([bytes.as_slice()]),
            bytes: Arc::new(bytes),
        }
    }

    /// Its length, bytes.
    pub fn size(&self) -> u32 {
        u32::try_from(self.bytes.len()).unwrap_or(u32::MAX)
    }

    /// The chunks from `from` on, at most [`WINDOW`].
    pub fn chunks(&self, from: u32) -> Vec<Chunk> {
        self.bytes
            .chunks(CHUNK)
            .zip(0..)
            .skip(from as usize)
            .take(WINDOW as usize)
            .map(|(bytes, at)| Chunk {
                file: self.file.clone(),
                at,
                bytes: bytes.to_vec(),
            })
            .collect()
    }
}

/// A file being received: its hash and length as offered, and the
/// chunks in.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub file: String,
    size: u32,
    chunks: BTreeMap<u32, Vec<u8>>,
}

impl Incoming {
    pub const fn new(file: String, size: u32) -> Incoming {
        Incoming {
            file,
            size,
            chunks: BTreeMap::new(),
        }
    }

    /// Its length as offered, bytes.
    pub const fn size(&self) -> u32 {
        self.size
    }

    /// Chunks the file is.
    fn count(&self) -> u32 {
        self.size.div_ceil(u32::try_from(CHUNK).unwrap_or(u32::MAX))
    }

    /// `chunk` kept, if of this file and in it.
    pub fn take(&mut self, chunk: Chunk) {
        if chunk.file == self.file && chunk.at < self.count() {
            self.chunks.entry(chunk.at).or_insert(chunk.bytes);
        }
    }

    /// The first chunk not in: what to ask from.
    pub fn from(&self) -> u32 {
        (0..self.count())
            .find(|at| !self.chunks.contains_key(at))
            .unwrap_or_else(|| self.count())
    }

    /// Chunks in, of all: for the lobby to show.
    pub fn progress(&self) -> (u32, u32) {
        (
            u32::try_from(self.chunks.len()).unwrap_or(u32::MAX),
            self.count(),
        )
    }

    /// The file, once every chunk is in: its bytes, or why not, where
    /// they are not what was offered.
    pub fn done(&self) -> Option<Result<Vec<u8>, String>> {
        (self.from() == self.count()).then(|| {
            let bytes: Vec<u8> = self.chunks.values().flatten().copied().collect();
            let hash = crate::files::store::hash([bytes.as_slice()]);

            if bytes.len() != self.size as usize {
                Err(format!("{} bytes came, {} offered", bytes.len(), self.size))
            } else if hash != self.file {
                Err(format!("the save came as {hash}, offered as {}", self.file))
            } else {
                Ok(bytes)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of three chunks and some, out of order and twice, one
    /// lost then asked again: whole once all are in.
    #[test]
    fn a_file_arrives_whole_in_any_order() {
        let bytes: Vec<u8> = (0..CHUNK * 3 + 7)
            .map(|n| u8::try_from(n % 251).unwrap_or_default())
            .collect();
        let sent = Outgoing::new(bytes.clone());
        let mut came = Incoming::new(sent.file.clone(), sent.size());
        let mut chunks = sent.chunks(0);

        assert_eq!(chunks.len(), 4);

        let lost = chunks.remove(1);

        for chunk in chunks.iter().rev().chain(&chunks) {
            came.take(chunk.clone());
        }

        assert_eq!((came.from(), came.done()), (1, None));

        came.take(lost);

        assert_eq!(came.done(), Some(Ok(bytes)));
        assert_eq!(came.progress(), (4, 4));
    }

    /// Bytes that are not those offered are refused, by their hash.
    #[test]
    fn a_file_not_as_offered_is_refused() {
        let sent = Outgoing::new(vec![1, 2, 3]);
        let mut came = Incoming::new(sent.file.clone(), sent.size());
        let mut chunk = sent.chunks(0).remove(0);

        chunk.bytes[1] = 9;
        came.take(chunk);

        assert!(
            came.done()
                .is_some_and(|d| d.is_err_and(|e| e.contains("offered as")))
        );
    }

    /// A window's chunks from where the receiver asks.
    #[test]
    fn chunks_go_a_window_at_a_time_from_the_ask() {
        let sent = Outgoing::new(vec![0; CHUNK * 40]);
        let ats = |from| sent.chunks(from).iter().map(|c| c.at).collect::<Vec<_>>();

        assert_eq!(ats(0), (0..WINDOW).collect::<Vec<_>>());
        assert_eq!(ats(35), [35, 36, 37, 38, 39]);
    }
}
