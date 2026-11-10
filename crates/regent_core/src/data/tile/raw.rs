//! Versions 1 and 2: raw pixels, row-major.

use super::header::{HEADER_LEN, PixelFormat};
use super::{PaletteRef, Pixels};
use crate::data::error::{Error, Result, slice};
use crate::data::payload::PALETTE_LEN;

/// Decode a version 1 or 2 body: raw pixels, then maybe a palette.
///
/// Three checks make a wrong pixel format fail: stride against
/// `width × bytes-per-pixel`, payload length against `header + height × stride`
/// plus any palette, and an embedded palette's offset against the pixel end.
pub(super) fn parse_raw(
    payload: &[u8],
    width: u16,
    height: u16,
    stride: usize,
    format: PixelFormat,
    embedded: bool,
    palette_word: u32,
) -> Result<(PaletteRef, Pixels<'_>)> {
    let want = usize::from(width) * format.bytes();

    if stride != want {
        return Err(Error::BrokenInvariant {
            what: "tile stride against width",
            expected: want as u64,
            found: stride as u64,
        });
    }

    let len = usize::from(height) * stride;
    let bytes = slice(payload, "tile pixels", HEADER_LEN, len)?;
    let pixels_end = HEADER_LEN + len;

    let palette = if embedded {
        // Holds on every shipped tile; makes the length test below a
        // cross-check of two independent fields.
        if palette_word as usize != pixels_end {
            return Err(Error::BrokenInvariant {
                what: "embedded palette offset against pixel end",
                expected: pixels_end as u64,
                found: palette_word.into(),
            });
        }

        PaletteRef::Embedded(pixels_end)
    } else {
        PaletteRef::Splt(palette_word)
    };

    let expected = pixels_end + if embedded { PALETTE_LEN } else { 0 };

    if payload.len() != expected {
        return Err(Error::BrokenInvariant {
            what: "uncompressed tile length",
            expected: expected as u64,
            found: payload.len() as u64,
        });
    }

    Ok((palette, Pixels::Raw { bytes, stride }))
}

#[cfg(test)]
mod tests {
    use super::super::fixture::raw_bytes;
    use super::super::header::PALETTE_EMBEDDED;
    use super::super::{Packing, Tile};
    use super::*;

    #[test]
    fn an_uncompressed_tile_is_read_row_major_and_opaque() {
        // 3 wide, 2 high, 8-bit: six bytes, no runs, nothing transparent.
        let bytes = raw_bytes(1, 3, 2, 0, &[1, 2, 3, 4, 5, 6]);
        let t = Tile::parse(&bytes).unwrap();

        assert!(t.runs().is_none());
        assert_eq!(t.max_end(), 3);
        assert_eq!(t.indices().unwrap(), [1, 2, 3, 4, 5, 6].map(Some).to_vec());
    }

    #[test]
    fn version_2_reads_exactly_as_version_1() {
        // As all 60 shipped version 2 tiles do.
        let pixels = [1, 2, 3, 4, 5, 6];
        let (first, second) = (
            raw_bytes(1, 3, 2, 0, &pixels),
            raw_bytes(2, 3, 2, 0, &pixels),
        );
        let one = Tile::parse(&first).unwrap();
        let two = Tile::parse(&second).unwrap();

        assert_eq!(one.version, 1);
        assert_eq!(two.version, 2);
        assert_eq!(one.indices().unwrap(), two.indices().unwrap());
    }

    #[test]
    fn an_uncompressed_rgb565_tile_uses_a_doubled_stride() {
        let bytes = raw_bytes(1, 2, 1, 0x21, &[0x11, 0x22, 0x33, 0x44]);
        let t = Tile::parse(&bytes).unwrap();

        assert_eq!(t.format, PixelFormat::Direct(Packing::Rgb555));
        assert_eq!(t.words().unwrap(), [Some(0x2211), Some(0x4433)]);
    }

    #[test]
    fn an_uncompressed_stride_must_match_the_width() {
        // The stride check makes a wrong pixel format fail.
        let mut bytes = raw_bytes(1, 3, 2, 0, &[1, 2, 3, 4, 5, 6]);
        bytes[6..8].copy_from_slice(&4u16.to_le_bytes());

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
        assert!(Tile::parse_as(&bytes, PixelFormat::Direct(Packing::Rgb565)).is_err());
    }

    /// A tile no pixel wide has rows of no bytes, and no pixels.
    #[test]
    fn an_uncompressed_tile_of_width_0_has_no_pixels() {
        let bytes = raw_bytes(1, 0, 2, 0, &[]);

        assert_eq!(Tile::parse(&bytes).unwrap().indices(), Some(Vec::new()));
    }

    #[test]
    fn an_uncompressed_tile_must_account_for_every_byte() {
        let mut bytes = raw_bytes(1, 3, 2, 0, &[1, 2, 3, 4, 5, 6]);
        bytes.push(0);

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn an_embedded_palette_must_start_where_the_pixels_stop() {
        let mut bytes = raw_bytes(1, 3, 2, 0, &[1, 2, 3, 4, 5, 6]);
        let pixels_end = u32::try_from(HEADER_LEN + 6).unwrap();
        bytes[20..22].copy_from_slice(&PALETTE_EMBEDDED.to_le_bytes());
        bytes[22..26].copy_from_slice(&pixels_end.to_le_bytes());
        bytes.extend_from_slice(&vec![0u8; PALETTE_LEN]);

        let t = Tile::parse(&bytes).unwrap();
        assert_eq!(t.palette, PaletteRef::Embedded(pixels_end as usize));

        // Off by one byte.
        bytes[22..26].copy_from_slice(&(pixels_end + 1).to_le_bytes());
        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }
}
