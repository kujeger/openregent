//! Compositing sprites into one RGBA buffer: a board edge to edge, or
//! cells a caller lays out (`regent_viewer`'s labelled grid).
//!
//! # Why compose
//!
//! Seams. Drawing each tile at its own transform rounds each independently; at
//! fractional zoom a row shows one-pixel gaps and doubled columns that look
//! like wrong tile choices. A composed image magnifies as one picture.
//!
//! # Laying out versus drawing
//!
//! [`tile`] and [`Sheet::place`] copy rows into non-overlapping cells.
//! [`Sheet::draw`] draws one sprite over the sheet in engine order. A shadow acts on the destination
//! ([`crate::view::raster::shade`]), so only [`Sheet::draw`] produces one, and only over
//! something.

use super::shade::Shade;
use crate::data::imag::Blit;
use crate::data::payload::Rgb;
use crate::view::sprite::{Ink, Sprite};

/// Bytes per pixel: straight RGBA8, as [`Sprite::rgba`].
const CHANNELS: usize = 4;

/// Alpha of a drawn pixel.
const OPAQUE: u8 = u8::MAX;

/// What a control-band pixel reads where the sheet is empty.
///
/// The engine always has a destination (the ground in the framebuffer); a
/// sheet has holes.
#[derive(Debug, Clone, Copy)]
pub enum Backdrop {
    /// Shadow over a hole draws nothing.
    None,
    /// One colour behind the whole sheet, like a browser background.
    Flat(Rgb),
}

/// A composed picture, pixels row-major from top left.
///
/// Transparent where nothing was placed, so holes stay visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Sheet {
    /// A transparent picture of this size.
    #[must_use]
    pub fn blank(width: usize, height: usize) -> Self {
        Sheet {
            width,
            height,
            rgba: vec![0; width * height * CHANNELS],
        }
    }

    /// Draw `sprite` over the sheet, top left at `at`.
    ///
    /// Painter order, so a shadow lands on what was drawn before. Clipped to
    /// the sheet; `at` is signed, so a sprite half off the edge draws its other
    /// half. `under` is what a shadow finds where the sheet is empty; `blit`
    /// says which pixels are shadows. A shadow is the engine's
    /// ([`Shade::over`]).
    pub fn draw(&mut self, sprite: &Sprite, at: (i32, i32), under: Backdrop, blit: Blit) {
        self.draw_with(sprite, at, under, blit, Shade::over);
    }

    /// [`Sheet::draw`], a shadow drawn by `shade` over what is under it: a
    /// front end's own shading.
    pub fn draw_with(
        &mut self,
        sprite: &Sprite,
        at: (i32, i32),
        under: Backdrop,
        blit: Blit,
        shade: impl Fn(Shade, Rgb) -> Rgb,
    ) {
        // Sprite rows and columns inside the sheet, computed once, not per pixel.
        let span = |offset: i32, extent: u16, limit: usize| {
            let limit = i32::try_from(limit).unwrap_or(i32::MAX);
            let extent = i32::from(extent);

            (-offset).clamp(0, extent)..(limit - offset).clamp(0, extent)
        };

        let width = usize::from(sprite.width);

        for row in span(at.1, sprite.height, self.height) {
            for col in span(at.0, sprite.width, self.width) {
                #[expect(
                    clippy::cast_sign_loss,
                    reason = "the spans are clipped to the sheet, so both sums are in it"
                )]
                let (x, y) = ((at.0 + col) as usize, (at.1 + row) as usize);
                #[expect(clippy::cast_sign_loss, reason = "the spans start at zero or above")]
                let from = row as usize * width + col as usize;
                let to = (y * self.width + x) * CHANNELS;

                match sprite.ink(from, blit) {
                    Ink::Hole => {}
                    Ink::Colour(rgb) => self.put(to, rgb),
                    Ink::Shade(ink) => self.shade(to, under, |rgb| shade(ink, rgb)),
                }
            }
        }
    }

    /// Write an opaque pixel at byte offset `to`.
    fn put(&mut self, to: usize, Rgb { r, g, b }: Rgb) {
        self.rgba[to..to + CHANNELS].copy_from_slice(&[r, g, b, OPAQUE]);
    }

    /// Shade the pixel at `to`, or the backdrop behind it, by `shade`.
    fn shade(&mut self, to: usize, under: Backdrop, shade: impl Fn(Rgb) -> Rgb) {
        let destination = match (self.rgba[to + 3], under) {
            // Already drawn (ground or earlier sprite): shadow falls on it.
            (OPAQUE, _) => Some(Rgb::from([
                self.rgba[to],
                self.rgba[to + 1],
                self.rgba[to + 2],
            ])),
            (_, Backdrop::Flat(colour)) => Some(colour),
            (_, Backdrop::None) => None,
        };

        if let Some(destination) = destination {
            self.put(to, shade(destination));
        }
    }

    /// Copy `sprite` in at `(x, y)`, clipped to `cell` and the sheet.
    ///
    /// Clipped, not refused: an oversize sprite is a finding. Not spilled into
    /// the neighbour cell, which would misattribute pixels.
    pub fn place(&mut self, sprite: &Sprite, x: usize, y: usize, cell: (u16, u16)) {
        let src = sprite.rgba();
        let stride = self.width * CHANNELS;

        let visible = |offset: usize, extent: u16, bound: u16, limit: usize| -> usize {
            usize::from(extent.min(bound)).min(limit.saturating_sub(offset))
        };

        let cols = visible(x, sprite.width, cell.0, self.width);
        let rows = visible(y, sprite.height, cell.1, self.height);

        for row in 0..rows {
            let from = row * usize::from(sprite.width) * CHANNELS;
            let to = (y + row) * stride + x * CHANNELS;

            self.rgba[to..to + cols * CHANNELS].copy_from_slice(&src[from..from + cols * CHANNELS]);
        }
    }
}

/// Lay cells out edge to edge, `width` to a row, for a board. A seam in
/// the result is a seam in the data. A `None` cell stays transparent.
#[must_use]
pub fn tile(cells: &[Option<&Sprite>], width: usize, cell: (u16, u16)) -> Sheet {
    let columns = width.max(1).min(cells.len());
    let rows = cells.len().div_ceil(columns.max(1));
    let (cell_width, cell_height) = (usize::from(cell.0), usize::from(cell.1));
    let mut sheet = Sheet::blank(columns * cell_width, rows * cell_height);

    for (i, sprite) in cells.iter().enumerate() {
        if let Some(sprite) = sprite {
            let (x, y) = ((i % columns) * cell_width, (i / columns) * cell_height);

            sheet.place(sprite, x, y, cell);
        }
    }

    sheet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::payload::{Palette, Rgb};
    use crate::data::tile::{Mode, Packing};
    use crate::view::sprite::{Colour, PaletteSource};

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
        let i = (y * sheet.width + x) * CHANNELS;

        [
            sheet.rgba[i],
            sheet.rgba[i + 1],
            sheet.rgba[i + 2],
            sheet.rgba[i + 3],
        ]
    }

    const RED: u16 = 0xf800;
    const BLUE: u16 = 0x001f;

    #[test]
    fn a_second_tile_lands_beside_the_first_and_not_under_it() {
        let a = solid(2, 2, RED);
        let b = solid(2, 2, BLUE);

        let sheet = tile(&[Some(&a), Some(&b)], 2, (2, 2));

        assert_eq!((sheet.width, sheet.height), (4, 2));
        assert_eq!(at(&sheet, 0, 0)[0], 255, "the first is red");
        assert_eq!(at(&sheet, 2, 0)[2], 255, "the second is blue, beside it");
    }

    #[test]
    fn a_short_last_row_leaves_its_gap_transparent() {
        let a = solid(2, 2, RED);

        // Three cells in a two-wide grid: the fourth is never written.
        let sheet = tile(&[Some(&a), Some(&a), Some(&a)], 2, (2, 2));

        assert_eq!((sheet.width, sheet.height), (4, 4));
        assert_eq!(at(&sheet, 0, 2), [255, 0, 0, 255], "the third cell");
        assert_eq!(at(&sheet, 2, 2), [0, 0, 0, 0], "the gap beside it");
    }

    #[test]
    fn an_absent_sprite_leaves_its_cell_transparent() {
        let a = solid(2, 2, RED);
        let sheet = tile(&[None, Some(&a)], 2, (2, 2));

        assert_eq!(at(&sheet, 0, 0), [0, 0, 0, 0]);
        assert_eq!(at(&sheet, 2, 0)[0], 255);
    }

    #[test]
    fn compositing_does_not_flip_rows() {
        // Two colours, so a flipped copy shows. Formats and sheet both count
        // rows downward.
        let sprite = Sprite {
            width: 1,
            height: 2,
            version: 1,
            anchor: (0, 0),
            covered: vec![true; 2],
            colour: Colour::Direct {
                words: vec![RED, BLUE],
                packing: Packing::Rgb565,
            },
            source: PaletteSource::None,
            mode: Mode::Plain,
            level: 0,
        };

        let sheet = tile(&[Some(&sprite)], 1, (1, 2));

        assert_eq!(at(&sheet, 0, 0)[0], 255, "row 0 is the first pixel");
        assert_eq!(at(&sheet, 0, 1)[2], 255, "row 1 is the second");
    }

    #[test]
    fn a_sprite_wider_than_its_cell_is_clipped_rather_than_refused() {
        let wide = solid(4, 1, RED);
        let sheet = tile(&[Some(&wide), None], 2, (2, 1));

        assert_eq!(sheet.width, 4);
        assert_eq!(at(&sheet, 1, 0)[0], 255, "inside the cell");
        assert_eq!(
            at(&sheet, 2, 0),
            [0, 0, 0, 0],
            "past it, into the next cell"
        );
    }

    #[test]
    fn an_indexed_sprite_composites_through_its_palette() {
        // Terrain is direct colour; the compositor handles indexed art too.
        let mut colours = [Rgb { r: 0, g: 0, b: 0 }; 256];
        colours[1] = Rgb {
            r: 10,
            g: 20,
            b: 30,
        };

        let sprite = Sprite {
            width: 1,
            height: 1,
            version: 3,
            anchor: (0, 0),
            covered: vec![true],
            colour: Colour::Indexed {
                indices: vec![1],
                palette: Box::new(Palette { colours }),
            },
            source: PaletteSource::Embedded,
            mode: Mode::Band,
            level: 0,
        };

        let sheet = tile(&[Some(&sprite)], 1, (1, 1));

        assert_eq!(at(&sheet, 0, 0), [10, 20, 30, 255]);
    }

    /// A one-row indexed sprite for control indices.
    ///
    /// Every palette entry is the band's placeholder magenta, which a blitter
    /// that forgot to branch would draw.
    fn indexed(indices: &[u8]) -> Sprite {
        let colours = [Rgb {
            r: 254,
            g: 0,
            b: 254,
        }; 256];

        Sprite {
            width: u16::try_from(indices.len()).unwrap(),
            height: 1,
            version: 3,
            anchor: (0, 0),
            covered: vec![true; indices.len()],
            colour: Colour::Indexed {
                indices: indices.to_vec(),
                palette: Box::new(Palette { colours }),
            },
            source: PaletteSource::Embedded,
            mode: Mode::Band,
            level: 0,
        }
    }

    /// The dirt measured off a screenshot in `docs/formats/art.md`.
    const DIRT: Rgb = Rgb {
        r: 112,
        g: 84,
        b: 56,
    };

    /// [`DIRT`] as a direct-colour ground sprite.
    fn ground(width: u16) -> Sprite {
        let n = usize::from(width);
        let word = crate::view::raster::colour::narrow(DIRT);

        Sprite {
            width,
            height: 1,
            version: 1,
            anchor: (0, 0),
            covered: vec![true; n],
            colour: Colour::Direct {
                words: vec![word; n],
                packing: Packing::Rgb565,
            },
            source: PaletteSource::None,
            mode: Mode::Plain,
            level: 0,
        }
    }

    #[test]
    fn a_shadow_darkens_the_pixel_already_in_the_sheet() {
        let mut sheet = Sheet::blank(2, 1);

        sheet.draw(&ground(2), (0, 0), Backdrop::None, Blit::World);

        let lit = at(&sheet, 0, 0);

        sheet.draw(&indexed(&[247]), (1, 0), Backdrop::None, Blit::World);

        let shaded = at(&sheet, 1, 0);
        let expected = crate::view::raster::shade::apply(247, Rgb::from([lit[0], lit[1], lit[2]]));
        let Rgb { r, g, b } = expected.unwrap();

        assert_eq!(shaded, [r, g, b, 255]);
        assert!(shaded[0] < lit[0], "{shaded:?} is not darker than {lit:?}");
    }

    #[test]
    fn a_shadow_over_a_hole_draws_nothing() {
        let mut sheet = Sheet::blank(1, 1);

        sheet.draw(&indexed(&[247]), (0, 0), Backdrop::None, Blit::World);

        assert_eq!(at(&sheet, 0, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn a_shadow_over_a_hole_falls_on_the_backdrop() {
        let mut sheet = Sheet::blank(1, 1);

        sheet.draw(&indexed(&[247]), (0, 0), Backdrop::Flat(DIRT), Blit::World);

        let Rgb { r, g, b } = crate::view::raster::shade::apply(247, DIRT).unwrap();

        assert_eq!(at(&sheet, 0, 0), [r, g, b, 255]);
    }

    #[test]
    fn a_glow_lightens_and_the_band_never_draws_its_palette() {
        let mut sheet = Sheet::blank(2, 1);

        sheet.draw(&ground(2), (0, 0), Backdrop::None, Blit::World);
        sheet.draw(&indexed(&[247, 254]), (0, 0), Backdrop::None, Blit::World);

        let (dark, bright) = (at(&sheet, 0, 0), at(&sheet, 1, 0));

        assert!(
            dark[0] < DIRT.r && bright[0] > DIRT.r,
            "{dark:?} {bright:?}"
        );
        // The placeholder magenta is never drawn.
        assert!(dark[1] > 0 && bright[1] > 0, "green survived the band");
    }

    #[test]
    fn the_transparent_index_draws_nothing_though_it_is_covered() {
        // An uncompressed tile's only hole is index 255; skipped regardless of
        // palette.
        let mut sheet = Sheet::blank(2, 1);

        sheet.draw(&indexed(&[1, 255]), (0, 0), Backdrop::None, Blit::World);

        assert_eq!(at(&sheet, 0, 0)[3], 255, "an ordinary index");
        assert_eq!(at(&sheet, 1, 0), [0, 0, 0, 0]);
    }

    #[test]
    fn a_sprite_half_off_the_edge_draws_the_half_that_is_on() {
        let mut sheet = Sheet::blank(2, 1);

        sheet.draw(&indexed(&[1, 1, 1]), (-2, 0), Backdrop::None, Blit::World);

        assert_eq!(at(&sheet, 0, 0)[3], 255, "its third pixel");
        assert_eq!(at(&sheet, 1, 0), [0, 0, 0, 0], "and nothing beyond it");
    }

    #[test]
    fn nothing_at_all_composes_to_nothing() {
        let sheet = tile(&[], 4, (32, 32));

        assert_eq!((sheet.width, sheet.height), (0, 0));
        assert_eq!(sheet.rgba.len(), 0);
    }
}
