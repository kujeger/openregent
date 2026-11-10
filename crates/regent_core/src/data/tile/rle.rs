//! Version 3: a row-offset table, then each row as runs of pixels.

use super::header::{HEADER_LEN, PixelFormat};
use super::{PaletteRef, Pixels};
use crate::data::error::{Error, Result, slice, u16_at, u32_at};
use crate::data::payload::PALETTE_LEN;

const COUNT_MASK: u16 = 0x07ff;
const LAST_SEGMENT: u16 = 0x8000;

/// One horizontal run of stored pixels.
///
/// Covers `[x_end - count, x_end)`. Kept unexpanded so an encoder can
/// reproduce them; [`Tile::indices`](super::Tile::indices) rasterises.
#[derive(Debug, Clone, Copy)]
pub struct Run<'a> {
    /// Exclusive end column. Meaningless when the run is empty.
    pub x_end: u16,
    /// The whole word, so the upper bits round-trip.
    pub count_flags: u16,
    /// `count` pixels, `format.bytes()` bytes each.
    pub pixels: &'a [u8],
}

impl Run<'_> {
    pub const fn count(&self) -> u16 {
        self.count_flags & COUNT_MASK
    }

    pub const fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// First column covered, or `None` for an empty run.
    pub fn start(&self) -> Option<u16> {
        (!self.is_empty()).then(|| self.x_end - self.count())
    }

    /// Whether this run ends its row.
    pub const fn is_last(&self) -> bool {
        self.count_flags & LAST_SEGMENT != 0
    }
}

/// Decode a version 3 body: the row-offset table and the runs it points at.
pub(super) fn parse_runs(
    payload: &[u8],
    width: u16,
    height: u16,
    format: PixelFormat,
    embedded: bool,
    palette_word: u32,
) -> Result<(PaletteRef, Pixels<'_>)> {
    // A zero-height tile has no row table.
    let offsets = (0..usize::from(height))
        .map(|r| u32_at(payload, "tile row offset", HEADER_LEN + 4 * r).map(|o| o as usize))
        .collect::<Result<Vec<_>>>()?;

    // An embedded palette follows the last row, so it ends the row data.
    let (palette, body_end) = if embedded {
        let at = palette_word as usize;

        if at <= HEADER_LEN || at.saturating_add(PALETTE_LEN) > payload.len() {
            return Err(Error::OutOfBounds {
                what: "embedded tile palette",
                offset: at,
                len: PALETTE_LEN,
                size: payload.len(),
            });
        }

        (PaletteRef::Embedded(at), at)
    } else {
        (PaletteRef::Splt(palette_word), payload.len())
    };

    // Each row runs to the next row's offset, the last to the body end. The
    // segments must fill it exactly, so a wrong pixel format fails.
    let rows = offsets
        .iter()
        .enumerate()
        .map(|(r, &start)| {
            let end = match offsets.get(r + 1) {
                Some(&next) => HEADER_LEN + next,
                None => body_end,
            };

            let start = HEADER_LEN + start;
            let span = end
                .checked_sub(start)
                .ok_or(Error::BrokenInvariant {
                    what: "tile row order",
                    expected: start as u64,
                    found: end as u64,
                })
                .and_then(|len| slice(payload, "tile row", start, len))?;

            parse_row(span, width, format)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok((palette, Pixels::Runs(rows)))
}

/// Split one row's bytes into its segments.
fn parse_row(row: &[u8], width: u16, format: PixelFormat) -> Result<Vec<Run<'_>>> {
    let mut runs = Vec::new();
    let mut at = 0;
    let mut terminated = false;

    while at < row.len() {
        let x_end = u16_at(row, "tile run end", at)?;
        let count_flags = u16_at(row, "tile run count", at + 2)?;
        let count = count_flags & COUNT_MASK;
        let bytes = usize::from(count) * format.bytes();

        // An empty run's x_end is uninitialised in 304 shipped tiles, so only
        // runs with pixels are range-checked.
        if count > 0 {
            if count > x_end {
                return Err(Error::BrokenInvariant {
                    what: "tile run starts before column 0",
                    expected: count.into(),
                    found: x_end.into(),
                });
            }

            if width > 0 && x_end > width {
                return Err(Error::BrokenInvariant {
                    what: "tile run ends past the canvas",
                    expected: width.into(),
                    found: x_end.into(),
                });
            }
        }

        let pixels = slice(row, "tile run pixels", at + 4, bytes)?;
        at += 4 + bytes;

        let run = Run {
            x_end,
            count_flags,
            pixels,
        };
        terminated = run.is_last();
        runs.push(run);

        if terminated {
            break;
        }
    }

    // The row must end exactly on its terminator, or a wrong pixel format can
    // stop early or overrun unnoticed.
    if !terminated || at != row.len() {
        return Err(Error::BrokenInvariant {
            what: "tile row length",
            expected: row.len() as u64,
            found: at as u64,
        });
    }

    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::super::Tile;
    use super::super::fixture::{rows, tile_bytes};
    use crate::data::error::Error;

    #[test]
    fn a_run_covers_the_columns_below_its_end() {
        // Two pixels ending at column 5 occupy 3 and 4, not 5 and 6.
        let bytes = tile_bytes(5, 0, &[5, 0, 0x02, 0x80, 0xaa, 0xbb]);
        let t = Tile::parse(&bytes).unwrap();
        let run = rows(&t)[0][0];

        assert_eq!(run.start(), Some(3));
        assert_eq!(run.x_end, 5);
        assert_eq!(
            t.indices().unwrap(),
            [None, None, None, Some(0xaa), Some(0xbb)]
        );
    }

    #[test]
    fn index_zero_is_a_colour_and_not_a_gap() {
        let bytes = tile_bytes(2, 0, &[2, 0, 0x02, 0x80, 0x00, 0x00]);

        let drawn = Tile::parse(&bytes).unwrap().indices().unwrap();

        assert_eq!(drawn, [Some(0), Some(0)]);
    }

    #[test]
    fn an_empty_terminator_may_carry_a_garbage_column() {
        // 304 shipped tiles end a row with 0xcdcd (MSVC heap fill).
        let bytes = tile_bytes(8, 0, &[0xcd, 0xcd, 0x00, 0x80]);
        let t = Tile::parse(&bytes).unwrap();

        assert!(rows(&t)[0][0].is_empty());
        assert_eq!(rows(&t)[0][0].start(), None);
        assert_eq!(t.max_end(), 0);
    }

    #[test]
    fn a_run_past_the_canvas_is_rejected() {
        let bytes = tile_bytes(4, 0, &[9, 0, 0x02, 0x80, 0xaa, 0xbb]);

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn a_run_starting_left_of_zero_is_rejected() {
        // count 4 ending at column 2 would start at -2.
        let bytes = tile_bytes(8, 0, &[2, 0, 0x04, 0x80, 1, 2, 3, 4]);

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }

    #[test]
    fn a_row_must_end_on_its_terminator() {
        // Same runs, no end bit: the row is one byte short of accounted for.
        let bytes = tile_bytes(5, 0, &[5, 0, 0x02, 0x00, 0xaa, 0xbb]);

        assert!(matches!(
            Tile::parse(&bytes).unwrap_err(),
            Error::BrokenInvariant { .. }
        ));
    }
}
