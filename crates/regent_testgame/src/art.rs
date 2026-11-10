//! The two record kinds the game's XML cannot make: `IMAG`, art with no
//! pictures, and `DMOV`, a movement class. Written in the archive's layout
//! ([`regent_core::data::imag`], [`regent_core::sim::movement::Class`]).

use regent_core::data::imag::SetKind;

/// `IMAG` layout version with the smallest slot that holds a rect.
const VERSION: u32 = 4;

/// Bytes before the set table: version, then zeros, then the set count.
const HEADER_ZEROS: usize = 0x10;

/// A set's table entry: id and offset.
const SET_ENTRY: usize = 8;

/// Bytes from a set's slot count to its slot table.
const SET_CONTROL: usize = 0x3c;

/// A slot's offset within its set: past the one-entry slot table.
const SLOT_AT: u32 = 0x44;

/// A frame not drawn, loaded or hit-tested ([`regent_core::data::imag::HIDDEN`]):
/// no `TILE` needed.
const NO_PICTURE: u32 = regent_core::data::imag::HIDDEN;

/// A pad, screen pixels about the anchor: left, top, right, bottom.
pub type Rect = [i16; 4];

/// Art of `sets`, each one slot with `frames` frames standing on `pad`.
pub fn image(sets: &[SetKind], pad: Rect, frames: u16) -> Vec<u8> {
    let set = set(pad, frames);
    let table_end = 4 + HEADER_ZEROS + 4 + SET_ENTRY * sets.len();
    let mut out = word(VERSION);

    out.extend([0; HEADER_ZEROS]);
    out.extend(count(sets.len()).to_le_bytes());

    for (i, kind) in sets.iter().enumerate() {
        out.extend(kind.id().to_le_bytes());
        out.extend(count(table_end + i * set.len()).to_le_bytes());
    }

    for _ in sets {
        out.extend(&set);
    }

    out
}

/// One slot: header, block 0 the pad, the frame list's word, the frames.
fn set(pad: Rect, frames: u16) -> Vec<u8> {
    const BLOCK_0: u32 = 1;
    const ONE_STREAM: u32 = 1;

    let mut out = word(1);

    out.extend([0; SET_CONTROL]);
    out.extend(SLOT_AT.to_le_bytes());

    // Offset, then frames and streams, the block mask, no lists, no playback.
    out.extend(0u32.to_le_bytes());
    out.extend((u32::from(frames) << 16 | ONE_STREAM).to_le_bytes());
    out.extend(BLOCK_0.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());

    for side in pad {
        out.extend(side.to_le_bytes());
    }

    out.extend(0u32.to_le_bytes());

    for _ in 0..frames {
        out.extend(0u32.to_le_bytes());
        out.extend(NO_PICTURE.to_le_bytes());
    }

    out
}

/// A movement class named `name`, id `id`: `PRIM` words `turn_rate`, turn
/// step, `move_rate`, `move_dist`, the rest 0.
pub fn movement(id: [u8; 4], name: &str, [turn, step, rate, dist]: [u32; 4]) -> Vec<u8> {
    const PRIM_VERSION: u32 = 2;
    const PRIM_WORDS: usize = regent_core::sim::movement::Class::LEN / 4;

    let mut head = id.to_vec();

    head.extend(0u32.to_le_bytes());
    head.extend(name.as_bytes());
    head.extend([0, 0]);

    let prim: Vec<u8> = [turn, step, rate, dist]
        .into_iter()
        .chain(std::iter::repeat(0))
        .take(PRIM_WORDS)
        .flat_map(u32::to_le_bytes)
        .collect();
    let inner = [
        part(*b"HEAD", 0, 0, &head),
        part(*b"PRIM", PRIM_VERSION, 0, &prim),
    ]
    .concat();

    part(*b"DMOV", 0, 1, &part(*b"DATA", 0, 2, &inner))
}

/// A descriptor part: tag, length, version, children, body.
fn part(tag: [u8; 4], version: u32, children: u32, body: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();

    out.extend(count(body.len()).to_le_bytes());
    out.extend(version.to_le_bytes());
    out.extend(children.to_le_bytes());
    out.extend(body);
    out
}

fn word(w: u32) -> Vec<u8> {
    w.to_le_bytes().to_vec()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).expect("a fixture under 4 GiB")
}
