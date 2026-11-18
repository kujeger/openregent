//! A made-up base game and a made-up mod over it, as archives, built at
//! test time. No retail bytes or text: every code, name and value is
//! invented.
//!
//! ```text
//!  base_art.cam    TILE (flagged) 0..4, SPLT (flagged), IMAG ZBA1
//!  base_units.cam  base_units.xml ──> desc::xml
//!  base_misc.cam   DATA BDEP base_bdep.txt
//!  ── the mod, after ──────────────────────────────────────────────
//!  mod_art.cam     TILE (flagged): 0..4 copies of the base's at the
//!                  base's positions, 4..12 empty padding, 12.. its own;
//!                  SPLT (unflagged, ids are numbers); IMAG ZMA2..ZMA5
//!  mod_misc.cam    DATA BDEP mod_bdep.txt, STRT
//!  mod_sound.cam   WAVE ZMW1
//!  made.cam        mod_units.xml ──> desc::xml, over all of the above
//! ```
//!
//! Byte layouts follow the format modules: `data::tile`, `data::imag`,
//! `data::payload`, `data::cam`.

// Each test uses a subset.
#![allow(dead_code)]

use regent_core::data::cam::{self, ChunkFlag, Written};
use regent_core::data::catalogue::Catalogue;
use regent_core::data::desc::xml;
use regent_core::data::fourcc::FourCc;
use regent_core::data::imag::{INTERFACE_ICON, INTERFACE_PANEL, INTERFACE_PORTRAIT, SetKind};
use regent_core::data::tile::{SPLT_PALETTE, VERSION_KEYED, VERSION_OPAQUE, VERSION_RLE};

/// Named archives, in load order.
pub type Archives = Vec<(String, Vec<u8>)>;

/// The base's hero art: a stand pose and the three interface pictures.
pub const BASE_HERO_ART: FourCc = FourCc::new(b"ZBA1");

/// The mod's hero art, [`BASE_HERO_ART`]'s set ids over its own tiles.
pub const HERO_ART: FourCc = FourCc::new(b"ZMA4");

/// The mod's other art, one per `IMAG` layout: versions 2, 3 and 5.
pub const ART_V2: FourCc = FourCc::new(b"ZMA2");
pub const ART_V3: FourCc = FourCc::new(b"ZMA3");
pub const ART_V5: FourCc = FourCc::new(b"ZMA5");

/// The mod's `IMAG`s, in its chunk.
pub const MOD_IMAGES: [FourCc; 4] = [HERO_ART, ART_V2, ART_V3, ART_V5];

/// A base unit the mod's XML replaces whole.
pub const SENTRY: FourCc = FourCc::new(b"ZBS1");

/// A base building the mod's XML gives an upgrade, [`POST_2`].
pub const POST: FourCc = FourCc::new(b"ZBT1");
pub const POST_2: FourCc = FourCc::new(b"ZMT2");

/// The guild only the mod's XML defines; its `BDEP` names it.
pub const CRYPT: FourCc = FourCc::new(b"ZMG1");

/// The mod's sound, its wave, and its string table.
pub const SOUND: FourCc = FourCc::new(b"ZMS1");
pub const WAVE: FourCc = FourCc::new(b"ZMW1");
pub const STRINGS: FourCc = FourCc::new(b"ZMTX");

/// The `DATA` record the engine reads the building rules from.
pub const BDEP: FourCc = FourCc::new(b"BDEP");

pub const BASE_BDEP: &str = include_str!("base_bdep.txt");
pub const MOD_BDEP: &str = include_str!("mod_bdep.txt");
const BASE_UNITS: &str = include_str!("base_units.xml");
const MOD_UNITS: &str = include_str!("mod_units.xml");

/// The interface pictures' sizes: portrait, hero panel, list icon
/// (`docs/formats/art.md`, the three `Interface` sets).
pub const PORTRAIT: (u16, u16) = (100, 100);
pub const PANEL: (u16, u16) = (200, 245);
pub const ICON: (u16, u16) = (25, 25);

/// The base's tiles, by position: a stand pose, then the interface
/// pictures. The mod copies them to the same positions.
const BASE_TILES: [(&str, u16, (u16, u16)); 4] = [
    ("stand", VERSION_RLE, (8, 6)),
    ("portrait", VERSION_OPAQUE, PORTRAIT),
    ("panel", VERSION_OPAQUE, PANEL),
    ("icon", VERSION_RLE, ICON),
];

/// Copies of the base's tiles at the head of the mod's chunk.
pub const COPIED: usize = BASE_TILES.len();

/// Empty `TILE` records after the copies: padding no frame names.
pub const PADDING: usize = OWN as usize - COPIED;

/// The mod's first own tile: past the copies and the padding.
const OWN: u16 = 12;

/// The mod's own tiles, from [`OWN`] on: name, version, size, colours.
const MOD_TILES: [(&str, u16, (u16, u16), Colours); 7] = [
    ("shade_stand", VERSION_RLE, (8, 6), Colours::Splt(1)),
    ("shade_glow", VERSION_RLE, (6, 4), Colours::Embedded),
    ("shade_card", VERSION_OPAQUE, (5, 3), Colours::Direct565),
    ("shade_mist", VERSION_KEYED, (4, 4), Colours::Splt(0)),
    ("shade_portrait", VERSION_RLE, PORTRAIT, Colours::Splt(0)),
    ("shade_panel", VERSION_OPAQUE, PANEL, Colours::Splt(0)),
    ("shade_icon", VERSION_KEYED, ICON, Colours::Splt(1)),
];

/// Tile positions of [`MOD_TILES`].
const SHADE_STAND: u16 = OWN;
const SHADE_GLOW: u16 = OWN + 1;
const SHADE_CARD: u16 = OWN + 2;
const SHADE_MIST: u16 = OWN + 3;
const SHADE_PORTRAIT: u16 = OWN + 4;
const SHADE_PANEL: u16 = OWN + 5;
const SHADE_ICON: u16 = OWN + 6;

/// The mod's own tiles' names: content the base lacks.
pub fn own_tile_names() -> impl Iterator<Item = &'static str> {
    MOD_TILES.iter().map(|(name, ..)| *name)
}

/// The base's tile names: the mod's copies carry them.
pub fn copied_tile_names() -> impl Iterator<Item = &'static str> {
    BASE_TILES.iter().map(|(name, ..)| *name)
}

/// Palette index the base's pictures are filled with, and the mod's: the
/// art the mod swaps in.
pub const BASE_FILL: u8 = 1;
pub const MOD_FILL: u8 = 2;

/// Where an indexed tile's colours come from, or none.
#[derive(Clone, Copy)]
enum Colours {
    /// The archive's `SPLT` this number names.
    Splt(u32),
    /// A palette in the tile's own payload.
    Embedded,
    /// Two bytes a pixel, 565; no palette.
    Direct565,
}

/// The mod's frames, every one over all its `IMAG`s.
pub const MOD_FRAMES: usize = 4 + 1 + 1 + 3 + 1 + 2;

// --------------------------------------------------------------------------
// The archives
// --------------------------------------------------------------------------

/// The base game's archives, in load order.
pub fn base() -> Archives {
    let art = base_art();
    let units = {
        let owned = vec![("base_art.cam".to_owned(), art.clone())];
        let catalogue = Catalogue::build(&owned).expect("the base's art");

        made(BASE_UNITS, &catalogue).archive()
    };

    vec![
        ("base_art.cam".to_owned(), art),
        ("base_units.cam".to_owned(), units),
        ("base_misc.cam".to_owned(), data(&[(BDEP, BASE_BDEP)], &[])),
    ]
}

/// The mod's own archives, in load order: no descriptions.
pub fn mod_cams() -> Archives {
    let strings = string_table(&[(FourCc::new(b"ZMH1"), "Shade"), (CRYPT, "Crypt")]);
    let wave = wave();

    vec![
        ("mod_art.cam".to_owned(), mod_art()),
        (
            "mod_misc.cam".to_owned(),
            data(&[(BDEP, MOD_BDEP)], &[(STRINGS, "ShadeText", &strings)]),
        ),
        (
            "mod_sound.cam".to_owned(),
            cam::write(&[Written {
                tag: FourCc::new(b"WAVE"),
                flag: ChunkFlag::Clear,
                records: vec![(WAVE, "moan", wave.as_slice())],
            }]),
        ),
    ]
}

/// The mod's XML made into records over `catalogue`, as HD makes them
/// at load.
pub fn mod_descriptions(catalogue: &Catalogue<'_>) -> xml::Compiled {
    made(MOD_UNITS, catalogue)
}

/// The base, the mod's archives, then its descriptions made over both:
/// the game with the mod loaded.
pub fn composed() -> Archives {
    let mut out = base();

    out.extend(mod_cams());

    let made = {
        let catalogue = Catalogue::build(&out).expect("the archives");

        mod_descriptions(&catalogue).archive()
    };

    out.push(("made.cam".to_owned(), made));
    out
}

/// `text`'s descriptions, made over `catalogue`.
fn made(text: &str, catalogue: &Catalogue<'_>) -> xml::Compiled {
    let tree = regent_core::data::xml::parse(text).expect("well-formed XML");
    let made = xml::compile(&[&tree], catalogue, &xml::ImageSets::default());

    assert!(made.refused.is_empty(), "{:?}", made.refused);
    made
}

/// An archive of `DATA` text records, and `STRT` tables.
fn data(texts: &[(FourCc, &str)], tables: &[(FourCc, &str, &[u8])]) -> Vec<u8> {
    let mut chunks = vec![Written {
        tag: FourCc::new(b"DATA"),
        flag: ChunkFlag::Clear,
        records: texts
            .iter()
            .map(|(id, text)| (*id, "rules", text.as_bytes()))
            .collect(),
    }];

    if !tables.is_empty() {
        chunks.push(Written {
            tag: FourCc::new(b"STRT"),
            flag: ChunkFlag::Clear,
            records: tables.to_vec(),
        });
    }

    cam::write(&chunks)
}

/// The base's art: its tiles, one palette, its hero's `IMAG`.
fn base_art() -> Vec<u8> {
    let tiles: Vec<Vec<u8>> = BASE_TILES
        .iter()
        .map(|(_, version, size, ..)| tile(*version, *size, Colours::Splt(0), BASE_FILL))
        .collect();
    let palette = palette(0x10);
    let hero = image(4, &hero_sets([0, 1, 2, 3]));

    cam::write(&[
        Written {
            tag: FourCc::new(b"TILE"),
            flag: ChunkFlag::Set,
            records: positional(BASE_TILES.iter().map(|(n, ..)| *n).zip(&tiles)),
        },
        Written {
            tag: FourCc::new(b"SPLT"),
            flag: ChunkFlag::Set,
            records: vec![(FourCc::from_word(0), "base", palette.as_slice())],
        },
        Written {
            tag: FourCc::new(b"IMAG"),
            flag: ChunkFlag::Clear,
            records: vec![(BASE_HERO_ART, "SentryArt", hero.as_slice())],
        },
    ])
}

/// The mod's art: the base's tiles copied in place, padding, its own
/// tiles; two palettes by id, stored out of order; four `IMAG`s.
fn mod_art() -> Vec<u8> {
    let copies: Vec<Vec<u8>> = BASE_TILES
        .iter()
        .map(|(_, version, size, ..)| tile(*version, *size, Colours::Splt(0), BASE_FILL))
        .collect();
    let own: Vec<Vec<u8>> = MOD_TILES
        .iter()
        .map(|(_, version, size, colours)| tile(*version, *size, *colours, MOD_FILL))
        .collect();
    let names = copied_tile_names()
        .chain(std::iter::repeat_n("", PADDING))
        .chain(own_tile_names());
    let empty = Vec::new();
    let payloads = copies
        .iter()
        .chain(std::iter::repeat_n(&empty, PADDING))
        .chain(&own);
    let palettes = [palette(0x20), palette(0x30)];
    let images = [
        (
            HERO_ART,
            "ShadeArt",
            image(
                4,
                &hero_sets([SHADE_STAND, SHADE_PORTRAIT, SHADE_PANEL, SHADE_ICON]),
            ),
        ),
        (
            ART_V2,
            "MistArt",
            image(2, &[(SetKind::Stand.id(), vec![vec![SHADE_MIST]])]),
        ),
        (
            ART_V3,
            "CardArt",
            image(3, &[(SetKind::Active.id(), vec![vec![SHADE_CARD]])]),
        ),
        (
            ART_V5,
            "GlowArt",
            image(
                5,
                &[
                    (
                        SetKind::Special.id(),
                        vec![vec![SHADE_GLOW, SHADE_STAND, SHADE_GLOW]],
                    ),
                    (
                        SetKind::Dead.id(),
                        vec![vec![SHADE_STAND], vec![SHADE_GLOW, SHADE_MIST]],
                    ),
                ],
            ),
        ),
    ];

    cam::write(&[
        Written {
            tag: FourCc::new(b"TILE"),
            flag: ChunkFlag::Set,
            records: positional(names.zip(payloads)),
        },
        // Unflagged: a tile's number finds the record of that id
        // (`Catalogue::positional`), not the one at that position.
        Written {
            tag: FourCc::new(b"SPLT"),
            flag: ChunkFlag::Clear,
            records: vec![
                (FourCc::from_word(1), "pal1", palettes[1].as_slice()),
                (FourCc::from_word(0), "pal0", palettes[0].as_slice()),
            ],
        },
        Written {
            tag: FourCc::new(b"IMAG"),
            flag: ChunkFlag::Clear,
            records: images
                .iter()
                .map(|(id, name, bytes)| (*id, *name, bytes.as_slice()))
                .collect(),
        },
    ])
}

/// A hero's sets: stand, then the three interface pictures, one frame each.
fn hero_sets([stand, portrait, panel, icon]: [u16; 4]) -> Vec<(u32, Vec<Vec<u16>>)> {
    vec![
        (SetKind::Stand.id(), vec![vec![stand]]),
        (u32::from(INTERFACE_PORTRAIT), vec![vec![portrait]]),
        (u32::from(INTERFACE_PANEL), vec![vec![panel]]),
        (u32::from(INTERFACE_ICON), vec![vec![icon]]),
    ]
}

/// A flagged chunk's records: the id of each is its position.
fn positional<'a, 'n: 'a>(
    records: impl Iterator<Item = (&'n str, &'a Vec<u8>)>,
) -> Vec<(FourCc, &'a str, &'a [u8])> {
    records
        .enumerate()
        .map(|(i, (name, bytes))| (FourCc::from_word(count(i)), name, bytes.as_slice()))
        .collect()
}

// --------------------------------------------------------------------------
// Payloads
// --------------------------------------------------------------------------

/// `TILE` header length (`data::tile`): where rows or pixels start.
const TILE_HEADER: usize = 26;

/// Flags bit 0, two bytes a pixel, and bit 2, the 565 packing.
const DIRECT_565: u16 = 1 | 4;

/// A version 3 run's last-in-row bit.
const LAST_RUN: u16 = 0x8000;

/// A version 2 tile's transparent index.
const KEY: u8 = 255;

/// A palette's header, as every shipped one holds it, then 256 `RGBx`
/// entries (`data::payload::Palette`).
const PALETTE_HEADER: [u8; 8] = [0, 0, 0, 1, 0, 0, 0, 0];
const PALETTE_ENTRIES: usize = 256;
const PALETTE_LEN: usize = PALETTE_HEADER.len() + 4 * PALETTE_ENTRIES;

/// A palette whose entry `i` is `(i, green, 255 - i)`.
fn palette(green: u8) -> Vec<u8> {
    let entries = (0..=u8::MAX).flat_map(|i| [i, green, u8::MAX - i, 0]);

    PALETTE_HEADER.into_iter().chain(entries).collect()
}

/// A tile filled with `fill`. Version 3 leaves column 0 a run gap;
/// version 2 keys it out with index 255; version 1 draws it.
fn tile(version: u16, (width, height): (u16, u16), colours: Colours, fill: u8) -> Vec<u8> {
    let direct = matches!(colours, Colours::Direct565);
    let bytes = if direct { 2 } else { 1 };
    let pixel = |x: u16| -> Vec<u8> {
        match (direct, version, x) {
            (true, ..) => u16::from(fill).to_le_bytes().to_vec(),
            (false, VERSION_KEYED, 0) => vec![KEY],
            _ => vec![fill],
        }
    };

    let body: Vec<u8> = if version == VERSION_RLE {
        let row: Vec<u8> = width
            .to_le_bytes()
            .into_iter()
            .chain(((width - 1) | LAST_RUN).to_le_bytes())
            .chain((1..width).flat_map(pixel))
            .collect();
        let row_len = row.len();
        let offsets = (0..usize::from(height))
            .flat_map(|r| count(4 * usize::from(height) + r * row_len).to_le_bytes());

        offsets
            .chain(std::iter::repeat_n(row, usize::from(height)).flatten())
            .collect()
    } else {
        (0..height)
            .flat_map(|_| (0..width).flat_map(pixel))
            .collect()
    };

    let stride = if version == VERSION_RLE {
        0
    } else {
        width * bytes
    };
    let (flags, embedded, palette_word) = match colours {
        Colours::Splt(index) => (SPLT_PALETTE, 0u16, index),
        Colours::Embedded => (0, 1, count(TILE_HEADER + body.len())),
        Colours::Direct565 => (DIRECT_565, 0, 0),
    };

    let mut out = Vec::new();

    for word in [version, height, width, stride, flags, width / 2, height, 0] {
        out.extend(word.to_le_bytes());
    }

    out.extend(0u32.to_le_bytes());
    out.extend(embedded.to_le_bytes());
    out.extend(palette_word.to_le_bytes());
    out.extend(body);

    if embedded != 0 {
        out.extend(palette(0x40));
    }

    out
}

/// Bytes before an `IMAG`'s set count (`data::imag`): version, zeros.
const IMAG_HEAD: usize = 0x14;

/// A set's control bytes, between its slot count and slot table.
const SET_CONTROL: usize = 0x3c;

/// Block mask: block 0, the footprint, only.
const BLOCK_0: u32 = 1;

/// Every slot's footprint: left, top, right, bottom.
const FOOTPRINT: [i16; 4] = [-4, -3, 4, 3];

/// An `IMAG` of layout `version`: each set an id and its slots, each slot
/// its frames' tile positions in one stream.
fn image(version: u32, sets: &[(u32, Vec<Vec<u16>>)]) -> Vec<u8> {
    let bodies: Vec<Vec<u8>> = sets.iter().map(|(_, slots)| set(version, slots)).collect();
    let mut at = IMAG_HEAD + 4 + 8 * sets.len();
    let mut out = version.to_le_bytes().to_vec();

    out.resize(IMAG_HEAD, 0);
    out.extend(count(sets.len()).to_le_bytes());

    for ((id, _), body) in sets.iter().zip(&bodies) {
        out.extend(id.to_le_bytes());
        out.extend(count(at).to_le_bytes());
        at += body.len();
    }

    out.extend(bodies.concat());
    out
}

/// A set: slot count, control, slot offsets from the set's start, slots.
fn set(version: u32, slots: &[Vec<u16>]) -> Vec<u8> {
    let bodies: Vec<Vec<u8>> = slots.iter().map(|f| slot(version, f)).collect();
    let mut at = 4 + SET_CONTROL + 4 * slots.len();
    let mut out = count(slots.len()).to_le_bytes().to_vec();

    out.extend([0; SET_CONTROL]);

    for body in &bodies {
        out.extend(count(at).to_le_bytes());
        at += body.len();
    }

    out.extend(bodies.concat());
    out
}

/// One slot in `version`'s layout: header, block 0 the footprint, frames.
///
/// ```text
/// 2   three blocks, offset +0x18, counts +0x1c, frames +0x20
/// 3   offset, counts, block mask, list mask; blocks; frames
/// 4   ... playback flag, frame list count; blocks; list word; frames
/// 5   ... effect mask; blocks; list word; frames
/// ```
fn slot(version: u32, frames: &[u16]) -> Vec<u8> {
    const ONE_STREAM: u32 = 1;
    const ONE_FRAME_LIST: u16 = 1;

    let counts = count(frames.len()) << 16 | ONE_STREAM;
    let block: Vec<u8> = FOOTPRINT.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut out = Vec::new();

    if version == 2 {
        out.extend(&block);
        out.extend([0; 16]);
        out.extend(0u32.to_le_bytes());
        out.extend(counts.to_le_bytes());
    } else {
        out.extend(0u32.to_le_bytes());
        out.extend(counts.to_le_bytes());
        out.extend(BLOCK_0.to_le_bytes());
        out.extend(0u32.to_le_bytes());

        if version >= 4 {
            out.extend(0u16.to_le_bytes());
            out.extend(ONE_FRAME_LIST.to_le_bytes());
        }

        if version == 5 {
            out.extend(0u32.to_le_bytes());
        }

        out.extend(&block);

        if version >= 4 {
            out.extend(0u32.to_le_bytes());
        }
    }

    for tile in frames {
        out.extend(0u32.to_le_bytes());
        out.extend(u32::from(*tile).to_le_bytes());
    }

    out
}

/// Sample rate and the samples of [`WAVE`]: 16-bit mono PCM, as every
/// shipped `WAVE` is (`data::payload::Pcm`).
pub const WAVE_RATE: u32 = 11_025;
pub const WAVE_SAMPLES: [i16; 6] = [0, 900, -900, 1800, -1800, 0];

/// [`WAVE_SAMPLES`] as a RIFF/WAVE file: `fmt ` then `data`.
fn wave() -> Vec<u8> {
    const PCM: u16 = 1;
    const MONO: u16 = 1;
    const BITS: u16 = 16;
    const FMT_LEN: u32 = 16;

    let data: Vec<u8> = WAVE_SAMPLES.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut fmt = Vec::new();

    fmt.extend(PCM.to_le_bytes());
    fmt.extend(MONO.to_le_bytes());
    fmt.extend(WAVE_RATE.to_le_bytes());
    fmt.extend((WAVE_RATE * 2).to_le_bytes());
    fmt.extend(2u16.to_le_bytes());
    fmt.extend(BITS.to_le_bytes());

    let mut body = b"WAVE".to_vec();

    body.extend(b"fmt ");
    body.extend(FMT_LEN.to_le_bytes());
    body.extend(fmt);
    body.extend(b"data");
    body.extend(count(data.len()).to_le_bytes());
    body.extend(data);

    let mut out = b"RIFF".to_vec();

    out.extend(count(body.len()).to_le_bytes());
    out.extend(body);
    out
}

/// A version 2 `STRT`: count, encoding 0, version 2, `u32` offsets, then
/// `[u32 key] cstr` records (`docs/formats/payloads.md`).
fn string_table(entries: &[(FourCc, &str)]) -> Vec<u8> {
    const CP1250: u8 = 0;
    const KEYED: u8 = 2;

    let records: Vec<Vec<u8>> = entries
        .iter()
        .map(|(key, text)| {
            let mut r = key.as_bytes().to_vec();

            r.extend(text.as_bytes());
            r.push(0);
            r
        })
        .collect();
    let n = u16::try_from(entries.len()).expect("a small table");
    let mut at = 4 + 4 * records.len();
    let mut out = n.to_le_bytes().to_vec();

    out.extend([CP1250, KEYED]);

    for r in &records {
        out.extend(count(at).to_le_bytes());
        at += r.len();
    }

    out.extend(records.concat());
    out
}

fn count(n: usize) -> u32 {
    u32::try_from(n).expect("a fixture under 4 GiB")
}
