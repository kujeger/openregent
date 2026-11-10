//! Reader for `TILE` payloads: sprite pixels, one animation frame each.
//!
//! `IMAG` groups tiles into animations; this decodes the pixels.
//!
//! Three versions ship, sharing one header:
//!
//! ```text
//! +0x00  u16       version: 3 is row-RLE, 1 and 2 are uncompressed
//! +0x02  u16       height, in rows
//! +0x04  u16       width, the canvas in columns
//! +0x06  u16       row stride in versions 1 and 2; unknown in version 3
//! +0x08  u16       flags, see below
//! +0x0a  i16 × 2   anchor x, y
//! +0x0e  u16       draw mode, see [`Mode`]
//! +0x10  u32       copied by the engine, never read
//! +0x14  u16       embedded palette count, 1032 bytes each
//! +0x16  u32       palette: an SPLT record index, or an offset when embedded
//! +0x1a            version 3: u32×h row offsets, relative to here, then rows
//!                  versions 1 and 2: height × stride bytes of raw pixels
//!        then the embedded palette, if there is one
//! ```
//!
//! **Binary**, `MajestyHD.exe`. The loader reads the 26-byte
//! header into a `CYPixSprite`, reads the pixels
//! whole, then either the embedded palettes or, with none and flags bit 5, the
//! `SPLT` record. It decodes nothing; the version is read at draw time.
//!
//! Flags word at `+0x08`:
//!
//! ```text
//! bits 0-1   pixel format: zero is 8-bit indices (sprite format)
//! bits 2-3   16-bit packing: 0 555, 4 565, 8 4444, 12 a table (sprite convert)
//! bit  4     pixels are stored mirrored ([`MIRRORED`])
//! bit  5     palette is an SPLT record ([`SPLT_PALETTE`])
//! bits 8-12  the draw mode's level ([`Tile::level`])
//! bit  13    anchor at the canvas centre (CYOffport); set on no tile
//! ```
//!
//! `+0x10` is stored on the sprite and only copied: by
//! its clone and its writer. No blit, converter or draw path
//! reads it.
//!
//! Versions 1 and 2 share a layout but not a blit: version 1 is copied opaque,
//! version 2 skips index 255 ([`VERSION_KEYED`]).
//!
//! In version 3 each row is a list of segments, the last flagged. Gaps between
//! segments are the only transparency:
//!
//! ```text
//! [u16 x_end] [u16 count_flags] [count × pixel]
//!  ^ exclusive end column         count = count_flags & 0x07ff
//!    covering [x_end - count, x_end)
//! ```
//!
//! Measured facts. `tests/game_files.rs` checks them on HD; the
//! wider-corpus counts are from `docs/formats/art.md`, not held by a test:
//!
//! - Flags bit 0: set for 16-bit colour, clear for 8-bit indices. Published
//!   tooling tries both widths instead; the flag is right on all 61,102 tiles
//!   and settles the 39 that parse either way. Bit 2 is the 16-bit
//!   [`Packing`].
//! - An empty run's `x_end` is meaningless: 304 tiles end a row with
//!   `[0xcdcd] [0x8000]`, MSVC uninitialised-heap fill. It is not validated.
//!
//! Every version 3 tile in HD and the Workshop mods parses. The flag also holds
//! on every uncompressed tile checked as `stride == width × bytes-per-pixel`.
//!
//! Index 0 is an ordinary colour. See `docs/formats/art.md`.

#[cfg(test)]
pub(crate) mod fixture;
mod header;
mod raw;
mod rle;

pub use header::{
    MIRRORED, Mode, Packing, PaletteRef, PixelFormat, SPLT_PALETTE, VERSION_KEYED, VERSION_OPAQUE,
    VERSION_RLE, VERSIONS_RAW,
};
pub use rle::Run;

use super::error::{Error, Result, u16_at, u32_at};
use header::{LEVEL_MASK, LEVEL_SHIFT, PALETTE_EMBEDDED, PALETTE_SPLT};
use raw::parse_raw;
use rle::parse_runs;

/// A tile's pixels, in either stored shape.
#[derive(Debug, Clone)]
pub enum Pixels<'a> {
    /// Version 3: one entry per row, top to bottom, gaps transparent.
    Runs(Vec<Vec<Run<'a>>>),
    /// Versions 1 and 2: `height × stride` bytes, row-major, opaque. `stride`
    /// is the header's.
    Raw { bytes: &'a [u8], stride: usize },
}

/// A decoded tile of any version.
#[derive(Debug, Clone)]
pub struct Tile<'a> {
    width: u16,
    height: u16,
    format: PixelFormat,
    palette: PaletteRef,
    flags: u16,
    mode: Mode,
    anchor: (i16, i16),
    version: u16,
    pixels: Pixels<'a>,
}

impl<'a> Tile<'a> {
    /// Canvas columns.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Canvas rows.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// How the pixels are stored.
    #[must_use]
    pub const fn format(&self) -> PixelFormat {
        self.format
    }

    /// Where an indexed tile's colours come from.
    #[must_use]
    pub const fn palette(&self) -> PaletteRef {
        self.palette
    }

    /// The flags word at `+0x08`; see the module docs.
    #[must_use]
    pub const fn flags(&self) -> u16 {
        self.flags
    }

    /// The word at `+0x0e`.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// `+0x0a`, `+0x0c`: where the unit stands in the picture.
    ///
    /// **Viewer**: measured from it, firing points land on the building's
    /// towers (Guardhouse six faces, Dwarf Settlement four corners) and the
    /// Ranger's on his bow. See `docs/formats/art.md`.
    ///
    /// **Corpus**: inside the canvas on every version 1 tile and 27,919 of
    /// 29,289 version 3 ones. On 19,084 indexed version 3 tiles `x` is the
    /// art's horizontal centre on median, but within two pixels per tile only
    /// 36% of the time: an origin the figure moves around, not the art centre.
    #[must_use]
    pub const fn anchor(&self) -> (i16, i16) {
        self.anchor
    }

    /// The version word at `+0x00`: 1 and 2 decode alike but blit apart.
    #[must_use]
    pub const fn version(&self) -> u16 {
        self.version
    }

    /// The pixels, in their stored shape.
    #[must_use]
    pub const fn pixels(&self) -> &Pixels<'a> {
        &self.pixels
    }

    /// Read only the version word.
    ///
    /// # Errors
    ///
    /// If the payload is under two bytes.
    pub fn version_word(payload: &[u8]) -> Result<u16> {
        u16_at(payload, "tile version", 0)
    }

    /// Decode a tile of any shipped version.
    ///
    /// # Errors
    ///
    /// If the version, draw mode or palette mode is unknown, the header is
    /// truncated, or the bytes do not add up: a version 3 row not ending on
    /// its terminator, an embedded palette past the payload, or a version
    /// 1/2 length not matching `height × stride`. So a wrong pixel format
    /// fails.
    pub fn parse(payload: &'a [u8]) -> Result<Self> {
        let flags = u16_at(payload, "tile flags", 8)?;

        Self::parse_as(payload, PixelFormat::from_flags(flags))
    }

    /// Decode with the pixel format forced, ignoring the flags word.
    ///
    /// Tests the flag against try-both-widths: in HD's `Data` and `DataMX` the
    /// wrong width fails on all but 10 of 29,304 version 3 tiles, and the flag always
    /// picks the working one (`the_pixel_format_flag_is_never_wrong`).
    /// `docs/formats/art.md`: 39 of 61,102 over every corpus.
    ///
    /// # Errors
    ///
    /// As [`Tile::parse`]; a wrong format usually fails.
    pub fn parse_as(payload: &'a [u8], format: PixelFormat) -> Result<Self> {
        let version = Self::version_word(payload)?;

        let height = u16_at(payload, "tile height", 2)?;
        let width = u16_at(payload, "tile width", 4)?;
        let flags = u16_at(payload, "tile flags", 8)?;
        let palette_mode = u16_at(payload, "tile palette mode", 20)?;
        let palette_word = u32_at(payload, "tile palette", 22)?;
        let embedded = match palette_mode {
            PALETTE_SPLT => false,
            PALETTE_EMBEDDED => true,
            other => {
                return Err(Error::BadValue {
                    what: "tile palette mode",
                    value: other.into(),
                });
            }
        };
        let mode = u16_at(payload, "tile mode", 0x0e)?;
        let mode = Mode::from_word(mode).ok_or(Error::BadValue {
            what: "tile draw mode",
            value: mode.into(),
        })?;

        let (palette, pixels) = if version == VERSION_RLE {
            parse_runs(payload, width, height, format, embedded, palette_word)?
        } else if VERSIONS_RAW.contains(&version) {
            let stride = usize::from(u16_at(payload, "tile stride", 6)?);

            parse_raw(
                payload,
                width,
                height,
                stride,
                format,
                embedded,
                palette_word,
            )?
        } else {
            return Err(Error::BadValue {
                what: "tile version",
                value: version.into(),
            });
        };

        Ok(Tile {
            width,
            height,
            format,
            palette,
            flags,
            mode,
            anchor: (
                u16_at(payload, "tile anchor x", 0x0a)?.cast_signed(),
                u16_at(payload, "tile anchor y", 0x0c)?.cast_signed(),
            ),
            version,
            pixels,
        })
    }

    /// Flags bits 8 to 12: the level [`Mode::Translucent`] and mode 8 read.
    ///
    /// **Binary**: draw params subtracts 1 for mode 5, clamping to 0
    /// to 8, and 10 for mode 8. Returned raw.
    pub const fn level(&self) -> u16 {
        (self.flags >> LEVEL_SHIFT) & LEVEL_MASK
    }

    /// Whether the pixels are stored mirrored; see [`MIRRORED`].
    pub const fn mirrored(&self) -> bool {
        self.flags & MIRRORED != 0
    }

    /// Whether an index of 255 is a hole: version 2 only.
    ///
    /// Version 1 copies it as a colour ([`VERSION_OPAQUE`]); version 3's holes
    /// are its run gaps, and its mode 0 blit draws 255 from the palette.
    pub const fn keyed(&self) -> bool {
        self.version == VERSION_KEYED
    }

    /// The per-row runs, or `None` for an uncompressed tile.
    pub fn runs(&self) -> Option<&[Vec<Run<'a>>]> {
        match &self.pixels {
            Pixels::Runs(rows) => Some(rows),
            Pixels::Raw { .. } => None,
        }
    }

    /// The largest exclusive end column over every non-empty run.
    ///
    /// Usually [`Tile::width`]; less when the rightmost columns are empty. An
    /// uncompressed tile reports its width.
    pub fn max_end(&self) -> u16 {
        match &self.pixels {
            Pixels::Runs(rows) => rows
                .iter()
                .flatten()
                .filter(|run| !run.is_empty())
                .map(|run| run.x_end)
                .max()
                .unwrap_or(0),
            Pixels::Raw { .. } => self.width,
        }
    }

    /// Rasterise to one palette index per pixel, `None` where transparent.
    ///
    /// Row-major, `width * height` long. `None` for a direct-colour tile; see
    /// [`Tile::words`].
    pub fn indices(&self) -> Option<Vec<Option<u8>>> {
        (self.format == PixelFormat::Indexed).then(|| self.rasterise(1, |bytes| bytes[0]))
    }

    /// Rasterise to one raw word per pixel, `None` where transparent.
    ///
    /// Words as stored; [`Tile::format`] gives the [`Packing`]. `None` for an
    /// indexed tile.
    pub fn words(&self) -> Option<Vec<Option<u16>>> {
        matches!(self.format, PixelFormat::Direct(_))
            .then(|| self.rasterise(2, |bytes| u16::from_le_bytes([bytes[0], bytes[1]])))
    }

    /// Lay the stored pixels out row-major. `read` decodes one pixel of `size`
    /// bytes.
    fn rasterise<T: Copy>(&self, size: usize, read: impl Fn(&[u8]) -> T) -> Vec<Option<T>> {
        let width = usize::from(self.width);
        let mut out = vec![None; width * usize::from(self.height)];

        match &self.pixels {
            Pixels::Runs(rows) => {
                for (y, row) in rows.iter().enumerate() {
                    for run in row {
                        let Some(start) = run.start() else { continue };

                        for (i, pixel) in run.pixels.chunks_exact(size).enumerate() {
                            // Only a zero-width tile's run ends past the
                            // canvas: `parse_row` checks no other.
                            if let Some(cell) = out.get_mut(y * width + usize::from(start) + i) {
                                *cell = Some(read(pixel));
                            }
                        }
                    }
                }
            }

            // Every cell is written. Any stride padding past width * size is
            // skipped. A tile 0 wide has no cells.
            Pixels::Raw { bytes, stride } if *stride > 0 => {
                for (y, row) in bytes.chunks_exact(*stride).enumerate() {
                    for (x, pixel) in row.chunks_exact(size).take(width).enumerate() {
                        out[y * width + x] = Some(read(pixel));
                    }
                }
            }
            Pixels::Raw { .. } => {}
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{raw_bytes, tile_bytes};
    use super::*;

    #[test]
    fn an_unshipped_draw_or_palette_mode_is_refused() {
        let good = tile_bytes(1, 0, &[1, 0, 0x01, 0x80, 0xaa]);
        let mut mode = good.clone();
        let mut palette = good.clone();

        mode[0x0e] = 3;
        palette[20] = 2;

        assert!(Tile::parse(&good).is_ok());
        assert!(matches!(Tile::parse(&mode), Err(Error::BadValue { .. })));
        assert!(matches!(Tile::parse(&palette), Err(Error::BadValue { .. })));
    }

    #[test]
    fn an_embedded_palette_past_the_payload_is_refused() {
        // The palette word points at the row's end, with no palette after.
        let mut bytes = tile_bytes(1, 0, &[1, 0, 0x01, 0x80, 0xaa]);
        let end = u32::try_from(bytes.len()).unwrap();

        bytes[20] = 1;
        bytes[22..26].copy_from_slice(&end.to_le_bytes());

        assert!(matches!(
            Tile::parse(&bytes),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn the_flags_word_selects_the_pixel_width() {
        let indexed = tile_bytes(2, 0x20, &[2, 0, 0x02, 0x80, 0x11, 0x22]);
        let wide = tile_bytes(1, 0x21, &[1, 0, 0x01, 0x80, 0x11, 0x22]);

        assert_eq!(Tile::parse(&indexed).unwrap().format, PixelFormat::Indexed);
        assert!(matches!(
            Tile::parse(&wide).unwrap().format,
            PixelFormat::Direct(_)
        ));
        assert_eq!(Tile::parse(&wide).unwrap().words().unwrap(), [Some(0x2211)]);
        assert!(Tile::parse(&wide).unwrap().indices().is_none());
    }

    /// Bit 2 picks the packing, and the packing moves green by one bit.
    ///
    /// `0x4210` is `10000 10000 10000` in 555, `01000 010000 10000` in 565.
    #[test]
    fn bit_two_of_the_flags_is_the_sixth_bit_of_green() {
        let narrow = raw_bytes(1, 2, 1, 0x21, &[0xff, 0x7f, 0x10, 0x42]);
        let wide = raw_bytes(1, 2, 1, 0x25, &[0xff, 0x7f, 0x10, 0x42]);

        assert_eq!(
            Tile::parse(&narrow).unwrap().format,
            PixelFormat::Direct(Packing::Rgb555)
        );
        assert_eq!(
            Tile::parse(&wide).unwrap().format,
            PixelFormat::Direct(Packing::Rgb565)
        );

        // Same bytes; only the meaning differs.
        assert_eq!(
            Tile::parse(&narrow).unwrap().words().unwrap(),
            Tile::parse(&wide).unwrap().words().unwrap()
        );
    }

    #[test]
    fn forcing_the_wrong_pixel_width_fails() {
        // The row no longer adds up.
        let bytes = tile_bytes(2, 0x20, &[2, 0, 0x02, 0x80, 0x11, 0x22]);

        assert!(Tile::parse_as(&bytes, PixelFormat::Indexed).is_ok());
        assert!(Tile::parse_as(&bytes, PixelFormat::Direct(Packing::Rgb565)).is_err());
    }

    #[test]
    fn an_unknown_version_is_refused() {
        // Only 1, 2 and 3 ship.
        let mut bytes = tile_bytes(2, 0, &[2, 0, 0x02, 0x80, 1, 2]);
        bytes[0] = 4;

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BadValue { .. }
        ));
        assert_eq!(Tile::version_word(&bytes).unwrap(), 4);
    }
}
