//! The browsers' layout: sprites in fixed cells, rows of them, a gap
//! between, composed into one sheet ([`regent_core::view::raster::atlas`]).

use regent_core::view::Sprite;
use regent_core::view::raster::atlas::Sheet;

/// Browser layout: fixed cells in rows, with a gap between.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub columns: usize,
    /// Cell size. Terrain is 32x32, but not assumed: larger sprites clip,
    /// smaller ones sit top left.
    pub cell: (u16, u16),
    pub gap: u16,
}

/// Lay sprites out in a grid.
///
/// A `None` cell stays transparent: a short slot or an unresolved reference.
#[must_use]
pub fn compose(sprites: &[Option<&Sprite>], grid: Grid) -> Sheet {
    let columns = grid.columns.max(1).min(sprites.len());
    let rows = sprites.len().div_ceil(columns.max(1));

    let pitch = |cell: u16| usize::from(cell) + usize::from(grid.gap);
    let extent = |n: usize, cell: u16| match n {
        0 => 0,
        n => (n - 1) * pitch(cell) + usize::from(cell),
    };

    let mut sheet = Sheet::blank(extent(columns, grid.cell.0), extent(rows, grid.cell.1));

    for (i, sprite) in sprites.iter().enumerate() {
        if let Some(sprite) = sprite {
            let x = (i % columns) * pitch(grid.cell.0);
            let y = (i / columns) * pitch(grid.cell.1);

            sheet.place(sprite, x, y, grid.cell);
        }
    }

    sheet
}

#[cfg(test)]
mod tests {
    use super::*;
    use regent_core::data::tile::{Mode, Packing};
    use regent_core::view::sprite::{Colour, PaletteSource};

    const RED: u16 = 0xf800;

    /// A solid direct-colour sprite, like terrain.
    fn solid(width: u16, height: u16, rgb565: u16) -> Sprite {
        let n = usize::from(width) * usize::from(height);

        Sprite {
            width,
            height,
            version: 1,
            anchor: (0, 0),
            covered: vec![true; n],
            colour: Colour::Direct {
                words: vec![rgb565; n],
                packing: Packing::Rgb565,
            },
            source: PaletteSource::None,
            mode: Mode::Plain,
            level: 0,
        }
    }

    /// The pixel at `(x, y)`, as RGBA.
    fn at(sheet: &Sheet, x: usize, y: usize) -> [u8; 4] {
        let i = (y * sheet.width + x) * 4;

        [
            sheet.rgba[i],
            sheet.rgba[i + 1],
            sheet.rgba[i + 2],
            sheet.rgba[i + 3],
        ]
    }

    #[test]
    fn a_cell_smaller_than_the_grid_sits_at_its_top_left() {
        let small = solid(1, 1, RED);
        let sheet = compose(
            &[Some(&small)],
            Grid {
                columns: 1,
                cell: (4, 4),
                gap: 0,
            },
        );

        assert_eq!((sheet.width, sheet.height), (4, 4));
        assert_eq!(at(&sheet, 0, 0)[0], 255);
        assert_eq!(at(&sheet, 1, 1), [0, 0, 0, 0], "the rest of the cell");
    }

    #[test]
    fn a_gap_separates_the_cells_of_a_browsers_grid() {
        let a = solid(2, 2, RED);
        let sheet = compose(
            &[Some(&a), Some(&a)],
            Grid {
                columns: 2,
                cell: (2, 2),
                gap: 1,
            },
        );

        // Two cells, one gap between, none outside.
        assert_eq!(sheet.width, 5);
        assert_eq!(at(&sheet, 2, 0), [0, 0, 0, 0], "the gap");
        assert_eq!(at(&sheet, 3, 0)[0], 255, "the second cell");
    }
}
