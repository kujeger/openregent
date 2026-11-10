//! How the game stores bytes: containers, payloads and framing.
//!
//! Strict, zero-copy readers. An unexpected value fails the parse, so a
//! failure is a finding.
//!
//! [`block`] frames the blocks of `.bcd` files ([`bcd`]) and saves' GPL
//! chunk; [`crate::gpl`] reads the contents.
//!
//! Also the record index across a dataset's archives, [`catalogue`], and
//! the readers that need it: [`text`], [`desc::xml`], [`build_tree`];
//! the value types formats store: [`guid`], [`attrib`], [`stat`];
//! and GPL's value encodings, [`gpltype`].
//! The fixed-point types are [`crate::base`]'s. Uses only that layer.
//!
//! Not here: finding an install's files, and joins over several records
//! (a unit and its art), are [`crate::content`].
//!
//! Readers must also accept `.cam` archives from two Steam Workshop mods,
//! written by a third-party tool (the HD SDK ships no packer). See
//! `tests/mods.rs`.
//!
//! ```no_run
//! let bytes = std::fs::read("data/terrtype.cam")?;
//! let archive = regent_core::data::cam::Archive::parse(&bytes)?;
//!
//! for (chunk, record) in archive.records() {
//!     println!("{} {} {} bytes", chunk.tag, record.name(), record.len());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! ## What is understood
//!
//! | Format | State |
//! |---|---|
//! | `.cam` container | Solved |
//! | `WAVE` payloads | Solved — plain RIFF/WAVE |
//! | `PALT` / `SPLT` payloads | Solved — 256-entry palettes |
//! | `STRT` payloads | Solved — counted string tables, Windows-1250, keyed from version 2 |
//! | `TILE` payloads | Solved — see [`tile`]; all three versions |
//! | `D*` descriptor payloads | Tree solved — see [`desc`]; unit and action fields too |
//! | `.bcd` block framing | Solved — see [`crate::gpl::bcd`] for the contents |
//! | `.rgs`, `.q` | Solved — see [`rgs`]: five charts, and 36 quests across three versions |
//! | `.qdd` | Solved — plain text |
//! | `IMAG` | Solved — see [`imag`]. Animation metadata over `TILE`, not pixels |
//! | `DATA` | Solved — [`settings`] reads `GOPT`, [`deps`] the two dependency tables |
//! | `CUT ` | Frames of two FLIC animations, with the artist's build paths |
//! | `BTDT` | Build-tree variants: a name, and buildings with a cap each; `.BTD` files the same ([`build_tree`]) |
//! | `PICT` | A kind word, then an unmodified Bink video or TGA |
//! | `SMNU` | Framing solved — see [`smnu`]; the keys are not named |
//! | `FONT` | Solved — see [`font`]: a bitmap font, a bit a pixel |
//!
//! `PICT`: see `docs/formats/payloads.md`.

pub mod attrib;
pub mod bcd;
pub mod block;
pub mod build_tree;
pub mod cam;
pub mod catalogue;
pub mod cp1250;
pub mod crc;
pub mod cursor;
pub mod deps;
pub mod desc;
pub mod error;
pub mod fnv;
pub mod font;
pub mod fourcc;
pub mod gpltype;
pub mod guid;
pub mod imag;
pub mod payload;
pub mod qdd;
pub mod rgs;
pub mod settings;
pub mod smnu;
pub mod stat;
pub mod text;
pub mod tile;
pub mod xml;

pub use error::Error;
pub use fourcc::FourCc;

/// A guess at a file's type, from its leading bytes alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Cam,
    Bcd,
    RgsChart,
    RgsMap,
    Unknown,
}

/// Identify a file by its magic, without fully parsing it.
///
/// `.bcd` has no magic; it is guessed from a successful parse.
pub fn sniff(data: &[u8]) -> FileKind {
    if data.starts_with(cam::MAGIC) {
        FileKind::Cam
    } else if data.starts_with(b"RGM") {
        FileKind::RgsMap
    } else if data.starts_with(b"RGC") {
        FileKind::RgsChart
    } else if bcd::Container::parse(data).is_ok() {
        FileKind::Bcd
    } else {
        FileKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniff_rejects_junk() {
        assert_eq!(sniff(b""), FileKind::Unknown);
        assert_eq!(sniff(b"not a majesty file at all"), FileKind::Unknown);
    }

    #[test]
    fn cam_rejects_bad_magic() {
        let err = cam::Archive::parse(&[0u8; 64]).unwrap_err();
        assert!(matches!(err, Error::BadMagic { .. }));
    }

    #[test]
    fn cam_rejects_truncated_header() {
        let err = cam::Archive::parse(b"CYLB").unwrap_err();
        assert!(matches!(err, Error::Truncated { .. }));
    }

    #[test]
    fn palette_rejects_wrong_length() {
        let err = payload::Palette::parse(&[0u8; 100]).unwrap_err();
        assert!(matches!(err, Error::BadValue { .. }));
    }

    #[test]
    fn a_version_zero_record_is_a_bare_string() {
        // count 2, version 0 (u16 offsets), offsets 8 and 15, then two bare
        // strings.
        let mut p = vec![2, 0, 0, 0, 8, 0, 15, 0];
        p.extend_from_slice(b"ACCEPT\0");
        p.extend_from_slice(b"CANCEL\0");

        let t = payload::StringTable::parse(&p).unwrap();

        assert_eq!(t.form, payload::Form::Bare);
        assert_eq!(t.len(), 2);
        assert_eq!(t.get(0).unwrap(), "ACCEPT");
        assert_eq!(t.get(1).unwrap(), "CANCEL");
        assert_eq!(t.key(0), None);
    }

    #[test]
    fn a_version_two_record_carries_a_key_before_its_string() {
        // count 2, version 2 (u32 offsets), first record at 4 + 8 = 12. One
        // four-character key, one index key.
        let mut p = vec![2, 0, 0, 2];
        p.extend_from_slice(&12u32.to_le_bytes());
        p.extend_from_slice(&26u32.to_le_bytes());
        p.extend_from_slice(b"h001- Repairs\0");
        p.extend_from_slice(&1u32.to_le_bytes());
        p.extend_from_slice(b"OK\0");

        let t = payload::StringTable::parse(&p).unwrap();

        assert_eq!(t.form, payload::Form::Keyed);
        assert_eq!(t.get(0).unwrap(), "- Repairs");
        assert_eq!(t.get(1).unwrap(), "OK");
        assert_eq!(t.key(0), Some(FourCc::new(b"h001")));
        assert_eq!(t.key(1), Some(FourCc::from_word(1)));
        assert_eq!(t.get_by_key(FourCc::new(b"h001")).unwrap(), "- Repairs");
        assert_eq!(t.get_by_key(FourCc::new(b"nope")), None);
    }

    #[test]
    fn an_index_key_would_truncate_the_string_if_it_were_not_read() {
        // Index 0 is four NUL bytes; a bare read yields "" (as for every AITX
        // advisor line and AP07 caption).
        let mut p = vec![1, 0, 0, 2];
        p.extend_from_slice(&8u32.to_le_bytes());
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(b"PALACE\0");

        let t = payload::StringTable::parse(&p).unwrap();

        assert_eq!(t.get(0).unwrap(), "PALACE");
    }

    #[test]
    fn string_table_header_is_two_separate_bytes() {
        // Encoding (+2) and version (+3) vary independently; a single u16 read
        // would reject Unicode-plus-keyed.
        let t = payload::StringTable::parse(&[0, 0, 8, 2]).unwrap();

        assert_eq!(t.encoding, payload::Encoding::Unicode);
        assert_eq!(t.form, payload::Form::Keyed);
    }

    #[test]
    fn unicode_strings_are_refused_rather_than_guessed() {
        // No sample exists, so character width is unknown.
        let mut p = vec![1, 0, 8, 0, 6, 0];
        p.extend_from_slice(b"O\0K\0\0\0");

        let t = payload::StringTable::parse(&p).unwrap();

        assert_eq!(t.len(), 1);
        assert!(t.get(0).is_none());
    }

    #[test]
    fn strings_decode_as_windows_1250() {
        // 0xe8 is `c` with caron; invalid as UTF-8.
        let mut p = vec![1, 0, 0, 0, 6, 0];
        p.extend_from_slice(b"\xe8ep\0");

        let t = payload::StringTable::parse(&p).unwrap();

        assert_eq!(t.get(0).unwrap(), "\u{010d}ep");
    }

    #[test]
    fn string_table_rejects_unknown_header_bytes() {
        let bad_encoding = payload::StringTable::parse(&[1, 0, 0x99, 0, 8, 0]).unwrap_err();
        let bad_version = payload::StringTable::parse(&[1, 0, 0, 0x99, 8, 0]).unwrap_err();

        assert!(matches!(bad_encoding, Error::BadValue { .. }));
        assert!(matches!(bad_version, Error::BadValue { .. }));
    }

    #[test]
    fn a_cam_of_another_version_is_refused() {
        let mut head = cam::MAGIC.to_vec();
        head.extend_from_slice(&[2, 0, 1, 0]);
        head.extend_from_slice(&[0; 8]);

        assert!(matches!(
            cam::Archive::parse(&head),
            Err(Error::BadValue { .. })
        ));
    }

    #[test]
    fn every_string_offset_is_checked_not_only_the_first() {
        // Two bare records at 8 and 11; the second's offset then moved past
        // the end, then before the first.
        let mut p = vec![2, 0, 0, 0, 8, 0, 11, 0];
        p.extend_from_slice(b"OK\0NO\0");

        assert!(payload::StringTable::parse(&p).is_ok());

        p[6] = 99;
        assert!(payload::StringTable::parse(&p).is_err());

        p[6] = 7;
        assert!(payload::StringTable::parse(&p).is_err());
    }

    #[test]
    fn empty_string_table_is_accepted() {
        let t = payload::StringTable::parse(&[0, 0, 0, 0]).unwrap();
        assert!(t.is_empty());
    }

    #[test]
    fn wave_requires_riff_and_wave() {
        assert!(payload::wave(b"RIFF\0\0\0\0WAVEfmt ").is_ok());
        assert!(payload::wave(b"RIFF\0\0\0\0AVI fmt ").is_err());
    }

    #[test]
    fn qdd_groups_entries_under_each_name() {
        let text = "\
Name = \r\n[FIRST]\r\n\r\nShort Description = \r\n[one]\r\n\r\n\
Name = \r\n[SECOND]  // trailing comment\r\n\r\nShort Description = \r\n[two\r\nand more]\r\n";

        let quests = qdd::parse(text).unwrap();

        assert_eq!(quests.len(), 2);
        assert_eq!(quests[0].id, "FIRST");
        assert_eq!(quests[0].field("Short Description"), Some("one"));
        assert_eq!(quests[1].id, "SECOND");
        assert_eq!(quests[1].field("Short Description"), Some("two\nand more"));
    }

    #[test]
    fn fourcc_keeps_significant_trailing_space() {
        let tag = FourCc::new(b"CUT ");

        assert_eq!(tag.to_string(), "CUT ");
        assert_eq!(tag.trimmed(), "CUT");
    }
}
