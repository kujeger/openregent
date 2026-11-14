//! The 26-byte header every version shares: versions, flags, draw mode,
//! palette.

/// The row-RLE tile version.
pub const VERSION_RLE: u16 = 3;

/// The uncompressed versions. Both read identically; see the module docs.
pub const VERSIONS_RAW: [u16; 2] = [VERSION_OPAQUE, VERSION_KEYED];

/// Uncompressed and opaque: index 255 is a colour, draw mode 0 only.
///
/// **Binary**: its blit table, blits opaque, fills only the mode 0
/// column, and the routine there is a `rep movsd` row copy.
pub const VERSION_OPAQUE: u16 = 1;

/// Uncompressed, index 255 transparent, every draw mode.
///
/// **Binary**: its blit table, blits keyed, fills modes 0 to 8; the
/// 8-bit copy skips a byte of 255. **Corpus**: 8 of the 10
/// version 2 tiles hold 255, one 10 by 10 on 44 pixels.
pub const VERSION_KEYED: u16 = 2;

/// Header length: start of the row-offset table (v3) or pixels (v1, v2).
pub(super) const HEADER_LEN: usize = 26;
/// Palette mode words at `+0x14`: an `SPLT` index, or a palette in the payload.
pub(super) const PALETTE_SPLT: u16 = 0;
pub(super) const PALETTE_EMBEDDED: u16 = 1;

/// How a direct-colour tile packs its three channels into a word.
///
/// Both ship. The wrong one draws green noise over a correct picture, e.g.
/// `BBA1 Animal_Den`'s portrait read as 565.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Packing {
    /// `0rrrrrgg gggbbbbb`. 144 tiles; none of their 4,338,291 words sets
    /// bit 15.
    Rgb555,
    /// `rrrrrggg gggbbbbb`, the extra bit to green.
    Rgb565,
}

/// Stored pixel width and layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// One byte per pixel, indexing the tile's palette.
    Indexed,
    /// Two bytes per pixel, little-endian, in this packing.
    Direct(Packing),
}

/// Flags bit 0: two bytes per pixel.
const DIRECT: u16 = 1;

/// Flags bit 2: green gets the sixth bit. Bits 2 and 3 say how a 16-bit word
/// packs; only 0 and 4 ship.
///
/// **Binary**: sprite convert masks the flags with
/// `0x0c` and converts to the screen's format: sprite to turns 565 into 555
/// (`>> 1 & 0x7fe0`), sprite to 555 into 565, sprite to 565 into 4444.
/// **Corpus** agrees: of 2,274 direct tiles, the 144 without it never set bit
/// 15; the 2,130 with it do, in one word in eight.
/// Held by `the_packing_bit_agrees_with_the_pixels_it_describes`.
const WIDE_GREEN: u16 = 4;

/// Flags bit 4: the pixels are stored mirrored.
///
/// **Binary**: sprite mirror flips the pixels and
/// toggles this bit until it matches the frame's [`MIRROR`] request. **Corpus**:
/// the 22 tiles with it are exactly the 22 frames with the request, so each
/// draws as stored.
///
/// [`MIRROR`]: crate::data::imag::MIRROR
pub const MIRRORED: u16 = 0x10;

/// Flags bit 5: the palette is the `SPLT` record `+0x16` names.
///
/// **Binary**: the loader asks for `SPLT` only when the
/// embedded count is 0 and this bit is set. **Corpus**: set on every indexed
/// tile without an embedded palette, and no other.
pub const SPLT_PALETTE: u16 = 0x20;

/// Flags bits 8 to 12: a level for [`Mode::Translucent`] and mode 8.
///
/// **Binary**: draw params takes `(flags >> 8) & 0x1f`.
pub(super) const LEVEL_SHIFT: u16 = 8;
pub(super) const LEVEL_MASK: u16 = 0x1f;

/// How the blit treats an indexed pixel: the word at `+0x0e`.
///
/// **Binary**: draw params copies it as the blit-table column. Values
/// 0, 1, 2, 5, 7, 8 ship; version 1 tiles are all 0, as their table demands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// Every index a colour. Version 3's 8-to-16-bit blit
    /// compares no index.
    Plain,
    /// 247 to 254 darken, eight steps: the `(50, 6)` ramp family, tables at
    /// ramps shadow8 (blit blit runs shadow). The texture path gives the same
    /// indices black at falling alpha.
    Shadow,
    /// 247 to 250 darken, 251 to 254 lighten: families `(50, 12)` and
    /// `(30, 12)` at ramps band (blit blit runs band). What
    /// [`crate::view::raster::shade::BAND`] models.
    Band,
    /// Drawn translucent at [`Tile::level`](super::Tile::level). The texture path gives alpha
    /// `(10 - level) × 25` for levels 1 to 9.
    Translucent,
    /// 2 or 8, the other two that ship; 8 takes a level too. The blit
    /// table's other columns, 3, 4 and 6, ship on no tile and are refused.
    Other(u16),
}

impl Mode {
    const PLAIN: u16 = 0;
    const SHADOW: u16 = 1;
    const UNREAD: u16 = 2;
    const TRANSLUCENT: u16 = 5;
    const BAND: u16 = 7;
    const GLOW: u16 = 8;

    /// The word at `+0x0e`, or `None` for one no tile ships.
    pub const fn from_word(word: u16) -> Option<Self> {
        match word {
            Self::PLAIN => Some(Mode::Plain),
            Self::SHADOW => Some(Mode::Shadow),
            Self::TRANSLUCENT => Some(Mode::Translucent),
            Self::BAND => Some(Mode::Band),
            Self::UNREAD | Self::GLOW => Some(Mode::Other(word)),
            _ => None,
        }
    }

    /// The word back, and the blit-table column.
    pub const fn word(self) -> u16 {
        match self {
            Mode::Plain => Self::PLAIN,
            Mode::Shadow => Self::SHADOW,
            Mode::Translucent => Self::TRANSLUCENT,
            Mode::Band => Self::BAND,
            Mode::Other(word) => word,
        }
    }
}

impl PixelFormat {
    /// Bits 0 and 2 of the flags word at `+0x08`.
    pub(super) const fn from_flags(flags: u16) -> Self {
        if flags & DIRECT == 0 {
            PixelFormat::Indexed
        } else if flags & WIDE_GREEN == 0 {
            PixelFormat::Direct(Packing::Rgb555)
        } else {
            PixelFormat::Direct(Packing::Rgb565)
        }
    }

    pub const fn bytes(self) -> usize {
        match self {
            PixelFormat::Indexed => 1,
            PixelFormat::Direct(_) => 2,
        }
    }
}

/// Where an indexed tile's colours come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteRef {
    /// An index into the archive's `SPLT` chunk.
    Splt(u32),
    /// A byte offset inside this payload, past the last row.
    Embedded(usize),
}
