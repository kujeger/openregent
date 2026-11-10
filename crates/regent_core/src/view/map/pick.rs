//! Which ground a screen point is over: the iso view's pick,
//! `IsoGameView`. **Binary**.
//!
//! The build preview (build preview test, also a camp move), a
//! spell's point and an explore flag
//! all ask it.
//!
//! ```text
//!  screen point (sx, sy), map pixels
//!    column  x = sx / camera x, flat row y = sy / camera y
//!    rows    up till a row of corners lies wholly above the point,
//!            down till one lies wholly below
//!    cells   of that column, top row first: the first whose
//!            projected triangle holds the point
//!    corner  of that cell nearest the point on screen
//!  ─> that vertex, in 32nds: whole cells only
//! ```
//!
//! No iteration: the drawn triangles are tested directly. A point on a
//! triangle's edge is in neither (`inside`); a miss leaves the preview
//! where it was, a click does nothing.
//!
//! **Corpus**: every player-placed building and explore flag in the traces
//! (`regent_core/tests/trace.rs`, 8 points) is on a whole cell.

use super::cut::Cut;
use crate::base::fixed::{self, Fixed};
use crate::sim::placement::Heights;
use crate::sim::units::PER_CELL;

/// How a point on the ground reaches the screen, per cell across, per cell
/// down, per height unit up: view,.
///
/// **Binary**: `IsoGameView` projects
/// `(x * across, y * down - z * up)`, as the pick projects corners.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub across: f32,
    pub down: f32,
    pub up: f32,
}

/// The cell a screen point is over, and which of its two triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The cell, `(x, y)`.
    pub cell: [u32; 2],
    /// The triangle's vertices, in the engine's test order.
    pub triangle: [[u32; 2]; 3],
}

/// How a row of corners lies against the point: pick row sides's bits.
const ABOVE: u8 = 1;
const LEVEL: u8 = 2;
const BELOW: u8 = 4;

/// Where vertex `(x, y)` is drawn, in map pixels; 0 high past the edge.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    reason = "a map is at most 512 cells; heights are a few thousand units"
)]
pub fn corner(heights: &dyn Heights, camera: Camera, [x, y]: [u32; 2]) -> [f32; 2] {
    let up = fixed::to_f32(heights.height(x, y).unwrap_or(Fixed::ZERO));

    [
        x as f32 * camera.across,
        y as f32 * camera.down - up * camera.up,
    ]
}

/// The cell under a screen point, as iso pick cell finds it; `None` off the
/// drawn ground.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "a map is at most 512 cells"
)]
pub fn hit(heights: &dyn Heights, camera: Camera, at: [f32; 2]) -> Option<Hit> {
    let side = heights.across().saturating_sub(1) as i32;

    // The flat cell: x is the column, as height moves nothing across.
    let flat = [
        (at[0] / camera.across).floor() as i32,
        (at[1] / camera.down).floor() as i32,
    ];
    let (x0, x1) = (flat[0].max(0), flat[0].min(side - 1));
    let (y0, y1) = (flat[1].max(0), flat[1].min(side - 1));

    let (top, bottom) = rows(heights, camera, at, (x0, x1 + 1), (y0, y1));

    (x0..=x1)
        .flat_map(|x| (top..=bottom).map(move |y| (x, y)))
        .find_map(|(x, y)| triangle(heights, camera, at, [x.cast_unsigned(), y.cast_unsigned()]))
}

/// The vertex the engine picks under a screen point: the hit cell's
/// corner nearest it on screen, first of equals in the order top left, top
/// right, bottom left, bottom right (iso pick vertex, mode 0).
#[must_use]
pub fn vertex(heights: &dyn Heights, camera: Camera, at: [f32; 2]) -> Option<[u32; 2]> {
    let [x, y] = hit(heights, camera, at)?.cell;
    let away = |v: [u32; 2]| {
        let c = corner(heights, camera, v);

        (c[0] - at[0]).hypot(c[1] - at[1])
    };

    [[x, y], [x + 1, y], [x, y + 1], [x + 1, y + 1]]
        .into_iter()
        .min_by(|a, b| away(*a).total_cmp(&away(*b)))
}

/// method: the picked vertex in 32nds, as the preview, a spell and a flag
/// take it.
#[must_use]
pub fn pick(heights: &dyn Heights, camera: Camera, at: [f32; 2]) -> Option<(i32, i32)> {
    vertex(heights, camera, at)
        .map(|[x, y]| (x.cast_signed() * PER_CELL, y.cast_signed() * PER_CELL))
}

/// The rows to test, pick rows grow: from the flat rows, up to the nearest
/// row whose corners in `columns` all lie above the point, down to the
/// nearest whose corners all lie below; clamped to the cells.
fn rows(
    heights: &dyn Heights,
    camera: Camera,
    at: [f32; 2],
    columns: (i32, i32),
    flat: (i32, i32),
) -> (i32, i32) {
    let side = heights.across().saturating_sub(1).cast_signed();
    let lies = |y: i32| sides(heights, camera, at, y, columns);

    // Up while a corner is level with or below the point, then back down
    // past rows wholly above it.
    let up = (0..=flat.0)
        .rev()
        .find(|y| *y == 0 || lies(*y) & (LEVEL | BELOW) == 0)
        .unwrap_or(flat.0);
    let top = (up..=side)
        .find(|y| *y >= side || lies(*y) & (LEVEL | BELOW) != 0)
        .unwrap_or(side);

    // Down while a corner is level with or above it.
    let bottom = (flat.1..=side)
        .find(|y| *y >= side || lies(*y) & (ABOVE | LEVEL) == 0)
        .unwrap_or(flat.1);

    ((top - 1).max(0), bottom.min(side - 1))
}

/// How row `y`'s corners over `columns` lie against the point, as bits;
/// 0 off the cells. The last vertex row and column count as off.
fn sides(heights: &dyn Heights, camera: Camera, at: [f32; 2], y: i32, columns: (i32, i32)) -> u8 {
    let side = heights.across().saturating_sub(1).cast_signed();

    if (0..side).contains(&y) {
        (columns.0.max(0)..=columns.1.min(side - 1))
            .map(|x| corner(heights, camera, [x.cast_unsigned(), y.cast_unsigned()])[1])
            .fold(0, |bits, down| {
                bits | if down < at[1] {
                    ABOVE
                } else if down > at[1] {
                    BELOW
                } else {
                    LEVEL
                }
            })
    } else {
        0
    }
}

/// Which of cell `(x, y)`'s triangles holds the point, cell hit triangle: cut
/// as the ground is drawn ([`Cut`]).
fn triangle(heights: &dyn Heights, camera: Camera, at: [f32; 2], [x, y]: [u32; 2]) -> Option<Hit> {
    let (tl, tr, bl, br) = ([x, y], [x + 1, y], [x, y + 1], [x + 1, y + 1]);
    let pair = match Cut::of(x, y) {
        Cut::Main => [[tl, tr, br], [tl, bl, br]],
        Cut::Anti => [[tl, tr, bl], [tr, bl, br]],
    };

    pair.into_iter()
        .find(|t| inside(at, t.map(|v| corner(heights, camera, v))))
        .map(|triangle| Hit {
            cell: [x, y],
            triangle,
        })
}

/// Whether `p` is strictly inside triangle `abc`, either winding:
/// Point in triangle. On an edge is outside.
fn inside(p: [f32; 2], [a, b, c]: [[f32; 2]; 3]) -> bool {
    let cross = |from: [f32; 2], to: [f32; 2]| {
        (to[0] - from[0]) * (p[1] - from[1]) - (p[0] - from[0]) * (to[1] - from[1])
    };
    let turns = [cross(a, b), cross(b, c), cross(c, a)];

    turns.iter().all(|t| *t > 0.0) || turns.iter().all(|t| *t < 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square map of `side` cells, heights from `f` per vertex.
    struct Field {
        across: u32,
        heights: Vec<Fixed>,
    }

    impl Field {
        fn new(side: u32, f: impl Fn(u32, u32) -> f32) -> Self {
            let across = side + 1;
            #[allow(clippy::cast_possible_truncation, reason = "test heights")]
            let heights = (0..across * across)
                .map(|i| Fixed::from_bits((f(i % across, i / across) * fixed::ONE_F32) as i32))
                .collect();

            Self { across, heights }
        }
    }

    impl Heights for Field {
        fn across(&self) -> u32 {
            self.across
        }

        fn height(&self, x: u32, y: u32) -> Option<Fixed> {
            (x < self.across && y < self.across)
                .then(|| self.heights[(y * self.across + x) as usize])
        }
    }

    const CAMERA: Camera = Camera {
        across: 32.0,
        down: 26.0,
        up: 18.0,
    };

    /// Flat ground: the cell under the point is the point's cell.
    #[test]
    fn on_flat_ground_the_hit_is_the_flat_cell() {
        let field = Field::new(4, |_, _| 0.0);
        let hit = hit(&field, CAMERA, [2.3 * 32.0, 1.6 * 26.0]).expect("a cell");

        assert_eq!(hit.cell, [2, 1]);
    }

    /// A hill lifts the cells in front of it over the flat row: the pick
    /// finds the raised cell, not the one behind the point on flat ground.
    #[test]
    fn a_raised_cell_is_found_above_its_flat_row() {
        let field = Field::new(6, |_, y| if y >= 3 { 2.0 } else { 0.0 });

        // Flat row 2, but row 3 and down are lifted 36 pixels.
        let at = [1.5 * 32.0, 2.5 * 26.0];
        let hit = hit(&field, CAMERA, at).expect("a cell");

        assert_eq!(hit.cell, [1, 3]);
    }

    /// Where the ground folds over itself, the top row wins, though a row
    /// below is drawn over it.
    #[test]
    fn on_a_fold_the_top_row_is_taken() {
        // Rows 3 and down are lifted 54 pixels, past row 2: cells 1, 2 and
        // 3 all cover screen row 40.
        let field = Field::new(6, |_, y| if y >= 3 { 3.0 } else { 0.0 });
        let at = [1.5 * 32.0, 40.0];

        assert_eq!(hit(&field, CAMERA, at).map(|h| h.cell), Some([1, 1]));
    }

    /// The corner nearest on screen, not in cells: a high corner drawn
    /// close to the point wins over a low one nearer on the flat map.
    #[test]
    fn the_vertex_is_the_corner_nearest_on_screen() {
        let field = Field::new(4, |x, y| if (x, y) == (2, 2) { 1.0 } else { 0.0 });

        // In cell (1, 1), nearer (2, 1) on the flat map; (2, 2) is drawn
        // 18 pixels up, beside the point.
        let at = [1.6 * 32.0, 1.4 * 26.0];

        assert_eq!(vertex(&field, CAMERA, at), Some([2, 2]));
        assert_eq!(pick(&field, CAMERA, at), Some((64, 64)));
    }

    /// A point on a drawn edge is in no triangle.
    #[test]
    fn an_edge_is_in_neither_triangle() {
        let a = [0.0, 0.0];
        let b = [10.0, 0.0];
        let c = [0.0, 10.0];

        assert!(inside([2.0, 2.0], [a, b, c]));
        assert!(inside([2.0, 2.0], [a, c, b]), "either winding");
        assert!(!inside([5.0, 0.0], [a, b, c]));
    }

    /// Off the map, nothing.
    #[test]
    fn off_the_map_nothing_is_hit() {
        let field = Field::new(4, |_, _| 0.0);

        assert_eq!(hit(&field, CAMERA, [-5.0, 10.0]), None);
        assert_eq!(hit(&field, CAMERA, [10.0 * 32.0, 10.0]), None);
    }
}
