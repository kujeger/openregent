//! `docs/formats/README.md`'s structural claims, asserted against the shipped files.
//!
//! Needs `MAJESTY_HD`, from the environment
//! or `.env`. A missing corpus panics.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regent_core::content::imagery;
use regent_core::content::imagery::FrameSource;
use regent_core::data::catalogue::Catalogue;
use regent_core::data::imag::Blit;
use regent_core::data::imag::Slot;
use regent_core::data::{FileKind, FourCc, bcd, cam, desc, imag, payload, qdd, rgs, sniff, tile};
use regent_core::gpl::bcd::DataSection;
use regent_core::view::raster::shade;
use regent_core::view::{Backdrop, Colour, Ink, Sheet, Sprite};

/// Every non-empty `.cam` under `data/` and `datamx/`.
fn archives(root: &Path) -> Vec<PathBuf> {
    ["Data", "DataMX"]
        .iter()
        .flat_map(|dir| common::archives(&root.join(dir)))
        .collect()
}

#[test]
fn payloads_lie_inside_the_file_and_are_ordered() {
    let root = common::hd();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        // Back to back, in record order.
        for chunk in &archive.chunks {
            for pair in chunk.records.windows(2) {
                assert_eq!(
                    pair[0].offset + pair[0].len(),
                    pair[1].offset,
                    "{} {} payloads not contiguous",
                    path.display(),
                    chunk.tag
                );
            }
        }
    }
}

/// `PICT` bytes in `Data`'s and `DataMX`'s archives (`cam::DEFERRED`).
const VIDEO_BYTES: usize = 183_651_004;

/// `WAVE` bytes in them.
const SOUND_BYTES: usize = 156_393_376;

/// An image of each archive holds every record the whole file does, byte
/// for byte, its videos and sounds read from the file as asked; it holds
/// the rest alone.
#[test]
fn an_image_reads_every_record_as_the_whole_file_does() {
    let root = common::hd();
    let (mut files, mut held) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let whole = cam::Archive::parse(&data).unwrap();
        let image = cam::Image::read(&path).unwrap();
        let read = image.archive().unwrap();
        let pairs = whole.records().zip(read.records());

        for ((chunk, a), (_, b)) in pairs {
            assert_eq!((a.id, a.name(), a.offset), (b.id, b.name(), b.offset));
            assert_eq!(*b.bytes().unwrap(), *a.data, "{} {}", path.display(), a.id);
            let left = cam::DEFERRED.contains(&chunk.tag.as_bytes()) && !a.data.is_empty();

            assert_eq!(
                b.data.is_empty() && !a.data.is_empty(),
                left,
                "{}",
                chunk.tag
            );
        }

        assert_eq!(whole.records().count(), read.records().count());
        files += data.len();
        held += image.held();
    }

    assert_eq!(files - held, VIDEO_BYTES + SOUND_BYTES);
}

/// An image read for a game without a screen holds no `TILE` pixels, and
/// each reads from the file as the whole file has it.
#[test]
fn a_pixelless_image_reads_its_tiles_from_the_file() {
    let root = common::hd();
    let path = root.join("Data").join("interfacedata.cam");
    let data = std::fs::read(&path).unwrap();
    let whole = cam::Archive::parse(&data).unwrap();
    let image = cam::Image::read_as(&path, cam::Defer::Pixels).unwrap();
    let read = image.archive().unwrap();
    let tiles = |a: &'_ cam::Archive<'_>| a.chunk_by_tag(b"TILE").unwrap().records.len();

    assert_eq!(tiles(&whole), tiles(&read));
    let pairs = whole.chunk_by_tag(b"TILE").unwrap().records.iter();
    let pairs = pairs.zip(&read.chunk_by_tag(b"TILE").unwrap().records);

    assert!(image.held() < data.len() / 10);

    for (a, b) in pairs {
        assert_eq!(*b.bytes().unwrap(), *a.data);
        assert!(b.data.is_empty(), "left on disk");
    }
}

/// The payload heap runs to end of file. `Archive::parse` checks only where
/// the heap starts.
#[test]
fn no_archive_carries_trailing_bytes() {
    let root = common::hd();
    let mut slack: BTreeMap<String, usize> = BTreeMap::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        let heap_end = archive
            .records()
            .map(|(_, r)| r.offset + r.len())
            .max()
            .unwrap_or(0);

        if heap_end < data.len() {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();

            assert_eq!(
                &data[heap_end..],
                b"\n",
                "{}: unexpected trailing bytes",
                path.display()
            );
            slack.insert(name, data.len() - heap_end);
        }
    }

    assert_eq!(
        slack,
        BTreeMap::new(),
        "the set of archives with trailing bytes changed"
    );
}

/// A flagged chunk numbers its records; an unflagged one names them.
///
/// The flag does not mean compressed: `TILE` is plain row-RLE, `SPLT` a flat
/// 1032 bytes. A flagged record's id is its index, so `IMAG` refers to tiles
/// by position. The two unflagged non-tag ids are both `IMAG`; one is `0xcd`.
#[test]
fn a_flagged_chunk_numbers_its_records() {
    let root = common::hd();
    let (mut indexed, mut named, mut odd) = (0, 0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for chunk in &archive.chunks {
            for (i, r) in chunk.records.iter().enumerate() {
                let id = r.id.word();

                if chunk.flag == cam::ChunkFlag::Set {
                    assert_eq!(
                        id as usize,
                        i,
                        "{} {}: record {i} is numbered {id}",
                        path.display(),
                        chunk.tag
                    );
                    indexed += 1;
                } else if r
                    .id
                    .as_bytes()
                    .iter()
                    .all(|b| b.is_ascii_graphic() || *b == b' ')
                {
                    named += 1;
                } else {
                    odd += 1;
                }
            }
        }
    }

    assert_eq!(indexed, 33_455, "indexed record count changed");
    assert_eq!(named, 3_543, "tag-named record count changed");
    assert_eq!(odd, 2, "unflagged records with an untag-like id changed");
}

#[test]
fn chunk_flag_is_set_only_for_graphics_chunks() {
    let root = common::hd();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for chunk in &archive.chunks {
            let graphics = matches!(chunk.tag.as_bytes(), b"TILE" | b"SPLT" | b"CUT ");
            let set = chunk.flag == cam::ChunkFlag::Set;

            assert_eq!(
                graphics,
                set,
                "{} {}: flag {:?} on a {} chunk",
                path.display(),
                chunk.tag,
                chunk.flag,
                if graphics { "graphics" } else { "non-graphics" }
            );
        }
    }
}

#[test]
fn descriptor_payloads_share_a_header() {
    let root = common::hd();
    let mut seen = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if !payload::DESCRIPTOR_TAGS.contains(&chunk.tag.as_bytes()) {
                continue;
            }

            let d = payload::Descriptor::parse(r.data)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));

            // Repeats the chunk's tag.
            assert_eq!(d.tag, chunk.tag, "{}", path.display());
            seen += 1;
        }
    }

    assert_eq!(seen, 1438, "descriptor record count changed");
}

#[test]
fn data_and_btdt_do_not_use_the_descriptor_header() {
    let root = common::hd();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if matches!(chunk.tag.as_bytes(), b"DATA" | b"BTDT") {
                assert!(
                    payload::Descriptor::parse(r.data).is_err(),
                    "{} {} unexpectedly parsed as a descriptor",
                    path.display(),
                    chunk.tag
                );
            }
        }
    }
}

/// Every `BTDT` record parses to its last byte.
///
/// Five records, 428 bytes, one archive; `BuildTreeEditor_Controller` edits them.
#[test]
fn every_build_tree_variant_reads_to_its_last_byte() {
    let path = common::hd().join("DataMX/mx_btdata.cam");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let archive = cam::Archive::parse(&data).unwrap();
    let mut names = Vec::new();

    for (chunk, record) in archive.records() {
        if chunk.tag.as_bytes() != b"BTDT" {
            continue;
        }

        let variant = payload::Variant::parse(record.data)
            .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), record.name()));

        assert_eq!(variant.version, 1);
        names.push(variant.name.to_owned());
    }

    assert_eq!(
        names,
        [
            "default",
            "Brute Force",
            "No Towers",
            "One of a Kind",
            "Wiz War"
        ],
        "the five shipped variants, in file order"
    );
}

/// A variant's name predicts its caps.
///
/// A cap is how many of a building the player may have. `One of a Kind` caps
/// 16 buildings at 1. `No Towers` caps three at 0; their `DUNT` names are
/// `Ballista_Tower`, `Guardhouse1` and `Wizards_Tower`.
#[test]
fn a_variants_caps_are_what_its_name_says_they_are() {
    let path = common::hd().join("DataMX/mx_btdata.cam");
    let data = std::fs::read(&path).unwrap();
    let archive = cam::Archive::parse(&data).unwrap();

    let mut found = 0;

    for (chunk, record) in archive.records() {
        if chunk.tag.as_bytes() != b"BTDT" {
            continue;
        }

        let variant = payload::Variant::parse(record.data).unwrap();
        let caps: Vec<u32> = variant.caps().map(|(_, cap)| cap).collect();

        match variant.name {
            "default" => assert!(variant.is_empty(), "the unmodified tree caps nothing"),
            "One of a Kind" => {
                assert_eq!(caps.len(), 16);
                assert!(
                    caps.iter().all(|c| *c == 1),
                    "one of each, so every cap is 1"
                );
                found += 1;
            }
            "No Towers" => {
                let towers: Vec<String> = variant
                    .caps()
                    .map(|(id, _)| id.trimmed().to_owned())
                    .collect();

                assert_eq!(towers, ["ABB1", "ABE1", "ABM1"]);
                assert!(caps.iter().all(|c| *c == 0), "no towers, so every cap is 0");
                found += 1;
            }
            _ => {}
        }
    }

    assert_eq!(found, 2, "both named variants were checked");
}

/// Every `CUT` record parses, and is one frame of one of two FLIC files.
///
/// Twenty 886-byte records naming two Autodesk FLIC files by build-machine
/// path. The parser checks the frame number at `+0x34` equals the one at
/// `+0x24a`. Frames run 0..=5 and 0..=13, no gaps.
#[test]
fn every_cut_record_is_a_frame_of_one_of_two_flic_files() {
    let path = common::hd().join("Data/maindata.cam");
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let archive = cam::Archive::parse(&data).unwrap();
    let mut frames: std::collections::BTreeMap<String, Vec<u32>> =
        std::collections::BTreeMap::new();

    for (chunk, record) in archive.records() {
        if chunk.tag.as_bytes() != b"CUT " {
            continue;
        }

        let cut = payload::Cut::parse(record.data)
            .unwrap_or_else(|e| panic!("{} CUT {}: {e}", path.display(), record.id));

        assert_eq!(cut.version, 3);
        assert!(
            cut.source.starts_with("J:\\Projects\\Majesty\\Art\\"),
            "unexpected source root: {}",
            cut.source
        );

        frames
            .entry(cut.file().to_owned())
            .or_default()
            .push(cut.frame);
    }

    let mut seen: Vec<(&str, Vec<u32>)> = frames
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    seen.sort();

    assert_eq!(
        seen,
        vec![
            ("Build_Flag.flc", (0..6).collect::<Vec<_>>()),
            ("Highlight1.flc", (0..14).collect::<Vec<_>>()),
        ],
        "two files, each enumerated once with no gaps"
    );
}

#[test]
fn every_palette_is_256_colours() {
    let root = common::hd();
    let mut seen = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if payload::PALETTE_TAGS.contains(&chunk.tag.as_bytes()) {
                let p = payload::Palette::parse(r.data)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));

                assert_eq!(p.colours.len(), payload::PALETTE_COLOURS);
                seen += 1;
            }
        }
    }

    assert_eq!(seen, 1166, "palette record count changed");
}

/// A palette's 8-byte header is constant; its padding byte is not always zero.
///
/// Header meaning unknown. 64 sprite palettes in `maindata.cam` pad with `0xcd`,
/// MSVC's uninitialised-heap fill, so dropping the byte is right.
#[test]
fn palette_headers_are_constant_and_some_padding_is_uninitialised() {
    let root = common::hd();
    let mut headers: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut uninitialised = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if !payload::PALETTE_TAGS.contains(&chunk.tag.as_bytes()) {
                continue;
            }

            headers.insert(r.data[..8].to_vec());

            // Byte 4 of each entry.
            if (0..payload::PALETTE_COLOURS).any(|i| r.data[8 + 4 * i + 3] != 0) {
                uninitialised += 1;
            }
        }
    }

    assert_eq!(
        headers.into_iter().collect::<Vec<_>>(),
        [vec![0, 0, 0, 1, 0, 0, 0, 0]],
        "the palette header stopped being constant"
    );
    assert_eq!(uninitialised, 64, "palettes with non-zero padding changed");
}

#[test]
fn every_string_table_parses() {
    let root = common::hd();
    let mut seen = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"STRT" {
                continue;
            }

            let t = payload::StringTable::parse(r.data)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));

            // Every offset resolves to a terminated string.
            for i in 0..t.len() {
                assert!(t.get(i).is_some(), "{} {} string {i}", path.display(), r.id);
            }

            seen += 1;
        }
    }

    assert_eq!(seen, 359, "string table record count changed");
}

/// A version 2 record is `[u32 key] cstr`; a version 0 one is a bare `cstr`.
///
/// Each record ends where the next offset begins.
///
/// Except `HKTX`'s offset table: right for the first four hotkeys, then steps
/// of two or three. Walked in sequence its 34 records fill the payload exactly,
/// so the offsets are wrong, not the shape.
#[test]
fn a_version_two_string_record_carries_a_key() {
    let root = common::hd();
    let (mut bare, mut keyed) = (0, 0);
    let mut ragged = Vec::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"STRT" {
                continue;
            }

            let t = payload::StringTable::parse(r.data).unwrap();
            if t.is_empty() {
                continue;
            }

            let key = match t.form {
                payload::Form::Bare => {
                    bare += 1;
                    0
                }
                payload::Form::Keyed => {
                    keyed += 1;
                    4
                }
            };

            // Raw bytes: decoding turns a glyph table's high bytes into two
            // UTF-8 bytes each, breaking the length check.
            let tiles = (0..t.len()).all(|i| {
                let at = t.offset(i).map_or(r.data.len(), |at| at + key);
                let end = r
                    .data
                    .get(at..)
                    .and_then(|rest| rest.iter().position(|b| *b == 0))
                    .map(|z| at + z + 1);

                end == Some(t.offset(i + 1).unwrap_or(r.data.len()))
            });

            if !tiles {
                ragged.push(r.id.to_string());
            }
        }
    }

    assert_eq!((bare, keyed), (0, 291), "the version split changed");
    assert_eq!(ragged, Vec::<String>::new(), "a table stopped tiling");
}

/// A facing can carry a firing point when the art does not change.
///
/// Four buildings shoot. Each `Attack` set's slots share frames and differ only
/// in the bit-2 block, points 4 and 5; point 5 is tested. **Game:**
/// the Ballista Tower's art does not turn as it fires.
///
/// The points are the building's openings, not compass facings: the Guardhouse
/// has six arrow slits, the Dwarf Settlement four corner towers. The hexagon
/// below is the Guardhouse's plan, not evidence for the facing convention.
#[test]
fn a_facing_can_carry_a_firing_point_when_the_art_does_not_change() {
    /// Buildings whose `Attack` set draws the same art at every facing.
    const SHOOTERS: [(&[u8; 4], usize); 4] = [
        (b"ABB1", 8), // Ballista Tower
        (b"ABE1", 6), // Guardhouse
        (b"ABE2", 6), // Guardhouse, level 2
        (b"ABb1", 4), // Dwarf Settlement
    ];
    /// The block holding points 4 and 5.
    const FIRING_BIT: u32 = 1 << 2;

    let root = common::hd();
    let data = std::fs::read(root.join("Data/maindata.cam")).unwrap();
    let archive = cam::Archive::parse(&data).unwrap();

    let image = |id: &[u8; 4]| {
        let Some((_, record)) = archive
            .records()
            .find(|(chunk, r)| chunk.tag == b"IMAG" && r.id == FourCc::new(id))
        else {
            panic!("no IMAG {}", FourCc::new(id));
        };

        imag::Image::parse(record.data).unwrap()
    };

    for (id, slots) in SHOOTERS {
        let record = image(id);
        let set = record
            .sets
            .iter()
            .find(|s| s.kind() == imag::SetKind::Attack)
            .unwrap_or_else(|| panic!("{} has no Attack set", FourCc::new(id)));

        assert_eq!(set.slots.len(), slots, "{} attack slots", FourCc::new(id));

        let art: Vec<Vec<u32>> = set
            .slots
            .iter()
            .map(|s| s.frames().map(|f| f.reference).collect())
            .collect();
        let points: Vec<(i16, i16)> = set
            .slots
            .iter()
            .filter(|s| s.attachments & FIRING_BIT != 0)
            .map(|s| s.points()[imag::ATTACHMENT_BASE])
            .collect();

        assert!(
            art.windows(2).all(|w| w[0] == w[1]),
            "{} draws different art per facing after all",
            FourCc::new(id)
        );
        assert_eq!(points.len(), slots, "{} bit-2 points", FourCc::new(id));
        assert_eq!(
            points.iter().collect::<BTreeSet<_>>().len(),
            slots,
            "{} repeats a firing point",
            FourCc::new(id)
        );
    }

    // The Guardhouse's six: a hexagon starting straight up.
    let record = image(b"ABE1");
    let set = record
        .sets
        .iter()
        .find(|s| s.kind() == imag::SetKind::Attack)
        .unwrap();

    let points: Vec<(f64, f64)> = set
        .slots
        .iter()
        .filter(|s| s.attachments & FIRING_BIT != 0)
        .map(|s| s.points()[imag::ATTACHMENT_BASE])
        .map(|(x, y)| (f64::from(x), f64::from(y)))
        .collect();

    #[allow(
        clippy::cast_precision_loss,
        reason = "a slot count is at most 256; f64 is exact well past that"
    )]
    let n = points.len() as f64;
    let cx = points.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = points.iter().map(|p| p.1).sum::<f64>() / n;

    for (slot, (x, y)) in points.iter().enumerate() {
        // Clockwise from straight up; screen Y grows downward, as in every
        // other offset in these formats.
        let mut turn = (x - cx).atan2(cy - y).to_degrees();
        if turn < 0.0 {
            turn += 360.0;
        }

        #[allow(
            clippy::cast_precision_loss,
            reason = "a slot index is at most 255; f64 is exact well past that"
        )]
        let want = 360.0 * slot as f64 / n;
        let off = (turn - want).abs().min(360.0 - (turn - want).abs());

        assert!(
            off < 15.0,
            "Guardhouse slot {slot} fires at {turn:.0} degrees, not {want:.0}"
        );
    }
}

/// The control band: `247-250` darkens, `251-254` lightens.
///
/// Green tells them apart: the darken ramp holds green at 0 while red and blue
/// climb; the lighten ramp pins red and blue high while green climbs toward
/// white. Palette 800 (`level_up`) is the clean specimen: darken ramp unused
/// black, lighten ramp ending at `(253, 253, 253)`.
///
/// Neither range is reserved: 204 `maindata` tiles use `251-254` as ordinary
/// colours. The palette decides, not the index.
///
/// **Viewer:** evidence is a Dryad drawn with the band uncorrected.
#[test]
fn the_palette_control_band_darkens_below_251_and_lightens_above() {
    /// Magenta enough to be a control entry, not art.
    fn control(c: payload::Rgb) -> bool {
        c.r > 150 && c.b > 150 && c.r.abs_diff(c.b) < 40 && c.g < c.r.max(c.b)
    }

    let root = common::hd();
    let mut census = BTreeMap::new();

    for archive in ["Data/maindata.cam", "DataMX/mx_maindata.cam"] {
        let bytes = std::fs::read(root.join(archive)).unwrap();
        let cam = cam::Archive::parse(&bytes).unwrap();
        let palettes: Vec<_> = cam
            .chunk_by_tag(b"SPLT")
            .map(|chunk| {
                chunk
                    .records
                    .iter()
                    .map(|r| payload::Palette::parse(r.data).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let (mut band, mut art, mut key) = (0, 0, 0);

        for (chunk, record) in cam.records() {
            if chunk.tag != b"TILE" || record.data.is_empty() {
                continue;
            }

            let t = tile::Tile::parse(record.data).unwrap();

            // Version 3 only. The uncompressed ones are opaque 100x100
            // building portraits, where high indices are plain colours.
            let tile::PaletteRef::Splt(at) = t.palette() else {
                continue;
            };
            if t.version() != tile::VERSION_RLE {
                continue;
            }
            let (Some(indices), Some(Some(palette))) = (t.indices(), palettes.get(at as usize))
            else {
                continue;
            };

            let used: BTreeSet<u8> = indices.into_iter().flatten().collect();
            key += usize::from(used.contains(&255));

            let mut high = used.iter().filter(|i| (251..=254).contains(*i)).peekable();
            if high.peek().is_none() {
                continue;
            }

            if high.all(|i| control(palette.colours[*i as usize])) {
                band += 1;
            } else {
                art += 1;
            }
        }

        census.insert(archive, (band, art, key));
    }

    assert_eq!(
        census,
        BTreeMap::from([
            ("Data/maindata.cam", (279, 204, 0)),
            ("DataMX/mx_maindata.cam", (207, 49, 0)),
        ]),
        "control, ordinary and index-255 tile counts changed"
    );

    // Both ramps, in the two clearest palettes.
    let bytes = std::fs::read(root.join("Data/maindata.cam")).unwrap();
    let cam = cam::Archive::parse(&bytes).unwrap();
    let splt = cam.chunk_by_tag(b"SPLT").unwrap();
    let palette = |at: usize| payload::Palette::parse(splt.records[at].data).unwrap();

    let dryad = palette(564);
    let level_up = palette(800);
    let green = |p: &payload::Palette, from: usize| {
        (from..from + 4).map(|i| p.colours[i].g).collect::<Vec<_>>()
    };

    // Darken: green held at zero while red and blue climb.
    assert!((247..251).all(|i| dryad.colours[i].g == 0));
    assert!((247..250).all(|i| dryad.colours[i].r < dryad.colours[i + 1].r));

    // Lighten: green climbs toward white.
    assert_eq!(green(&dryad, 251), [42, 85, 127, 170]);
    assert_eq!(green(&level_up, 251), [142, 169, 197, 253]);
    let white = payload::Rgb {
        r: 253,
        g: 253,
        b: 253,
    };
    let unused = payload::Rgb { r: 0, g: 0, b: 0 };

    assert_eq!(level_up.colours[254], white);

    // The level-up star only glows, so its darken ramp is black.
    assert!((247..251).all(|i| level_up.colours[i] == unused));
}

/// An uncompressed indexed tile can have holes.
///
/// Run gaps are version 3's transparency; index 255 is every version's. The
/// blit loops skip it with a compare with `0xff`. 428 tiles hold it, sparsely:
/// `maindata` 4377 is a one-pixel ellipse outline.
///
/// Hardcoded, not the header field at `+0x10`: the blit region compares sprite
/// indices only against 247 and 255.
#[test]
fn an_uncompressed_indexed_tile_can_have_holes() {
    let root = common::hd();
    let mut census: BTreeMap<String, usize> = BTreeMap::new();

    for path in archives(&root) {
        let bytes = std::fs::read(&path).unwrap();
        let Ok(archive) = cam::Archive::parse(&bytes) else {
            continue;
        };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();

        for (chunk, record) in archive.records() {
            if chunk.tag != b"TILE" || record.data.is_empty() {
                continue;
            }

            let Ok(t) = tile::Tile::parse(record.data) else {
                continue;
            };
            if t.version() == tile::VERSION_RLE || t.format() != tile::PixelFormat::Indexed {
                continue;
            }

            let holes = t
                .indices()
                .into_iter()
                .flatten()
                .filter(|p| *p == Some(shade::TRANSPARENT))
                .count();

            if holes > 0 {
                *census.entry(name.clone()).or_default() += 1;
            }
        }
    }

    assert_eq!(
        census,
        BTreeMap::from([
            ("addinterface.cam".to_owned(), 2),
            ("addinterface_se.cam".to_owned(), 29),
            ("interfacedata.cam".to_owned(), 104),
            ("maindata.cam".to_owned(), 127),
            ("MDL1_maindata.cam".to_owned(), 3),
            ("mx_interfacedata.cam".to_owned(), 133),
            ("mx_maindata.cam".to_owned(), 51),
            ("XQD1_intro.cam".to_owned(), 3),
            ("XQD1_MainData.cam".to_owned(), 2),
        ]),
    );
}

/// A control index drawn over ground shades the ground, in every `maindata` tile.
///
/// The sheet gets the operation's result, never the palette entry. The ground
/// is dirt from `local/shots/heighty_lowerleft_native.png`, per `docs/formats/art.md`.
#[test]
fn a_control_index_drawn_over_ground_moves_the_ground() {
    /// Lit dirt beside the Wizards Tower's shadow.
    const DIRT: regent_core::data::payload::Rgb = regent_core::data::payload::Rgb {
        r: 112,
        g: 84,
        b: 56,
    };

    let root = common::hd();
    let path = root.join("Data/maindata.cam");
    let owned = vec![(path.display().to_string(), std::fs::read(&path).unwrap())];
    let catalogue = Catalogue::build(&owned).unwrap();
    let records = catalogue.chunk_len(0, b"TILE");

    let (mut shaded, mut band) = (0usize, 0usize);

    for index in 0..records {
        let Some(Ok(sprite)) = Sprite::at(&catalogue, 0, index) else {
            continue;
        };

        let control: Vec<(usize, shade::Shade)> = (0..sprite.covered.len())
            .filter_map(|i| match sprite.ink(i, Blit::World) {
                Ink::Shade(shade) => Some((i, shade)),
                _ => None,
            })
            .collect();

        if control.is_empty() {
            continue;
        }

        shaded += 1;
        band += control.len();

        let mut sheet = Sheet::blank(usize::from(sprite.width), usize::from(sprite.height));

        sheet.draw(&sprite, (0, 0), Backdrop::Flat(DIRT), Blit::World);

        for (i, shade) in control {
            assert_eq!(
                &sheet.rgba[i * 4..i * 4 + 3],
                shade.over(DIRT).channels(),
                "tile {index} pixel {i}",
            );
        }
    }

    assert_eq!(
        (shaded, band),
        (13_154, 13_232_893),
        "the shaded tiles of maindata, and their control pixels",
    );
}

/// The Warrior's panel picture has no holes under the interface blit.
///
/// `AVL1`'s `Interface` set 1001 is the 200x245 stonework of the selected-unit
/// panel. Its palette gives 247-255 ordinary colours (`247` is
/// `(228, 208, 218)`); the world blit would shade 1,786 pixels and punch 45
/// holes. Under [`Blit::Interface`] it is solid: the control band belongs to
/// the world blitter, not the tile.
#[test]
fn an_interface_picture_reads_its_colours_from_the_palette() {
    let library = common::library_of(&archives(&common::hd()));
    let catalogue = library.catalogue().unwrap();

    let unit = regent_core::content::unit::Unit::find(&catalogue, FourCc::new(b"AVL1"))
        .expect("the Warrior")
        .unwrap();
    let set = unit
        .interface(imag::INTERFACE_PANEL)
        .expect("its panel art");
    let FrameSource::Tile { archive, index, .. } =
        *set.poses.first().unwrap().stream(0).first().unwrap()
    else {
        panic!("the panel picture is not a tile");
    };

    let frame = Sprite::at(&catalogue, archive, index).unwrap().unwrap();

    assert_eq!(
        (frame.width, frame.height),
        (200, 245),
        "not the panel picture"
    );
    assert_eq!(
        marks(&frame, Blit::World),
        (1786, 45),
        "the world reading of a picture the world does not draw",
    );
    assert_eq!(
        marks(&frame, Blit::Interface),
        (0, 0),
        "the panel picture has a hole in it",
    );
}

/// The modal dialog's scrim is a control band even in interface art.
///
/// `INDa` set 1000 is `interfacedata` tile 912, 800x600, with 407,865 pixels of
/// index 247 whose palette entry is `(128, 0, 128)`. Both blits shade it.
#[test]
fn a_dialogs_scrim_is_a_control_band_under_either_blit() {
    let library = common::library_of(&archives(&common::hd()));
    let catalogue = library.catalogue().unwrap();

    let at = catalogue
        .find(b"IMAG", FourCc::new(b"INDa"))
        .expect("the dialog art");
    let sets = imagery::sets(&catalogue, at).unwrap();
    let set = sets
        .iter()
        .find(|s| regent_core::data::imag::set_number(s.id) == imag::INTERFACE_PORTRAIT)
        .expect("its first set");

    assert_eq!(set.blit(), Blit::Interface);

    let FrameSource::Tile { archive, index, .. } =
        *set.poses.first().unwrap().stream(0).first().unwrap()
    else {
        panic!("the dialog is not a tile");
    };

    let frame = Sprite::at(&catalogue, archive, index).unwrap().unwrap();

    assert_eq!((frame.width, frame.height), (800, 600));
    assert_eq!(marks(&frame, Blit::World), (407_865, 0));
    assert_eq!(
        marks(&frame, Blit::Interface),
        (407_865, 0),
        "the palette marks the scrim, so the interface reads it too",
    );
}

/// Pixels of a frame that shade, and that are holes.
fn marks(frame: &Sprite, blit: Blit) -> (usize, usize) {
    let (mut shaded, mut holes) = (0, 0);

    for i in 0..frame.covered.len() {
        match frame.ink(i, blit) {
            Ink::Shade(_) => shaded += 1,
            Ink::Hole => holes += 1,
            Ink::Colour(_) => {}
        }
    }

    (shaded, holes)
}

/// A tile is drawn by one blit or the other, almost never both.
///
/// So the set kind names the blit. Three of 31,250 are in both an `Interface`
/// and a world set: `maindata` 518 and 3404 (a `Crumble` frame doubling as a
/// picture of the ruin), and `interfacedata` 0.
///
/// The world blit would draw 1,348 interface tiles differently.
#[test]
fn the_two_blits_barely_share_a_tile() {
    let library = common::library_of(&archives(&common::hd()));
    let catalogue = library.catalogue().unwrap();

    // Which blits reference each tile.
    let mut drawn: BTreeMap<(usize, usize), BTreeSet<Blit>> = BTreeMap::new();

    for (at, _) in catalogue.tagged(b"IMAG") {
        let Ok(sets) = imagery::sets(&catalogue, at) else {
            continue;
        };

        for set in sets {
            let frames = set.poses.iter().flat_map(|pose| &pose.frames);

            for frame in frames {
                if let FrameSource::Tile { archive, index, .. } = frame {
                    drawn
                        .entry((*archive, *index))
                        .or_default()
                        .insert(set.blit());
                }
            }
        }
    }

    let census = |want: &[Blit]| drawn.values().filter(|b| b.iter().eq(want)).count();
    let interface: Vec<(usize, usize)> = drawn
        .iter()
        .filter(|(_, b)| b.iter().eq(&[Blit::Interface]))
        .map(|(&at, _)| at)
        .collect();

    assert_eq!(
        (
            drawn.len(),
            interface.len(),
            census(&[Blit::World]),
            census(&[Blit::World, Blit::Interface]),
        ),
        (31_322, 3_604, 27_715, 3),
        "referenced tiles, and how they split by blit",
    );

    let moved = interface
        .iter()
        .filter_map(|&(archive, index)| Sprite::at(&catalogue, archive, index)?.ok())
        .filter(|frame| marks(frame, Blit::World) != marks(frame, Blit::Interface))
        .count();

    assert_eq!(moved, 1_394, "interface tiles the world reading changes");
}

/// `HelpID` is a four-character key into `HPTX`; every non-zero one resolves.
///
/// The other 30 units declare zero.
#[test]
fn a_help_id_resolves_to_its_topic() {
    let root = common::hd();

    let text = std::fs::read(root.join("Data/gpltext.cam")).unwrap();
    let archive = cam::Archive::parse(&text).unwrap();
    let topics = archive
        .records()
        .find(|(chunk, r)| chunk.tag == b"STRT" && r.id == FourCc::new(b"HPTX"))
        .map(|(_, r)| payload::StringTable::parse(r.data).unwrap())
        .expect("gpltext.cam has no HPTX table");

    let units = std::fs::read(root.join("Data/unittype.cam")).unwrap();
    let archive = cam::Archive::parse(&units).unwrap();
    let (mut declared, mut resolved) = (0, 0);

    for (chunk, r) in archive.records() {
        if chunk.tag != b"DUNT" {
            continue;
        }

        let part = desc::Part::parse(r.data).unwrap();
        let unit = desc::Unit::parse(&part).unwrap();
        let help = unit
            .character()
            .and_then(|c| c.help_id())
            .or_else(|| unit.building().and_then(|b| b.help_id()));

        if let Some(id) = help {
            declared += 1;
            resolved += usize::from(topics.get_by_key(id).is_some());
        }
    }

    assert_eq!(topics.len(), 129, "HPTX topic count changed");
    assert_eq!(
        (declared, resolved),
        (142, 142),
        "a HelpID stopped naming a help topic"
    );
}

#[test]
fn every_wave_payload_is_riff() {
    let root = common::hd();
    let mut seen = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag == b"WAVE" {
                payload::wave(r.data)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));
                seen += 1;
            }
        }
    }

    assert_eq!(seen, 868, "wave record count changed");
}

#[test]
fn every_bcd_parses() {
    let root = common::hd();
    let mut seen = 0;
    let mut protos = 0;

    for dir in ["Data", "DataMX"] {
        for path in common::walk(&root.join(dir), &["bcd"]) {
            let data = std::fs::read(&path).unwrap();
            let program =
                bcd::Container::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            // Three sections tile the outer block, and the data section
            // round-trips: anything the decoder missed changes the bytes.
            let sections = program
                .sections()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let body = sections.data.body;
            let section =
                DataSection::decode(body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            assert_eq!(section.encode(), body, "{}", path.display());

            protos += section.prototypes.len();
            seen += 1;
        }
    }

    assert_eq!(seen, 9, "bcd file count changed");
    // 182 in each of two identical bytecode.bcd copies, 226 in
    // mx_data.bcd, 10 across the rest.
    assert_eq!(protos, 600, "prototype count changed");
}

/// Every `.q` and `.rgs` is recognised; the version spread is pinned.
///
/// Several versions of each ship, so the tag's fourth character is kept.
#[test]
fn quest_and_chart_files_are_recognised() {
    let root = common::hd();
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut maps = 0;
    let mut charts = 0;

    for dir in ["Quests", "QuestsMX", "Data", "DataMX"] {
        for path in common::walk(&root.join(dir), &["q", "rgs"]) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let data = std::fs::read(&path).unwrap();
            let f = rgs::File::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            match f.kind {
                rgs::Kind::Map => maps += 1,
                rgs::Kind::Chart => charts += 1,
            }

            *tags.entry(format!("{ext} {}", f.tag)).or_default() += 1;
        }
    }

    assert!(maps > 0, "found no quest maps");
    assert!(charts > 0, "found no generator charts");
    assert_eq!(
        tags,
        BTreeMap::from([
            ("q RGM6".into(), 19),
            ("q RGM9".into(), 13),
            ("q RGMa".into(), 4),
            ("rgs RGCA".into(), 1),
            ("rgs RGCB".into(), 3),
        ]),
        "the map and chart version spread changed"
    );
}

/// Every `.qdd` of every edition and mod reads whole: no stray line, no
/// unclosed value.
#[test]
fn every_shipped_qdd_parses() {
    let mut files = 0;

    for root in [common::hd(), common::workshop()] {
        for path in common::walk(&root, &["qdd"]) {
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let quests = qdd::parse(&String::from_utf8_lossy(&bytes))
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

            assert!(!quests.is_empty(), "{}", path.display());
            files += 1;
        }
    }

    assert_eq!(files, 6, "the shipped .qdd count changed");
}

#[test]
fn quest_metadata_ids_match_quest_filenames() {
    let root = common::hd();
    let path = root.join("Quests/Maj.qdd");

    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    let text = String::from_utf8_lossy(&bytes);
    let quests = qdd::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));

    assert!(quests.len() > 10, "expected many quest blocks");

    // Ids are the .q filenames, whatever their case.
    let on_disk: BTreeSet<String> = std::fs::read_dir(root.join("Quests"))
        .expect("the quest directory")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    let matched = quests
        .iter()
        .filter(|q| on_disk.contains(&format!("{}.q", q.id.to_lowercase())))
        .count();

    assert!(matched >= 10, "only {matched} quest ids matched a .q file");
}

#[test]
fn sniff_identifies_each_shipped_type() {
    let root = common::hd();

    let cases = [
        ("Data/terrtype.cam", FileKind::Cam),
        ("Data/bytecode.bcd", FileKind::Bcd),
        ("Data/constants.rgs", FileKind::RgsChart),
        ("Quests/Freestyle.q", FileKind::RgsMap),
    ];

    for (rel, want) in cases {
        let Ok(data) = std::fs::read(root.join(rel)) else {
            continue;
        };

        assert_eq!(sniff(&data), want, "{rel}");
    }
}

// --------------------------------------------------------------------------
// TILE
// --------------------------------------------------------------------------

/// Three tile versions ship, and all three are decoded.
#[test]
fn tile_versions_are_the_three_known_ones() {
    let root = common::hd();
    let mut counts: BTreeMap<u16, usize> = BTreeMap::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag == b"TILE" {
                *counts
                    .entry(tile::Tile::version_word(r.data).unwrap())
                    .or_default() += 1;
            }
        }
    }

    assert_eq!(
        counts,
        BTreeMap::from([(1, 2922), (2, 50), (3, 29_304)]),
        "tile version counts changed"
    );
}

/// Mean absolute green difference between horizontally adjacent pixels.
///
/// Reading 555 as 565 puts red's low bit at the top of green, swinging green by
/// half its range wherever red's parity flips.
fn green_roughness(words: &[u16], width: usize, green: fn(u16) -> i32) -> f64 {
    let (mut total, mut pairs) = (0i64, 0i64);

    for row in words.chunks(width) {
        for pair in row.windows(2) {
            total += i64::from((green(pair[0]) - green(pair[1])).abs());
            pairs += 1;
        }
    }

    #[allow(clippy::cast_precision_loss, reason = "a count, against a mean")]
    if pairs == 0 {
        0.0
    } else {
        total as f64 / pairs as f64
    }
}

/// Bit 2 of the flags word names a tile's 16-bit packing.
///
/// How the engine reads the bit is unknown; this is all the evidence:
///
/// - the 144 tiles without it never set bit 15 in 4,338,291 words, as 555
///   cannot. The 2,130 with it set bit 15 in about one word in eight.
/// - green is smooth under the named packing and rough under the other on
///   2,271 of 2,274 tiles. The three exceptions are 4 and 23 pixels wide.
#[test]
fn the_packing_bit_agrees_with_the_pixels_it_describes() {
    fn green565(word: u16) -> i32 {
        i32::from((word >> 5) & 0x3f) * 2
    }

    fn green555(word: u16) -> i32 {
        i32::from((word >> 5) & 0x1f) * 4
    }

    let library = common::library_of(&archives(&common::hd()));
    let catalogue = library.catalogue().unwrap();

    // Per packing: tiles, words, words with bit 15, and the two roughnesses.
    let mut census: BTreeMap<tile::Packing, (usize, usize, usize, f64, f64)> = BTreeMap::new();
    let mut disagree = 0;

    for archive in 0..catalogue.archives.len() {
        for index in 0..catalogue.chunk_len(archive, b"TILE") {
            let Some(record) = catalogue.positional(archive, b"TILE", index) else {
                continue;
            };

            if record.data.is_empty() {
                continue;
            }

            let parsed = tile::Tile::parse(record.data).unwrap();
            let tile::PixelFormat::Direct(packing) = parsed.format() else {
                continue;
            };

            let Some(Ok(frame)) = Sprite::at(&catalogue, archive, index) else {
                continue;
            };
            let Colour::Direct { words, .. } = &frame.colour else {
                panic!("a direct tile resolved to indices");
            };

            let width = usize::from(frame.width);
            let (as565, as555) = (
                green_roughness(words, width, green565),
                green_roughness(words, width, green555),
            );

            let row = census.entry(packing).or_default();

            row.0 += 1;
            row.1 += words.len();
            row.2 += words.iter().filter(|w| *w & 0x8000 != 0).count();
            row.3 += as565;
            row.4 += as555;

            if (as565 < as555) != (packing == tile::Packing::Rgb565) {
                disagree += 1;
            }
        }
    }

    let narrow = census[&tile::Packing::Rgb555];
    let wide = census[&tile::Packing::Rgb565];

    assert_eq!(
        (narrow.0, narrow.1, narrow.2),
        (150, 4_340_967, 2_676),
        "555 tiles, their words, and the words setting bit 15",
    );
    assert_eq!((wide.0, wide.1), (2130, 27_547_380), "the 565 tiles");
    assert!(wide.2 > wide.1 / 10, "{} of {} set bit 15", wide.2, wide.1);

    // In tenths of a green step, to compare integers.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a tile count against a mean, and a rounded mean against a step"
    )]
    let mean = |total: f64, tiles: usize| (total * 10.0 / tiles as f64).round() as u32;

    assert_eq!(
        [
            mean(narrow.3, narrow.0),
            mean(narrow.4, narrow.0),
            mean(wide.3, wide.0),
            mean(wide.4, wide.0),
        ],
        [228, 54, 60, 144],
        "555 and 565 tiles, each read both ways, in tenths of a step",
    );

    assert_eq!(disagree, 3, "tiles the measure and the flag disagree on");
}

/// Every version 3 tile decodes.
///
/// Rests on two `data::tile` findings: a header flag gives the pixel format,
/// and an empty run's column is uninitialised memory.
#[test]
fn every_v3_tile_decodes() {
    let root = common::hd();
    let (mut indexed, mut direct, mut embedded) = (0, 0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE"
                || tile::Tile::version_word(r.data).unwrap() != tile::VERSION_RLE
            {
                continue;
            }

            let t = tile::Tile::parse(r.data)
                .unwrap_or_else(|e| panic!("{} tile {}: {e}", path.display(), r.name()));

            assert_eq!(
                t.runs().unwrap().len(),
                usize::from(t.height()),
                "{}",
                path.display()
            );

            match t.format() {
                tile::PixelFormat::Indexed => indexed += 1,
                tile::PixelFormat::Direct(_) => direct += 1,
            }

            if matches!(t.palette(), tile::PaletteRef::Embedded(_)) {
                embedded += 1;
            }
        }
    }

    assert_eq!(indexed + direct, 29_304, "v3 tile count changed");
    assert_eq!(direct, 377, "direct-colour tile count changed");
    assert_eq!(embedded, 2269, "embedded-palette tile count changed");
}

/// Every version 1 and 2 tile decodes.
///
/// Version 3's header without compression. Three independent fields agree:
/// stride with `width x bytes-per-pixel`, payload length with
/// `header + height x stride`, and the embedded palette starts where pixels end.
#[test]
fn every_uncompressed_tile_decodes() {
    let root = common::hd();
    let (mut raw, mut embedded) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            let version = tile::Tile::version_word(r.data).unwrap();

            if chunk.tag != b"TILE" || !tile::VERSIONS_RAW.contains(&version) {
                continue;
            }

            let t = tile::Tile::parse(r.data)
                .unwrap_or_else(|e| panic!("{} tile v{version} {}: {e}", path.display(), r.name()));

            assert!(t.runs().is_none(), "{}: raw tile grew runs", path.display());

            // A raw block has no gaps, so it fills the canvas.
            let drawn = match t.format() {
                tile::PixelFormat::Indexed => {
                    t.indices().unwrap().iter().filter(|p| p.is_some()).count()
                }
                tile::PixelFormat::Direct(_) => {
                    t.words().unwrap().iter().filter(|p| p.is_some()).count()
                }
            };

            assert_eq!(
                drawn,
                usize::from(t.width()) * usize::from(t.height()),
                "{} tile {}: uncompressed tile left holes",
                path.display(),
                r.name()
            );

            if matches!(t.palette(), tile::PaletteRef::Embedded(_)) {
                embedded += 1;
            }

            raw += 1;
        }
    }

    assert_eq!(raw, 2_972, "v1/v2 tile count changed");
    assert_eq!(embedded, 834, "uncompressed embedded-palette count changed");
}

/// No sprite is stored in a format that cannot be recoloured.
///
/// The engine recolours by swapping a palette range, so anything player colour
/// touches must be indexed (`docs/formats/README.md`).
///
/// Every version 3 tile outside the interface archives is indexed. Version 3
/// is the RLE form, the only one with transparency, so the only one an
/// animation frame can be. Direct colour appears only where nothing moves:
/// terrain, interface backgrounds, 27 fixed 100x100 building portraits in
/// `maindata`. It is most of version 1.
#[test]
fn no_animated_sprite_is_stored_in_direct_colour() {
    let root = common::hd();
    let (mut v3_wide, mut raw_wide, mut terrain) = (0, 0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().to_lowercase();
        let interface = name.contains("interfacedata") || name.contains("addinterface");

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE" || r.data.is_empty() {
                continue;
            }

            let t = tile::Tile::parse(r.data).unwrap();
            let is_v3 = t.version() == tile::VERSION_RLE;

            if name.contains("tilesetdata") {
                terrain += 1;
                assert!(
                    !is_v3 && matches!(t.format(), tile::PixelFormat::Direct(_)),
                    "{}: a terrain tile is no longer uncompressed direct colour",
                    path.display()
                );
            }

            if t.format() == tile::PixelFormat::Indexed {
                continue;
            }

            if is_v3 {
                assert!(
                    interface,
                    "{}: RLE tile {} is direct colour outside the interface, \
                     so it could not be recoloured",
                    path.display(),
                    r.name()
                );
                v3_wide += 1;
            } else {
                raw_wide += 1;
            }
        }
    }

    assert_eq!(v3_wide, 377, "v3 RGB565 tile count changed");
    assert_eq!(raw_wide, 1_903, "uncompressed RGB565 tile count changed");
    assert_eq!(terrain, 1_860, "terrain tile count changed");
}

/// Runs within a row never overlap: proof that `x_end` is exclusive.
///
/// Read as a start column, every multi-run row would overlap.
#[test]
fn tile_runs_never_overlap_within_a_row() {
    let root = common::hd();
    let mut multi_run_rows = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE"
                || tile::Tile::version_word(r.data).unwrap() != tile::VERSION_RLE
            {
                continue;
            }

            for row in tile::Tile::parse(r.data).unwrap().runs().unwrap() {
                let filled: Vec<_> = row.iter().filter(|run| !run.is_empty()).collect();

                if filled.len() > 1 {
                    multi_run_rows += 1;
                }

                for pair in filled.windows(2) {
                    assert!(
                        pair[0].x_end <= pair[1].start().unwrap(),
                        "{} tile {}: run ending {} overlaps the next starting {}",
                        path.display(),
                        r.name(),
                        pair[0].x_end,
                        pair[1].start().unwrap()
                    );
                }
            }
        }
    }

    // One-run rows read the same either way, so need many multi-run rows.
    assert!(
        multi_run_rows >= 500_000,
        "only {multi_run_rows} multi-run rows to test against"
    );
}

/// No run ends past the header width; most tiles reach it exactly.
///
/// A tile whose rightmost columns are empty in every row stops short.
#[test]
fn tile_width_bounds_every_run() {
    let root = common::hd();
    let (mut exact, mut short) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE"
                || tile::Tile::version_word(r.data).unwrap() != tile::VERSION_RLE
            {
                continue;
            }

            let t = tile::Tile::parse(r.data).unwrap();
            let max_end = t.max_end();

            assert!(
                max_end <= t.width(),
                "{} tile {}: run ends at {max_end}, past width {}",
                path.display(),
                r.name(),
                t.width()
            );

            if max_end == t.width() {
                exact += 1;
            } else {
                short += 1;
            }
        }
    }

    assert_eq!(exact, 28_733, "tiles reaching their full width changed");
    assert_eq!(short, 571, "tiles falling short of their width changed");
}

/// Rasterising covers every stored pixel and nothing else.
///
/// Equality, not a bound: `parse_row` refuses runs leaving the canvas, and runs
/// never overlap, so each stored pixel has its own cell.
#[test]
fn tile_rasterises_exactly_its_runs() {
    let root = common::hd();
    let (mut indexed, mut wide) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE"
                || tile::Tile::version_word(r.data).unwrap() != tile::VERSION_RLE
            {
                continue;
            }

            let t = tile::Tile::parse(r.data).unwrap();
            let stored: usize = t
                .runs()
                .unwrap()
                .iter()
                .flatten()
                .map(|run| usize::from(run.count()))
                .sum();

            // Both pixel formats, covering the RGB565 path.
            let drawn = match t.format() {
                tile::PixelFormat::Indexed => {
                    indexed += 1;
                    t.indices().unwrap().iter().filter(|p| p.is_some()).count()
                }
                tile::PixelFormat::Direct(_) => {
                    wide += 1;
                    t.words().unwrap().iter().filter(|p| p.is_some()).count()
                }
            };

            assert_eq!(
                drawn,
                stored,
                "{} tile {}: drew {drawn} of {stored} stored pixels",
                path.display(),
                r.name()
            );
        }
    }

    assert_eq!(indexed, 28_927, "indexed v3 tile count changed");
    assert_eq!(wide, 377, "RGB565 v3 tile count changed");
}

/// The flags word predicts the pixel format.
///
/// Trial decoding at both widths cannot decide alone: 10 HD version 3 tiles
/// (39 across all corpora) parse either way. Where the two differ, the trial
/// is ambiguous; the flag is never wrong.
#[test]
fn the_pixel_format_flag_is_never_wrong() {
    let root = common::hd();
    let (mut decided, mut ambiguous) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"TILE"
                || tile::Tile::version_word(r.data).unwrap() != tile::VERSION_RLE
            {
                continue;
            }

            let flagged = tile::Tile::parse(r.data).unwrap().format();
            let other = match flagged {
                // Packing does not change pixel width; only width can differ.
                tile::PixelFormat::Indexed => tile::PixelFormat::Direct(tile::Packing::Rgb565),
                tile::PixelFormat::Direct(_) => tile::PixelFormat::Indexed,
            };

            if tile::Tile::parse_as(r.data, other).is_ok() {
                ambiguous += 1;
            } else {
                decided += 1;
            }
        }
    }

    assert_eq!(decided, 29_294, "tiles the trial decides on its own");
    assert_eq!(ambiguous, 10, "tiles only the flag can resolve");
}

// --------------------------------------------------------------------------
// IMAG
// --------------------------------------------------------------------------

/// Four record layouts ship; the version word at `+0x00` selects one.
#[test]
fn imag_versions_are_the_four_known_ones() {
    let root = common::hd();
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag == b"IMAG" {
                *counts
                    .entry(imag::Image::version_word(r.data).unwrap())
                    .or_default() += 1;
            }
        }
    }

    assert_eq!(
        counts,
        BTreeMap::from([(2, 98), (3, 28), (4, 498), (5, 25)]),
        "imag version counts changed"
    );
}

/// Every record decodes to frames.
///
/// The frame count matters: a reader checking only that the frame table clears
/// the header passes the 98 version 2 records while decoding zero frames.
#[test]
fn every_imag_record_decodes_to_frames() {
    let root = common::hd();
    let (mut records, mut slots, mut frames) = (0, 0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }

            let image = imag::Image::parse(r.data)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.name()));

            assert!(!image.sets.is_empty(), "{} {}", path.display(), r.name());

            records += 1;
            slots += image.slot_count();
            frames += image.frame_count();
        }
    }

    assert_eq!((records, slots, frames), (649, 13_439, 86_868));
}

/// Every frame reference resolves, in its own archive.
///
/// `IMAG` and `TILE` decode separately, so a misparse of either shows as an
/// index pointing at nothing. Tile numbers are positional and archive-local.
///
/// No reference is negative. The 101 that seem so are the first eight bytes of
/// a point list: a slot carrying one has its frame table at the slot's end,
/// not after the attachment blocks.
#[test]
fn every_imag_frame_reference_resolves() {
    let root = common::hd();
    let (mut tiles, mut sentinels, mut unknown) = (0, 0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();
        let count = archive.chunk_by_tag(b"TILE").map_or(0, |c| c.records.len());

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }

            let image = imag::Image::parse(r.data).unwrap();

            for frame in image
                .sets
                .iter()
                .flat_map(|s| &s.slots)
                .flat_map(Slot::frames)
            {
                match frame.target() {
                    imag::FrameRef::Tile { index, .. } => {
                        assert!(
                            usize::from(index) < count,
                            "{} {}: tile {index} of {count}",
                            path.display(),
                            r.name()
                        );
                        tiles += 1;
                    }
                    imag::FrameRef::Sentinel(_) => sentinels += 1,
                    imag::FrameRef::Unknown(w) => {
                        unknown += 1;
                        eprintln!(
                            "{} {}: unclassified reference {w:#010x}",
                            path.display(),
                            r.name()
                        );
                    }
                }
            }
        }
    }

    assert_eq!(tiles, 86_868, "resolving tile references changed");
    assert_eq!(
        sentinels, 0,
        "a negative reference is a misread frame table"
    );
    assert_eq!(unknown, 0, "a reference fell outside both known forms");
}

/// A set runs to where the next starts, so the table is in address order.
#[test]
fn imag_set_tables_are_in_address_order() {
    let root = common::hd();
    let mut checked = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }

            let count = u32::from_le_bytes(r.data[0x14..0x18].try_into().unwrap()) as usize;
            let offsets: Vec<u32> = (0..count)
                .map(|i| {
                    let at = 0x18 + 8 * i + 4;
                    u32::from_le_bytes(r.data[at..at + 4].try_into().unwrap())
                })
                .collect();

            assert!(
                offsets.windows(2).all(|p| p[0] < p[1]),
                "{} {}: set table out of order",
                path.display(),
                r.name()
            );
            checked += 1;
        }
    }

    assert_eq!(checked, 649);
}

/// Slot counts run well past the eight published tooling allows.
#[test]
fn imag_slot_counts_exceed_eight() {
    let root = common::hd();
    let mut widest = 0;
    let mut over_eight = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }

            for set in &imag::Image::parse(r.data).unwrap().sets {
                widest = widest.max(set.slots.len());

                if set.slots.len() > 8 {
                    over_eight += 1;
                }
            }
        }
    }

    assert_eq!(widest, 256, "widest set changed");
    assert_eq!(over_eight, 146, "sets with more than eight slots changed");
}

/// `SetKind` against HD's `Data/ImageSetIDXRef.xml`, the engine's name-to-id
/// table: 367 distinct ids.
///
/// A table built from the corpus alone lumps the 256 `MapTexture` blends into
/// `UnitTexture`, stops at 5200 not 5255, and misses the five power-state
/// families.
///
/// The file names block members `Die-1` to `Die-8`. `SetKind` is the family,
/// the name without the ordinal; [`imag::name`] adds the number back, which
/// matters because a building's `Die-1` to `Die-8` are damage stages.
/// [`imag::name`] derives from id ranges, not a lookup, so its spelling may not
/// be in the SDK.
#[test]
fn image_set_kinds_match_the_cross_reference() {
    let path = common::hd().join("Data/ImageSetIDXRef.xml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc = roxmltree::Document::parse(&text).expect("the cross reference parses");

    let mut ids: BTreeSet<u16> = BTreeSet::new();
    let mut bad: Vec<String> = Vec::new();

    // Entries with an ordinal, split by whether `imag::name` reproduces it.
    let (mut numbered, mut reproduced) = (0, 0);
    let mut apart: BTreeMap<String, usize> = BTreeMap::new();

    for entry in doc.descendants().filter(|n| n.has_tag_name("Entry")) {
        let (Some(name), Some(id)) = (entry.attribute("name"), entry.attribute("ID")) else {
            continue;
        };
        let id: u16 = id.parse().expect("a numeric id");

        // `Walk-3`, `Interface-07`: block members. Only these end in
        // `-<digits>`.
        let family = name
            .rsplit_once('-')
            .filter(|(_, tail)| tail.chars().all(|c| c.is_ascii_digit()))
            .map_or(name, |(head, _)| head);

        let got = imag::SetKind::from_id(id).to_string();

        if got != family {
            bad.push(format!(
                "{id} is {got:?}, the cross reference says {family:?}"
            ));
        }

        // The member number, where given.
        if name != family {
            numbered += 1;

            if imag::name(id) == name {
                reproduced += 1;
            } else {
                *apart.entry(family.to_owned()).or_default() += 1;
            }
        }

        ids.insert(id);
    }

    assert!(
        bad.is_empty(),
        "{} ids disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(ids.len(), 367, "cross reference id count changed");

    // The 48 misses are two habits of the file: it numbers a one-member
    // family `-1`, and counts `Interface` from zero, two digits wide.
    assert_eq!(numbered, 97, "entries the file gives an ordinal");
    assert_eq!(reproduced, 49, "ordinals imag::name puts back unchanged");
    assert_eq!(
        apart,
        BTreeMap::from([
            ("Assimilate".to_owned(), 1),
            ("Crumble".to_owned(), 1),
            ("Damage".to_owned(), 1),
            ("Hotspot".to_owned(), 1),
            ("Interface".to_owned(), 41),
            ("Selection-Overlay".to_owned(), 1),
            ("Selection-Underlay".to_owned(), 1),
            ("Unpowered".to_owned(), 1),
        ]),
        "which families the file numbers and imag::name does not"
    );
}

/// The cross reference names every shipped set id but two.
///
/// 196 and 212 are each one past the end of the `Active` and `Inactive`
/// blocks. **Guess:** they extend those blocks.
#[test]
fn only_two_shipped_set_ids_are_unnamed() {
    let root = common::hd();
    let mut unnamed: BTreeSet<u16> = BTreeSet::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"IMAG" {
                continue;
            }

            for set in &imag::Image::parse(r.data).unwrap().sets {
                if let imag::SetKind::Other(id) = set.kind() {
                    unnamed.insert(id);
                }
            }
        }
    }

    assert_eq!(
        unnamed.into_iter().collect::<Vec<_>>(),
        [196, 212],
        "the set of unnamed image-set ids changed"
    );
}

/// HD's `Data/setnames.txt` against `ImageSetIDXRef.xml`: no name disagrees.
///
/// `setnames.txt` is 380 `[name] id` lines. 349 names are in both, each with
/// one id. Its own 31: `Idle-n` and `Collect-n` for the cross reference's
/// `Special-n` and `Carry-n` ids, and 23 ids from 3000 to 3900 (damage and
/// transition sets) that no shipped HD set uses. No executable names the file
/// (**Binary**, `MajestyHD.exe` names only `ImageSetIDXRef.xml`).
/// **Inferred**: an older vocabulary, left in the install.
#[test]
fn setnames_agrees_with_the_cross_reference() {
    let data = common::hd().join("Data");
    let names = std::fs::read_to_string(data.join("setnames.txt")).unwrap();
    let xml = std::fs::read_to_string(data.join("ImageSetIDXRef.xml")).unwrap();
    let doc = roxmltree::Document::parse(&xml).expect("the cross reference parses");

    // `[Die-1]		 96`; blank lines between families.
    let listed: BTreeMap<&str, u16> = names
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (name, id) = l.trim().trim_start_matches('[').split_once(']').unwrap();
            (name, id.trim().parse().unwrap())
        })
        .collect();

    let crossed: BTreeMap<&str, u16> = doc
        .descendants()
        .filter(|n| n.has_tag_name("Entry"))
        .map(|n| {
            (
                n.attribute("name").unwrap(),
                n.attribute("ID").unwrap().parse().unwrap(),
            )
        })
        .collect();

    let disagree: Vec<_> = listed
        .iter()
        .filter(|(name, id)| crossed.get(*name).is_some_and(|x| x != *id))
        .collect();
    let shared = listed.keys().filter(|n| crossed.contains_key(*n)).count();
    let own: BTreeSet<u16> = listed
        .iter()
        .filter(|(name, _)| !crossed.contains_key(*name))
        .map(|(_, id)| *id)
        .collect();
    let unnamed: Vec<u16> = own
        .iter()
        .copied()
        .filter(|id| !crossed.values().any(|x| x == id))
        .collect();

    assert_eq!(listed.len(), 380, "setnames.txt entries");
    assert!(disagree.is_empty(), "names with two ids: {disagree:?}");
    assert_eq!(shared, 349, "names in both");
    assert_eq!(own.len(), 31, "ids setnames.txt names its own way");
    assert_eq!(unnamed.len(), 23, "ids only setnames.txt has");
    assert!(unnamed.iter().all(|id| (3000..=3900).contains(id)));

    // No shipped HD set uses them.
    let hd = common::hd();
    let used: BTreeSet<u16> = ["Data", "DataMX"]
        .iter()
        .flat_map(|dir| common::archives(&hd.join(dir)))
        .flat_map(|path| {
            let data = std::fs::read(&path).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            archive
                .records()
                .filter(|(chunk, _)| chunk.tag == b"IMAG")
                .flat_map(|(_, r)| imag::Image::parse(r.data).unwrap().sets)
                .map(|set| regent_core::data::imag::set_number(set.id))
                .collect::<Vec<_>>()
        })
        .collect();

    assert!(!used.is_empty(), "found no HD art");
    assert!(unnamed.iter().all(|id| !used.contains(id)));
}

/// The three animations published write-ups name, with their stated shapes.
#[test]
fn the_documented_animations_have_their_documented_shapes() {
    let root = common::hd();
    let data = std::fs::read(root.join("Data/maindata.cam")).unwrap();
    let archive = cam::Archive::parse(&data).unwrap();

    let find = |id: &[u8; 4]| {
        archive
            .records()
            .find(|(c, r)| c.tag == b"IMAG" && r.id == id)
            .map(|(_, r)| imag::Image::parse(r.data).unwrap())
            .expect("record present")
    };

    // Barbarian Walk: eight facings of eight frames.
    let walk = find(b"AVB1");
    let set = walk.set_of(imag::SetKind::Walk).unwrap();
    assert_eq!(set.slots.len(), 8);
    assert!(set.slots.iter().all(|s| s.frames == 8 && s.streams == 1));

    // Healer Cast: the only multi-stream hero animation; tests stream-major
    // order.
    let cast = find(b"AVD1");
    let set = cast.set_of(imag::SetKind::Cast).unwrap();
    assert_eq!(set.slots.len(), 8);
    assert!(set.slots.iter().all(|s| s.frames == 9 && s.streams == 2));

    // Ranger Die: the only uneven Die, catching a parser that derives frame
    // counts from pointer spacing.
    let die = find(b"AVH1");
    let set = die.set_of(imag::SetKind::Die).unwrap();
    let counts: Vec<u16> = set.slots.iter().map(|s| s.frames).collect();
    assert_eq!(counts, [16, 16, 16, 16, 15, 16, 16, 16]);
}

// --------------------------------------------------------------------------
// D* descriptors
// --------------------------------------------------------------------------

/// Every descriptor decomposes into its tree; the shapes are pinned.
#[test]
fn every_descriptor_decomposes() {
    let root = common::hd();
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    let mut parts = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if !payload::DESCRIPTOR_TAGS.contains(&chunk.tag.as_bytes()) {
                continue;
            }

            let part = desc::Part::parse(r.data)
                .unwrap_or_else(|e| panic!("{} {} {}: {e}", path.display(), chunk.tag, r.name()));

            parts += part.walk().count();

            let inner: Vec<String> = part
                .parts
                .iter()
                .flat_map(|d| d.parts.iter().map(|c| c.tag.to_string()))
                .collect();

            *shapes
                .entry(format!("{} [{}]", part.tag, inner.join(" ")))
                .or_default() += 1;
        }
    }

    assert_eq!(
        shapes,
        BTreeMap::from([
            ("DACT [HEAD PRIM SECD]".into(), 140),
            ("DFCT [HEAD]".into(), 2),
            ("DMOV [HEAD PRIM]".into(), 62),
            ("DSDG [HEAD PRIM]".into(), 50),
            ("DSDP [HEAD]".into(), 130),
            ("DSND [HEAD PRIM]".into(), 448),
            ("DTPT [HEAD PRIM]".into(), 21),
            ("DTRN [HEAD PRIM]".into(), 2),
            ("DUNT [HEAD PRIM SECD]".into(), 443),
            ("DUNT [HEAD PRIM]".into(), 140),
        ]),
        "descriptor tree shapes changed"
    );
    assert_eq!(parts, 6_203, "total part count changed");
}

/// Every descriptor's `HEAD` id matches its archive record id.
///
/// The two are written independently, so a misparse of either shows.
#[test]
fn every_descriptor_head_matches_its_record_id() {
    let root = common::hd();
    let (mut checked, mut nameless) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if !payload::DESCRIPTOR_TAGS.contains(&chunk.tag.as_bytes()) {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let head = part.find(b"HEAD").expect("every descriptor has a HEAD");
            let head = desc::Head::parse(head.body)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.name()));

            assert_eq!(head.id, r.id, "{} {}", path.display(), r.name());

            if head.name.is_empty() {
                nameless += 1;
            }

            checked += 1;
        }
    }

    assert_eq!(checked, 1_438, "descriptor record count changed");
    // The expansion's `XS03`, a placeholder unnamed in payload and directory.
    assert_eq!(nameless, 1, "nameless descriptor count changed");
}

// --------------------------------------------------------------------------
// The SDK XML oracle
// --------------------------------------------------------------------------
//
// `OriginalQuests/Data` and `DataMX` hold every base and expansion descriptor
// as XML, fields named: the bytes decoded here.
//
// Descriptions are copied out: a `roxmltree` node borrows its file, and the
// oracle outlives it.

/// Every `<Tag value="..."/>` in one section; values in document order.
///
/// A `Vec` because bitfields `Info` and `Flags` repeat, one element per set bit.
type Fields = BTreeMap<String, Vec<String>>;

/// One `<Description>`, reduced to what the oracle reads.
#[derive(Default)]
struct Desc {
    sub_type: String,
    /// `<Engine>`, the binary's `PRIM`.
    engine: Fields,
    /// `<Game>`, the binary's `SECD`.
    game: Fields,
    /// Sound descriptions only; empty for everything else.
    phases: Vec<PhaseXml>,
    /// Particle systems only: the two sections inside `<Engine>`.
    emitter: Fields,
    particle: Fields,
}

/// One `<Phase>` of a sound description.
#[derive(Default)]
struct PhaseXml {
    id: String,
    wave: Option<String>,
    group: Option<String>,
    flags: Vec<String>,
    distance: Option<f32>,
    volume: Option<(i64, i64)>,
    frequency: Option<(i64, i64)>,
}

impl Desc {
    /// A field's first value, from `<Engine>` or `<Game>`.
    ///
    /// No field name is in both, so order does not matter.
    fn text(&self, field: &str) -> Option<&str> {
        first(&self.engine, field).or_else(|| first(&self.game, field))
    }

    fn num(&self, field: &str) -> Option<i64> {
        self.text(field)?.parse().ok()
    }
}

fn first<'a>(fields: &'a Fields, field: &str) -> Option<&'a str> {
    fields.get(field)?.first().map(String::as_str)
}

/// Collect `<Tag value="..."/>` children into a map.
fn fields_of(node: roxmltree::Node<'_, '_>) -> Fields {
    let mut out = Fields::new();

    for child in node.children().filter(roxmltree::Node::is_element) {
        let tag = child.tag_name().name();

        // Mostly `<Tag value="x"/>`. Others, like
        // `<AttackRange min="1" max="100"/>`, key as `Tag@attr`.
        match child.attribute("value") {
            Some(v) => out.entry(tag.to_owned()).or_default().push(v.to_owned()),
            None => {
                for a in child.attributes() {
                    out.entry(format!("{tag}@{}", a.name()))
                        .or_default()
                        .push(a.value().to_owned());
                }
            }
        }
    }

    out
}

fn phases_of(engine: roxmltree::Node<'_, '_>) -> Vec<PhaseXml> {
    engine
        .children()
        .filter(|n| n.has_tag_name("Phase"))
        .map(|p| {
            let f = fields_of(p);
            let pair = |name: &str| {
                p.children()
                    .find(|n| n.has_tag_name(name))
                    .and_then(|n| Some((attr_num(n, "up")?, attr_num(n, "down")?)))
            };

            PhaseXml {
                id: p.attribute("ID").unwrap_or_default().to_owned(),
                wave: first(&f, "Wave").map(str::to_owned),
                group: first(&f, "Group").map(str::to_owned),
                flags: f.get("Flags").cloned().unwrap_or_default(),
                distance: first(&f, "DistanceModifier").and_then(|s| s.parse().ok()),
                volume: pair("VolumeVariation"),
                frequency: pair("FrequencyVariation"),
            }
        })
        .collect()
}

/// A section nested inside `<Engine>`; only particle systems have one.
fn nested(engine: roxmltree::Node<'_, '_>, tag: &str) -> Option<Fields> {
    engine
        .children()
        .find(|n| n.has_tag_name(tag))
        .map(fields_of)
}

/// A vector packed in one attribute: `value="0.2, 0.2, 0.0"`.
fn xml_vec(fields: &Fields, field: &str) -> Option<Vec<f64>> {
    first(fields, field)?
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect()
}

fn attr_num(node: roxmltree::Node<'_, '_>, name: &str) -> Option<i64> {
    node.attribute(name)?.parse().ok()
}

/// An edition's archives and their XML.
///
/// Not merged: the expansion rebalances shared units. `AVA1` (Adept) parries
/// 75 in `data`, 65 in `datamx`.
struct Corpus {
    units: &'static str,
    actions: &'static str,
    sounds: &'static str,
    /// All six subtypes; terrain features, projectiles and particle systems
    /// are `DUNT` records too.
    unit_xml: &'static [&'static str],
    action_xml: &'static [&'static str],
    sound_xml: &'static [&'static str],
}

const BASE: Corpus = Corpus {
    units: "Data/unittype.cam",
    actions: "Data/action.cam",
    sounds: "Data/sounddesc.cam",
    unit_xml: &[
        "Data/M_Characters",
        "Data/M_Buildings",
        "Data/M_Overlays",
        "Data/M_TerrainFeatures",
        "Data/M_Projectiles",
        "Data/M_ParticleSystems",
    ],
    action_xml: &["Data/M_Actions"],
    sound_xml: &["Data/M_Sounds"],
};

const EXPANSION: Corpus = Corpus {
    units: "DataMX/mx_Unittype.cam",
    actions: "DataMX/mx_action.cam",
    sounds: "DataMX/mx_sounddesc.cam",
    unit_xml: &[
        "DataMX/MX_Characters",
        "DataMX/MX_Buildings",
        "DataMX/MX_Overlays",
        "DataMX/MX_TerrainFeatures",
        "DataMX/MX_Projectiles",
        "DataMX/MX_ParticleSystems",
    ],
    action_xml: &["DataMX/MX_Actions"],
    sound_xml: &["DataMX/MX_Sounds"],
};

const EDITIONS: [&Corpus; 2] = [&BASE, &EXPANSION];

/// The SDK's descriptions, keyed by four-character id.
///
/// Names are relative to `OriginalQuests`.
fn sdk_descriptions(sdk: &Path, names: &[&str]) -> BTreeMap<String, Desc> {
    let mut out = BTreeMap::new();

    for name in names {
        let path = sdk.join(format!("OriginalQuests/{name}.xml"));

        // A partial oracle makes solvable tags look unsolvable.
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the oracle is incomplete: {}: {e}", path.display()));

        let doc = roxmltree::Document::parse(&text).expect("the SDK's XML parses");

        for d in doc.descendants().filter(|n| n.has_tag_name("Description")) {
            let Some(id) = d.attribute("ID") else {
                continue;
            };

            let section = |tag| {
                d.children()
                    .find(|n: &roxmltree::Node<'_, '_>| n.has_tag_name(tag))
            };
            let engine = section("Engine");

            out.insert(
                id.to_owned(),
                Desc {
                    sub_type: d.attribute("subType").unwrap_or_default().to_owned(),
                    engine: engine.map(fields_of).unwrap_or_default(),
                    emitter: engine
                        .and_then(|e| nested(e, "Emitter"))
                        .unwrap_or_default(),
                    particle: engine
                        .and_then(|e| nested(e, "Particle"))
                        .unwrap_or_default(),
                    game: section("Game").map(fields_of).unwrap_or_default(),
                    phases: engine.map(phases_of).unwrap_or_default(),
                },
            );
        }
    }

    out
}

/// Compares one unit's fields against its XML; returns how many.
fn check_unit(id: &str, unit: &desc::Unit<'_>, block: &Desc, bad: &mut Vec<String>) -> usize {
    let got_kind = format!("{:?}", unit.kind());

    assert_eq!(got_kind, block.sub_type, "{id} subtype");
    assert_eq!(
        unit.image_id().map(|f| f.to_string()).as_deref(),
        block.text("ImageIDBase"),
        "{id} ImageIDBase"
    );

    let mut pairs: Vec<(&str, Option<i64>)> = vec![("Menu", unit.menu().map(i64::from))];
    // Four-character fields; not `i64`.
    let mut ids: Vec<(&str, Option<String>)> = Vec::new();

    if let Some(c) = unit.character() {
        pairs.extend([
            ("MaxHP", c.max_hp().map(i64::from)),
            ("Attack", c.attack().map(i64::from)),
            ("Parry", c.parry().map(i64::from)),
            ("Dodge", c.dodge().map(i64::from)),
            ("Speed", c.speed().map(i64::from)),
            ("Strength", c.strength().map(i64::from)),
            ("Intelligence", c.intelligence().map(i64::from)),
            ("SightRange", c.sight_range().map(i64::from)),
            ("Experience", c.experience().map(i64::from)),
            ("Gold", c.gold().map(i64::from)),
            ("RecruitDelay", c.recruit_delay().map(i64::from)),
            ("MagicResistance", c.magic_resistance().map(i64::from)),
            ("ArmorBasicDamage", c.armor_damage().map(i64::from)),
            ("AttackRange@min", c.attack_range_min().map(i64::from)),
            ("AttackRange@max", c.attack_range_max().map(i64::from)),
            ("Cost", c.cost().map(i64::from)),
            ("Vitality", c.vitality().map(i64::from)),
            ("Artifice", c.artifice().map(i64::from)),
            ("WillPower", c.will_power().map(i64::from)),
            ("RangedAttack", c.ranged_attack().map(i64::from)),
            ("WeaponBasicDamage", c.weapon_damage().map(i64::from)),
            ("PrimaryStat", c.primary_stat().map(i64::from)),
        ]);
        ids.extend([
            ("DialogID", c.dialog_id().map(|f| f.to_string())),
            ("NameGenType", c.name_gen_type().map(|f| f.to_string())),
            ("HelpID", c.help_id().map(|f| f.to_string())),
        ]);
    } else if let Some(b) = unit.building() {
        pairs.extend([
            ("MaxHP", b.max_hp().map(i64::from)),
            ("Cost", b.cost().map(i64::from)),
            ("SightRange", b.sight_range().map(i64::from)),
            ("IncomeType", b.income_type().map(i64::from)),
            ("IncomeAmount", b.income_amount().map(i64::from)),
            ("MaxGuildMembers", b.max_guild_members().map(i64::from)),
            ("Gold", b.gold().map(i64::from)),
        ]);
        ids.extend([
            ("DialogID", b.dialog_id().map(|f| f.to_string())),
            ("NameGenType", b.name_gen_type().map(|f| f.to_string())),
            ("HelpID", b.help_id().map(|f| f.to_string())),
        ]);
    } else {
        pairs.push(("StackPriority", unit.stack_priority().map(i64::from)));
    }

    compare(id, &pairs, block, bad) + compare_ids(id, &ids, block, bad)
}

fn check_action(id: &str, action: &desc::Action<'_>, block: &Desc, bad: &mut Vec<String>) -> usize {
    let mut n = 0;

    // The script block, past PRIM's declared length.
    {
        let s = action.script();

        n += 1;

        let got = (
            s.kind.to_string(),
            s.c_proc.to_string(),
            String::from_utf8_lossy(s.function).into_owned(),
        );
        let want = (
            block.text("Script@type").unwrap_or("0").to_owned(),
            block.text("Script@cProc").unwrap_or("0").to_owned(),
            block
                .text("Script@GPLFunction")
                .unwrap_or_default()
                .to_owned(),
        );

        if got != want {
            bad.push(format!("{id} Script: {got:?} vs {want:?}"));
        }
    }

    let (rate, distance) = (action.rate(), action.distance());
    let pairs = [
        ("Rate@min", rate.map(|r| i64::from(r.0))),
        ("Rate@max", rate.map(|r| i64::from(r.1))),
        ("Distance@min", distance.map(|d| i64::from(d.0))),
        ("Distance@max", distance.map(|d| i64::from(d.1))),
        ("TimeoutDuration", action.timeout_duration().map(i64::from)),
        ("SpellRank", action.spell_rank().map(i64::from)),
        ("CharacterLevel", action.character_level().map(i64::from)),
        ("SpellType", action.spell_type().map(i64::from)),
        (
            "EffectorDuration",
            action.effector_duration().map(i64::from),
        ),
    ];

    n + compare(id, &pairs, block, bad)
}

/// Compares decoded values against the XML; returns how many.
fn compare(id: &str, pairs: &[(&str, Option<i64>)], block: &Desc, bad: &mut Vec<String>) -> usize {
    let mut n = 0;

    for (field, got) in pairs {
        let (Some(got), Some(want)) = (got, block.num(field)) else {
            continue;
        };

        n += 1;

        if *got != want {
            bad.push(format!("{id} {field}: binary {got}, xml {want}"));
        }
    }

    n
}

/// `compare` for four-character id fields: `DialogID`, `HelpID`, `NameGenType`.
fn compare_ids(
    id: &str,
    pairs: &[(&str, Option<String>)],
    block: &Desc,
    bad: &mut Vec<String>,
) -> usize {
    let mut n = 0;

    for (field, got) in pairs {
        let (Some(got), Some(want)) = (got, block.text(field)) else {
            continue;
        };

        n += 1;

        if got != want {
            bad.push(format!("{id} {field}: binary {got:?}, xml {want:?}"));
        }
    }

    n
}

// --------------------------------------------------------------------------
// The bit-solver
// --------------------------------------------------------------------------
//
// `Info` and `Flags` are bitfields, one XML element per set bit, so set
// equality pins a bit: a candidate `(word, bit)` survives only if the records
// setting it are exactly those whose XML names the flag.

const SUBTYPES: [desc::UnitKind; 6] = [
    desc::UnitKind::General,
    desc::UnitKind::Projectile,
    desc::UnitKind::Building,
    desc::UnitKind::Character,
    desc::UnitKind::Overlay,
    desc::UnitKind::ParticleSystem,
];

/// The descriptor section holding a bitfield.
#[derive(Clone, Copy)]
enum Section {
    /// `<Engine>`, holding `Info`.
    Prim,
    /// `<Game>`, holding `Flags`.
    Secd,
}

/// Repeated XML elements that look like bitfields, and their sections.
const BITFIELDS: [(Section, &str); 3] = [
    (Section::Prim, "Info"),
    (Section::Prim, "CanUse"),
    (Section::Secd, "Flags"),
];

/// One record's section bytes and the flags its XML declares.
///
/// Owned: a `Part` borrows the archive, which the samples outlive.
struct BitSample {
    kind: Option<desc::UnitKind>,
    section: Vec<u8>,
    declared: BTreeSet<String>,
}

/// The `(word offset, bit)` positions that could hold each named flag.
///
/// One candidate is a solve; none, a contradiction; several, not yet separated.
fn solve_bits(samples: &[BitSample]) -> BTreeMap<String, Vec<(usize, u32)>> {
    let names: BTreeSet<&String> = samples.iter().flat_map(|s| s.declared.iter()).collect();
    let words = samples.iter().map(|s| s.section.len()).max().unwrap_or(0) / 4;

    names
        .into_iter()
        .map(|name| {
            let want: Vec<bool> = samples.iter().map(|s| s.declared.contains(name)).collect();

            let hits = (0..words)
                .flat_map(|w| (0..32).map(move |b| (w * 4, b)))
                .filter(|(at, bit)| {
                    samples
                        .iter()
                        .map(|s| word(&s.section, *at) >> bit & 1 == 1)
                        .eq(want.iter().copied())
                })
                .collect();

            (name.clone(), hits)
        })
        .collect()
}

/// A section word, or zero past the end: an unstored bit is unset.
fn word(section: &[u8], at: usize) -> u32 {
    section
        .get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Bitfield samples for one edition's units.
fn bit_samples(
    archive: &cam::Archive<'_>,
    xml: &BTreeMap<String, Desc>,
    which: Section,
    element: &str,
) -> Vec<BitSample> {
    archive
        .records()
        .filter(|(chunk, _)| chunk.tag == b"DUNT")
        .filter_map(|(_, r)| {
            let block = xml.get(&r.id.to_string())?;
            let part = desc::Part::parse(r.data).ok()?;
            let unit = desc::Unit::parse(&part).ok()?;

            let section = match which {
                Section::Prim => unit.prim,
                Section::Secd => unit.secd,
            };

            let fields = match which {
                Section::Prim => &block.engine,
                Section::Secd => &block.game,
            };

            Some(BitSample {
                kind: Some(unit.kind()),
                section: section.to_vec(),
                declared: fields
                    .get(element)
                    .map(|v| v.iter().cloned().collect())
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Print where each `Info` and `Flags` value lives. Run with `--nocapture`.
#[test]
#[ignore = "a solver, not an assertion: run it when adding a flag"]
fn solve_the_descriptor_bitfields() {
    let root = common::hd();

    let sdk = common::sdk();

    for (which, element) in BITFIELDS {
        // Both editions: the base game declares `NoFlaggable` and
        // `NotSpellTarget` on the same 14 buildings; only the expansion
        // separates them.
        let all: Vec<BitSample> = EDITIONS
            .iter()
            .flat_map(|corpus| {
                let xml = sdk_descriptions(&sdk, corpus.unit_xml);
                let data = std::fs::read(root.join(corpus.units)).unwrap();
                let archive = cam::Archive::parse(&data).unwrap();

                bit_samples(&archive, &xml, which, element)
            })
            .collect();

        // `PRIM` is common to all subtypes, so solve it at once. `SECD`'s
        // layout varies by subtype, so solve per subtype.
        let groups: Vec<(String, Vec<&BitSample>)> = match which {
            Section::Prim => vec![("all".to_owned(), all.iter().collect())],
            Section::Secd => SUBTYPES
                .iter()
                .map(|k| {
                    let of_kind = all.iter().filter(|s| s.kind == Some(*k)).collect();
                    (format!("{k:?}"), of_kind)
                })
                .collect(),
        };

        for (group, refs) in groups {
            let samples: Vec<BitSample> = refs
                .into_iter()
                .map(|s| BitSample {
                    kind: s.kind,
                    section: s.section.clone(),
                    declared: s.declared.clone(),
                })
                .collect();

            if samples.is_empty() {
                continue;
            }

            println!("\n{element} {group}, {} records", samples.len());

            for (name, hits) in solve_bits(&samples) {
                let n = samples
                    .iter()
                    .filter(|s| s.declared.contains(&name))
                    .count();
                let where_ = hits
                    .iter()
                    .map(|(at, bit)| format!("{at:#04x}:{bit}"))
                    .collect::<Vec<_>>();

                println!("  {name:<34} [{n:>3}]  {}", where_.join(" "));
            }
        }
    }
}

/// Every `DSND` body is a 12-byte head, then whole phase entries.
///
/// `Sound::parse` enforces this, so its phase iterator is infallible.
#[test]
fn sound_payload_is_a_phase_array() {
    let root = common::hd();
    let (mut records, mut phases) = (0, 0);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DSND" {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let sound = desc::Sound::parse(&part)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));

            records += 1;
            phases += sound.phases().count();

            assert_eq!(
                sound.phases().count(),
                sound.phase_count(),
                "{} {} short phase array",
                path.display(),
                r.id
            );
        }
    }

    assert_eq!(records, 448, "DSND record count changed");
    assert_eq!(phases, 1_766, "DSND phase count changed");
}

/// `DSND` against `M_Sounds.xml`, phase by phase, field by field.
#[test]
fn sound_descriptors_match_the_sdk_xml() {
    let root = common::hd();

    let sdk = common::sdk();

    // `DSDP` and `DSDG` ship only in the base archive; expansion sounds use
    // them too.
    let base = std::fs::read(root.join(BASE.sounds)).unwrap();
    let tables = cam::Archive::parse(&base).unwrap();
    let phase_names = head_names(&tables, *b"DSDP");
    let group_names = head_names(&tables, *b"DSDG");

    assert_eq!(phase_names.len(), 65, "DSDP record count changed");
    assert_eq!(group_names.len(), 25, "DSDG record count changed");

    let mut records = 0;
    let mut phases = 0;
    let mut dangling = 0;
    let mut unnamed = 0;
    let mut no_xml: Vec<String> = Vec::new();
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        let xml = sdk_descriptions(&sdk, corpus.sound_xml);
        let data = std::fs::read(root.join(corpus.sounds)).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DSND" {
                continue;
            }

            let id = r.id.to_string();

            let Some(block) = xml.get(&id) else {
                no_xml.push(id);
                continue;
            };

            let part = desc::Part::parse(r.data).unwrap();
            let sound = desc::Sound::parse(&part).unwrap();
            let got: Vec<desc::Phase<'_>> = sound.phases().collect();

            records += 1;

            // Binary count word against XML phase elements.
            assert_eq!(
                got.len(),
                block.phases.len(),
                "{id} phase count ({})",
                corpus.sounds
            );

            for (i, (g, w)) in got.iter().zip(&block.phases).enumerate() {
                phases += 1;

                let (d, u) = check_phase(&id, i, g, w, &phase_names, &group_names, &mut bad);
                dangling += d;
                unnamed += u;
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(
        no_xml,
        Vec::<String>::new(),
        "sound records without XML changed"
    );
    assert_eq!(records, 442, "sound records with XML changed");
    assert_eq!(phases, 1_750, "sound phase count changed");
    // Four phases name `AS01` or `ETA0`, which have no `DSDP` record.
    assert_eq!(dangling, 4, "dangling phase references changed");
    assert_eq!(unnamed, 4, "phase flag bits with no name changed");
}

/// Compares one phase against its XML.
///
/// Returns dangling `DSDP` references and unnamed flag bits: data quirks, not
/// decode errors.
fn check_phase(
    id: &str,
    i: usize,
    got: &desc::Phase<'_>,
    want: &PhaseXml,
    phase_names: &BTreeMap<String, String>,
    group_names: &BTreeMap<String, String>,
    bad: &mut Vec<String>,
) -> (usize, usize) {
    let mut check = |what: &str, got: Option<String>, want: Option<String>| {
        if got != want {
            bad.push(format!("{id} phase {i} {what}: {got:?} vs {want:?}"));
        }
    };

    check("wave", got.wave().map(|f| f.to_string()), want.wave.clone());

    // The binary stores ids, the XML their names: the lookup checks the
    // join. A missing `DSDP` falls back to the raw id, as the exporter did.
    let resolve = |id: Option<regent_core::data::fourcc::FourCc>,
                   table: &BTreeMap<String, String>| {
        id.map(|f| {
            table
                .get(&f.to_string())
                .cloned()
                .unwrap_or_else(|| f.to_string())
        })
    };

    check(
        "group",
        resolve(got.group(), group_names),
        want.group.clone(),
    );
    check(
        "phase id",
        resolve(got.id(), phase_names),
        Some(want.id.clone()),
    );
    check(
        "volume",
        got.volume().map(|(u, d)| format!("{u} {d}")),
        want.volume.map(|(u, d)| format!("{u} {d}")),
    );
    check(
        "frequency",
        got.frequency().map(|(u, d)| format!("{u} {d}")),
        want.frequency.map(|(u, d)| format!("{u} {d}")),
    );

    // Exact to one decimal: both sides come from one authored value.
    check(
        "distance",
        got.distance().map(|v| format!("{v:.1}")),
        want.distance.map(|v| format!("{v:.1}")),
    );

    // The exporter writes a flag's name, or its raw mask when unnamed:
    // `<Flags value="2"/>` is bit 1. Four phases carry one.
    let (named, numeric): (Vec<&String>, Vec<&String>) =
        want.flags.iter().partition(|f| f.parse::<u32>().is_err());

    let got_flags: BTreeSet<&str> = got.flag_names().map(desc::PhaseFlag::name).collect();
    let want_flags: BTreeSet<&str> = named.iter().map(|s| s.as_str()).collect();

    if got_flags != want_flags {
        bad.push(format!(
            "{id} phase {i} flags: {got_flags:?} vs {want_flags:?}"
        ));
    }

    for n in &numeric {
        let mask: u32 = n.parse().unwrap();

        if got.flags().is_none_or(|w| w & mask == 0) {
            bad.push(format!("{id} phase {i} flag mask {mask}"));
        }
    }

    let dangling = usize::from(
        got.id()
            .is_some_and(|f| !phase_names.contains_key(&f.to_string())),
    );

    (dangling, numeric.len())
}

/// A chunk's records, id to `HEAD` name.
fn head_names(archive: &cam::Archive<'_>, tag: [u8; 4]) -> BTreeMap<String, String> {
    archive
        .records()
        .filter(|(chunk, _)| chunk.tag == &tag)
        .filter_map(|(_, r)| {
            let part = desc::Part::parse(r.data).ok()?;
            let head = desc::Head::parse(part.find(b"HEAD")?.body).ok()?;

            Some((
                r.id.to_string(),
                String::from_utf8_lossy(head.name).into_owned(),
            ))
        })
        .collect()
}

/// `PRIM`'s tail accounts for every byte of every unit descriptor.
///
/// Engine block, callback string, optional `ATCH` tree and sound tail sum to
/// the declared length. Counts below sum HD's four unit datasets;
/// `editions.rs` checks each:
///
/// ```text
///            records  ATCH  WEAP  MOVE  overrides  scripts
/// base           394   249   226    93          5       68
/// expansion      174    90    73    40          0       45
/// mdl1             9     6     6     5          0        0
/// xqd1             6     4     4     2          0        1
/// ```
#[test]
fn unit_prim_tails_account_for_every_byte() {
    let root = common::hd();

    let mut records = 0;
    let mut with_atch = 0;
    let mut with_weapon = 0;
    let mut movement = 0;
    let mut overrides = 0;
    let mut occupied = 0;
    let mut scripts = 0;
    let mut lengths: BTreeSet<usize> = BTreeSet::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));
            let tail = unit.tail();

            records += 1;
            overrides += tail.overrides().count();
            scripts += usize::from(!tail.script.is_empty());

            // The callback's length shifts what follows, so `ATCH` has no
            // fixed offset.
            lengths.insert(tail.script.len());

            if let Some(a) = tail.attachments {
                with_atch += 1;
                movement += usize::from(a.movement_class().is_some());
                with_weapon += usize::from(a.weapon.is_some());

                // Never filled in the shipped corpus.
                occupied +=
                    a.explosion.filled().count() + a.weapon.map_or(0, |w| w.filled().count());
            }
        }
    }

    assert_eq!(records, 583, "DUNT record count changed");
    assert_eq!(with_atch, 349, "records carrying ATCH changed");
    assert_eq!(with_weapon, 309, "records carrying WEAP changed");
    assert_eq!(movement, 140, "movement attachments changed");
    assert_eq!(overrides, 5, "sound overrides changed");
    assert_eq!(scripts, 114, "units with a GPL callback changed");
    assert_eq!(
        occupied, 0,
        "an EXPL or WEAP slot is occupied — their meaning is unsolved"
    );
    assert!(
        lengths.len() > 1,
        "callback strings are all one length, so ATCH would sit at a fixed offset"
    );
}

/// Shipped quests join the base and expansion. Repeated ids replace earlier
/// descriptors; signs, chests, and zero-health records are not lore entries.
#[test]
fn shipped_character_and_building_ids() {
    let root = common::hd();
    let mut kinds = BTreeMap::new();
    let mut characters = BTreeMap::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, record) in archive.records() {
            if chunk.tag == b"DUNT" {
                let part = desc::Part::parse(record.data).unwrap();
                let unit = desc::Unit::parse(&part).unwrap();

                kinds.insert(
                    record.id,
                    (
                        unit.kind(),
                        unit.building().and_then(|b| b.dialog_id()),
                        unit.building().and_then(|b| b.max_hp()).unwrap_or_default(),
                    ),
                );

                if let Some(character) = unit.character() {
                    characters.insert(
                        record.id,
                        (
                            character.max_hp().unwrap_or_default(),
                            character.has(desc::CharacterFlag::HasHpBar),
                        ),
                    );
                }
            }
        }
    }

    assert_eq!(
        kinds
            .values()
            .filter(|(kind, _, _)| *kind == desc::UnitKind::Character)
            .count(),
        104
    );
    assert_eq!(
        kinds
            .values()
            .filter(|(kind, _, _)| *kind == desc::UnitKind::Building)
            .count(),
        117
    );

    let chest = regent_core::view::panel::portrait::details::CHEST_PANEL;
    let sign = regent_core::view::panel::portrait::details::SIGN_PANEL;
    let without_posts = kinds
        .values()
        .filter(|(kind, dialog, _)| {
            *kind == desc::UnitKind::Building && *dialog != Some(chest) && *dialog != Some(sign)
        })
        .count();

    assert_eq!(without_posts, 104);

    let lore_buildings = kinds
        .values()
        .filter(|(kind, dialog, hp)| {
            *kind == desc::UnitKind::Building
                && *dialog != Some(chest)
                && *dialog != Some(sign)
                && *hp > 0
        })
        .count();

    assert_eq!(lore_buildings, 102);

    let living = characters
        .values()
        .filter(|(hp, bar)| *hp > 0 && *bar)
        .count();
    let resources = characters
        .values()
        .filter(|(hp, bar)| *hp > 0 && !*bar)
        .count();

    assert_eq!((living, resources), (80, 4));
}

/// Every particle system's block is 272 bytes and parses.
///
/// `ParticleSystem::parse` enforces the length, so its section views are
/// infallible.
#[test]
fn particle_blocks_are_a_fixed_size() {
    let root = common::hd();
    let mut seen = 0;

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part)
                .unwrap_or_else(|e| panic!("{} {}: {e}", path.display(), r.id));

            if unit.particle_system().is_some() {
                seen += 1;
            }
        }
    }

    assert_eq!(seen, 22, "particle system count changed");
}

/// `PRIM`'s version word is 5 exactly on particle systems.
///
/// Independent of `HEAD`'s subtype word, which the reader uses. Their agreement
/// supports the 272-byte block.
#[test]
fn the_prim_version_agrees_with_the_subtype() {
    let root = common::hd();
    let mut versions: BTreeMap<u32, usize> = BTreeMap::new();

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let prim = part.find(b"PRIM").expect("every DUNT has a PRIM");
            let unit = desc::Unit::parse(&part).unwrap();
            let particle = unit.kind() == desc::UnitKind::ParticleSystem;

            assert_eq!(
                prim.version == 5,
                particle,
                "{} {}: PRIM version {} on a {:?}",
                path.display(),
                r.id,
                prim.version,
                unit.kind()
            );

            *versions.entry(prim.version).or_default() += 1;
        }
    }

    assert_eq!(
        versions,
        BTreeMap::from([(4, 561), (5, 22)]),
        "DUNT PRIM version counts changed"
    );
}

/// The 272-byte `ParticleSystem` block against the XML, every named field of
/// all 22 records.
///
/// Evidence for the tiling argument in `desc::particle`.
#[test]
fn particle_systems_match_the_sdk_xml() {
    let root = common::hd();

    let sdk = common::sdk();

    let mut records = 0;
    let mut compared = 0;
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        let xml = sdk_descriptions(&sdk, corpus.unit_xml);
        let data = std::fs::read(root.join(corpus.units)).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let id = r.id.to_string();
            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part).unwrap();

            let Some(system) = unit.particle_system() else {
                continue;
            };

            let block = xml.get(&id).expect("every particle system has XML");

            records += 1;
            compared += check_particles(&id, &system, block, &mut bad);
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(records, 22, "particle system count changed");
    assert_eq!(compared, 1_386, "particle comparison count changed");
}

/// Compares one particle system against its XML; returns how many values.
///
/// Everything is a float list, so one loop covers scalars, vectors, colours
/// and `in`/`out` pairs.
fn check_particles(
    id: &str,
    system: &desc::ParticleSystem<'_>,
    block: &Desc,
    bad: &mut Vec<String>,
) -> usize {
    let mut n = 0;

    for (what, got, want) in particle_pairs(system, block) {
        let (Some(got), Some(want)) = (got, want) else {
            continue;
        };

        for (i, (a, b)) in got.iter().zip(&want).enumerate() {
            n += 1;

            // Both sides come from one authored decimal; a difference is a
            // decode error, not drift.
            if (a - b).abs() > 1e-4 * b.abs().max(1.0) {
                bad.push(format!("{id} {what}[{i}]: {a} vs {b}"));
            }
        }
    }

    // The two bitfields, by set equality.
    let named = |f: &Fields, k: &str| -> BTreeSet<String> {
        f.get(k)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default()
    };

    let e = system.emitter();
    let shapes: BTreeSet<String> = e.shapes().map(|s| s.name().to_owned()).collect();
    let flags: BTreeSet<String> = e.flags().map(|f| f.name().to_owned()).collect();

    for (what, got, want) in [
        ("Emitter.Type", shapes, named(&block.emitter, "Type")),
        ("Emitter.Flags", flags, named(&block.emitter, "Flags")),
    ] {
        n += 1;

        if got != want {
            bad.push(format!("{id} {what}: {got:?} vs {want:?}"));
        }
    }

    n
}

/// Per named field: decoded value, XML value.
type Pairs<'a> = Vec<(&'a str, Option<Vec<f64>>, Option<Vec<f64>>)>;

#[allow(clippy::too_many_lines, reason = "a field table, not logic")]
fn particle_pairs<'a>(system: &desc::ParticleSystem<'_>, block: &Desc) -> Pairs<'a> {
    let e = system.emitter();
    let p = system.particle();

    let one = |f: &Fields, k: &str| xml_vec(f, k).filter(|v| v.len() == 1);
    let v3 = |v: (f32, f32, f32)| vec![f64::from(v.0), f64::from(v.1), f64::from(v.2)];
    let v4 = |c: (f32, f32, f32, f32)| {
        vec![
            f64::from(c.0),
            f64::from(c.1),
            f64::from(c.2),
            f64::from(c.3),
        ]
    };
    let bounds = system.bounds();

    // `None` on either side: field not carried; skipped.
    let pairs: Pairs = vec![
        (
            "AttachmentPointID",
            system.attach_point().map(|v| vec![f64::from(v)]),
            one(&block.engine, "AttachmentPointID"),
        ),
        (
            "InitialParticles",
            system.initial_particles().map(|v| vec![f64::from(v)]),
            one(&block.engine, "InitialParticles"),
        ),
        (
            "MaxParticles",
            system.max_particles().map(|v| vec![f64::from(v)]),
            one(&block.engine, "MaxParticles"),
        ),
        (
            "Bounds.min",
            bounds.map(|(a, _)| v3(a)),
            xml_vec(&block.engine, "Bounds@min"),
        ),
        (
            "Bounds.max",
            bounds.map(|(_, b)| v3(b)),
            xml_vec(&block.engine, "Bounds@max"),
        ),
        (
            "Velocity",
            e.velocity().map(v3),
            xml_vec(&block.emitter, "Velocity"),
        ),
        (
            "Acceleration",
            e.acceleration().map(v3),
            xml_vec(&block.emitter, "Acceleration"),
        ),
        (
            "Offset",
            e.offset().map(v3),
            xml_vec(&block.emitter, "Offset"),
        ),
        (
            "SurfaceNormal",
            e.surface_normal().map(v3),
            xml_vec(&block.emitter, "SurfaceNormal"),
        ),
        (
            "Radius",
            e.radius().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "Radius"),
        ),
        (
            "RadiusVariance",
            e.radius_variance().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "RadiusVariance"),
        ),
        (
            "DirectionVariance",
            e.direction_variance().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "DirectionVariance"),
        ),
        (
            "Rate",
            e.rate().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "Rate"),
        ),
        (
            "Emitter.Lifespan",
            e.lifespan().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "Lifespan"),
        ),
        (
            "Recycle",
            e.recycle().map(|v| vec![f64::from(v)]),
            one(&block.emitter, "Recycle"),
        ),
        (
            "Particle.Lifespan",
            p.lifespan().map(|v| vec![f64::from(v)]),
            one(&block.particle, "Lifespan"),
        ),
        (
            "LifespanVariance",
            p.lifespan_variance().map(|v| vec![f64::from(v)]),
            one(&block.particle, "LifespanVariance"),
        ),
        (
            "InitialSpeed",
            p.initial_speed().map(|v| vec![f64::from(v)]),
            one(&block.particle, "InitialSpeed"),
        ),
        (
            "InitialSpeedVariance",
            p.initial_speed_variance().map(|v| vec![f64::from(v)]),
            one(&block.particle, "InitialSpeedVariance"),
        ),
        (
            "Gravity",
            p.gravity().map(v3),
            xml_vec(&block.particle, "Gravity"),
        ),
        (
            "BirthColor",
            p.birth_colour().map(v4),
            xml_vec(&block.particle, "BirthColor"),
        ),
        (
            "MidlifeColor",
            p.midlife_colour().map(v4),
            xml_vec(&block.particle, "MidlifeColor"),
        ),
        (
            "DeathColor",
            p.death_colour().map(v4),
            xml_vec(&block.particle, "DeathColor"),
        ),
        (
            "BirthSize",
            p.birth_size().map(|v| vec![f64::from(v)]),
            one(&block.particle, "BirthSize"),
        ),
        (
            "MaxSize",
            p.max_size().map(|v| vec![f64::from(v)]),
            one(&block.particle, "MaxSize"),
        ),
        (
            "DeathSize",
            p.death_size().map(|v| vec![f64::from(v)]),
            one(&block.particle, "DeathSize"),
        ),
        // The XML omits `SizeTime` when both halves are zero.
        (
            "SizeTime.in",
            p.size_time().map(|t| vec![f64::from(t.0)]),
            one(&block.particle, "SizeTime@in"),
        ),
        (
            "SizeTime.out",
            p.size_time().map(|t| vec![f64::from(t.1)]),
            one(&block.particle, "SizeTime@out"),
        ),
        (
            "FadeTime.in",
            p.fade_time().map(|t| vec![f64::from(t.0)]),
            one(&block.particle, "FadeTime@in"),
        ),
        (
            "FadeTime.out",
            p.fade_time().map(|t| vec![f64::from(t.1)]),
            one(&block.particle, "FadeTime@out"),
        ),
        (
            "JitterOrientation",
            p.jitter_orientation().map(v3),
            xml_vec(&block.particle, "JitterOrientation"),
        ),
        (
            "JitterDisplacement",
            p.jitter_displacement().map(v3),
            xml_vec(&block.particle, "JitterDisplacement"),
        ),
        (
            "BirthSpinRate",
            p.birth_spin_rate().map(|v| vec![f64::from(v)]),
            one(&block.particle, "BirthSpinRate"),
        ),
        (
            "MidlifeSpinRate",
            p.midlife_spin_rate().map(|v| vec![f64::from(v)]),
            one(&block.particle, "MidlifeSpinRate"),
        ),
        (
            "DeathSpinRate",
            p.death_spin_rate().map(|v| vec![f64::from(v)]),
            one(&block.particle, "DeathSpinRate"),
        ),
    ];

    pairs
}

/// `PRIM`'s tail references against the SDK's XML.
///
/// `Attachment` names a `DMOV`, `DefaultSound` a `DSND`, `SoundOverride` a
/// `DACT` and a `DSND`: ids in the binary, names in the XML, so ids resolve
/// through the target table. `Script`, a GPL function name, compares directly.
#[test]
fn unit_tail_joins_match_the_sdk_xml() {
    let root = common::hd();

    let sdk = common::sdk();

    // Expansion units use base movement classes, so tables are merged.
    let mut moves = BTreeMap::new();
    let mut sounds = BTreeMap::new();
    let mut actions = BTreeMap::new();

    let loaded: Vec<Vec<u8>> = EDITIONS
        .iter()
        .flat_map(|c| [c.units, c.sounds, c.actions])
        .map(|f| std::fs::read(root.join(f)).unwrap())
        .collect();

    for data in &loaded {
        let archive = cam::Archive::parse(data).unwrap();
        moves.extend(head_names(&archive, *b"DMOV"));
        sounds.extend(head_names(&archive, *b"DSND"));
        actions.extend(head_names(&archive, *b"DACT"));
    }

    let mut checked = 0;
    let mut unresolved = 0;
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        let xml = sdk_descriptions(&sdk, corpus.unit_xml);
        let data = std::fs::read(root.join(corpus.units)).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let id = r.id.to_string();

            let Some(block) = xml.get(&id) else {
                continue;
            };

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part).unwrap();
            let tail = unit.tail();

            let mut check = |what: &str, got: Option<String>, want: Option<&str>| {
                checked += 1;

                if got.as_deref() != want {
                    bad.push(format!("{id} {what}: {got:?} vs {want:?}"));
                }
            };

            // The callback, a bare string.
            let script = String::from_utf8_lossy(tail.script).into_owned();
            check(
                "Script",
                (!script.is_empty()).then_some(script),
                block.text("Script@GPLFunction"),
            );

            // Movement class and type.
            let mv = tail.attachments.and_then(|a| a.movement_class());
            check(
                "Attachment ID",
                mv.and_then(|(_, id)| moves.get(&id.to_string()).cloned()),
                block.text("Attachment@ID"),
            );
            check(
                "Attachment type",
                mv.and_then(|(m, _)| m).map(|m| m.name().to_owned()),
                block.text("Attachment@type"),
            );

            // An unresolved id falls back to itself, as the exporter's did.
            if tail
                .default_sound
                .is_some_and(|s| !sounds.contains_key(&s.to_string()))
            {
                unresolved += 1;
            }

            // `ABM1`'s sound is named "Copy of Wizards_Tower", yet the
            // exporter wrote the raw id; accept the raw id where the XML has it.
            let want = block.text("DefaultSound").filter(|v| *v != "0");
            let got = tail.default_sound.map(|s| {
                let raw = s.to_string();

                match sounds.get(&raw) {
                    Some(n) if want != Some(raw.as_str()) => n.clone(),
                    _ => raw,
                }
            });

            check("DefaultSound", got, want);

            for o in tail.overrides() {
                check(
                    "SoundOverride type",
                    actions.get(&o.action.to_string()).cloned(),
                    block.text("SoundOverride@type"),
                );
                check(
                    "SoundOverride ID",
                    sounds.get(&o.sound.to_string()).cloned(),
                    block.text("SoundOverride@ID"),
                );
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(checked, 2_282, "tail comparison count changed");
    assert_eq!(unresolved, 1, "unresolved DefaultSound ids changed");
}

/// Every set bit is one the flag tables name, bar the exceptions pinned below.
#[test]
fn descriptor_bitfields_are_fully_explained() {
    let root = common::hd();

    let named = |bits: &[u32]| bits.iter().fold(0, |m, b| m | 1 << b);

    let info_named = named(&Info_bits());
    let building_named = named(&building_bits());
    let character_named = named(&character_bits());

    let (mut info, mut building, mut character, mut overlay) = (0u32, 0u32, 0u32, 0u32);

    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part).unwrap();

            info |= unit.engine().info().unwrap_or(0);

            if let Some(c) = unit.character() {
                character |= c.flags().unwrap_or(0);
            } else if let Some(b) = unit.building() {
                building |= b.flags().unwrap_or(0);
            } else if unit.kind() == desc::UnitKind::Overlay {
                overlay |= unit.overlay_flags().unwrap_or(0);
            }
        }
    }

    assert_eq!(info & !info_named, 0, "unnamed Info bits: {info:#x}");
    assert_eq!(
        building & !building_named,
        0,
        "unnamed building flag bits: {building:#x}"
    );
    assert_eq!(overlay & !1, 0, "unnamed overlay flag bits: {overlay:#x}");
    assert_eq!(
        character & !character_named,
        0,
        "unnamed character flag bits: {character:#x}"
    );
}

#[allow(non_snake_case, reason = "reads as the field name it collects")]
fn Info_bits() -> Vec<u32> {
    desc::Info::ALL.into_iter().map(|f| f as u32).collect()
}

fn building_bits() -> Vec<u32> {
    desc::BuildingFlag::ALL
        .into_iter()
        .map(|f| f as u32)
        .collect()
}

fn character_bits() -> Vec<u32> {
    desc::CharacterFlag::ALL
        .into_iter()
        .map(|f| f as u32)
        .collect()
}

/// Every solved bit against the XML: a named flag's bit is set, and a set bit
/// is named.
#[test]
fn descriptor_bitfields_match_the_sdk_xml() {
    let root = common::hd();

    let sdk = common::sdk();

    let mut checked = 0;
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        let xml = sdk_descriptions(&sdk, corpus.unit_xml);
        let data = std::fs::read(root.join(corpus.units)).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let id = r.id.to_string();

            let Some(block) = xml.get(&id) else {
                continue;
            };

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part).unwrap();

            let named = |fields: &Fields, element: &str| -> BTreeSet<String> {
                fields
                    .get(element)
                    .map(|v| v.iter().cloned().collect())
                    .unwrap_or_default()
            };

            let info = named(&block.engine, "Info");
            let flags = named(&block.game, "Flags");

            for f in desc::Info::ALL {
                checked += 1;

                if unit.engine().has(f) != info.contains(f.name()) {
                    bad.push(format!("{id} Info {}", f.name()));
                }
            }

            if let Some(c) = unit.character() {
                for f in desc::CharacterFlag::ALL {
                    checked += 1;

                    if c.has(f) != flags.contains(f.name()) {
                        bad.push(format!("{id} character flag {}", f.name()));
                    }
                }
            } else if let Some(b) = unit.building() {
                for f in desc::BuildingFlag::ALL {
                    checked += 1;

                    if b.has(f) != flags.contains(f.name()) {
                        bad.push(format!("{id} building flag {}", f.name()));
                    }
                }
            } else if unit.kind() == desc::UnitKind::Overlay {
                checked += 1;

                if unit.transparent_to_mouse() != flags.contains("TransparentToMouse") {
                    bad.push(format!("{id} TransparentToMouse"));
                }
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(checked, 13_313, "bit comparison count changed");
}

/// `CanUse` and `Category` against the SDK XML, which prints what the engine
/// holds after loading: the `CanUse` word is zero in every file, and the
/// loader's default by id letter gives every declaration.
#[test]
fn the_loader_fills_can_use_from_the_ids_first_letter() {
    let root = common::hd();

    let sdk = common::sdk();

    let mut declared: BTreeMap<&str, usize> = BTreeMap::new();
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        let xml = sdk_descriptions(&sdk, corpus.unit_xml);
        let data = std::fs::read(root.join(corpus.units)).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag != b"DUNT" {
                continue;
            }

            let id = r.id.to_string();

            let Some(block) = xml.get(&id) else {
                continue;
            };

            let part = desc::Part::parse(r.data).unwrap();
            let unit = desc::Unit::parse(&part).unwrap();
            let values = |name: &str| block.engine.get(name).cloned().unwrap_or_default();

            let can_use = values("CanUse");

            for user in desc::User::ALL {
                let named = can_use.iter().any(|v| v == user.name());

                if named {
                    *declared.entry(user.name()).or_default() += 1;
                }

                if unit.can_use(user) != named {
                    bad.push(format!("{id} CanUse {}", user.name()));
                }
            }

            // Only the four reward flags declare one; the rest hold 0.
            let category = values("Category")
                .first()
                .map_or(Some(0), |v| v.parse().ok());

            if unit.engine().category() != category {
                bad.push(format!("{id} Category"));
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    assert_eq!(
        declared,
        BTreeMap::from([("HumanPlayer", 142), ("Monster", 116)])
    );

    // The file word, in all four datasets.
    for path in archives(&root) {
        let data = std::fs::read(&path).unwrap();
        let archive = cam::Archive::parse(&data).unwrap();

        for (chunk, r) in archive.records() {
            if chunk.tag == b"DUNT" {
                let part = desc::Part::parse(r.data).unwrap();
                let unit = desc::Unit::parse(&part).unwrap();

                assert_eq!(unit.engine().can_use(), Some(0), "{}", r.id);
            }
        }
    }
}

/// Typed descriptor fields against the SDK's XML.
///
/// `data::desc`'s offsets were solved by matching XML values to the binary;
/// this checks them over every unit and action of both editions.
#[test]
fn descriptor_fields_match_the_sdk_xml() {
    let root = common::hd();

    let sdk = common::sdk();

    let mut compared = 0;
    let mut bad: Vec<String> = Vec::new();

    for corpus in EDITIONS {
        // Separate maps: `CRa1` is both an overlay and an action.
        let units = sdk_descriptions(&sdk, corpus.unit_xml);
        let actions = sdk_descriptions(&sdk, corpus.action_xml);

        assert!(
            !units.is_empty() && !actions.is_empty(),
            "no descriptor XML in the SDK"
        );

        for file in [corpus.units, corpus.actions] {
            let data = std::fs::read(root.join(file)).unwrap();
            let archive = cam::Archive::parse(&data).unwrap();

            for (chunk, r) in archive.records() {
                let id = r.id.to_string();
                let table = if chunk.tag == b"DUNT" {
                    &units
                } else {
                    &actions
                };

                let Some(block) = table.get(&id) else {
                    continue;
                };

                let part = desc::Part::parse(r.data).unwrap();

                compared += if chunk.tag == b"DUNT" {
                    check_unit(&id, &desc::Unit::parse(&part).unwrap(), block, &mut bad)
                } else {
                    check_action(&id, &desc::Action::parse(&part).unwrap(), block, &mut bad)
                };
            }
        }
    }

    assert!(
        bad.is_empty(),
        "{} disagree: {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
    // 3,226 fields plus 135 `DACT` script blocks.
    assert_eq!(compared, 3_361, "field comparison count changed");
}

/// **Game**: an Attack flag will not go on a Graveyard, a treasure chest or a
/// Hooligan, and does go on the player's own Palace. The three refused carry
/// `NoFlaggable` / `NotFlaggable`; the Palace does not. So the flag bit, not
/// `CanUse`, is what refuses a flag.
#[test]
fn a_flag_is_refused_where_the_flaggable_bit_says() {
    const REFUSED_BUILDINGS: [&[u8; 4]; 2] = [b"BBJ1", b"BBt1"];
    const REFUSED_CHARACTERS: [&[u8; 4]; 1] = [b"AVh1"];
    const ACCEPTED_BUILDINGS: [&[u8; 4]; 1] = [b"ABJ1"];

    let data = std::fs::read(common::hd().join(BASE.units)).unwrap();
    let archive = cam::Archive::parse(&data).unwrap();

    let unit = |id: &[u8; 4]| {
        let (_, r) = archive
            .records()
            .find(|(c, r)| c.tag == b"DUNT" && r.id.0 == *id)
            .unwrap();
        let part = desc::Part::parse(r.data).unwrap();
        desc::Unit::parse(&part).unwrap()
    };

    let refused = |id: &[u8; 4]| {
        unit(id)
            .building()
            .unwrap()
            .has(desc::BuildingFlag::NoFlaggable)
    };

    assert!(REFUSED_BUILDINGS.iter().all(|id| refused(id)));
    assert!(ACCEPTED_BUILDINGS.iter().all(|id| !refused(id)));
    assert!(REFUSED_CHARACTERS.iter().all(|id| {
        unit(id)
            .character()
            .unwrap()
            .has(desc::CharacterFlag::NotFlaggable)
    }));
}
