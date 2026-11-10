//! A music track's end in game time, for a run that plays no audio
//! (`--headless`), so its scripts' music callback runs as a drawn game's
//! does at the default pace, where a game ms is a real one.
//!
//! ```text
//!  Sim::music() n ──> MusicTracks.txt line n ──> its MP3's length
//!  started at game ms t ──> ends at t + length ──> Sim::music_stopped
//!  the callback leaves the track ──> it starts again a poll later
//! ```
//!
//! **Inferred**: a stand-in for real time. A drawn game's track ends by
//! the wall clock; a run with no audio has no clock but the game's. The
//! callback changes no play (`Game::music_stopped`), so no replay needs
//! it.

use std::path::Path;

use regent_core::view::menu::music::POLL_MS;

use crate::audio::{MUSIC, TRACKS};

/// An MPEG audio frame header's sync bits, its first 11.
const SYNC: u32 = 0xffe0_0000;

/// The header's bytes.
const HEADER: usize = 4;

/// An `ID3v2` tag's head and its bytes: "ID3", version, flags, size.
const ID3: &[u8; 3] = b"ID3";
const ID3_HEAD: usize = 10;

/// The version field: MPEG-1, MPEG-2, MPEG-2.5.
const MPEG1: u32 = 3;
const MPEG2: u32 = 2;
const MPEG25: u32 = 0;

/// The layer field's Layer III.
const LAYER3: u32 = 1;

/// Layer III bit rates, kbit/s, by index: MPEG-1's, then MPEG-2's and
/// 2.5's (ISO 11172-3, ISO 13818-3); 0 free, 15 bad.
const RATES1: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const RATES2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];

/// MPEG-1 sample rates, Hz, by index; MPEG-2 halves them, 2.5 quarters.
const HERTZ1: [u32; 3] = [44_100, 48_000, 32_000];

/// Samples in a Layer III frame: MPEG-1's; MPEG-2 and 2.5 have half.
const SAMPLES1: u32 = 1_152;

/// One frame: its bytes, samples and sample rate.
struct Frame {
    bytes: usize,
    samples: u32,
    hertz: u32,
}

/// The Layer III frame whose header `word` is; `None` for anything else.
fn frame(word: u32) -> Option<Frame> {
    let version = (word >> 19) & 3;
    let layer = (word >> 17) & 3;
    let rate = ((word >> 12) & 0xf) as usize;
    let hertz = ((word >> 10) & 3) as usize;
    let padding = (word >> 9) & 1;
    let (rates, shift) = match version {
        MPEG1 => Some((&RATES1, 0)),
        MPEG2 => Some((&RATES2, 1)),
        MPEG25 => Some((&RATES2, 2)),
        _ => None,
    }?;
    let kbits = rates.get(rate).copied().filter(|k| *k > 0)?;
    let hertz = HERTZ1.get(hertz)? >> shift;
    let samples = if version == MPEG1 {
        SAMPLES1
    } else {
        SAMPLES1 / 2
    };
    // Bytes: samples / 8 a second's bits at the rate, plus the padding.
    let bytes = samples / 8 * kbits * 1000 / hertz + padding;

    (word & SYNC == SYNC && layer == LAYER3).then_some(Frame {
        bytes: bytes as usize,
        samples,
        hertz,
    })
}

/// An MP3's length, ms: its Layer III frames' samples, walked from the
/// first header after any `ID3v2` tag to the first bytes that are none.
/// `None` with no frame.
#[must_use]
pub fn length_ms(mp3: &[u8]) -> Option<u32> {
    let start = if mp3.starts_with(ID3) && mp3.len() >= ID3_HEAD {
        // A syncsafe size: seven bits a byte.
        let size = mp3[6..ID3_HEAD]
            .iter()
            .fold(0usize, |n, b| (n << 7) | usize::from(b & 0x7f));

        ID3_HEAD + size
    } else {
        0
    };
    let mut at = start;
    let mut samples = 0u64;
    let mut hertz = None;

    while let Some(f) = mp3
        .get(at..at + HEADER)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .and_then(frame)
    {
        samples += u64::from(f.samples);
        hertz = Some(f.hertz);
        at += f.bytes;
    }

    hertz.and_then(|h| u32::try_from(samples * 1000 / u64::from(h)).ok())
}

/// The track playing, as a run with no audio follows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Playing {
    track: i32,
    /// Game ms it ends at.
    ends: u32,
}

/// Each track's length and the one playing.
#[derive(Debug, Default)]
pub struct Tunes {
    /// Ms by `MusicTracks.txt` line, from 1; `None` a CD track or a file
    /// that will not read, which never plays.
    lengths: Vec<Option<u32>>,
    playing: Option<Playing>,
}

impl Tunes {
    /// The install at `hd`'s tracks, as the music player plays them: a
    /// line opening with a digit is a CD track.
    #[must_use]
    pub fn read(hd: &Path) -> Tunes {
        let text = std::fs::read_to_string(hd.join(TRACKS)).unwrap_or_default();
        let lengths = text
            .lines()
            .map(str::trim)
            .map(|name| {
                (!name.starts_with(|c: char| c.is_ascii_digit()))
                    .then(|| std::fs::read(hd.join(MUSIC).join(name)).ok())
                    .flatten()
                    .and_then(|mp3| length_ms(&mp3))
            })
            .collect();

        Tunes::of(lengths)
    }

    /// Tracks of `lengths`, ms, by line from 1.
    #[must_use]
    pub const fn of(lengths: Vec<Option<u32>>) -> Tunes {
        Tunes {
            lengths,
            playing: None,
        }
    }

    /// Track `track`'s length, ms.
    fn length(&self, track: i32) -> Option<u32> {
        let line = usize::try_from(track).ok()?.checked_sub(1)?;

        self.lengths.get(line).copied().flatten()
    }

    /// `track` started at game ms `now`, if it plays.
    fn start(&mut self, track: Option<i32>, now: u32) {
        self.playing = track.and_then(|track| {
            self.length(track).map(|len| Playing {
                track,
                ends: now.saturating_add(len),
            })
        });
    }

    /// After a frame at game ms `now`, the world asking `music`: whether
    /// the track playing has ended. Another asked starts now, as the
    /// player starts it at its next update.
    pub fn ended(&mut self, music: Option<i32>, now: u32) -> bool {
        match self.playing {
            Some(p) if Some(p.track) == music => now >= p.ends,
            _ => {
                self.start(music, now);
                false
            }
        }
    }

    /// After the end's callback, the world asking `music`: another track
    /// starts now; the same, left silent, a poll later
    /// ([`POLL_MS`], `GE_MusicManager`).
    pub fn next(&mut self, music: Option<i32>, now: u32) {
        let same = self.playing.is_some_and(|p| Some(p.track) == music);
        let poll = u32::try_from(POLL_MS).unwrap_or(u32::MAX);

        self.start(music, if same { now.saturating_add(poll) } else { now });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `EarlyGame.mp3`: 16,631 frames of 1,152 samples at 44,100 Hz.
    /// **Corpus**: `ffprobe -count_frames` counts 16,631 and gives
    /// 434.442450 s.
    const EARLY_GAME_MS: u32 = 434_442;

    /// Each HD track's length, by line: the theme twice, early, mid and
    /// end game, the epic quest. **Corpus**: `ffprobe` gives 186.906100,
    /// 434.442450, 434.442450, 425.142850 and 132.022850 s.
    const LENGTHS: [u32; 6] = [186_906, 186_906, 434_442, 434_442, 425_142, 132_022];

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn an_mp3s_length_is_its_frames_samples() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");
        let mp3 = std::fs::read(hd.join(MUSIC).join("EarlyGame.mp3")).expect("the track");

        assert_eq!(length_ms(&mp3), Some(EARLY_GAME_MS));
        assert_eq!(length_ms(&[0; 64]), None);
    }

    #[test]
    #[cfg_attr(not(feature = "install"), ignore = "needs the game's install")]
    fn the_installs_tracks_are_read_by_line() {
        let hd = crate::files::env::path(crate::files::env::HD).expect("MAJESTY_HD");

        assert_eq!(Tunes::read(&hd).lengths, LENGTHS.map(Some));
    }

    /// A track ends its length after it started; the callback's next
    /// track starts then, the same one a poll later.
    #[test]
    fn a_track_ends_its_length_on() {
        let mut tunes = Tunes::of(vec![Some(1_000), Some(2_000)]);

        assert!(!tunes.ended(Some(1), 100));
        assert!(!tunes.ended(Some(1), 1_050));
        assert!(tunes.ended(Some(1), 1_100));

        tunes.next(Some(1), 1_100);

        let poll = u32::try_from(POLL_MS).expect("small");

        assert!(!tunes.ended(Some(1), 2_099 + poll));
        assert!(tunes.ended(Some(1), 2_100 + poll));

        tunes.next(Some(2), 3_000);

        assert!(!tunes.ended(Some(2), 4_999));
        assert!(tunes.ended(Some(2), 5_000));
        assert!(!tunes.ended(Some(3), 5_050), "no such track: silence");
    }
}
