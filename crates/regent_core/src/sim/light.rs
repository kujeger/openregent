//! The diffuse shade a save keeps on every vertex, relit after a
//! building levels the ground.
//!
//! **Binary** — `IsoMap`'s `Map` shading. Bakes every vertex's shade byte over a rect, or the whole map
//! for a null rect.
//!
//! Uses the game's lighting constants, nothing fitted: 99.97% of bytes exact
//! over the saves (**Measured**).
//!
//! # The arithmetic
//!
//! ```text
//! per cell, corners A=(x,y) B=(x,y+1) C=(x+1,y+1) D=(x+1,y):
//!     (x^y) even   t1 = (A,D,C)   t2 = (A,C,B)      -- the A-C diagonal
//!     (x^y) odd    t1 = (D,B,A)   t2 = (D,C,B)      -- the D-B diagonal
//!     each normal is the unit cross product        -- tri_unit_normal
//!
//! per vertex:
//!     n = the mean of the unit normals of every triangle touching it,
//!         which is 8 of the surrounding cells' triangles where (x^y) is
//!         even and 4 where it is odd -- and the mean is NOT renormalised
//!
//!     dot = n . L
//!     dot <  0  ->   0
//!     dot >= 1  -> 255
//!     else      -> trunc(dot * 256)
//! ```
//!
//! Height-to-cell ratio is 1: 16.16 height over 65,536 against cell indices.
//!
//! # The light is the file's, scaled by the contrast
//!
//! `GE_Level` copies `light_defaults` at light defaults
//! into the level, normalises and scales by contrast:
//!
//! ```text
//! L = normalise(LightX, LightY, LightZ) * Contrast
//!   = normalise(0.90, 0.90, 1.0) * 1.5
//! ```
//!
//! Level ground: `dot = L.z` = 0.9267, byte 237. Checked by
//! **Measured**.
//!
//! Ambient is not baked: `GOPT`'s `Ambient` is a bias vertex shade adds
//! at draw time, from `level` in draw ground cell.
//!
//! # Two engine quirks, in every save
//!
//! * The last vertex row is never lit and stays zero, in every save.
//! * On the last lit row the cell-row buffer is stale: its fill loop is guarded
//!   by `y < cells - 1`, so the "below" normals are cell row `y - 2`. See
//!   `STALE_BACK`.
//!
//! # What is left
//!
//! 0.028% of vertices, below building pads' bottom edges. **Inferred**:
//! `Map` reruns over a rect after each ground edit, and the stale buffer
//! hits that rect's bottom row, which the heights cannot reveal.

use super::placement::Heights;
use crate::base::fixed;

/// Cell rows back the stale buffer reaches on the last lit row.
///
/// **Binary** — the two-row buffer is not refilled once `y` reaches
/// `cells - 1`.
const STALE_BACK: u32 = 2;

/// The light, normalised and scaled by the contrast.
///
/// **Binary** — `GE_Level`, over `GOPT`'s
/// `crate::data::settings::LIGHT` and `CONTRAST`.
#[must_use]
pub fn light() -> [f64; 3] {
    let raw = crate::data::settings::LIGHT.map(f64::from);
    let length = raw.iter().map(|c| c * c).sum::<f64>().sqrt();
    let contrast = f64::from(crate::data::settings::CONTRAST);

    raw.map(|c| c / length * contrast)
}

/// One shade per vertex, in the order a save stores them.
#[must_use]
pub fn bake(field: &dyn Heights) -> Vec<u8> {
    let across = field.across();
    let cells = across.saturating_sub(1);
    let light = light();

    let normals = cell_normals(field, cells);

    (0..across)
        .flat_map(|y| (0..across).map(move |x| (x, y)))
        .map(|(x, y)| {
            // The last vertex row is never lit.
            if y + 1 >= across {
                return 0;
            }

            let mean = vertex_normal(&normals, cells, x, y);

            // The engine stores the dot as `f32` before compare and truncate.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "x87 stores the dot to a 32-bit slot before truncating"
            )]
            let dot =
                f64::from((mean[0] * light[0] + mean[1] * light[1] + mean[2] * light[2]) as f32);

            byte(dot)
        })
        .collect()
}

/// Both unit triangle normals of every cell, cell-major.
fn cell_normals(field: &dyn Heights, cells: u32) -> Vec<[[f64; 3]; 2]> {
    let height = |x: u32, y: u32| field.height(x, y).map_or(0.0, fixed::to_f64);

    (0..cells)
        .flat_map(|y| (0..cells).map(move |x| (x, y)))
        .map(|(x, y)| {
            let at =
                |dx: u32, dy: u32| [f64::from(x + dx), f64::from(y + dy), height(x + dx, y + dy)];
            let (a, b, c, d) = (at(0, 0), at(0, 1), at(1, 1), at(1, 0));

            // Checkerboard cut.
            if (x ^ y) & 1 == 0 {
                [normal(a, d, c), normal(a, c, b)]
            } else {
                [normal(d, b, a), normal(d, c, b)]
            }
        })
        .collect()
}

/// The mean of the unit normals of every triangle touching a vertex.
///
/// Not renormalised, so uneven ground gets a shorter vector and darker byte.
fn vertex_normal(normals: &[[[f64; 3]; 2]], cells: u32, x: u32, y: u32) -> [f64; 3] {
    // Even `x ^ y`: both triangles of each cell touch the vertex; odd: one.
    let both = (x ^ y) & 1 == 0;
    let mut sum = [0.0; 3];
    let mut count = 0.0;

    // Up-left, up-right, down-left, down-right, and which triangle always
    // touches the vertex.
    let corners = [
        (x.checked_sub(1), y.checked_sub(1), 1),
        (Some(x), y.checked_sub(1), 1),
        (x.checked_sub(1), Some(y), 0),
        (Some(x), Some(y), 0),
    ];

    for (cx, cy, always) in corners {
        let (Some(cx), Some(cy)) = (cx, cy) else {
            continue;
        };

        if cx >= cells || cy >= cells {
            continue;
        }

        // Stale buffer: cells below the last lit row are two rows early.
        let row = if cy + 1 == cells && cy >= STALE_BACK && always == 0 {
            cy - STALE_BACK
        } else {
            cy
        };
        let cell = normals[(row * cells + cx) as usize];

        for (out, part) in sum.iter_mut().zip(cell[always]) {
            *out += part;
        }

        count += 1.0;

        if both {
            for (out, part) in sum.iter_mut().zip(cell[1 - always]) {
                *out += part;
            }

            count += 1.0;
        }
    }

    if count == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        sum.map(|c| c / count)
    }
}

/// The unit normal of a triangle.
///
/// **Binary** — `tri_unit_normal` in tri unit normal: the cross product of
/// `p1 - p0` with `p2 - p0`, divided by its length.
fn normal(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3]) -> [f64; 3] {
    let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let v = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = cross.iter().map(|c| c * c).sum::<f64>().sqrt();

    if length == 0.0 {
        [0.0, 0.0, 0.0]
    } else {
        cross.map(|c| c / length)
    }
}

/// A dot product as the byte the engine keeps.
///
/// **Binary** — `Map` on: below zero is 0, at or above one is 255,
/// else `dot * 256` truncated (FPU set to chop).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded either side before the cast, and the engine chops"
)]
fn byte(dot: f64) -> u8 {
    if dot < 0.0 {
        0
    } else if dot >= 1.0 {
        255
    } else {
        (dot * 256.0) as u8
    }
}
