//! Tile bytes for tests.

use super::header::{HEADER_LEN, VERSION_RLE};
use super::{Run, Tile};

/// A one-row version 3 tile from raw run bytes.
pub(crate) fn tile_bytes(width: u16, flags: u16, row: &[u8]) -> Vec<u8> {
    let mut out = header(VERSION_RLE, width, 1, 0, flags);
    // Offsets are relative to the table start, so the row is at 4.
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(row);
    out
}

/// The 26 bytes every version shares.
pub(crate) fn header(version: u16, width: u16, height: u16, stride: u16, flags: u16) -> Vec<u8> {
    let mut out = vec![0u8; HEADER_LEN];
    out[0..2].copy_from_slice(&version.to_le_bytes());
    out[2..4].copy_from_slice(&height.to_le_bytes());
    out[4..6].copy_from_slice(&width.to_le_bytes());
    out[6..8].copy_from_slice(&stride.to_le_bytes());
    out[8..10].copy_from_slice(&flags.to_le_bytes());
    out
}

/// An uncompressed tile: header, then pixels.
pub(crate) fn raw_bytes(
    version: u16,
    width: u16,
    height: u16,
    flags: u16,
    pixels: &[u8],
) -> Vec<u8> {
    let bpp = if flags & 1 == 0 { 1 } else { 2 };
    let mut out = header(version, width, height, width * bpp, flags);
    out.extend_from_slice(pixels);
    out
}

/// The runs of a version 3 tile.
pub(crate) fn rows<'a>(tile: &'a Tile<'a>) -> &'a [Vec<Run<'a>>] {
    tile.runs().expect("an RLE tile has runs")
}
