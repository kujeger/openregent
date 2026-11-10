//! A dataset's archives, and an index finding records across them.
//!
//! Lookups span the dataset ([`crate::content::dataset`]): none of the 583 `DUNT`
//! records names an `IMAG` in its own archive.
//!
//! Format rules:
//!
//! - An id is unique within a chunk, not across archives (18 `IMAG` ids
//!   repeat). [`Catalogue::find`] applies load order; [`Catalogue::all`] shows
//!   the collision.
//! - `TILE` and `SPLT` are referenced by number within the referring
//!   archive, so [`Catalogue::positional`] takes an archive. They stay out of
//!   the id index: a flagged chunk's ids are positions, so every archive has a
//!   tile 0.
//! - No archive is sparse. An empty record is found like a full one, and
//!   no lookup falls back to another archive (**Binary**,
//!   [`Catalogue::find`], [`Catalogue::positional`]). Phantoms Haunt's
//!   16,763 empty `TILE` records are padding that none of its frames name.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::data::cam;
use crate::data::desc::{self, UnitKind};
use crate::data::error::Result;
use crate::data::fourcc::FourCc;

/// Numbers below this index a flagged chunk directly; a number with any
/// bit of its top byte set is looked up by id.
const DIRECT_LIMIT: usize = 1 << 24;

/// A record's archive and position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Located {
    /// Index into [`Catalogue::archives`] (load order).
    pub archive: usize,
    /// Index into the chunk; also the record number for `TILE` and `SPLT`.
    pub record: usize,
}

/// Archive bytes, owned so records can borrow them.
///
/// Split from [`Catalogue`] so only this half touches the filesystem; tests
/// build catalogues from in-memory bytes. One definition's list reads by
/// [`Dataset::library`](crate::content::dataset::Dataset::library).
#[derive(Debug, Default)]
pub struct Library {
    pub(crate) loaded: Vec<(String, Vec<u8>)>,
    /// Archives named by the dataset but absent.
    ///
    /// Empty on HD (all 34 present). Not fatal, for non-target editions.
    pub missing: Vec<String>,
    /// Archives present but zero-byte. Separate from [`Library::missing`].
    pub empty: Vec<String>,
}

impl Library {
    /// Read an explicit list of archives, in order: an install's composed
    /// dataset ([`Install::files`]).
    ///
    /// Order matters: [`Catalogue::find`] takes the last of a collision.
    ///
    /// [`Install::files`]: crate::content::dataset::Install::files
    ///
    /// # Errors
    ///
    /// If a path will not read. Zero-byte files go to [`Library::empty`].
    pub fn from_files(paths: &[PathBuf]) -> std::io::Result<Self> {
        let mut out = Library::default();

        for path in paths {
            let name = path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into(),
            );
            let bytes = std::fs::read(path)?;

            if bytes.is_empty() {
                out.empty.push(name);
            } else {
                out.loaded.push((name, bytes));
            }
        }

        Ok(out)
    }

    /// Index everything that loaded.
    ///
    /// # Errors
    ///
    /// If an archive will not parse. Every shipped one does.
    pub fn catalogue(&self) -> Result<Catalogue<'_>> {
        Catalogue::build(&self.loaded)
    }

    pub const fn len(&self) -> usize {
        self.loaded.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.loaded.is_empty()
    }
}

/// Parsed archives with an index over every record.
#[derive(Debug)]
pub struct Catalogue<'a> {
    /// Load order: the tie-break for shared ids.
    pub archives: Vec<(&'a str, cam::Archive<'a>)>,
    index: BTreeMap<(FourCc, FourCc), Vec<Located>>,
}

impl<'a> Catalogue<'a> {
    /// Parse and index archives, given in load order.
    ///
    /// # Errors
    ///
    /// If any archive fails to parse.
    pub fn build(archives: &'a [(String, Vec<u8>)]) -> Result<Self> {
        Catalogue::parse(archives.iter().map(|(n, b)| (n.as_str(), b.as_slice())))
    }

    /// [`Catalogue::build`] on archives held elsewhere, by name and bytes.
    ///
    /// # Errors
    ///
    /// If any archive fails to parse.
    pub fn parse(archives: impl IntoIterator<Item = (&'a str, &'a [u8])>) -> Result<Self> {
        let parsed = archives
            .into_iter()
            .map(|(name, bytes)| Ok((name, cam::Archive::parse(bytes)?)))
            .collect::<Result<Vec<_>>>()?;

        Ok(Catalogue::of(parsed))
    }

    /// Index archives already parsed, given in load order: an
    /// [`Image`](cam::Image)'s among them.
    pub fn of(archives: impl IntoIterator<Item = (&'a str, cam::Archive<'a>)>) -> Self {
        let mut out = Catalogue {
            archives: Vec::new(),
            index: BTreeMap::new(),
        };

        for (name, archive) in archives {
            let at = out.archives.len();

            // Per chunk, not `Archive::records`: positions are per chunk.
            for chunk in &archive.chunks {
                // A flagged chunk's ids are positions 0, 1, 2…; indexing them
                // would collide 2,624 ways on `TILE` alone. Use `positional`.
                if chunk.flag == cam::ChunkFlag::Set {
                    continue;
                }

                for (record, entry) in chunk.records.iter().enumerate() {
                    out.index
                        .entry((chunk.tag, entry.id))
                        .or_default()
                        .push(Located {
                            archive: at,
                            record,
                        });
                }
            }

            out.archives.push((name, archive));
        }

        out
    }

    /// Location of the record with this tag and id.
    ///
    /// Last loaded wins. **Binary**: `CYLibManager` appends each
    /// archive opened, and `CYLibManager`, asked with no library,
    /// tries them from the end; the first holding the tag and id answers.
    /// An empty record answers too: its load fails and no other archive
    /// is asked. [`Catalogue::all`] lists every copy.
    pub fn find(&self, tag: &[u8; 4], id: FourCc) -> Option<Located> {
        self.all(tag, id).last().copied()
    }

    /// Every record with this tag and id, in load order.
    pub fn all(&self, tag: &[u8; 4], id: FourCc) -> &[Located] {
        self.index
            .get(&(FourCc(*tag), id))
            .map_or(&[], Vec::as_slice)
    }

    /// The record at a location.
    pub fn record(&self, tag: &[u8; 4], at: Located) -> Option<&cam::Record<'a>> {
        self.archives
            .get(at.archive)?
            .1
            .chunk_by_tag(tag)?
            .records
            .get(at.record)
    }

    /// Record `index` of a chunk in one archive: how `TILE` and `SPLT`
    /// references resolve. An `IMAG`'s frames name tiles in the archive
    /// holding the `IMAG` (imag load keeps it), a tile its palette
    /// in its own.
    ///
    /// **Binary**: in a flagged chunk the number
    /// under 2^24 is the position, the stored id unread; past the end,
    /// none. Otherwise the last record whose id is the number: Phantoms
    /// Haunt's unflagged `SPLT` chunk. An empty record is returned like
    /// any other; HD's loader would read on past it (lib read
    /// ignores the length), but nothing names one.
    pub fn positional(
        &self,
        archive: usize,
        tag: &[u8; 4],
        index: usize,
    ) -> Option<&cam::Record<'a>> {
        let chunk = self.archives.get(archive)?.1.chunk_by_tag(tag)?;

        if chunk.flag == cam::ChunkFlag::Set && index < DIRECT_LIMIT {
            chunk.records.get(index)
        } else {
            let id = FourCc::from_word(u32::try_from(index).ok()?);

            chunk.records.iter().rev().find(|r| r.id == id)
        }
    }

    /// Every record with this tag, in load order. Walks chunks, not the index,
    /// so flagged chunks are included.
    pub fn tagged(&self, tag: &[u8; 4]) -> impl Iterator<Item = (Located, &cam::Record<'a>)> {
        self.archives
            .iter()
            .enumerate()
            .filter_map(move |(archive, (_, a))| Some((archive, a.chunk_by_tag(tag)?)))
            .flat_map(|(archive, chunk)| {
                chunk
                    .records
                    .iter()
                    .enumerate()
                    .map(move |(record, r)| (Located { archive, record }, r))
            })
    }

    /// Record count of one archive's chunk, 0 if absent. Bounds positional
    /// references, so out-of-range ones are reportable.
    pub fn chunk_len(&self, archive: usize, tag: &[u8; 4]) -> usize {
        self.archives
            .get(archive)
            .and_then(|(_, a)| a.chunk_by_tag(tag))
            .map_or(0, |c| c.records.len())
    }

    /// Ids held by more than one archive, with each copy's location. Each
    /// relies on the unconfirmed last-wins rule in [`Catalogue::find`].
    pub fn ambiguous(&self) -> impl Iterator<Item = (FourCc, FourCc, &[Located])> {
        self.index
            .iter()
            .filter(|(_, at)| at.len() > 1)
            .map(|((tag, id), at)| (*tag, *id, at.as_slice()))
    }

    /// Unit `id`'s subtype, off its `DUNT` descriptor alone: no art or
    /// tail resolved. `None` without a descriptor, or one that will not
    /// read.
    pub fn unit_kind(&self, id: FourCc) -> Option<UnitKind> {
        let record = self.record(b"DUNT", self.find(b"DUNT", id)?)?;
        let part = desc::Part::parse(record.data).ok()?;

        desc::Unit::parse(&part).ok().map(|unit| unit.kind())
    }

    /// Total records in the dataset.
    pub fn records(&self) -> usize {
        self.archives.iter().map(|(_, a)| a.records().count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal archive: header and one unflagged chunk of `ids.len()`
    /// records.
    fn archive(tag: [u8; 4], ids: &[&[u8; 4]]) -> Vec<u8> {
        archive_flagged(tag, ids, cam::ChunkFlag::Clear)
    }

    /// As [`archive`], the chunk flag given.
    fn archive_flagged(tag: [u8; 4], ids: &[&[u8; 4]], flag: cam::ChunkFlag) -> Vec<u8> {
        let dir_end = 0x14 + 8;
        let table = 8 + 28 * ids.len();
        let mut out = Vec::new();

        out.extend_from_slice(cam::MAGIC);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&u32::try_from(table).unwrap().to_le_bytes());
        out.extend_from_slice(&tag);
        out.extend_from_slice(&u32::try_from(dir_end).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(ids.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::from(flag == cam::ChunkFlag::Set).to_le_bytes());

        for (i, id) in ids.iter().enumerate() {
            out.extend_from_slice(*id);
            out.extend_from_slice(&[0; 16]);
            out.extend_from_slice(&u32::try_from(dir_end + table + i).unwrap().to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
        }

        out.extend((0..ids.len()).map(|i| u8::try_from(i).unwrap()));
        out
    }

    /// Fixture archive: name, chunk tag, record ids.
    type Fixture<'a> = (&'a str, [u8; 4], &'a [&'a [u8; 4]]);

    fn library(entries: &[Fixture<'_>]) -> Vec<(String, Vec<u8>)> {
        entries
            .iter()
            .map(|(name, tag, ids)| ((*name).to_owned(), archive(*tag, ids)))
            .collect()
    }

    #[test]
    fn a_record_is_found_across_archives() {
        // Reference in one archive, record in another, like every real lookup.
        let bytes = library(&[
            ("unittype.cam", *b"DUNT", &[b"AVB1"]),
            ("maindata.cam", *b"IMAG", &[b"AVB1"]),
        ]);
        let cat = Catalogue::build(&bytes).expect("two archives");

        let at = cat.find(b"IMAG", FourCc::new(b"AVB1")).expect("the art");

        assert_eq!(
            at,
            Located {
                archive: 1,
                record: 0
            }
        );
        assert!(cat.find(b"IMAG", FourCc::new(b"ZZZZ")).is_none());
    }

    #[test]
    fn a_repeated_id_resolves_to_the_later_archive() {
        // As 18 IMAG ids do. Last wins; both stay visible.
        let bytes = library(&[
            ("maindata.cam", *b"IMAG", &[b"AVB1"]),
            ("mx_maindata.cam", *b"IMAG", &[b"AVB1"]),
        ]);
        let cat = Catalogue::build(&bytes).expect("two archives");

        assert_eq!(cat.all(b"IMAG", FourCc::new(b"AVB1")).len(), 2);
        assert_eq!(cat.find(b"IMAG", FourCc::new(b"AVB1")).unwrap().archive, 1);
        assert_eq!(cat.ambiguous().count(), 1);
    }

    #[test]
    fn a_positional_reference_needs_its_archive() {
        // Two archives, three tiles each.
        let bytes = library(&[
            (
                "maindata.cam",
                *b"TILE",
                &[b"\0\0\0\0", b"\x01\0\0\0", b"\x02\0\0\0"],
            ),
            (
                "mx_maindata.cam",
                *b"TILE",
                &[b"\0\0\0\0", b"\x01\0\0\0", b"\x02\0\0\0"],
            ),
        ]);
        let cat = Catalogue::build(&bytes).expect("two archives");

        assert_eq!(cat.positional(0, b"TILE", 1).unwrap().data, [1]);
        assert_eq!(cat.positional(1, b"TILE", 1).unwrap().data, [1]);
        assert!(cat.positional(0, b"TILE", 3).is_none(), "past the end");
        assert!(cat.positional(9, b"TILE", 0).is_none(), "no such archive");
    }

    #[test]
    fn a_flagged_chunk_is_indexed_by_position_not_id() {
        // Ids out of order: the position answers, as lib entry by id.
        let bytes = vec![(
            "maindata.cam".to_owned(),
            archive_flagged(
                *b"TILE",
                &[b"\x02\0\0\0", b"\x00\0\0\0", b"\x01\0\0\0"],
                cam::ChunkFlag::Set,
            ),
        )];
        let cat = Catalogue::build(&bytes).expect("one archive");

        assert_eq!(cat.positional(0, b"TILE", 0).unwrap().data, [0]);
        assert_eq!(cat.positional(0, b"TILE", 2).unwrap().data, [2]);
        assert!(cat.positional(0, b"TILE", 3).is_none(), "past the end");
    }

    #[test]
    fn an_unflagged_chunk_is_indexed_by_its_last_matching_id() {
        // As Phantoms Haunt's SPLT chunk, but with ids out of order.
        let bytes = library(&[(
            "phantom_maindata.cam",
            *b"SPLT",
            &[b"\x01\0\0\0", b"\x00\0\0\0", b"\x01\0\0\0"],
        )]);
        let cat = Catalogue::build(&bytes).expect("one archive");

        assert_eq!(cat.positional(0, b"SPLT", 0).unwrap().data, [1]);
        assert_eq!(cat.positional(0, b"SPLT", 1).unwrap().data, [2]);
        assert!(cat.positional(0, b"SPLT", 2).is_none(), "no such id");
    }

    #[test]
    fn the_index_covers_every_record() {
        let bytes = library(&[("a.cam", *b"DUNT", &[b"AAA1", b"AAA2", b"AAA3"])]);
        let cat = Catalogue::build(&bytes).expect("one archive");

        assert_eq!(cat.records(), 3);
        for (i, id) in [b"AAA1", b"AAA2", b"AAA3"].into_iter().enumerate() {
            let at = cat.find(b"DUNT", FourCc::new(id)).expect("indexed");
            assert_eq!(at.record, i, "position is the record's own number");
        }
    }
}
