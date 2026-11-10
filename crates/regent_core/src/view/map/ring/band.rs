//! A ring drawn as a shape, not as its art: openregent's `[enhancements]
//! rings = "native"`. A departure, so **Inferred** throughout: fitted to
//! the slot's own tile, so it keeps the art's size and place.
//!
//! ```text
//!  weights   a glow pixel its raise over the strongest (251..254: 9/21
//!            .. 21/21); a mode 8 pixel its alpha (raster::blend)
//!  ellipse   weighted least squares, A x² + C y² + D x + E y = 1: the
//!            band's middle line, centre and semi-axes
//!  half      covered pixels over the middle line's length, halved
//!  profile   t = |distance from the middle line| / half
//!
//!   peak ┤────┐
//!        │    └──┐                 core, knee, edge: Look's Profile
//!   edge ┤       └────┐
//!      0 ┼─────────────┴──> t
//!        0   core knee  1
//! ```
//!
//! The three images share one middle line a slot from slot 3 up; each
//! has its own width and fall-off. Against the art, every slot of
//! `AVn1`, `AVn8` and `AVn9`: covered pixels overlap by 86 % or more,
//! weights differ by under a fifth of the peak on average
//! (`a_native_ring_covers_its_art`).

use std::f32::consts::PI;

use crate::data::payload::Rgb;
use crate::data::tile::Mode;
use crate::view::raster::blend;
use crate::view::raster::shade::{self, Shade};
use crate::view::sprite::{Colour, Sprite};

use super::Look;

/// What a native ring does to what is under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// Lightens it, as `AVn1`'s raises.
    Glow,
    /// Covers it with this colour, as `AVn8`'s and `AVn9`'s mode 8 pixels.
    Colour(Rgb),
}

/// How a band's weight falls off across it: shares of its half width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// Full weight out to here.
    pub core: f32,
    /// The fall-off ends here, at [`Profile::edge`].
    pub knee: f32,
    /// The weight's share of the peak from the knee to the edge.
    pub edge: f32,
}

impl Profile {
    /// The share of the peak at `t`, a distance from the middle line in
    /// half widths: 0 past 1.
    #[must_use]
    pub fn at(self, t: f32) -> f32 {
        let t = t.abs();

        if t >= 1.0 {
            0.0
        } else if t <= self.core {
            1.0
        } else if t >= self.knee {
            self.edge
        } else {
            1.0 + (self.edge - 1.0) * (t - self.core) / (self.knee - self.core)
        }
    }
}

impl Look {
    /// The fall-off fitted to this look's art; `None` for the flag's
    /// brackets, which are no ring.
    #[must_use]
    pub const fn profile(self) -> Option<Profile> {
        match self {
            Look::Selected => Some(Profile {
                core: 0.45,
                knee: 0.75,
                edge: 0.45,
            }),
            Look::Fits => Some(Profile {
                core: 0.35,
                knee: 0.85,
                edge: 0.15,
            }),
            Look::Blocked => Some(Profile {
                core: 0.35,
                knee: 0.9,
                edge: 0.2,
            }),
            Look::Flag => None,
        }
    }
}

/// A ring's band: an elliptic strip, art pixels, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// The middle line's centre: about the ring tile's anchor from
    /// [`Band::fit`], the unit's from [`super::Ring::band`].
    pub centre: (f32, f32),
    /// The middle line's semi-axes.
    pub radii: (f32, f32),
    /// Half the band's width.
    pub half: f32,
    pub paint: Paint,
    /// The strongest weight: a glow's share of the strongest raise, a
    /// colour's alpha.
    pub peak: f32,
    pub profile: Profile,
}

impl Band {
    /// The band fitted to `sprite`, a `look` ring's tile. `None` for the
    /// flag's brackets, or a tile with nothing to fit.
    #[must_use]
    pub fn fit(sprite: &Sprite, look: Look) -> Option<Band> {
        let profile = look.profile()?;
        let pixels = weights(sprite);
        let (centre, radii) = ellipse(&pixels)?;
        let (_, _, peak, colour) = pixels.iter().copied().max_by(|a, b| a.2.total_cmp(&b.2))?;
        let paint = match sprite.mode {
            Mode::Other(8) => Paint::Colour(colour),
            _ => Paint::Glow,
        };

        #[allow(clippy::cast_precision_loss, reason = "a tile's pixels")]
        let half = pixels.len() as f32 / perimeter(radii) / 2.0;

        Some(Band {
            centre,
            radii,
            half,
            paint,
            peak,
            profile,
        })
    }

    /// How far `at` is from the middle line, pixels about the anchor
    /// [`Band::centre`] is: the ellipse's level over its gradient, first
    /// order.
    #[must_use]
    pub fn distance(&self, at: (f32, f32)) -> f32 {
        let (rx, ry) = self.radii;
        let (px, py) = (at.0 - self.centre.0, at.1 - self.centre.1);
        let level = (px / rx).hypot(py / ry);
        let slope = (px / (rx * rx)).hypot(py / (ry * ry));

        (level - 1.0) * level / slope.max(f32::EPSILON)
    }

    /// The band's weight at `(x, y)`, as [`weights`] reads the art's.
    #[must_use]
    pub fn weight(&self, at: (f32, f32)) -> f32 {
        self.peak * self.profile.at(self.distance(at) / self.half)
    }
}

/// The strongest raise, `AVn1`'s 254: what a glow's weight is a share of.
const STRONGEST: u32 = 21;

/// Each drawn pixel of a ring tile: its middle about the tile's anchor,
/// its weight, its colour. A glow's weight is its raise over
/// `STRONGEST`; a mode 8 pixel's its alpha.
#[must_use]
pub fn weights(sprite: &Sprite) -> Vec<(f32, f32, f32, Rgb)> {
    let Colour::Indexed { indices, palette } = &sprite.colour else {
        return Vec::new();
    };
    let alphas = blend::alphas(sprite.mode, sprite.level, palette);
    let width = usize::from(sprite.width);
    let (ax, ay) = (f32::from(sprite.anchor.0), f32::from(sprite.anchor.1));

    #[allow(clippy::cast_precision_loss, reason = "a tile's pixels")]
    let at = |i: usize| ((i % width) as f32 + 0.5 - ax, (i / width) as f32 + 0.5 - ay);

    #[allow(clippy::cast_precision_loss, reason = "steps of 32")]
    let weight = |index: u8| match (&alphas, shade::by_mode(sprite.mode, index)) {
        (Some(alphas), _) => f32::from(alphas[usize::from(index)]) / f32::from(u8::MAX),
        (None, Some(Shade::Raise(steps))) => steps as f32 / STRONGEST as f32,
        (None, _) => 0.0,
    };

    indices
        .iter()
        .enumerate()
        .filter(|(i, _)| sprite.covered[*i])
        .map(|(i, index)| {
            let (x, y) = at(i);

            (x, y, weight(*index), palette.colours[usize::from(*index)])
        })
        .filter(|(.., w, _)| *w > 0.0)
        .collect()
}

/// The axis-aligned ellipse through weighted points, least squares on
/// `A x² + C y² + D x + E y = 1`: its centre and semi-axes.
fn ellipse(pixels: &[(f32, f32, f32, Rgb)]) -> Option<((f32, f32), (f32, f32))> {
    // The normal equations, in f64: x⁴ of 400 pixels sums past f32.
    let mut normal = [[0.0f64; 4]; 4];
    let mut rhs = [0.0f64; 4];

    for (x, y, w, _) in pixels {
        let (x, y, w) = (f64::from(*x), f64::from(*y), f64::from(*w));
        let row = [x * x, y * y, x, y];

        for (i, ri) in row.iter().enumerate() {
            rhs[i] += w * ri;

            for (j, rj) in row.iter().enumerate() {
                normal[i][j] += w * ri * rj;
            }
        }
    }

    let [a, c, d, e] = solve(normal, rhs)?;
    let (cx, cy) = (-d / (2.0 * a), -e / (2.0 * c));
    let level = 1.0 + d * d / (4.0 * a) + e * e / (4.0 * c);
    let radii = ((level / a).sqrt(), (level / c).sqrt());

    #[allow(clippy::cast_possible_truncation, reason = "art pixels")]
    let narrow = |v: f64| v as f32;

    (radii.0.is_finite() && radii.1.is_finite())
        .then(|| ((narrow(cx), narrow(cy)), (narrow(radii.0), narrow(radii.1))))
}

/// `m x = v` by Gaussian elimination with partial pivoting; `None` if
/// singular.
fn solve(mut m: [[f64; 4]; 4], mut v: [f64; 4]) -> Option<[f64; 4]> {
    for col in 0..4 {
        let pivot = (col..4).max_by(|a, b| m[*a][col].abs().total_cmp(&m[*b][col].abs()))?;

        if m[pivot][col].abs() < f64::EPSILON {
            return None;
        }

        m.swap(col, pivot);
        v.swap(col, pivot);

        for row in col + 1..4 {
            let f = m[row][col] / m[col][col];
            let top = m[col];

            for (cell, above) in m[row].iter_mut().zip(top).skip(col) {
                *cell -= f * above;
            }

            v[row] -= f * v[col];
        }
    }

    let mut x = [0.0; 4];

    for row in (0..4).rev() {
        let rest: f64 = (row + 1..4).map(|k| m[row][k] * x[k]).sum();

        x[row] = (v[row] - rest) / m[row][row];
    }

    Some(x)
}

/// An ellipse's length, Ramanujan's first approximation.
fn perimeter((a, b): (f32, f32)) -> f32 {
    PI * (3.0 * (a + b) - ((3.0 * a + b) * (a + 3.0 * b)).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_is_flat_falls_and_stops() {
        let p = Profile {
            core: 0.4,
            knee: 0.8,
            edge: 0.5,
        };

        assert!((p.at(0.0) - 1.0).abs() < 1e-6);
        assert!((p.at(-0.4) - 1.0).abs() < 1e-6);
        assert!((p.at(0.6) - 0.75).abs() < 1e-6);
        assert!((p.at(0.9) - 0.5).abs() < 1e-6);
        assert!(p.at(1.0).abs() < 1e-6);
    }

    /// Points on a circle of radius 10 about (3, -2) fit it.
    #[test]
    fn the_least_squares_ellipse_finds_a_circle() {
        let points: Vec<_> = (0..64)
            .map(|n| {
                #[allow(clippy::cast_precision_loss, reason = "a small count")]
                let angle = n as f32 * PI / 32.0;

                (
                    3.0 + 10.0 * angle.cos(),
                    -2.0 + 10.0 * angle.sin(),
                    1.0,
                    Rgb { r: 0, g: 0, b: 0 },
                )
            })
            .collect();
        let ((cx, cy), (a, b)) = ellipse(&points).expect("an ellipse");

        assert!((cx - 3.0).abs() < 1e-3 && (cy + 2.0).abs() < 1e-3);
        assert!((a - 10.0).abs() < 1e-3 && (b - 10.0).abs() < 1e-3);
    }

    #[test]
    fn a_point_on_the_middle_line_is_at_distance_0() {
        let band = Band {
            centre: (1.0, 1.0),
            radii: (20.0, 10.0),
            half: 3.0,
            paint: Paint::Glow,
            peak: 1.0,
            profile: Look::Selected.profile().expect("a ring"),
        };

        assert!(band.distance((21.0, 1.0)).abs() < 1e-4);
        assert!((band.distance((24.0, 1.0)) - 3.0).abs() < 1e-3);
        assert!((band.distance((1.0, -11.0)) - 2.0).abs() < 1e-3);
        assert!((band.weight((1.0, 11.0)) - 1.0).abs() < 1e-4);
    }
}
