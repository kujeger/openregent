//! Readers for understood `.cam` payload types.
//!
//! These take a payload slice, not a `Record`: the chunk tag, not the record,
//! decides the reader.
//!
//! `TILE` and `SMNU` have their own modules, [`super::tile`] and
//! [`super::smnu`]. `FONT` is [`super::font`]. `DATA` records are plain text.

use super::cursor::Cursor;
use super::error::{Error, Result, slice, u16_at, u32_at};
use super::fourcc::FourCc;

/// Chunk tags whose payloads share the descriptor sub-header.
pub const DESCRIPTOR_TAGS: [&[u8; 4]; 9] = [
    b"DUNT", b"DMOV", b"DACT", b"DTRN", b"DTPT", b"DSND", b"DSDP", b"DSDG", b"DFCT",
];

/// Chunk tags whose payloads are plain RIFF/WAVE files.
pub const WAVE_TAGS: [&[u8; 4]; 1] = [b"WAVE"];

/// Chunk tags whose payloads are 256-entry palettes.
pub const PALETTE_TAGS: [&[u8; 4]; 2] = [b"PALT", b"SPLT"];

// --------------------------------------------------------------------------
// Descriptors
// --------------------------------------------------------------------------

const DESC_HEADER_LEN: usize = 16;

/// The 16-byte header of every `D*` descriptor payload.
///
/// Holds on all 1,438 descriptor records. `DATA` and `BTDT` lack it.
#[derive(Debug, Clone, Copy)]
pub struct Descriptor {
    /// Repeats the owning chunk's tag.
    pub tag: FourCc,
}

impl Descriptor {
    /// Split the shared header off a `D*` payload.
    ///
    /// # Errors
    ///
    /// If the payload is shorter than the header, or the stored body length
    /// disagrees with it. This tells `DATA` and `BTDT` apart from descriptors.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        let head = slice(payload, "descriptor header", 0, DESC_HEADER_LEN)?;
        let len = u32_at(head, "descriptor body length", 4)? as usize;

        // Stored length excludes the header.
        let expected = payload.len() - DESC_HEADER_LEN;

        if len != expected {
            return Err(Error::BrokenInvariant {
                what: "descriptor body length",
                expected: expected as u64,
                found: len as u64,
            });
        }

        Ok(Descriptor {
            tag: FourCc::from_prefix(head)?,
        })
    }
}

// --------------------------------------------------------------------------
// Palettes
// --------------------------------------------------------------------------

/// Length of every shipped palette. [`crate::data::tile`] uses it for the copy
/// after an uncompressed tile's pixels.
pub(crate) const PALETTE_LEN: usize = 1032;
const PALETTE_HEADER_LEN: usize = 8;
pub const PALETTE_COLOURS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Its channels, red first.
    #[must_use]
    pub const fn channels(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }
}

impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Rgb {
        Rgb { r, g, b }
    }
}

impl From<Rgb> for [u8; 3] {
    fn from(c: Rgb) -> [u8; 3] {
        c.channels()
    }
}

/// A `PALT` or `SPLT` payload: 8 header bytes then 256 RGB entries.
///
/// Header meaning unknown; all 1,166 hold `00 00 00 01 00 00 00 00`.
///
/// Each entry's fourth byte is dropped: zero, except `0xcd` (MSVC
/// uninitialised-heap fill) in 64 sprite palettes of `maindata.cam`.
#[derive(Debug, Clone)]
pub struct Palette {
    pub colours: [Rgb; PALETTE_COLOURS],
}

impl Palette {
    /// Read a `PALT` or `SPLT` payload.
    ///
    /// # Errors
    ///
    /// If the payload is not exactly 1032 bytes.
    pub fn parse(payload: &[u8]) -> Result<Self> {
        if payload.len() != PALETTE_LEN {
            return Err(Error::BadValue {
                what: "palette length",
                value: payload.len() as u64,
            });
        }

        let mut colours = [Rgb { r: 0, g: 0, b: 0 }; PALETTE_COLOURS];

        for (i, c) in colours.iter_mut().enumerate() {
            let e = &payload[PALETTE_HEADER_LEN + 4 * i..][..4];
            *c = Rgb {
                r: e[0],
                g: e[1],
                b: e[2],
            };
        }

        Ok(Palette { colours })
    }
}

// --------------------------------------------------------------------------
// String tables
// --------------------------------------------------------------------------

/// How a `STRT` payload frames a record, from the version byte at `+3`.
///
/// Sets the offset width and whether a key precedes each string.
///
/// Version 0 appears only in the two older editions; HD ships only version 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// Version 0: `u16` offsets, and a record is a bare string.
    Bare,
    /// Version 2: `u32` offsets, and a record is `[u32 key] cstr`.
    Keyed,
}

impl Form {
    fn parse(version: u8) -> Result<Self> {
        match version {
            0 => Ok(Form::Bare),
            2 => Ok(Form::Keyed),
            other => Err(Error::BadValue {
                what: "string table version",
                value: u64::from(other),
            }),
        }
    }

    const fn offset_bytes(self) -> usize {
        match self {
            Form::Bare => 2,
            Form::Keyed => 4,
        }
    }

    const fn key_bytes(self) -> usize {
        match self {
            Form::Bare => 0,
            Form::Keyed => 4,
        }
    }
}

/// How a `STRT` payload's strings are encoded, from the byte at `+2`.
///
/// Every table on hand is [`Encoding::Cp1250`]. [`Encoding::Unicode`] comes from
/// third-party tooling and has no sample: it parses, but its strings are
/// refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// One byte per character, Windows-1250.
    Cp1250,
    /// Never seen; width and byte order unknown.
    Unicode,
}

impl Encoding {
    fn parse(flag: u8) -> Result<Self> {
        match flag {
            0 => Ok(Encoding::Cp1250),
            8 => Ok(Encoding::Unicode),
            other => Err(Error::BadValue {
                what: "string table encoding",
                value: u64::from(other),
            }),
        }
    }
}

/// A `STRT` payload: a counted offset table followed by the records.
///
/// Holds on all 352 shipped string tables.
///
/// A [`Form::Keyed`] key is a four-character record id (`UNTN` by unit, `ACTN`
/// by action, `HPTX` by help topic) or the entry's own index, little-endian.
/// [`Self::get_by_key`] resolves a `HelpID` or `DialogID` to text.
#[derive(Debug, Clone)]
pub struct StringTable<'a> {
    pub form: Form,
    pub encoding: Encoding,
    payload: &'a [u8],
    offsets: Vec<usize>,
}

impl<'a> StringTable<'a> {
    /// Read a `STRT` payload.
    ///
    /// # Errors
    ///
    /// If the header is short, a flag byte is unknown, the first record does
    /// not follow the offset table, the offsets do not ascend, or a record's
    /// key or, in a Windows-1250 table, its terminated string runs past the
    /// payload.
    ///
    /// A record need not end at the next offset: `HKTX`'s offsets step two or
    /// three bytes past its fourth while its records run on (see
    /// `docs/formats/payloads.md`), and the reader follows the offsets.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let count = u16_at(payload, "string count", 0)? as usize;

        // Two independent bytes, not one u16: read as a word, Unicode plus
        // keyed would be rejected.
        let head = slice(payload, "string table header", 0, 4)?;
        let encoding = Encoding::parse(head[2])?;
        let form = Form::parse(head[3])?;

        let offsets = (0..count)
            .map(|i| {
                let at = 4 + form.offset_bytes() * i;

                match form {
                    Form::Bare => u16_at(payload, "string offset", at).map(usize::from),
                    Form::Keyed => u32_at(payload, "string offset", at).map(|o| o as usize),
                }
            })
            .collect::<Result<Vec<_>>>()?;

        // Records follow the offset table directly; catches a misread width.
        if let Some(&first) = offsets.first() {
            let expected = 4 + form.offset_bytes() * count;

            if first != expected {
                return Err(Error::BrokenInvariant {
                    what: "first string offset",
                    expected: expected as u64,
                    found: first as u64,
                });
            }
        }

        if let Some(pair) = offsets.windows(2).find(|p| p[0] >= p[1]) {
            return Err(Error::BrokenInvariant {
                what: "string offsets ascend",
                expected: pair[0] as u64,
                found: pair[1] as u64,
            });
        }

        // Every record's key and string lie inside the payload.
        for &start in &offsets {
            let text = slice(payload, "string record key", start, form.key_bytes())
                .map(|_| &payload[start + form.key_bytes()..])?;

            if encoding == Encoding::Cp1250 {
                super::error::cstr(text, "string")?;
            }
        }

        Ok(StringTable {
            form,
            encoding,
            payload,
            offsets,
        })
    }

    pub const fn len(&self) -> usize {
        self.offsets.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// The `i`th string, without its key or its terminator.
    ///
    /// `None` past the end, and for an [`Encoding::Unicode`] table.
    pub fn get(&self, i: usize) -> Option<std::borrow::Cow<'a, str>> {
        if self.encoding != Encoding::Cp1250 {
            return None;
        }

        let start = self.offsets.get(i)? + self.form.key_bytes();
        let rest = &self.payload[start..];
        let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());

        Some(super::cp1250::decode(&rest[..end]))
    }

    /// Payload offset of the `i`th record: the key if any, else the string.
    pub fn offset(&self, i: usize) -> Option<usize> {
        self.offsets.get(i).copied()
    }

    /// The `i`th record's key, or `None` for a [`Form::Bare`] table.
    ///
    /// For index keys, entry `n` has key `FourCc::from_word(n)`.
    pub fn key(&self, i: usize) -> Option<FourCc> {
        if self.form != Form::Keyed {
            return None;
        }

        let start = *self.offsets.get(i)?;

        Cursor::at_offset(self.payload, start)
            .fourcc("string key")
            .ok()
    }

    /// The string a key names, by linear scan.
    ///
    /// Tables are small (largest: `UNTN`, 394), so no index.
    pub fn get_by_key(&self, key: FourCc) -> Option<std::borrow::Cow<'a, str>> {
        (0..self.len())
            .find(|i| self.key(*i) == Some(key))
            .and_then(|i| self.get(i))
    }

    pub fn iter(&self) -> impl Iterator<Item = std::borrow::Cow<'a, str>> + '_ {
        (0..self.len()).filter_map(|i| self.get(i))
    }

    /// Every key in order, empty for a [`Form::Bare`] table.
    pub fn keys(&self) -> impl Iterator<Item = FourCc> + '_ {
        (0..self.len()).filter_map(|i| self.key(i))
    }
}

// --------------------------------------------------------------------------
// Audio
// --------------------------------------------------------------------------

/// Confirm a `WAVE` payload really is a RIFF/WAVE file.
///
/// Payloads are unmodified `.wav`; returned as-is.
///
/// # Errors
///
/// If the payload is under 12 bytes or is not `RIFF`-something-`WAVE`.
pub fn wave(payload: &[u8]) -> Result<&[u8]> {
    let head = slice(payload, "riff header", 0, 12)?;

    if &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
        Ok(payload)
    } else {
        Err(Error::BadMagic {
            what: "riff/wave",
            found: head.to_vec(),
        })
    }
}

/// The `fmt ` format tag of integer PCM.
const PCM_FORMAT: u16 = 1;

/// The only sample width shipped.
const PCM_BITS: u16 = 16;

/// A `WAVE` payload's samples: 16-bit integer PCM.
///
/// **Corpus**: every `WAVE` in HD's archives is format 1, 16 bits, mono or
/// stereo, at 11,025, 22,050 or 44,100 Hz (`every_wave_is_sixteen_bit_pcm`).
/// The engine hands the file to `DirectSound` as is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pcm<'a> {
    pub channels: u16,
    /// Frames a second.
    pub rate: u32,
    /// The `data` chunk: little-endian samples, channels interleaved.
    pub data: &'a [u8],
}

impl<'a> Pcm<'a> {
    /// Read a `WAVE` payload's `fmt ` and `data` chunks.
    ///
    /// # Errors
    ///
    /// Not RIFF/WAVE, a chunk past the end, no `fmt ` or `data`, or a format
    /// other than 16-bit PCM.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let file = wave(payload)?;
        let fmt = riff_chunk(file, *b"fmt ")?;
        let data = riff_chunk(file, *b"data")?;
        let format = u16_at(fmt, "wave format", 0)?;
        let bits = u16_at(fmt, "wave sample bits", 14)?;

        if format != PCM_FORMAT || bits != PCM_BITS {
            return Err(Error::BadValue {
                what: "wave format and bits",
                value: u64::from(format) << 16 | u64::from(bits),
            });
        }

        Ok(Pcm {
            channels: u16_at(fmt, "wave channels", 2)?,
            rate: u32_at(fmt, "wave rate", 4)?,
            data,
        })
    }

    /// Frames: one sample per channel.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.len() / (2 * usize::from(self.channels.max(1)))
    }

    /// Every sample in order, channels interleaved.
    pub fn samples(&self) -> impl Iterator<Item = i16> + 'a {
        self.data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
    }
}

/// The body of RIFF chunk `tag`, walking chunks from offset 12; odd lengths
/// pad to even.
fn riff_chunk(file: &[u8], tag: [u8; 4]) -> Result<&[u8]> {
    let mut at = 12;

    loop {
        let id = slice(file, "riff chunk id", at, 4)?;
        let len = u32_at(file, "riff chunk length", at + 4)? as usize;
        let body = slice(file, "riff chunk", at + 8, len)?;

        if id == tag.as_slice() {
            break Ok(body);
        }

        at += 8 + len + (len & 1);
    }
}

// --------------------------------------------------------------------------
// Build-tree variants
// --------------------------------------------------------------------------

/// A building id and the cap that follows it.
const ENTRY: usize = 8;

/// One `BTDT` record: a named game variant and the buildings it caps.
///
/// **Corpus**: five records, 428 bytes, read from the file alone. Edited by
/// `BuildTreeEditor_Controller`, a developer tool left in the expansion binary.
///
/// ```text
/// u32    version, 1 on all five
/// char*  variant name, NUL-terminated
/// u32    building count
///        that many: char[4] building id, u32 cap
/// ```
///
/// All five end exactly at the record's end.
///
/// The cap is how many of that building the player may have: `One of a Kind`
/// lists sixteen, all 1; `No Towers` lists three at 0 — `ABB1
/// Ballista_Tower`, `ABE1 Guardhouse1`, `ABM1 Wizards_Tower`.
///
/// `mx_btdata.cam`'s `STRT` record `BTDN` lists the same five names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variant<'a> {
    pub version: u32,
    /// Display name; `default` for the unmodified tree.
    pub name: &'a str,
    /// Building id and cap, file order. Empty on `default`.
    entries: &'a [u8],
}

impl<'a> Variant<'a> {
    /// Read a `BTDT` payload.
    ///
    /// # Errors
    ///
    /// If the name is unterminated or not UTF-8, or the entries do not fill the
    /// record exactly: a half-parsed tree is wrong rules, not partial ones.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let version = u32_at(payload, "BTDT version", 0)?;
        let rest = slice(payload, "BTDT name", 4, payload.len() - 4)?;
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or(Error::BrokenInvariant {
                what: "BTDT name is not terminated",
                expected: 0,
                found: rest.len() as u64,
            })?;

        let name = std::str::from_utf8(&rest[..end]).map_err(|_| Error::BrokenInvariant {
            what: "BTDT name is not text",
            expected: 0,
            found: end as u64,
        })?;

        let at = 4 + end + 1;
        let count = u32_at(payload, "BTDT entry count", at)? as usize;
        let entries = slice(payload, "BTDT entries", at + 4, count * ENTRY)?;

        if at + 4 + count * ENTRY != payload.len() {
            return Err(Error::BrokenInvariant {
                what: "BTDT entries do not fill the record",
                expected: payload.len() as u64,
                found: (at + 4 + count * ENTRY) as u64,
            });
        }

        Ok(Variant {
            version,
            name,
            entries,
        })
    }

    /// Each building and its cap.
    pub fn caps(&self) -> impl Iterator<Item = (FourCc, u32)> + '_ {
        self.entries
            .as_chunks::<ENTRY>()
            .0
            .iter()
            .filter_map(|entry| {
                Some((
                    FourCc::from_prefix(entry).ok()?,
                    u32_at(entry, "BTDT cap", 4).ok()?,
                ))
            })
    }

    /// How many buildings this variant caps.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len() / ENTRY
    }

    /// Whether it caps nothing, as `default` does.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// --------------------------------------------------------------------------
// FLIC frame metadata
// --------------------------------------------------------------------------

/// Record length, and where the source path sits.
const CUT_LEN: usize = 886;
const CUT_PATH: usize = 0x4a;

/// One `CUT ` record: metadata for a frame of an Autodesk FLIC animation.
///
/// **Corpus**: twenty 886-byte records in `maindata.cam`, identical in all
/// three editions. The chunk has the index flag, so record ids are 0 to 19.
///
/// Mostly zeroes. Every byte ever non-zero falls in one of these:
///
/// ```text
/// +0x000  u32    version, 3 on all twenty
/// +0x014  fixed  211.0 for one source file, 83.0 for the other
/// +0x034  fixed  the frame's number in its animation
/// +0x038  fixed  245.0, or 0.0
/// +0x03c  fixed  253.0, or 0.0
/// +0x040  fixed  1.0, or 8.0
/// +0x04a  cstr   the artist's own path to the source
/// +0x24a  u8     the frame number again
/// +0x372  f32    0.004 on all twenty, and the record's last four bytes
/// ```
///
/// `fixed` is 16.16, as in save positions and the engine's `cyfixedlp.h`.
///
/// Field meanings are unknown. Only two sources span the twenty —
/// `…\Effects\Build_Flag\Build_Flag.flc` (six frames) and
/// `…\Effects\Particles\Highlight1.flc` (fourteen) — too few to tell a
/// property from a coincidence.
///
/// 886 bytes cannot hold a frame, so these are provenance: the FLIC the
/// shipped tiles were converted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cut<'a> {
    pub version: u32,
    /// The artist's path on the build machine.
    pub source: &'a str,
    /// Frame number within that animation.
    pub frame: u32,
    /// The whole record, for the unnamed fields.
    pub body: &'a [u8],
}

impl<'a> Cut<'a> {
    /// Read a `CUT ` payload.
    ///
    /// # Errors
    ///
    /// If the payload is not `CUT_LEN` bytes, the path is unterminated or not
    /// UTF-8, or the two frame numbers disagree (holds on all twenty).
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        if payload.len() != CUT_LEN {
            return Err(Error::BadValue {
                what: "CUT record length",
                value: payload.len() as u64,
            });
        }

        let rest = &payload[CUT_PATH..];
        let end = rest
            .iter()
            .position(|b| *b == 0)
            .ok_or(Error::BrokenInvariant {
                what: "CUT source path is not terminated",
                expected: 0,
                found: rest.len() as u64,
            })?;

        let source = std::str::from_utf8(&rest[..end]).map_err(|_| Error::BrokenInvariant {
            what: "CUT source path is not text",
            expected: 0,
            found: end as u64,
        })?;

        // 16.16, so the whole number is the top half.
        let frame = u32_at(payload, "CUT frame", 0x34)? >> 16;

        if frame != u32::from(payload[0x24a]) {
            return Err(Error::BrokenInvariant {
                what: "CUT states its frame number twice and they disagree",
                expected: frame.into(),
                found: u64::from(payload[0x24a]),
            });
        }

        Ok(Cut {
            version: u32_at(payload, "CUT version", 0)?,
            source,
            frame,
            body: payload,
        })
    }

    /// The source file name, without directories.
    #[must_use]
    pub fn file(&self) -> &'a str {
        self.source.rsplit('\\').next().unwrap_or(self.source)
    }
}

// --------------------------------------------------------------------------
// Cinematics and full-frame images
// --------------------------------------------------------------------------

/// A `PICT` payload: a kind word, then a standard file.
///
/// **Corpus**: 34 records, 20 in `cinedata1.cam` and 14 in `mx_cinedata1.cam`,
/// See `docs/formats/payloads.md`. The cinematics' `.dat` archives
/// (`view::menu::CINEMATICS`) hold six more videos, each with a
/// sound track, and `SPLH` again.
///
/// Like [`wave`], this validates and hands the file on; the `bink` crate
/// decodes the videos (`tests/video.rs`).
///
/// | Kind | Holds | Records |
/// |---|---|---|
/// | 7 | A Bink video: `MV01`, `MV06` 640×480, the 31 `QM*` 260×260 | 33 |
/// | 1 | A Truevision TGA | 1, `SPLH` |
///
/// Bink revisions: `BIKh` on 19 of the base game's 20, `BIKf` on `QM13`,
/// `BIKi` on the expansion's 13. All 15 frames a second, no audio track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pict<'a> {
    /// Kind 7. The four-byte revision, and the whole Bink file.
    Video { revision: [u8; 4], file: &'a [u8] },
    /// Kind 1. The whole TGA file, header and version-2 footer included.
    Image { file: &'a [u8] },
}

/// What TGA version 2 puts in its last 18 bytes.
const TGA_FOOTER: &[u8; 18] = b"TRUEVISION-XFILE.\0";

impl<'a> Pict<'a> {
    /// Read a `PICT` payload.
    ///
    /// # Errors
    ///
    /// If the kind is not 1 or 7, or the file does not match it. Bink's length
    /// field must end exactly at the payload's end; TGA must carry its footer.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let kind = u32_at(payload, "PICT kind", 0)?;
        let file = slice(payload, "PICT file", 4, payload.len() - 4)?;

        match kind {
            7 => {
                let head = slice(file, "bink header", 0, 12)?;

                if &head[..3] != b"BIK" {
                    return Err(Error::BadMagic {
                        what: "bink video",
                        found: head[..4].to_vec(),
                    });
                }

                // Bink's size excludes the magic and the size field.
                let stated = u32_at(head, "bink size", 4)? as usize + 8;

                if stated != file.len() {
                    return Err(Error::BrokenInvariant {
                        what: "bink video does not fill its PICT record",
                        expected: file.len() as u64,
                        found: stated as u64,
                    });
                }

                Ok(Pict::Video {
                    revision: [head[0], head[1], head[2], head[3]],
                    file,
                })
            }
            1 => {
                let footer = file
                    .len()
                    .checked_sub(TGA_FOOTER.len())
                    .and_then(|at| file.get(at..))
                    .ok_or(Error::BadValue {
                        what: "PICT image too short for a TGA footer",
                        value: file.len() as u64,
                    })?;

                if footer != TGA_FOOTER {
                    return Err(Error::BadMagic {
                        what: "truevision tga",
                        found: footer.to_vec(),
                    });
                }

                Ok(Pict::Image { file })
            }
            other => Err(Error::BadValue {
                what: "PICT kind",
                value: u64::from(other),
            }),
        }
    }
}
