//! Pictures that are only bands of colour, drawn as shapes at the
//! window's pixels instead of a small bitmap scaled up and filtered.
//!
//! ```text
//!  a bar's fill (INBc 1004, 163 × 12)    each row its mean colour, over
//!                                        the pixels its track lacks
//!  an edge piece (INDz 1006, 4 × 245)    each column one colour: exact
//! ```
//!
//! **Inferred**: a bar's fill is a painted bevel, its rows dithered; a
//! row's mean stands in for its pixels.

use bevy_egui::egui::{Color32, ColorImage, Pos2, Rect, Vec2, pos2, vec2};

/// Which way a picture's bands run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Across {
    /// One colour a row, top to bottom.
    Rows,
    /// One colour a column, left to right.
    Columns,
}

/// A picture as bands of one colour each.
#[derive(Clone, Debug, PartialEq)]
pub struct Stripes {
    pub across: Across,
    /// The picture's size, pixels.
    pub size: Vec2,
    /// Where the bands lie in it, pixels.
    pub rect: Rect,
    /// Each band's colour, in order.
    pub colours: Vec<Color32>,
}

impl Stripes {
    /// `image` where each row, else each column, is one colour; `None`
    /// where neither is.
    #[must_use]
    pub fn exact(image: &ColorImage) -> Option<Stripes> {
        let [w, h] = image.size;
        let px = |x: usize, y: usize| image.pixels[y * w + x];
        let rows = (0..h).all(|y| (0..w).all(|x| px(x, y) == px(0, y)));
        let columns = (0..w).all(|x| (0..h).all(|y| px(x, y) == px(x, 0)));
        let whole = Rect::from_min_size(Pos2::ZERO, size(image));

        match (w * h > 0, rows, columns) {
            (true, true, _) => Some(Stripes {
                across: Across::Rows,
                size: whole.size(),
                rect: whole,
                colours: (0..h).map(|y| px(0, y)).collect(),
            }),
            (true, false, true) => Some(Stripes {
                across: Across::Columns,
                size: whole.size(),
                rect: whole,
                colours: (0..w).map(|x| px(x, 0)).collect(),
            }),
            _ => None,
        }
    }

    /// A bar's `fill` as rows, each its mean colour: over the pixels
    /// where it differs from its `track` (the frame they share left to
    /// the track's picture), else all of it. `None` where nothing
    /// differs.
    #[must_use]
    pub fn rows(fill: &ColorImage, track: Option<&ColorImage>) -> Option<Stripes> {
        let [w, h] = fill.size;
        let px = |x: usize, y: usize| fill.pixels[y * w + x];
        let differs = |x: usize, y: usize| {
            track
                .filter(|t| t.size == fill.size)
                .is_none_or(|t| t.pixels[y * w + x] != px(x, y))
        };
        let cells: Vec<(usize, usize)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| differs(x, y))
            .collect();
        let (x0, x1) = span(cells.iter().map(|c| c.0))?;
        let (y0, y1) = span(cells.iter().map(|c| c.1))?;

        #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
        let rect = Rect::from_min_max(
            pos2(x0 as f32, y0 as f32),
            pos2((x1 + 1) as f32, (y1 + 1) as f32),
        );

        Some(Stripes {
            across: Across::Rows,
            size: size(fill),
            rect,
            colours: (y0..=y1)
                .map(|y| mean((x0..=x1).map(|x| px(x, y))))
                .collect(),
        })
    }

    /// Each band's rectangle with the picture laid over `onto`.
    #[must_use]
    pub fn bands(&self, onto: Rect) -> Vec<(Rect, Color32)> {
        let scale = onto.size() / self.size;
        let rect = Rect::from_min_size(
            onto.min + self.rect.min.to_vec2() * scale,
            self.rect.size() * scale,
        );
        #[allow(clippy::cast_precision_loss, reason = "a picture's bands")]
        let count = self.colours.len() as f32;

        self.colours
            .iter()
            .enumerate()
            .map(|(i, &colour)| {
                #[allow(clippy::cast_precision_loss, reason = "a picture's bands")]
                let (from, to) = (i as f32 / count, (i + 1) as f32 / count);
                let band = match self.across {
                    Across::Rows => Rect::from_x_y_ranges(
                        rect.x_range(),
                        rect.top() + rect.height() * from..=rect.top() + rect.height() * to,
                    ),
                    Across::Columns => Rect::from_x_y_ranges(
                        rect.left() + rect.width() * from..=rect.left() + rect.width() * to,
                        rect.y_range(),
                    ),
                };

                (band, colour)
            })
            .collect()
    }
}

/// `image`'s size, pixels.
#[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
const fn size(image: &ColorImage) -> Vec2 {
    vec2(image.size[0] as f32, image.size[1] as f32)
}

/// The least and most of `values`; `None` for none.
fn span(values: impl Iterator<Item = usize> + Clone) -> Option<(usize, usize)> {
    Some((values.clone().min()?, values.max()?))
}

/// The mean of `colours`, each channel apart.
fn mean(colours: impl Iterator<Item = Color32>) -> Color32 {
    let (sum, n) = colours.fold(([0_u32; 4], 0_u32), |(sum, n), c| {
        let [r, g, b, a] = c.to_srgba_unmultiplied().map(u32::from);

        ([sum[0] + r, sum[1] + g, sum[2] + b, sum[3] + a], n + 1)
    });
    let channel = |v: u32| u8::try_from((v + n / 2) / n.max(1)).unwrap_or(u8::MAX);

    Color32::from_rgba_unmultiplied(
        channel(sum[0]),
        channel(sum[1]),
        channel(sum[2]),
        channel(sum[3]),
    )
}

#[cfg(test)]
#[allow(clippy::float_cmp, reason = "whole pixels, exact in f32")]
mod tests {
    use super::*;

    const RED: Color32 = Color32::from_rgb(200, 0, 0);
    const DARK: Color32 = Color32::from_rgb(100, 0, 0);
    const GOLD: Color32 = Color32::from_rgb(200, 160, 60);

    fn image(w: usize, pixels: &[Color32]) -> ColorImage {
        ColorImage::new([w, pixels.len() / w], pixels.to_vec())
    }

    #[test]
    fn a_picture_of_one_colour_a_column_is_stripes() {
        let edge = image(2, &[RED, GOLD, RED, GOLD, RED, GOLD]);
        let stripes = Stripes::exact(&edge).expect("columns");

        assert_eq!(stripes.across, Across::Columns);
        assert_eq!(stripes.colours, [RED, GOLD]);
        assert_eq!(Stripes::exact(&image(2, &[RED, GOLD, GOLD, RED])), None);
    }

    /// A 3 × 2 fill, dithered in its top row: the row's mean.
    #[test]
    fn a_bars_rows_are_their_mean_colours() {
        let fill = image(3, &[RED, DARK, RED, DARK, DARK, DARK]);
        let stripes = Stripes::rows(&fill, None).expect("rows");

        assert_eq!(stripes.colours[0], Color32::from_rgb(167, 0, 0));
        assert_eq!(stripes.colours[1], DARK);
        assert_eq!(stripes.rect.size(), vec2(3.0, 2.0));
    }

    /// The frame both pictures share stays the track's: only the middle
    /// column differs.
    #[test]
    fn a_fill_keeps_to_where_its_track_differs() {
        let track = image(3, &[GOLD, DARK, GOLD, GOLD, DARK, GOLD]);
        let fill = image(3, &[GOLD, RED, GOLD, GOLD, RED, GOLD]);
        let stripes = Stripes::rows(&fill, Some(&track)).expect("rows");

        assert_eq!(
            stripes.rect,
            Rect::from_min_max(pos2(1.0, 0.0), pos2(2.0, 2.0))
        );
        assert_eq!(stripes.colours, [RED, RED]);
        assert_eq!(Stripes::rows(&track, Some(&track)), None);
    }

    /// A 2-row picture laid over a rectangle twice its size: each band
    /// twice as tall.
    #[test]
    fn bands_scale_with_the_rectangle() {
        let stripes = Stripes::rows(&image(1, &[RED, DARK]), None).expect("rows");
        let bands = stripes.bands(Rect::from_min_size(pos2(10.0, 10.0), vec2(4.0, 4.0)));

        assert_eq!(
            bands[0].0,
            Rect::from_min_size(pos2(10.0, 10.0), vec2(4.0, 2.0))
        );
        assert_eq!(
            bands[1].0,
            Rect::from_min_size(pos2(10.0, 12.0), vec2(4.0, 2.0))
        );
    }
}
