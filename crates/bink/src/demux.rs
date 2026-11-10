// Vendored from NihAV, nihav-rad/src/demuxers/bink.rs; author Kostya
// Shishkov; GNU AGPL version 3. Source, commit and changes: ../README.md.
// Changed here: the header and frame index are read from a byte slice
// into plain structs instead of NihAV's `ByteIO`, `StreamManager` and
// `SeekIndex`; Bink 2 (`KB2*`) and Bink 'b' are refused; audio tracks are
// listed (`AudioTrack`) and their packets cut out (`audio_packet`) rather
// than queued as `NAPacket`s; seeking and the demuxer options removed; a
// frame count the file cannot index is refused before its index is
// allocated.

// Upstream formatting kept.
#![cfg_attr(rustfmt, rustfmt::skip)]

use crate::Error as DemuxerError;

type DemuxerResult<T> = Result<T, DemuxerError>;

macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DemuxerError::InvalidData); } };
}

/// A Bink file's header, as its first bytes state it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The magic's last byte: `b'f'` to `b'k'`.
    pub revision: u8,
    /// Picture width in pixels.
    pub width: usize,
    /// Picture height in pixels.
    pub height: usize,
    /// Number of frames.
    pub frames: usize,
    /// Frame rate numerator: `rate_num / rate_den` frames a second.
    pub rate_num: u32,
    /// Frame rate denominator.
    pub rate_den: u32,
    /// The video flags word (alpha `0x0010_0000`, grey `0x0002_0000`).
    pub flags: u32,
    /// Number of audio tracks.
    pub audio_tracks: usize,
}

/// One audio track, as the header states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioTrack {
    /// Samples a second.
    pub rate: u32,
    /// 1 or 2.
    pub channels: usize,
    /// Coded with the DCT, not the RDFT.
    pub dct: bool,
}

const BINK_AUD_FLAG_DCT:    u8 = 0x10;
const BINK_AUD_FLAG_STEREO: u8 = 0x20;

const BINK_FLAG_ALPHA:  u32 = 0x00100000;
const BINK_FLAG_GRAY:   u32 = 0x00020000;

/// The header and where each frame's packet starts. Bit 0 of a
/// position marks a key frame; the last entry is the end of the data.
pub(crate) struct Index {
    pub(crate) header: Header,
    pub(crate) audio: Vec<AudioTrack>,
    pub(crate) frame_pos: Vec<u32>,
}

fn u32le(src: &[u8], at: usize) -> DemuxerResult<u32> {
    src.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(DemuxerError::ShortData)
}

pub(crate) fn open(src: &[u8]) -> DemuxerResult<Index> {
    let magic = src.get(0..4).ok_or(DemuxerError::ShortData)?;
    validate!(&magic[..3] == b"BIK");
    // Upstream takes `b` to `k` and Bink 2; this decoder only `f` to `k`.
    if !(b'f'..=b'k').contains(&magic[3]) { return Err(DemuxerError::NotImplemented); }
    let fsize                           = (u32le(src, 4)? as usize) + 8;
    let frames                          = u32le(src, 8)? as usize;
    let max_size                        = u32le(src, 12)? as usize;
    let _frames2                        = u32le(src, 16)? as usize;
    let width                           = u32le(src, 20)? as usize;
    let height                          = u32le(src, 24)? as usize;
    let tb_den                          = u32le(src, 28)?;
    let tb_num                          = u32le(src, 32)?;
    validate!((width > 0) && (height > 0) && (width <= 7680) && (height <= 4800));
    validate!((frames > 0) && (tb_num > 0) && (tb_den > 0) && (max_size < fsize));
    let flags                           = u32le(src, 36)?;
    if (flags & (BINK_FLAG_ALPHA | BINK_FLAG_GRAY)) != 0 { return Err(DemuxerError::NotImplemented); }

    let num_audio                       = u32le(src, 40)? as usize;
    validate!(num_audio < 256);
    // Per track: max output frame size, then rate and flags, then an id.
    let mut pos = 44 + num_audio * 4;
    let mut audio = Vec::with_capacity(num_audio);
    for _ in 0..num_audio {
        let word                        = u32le(src, pos)?;
        let (srate, flags) = (word & 0xFF_FFFF, (word >> 24) as u8);
        validate!(srate > 0);
        let channels = if (flags & BINK_AUD_FLAG_STEREO) != 0 { 2 } else { 1 };
        audio.push(AudioTrack { rate: srate, channels, dct: (flags & BINK_AUD_FLAG_DCT) != 0 });
        pos += 4;
    }
    pos += num_audio * 4;

    // The index's words must fit in the file before they are allocated.
    validate!(frames < src.len().saturating_sub(pos) / 4);
    let mut frame_pos = Vec::with_capacity(frames + 1);
    for _ in 0..=frames {
        frame_pos.push(u32le(src, pos)?);
        pos += 4;
    }
    validate!((pos as u32) == (frame_pos[0] & !1));
    validate!(frame_pos.windows(2).all(|w| (w[0] & !1) < (w[1] & !1)));
    validate!(((frame_pos[frames] & !1) as usize) <= src.len());

    let header = Header {
        revision: magic[3],
        width,
        height,
        frames,
        rate_num: tb_den,
        rate_den: tb_num,
        flags,
        audio_tracks: num_audio,
    };
    Ok(Index { header, audio, frame_pos })
}

/// Frame `n`'s audio packet of track `track`; empty when it has none.
pub(crate) fn audio_packet<'a>(src: &'a [u8], index: &Index, n: usize, track: usize) -> DemuxerResult<&'a [u8]> {
    let start = (index.frame_pos[n] & !1) as usize;
    let end = (index.frame_pos[n + 1] & !1) as usize;
    let mut pos = start;
    validate!(track < index.header.audio_tracks);
    for t in 0..=track {
        let size                            = u32le(src, pos)? as usize;
        validate!(pos + 4 + size <= end);
        if t == track {
            return src.get(pos + 4..pos + 4 + size).ok_or(DemuxerError::ShortData);
        }
        pos += size + 4;
    }
    Err(DemuxerError::InvalidData)
}

/// Frame `n`'s video packet: the frame's data less its audio packets.
pub(crate) fn video_packet<'a>(src: &'a [u8], index: &Index, n: usize) -> DemuxerResult<&'a [u8]> {
    let start = (index.frame_pos[n] & !1) as usize;
    let end = (index.frame_pos[n + 1] & !1) as usize;
    let mut payload_size = end - start;
    let mut pos = start;
    validate!(payload_size > index.header.audio_tracks * 4);
    for _ in 0..index.header.audio_tracks {
        let size                            = u32le(src, pos)? as usize;
        validate!(payload_size > size + 4);
        payload_size -= size + 4;
        pos += size + 4;
    }
    src.get(pos..pos + payload_size).ok_or(DemuxerError::ShortData)
}
