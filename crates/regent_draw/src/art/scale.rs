//! xBRZ, an enhancement (`[enhancements] ui_upscale`): the factor a
//! picture takes for its size on screen, and the upscale itself.

use bevy_egui::egui::{Color32, ColorImage, Vec2};

/// The factors xBRZ is run at.
pub const FACTORS: [usize; 3] = [2, 3, 4];

/// Source pixels of padding each side before xBRZ, more than its
/// kernel's reach ([`REACH`]), cut off after: tiles meet seamlessly, and an
/// opaque picture's edge stays opaque.
const PAD: usize = 4;

/// Source pixels from its edge a picture's upscale reads its padding:
/// xBRZ's kernel's reach.
pub const REACH: usize = 2;

/// The most pixels an upscaled picture may have: a larger one takes a
/// smaller factor, or none. Bounds a task's memory.
const MOST: usize = 4096 * 4096;

/// A scale this far over a factor still takes it: rounding to whole
/// pixels makes 2× read as 2.01×.
pub const SLACK: f32 = 0.02;

/// How a picture's edges continue: as its edge pixels, or wrapped round
/// (a tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Clamp,
    Repeat,
}

impl Edge {
    pub const fn name(self) -> &'static str {
        match self {
            Edge::Clamp => "clamp",
            Edge::Repeat => "repeat",
        }
    }
}

/// The factor for a `size` picture shown at `scale` (window pixels a
/// pixel, each way; the larger counts): the least of [`FACTORS`] at or
/// over it, else the largest; one that keeps within `MOST`. `None` at
/// scale 1 or less, for an empty picture, or where none fits.
///
/// ```text
///  scale  ≤1    1.5   2    2.5   3.2   6
///  factor none  2     2    3     4     4
/// ```
#[must_use]
pub fn factor(size: [usize; 2], scale: Vec2) -> Option<usize> {
    let scale = scale.max_elem();
    #[allow(clippy::cast_precision_loss, reason = "a factor")]
    let wanted = FACTORS
        .into_iter()
        .find(|&f| f as f32 + SLACK >= scale)
        .unwrap_or(FACTORS[FACTORS.len() - 1]);
    let pixels = size[0] * size[1];
    let fits = |f: usize| pixels > 0 && pixels * f * f <= MOST;

    if scale > 1.0 + SLACK {
        FACTORS.into_iter().rev().find(|&f| f <= wanted && fits(f))
    } else {
        None
    }
}

/// `pixels` kept within `most` a side, the shape kept: what a texture
/// may be. A larger picture is drawn stretched from it.
#[must_use]
pub fn capped(pixels: [usize; 2], most: usize) -> [usize; 2] {
    let side = pixels[0].max(pixels[1]);

    if side > most {
        pixels.map(|n| (n * most).div_ceil(side).min(most))
    } else {
        pixels
    }
}

/// `image` upscaled `factor` times by xBRZ, its edges continued as
/// `edge` says.
///
/// xBRZ takes straight alpha and weighs each colour by its alpha, so a
/// see-through pixel's colour never darkens an edge; colour and alpha
/// are upscaled together.
#[must_use]
pub fn xbrz(image: &ColorImage, factor: usize, edge: Edge) -> ColorImage {
    let padded = padded(image, edge);
    let [w, h] = padded.size;
    let rgba: Vec<u8> = padded
        .pixels
        .iter()
        .flat_map(Color32::to_srgba_unmultiplied)
        .collect();
    let big = xbrz::scale_rgba(&rgba, w, h, factor);
    let pixels: Vec<Color32> = big
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
        .collect();
    let big = ColorImage::new([w * factor, h * factor], pixels);
    let [iw, ih] = image.size;

    cut(&big, [PAD * factor; 2], [iw * factor, ih * factor])
}

/// `image` with [`PAD`] pixels round it, continued as `edge` says.
fn padded(image: &ColorImage, edge: Edge) -> ColorImage {
    let [w, h] = image.size;
    let at = |i: usize, n: usize| {
        let i = i.cast_signed() - PAD.cast_signed();
        let n = n.cast_signed();

        match edge {
            Edge::Clamp => i.clamp(0, n - 1),
            Edge::Repeat => i.rem_euclid(n),
        }
        .cast_unsigned()
    };
    let size = [w + 2 * PAD, h + 2 * PAD];
    let pixels = (0..size[1])
        .flat_map(|y| (0..size[0]).map(move |x| image.pixels[at(y, h) * w + at(x, w)]))
        .collect();

    ColorImage::new(size, pixels)
}

/// The `size` part of `image` from `corner`.
#[must_use]
pub fn cut(image: &ColorImage, corner: [usize; 2], size: [usize; 2]) -> ColorImage {
    let w = image.size[0];
    let pixels = (0..size[1])
        .flat_map(|y| {
            let row = (corner[1] + y) * w + corner[0];

            image.pixels[row..row + size[0]].iter().copied()
        })
        .collect();

    ColorImage::new(size, pixels)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::art::shrink;

    const RED: Color32 = Color32::from_rgb(200, 30, 30);

    /// A small picture with a diagonal and a step, so xBRZ blends.
    pub(crate) fn sample(w: usize, h: usize) -> ColorImage {
        let pixels = (0..h)
            .flat_map(|y| {
                (0..w).map(move |x| match (x + y) % 5 {
                    0 | 1 => RED,
                    2 => Color32::from_rgb(20, 120, 220),
                    _ => Color32::from_rgb(240, 220, 90),
                })
            })
            .collect();

        ColorImage::new([w, h], pixels)
    }

    #[test]
    fn the_factor_is_the_least_covering_the_scale() {
        let at = |scale: f32| factor([100, 100], Vec2::splat(scale));

        assert_eq!(at(0.8), None);
        assert_eq!(at(1.0), None);
        assert_eq!(at(1.5), Some(2));
        assert_eq!(at(2.0), Some(2));
        assert_eq!(at(2.01), Some(2));
        assert_eq!(at(2.5), Some(3));
        assert_eq!(at(3.2), Some(4));
        assert_eq!(at(6.0), Some(4));
    }

    /// An empty picture, as some sprites' canvases, takes none: `xbrz`
    /// pads it from pixels it has not.
    #[test]
    fn an_empty_picture_takes_no_factor() {
        assert_eq!(factor([0, 5], Vec2::splat(3.0)), None);
        assert_eq!(factor([5, 0], Vec2::splat(f32::INFINITY)), None);
    }

    /// The wider axis decides: a stretched piece.
    #[test]
    fn the_factor_takes_the_larger_axis() {
        assert_eq!(factor([100, 10], Vec2::new(1.5, 3.5)), Some(4));
    }

    /// egui's 2048 a side: 2133 × 1600 is cut to 2048 × 1537.
    #[test]
    fn a_texture_keeps_within_the_gpus_side() {
        assert_eq!(capped([2133, 1600], 2048), [2048, 1537]);
        assert_eq!(capped([300, 200], 2048), [300, 200]);
    }

    /// A 768-wide picture, as the menus' room, at 4 window pixels a page
    /// pixel (a 3200-wide window): capped
    /// at egui's 2048 it takes 3× and is stretched; at a GPU's 8192, 4×
    /// at its full 3072.
    #[test]
    fn the_cap_decides_a_large_pictures_factor() {
        let room = [768, 480];
        let shown = |most: usize| {
            let pixels = capped(room.map(|n| n * 4), most);
            #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
            let scale = Vec2::new(pixels[0] as f32 / 768.0, pixels[1] as f32 / 480.0);

            (pixels, factor(room, scale))
        };

        assert_eq!(shown(2048), ([2048, 1280], Some(3)));
        assert_eq!(shown(8192), ([3072, 1920], Some(4)));
    }

    #[test]
    fn a_large_picture_takes_a_smaller_factor_or_none() {
        // 4× of 1500 is over 4096 a side; 2× fits.
        assert_eq!(factor([1500, 1500], Vec2::splat(5000.0 / 1500.0)), Some(2));
        assert_eq!(factor([3000, 3000], Vec2::splat(5000.0 / 3000.0)), None);
    }

    /// A tile upscaled alone, laid twice side by side and twice down,
    /// matches the 2 × 2 tiling upscaled whole: no seam where tiles meet.
    #[test]
    fn upscaled_tiles_meet_without_a_seam() {
        let tile = sample(7, 6);
        let [w, h] = tile.size;
        let tiling = ColorImage::new(
            [w * 2, h * 2],
            (0..h * 2)
                .flat_map(|y| (0..w * 2).map(move |x| (x % w, y % h)))
                .map(|(x, y)| tile.pixels[y * w + x])
                .collect(),
        );

        for f in FACTORS {
            let one = xbrz(&tile, f, Edge::Repeat);
            let whole = xbrz(&tiling, f, Edge::Repeat);

            for (x, y) in [(0, 0), (w, 0), (0, h), (w, h)] {
                assert_eq!(
                    cut(&whole, [x * f, y * f], one.size),
                    one,
                    "{f}× at {x},{y}"
                );
            }
        }
    }

    /// An opaque picture stays opaque to its edges: no rounded corners
    /// from a see-through outside.
    #[test]
    fn an_opaque_pictures_edges_stay_opaque() {
        let big = xbrz(&sample(6, 5), 3, Edge::Clamp);

        assert_eq!(big.size, [18, 15]);
        assert!(big.pixels.iter().all(|p| p.a() == u8::MAX));
    }

    /// A red disc on see-through black: every pixel xBRZ makes at its edge,
    /// and the shrink after, is red, part see-through, not darkened by the
    /// black.
    #[test]
    fn a_binary_alpha_edge_gets_no_dark_fringe() {
        let side: usize = 9;
        let pixels = (0..side * side)
            .map(|i| {
                let (x, y) = ((i % side).cast_signed() - 4, (i / side).cast_signed() - 4);

                if x * x + y * y <= 10 {
                    RED
                } else {
                    Color32::TRANSPARENT
                }
            })
            .collect();
        let disc = ColorImage::new([side, side], pixels);

        for f in FACTORS {
            let big = xbrz(&disc, f, Edge::Clamp);
            let edge = big.pixels.iter().filter(|p| p.a() > 0 && p.a() < u8::MAX);
            // Premultiplied, as egui keeps it: red times the pixel's alpha.
            let red = |p: &Color32| {
                let share = f32::from(p.a()) / 255.0;
                let near =
                    |have: u8, full: u8| (f32::from(have) - f32::from(full) * share).abs() <= 2.0;

                near(p.r(), RED.r()) && near(p.g(), RED.g()) && near(p.b(), RED.b())
            };

            assert!(edge.count() > 0, "{f}×: no blended edge");
            assert!(big.pixels.iter().all(red), "{f}×: {:?}", big.pixels);

            // Shrunk to the screen, by premultiplied means: still no fringe.
            let shown = shrink::shrink(&big, [side * 3 / 2; 2]);

            assert!(
                shown.pixels.iter().all(red),
                "{f}× shrunk: {:?}",
                shown.pixels
            );
        }
    }
}
