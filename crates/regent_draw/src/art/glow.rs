//! A hovered painted quest's look, a departure: the original draws its
//! `IX33` lit picture, the map under a warm glow in a palette of its own
//! (`docs/formats/panels.md`). Here the map's own pixels take one colour table,
//! faded in from an ellipse's edge, so the look holds at any zoom and
//! upscaled level.
//!
//! ```text
//!  out = map + w · (T(map) − map)
//!  T   one 17³ RGB table, trilinear, fitted to the 31 lit pictures
//!  w   Φ((d − 16) / 8), d map px in from the edge of an ellipse filling
//!      0.9 of the quest's lit rectangle: 0 at the edge, ½ at 16, 1 from ~32
//! ```
//!
//! The table is fitted on the lit pictures' full-strength pixels (w over
//! [`FULL`] in their own opaque shape), smoothed by its second differences.

/// Nodes a side of the colour table.
pub const SIDE: usize = 17;

/// Map pixels in from the mask's edge where the look is half on.
pub const HALF_IN: f64 = 16.0;

/// The fade's spread, map pixels: Φ's sigma.
pub const SPREAD: f64 = 8.0;

/// The share of a quest's lit rectangle the ellipse fills, each axis.
pub const ELLIPSE: f64 = 0.9;

/// The fit's smoothing weight, per sample a node.
pub const SMOOTHING: f64 = 0.3;

/// The least weight a pixel the fit takes has: full strength.
pub const FULL: f64 = 0.97;

/// The fit's stop: residual over right-hand side.
const TOLERANCE: f64 = 1e-10;

/// The fit's most iterations.
const MOST_STEPS: usize = 20_000;

/// A channel's top.
const TOP: f64 = 255.0;

/// Nodes in the table.
const NODES: usize = SIDE * SIDE * SIDE;

/// Neighbours a node's data can tie it to: offsets −1 to 1 each axis.
const STENCIL: usize = 27;

/// RGBA pixels, rows first.
#[derive(Clone, Copy)]
pub struct Picture<'a> {
    pub size: [usize; 2],
    pub rgba: &'a [u8],
}

impl Picture<'_> {
    fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.size[0] + x) * 4;

        [0, 1, 2, 3].map(|c| self.rgba[i + c])
    }
}

/// Level pixels a mask is made on: its corner and size, level pixels, and
/// level pixels a map pixel.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub at: [usize; 2],
    pub size: [usize; 2],
    pub scale: f64,
}

/// The colour table: each node's colour, 0 to 1, red slowest.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    nodes: Vec<[f32; 3]>,
}

/// A node's colour on the identity, 0 to 1.
#[allow(clippy::cast_precision_loss, reason = "a node index")]
fn place(node: usize) -> [f64; 3] {
    let at = [node / (SIDE * SIDE), node / SIDE % SIDE, node % SIDE];

    at.map(|n| n as f64 / (SIDE - 1) as f64)
}

/// A colour's cell, low corner, and where in it, each axis.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "within the table"
)]
fn cell(rgb: [f64; 3]) -> ([usize; 3], [f64; 3]) {
    let top = (SIDE - 1) as f64;
    let g = rgb.map(|c| c.clamp(0.0, 1.0) * top);
    let low = g.map(|c| (c.floor() as usize).min(SIDE - 2));
    let part = [0, 1, 2].map(|i| g[i] - low[i] as f64);

    (low, part)
}

/// A colour's eight nodes and their trilinear shares.
fn corners(rgb: [f64; 3]) -> [(usize, f64); 8] {
    let (low, part) = cell(rgb);

    std::array::from_fn(|k| {
        let up = [k >> 2 & 1, k >> 1 & 1, k & 1];
        let node = ((low[0] + up[0]) * SIDE + low[1] + up[1]) * SIDE + low[2] + up[2];
        let share = (0..3)
            .map(|i| if up[i] == 1 { part[i] } else { 1.0 - part[i] })
            .product();

        (node, share)
    })
}

/// The stencil slot from node `a` to node `b`, 1 apart at most each axis.
fn slot(a: usize, b: usize) -> usize {
    let off = |d: usize| b / d % SIDE + 1 - a / d % SIDE;

    off(SIDE * SIDE) * 9 + off(SIDE) * 3 + off(1)
}

/// The node `slot` of [`STENCIL`] leads to from `node`, if in the table.
fn neighbour(node: usize, slot: usize) -> Option<usize> {
    let at = [node / (SIDE * SIDE), node / SIDE % SIDE, node % SIDE];
    let off = [slot / 9, slot / 3 % 3, slot % 3];
    let moved: Option<Vec<usize>> = (0..3)
        .map(|i| (at[i] + off[i]).checked_sub(1).filter(|&n| n < SIDE))
        .collect();

    moved.map(|m| (m[0] * SIDE + m[1]) * SIDE + m[2])
}

/// The node triples along each axis the smoothing takes second
/// differences over.
fn triples() -> Vec<[usize; 3]> {
    let strides = [SIDE * SIDE, SIDE, 1];

    strides
        .iter()
        .flat_map(|&stride| {
            (0..NODES)
                .filter(move |&n| n / stride % SIDE < SIDE - 2)
                .map(move |n| [n, n + stride, n + 2 * stride])
        })
        .collect()
}

/// The fit's normal equations: data and smoothing.
struct Normal {
    /// Each node's data ties, by [`STENCIL`] slot.
    data: Vec<[f64; STENCIL]>,
    /// Each node's ties as other nodes and weights, the zero ones left out.
    ties: Vec<Vec<(usize, f64)>>,
    triples: Vec<[usize; 3]>,
    smoothing: f64,
}

impl Normal {
    fn new(data: Vec<[f64; STENCIL]>, smoothing: f64) -> Normal {
        let ties = (0..NODES)
            .map(|n| {
                (0..STENCIL)
                    .filter(|&s| data[n][s] != 0.0)
                    .filter_map(|s| neighbour(n, s).map(|m| (m, data[n][s])))
                    .collect()
            })
            .collect();

        Normal {
            data,
            ties,
            triples: triples(),
            smoothing,
        }
    }

    /// The matrix times `x`.
    fn times(&self, x: &[f64]) -> Vec<f64> {
        let mut out: Vec<f64> = self
            .ties
            .iter()
            .map(|t| t.iter().map(|&(m, v)| v * x[m]).sum())
            .collect();

        for &[a, b, c] in &self.triples {
            let bend = self.smoothing * (x[a] - 2.0 * x[b] + x[c]);

            out[a] += bend;
            out[b] -= 2.0 * bend;
            out[c] += bend;
        }

        out
    }

    /// The matrix's diagonal.
    fn diagonal(&self) -> Vec<f64> {
        let mut out: Vec<f64> = self.data.iter().map(|d| d[STENCIL / 2]).collect();

        for &[a, b, c] in &self.triples {
            out[a] += self.smoothing;
            out[b] += 4.0 * self.smoothing;
            out[c] += self.smoothing;
        }

        out
    }

    /// `x` solving the equations for `rhs`, by conjugate gradients with
    /// the diagonal as preconditioner.
    fn solve(&self, rhs: &[f64]) -> Vec<f64> {
        let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(p, q)| p * q).sum::<f64>();
        let inverse: Vec<f64> = self.diagonal().iter().map(|d| 1.0 / d).collect();
        let precondition = |r: &[f64]| r.iter().zip(&inverse).map(|(r, i)| r * i).collect();
        let goal = TOLERANCE * dot(rhs, rhs).sqrt();
        let mut x = vec![0.0; NODES];
        let mut r = rhs.to_vec();
        let mut z: Vec<f64> = precondition(&r);
        let mut p = z.clone();
        let mut rz = dot(&r, &z);

        for _ in 0..MOST_STEPS {
            if dot(&r, &r).sqrt() <= goal {
                break;
            }

            let ap = self.times(&p);
            let step = rz / dot(&p, &ap);

            (0..NODES).for_each(|i| {
                x[i] += step * p[i];
                r[i] -= step * ap[i];
            });
            z = precondition(&r);

            let next = dot(&r, &z);

            p = (0..NODES).map(|i| z[i] + next / rz * p[i]).collect();
            rz = next;
        }

        x
    }
}

impl Table {
    /// The table that changes nothing.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "0 to 1")]
    pub fn identity() -> Table {
        Table {
            nodes: (0..NODES).map(|n| place(n).map(|c| c as f32)).collect(),
        }
    }

    /// The table fitted to `lits`, each a lit picture and its corner on
    /// `map`: least squares on their full-strength pixels, the change from
    /// the identity smoothed by its second differences along each axis.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "pixel counts; 0 to 1"
    )]
    pub fn fit(map: Picture<'_>, lits: &[([usize; 2], Picture<'_>)]) -> Table {
        let pairs: Vec<([f64; 3], [f64; 3])> = lits
            .iter()
            .flat_map(|&(at, lit)| samples(map, at, lit))
            .collect();
        let mut data = vec![[0.0; STENCIL]; NODES];
        let mut rhs = vec![[0.0; 3]; NODES];

        for (src, dst) in &pairs {
            let shares = corners(*src);

            for &(a, wa) in &shares {
                for &(b, wb) in &shares {
                    data[a][slot(a, b)] += wa * wb;
                }

                for c in 0..3 {
                    rhs[a][c] += wa * (dst[c] - src[c]);
                }
            }
        }

        let normal = Normal::new(data, SMOOTHING * pairs.len() as f64 / NODES as f64);
        let change: Vec<Vec<f64>> = (0..3)
            .map(|c| normal.solve(&rhs.iter().map(|r| r[c]).collect::<Vec<_>>()))
            .collect();
        let nodes = (0..NODES)
            .map(|n| {
                let at = place(n);

                [0, 1, 2].map(|c| (at[c] + change[c][n]) as f32)
            })
            .collect();

        Table { nodes }
    }

    /// `rgb`, 0 to 1, through the table, trilinear.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, reason = "0 to 1")]
    pub fn get(&self, rgb: [f32; 3]) -> [f32; 3] {
        corners(rgb.map(f64::from))
            .iter()
            .fold([0.0; 3], |sum, &(n, share)| {
                [0, 1, 2].map(|c| sum[c] + self.nodes[n][c] * share as f32)
            })
    }

    /// `rgb`, 0 to 1, with the look at weight `w`, kept within 0 to 1.
    #[must_use]
    pub fn look(&self, rgb: [f32; 3], w: f32) -> [f32; 3] {
        let to = self.get(rgb);

        [0, 1, 2].map(|c| (rgb[c] + w * (to[c] - rgb[c])).clamp(0.0, 1.0))
    }

    /// A pixel with the look at weight `w`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel"
    )]
    pub fn glow(&self, rgb: [u8; 3], w: f32) -> [u8; 3] {
        let top = TOP as f32;
        let out = self.look(rgb.map(|c| f32::from(c) / top), w);

        out.map(|c| (c * top).round() as u8)
    }

    /// The nodes as little-endian floats, for a cache.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.nodes
            .iter()
            .flatten()
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }

    /// A table from [`Table::bytes`]; `None` for the wrong length.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Table> {
        let floats: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();

        (bytes.len() == NODES * 3 * 4).then(|| Table {
            nodes: floats.as_chunks::<3>().0.to_vec(),
        })
    }
}

/// `lit`'s full-strength pixels and the map's under them, 0 to 1: opaque,
/// and weighted over [`FULL`] in its own opaque shape.
fn samples(map: Picture<'_>, at: [usize; 2], lit: Picture<'_>) -> Vec<([f64; 3], [f64; 3])> {
    let [w, h] = lit.size;
    let mask: Vec<bool> = (0..w * h).map(|i| lit.rgba[i * 4 + 3] > 0).collect();
    let signed = signed(&mask, lit.size);
    let unit = |p: [u8; 4]| [0, 1, 2].map(|c| f64::from(p[c]) / TOP);

    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| mask[y * w + x] && weight(signed[y * w + x]) > FULL)
        .map(|(x, y)| (unit(map.at(at[0] + x, at[1] + y)), unit(lit.at(x, y))))
        .collect()
}

/// The standard normal's distribution, Φ: erf by Abramowitz and Stegun
/// 7.1.26, within 1.5e-7.
#[must_use]
pub fn normal(z: f64) -> f64 {
    const P: f64 = 0.327_591_1;
    const A: [f64; 5] = [
        0.254_829_592,
        -0.284_496_736,
        1.421_413_741,
        -1.453_152_027,
        1.061_405_429,
    ];
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + P * x);
    let poly = A.iter().rev().fold(0.0, |sum, a| sum * t + a) * t;
    let erf = 1.0 - poly * (-x * x).exp();

    f64::midpoint(1.0, erf.copysign(z))
}

/// The look's weight `d` map pixels in from the mask's edge, out where
/// negative: Φ((d − 16) / 8).
///
/// ```text
///  d  0     16   32
///  w  0.02  0.5  0.98
/// ```
#[must_use]
pub fn weight(d: f64) -> f64 {
    normal((d - HALF_IN) / SPREAD)
}

/// Each pixel of `grid` inside the ellipse filling [`ELLIPSE`] of `rect`
/// (corner and size, map pixels), by its centre.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a picture's pixels")]
pub fn ellipse(rect: [f64; 4], grid: Grid) -> Vec<bool> {
    let centre = [0, 1].map(|i| rect[i] + rect[i + 2] / 2.0);
    let half = [0, 1].map(|i| ELLIPSE * rect[i + 2] / 2.0);
    // A pixel's centre in map pixels, from the ellipse's centre, in half axes.
    let off = |n: usize, i: usize| ((n as f64 + 0.5) / grid.scale - centre[i]) / half[i];
    let [width, height] = grid.size;

    (0..height)
        .flat_map(|row| (0..width).map(move |col| (col, row)))
        .map(|(col, row)| {
            let across = off(grid.at[0] + col, 0);
            let down = off(grid.at[1] + row, 1);

            across * across + down * down <= 1.0
        })
        .collect()
}

/// Each pixel's weight on `grid` for `mask`: its signed distance in map
/// pixels (level pixels over the scale).
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "0 to 1")]
pub fn weights(mask: &[bool], grid: Grid) -> Vec<f32> {
    signed(mask, grid.size)
        .iter()
        .map(|d| weight(d / grid.scale) as f32)
        .collect()
}

/// Each pixel's distance from `mask`'s edge, pixels: inside, to the
/// nearest pixel out (past the border counts as out), positive; outside,
/// to the nearest pixel in, negative. None in: −∞ outside.
#[must_use]
pub fn signed(mask: &[bool], size: [usize; 2]) -> Vec<f64> {
    let [w, h] = size;
    let padded = [w + 2, h + 2];
    let out: Vec<bool> = (0..padded[0] * padded[1])
        .map(|i| {
            let (x, y) = (i % padded[0], i / padded[0]);
            let inside = (1..=w).contains(&x) && (1..=h).contains(&y);

            !(inside && mask[(y - 1) * w + x - 1])
        })
        .collect();
    let to_out = distance(&out, padded);
    let to_in = distance(mask, size);

    (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);

            if mask[i] {
                to_out[(y + 1) * padded[0] + x + 1]
            } else {
                -to_in[i]
            }
        })
        .collect()
}

/// Each pixel's Euclidean distance to the nearest `feature` pixel, by
/// Felzenszwalb and Huttenlocher's two passes; ∞ with none.
fn distance(feature: &[bool], size: [usize; 2]) -> Vec<f64> {
    let [w, h] = size;
    let start: Vec<f64> = feature
        .iter()
        .map(|&f| if f { 0.0 } else { f64::INFINITY })
        .collect();
    let mut cols = vec![0.0; w * h];

    for x in 0..w {
        let line: Vec<f64> = (0..h).map(|y| start[y * w + x]).collect();

        for (y, d) in squared(&line).into_iter().enumerate() {
            cols[y * w + x] = d;
        }
    }

    cols.chunks(w.max(1))
        .flat_map(squared)
        .map(f64::sqrt)
        .collect()
}

/// One line's squared distances from its sampled squared distances `f`:
/// the lower envelope of the parabolas at its finite samples.
#[allow(clippy::cast_precision_loss, reason = "a line's pixels")]
fn squared(f: &[f64]) -> Vec<f64> {
    let at: Vec<usize> = (0..f.len()).filter(|&q| f[q].is_finite()).collect();
    let crossing = |p: usize, q: usize| {
        let (pf, qf) = (p as f64, q as f64);

        ((f[q] + qf * qf) - (f[p] + pf * pf)) / (2.0 * (qf - pf))
    };
    // The envelope's parabolas and where each starts.
    let mut hull: Vec<(usize, f64)> = Vec::new();

    for &q in &at {
        while let Some(&(p, from)) = hull.last() {
            if crossing(p, q) <= from {
                hull.pop();
            } else {
                break;
            }
        }

        let from = hull
            .last()
            .map_or(f64::NEG_INFINITY, |&(p, _)| crossing(p, q));

        hull.push((q, from));
    }

    let mut k = 0;

    (0..f.len())
        .map(|x| {
            let xf = x as f64;

            while k + 1 < hull.len() && hull[k + 1].1 <= xf {
                k += 1;
            }

            hull.get(k).map_or(f64::INFINITY, |&(p, _)| {
                let d = xf - p as f64;

                d * d + f[p]
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fade: 0.02 at the edge, half at 16 px in, full from 32.
    #[test]
    fn the_weight_is_half_at_sixteen_pixels_in() {
        assert!((weight(HALF_IN) - 0.5).abs() < 1e-7);
        assert!((weight(0.0) - 0.022_75).abs() < 1e-4);
        assert!(weight(32.0) > 0.977 && weight(32.0) < FULL + 0.01);
        assert!(weight(-8.0) < 0.002);
        assert!((normal(1.0) - 0.841_344_7).abs() < 1e-6);
        assert!((normal(-1.0) + normal(1.0) - 1.0).abs() < 1e-12);
    }

    /// A line's distances: to the nearest feature either side.
    #[test]
    fn a_distance_is_to_the_nearest_feature() {
        let feature = [false, true, false, false, false, true, false];

        assert_eq!(
            distance(&feature, [7, 1]),
            [1.0, 0.0, 1.0, 2.0, 1.0, 0.0, 1.0]
        );
        assert!(
            distance(&[false; 4], [2, 2])
                .iter()
                .all(|d| d.is_infinite())
        );

        // Diagonal: √2.
        let corner = [true, false, false, false];

        assert!((distance(&corner, [2, 2])[3] - 2f64.sqrt()).abs() < 1e-12);
    }

    /// A 3×3 square in a 5×5 grid: its centre 2 in, its edge 1, the grid's
    /// corner √2 out.
    #[test]
    fn the_signed_distance_is_positive_inside() {
        let mask: Vec<bool> = (0..25)
            .map(|i| (1..4).contains(&(i % 5)) && (1..4).contains(&(i / 5)))
            .collect();
        let d = signed(&mask, [5, 5]);

        assert!((d[12] - 2.0).abs() < 1e-12);
        assert!((d[6] - 1.0).abs() < 1e-12);
        assert!((d[0] + 2f64.sqrt()).abs() < 1e-12);
    }

    /// An ellipse on a 200×100 rect: its centre in, corners out, 0.9 of
    /// each axis, its area within 1 % of π · 90 · 45; four times the pixels
    /// on a level twice the map.
    #[test]
    fn the_ellipse_fills_nine_tenths_of_the_rect() {
        let rect = [4.0, 2.0, 200.0, 100.0];
        let one = Grid {
            at: [4, 2],
            size: [200, 100],
            scale: 1.0,
        };
        let mask = ellipse(rect, one);
        let row: usize = (0..200).filter(|&x| mask[50 * 200 + x]).count();

        assert!(mask[50 * 200 + 100]);
        assert!(!mask[0] && !mask[199] && !mask[99 * 200]);
        assert_eq!(row, 180);

        let two = Grid {
            at: [8, 4],
            size: [400, 200],
            scale: 2.0,
        };
        #[allow(clippy::cast_precision_loss, reason = "a pixel count")]
        let share = |m: &[bool], scale: f64| {
            let area = std::f64::consts::PI * 90.0 * 45.0 * scale * scale;

            m.iter().filter(|&&b| b).count() as f64 / area
        };
        let big = ellipse(rect, two);

        assert!((share(&mask, 1.0) - 1.0).abs() < 0.01);
        assert!((share(&big, 2.0) - 1.0).abs() < 0.01);
    }

    /// The identity changes nothing, and reads back from its bytes.
    #[test]
    fn the_identity_table_changes_nothing() {
        let table = Table::identity();

        for rgb in [[0.0, 0.5, 1.0], [0.3, 0.71, 0.05]] {
            let out = table.look(rgb, 1.0);

            assert!((0..3).all(|c| (out[c] - rgb[c]).abs() < 1e-6));
        }

        assert_eq!(Table::from_bytes(&table.bytes()), Some(table));
        assert_eq!(Table::from_bytes(&[0; 12]), None);
    }

    /// A lit picture that is the map with red raised: the fit raises red.
    #[test]
    fn a_fit_takes_a_lit_pictures_change() {
        const SIZE: usize = 80;

        let map: Vec<u8> = (0..SIZE * SIZE)
            .flat_map(|i| {
                #[allow(clippy::cast_possible_truncation, reason = "under 256")]
                let v = (i * 37 % 200) as u8;

                [v, 255 - v, v / 2, 255]
            })
            .collect();
        let lit: Vec<u8> = map
            .chunks(4)
            .flat_map(|p| [p[0].saturating_add(40), p[1], p[2], 255])
            .collect();
        let picture = |rgba| Picture {
            size: [SIZE, SIZE],
            rgba,
        };
        let table = Table::fit(picture(&map), &[([0, 0], picture(&lit))]);
        let out = table.glow([100, 155, 50], 1.0);

        assert!(out[0].abs_diff(140) <= 2, "{out:?}");
        assert!(
            out[1].abs_diff(155) <= 2 && out[2].abs_diff(50) <= 2,
            "{out:?}"
        );
    }
}
