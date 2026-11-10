//! A Bink 1 decoder: revisions `f` to `k`, video and RDFT audio.
//!
//! Vendored from Kostya Shishkov's `NihAV` (`nihav-rad`, `nihav-core`),
//! GNU AGPL version 3; what came from where and what changed is in
//! `README.md`. This file is the crate's own API around it.
//!
//! ```text
//! .bik bytes ──Video::open──> Header (size, frames, rate)
//!            ──next_frame───> Frame: Y, U, V planes ──rgba──> RGBA bytes
//! .bik bytes ──Audio::open──> AudioTrack (rate, channels)
//!            ──next_packet──> a frame's samples, f32, interleaved
//! ```

use std::fmt;

// Upstream code keeps its own style; the workspace's pedantic lints are
// not applied to it.
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod binkaud;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod binkauddata;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod binkvid;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod binkviddata;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod bitreader;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod codebook;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod demux;
#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
mod fft;

pub use demux::{AudioTrack, Header};

/// Why a file or frame did not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The data breaks the format.
    InvalidData,
    /// The data ends early.
    ShortData,
    /// A Bink this decoder does not take: Bink 2, revision `b`, alpha or
    /// grey, DCT audio; or audio asked of a file with none.
    NotImplemented,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self {
            Error::InvalidData => "invalid Bink data",
            Error::ShortData => "Bink data ends early",
            Error::NotImplemented => "unsupported Bink variant",
        };
        f.write_str(what)
    }
}

impl std::error::Error for Error {}

impl From<bitreader::BitReaderError> for Error {
    fn from(e: bitreader::BitReaderError) -> Self {
        match e {
            bitreader::BitReaderError::BitstreamEnd => Error::ShortData,
            bitreader::BitReaderError::TooManyBitsRequested => Error::InvalidData,
        }
    }
}

impl From<codebook::CodebookError> for Error {
    fn from(_: codebook::CodebookError) -> Self {
        Error::InvalidData
    }
}

/// One plane of a picture: rows of `stride` bytes, `width` by `height`
/// of them shown.
#[derive(Debug, Clone, Default)]
pub struct Plane {
    data: Vec<u8>,
    stride: usize,
    width: usize,
    height: usize,
}

/// Buffers are padded to this many pixels each way, as `NihAV` allocates
/// them: blocks are 8×8, and chroma is half size.
const ALIGN: usize = 16;

impl Plane {
    fn new(width: usize, height: usize, stride: usize, rows: usize) -> Plane {
        Plane {
            data: vec![0; stride * rows],
            stride,
            width,
            height,
        }
    }

    /// Row `y`, its shown pixels only.
    pub fn row(&self, y: usize) -> &[u8] {
        &self.data[y * self.stride..][..self.width]
    }
}

/// A decoded picture, YUV 4:2:0: planes Y, U (Cb), V (Cr).
#[derive(Debug, Clone)]
pub struct Frame {
    planes: [Plane; 3],
}

impl Frame {
    fn new(width: usize, height: usize) -> Frame {
        let (padded_w, padded_h) = (
            width.next_multiple_of(ALIGN),
            height.next_multiple_of(ALIGN),
        );
        let chroma = || {
            Plane::new(
                width.div_ceil(2),
                height.div_ceil(2),
                padded_w / 2,
                padded_h / 2,
            )
        };

        Frame {
            planes: [
                Plane::new(width, height, padded_w, padded_h),
                chroma(),
                chroma(),
            ],
        }
    }

    pub const fn width(&self) -> usize {
        self.planes[0].width
    }

    pub const fn height(&self) -> usize {
        self.planes[0].height
    }

    /// The shown pixels of Y, U and V in turn, rows packed: the layout
    /// ffmpeg's `-pix_fmt yuv420p` writes, so hashes compare with it.
    pub fn yuv420p(&self) -> Vec<u8> {
        self.planes
            .iter()
            .flat_map(|p| (0..p.height).flat_map(move |y| p.row(y).iter().copied()))
            .collect()
    }

    /// The picture as RGBA, rows packed, alpha 255, into `out`.
    ///
    /// BT.601, limited range (ffmpeg tags these files `tv`), each chroma
    /// sample over its 2×2 pixels. **Inferred**: what the original's
    /// Bink SDK blit does is unread; this is close, not proven equal.
    pub fn rgba(&self, out: &mut Vec<u8>) {
        let [y_plane, u_plane, v_plane] = &self.planes;
        out.clear();
        out.reserve(self.width() * self.height() * 4);

        for y in 0..self.height() {
            let (luma, u_row, v_row) = (y_plane.row(y), u_plane.row(y / 2), v_plane.row(y / 2));

            for (x, &l) in luma.iter().enumerate() {
                out.extend_from_slice(&bt601(l, u_row[x / 2], v_row[x / 2]));
            }
        }
    }
}

/// One limited-range BT.601 pixel to RGBA, in 16.16 fixed point:
/// `R = 1.164 (Y − 16) + 1.596 (V − 128)` and so on.
fn bt601(luma: u8, cb: u8, cr: u8) -> [u8; 4] {
    const ONE: i32 = 1 << 16;
    const HALF: i32 = ONE / 2;
    let luma = (i32::from(luma) - 16) * 76_309;
    let cb = i32::from(cb) - 128;
    let cr = i32::from(cr) - 128;
    let clamp = |x: i32| u8::try_from(((x + HALF) >> 16).clamp(0, 255)).unwrap_or(u8::MAX);

    [
        clamp(luma + 104_597 * cr),
        clamp(luma - 25_675 * cb - 53_279 * cr),
        clamp(luma + 132_201 * cb),
        u8::MAX,
    ]
}

/// A Bink file opened for decoding, frame after frame.
pub struct Video<B: AsRef<[u8]>> {
    file: B,
    index: demux::Index,
    decoder: binkvid::BinkDecoder,
    /// The frame last decoded, and the one before it, which the next
    /// predicts from.
    shown: Frame,
    back: Frame,
    next: usize,
}

impl<B: AsRef<[u8]>> Video<B> {
    /// Reads `file`'s header and frame index.
    ///
    /// # Errors
    ///
    /// If the header or index is broken, or the file is a Bink this
    /// decoder does not take ([`Error::NotImplemented`]).
    pub fn open(file: B) -> Result<Self, Error> {
        let index = demux::open(file.as_ref())?;
        let Header {
            width,
            height,
            revision,
            ..
        } = index.header;

        Ok(Video {
            decoder: binkvid::BinkDecoder::new(width, height, revision),
            shown: Frame::new(width, height),
            back: Frame::new(width, height),
            next: 0,
            file,
            index,
        })
    }

    pub const fn header(&self) -> &Header {
        &self.index.header
    }

    /// How many frames have been decoded since the start.
    pub const fn position(&self) -> usize {
        self.next
    }

    /// Decodes the next frame; `None` after the last.
    ///
    /// # Errors
    ///
    /// If the frame's data is broken. The video should then be dropped.
    pub fn next_frame(&mut self) -> Option<Result<&Frame, Error>> {
        (self.next < self.index.header.frames).then(|| {
            let packet = demux::video_packet(self.file.as_ref(), &self.index, self.next)?;
            let prev = (self.next > 0).then_some(&self.shown);
            self.decoder.decode(packet, &mut self.back, prev)?;
            std::mem::swap(&mut self.shown, &mut self.back);
            self.next += 1;
            Ok(&self.shown)
        })
    }
}

/// A Bink file's first audio track opened for decoding, frame after frame.
pub struct Audio<B: AsRef<[u8]>> {
    file: B,
    index: demux::Index,
    track: AudioTrack,
    decoder: binkaud::BinkAudioDecoder,
    /// The last packet's samples.
    samples: Vec<f32>,
    next: usize,
}

impl<B: AsRef<[u8]>> Audio<B> {
    /// Reads `file`'s header and frame index, for its first audio track.
    ///
    /// # Errors
    ///
    /// If the header or index is broken, or there is no RDFT track
    /// ([`Error::NotImplemented`]).
    pub fn open(file: B) -> Result<Self, Error> {
        let index = demux::open(file.as_ref())?;
        let track = *index.audio.first().ok_or(Error::NotImplemented)?;

        if track.dct {
            return Err(Error::NotImplemented);
        }

        let rate = usize::try_from(track.rate).map_err(|_| Error::InvalidData)?;

        Ok(Audio {
            decoder: binkaud::BinkAudioDecoder::new(rate, track.channels)?,
            samples: Vec::new(),
            next: 0,
            file,
            index,
            track,
        })
    }

    pub const fn track(&self) -> &AudioTrack {
        &self.track
    }

    pub const fn header(&self) -> &Header {
        &self.index.header
    }

    /// Decodes the next frame's audio, its samples interleaved by
    /// channel; `None` after the last frame. A frame with no audio gives
    /// none.
    ///
    /// # Errors
    ///
    /// If the packet is broken. The track should then be dropped.
    pub fn next_packet(&mut self) -> Option<Result<&[f32], Error>> {
        (self.next < self.index.header.frames).then(|| {
            let packet = demux::audio_packet(self.file.as_ref(), &self.index, self.next, 0)?;

            self.samples.clear();
            self.next += 1;

            if !packet.is_empty() {
                self.decoder.decode(packet, &mut self.samples)?;
            }

            Ok(self.samples.as_slice())
        })
    }

    /// The samples a packet states it holds, every channel's together:
    /// its first word is that count in 16-bit bytes.
    ///
    /// # Errors
    ///
    /// If the frame's packet is broken.
    pub fn stated(&self, frame: usize) -> Result<usize, Error> {
        let packet = demux::audio_packet(self.file.as_ref(), &self.index, frame, 0)?;
        let bytes = packet
            .get(..4)
            .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));

        usize::try_from(bytes / 2).map_err(|_| Error::InvalidData)
    }
}
