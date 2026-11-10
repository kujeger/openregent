//! A tile resolved against its palette.
//!
//! An 8-bit tile names an `SPLT` by position within its own archive, so
//! resolving needs the [`Catalogue`].
//!
//! # Why this stays indexed
//!
//! Player colour overwrites a palette index range (`docs/formats/payloads.md`), so
//! [`Sprite`] keeps indices and palette apart. [`Sprite::rgba`] flattens.
//!
//! # Coverage is a separate plane
//!
//! Index 0 is an ordinary colour and version 3's transparency is the gaps
//! between runs, so no index can key it. Coverage gets its own mask.
//!
//! Uncompressed tiles are gapless, so their mask is all true, although the
//! world blitter also skips index 255. See [`super::raster::shade::TRANSPARENT`].

use crate::data::cam;
use crate::data::error::{Error, Result};
use crate::data::payload::{PALETTE_LEN, Palette, Rgb};
use crate::data::tile::{Mode, Packing, PaletteRef, PixelFormat, Tile};

use super::raster::colour;
use super::raster::shade::{self, Shade};
use crate::content::imagery;
use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;
use crate::data::imag::Blit;
use crate::sim::footprint::ArtRect;

/// Where a tile's colours came from, for grouping sprites by palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteSource {
    /// The `index`th `SPLT` of the archive the tile itself came from.
    Splt { archive: usize, index: usize },
    /// Inside the tile's own payload.
    Embedded,
    /// Direct colour; no palette.
    None,
}

/// A tile's pixel values.
#[derive(Debug, Clone)]
pub enum Colour {
    /// One palette index per pixel, row-major, and the palette.
    ///
    /// **Corpus**: all 91,730 indexed tiles (three editions and mods) name an
    /// `SPLT` their archive holds, so [`Sprite::resolve`] errors on one that
    /// does not. See `docs/openregent/viewer.md`.
    ///
    /// Uncovered pixels hold 0, a real colour; check [`Sprite::covered`].
    Indexed {
        indices: Vec<u8>,
        palette: Box<Palette>,
    },
    /// One 16-bit word per pixel, row-major, split per [`Packing`].
    Direct { words: Vec<u16>, packing: Packing },
}

/// What one pixel of a sprite does when blitted.
///
/// The blitter's branch, in test order. See [`super::raster::shade`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// Nothing written: a run gap, or
    /// [`TRANSPARENT`](super::raster::shade::TRANSPARENT).
    Hole,
    /// From the palette or the tile's own word.
    Colour(Rgb),
    /// Changes the destination's lightness.
    Shade(Shade),
}

/// One tile, resolved against its palette.
///
/// `covered` and [`Sprite::colour`] are both `width * height`, row-major from
/// top left.
#[derive(Debug, Clone)]
pub struct Sprite {
    pub width: u16,
    pub height: u16,
    /// Tile version word: RLE or uncompressed.
    pub version: u16,
    /// See [`crate::data::tile::Tile::anchor`].
    pub anchor: (i16, i16),
    /// False where the tile draws nothing. All true when uncompressed.
    pub covered: Vec<bool>,
    pub colour: Colour,
    pub source: PaletteSource,
    /// The tile's draw mode and level: [`Tile::mode`], [`Tile::level`].
    pub mode: Mode,
    pub level: u16,
}

impl Sprite {
    /// Resolve a `TILE` payload that came from `archive` of `catalogue`.
    ///
    /// `archive` is needed: an 8-bit tile names its palette by position in it.
    ///
    /// # Errors
    ///
    /// If the tile will not decode, an embedded palette is truncated or the
    /// wrong length, or a referenced `SPLT` is past the archive's chunk (no
    /// shipped data; see [`Colour::Indexed`]).
    ///
    /// # Panics
    ///
    /// Never in practice: [`crate::data::tile`]'s `indices` and `words` return
    /// `Some` exactly for the matching format.
    pub fn resolve(catalogue: &Catalogue<'_>, archive: usize, payload: &[u8]) -> Result<Self> {
        let tile = Tile::parse(payload)?;

        // Split the Option into two planes.
        let (colour, covered, source) = match tile.format() {
            PixelFormat::Indexed => {
                let drawn = tile.indices().expect("an indexed tile has indices");
                let covered = drawn.iter().map(Option::is_some).collect();
                let indices = drawn.into_iter().map(Option::unwrap_or_default).collect();
                let (palette, source) =
                    resolve_palette(catalogue, archive, payload, tile.palette())?;

                (Colour::Indexed { indices, palette }, covered, source)
            }

            PixelFormat::Direct(packing) => {
                let drawn = tile.words().expect("a direct tile has words");
                let covered = drawn.iter().map(Option::is_some).collect();
                let words = drawn.into_iter().map(Option::unwrap_or_default).collect();

                (
                    Colour::Direct { words, packing },
                    covered,
                    PaletteSource::None,
                )
            }
        };

        Ok(Sprite {
            width: tile.width(),
            height: tile.height(),
            version: tile.version(),
            anchor: tile.anchor(),
            covered,
            colour,
            source,
            mode: tile.mode(),
            level: tile.level(),
        })
    }

    /// Resolve the `index`th tile of `archive`.
    ///
    /// `None` when there is no such record, or the slot is an empty pad (an
    /// expansion or mod archive pads to keep later positions). A pad is a
    /// record with no bytes, which would otherwise fail to decode.
    ///
    /// # Errors
    ///
    /// As [`Sprite::resolve`].
    pub fn at(catalogue: &Catalogue<'_>, archive: usize, index: usize) -> Option<Result<Self>> {
        let record = catalogue.positional(archive, b"TILE", index)?;

        (!record.data.is_empty()).then(|| Self::resolve(catalogue, archive, record.data))
    }

    /// Frame `frame` of slot `slot` of set `set` in `IMAG` `record`: an
    /// interface picture. `None` where the record, set, slot or frame is
    /// missing, or the frame is no tile.
    ///
    /// # Errors
    ///
    /// If the image or the tile will not parse.
    pub fn in_set(
        catalogue: &Catalogue<'_>,
        record: FourCc,
        set: u32,
        (slot, frame): (usize, usize),
    ) -> Option<Result<Self>> {
        let sets = match imagery::sets(catalogue, catalogue.find(b"IMAG", record)?) {
            Ok(sets) => sets,
            Err(e) => return Some(Err(e)),
        };
        let found = sets.iter().find(|a| a.id == set)?;
        let (archive, index) = found.poses.get(slot)?.frames.get(frame)?.tile()?;

        Self::at(catalogue, archive, index)
    }

    /// Drawn pixel count.
    pub fn drawn(&self) -> usize {
        self.covered.iter().filter(|c| **c).count()
    }

    /// What pixel `i` (row-major) does under `blit`.
    ///
    /// World: [`TRANSPARENT`](super::raster::shade::TRANSPARENT) is a hole, 247–254 an
    /// operation, the rest a colour. Interface: the palette decides for high
    /// indices. Direct-colour tiles have no index to branch on.
    ///
    /// # Panics
    ///
    /// If `i` is past the end of the sprite.
    #[must_use]
    pub fn ink(&self, i: usize, blit: Blit) -> Ink {
        match &self.colour {
            _ if !self.covered[i] => Ink::Hole,

            Colour::Direct { words, packing } => Ink::Colour(colour::split(words[i], *packing)),

            Colour::Indexed { indices, palette } => {
                let index = indices[i];
                let c = palette.colours[usize::from(index)];

                if shade::controls(index, c, blit) {
                    // TRANSPARENT is the controlling index with no ramp.
                    shade::shade(index).map_or(Ink::Hole, Ink::Shade)
                } else {
                    Ink::Colour(c)
                }
            }
        }
    }

    /// What pixel `i` does as the tile's own draw mode says: 247–254 an
    /// operation in modes 1 and 7, colours in mode 0
    /// ([`shade::by_mode`]). Other indices and modes as
    /// [`Blit::Interface`] reads them.
    ///
    /// # Panics
    ///
    /// If `i` is past the end of the sprite.
    #[must_use]
    pub fn as_drawn(&self, i: usize) -> Ink {
        match (&self.colour, self.mode) {
            (Colour::Indexed { indices, palette }, Mode::Plain | Mode::Shadow | Mode::Band)
                if self.covered[i] && shade::shade(indices[i]).is_some() =>
            {
                let index = indices[i];
                let c = palette.colours[usize::from(index)];

                shade::by_mode(self.mode, index).map_or(Ink::Colour(c), Ink::Shade)
            }
            _ => self.ink(i, Blit::Interface),
        }
    }

    /// Whether darkening pixel `i` is a part's (a flag), not a ground
    /// shadow, in a building standing on `stands`, the tile moved `shift`
    /// from the anchor (y down). A tile holds both in one draw mode; where
    /// it lies tells them apart.
    ///
    /// A shadow on the ground lies left of what casts it and climbs the
    /// screen slowly: it can rise above the footprint's top by no more
    /// than it lies left of the footprint's right edge. **Inferred**, from
    /// the suns the art fits (`docs/original/art-camera.md`) and held by
    /// `a_guardhouses_flag_is_aloft_and_its_shadow_not`.
    ///
    /// ```text
    ///           flag ~~    rise > run: aloft
    ///                |
    ///  ~~~~~~~~ [ body ]
    ///    ~~~~~~ [      ]   rise <= run: on the ground
    /// ```
    ///
    /// # Panics
    ///
    /// Never in practice: a tile is at most `u16::MAX` squared pixels.
    #[must_use]
    pub fn aloft(&self, i: usize, stands: ArtRect, (dx, dy): (i32, i32)) -> bool {
        let at = i32::try_from(i).expect("a tile's pixels");
        let width = i32::from(self.width);
        let x = at % width - i32::from(self.anchor.0) + dx;
        let y = at / width - i32::from(self.anchor.1) + dy;
        let [_, top, right, _] = stands.map(i32::from);

        top - y > right - x
    }

    /// Straight RGBA8, transparent where the tile draws nothing.
    ///
    /// Direct-colour channels widen by [`colour::widen_replicate`].
    ///
    /// Indices 247–254 come out as the palette's magenta; they are an operation
    /// on what is beneath. See [`crate::view::raster::shade`].
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.covered.len() * 4];

        for (i, pixel) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if !self.covered[i] {
                continue;
            }

            let Rgb { r, g, b } = match &self.colour {
                Colour::Indexed { indices, palette } => palette.colours[usize::from(indices[i])],
                Colour::Direct { words, packing } => colour::split(words[i], *packing),
            };

            pixel.copy_from_slice(&[r, g, b, u8::MAX]);
        }

        out
    }
}

/// The palette an indexed tile points at.
fn resolve_palette(
    catalogue: &Catalogue<'_>,
    archive: usize,
    payload: &[u8],
    reference: PaletteRef,
) -> Result<(Box<Palette>, PaletteSource)> {
    match reference {
        PaletteRef::Splt(index) => {
            let index = index as usize;

            // Every corpus tile resolves; failure means a misread reference.
            let record: &cam::Record<'_> =
                catalogue
                    .positional(archive, b"SPLT", index)
                    .ok_or(Error::BrokenInvariant {
                        what: "tile palette reference past the archive's SPLT chunk",
                        expected: catalogue.chunk_len(archive, b"SPLT") as u64,
                        found: index as u64,
                    })?;

            Ok((
                Box::new(Palette::parse(record.data)?),
                PaletteSource::Splt { archive, index },
            ))
        }

        // `Tile::parse` checked the palette lies inside the payload.
        PaletteRef::Embedded(at) => Ok((
            Box::new(Palette::parse(&payload[at..at + PALETTE_LEN])?),
            PaletteSource::Embedded,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Palette with entry 0 `first` and entry 1 grey.
    fn palette_bytes(first: (u8, u8, u8)) -> Vec<u8> {
        let mut out = vec![0u8; PALETTE_LEN];
        out[3] = 1; // the header every shipped palette carries
        out[8..11].copy_from_slice(&[first.0, first.1, first.2]);
        out[12..15].copy_from_slice(&[9, 9, 9]);
        out
    }

    /// Uncompressed one-row tile: header, pixels, embedded palette.
    fn embedded_tile(pixels: &[u8]) -> Vec<u8> {
        let width = u16::try_from(pixels.len()).unwrap();
        let palette_at = u32::try_from(26 + pixels.len()).unwrap();

        let mut out = vec![0u8; 26];
        out[0..2].copy_from_slice(&1u16.to_le_bytes());
        out[2..4].copy_from_slice(&1u16.to_le_bytes());
        out[4..6].copy_from_slice(&width.to_le_bytes());
        out[6..8].copy_from_slice(&width.to_le_bytes());
        out[20..22].copy_from_slice(&1u16.to_le_bytes());
        out[22..26].copy_from_slice(&palette_at.to_le_bytes());
        out.extend_from_slice(pixels);
        out.extend_from_slice(&palette_bytes((10, 20, 30)));
        out
    }

    fn empty_catalogue() -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }

    #[test]
    fn an_embedded_palette_colours_its_own_tile() {
        let archives = empty_catalogue();
        let catalogue = Catalogue::build(&archives).unwrap();
        let bytes = embedded_tile(&[0, 1]);

        let s = Sprite::resolve(&catalogue, 0, &bytes).unwrap();

        assert_eq!(s.source, PaletteSource::Embedded);
        assert_eq!(s.covered, [true, true]);
        assert_eq!(s.drawn(), 2);
        // Index 0 is a colour, not a hole.
        assert_eq!(s.rgba(), [10, 20, 30, 255, 9, 9, 9, 255]);
    }

    #[test]
    fn a_dangling_splt_reference_is_an_error() {
        // No SPLT chunk, so the reference cannot land.
        let archives = empty_catalogue();
        let catalogue = Catalogue::build(&archives).unwrap();

        let mut bytes = vec![0u8; 26];
        bytes[0..2].copy_from_slice(&1u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&1u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&2u16.to_le_bytes());
        bytes[22..26].copy_from_slice(&7u32.to_le_bytes());
        bytes.extend_from_slice(&[3, 4]);

        assert!(matches!(
            Sprite::resolve(&catalogue, 0, &bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn a_direct_colour_tile_needs_no_palette() {
        let archives = empty_catalogue();
        let catalogue = Catalogue::build(&archives).unwrap();

        let mut bytes = vec![0u8; 26];
        bytes[0..2].copy_from_slice(&1u16.to_le_bytes());
        bytes[2..4].copy_from_slice(&1u16.to_le_bytes());
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&2u16.to_le_bytes());
        // Bits 0 and 2: two bytes a pixel, six of them green's.
        bytes[8..10].copy_from_slice(&5u16.to_le_bytes());
        // Pure white: all five, six and five bits set.
        bytes.extend_from_slice(&u16::MAX.to_le_bytes());

        let s = Sprite::resolve(&catalogue, 0, &bytes).unwrap();

        assert_eq!(s.source, PaletteSource::None);
        assert!(matches!(s.colour, Colour::Direct { .. }));
        // Replication must reach full scale.
        assert_eq!(s.rgba(), [255, 255, 255, 255]);
    }

    #[test]
    fn full_scale_and_zero_survive_the_565_widening() {
        let split = |word| colour::split(word, Packing::Rgb565).channels();

        assert_eq!(split(0xffff), [255, 255, 255]);
        assert_eq!(split(0), [0, 0, 0]);
        // Each channel alone.
        assert_eq!(split(0xf800), [255, 0, 0]);
        assert_eq!(split(0x07e0), [0, 255, 0]);
        assert_eq!(split(0x001f), [0, 0, 255]);
    }

    /// 555 masks. Bit 15 is unused.
    #[test]
    fn the_555_packing_gives_green_five_bits_like_the_others() {
        let split = |word| colour::split(word, Packing::Rgb555).channels();

        assert_eq!(split(0x7fff), [255, 255, 255]);
        assert_eq!(split(0xffff), [255, 255, 255], "bit 15 is not a channel");
        assert_eq!(split(0x7c00), [255, 0, 0]);
        assert_eq!(split(0x03e0), [0, 255, 0]);
        assert_eq!(split(0x001f), [0, 0, 255]);
    }

    /// One-row version 3 tile with embedded palette.
    fn rle_tile(width: u16, row: &[u8]) -> Vec<u8> {
        // Row table: one entry at 26, offsets relative to it. Palette follows.
        let palette_at = u32::try_from(26 + 4 + row.len()).unwrap();

        let mut out = vec![0u8; 26];
        out[0..2].copy_from_slice(&3u16.to_le_bytes());
        out[2..4].copy_from_slice(&1u16.to_le_bytes());
        out[4..6].copy_from_slice(&width.to_le_bytes());
        out[20..22].copy_from_slice(&1u16.to_le_bytes());
        out[22..26].copy_from_slice(&palette_at.to_le_bytes());
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(row);
        out.extend_from_slice(&palette_bytes((10, 20, 30)));
        out
    }

    #[test]
    fn an_rle_gap_is_uncovered_while_index_zero_is_not() {
        let archives = empty_catalogue();
        let catalogue = Catalogue::build(&archives).unwrap();

        // Three wide, one run of two index-0 pixels; column 2 is never written.
        let bytes = rle_tile(3, &[2, 0, 0x02, 0x80, 0x00, 0x00]);

        let s = Sprite::resolve(&catalogue, 0, &bytes).unwrap();

        assert_eq!(s.covered, [true, true, false]);
        assert_eq!(s.drawn(), 2);
        assert_eq!(
            s.rgba(),
            [10, 20, 30, 255, 10, 20, 30, 255, 0, 0, 0, 0],
            "a gap is transparent and index 0 is not"
        );
    }

    /// One pixel of `index`, palette entry `entry`.
    fn one_pixel(index: u8, entry: (u8, u8, u8)) -> Sprite {
        use crate::data::payload::{PALETTE_COLOURS, Rgb};

        let mut colours = [Rgb { r: 0, g: 0, b: 0 }; PALETTE_COLOURS];

        colours[usize::from(index)] = Rgb {
            r: entry.0,
            g: entry.1,
            b: entry.2,
        };

        Sprite {
            width: 1,
            height: 1,
            version: 1,
            anchor: (0, 0),
            covered: vec![true],
            colour: Colour::Indexed {
                indices: vec![index],
                palette: Box::new(Palette { colours }),
            },
            source: PaletteSource::Embedded,
            mode: Mode::Band,
            level: 0,
        }
    }

    /// Interface reads an unmarked high index as its colour (the Warrior's
    /// panel wall); world shades it regardless.
    #[test]
    fn the_interface_blit_takes_a_high_index_the_palette_colours() {
        let wall = one_pixel(247, (228, 208, 218));

        assert_eq!(
            wall.ink(0, Blit::Interface),
            Ink::Colour(Rgb::from([228, 208, 218]))
        );
        assert_eq!(wall.ink(0, Blit::World), Ink::Shade(Shade::Keep(50)));

        // 255: world skips it.
        let keyed = one_pixel(255, (116, 62, 74));

        assert_eq!(
            keyed.ink(0, Blit::Interface),
            Ink::Colour(Rgb::from([116, 62, 74]))
        );
        assert_eq!(keyed.ink(0, Blit::World), Ink::Hole);
    }

    /// A marked entry is an operation to both blits (the dialog scrim).
    #[test]
    fn the_interface_blit_still_reads_a_marked_index_as_an_operation() {
        let scrim = one_pixel(247, (128, 0, 128));

        assert_eq!(scrim.ink(0, Blit::Interface), Ink::Shade(Shade::Keep(50)));
        assert_eq!(scrim.ink(0, Blit::World), Ink::Shade(Shade::Keep(50)));

        let key = one_pixel(255, (255, 0, 255));

        assert_eq!(key.ink(0, Blit::Interface), Ink::Hole);
        assert_eq!(key.ink(0, Blit::World), Ink::Hole);
    }
}
