//! A picture shrunk by area: each new pixel the mean of the source area
//! it covers, a part-covered pixel by its share. No aliasing, where the
//! GPU's linear sampling of a larger picture skips pixels.
//!
//! ```text
//!  100 px  |a|b|c|d|e|...      to 40 px: each new pixel 2.5 old,
//!  40 px   | a+b+c/2 | c/2+d+e |   ...   the shares divided by 2.5
//! ```

use bevy_egui::egui::{Color32, ColorImage, Pos2, Rect, vec2};

/// One new pixel's taps: source index and share, the shares summing to 1.
type Taps = Vec<(usize, f32)>;

/// Each of `to` new pixels' taps along a row or column of `from`, over
/// its part `start..end` (source pixels, fractional).
fn taps(from: usize, (start, end): (f32, f32), to: usize) -> Vec<Taps> {
    #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
    let step = (end - start) / to as f32;

    (0..to)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
            let (start, end) = (start + i as f32 * step, start + (i + 1) as f32 * step);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "inside the picture"
            )]
            let span = start.max(0.0).floor() as usize..(end.ceil() as usize).min(from);

            span.map(|j| {
                #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
                let cover = ((j + 1) as f32).min(end) - (j as f32).max(start);

                (j, cover / step)
            })
            .collect()
        })
        .collect()
}

/// The weighted mean of `pixels`, premultiplied as egui keeps them.
fn mix(pixels: impl Iterator<Item = ([f32; 4], f32)>) -> [f32; 4] {
    pixels.fold([0.0; 4], |sum, (p, share)| {
        [0, 1, 2, 3].map(|c| sum[c] + p[c] * share)
    })
}

/// `image` shrunk to `size`; the same picture where `size` is no smaller
/// either way.
#[must_use]
pub fn shrink(image: &ColorImage, size: [usize; 2]) -> ColorImage {
    part(image, whole(image), size)
}

/// All of `image`, its pixels.
#[must_use]
pub fn whole(image: &ColorImage) -> Rect {
    #[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
    let size = vec2(image.size[0] as f32, image.size[1] as f32);

    Rect::from_min_size(Pos2::ZERO, size)
}

/// Whether [`part`] gives `image` as it is: `cut` whole and `size` no
/// smaller either way, or empty.
#[must_use]
pub fn kept(image: &ColorImage, cut: Rect, size: [usize; 2]) -> bool {
    let larger = size[0] >= image.size[0] && size[1] >= image.size[1];

    (larger && cut == whole(image)) || size.contains(&0)
}

/// `image`'s part `cut` (its pixels, fractional) resampled by area to
/// `size`: a picture laid on another's pixels (`art::Snap::Over`). The
/// same picture where `cut` is whole and `size` no smaller either way.
#[must_use]
pub fn part(image: &ColorImage, cut: Rect, size: [usize; 2]) -> ColorImage {
    let [wide, high] = image.size;

    if kept(image, cut, size) {
        image.clone()
    } else {
        let across = taps(wide, (cut.min.x, cut.max.x), size[0]);
        let down = taps(high, (cut.min.y, cut.max.y), size[1]);
        // Read in place, not copied as floats: an upscaled picture is large.
        let source = |i: usize| image.pixels[i].to_array().map(f32::from);

        // Each row narrowed, then each column shortened.
        let rows: Vec<[f32; 4]> = (0..high)
            .flat_map(|y| {
                across
                    .iter()
                    .map(move |t| mix(t.iter().map(|&(x, s)| (source(y * wide + x), s))))
            })
            .collect();
        let pixels = down
            .iter()
            .flat_map(|t| {
                let rows = &rows;

                (0..size[0]).map(move |x| {
                    let [r, g, b, a] =
                        mix(t.iter().map(|&(y, s)| (rows[y * size[0] + x], s))).map(|c| {
                            #[allow(
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "a channel"
                            )]
                            let c = c.round().clamp(0.0, 255.0) as u8;

                            c
                        });

                    Color32::from_rgba_premultiplied(r, g, b, a)
                })
            })
            .collect();

        ColorImage::new(size, pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_pixel_is_the_mean_of_the_area_it_covers() {
        // A row: black, white, black, white; halved, each pair grey.
        let (black, white) = (Color32::BLACK, Color32::WHITE);
        let image = ColorImage::new([4, 1], vec![black, white, black, white]);
        let half = shrink(&image, [2, 1]);

        assert_eq!(half.pixels, [Color32::from_gray(128); 2]);
    }

    #[test]
    fn a_part_covered_pixel_counts_by_its_share() {
        // Three to two: the middle pixel is half in each.
        let shares = taps(3, (0.0, 3.0), 2);

        assert_eq!(shares[0], [(0, 2.0 / 3.0), (1, 1.0 / 3.0)]);
        assert_eq!(shares[1], [(1, 1.0 / 3.0), (2, 2.0 / 3.0)]);
    }

    #[test]
    fn a_picture_is_never_grown() {
        let image = ColorImage::new([2, 2], vec![Color32::RED; 4]);

        assert_eq!(shrink(&image, [5, 5]).size, [2, 2]);
    }
}
