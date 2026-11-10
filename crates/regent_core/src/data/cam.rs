//! Reader for `.cam` archives — the CYLB container holding all bulk game data.
//!
//! Layout, verified against all 46 HD archives:
//!
//! ```text
//! header (0x14 bytes)
//!     0x00  char[8]  "CYLBPC  "      two trailing spaces
//!     0x08  u16      version major   always 1
//!     0x0a  u16      version minor   always 1
//!     0x0c  u32      chunk count
//!     0x10  u32      total bytes of every chunk's record table
//!
//! directory (immediately after the header, 8 bytes per chunk)
//!     char[4]  tag
//!     u32      absolute offset of that chunk's record table
//!
//! chunk (at that offset)
//!     u32      record count
//!     u32      flags
//!     record[] 28 bytes each:
//!         char[4]   id
//!         char[16]  name, NUL-padded
//!         u32       absolute payload offset
//!         u32       payload length
//! ```
//!
//! The first chunk begins where the directory ends, and the payload heap
//! exactly `len_tables` bytes later. The load checks the second.
//!
//! An [`Image`] is a file read but its [`DEFERRED`] payloads, which a
//! record reads from disk when asked ([`Record::bytes`]); [`Defer::Pixels`]
//! leaves `TILE` there too:
//!
//! ```text
//!  file    [header, directory, tables][DUNT][WAVE ......][DUNT][PICT ....]
//!  Image   [header, directory, tables][DUNT]             [DUNT]
//!  bytes() of a WAVE or PICT record: seek, read its own range
//! ```

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::cursor::Cursor;
use super::error::{Error, Result, slice, u32_at};
use super::fourcc::FourCc;

pub const MAGIC: &[u8; 8] = b"CYLBPC  ";

/// The header's version, major and minor: 1.1 in every edition and both
/// Workshop mods.
pub const VERSION: (u16, u16) = (1, 1);

const HEADER_LEN: usize = 0x14;
const DIR_ENTRY_LEN: usize = 8;
const RECORD_LEN: usize = 28;
const NAME_LEN: usize = 16;

/// A chunk's flag word. Set on exactly `TILE`, `SPLT` and `CUT ` in the
/// install; Phantoms Haunt leaves its `SPLT` chunk clear.
///
/// In every flagged chunk the record id is the record's little-endian index
/// (33,383 of 33,383); unflagged chunks use four-character tags.
///
/// Not compression: `TILE` is plain row-RLE and a `SPLT` a flat 1032 bytes.
///
/// **Binary**: bit 0 lets a number index the chunk by position
/// (lib entry by id, kept at the type table's by
/// lib read index); clear, a number is matched against the ids.
/// See [`Catalogue::positional`](super::catalogue::Catalogue::positional).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkFlag {
    Clear,
    Set,
}

impl ChunkFlag {
    fn parse(raw: u32) -> Result<Self> {
        match raw {
            0 => Ok(ChunkFlag::Clear),
            1 => Ok(ChunkFlag::Set),
            other => Err(Error::BadValue {
                what: "chunk flags",
                value: u64::from(other),
            }),
        }
    }
}

/// Chunk tags an [`Image`] leaves on disk: Bink videos and sounds.
///
/// **Corpus**: of the 567 MB in `Data`'s and `DataMX`'s archives, `PICT`
/// holds 184 MB and `WAVE` 156 MB, each in archives holding nothing
/// else. No load reads them; a video or a sound reads one as it plays.
/// Every other tag is parsed at load, or drawn from memory (`TILE`).
pub const DEFERRED: [&[u8; 4]; 2] = [b"PICT", b"WAVE"];

/// Which payloads an [`Image`] leaves on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defer {
    /// [`DEFERRED`]: what a drawn game's memory holds is the rest.
    Media,
    /// [`DEFERRED`] and the `TILE` pixels: a game without a screen reads
    /// the few its simulation measures, as [`Record::bytes`] (the pixels
    /// are most of an archive: the corpus's `TILE`s are 214 MB of `Data`'s
    /// and `DataMX`'s `main` and `interface` archives).
    Pixels,
}

/// One entry in a chunk's record table, borrowing its payload from the archive.
#[derive(Debug, Clone, Copy)]
pub struct Record<'a> {
    pub id: FourCc,
    name: &'a [u8],
    /// The payload's offset in the file.
    pub offset: usize,
    /// The payload; empty while it stays on disk ([`Record::bytes`]).
    pub data: &'a [u8],
    len: usize,
    /// The file a deferred payload is read from.
    disk: Option<&'a Origin>,
}

impl<'a> Record<'a> {
    /// The record's label, with NUL padding stripped.
    ///
    /// Lossy: every shipped name is ASCII, and an odd byte should not fail the
    /// load.
    pub fn name(&self) -> std::borrow::Cow<'a, str> {
        String::from_utf8_lossy(self.name)
    }

    /// The payload's length, read or not.
    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The payload: [`Record::data`], or a deferred one read from its
    /// file.
    ///
    /// # Errors
    ///
    /// If the file will not read, or changed since its index was read.
    pub fn bytes(&self) -> Result<Cow<'a, [u8]>> {
        match self.disk {
            None => Ok(Cow::Borrowed(self.data)),
            Some(origin) => origin.read(self.offset, self.len).map(Cow::Owned),
        }
    }
}

/// A tagged group of records.
#[derive(Debug, Clone)]
pub struct Chunk<'a> {
    pub tag: FourCc,
    pub flag: ChunkFlag,
    pub records: Vec<Record<'a>>,
}

/// A parsed `.cam` archive. Borrows the whole file; payloads are not copied.
#[derive(Debug, Clone)]
pub struct Archive<'a> {
    pub version: (u16, u16),
    pub chunks: Vec<Chunk<'a>>,
}

impl<'a> Archive<'a> {
    /// Parse an archive from a whole-file buffer.
    ///
    /// # Errors
    ///
    /// If the magic or [`VERSION`] is wrong, the file is truncated, or the
    /// payload heap does not start where the directory and the record tables
    /// end.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        Self::parse_in(data, None)
    }

    /// [`Archive::parse`] on `data`, or on an `image`'s bytes.
    fn parse_in(data: &'a [u8], image: Option<&'a Image>) -> Result<Self> {
        let head = slice(data, "header", 0, HEADER_LEN).map_err(|_| Error::Truncated {
            what: "cam header",
            need: HEADER_LEN,
            have: data.len(),
        })?;

        if &head[..8] != MAGIC {
            return Err(Error::BadMagic {
                what: "cam",
                found: head[..8].to_vec(),
            });
        }

        let mut r = Cursor::at_offset(head, MAGIC.len());
        let version = (r.u16("cam version major")?, r.u16("cam version minor")?);

        if version != VERSION {
            return Err(Error::BadValue {
                what: "cam version, major then minor word",
                value: u64::from(version.0) << 16 | u64::from(version.1),
            });
        }

        let count = u32_at(head, "chunk count", 0x0c)? as usize;
        let len_tables = u32_at(head, "table size", 0x10)? as usize;

        let dir_end = HEADER_LEN + DIR_ENTRY_LEN * count;
        let chunks = (0..count)
            .map(|i| Self::chunk(data, HEADER_LEN + DIR_ENTRY_LEN * i, image))
            .collect::<Result<Vec<_>>>()?;

        Self::check_layout(&chunks, dir_end, len_tables)?;

        Ok(Archive { version, chunks })
    }

    /// Read one directory entry and the chunk it points at.
    fn chunk(data: &'a [u8], entry: usize, image: Option<&'a Image>) -> Result<Chunk<'a>> {
        let e = slice(data, "directory entry", entry, DIR_ENTRY_LEN)?;
        let tag = FourCc::from_prefix(e)?;
        let at = u32_at(e, "chunk offset", 4)? as usize;

        let n = u32_at(data, "record count", at)? as usize;
        let flag = ChunkFlag::parse(u32_at(data, "chunk flags", at + 4)?)?;

        let records = (0..n)
            .map(|i| Self::record(data, at + 8 + RECORD_LEN * i, tag, image))
            .collect::<Result<Vec<_>>>()?;

        Ok(Chunk { tag, flag, records })
    }

    /// Read one 28-byte record descriptor and borrow its payload, or
    /// note where it is in an `image`'s file.
    fn record(
        data: &'a [u8],
        at: usize,
        tag: FourCc,
        image: Option<&'a Image>,
    ) -> Result<Record<'a>> {
        let r = slice(data, "record", at, RECORD_LEN)?;
        let offset = u32_at(r, "record offset", 4 + NAME_LEN)? as usize;
        let len = u32_at(r, "record length", 8 + NAME_LEN)? as usize;

        // Names are NUL-padded to a fixed width; keep only the leading text.
        let name = &r[4..4 + NAME_LEN];
        let name = &name[..name.iter().position(|b| *b == 0).unwrap_or(NAME_LEN)];

        let (data, disk) = match image {
            Some(image) if deferred(image.defer, tag, len) => {
                (image.check(offset, len)?, Some(&image.origin))
            }
            Some(image) => (slice(data, "record payload", image.at(offset), len)?, None),
            None => (slice(data, "record payload", offset, len)?, None),
        };

        Ok(Record {
            id: FourCc::from_prefix(r)?,
            name,
            offset,
            data,
            len,
            disk,
        })
    }

    /// Check that the payload heap starts `len_tables` bytes after the directory.
    fn check_layout(chunks: &[Chunk<'a>], dir_end: usize, len_tables: usize) -> Result<()> {
        let first_payload = chunks
            .iter()
            .flat_map(|c| c.records.iter())
            .map(|r| r.offset)
            .min();

        match first_payload {
            // An archive with no records at all has nothing to check.
            None => Ok(()),
            Some(heap) if heap == dir_end + len_tables => Ok(()),
            Some(heap) => Err(Error::BrokenInvariant {
                what: "payload heap start",
                expected: (dir_end + len_tables) as u64,
                found: heap as u64,
            }),
        }
    }

    /// The chunk carrying `tag`, if present.
    pub fn chunk_by_tag(&self, tag: &[u8; 4]) -> Option<&Chunk<'a>> {
        self.chunks.iter().find(|c| c.tag == tag)
    }

    /// Every record in the archive, in chunk order.
    pub fn records(&self) -> impl Iterator<Item = (&Chunk<'a>, &Record<'a>)> {
        self.chunks
            .iter()
            .flat_map(|c| c.records.iter().map(move |r| (c, r)))
    }
}

/// Whether a record of `tag` stays on disk: a [`DEFERRED`] tag, or `TILE`
/// for [`Defer::Pixels`], with bytes to read. An empty one reads as empty
/// from memory.
fn deferred(defer: Defer, tag: FourCc, len: usize) -> bool {
    len > 0 && (DEFERRED.iter().any(|d| tag == *d) || (defer == Defer::Pixels && tag == b"TILE"))
}

/// An archive file as it was when its index was read: what a deferred
/// payload is read from, if it has not changed since.
#[derive(Debug)]
pub struct Origin {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl Origin {
    /// The error reading this file.
    fn unread(&self, why: impl std::fmt::Display) -> Error {
        Error::Unread {
            path: self.path.display().to_string(),
            why: why.to_string(),
        }
    }

    /// Bytes `offset..offset + len` of the file.
    fn read(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        let mut file = File::open(&self.path).map_err(|e| self.unread(e))?;
        let meta = file.metadata().map_err(|e| self.unread(e))?;

        if meta.len() == self.len && meta.modified().ok() == self.modified {
            let mut out = vec![0; len];

            file.seek(SeekFrom::Start(offset as u64))
                .and_then(|_| file.read_exact(&mut out))
                .map_err(|e| self.unread(e))?;
            Ok(out)
        } else {
            Err(self.unread("changed since its index was read"))
        }
    }
}

/// An archive file read but its [`DEFERRED`] payloads: their records
/// read them from the file when asked ([`Record::bytes`]).
pub struct Image {
    origin: Origin,
    /// The file, the gaps cut out.
    bytes: Vec<u8>,
    /// The deferred payloads left out, as file ranges: in order, apart.
    gaps: Vec<Range<usize>>,
    defer: Defer,
}

impl Image {
    /// Read the archive at `path`: its header and tables, and every
    /// payload not [`DEFERRED`]. A file that is no archive is read
    /// whole, for [`Image::archive`] to fail on.
    ///
    /// # Errors
    ///
    /// If the file will not read.
    pub fn read(path: &Path) -> Result<Image> {
        Image::read_as(path, Defer::Media)
    }

    /// [`Image::read`], leaving on disk what `defer` says.
    ///
    /// # Errors
    ///
    /// If the file will not read.
    pub fn read_as(path: &Path, defer: Defer) -> Result<Image> {
        let unread = |e: std::io::Error| Error::Unread {
            path: path.display().to_string(),
            why: e.to_string(),
        };
        let mut file = File::open(path).map_err(unread)?;
        let meta = file.metadata().map_err(unread)?;
        let size = usize::try_from(meta.len()).unwrap_or(usize::MAX);
        let gaps = tables(&mut file, size)
            .map_err(unread)?
            .map_or_else(Vec::new, |head| gaps(&head, size, defer));
        let mut bytes = Vec::new();
        let mut at = 0;

        // What lies before, between and after the gaps.
        for gap in gaps.iter().cloned().chain(std::iter::once(size..size)) {
            let start = bytes.len();

            bytes.resize(start + gap.start - at, 0);
            file.seek(SeekFrom::Start(at as u64))
                .and_then(|_| file.read_exact(&mut bytes[start..]))
                .map_err(unread)?;
            at = gap.end;
        }

        Ok(Image {
            origin: Origin {
                path: path.to_path_buf(),
                len: meta.len(),
                modified: meta.modified().ok(),
            },
            bytes,
            gaps,
            defer,
        })
    }

    /// The archive read.
    ///
    /// # Errors
    ///
    /// As [`Archive::parse`]; also a deferred payload past the file's end.
    pub fn archive(&self) -> Result<Archive<'_>> {
        Archive::parse_in(&self.bytes, Some(self))
    }

    /// Whether the file was empty.
    pub const fn is_empty(&self) -> bool {
        self.origin.len == 0
    }

    /// Bytes held in memory: the file but its deferred payloads.
    pub const fn held(&self) -> usize {
        self.bytes.len()
    }

    /// Where file offset `offset` is in the bytes held: the gaps before
    /// it cut out.
    fn at(&self, offset: usize) -> usize {
        let cut: usize = self
            .gaps
            .iter()
            .take_while(|g| g.start < offset)
            .map(|g| g.end.min(offset) - g.start)
            .sum();

        offset - cut
    }

    /// A deferred payload must lie in the file; it holds no bytes until
    /// asked.
    fn check(&self, offset: usize, len: usize) -> Result<&'static [u8]> {
        let size = usize::try_from(self.origin.len).unwrap_or(usize::MAX);

        if offset.checked_add(len).is_some_and(|end| end <= size) {
            Ok(&[])
        } else {
            Err(Error::OutOfBounds {
                what: "record payload",
                offset,
                len,
                size,
            })
        }
    }
}

/// The file's header, directory and record tables, if it starts as an
/// archive and they fit in its `size` bytes.
fn tables(file: &mut File, size: usize) -> std::io::Result<Option<Vec<u8>>> {
    let mut head = vec![0; HEADER_LEN.min(size)];

    file.read_exact(&mut head)?;

    let word = |at: usize| u32_at(&head, "header word", at).ok().map(|w| w as usize);
    let end = head
        .starts_with(MAGIC)
        .then(|| Some(HEADER_LEN + DIR_ENTRY_LEN * word(0x0c)? + word(0x10)?))
        .flatten()
        .filter(|end| *end <= size);

    if let Some(end) = end {
        head.resize(end, 0);
        file.read_exact(&mut head[HEADER_LEN..])?;
    }

    Ok(end.map(|_| head))
}

/// The ranges an [`Image`] leaves out: the deferred payloads `head`'s
/// tables name, joined, past the tables, inside `size` bytes, and apart
/// from every payload kept. None where a table will not read: the file
/// is read whole.
fn gaps(head: &[u8], size: usize, defer: Defer) -> Vec<Range<usize>> {
    let (left, kept): (Vec<_>, Vec<_>) = spans(head)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, r)| !r.is_empty())
        .partition(|(tag, r)| deferred(defer, *tag, r.len()));
    let mut left: Vec<Range<usize>> = left.into_iter().map(|(_, r)| r).collect();
    let mut joined: Vec<Range<usize>> = Vec::new();

    left.sort_by_key(|r| r.start);

    for r in left {
        match joined.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => joined.push(r),
        }
    }

    joined
        .into_iter()
        .filter(|g| g.start >= head.len() && g.end <= size)
        .filter(|g| {
            kept.iter()
                .all(|(_, k)| k.end <= g.start || g.end <= k.start)
        })
        .collect()
}

/// Every record's tag and payload range, from the tables in `head`.
fn spans(head: &[u8]) -> Result<Vec<(FourCc, Range<usize>)>> {
    let count = u32_at(head, "chunk count", 0x0c)? as usize;
    let mut out = Vec::new();

    for entry in 0..count {
        let at = HEADER_LEN + DIR_ENTRY_LEN * entry;
        let e = slice(head, "directory entry", at, DIR_ENTRY_LEN)?;
        let tag = FourCc::from_prefix(e)?;
        let table = u32_at(e, "chunk offset", 4)? as usize;

        for record in 0..u32_at(head, "record count", table)? as usize {
            let r = slice(head, "record", table + 8 + RECORD_LEN * record, RECORD_LEN)?;
            let offset = u32_at(r, "record offset", 4 + NAME_LEN)? as usize;
            let len = u32_at(r, "record length", 8 + NAME_LEN)? as usize;

            out.push((tag, offset..offset + len));
        }
    }

    Ok(out)
}

/// One chunk to [`write()`]: its tag, flag and records, each an id, a name
/// and a payload.
#[derive(Debug, Clone)]
pub struct Written<'a> {
    pub tag: FourCc,
    pub flag: ChunkFlag,
    pub records: Vec<(FourCc, &'a str, &'a [u8])>,
}

/// An archive of `chunks`, laid out as every shipped one is: header,
/// directory, the record tables, then the payloads in record order. A
/// name past 15 bytes is cut, as the shipped directories cut
/// `flame_shield_icon` to `flame_shield_ic`; the 16th byte stays NUL.
///
/// For records made in memory: XML descriptions
/// ([`super::desc::xml`]).
///
/// # Panics
///
/// An archive past 4 GiB, whose offsets a `u32` cannot hold.
#[must_use]
pub fn write(chunks: &[Written<'_>]) -> Vec<u8> {
    let word = |n: usize| {
        u32::try_from(n)
            .expect("an archive under 4 GiB")
            .to_le_bytes()
    };
    let tables: usize = chunks
        .iter()
        .map(|c| 8 + RECORD_LEN * c.records.len())
        .sum();
    let dir_end = HEADER_LEN + DIR_ENTRY_LEN * chunks.len();
    let mut out = Vec::new();

    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.0.to_le_bytes());
    out.extend_from_slice(&VERSION.1.to_le_bytes());
    out.extend_from_slice(&word(chunks.len()));
    out.extend_from_slice(&word(tables));

    // Directory: each chunk's table follows the one before.
    let mut table_at = dir_end;

    for chunk in chunks {
        out.extend_from_slice(chunk.tag.as_bytes());
        out.extend_from_slice(&word(table_at));
        table_at += 8 + RECORD_LEN * chunk.records.len();
    }

    // Tables, the payloads placed back to back after the last.
    let mut payload_at = dir_end + tables;

    for chunk in chunks {
        out.extend_from_slice(&word(chunk.records.len()));
        out.extend_from_slice(&word(usize::from(chunk.flag == ChunkFlag::Set)));

        for (id, name, payload) in &chunk.records {
            let mut padded = [0; NAME_LEN];
            let kept = name.len().min(NAME_LEN - 1);

            padded[..kept].copy_from_slice(&name.as_bytes()[..kept]);
            out.extend_from_slice(id.as_bytes());
            out.extend_from_slice(&padded);
            out.extend_from_slice(&word(payload_at));
            out.extend_from_slice(&word(payload.len()));
            payload_at += payload.len();
        }
    }

    for chunk in chunks {
        for (_, _, payload) in &chunk.records {
            out.extend_from_slice(payload);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_archive_reads_back() {
        let chunks = [
            Written {
                tag: FourCc::new(b"DUNT"),
                flag: ChunkFlag::Clear,
                records: vec![
                    (FourCc::new(b"ABE1"), "flame_shield_icon", b"one"),
                    (FourCc::new(b"ABE2"), "two", b""),
                ],
            },
            Written {
                tag: FourCc::new(b"DACT"),
                flag: ChunkFlag::Clear,
                records: vec![(FourCc::new(b"AXA6"), "arrow", b"three")],
            },
        ];
        let bytes = write(&chunks);
        let archive = Archive::parse(&bytes).expect("a valid archive");
        let records: Vec<_> = archive
            .records()
            .map(|(c, r)| (c.tag, r.id, r.name().into_owned(), r.data.to_vec()))
            .collect();

        assert_eq!(records.len(), 3);
        assert_eq!(records[0].2, "flame_shield_ic", "cut to 15 bytes");
        assert_eq!(records[0].3, b"one");
        assert_eq!(records[1].3.len(), 0);
        assert_eq!(records[2].0, FourCc::new(b"DACT"));
        assert_eq!(records[2].3, b"three");
    }

    /// A record's id and payload.
    type Made = (&'static [u8; 4], &'static [u8]);

    /// An archive of one chunk per tag, each record an id and a payload.
    fn archive(chunks: &[(&[u8; 4], &[Made])]) -> Vec<u8> {
        let chunks: Vec<Written<'_>> = chunks
            .iter()
            .map(|(tag, records)| Written {
                tag: FourCc::new(tag),
                flag: ChunkFlag::Clear,
                records: records
                    .iter()
                    .map(|(id, payload)| (FourCc::new(id), "record", *payload))
                    .collect(),
            })
            .collect();

        write(&chunks)
    }

    /// `bytes` written as file `name` of a scratch folder.
    fn on_disk(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join("regent-cam-image");
        let path = dir.join(name);

        std::fs::create_dir_all(&dir).expect("a scratch folder");
        std::fs::write(&path, bytes).expect("written");
        path
    }

    /// Unit, sound, action, video: payloads in that order in the file.
    fn mixed() -> Vec<u8> {
        archive(&[
            (b"DUNT", &[(b"AAA1", b"unit")]),
            (b"WAVE", &[(b"SND1", b"sound bytes"), (b"SND2", b"")]),
            (b"DACT", &[(b"ACT1", b"action")]),
            (b"PICT", &[(b"VID1", b"video bytes")]),
        ])
    }

    #[test]
    fn an_image_holds_all_but_its_videos_and_sounds() {
        let bytes = mixed();
        let path = on_disk("held.cam", &bytes);
        let image = Image::read(&path).expect("read");
        let archive = image.archive().expect("an archive");
        let record = |tag: &[u8; 4], at: usize| archive.chunk_by_tag(tag).unwrap().records[at];

        assert_eq!(
            image.held(),
            bytes.len() - b"sound bytes".len() - b"video bytes".len()
        );
        assert_eq!(record(b"DUNT", 0).data, b"unit");
        assert_eq!(record(b"DACT", 0).data, b"action", "past a gap");
        assert_eq!(record(b"WAVE", 0).data, b"", "left on disk");
        assert_eq!(record(b"WAVE", 0).len(), b"sound bytes".len());
        assert_eq!(*record(b"WAVE", 0).bytes().unwrap(), *b"sound bytes");
        assert_eq!(*record(b"PICT", 0).bytes().unwrap(), *b"video bytes");
        assert!(record(b"WAVE", 1).is_empty(), "an empty record stays empty");

        // Gone from disk: what is held still reads, a deferred one fails.
        std::fs::remove_file(&path).expect("removed");
        assert_eq!(*record(b"DACT", 0).bytes().unwrap(), *b"action");
        assert_eq!(*record(b"WAVE", 1).bytes().unwrap(), *b"");
        assert!(matches!(
            record(b"WAVE", 0).bytes(),
            Err(Error::Unread { .. })
        ));
    }

    #[test]
    fn pixels_stay_on_disk_when_asked() {
        let bytes = archive(&[
            (b"DUNT", &[(b"AAA1", b"unit")]),
            (b"TILE", &[(b"0000", b"pixels"), (b"0001", b"")]),
            (b"DACT", &[(b"ACT1", b"action")]),
        ]);
        let path = on_disk("pixels.cam", &bytes);
        let image = Image::read_as(&path, Defer::Pixels).expect("read");
        let archive = image.archive().expect("an archive");
        let record = |tag: &[u8; 4], at: usize| archive.chunk_by_tag(tag).unwrap().records[at];

        assert_eq!(image.held(), bytes.len() - b"pixels".len());
        assert_eq!(record(b"TILE", 0).data, b"");
        assert_eq!(*record(b"TILE", 0).bytes().unwrap(), *b"pixels");
        assert_eq!(record(b"DACT", 0).data, b"action");
        assert_eq!(
            Image::read(&path).expect("read").held(),
            bytes.len(),
            "held by default"
        );
    }

    #[test]
    fn a_file_changed_after_its_index_is_an_error() {
        let bytes = mixed();
        let path = on_disk("changed.cam", &bytes);
        let image = Image::read(&path).expect("read");
        let archive = image.archive().expect("an archive");
        let sound = archive.chunk_by_tag(b"WAVE").unwrap().records[0];

        on_disk("changed.cam", &bytes[..bytes.len() / 2]);
        assert!(
            matches!(sound.bytes(), Err(Error::Unread { .. })),
            "shorter"
        );

        on_disk("changed.cam", &[&bytes[..], b"more"].concat());
        assert!(matches!(sound.bytes(), Err(Error::Unread { .. })), "longer");
    }

    #[test]
    fn a_short_file_is_an_error_at_load() {
        let bytes = mixed();

        // Cut inside the video, then inside the action.
        for cut in [b"video".len() + 1, b"video bytes".len() + 2] {
            let path = on_disk("short.cam", &bytes[..bytes.len() - cut]);
            let image = Image::read(&path).expect("read");

            assert!(
                matches!(image.archive(), Err(Error::OutOfBounds { .. })),
                "cut {cut}"
            );
        }

        let path = on_disk("empty.cam", b"");

        assert!(Image::read(&path).expect("read").is_empty());
    }

    #[test]
    fn a_later_image_overrides_by_name_and_its_empty_record_wins() {
        let first = archive(&[(b"WAVE", &[(b"SND1", b"first"), (b"SND2", b"full")])]);
        let second = archive(&[(b"WAVE", &[(b"SND1", b"second"), (b"SND2", b"")])]);
        let images = [
            Image::read(&on_disk("first.cam", &first)).expect("read"),
            Image::read(&on_disk("second.cam", &second)).expect("read"),
        ];
        let catalogue = crate::data::catalogue::Catalogue::of(
            images
                .iter()
                .zip(["first.cam", "second.cam"])
                .map(|(image, name)| (name, image.archive().expect("an archive"))),
        );
        let found = |id: &[u8; 4]| {
            let at = catalogue.find(b"WAVE", FourCc::new(id)).expect("found");

            (
                at.archive,
                catalogue
                    .record(b"WAVE", at)
                    .unwrap()
                    .bytes()
                    .unwrap()
                    .into_owned(),
            )
        };

        assert_eq!(found(b"SND1"), (1, b"second".to_vec()));
        assert_eq!(found(b"SND2"), (1, Vec::new()), "no archive is sparse");
    }
}
