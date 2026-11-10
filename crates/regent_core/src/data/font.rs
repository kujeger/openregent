//! `FONT`: a bitmap font, a bit a pixel. **Binary**: the `CYFontPixelmap`
//! loader's branch for a record not opening `0xff`, the branch both shipped records take.
//!
//! ```text
//!  +0         u8        name length n, then the name
//!  h = n + 1  u16       k: words from h to the header, this one counted
//!  b = h + 2k 13 x u16  -, first, last, -, spacing (i16), -, -, rows,
//!                       widths at (words past b + 0x10), ascent,
//!                       descent, -, words a row
//!  b + 0x1a             bitmap: rows x (2 x words a row) bytes, MSB first
//!  w - 2c               c = last - first + 3 column starts, u16
//!  w = b + 0x10 + 2 x widths at
//!                       c x (u8 advance, u8 bearing)
//! ```
//!
//! A glyph is the bitmap's columns from its start to the next's
//! (`CYFontPixelmap`: bit `0x80 >> (x & 7)` of byte
//! `x >> 3`); it advances the pen its width plus the spacing, a space
//! its width alone. A code outside first to last draws the
//! entry after last.

use super::error::{Error, Result, slice, u16_at};

/// Where the 13 header words start past `b`; the bitmap after them.
const HEADER_LEN: usize = 0x1a;

/// The width table sits this far past `b`, plus twice its word.
const WIDTHS_BASE: usize = 0x10;

/// The first byte of a record in the other loader branch, not read.
const NEWER: u8 = 0xff;

/// One glyph: method's box and method's columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    /// Pixels the pen moves past it.
    pub advance: u8,
    /// method's left edge; method starts no later than the pen.
    pub bearing: u8,
    /// Its first column in the bitmap.
    pub column: u16,
    /// How many columns it spans.
    pub width: u16,
}

/// A decoded `FONT` record.
#[derive(Debug, Clone)]
pub struct Font<'a> {
    pub first: u16,
    pub last: u16,
    /// Pixels added to each advance but a space's (loaded).
    pub spacing: i16,
    /// Rows above and below the baseline (loaded).
    pub ascent: u16,
    pub descent: u16,
    /// Bitmap rows, and bytes a row.
    pub rows: usize,
    pub stride: usize,
    pub bitmap: &'a [u8],
    /// First to last, then the fallback.
    pub glyphs: Vec<Glyph>,
}

impl<'a> Font<'a> {
    /// # Errors
    ///
    /// A record of the other loader branch, or one whose tables run past
    /// its end.
    pub fn parse(data: &'a [u8]) -> Result<Font<'a>> {
        let n = usize::from(*data.first().ok_or(Error::Truncated {
            what: "font name length",
            need: 1,
            have: 0,
        })?);

        if n == usize::from(NEWER) {
            return Err(Error::BadValue {
                what: "font loader branch",
                value: u64::from(NEWER),
            });
        }

        slice(data, "font name", 1, n)?;
        let h = n + 1;
        let b = h + 2 * usize::from(u16_at(data, "font preamble", h)?);
        let word = |i: usize| u16_at(data, "font header", b + 2 * i);
        let (first, last) = (word(1)?, word(2)?);
        let spacing = word(4)?.cast_signed();
        let rows = usize::from(word(7)?);
        let widths = b + WIDTHS_BASE + 2 * usize::from(word(8)?);
        let (ascent, descent) = (word(9)?, word(10)?);
        let stride = 2 * usize::from(word(12)?);
        let bitmap = slice(data, "font bitmap", b + HEADER_LEN, rows * stride)?;
        let count = usize::from(last.saturating_sub(first)) + 1;
        let starts = widths.checked_sub(2 * (count + 2)).ok_or(Error::BadValue {
            what: "font column table",
            value: widths as u64,
        })?;
        let start = |i: usize| u16_at(data, "font column", starts + 2 * i);
        let glyphs = (0..=count)
            .map(|i| {
                let pair = slice(data, "font width", widths + 2 * i, 2)?;
                let column = start(i)?;

                Ok(Glyph {
                    advance: pair[0],
                    bearing: pair[1],
                    column,
                    width: start(i + 1)?.saturating_sub(column),
                })
            })
            .collect::<Result<Vec<Glyph>>>()?;

        Ok(Font {
            first,
            last,
            spacing,
            ascent,
            descent,
            rows,
            stride,
            bitmap,
            glyphs,
        })
    }

    /// The glyph character `c` draws: the fallback outside first to last.
    #[must_use]
    pub fn glyph(&self, c: u8) -> Glyph {
        let i = u16::from(c)
            .checked_sub(self.first)
            .filter(|_| u16::from(c) <= self.last)
            .map_or(self.glyphs.len() - 1, usize::from);

        self.glyphs[i]
    }

    /// Whether `glyph`'s pixel at column `x`, row `y` is set.
    #[must_use]
    pub fn lit(&self, glyph: Glyph, x: u16, y: usize) -> bool {
        let at = usize::from(glyph.column + x);
        let byte = self.bitmap.get(y * self.stride + (at >> 3)).copied();

        x < glyph.width && byte.is_some_and(|b| b & (0x80 >> (at & 7)) != 0)
    }

    /// Pixels the pen moves over `text`.
    #[must_use]
    pub fn width(&self, text: &[u8]) -> i32 {
        text.iter()
            .map(|c| {
                let extra = if *c == b' ' {
                    0
                } else {
                    i32::from(self.spacing)
                };

                i32::from(self.glyph(*c).advance) + extra
            })
            .sum()
    }

    /// `text`'s set pixels, x and y from the pen's start and the top row:
    /// each glyph from the pen, as method starts it with a bearing of 0
    /// or more.
    #[must_use]
    pub fn pixels(&self, text: &[u8]) -> Vec<(i32, i32)> {
        let mut pen = 0;
        let mut out = Vec::new();

        for c in text {
            let glyph = self.glyph(*c);

            for y in 0..self.rows {
                for x in (0..glyph.width).filter(|x| self.lit(glyph, *x, y)) {
                    out.push((pen + i32::from(x), i32::try_from(y).unwrap_or(i32::MAX)));
                }
            }

            pen += self.width(&[*c]);
        }

        out
    }
}
