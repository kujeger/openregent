//! Whether a unit's tile takes a screen point: the pixel half of the
//! unit pick, unit hit. See [`super::hit`].

use crate::data::tile::{PixelFormat, Pixels, Run, Tile};

/// Whether the engine's pick takes pixel `(x, y)` of `tile`, from the top
/// left.
///
/// **Binary**, unit hit and the pixel read it
/// calls: a point on the canvas; a stored pixel —
/// any in versions 1 and 2, under a run in version 3 — and, on an
/// 8-bit tile, an index below [`UNPICKED`]. A run takes the column
/// past its end too (`jle` in sprite pixel runs), reading the byte after
/// its pixels. **Inferred**: past the last row that byte is unknown;
/// taken as a hit.
#[must_use]
pub fn picks(tile: &Tile<'_>, x: i32, y: i32) -> bool {
    let on =
        (0..i32::from(tile.width())).contains(&x) && (0..i32::from(tile.height())).contains(&y);
    let touch = if on { touch(tile, x, y) } else { Touch::Gap };

    match (touch, tile.format()) {
        (Touch::Gap, _) => false,
        (Touch::Pixel(Some(index)), PixelFormat::Indexed) => index < UNPICKED,
        (Touch::Pixel(_), _) => true,
    }
}

/// What the pick reads at `(x, y)`, both on the canvas.
fn touch(tile: &Tile<'_>, x: i32, y: i32) -> Touch {
    let at = usize::try_from(x).ok().zip(usize::try_from(y).ok());

    match (tile.pixels(), at) {
        (_, None) => Touch::Gap,
        (Pixels::Raw { bytes, stride }, Some((col, row))) => {
            Touch::Pixel(bytes.get(row * stride + col).copied())
        }
        (Pixels::Runs(rows), Some((_, row))) => run_touch(rows, row, x),
    }
}

/// What the pick reads under a point.
enum Touch {
    /// No stored pixel.
    Gap,
    /// A stored pixel: its first byte, an 8-bit tile's index; `None` for
    /// an unknown byte.
    Pixel(Option<u8>),
}

/// The pick's walk along row `y` to column `x`: stop in
/// a gap before a run, take a run's columns and the one past its end,
/// stop after the last run.
fn run_touch(rows: &[Vec<Run<'_>>], y: usize, x: i32) -> Touch {
    let row = rows.get(y).map_or(&[][..], Vec::as_slice);
    let step = |(i, run): (usize, &Run<'_>)| {
        let end = i32::from(run.x_end);
        let start = end - i32::from(run.count_flags & PICK_COUNT_MASK);

        if x < start {
            Some(Touch::Gap)
        } else if x <= end {
            let offset = usize::try_from(x - start).unwrap_or(usize::MAX);

            Some(Touch::Pixel(byte_at(rows, y, i, offset)))
        } else if run.is_last() {
            Some(Touch::Gap)
        } else {
            None
        }
    };

    row.iter().enumerate().find_map(step).unwrap_or(Touch::Gap)
}

/// Byte `offset` of run `i` of row `y`'s pixels; past them, the next
/// run's or row's first byte, its end column's low byte. `None` past the
/// last row.
fn byte_at(rows: &[Vec<Run<'_>>], y: usize, i: usize, offset: usize) -> Option<u8> {
    let run = rows.get(y)?.get(i)?;
    let next = || {
        rows[y]
            .get(i + 1)
            .or_else(|| rows.get(y + 1)?.first())
            .map(|r| r.x_end.to_le_bytes()[0])
    };

    run.pixels.get(offset).copied().or_else(next)
}

/// The pick's run count: 15 bits, where the blit takes 11.
///
/// **Binary**: a mask of `0x7fff` in sprite pixel runs.
const PICK_COUNT_MASK: u16 = 0x7fff;

/// The first index the pick passes over, any draw mode: 247 to 255, the
/// shadow and glow marks and 255.
///
/// **Binary**: unit hit, a range test from 247 over 8 values, on an
/// 8-bit tile only.
pub const UNPICKED: u8 = 247;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tile::VERSION_OPAQUE;
    use crate::data::tile::fixture::{raw_bytes, tile_bytes};

    #[test]
    fn the_pick_takes_a_runs_pixels_and_the_column_past_its_end() {
        // Width 9: index 0 at 1, a shadow at 2, a gap at 4, index 7 at 5
        // and 6; past it, the terminator's low byte 0xcd, 7 past again.
        let row = [
            3, 0, 0x02, 0x00, 0x00, 0xf8, // columns 1, 2
            7, 0, 0x02, 0x00, 0x07, 0x07, // columns 5, 6
            0xcd, 0xcd, 0x00, 0x80, // terminator
        ];
        let bytes = tile_bytes(9, 0, &row);
        let t = Tile::parse(&bytes).unwrap();
        let picked: Vec<bool> = (-1..10).map(|x| picks(&t, x, 0)).collect();

        // Column 3 reads the next run's 7; column 7, 0xcd.
        assert_eq!(
            picked,
            [
                false, false, true, false, true, false, true, true, true, false, false
            ]
        );
        assert!(!picks(&t, 1, 1), "below the canvas");
    }

    #[test]
    fn the_pick_passes_over_high_indices_in_a_raw_tile() {
        let bytes = raw_bytes(VERSION_OPAQUE, 3, 1, 0, &[0, UNPICKED - 1, UNPICKED]);
        let t = Tile::parse(&bytes).unwrap();

        assert_eq!([0, 1, 2].map(|x| picks(&t, x, 0)), [true, true, false]);
    }
}
