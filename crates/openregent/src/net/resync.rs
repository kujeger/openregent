//! The file a desync's resync sends: the host's game at the step it
//! holds, and its recording, which every peer's goes on from.
//!
//! ```text
//!  Payload (steps, the desync's frame, snapshot, play.input) ─> RON ─> zstd
//! ```

use serde::{Deserialize, Serialize};

/// zstd's level: its default, as a save's.
const LEVEL: i32 = 3;

/// The host's game as a resync sends it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Payload {
    /// The host's steps: every peer's lockstep goes on from there.
    pub steps: u32,
    /// The desync's frame, as the chat says it.
    pub frame: u32,
    /// The game (`Sim::snapshot`).
    pub game: regent_core::save::Snapshot,
    /// The host's `play.input` so far (`record::Session::text`).
    pub play: String,
}

impl Payload {
    /// As sent.
    ///
    /// # Errors
    ///
    /// A snapshot that will not write.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let text = ron::to_string(self).map_err(|e| e.to_string())?;

        zstd::encode_all(text.as_bytes(), LEVEL).map_err(|e| e.to_string())
    }

    /// As sent, read back; and its RON, to keep beside the desync's
    /// snapshots.
    ///
    /// # Errors
    ///
    /// Bytes that are no payload.
    pub fn decode(bytes: &[u8]) -> Result<(Payload, String), String> {
        let raw = zstd::decode_all(bytes).map_err(|e| e.to_string())?;
        let text = String::from_utf8(raw).map_err(|e| e.to_string())?;
        let payload = ron::from_str(&text).map_err(|e| e.to_string())?;

        Ok((payload, text))
    }
}
