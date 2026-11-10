//! A grid of terrain, and which blend case each of its cells needs.
//!
//! Terrain lives on vertices. A cell's picture follows from its four corners,
//! so `(w + 1) * (h + 1)` classes decide `w * h` pictures. [`super::schema`]
//! rests on this; **Viewer** confirms it.
//!
//! ```text
//! vertices          cells
//!   A   B   A       ┌─────┬─────┐    the middle vertex is B, so both
//!                   │  1  │  2  │    cells take it as one corner:
//!   A   A   A       └─────┴─────┘    cell 1 at its top right,
//!                                    cell 2 at its top left
//! ```
//!
//! Case selection is here, not in a front end, so tests hold it. Whether the
//! pictures join is the painter's concern.

use super::schema::{Corners, Join, Schema, Turn};

/// A rectangular patch of terrain, held per vertex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    cells: (u32, u32),
    vertices: Vec<u8>,
}

impl Board {
    /// A board of one class throughout.
    #[must_use]
    pub fn new(width: u32, height: u32, class: u8) -> Self {
        let mut board = Board {
            cells: (width, height),
            vertices: Vec::new(),
        };

        board.vertices = vec![class; board.across() * board.down()];
        board
    }

    #[must_use]
    pub const fn cells(&self) -> (u32, u32) {
        self.cells
    }

    /// Vertices per row: cells + 1.
    const fn across(&self) -> usize {
        self.cells.0 as usize + 1
    }

    const fn down(&self) -> usize {
        self.cells.1 as usize + 1
    }

    /// Table index of a vertex, or `None` past the edge. The only signed to
    /// index conversion, so bounds check and conversion stay together.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);

        (x < self.across() && y < self.down()).then_some(y * self.across() + x)
    }

    #[must_use]
    pub fn vertex(&self, x: i32, y: i32) -> Option<u8> {
        self.vertices.get(self.index(x, y)?).copied()
    }

    /// Set a vertex; returns whether it changed.
    pub fn set(&mut self, x: i32, y: i32, class: u8) -> bool {
        match self.index(x, y).and_then(|at| self.vertices.get_mut(at)) {
            Some(at) if *at != class => {
                *at = class;
                true
            }
            _ => false,
        }
    }

    pub fn fill(&mut self, class: u8) {
        self.vertices.fill(class);
    }

    /// Set every vertex within `radius` of `at` to `class`; returns whether
    /// any changed.
    pub fn brush(&mut self, at: (i32, i32), radius: i32, class: u8) -> bool {
        let mut changed = false;

        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    changed |= self.set(at.0 + dx, at.1 + dy, class);
                }
            }
        }

        changed
    }

    /// A cell's four corners: TL, TR, BL, BR. The fallback never fires for a
    /// cell inside the board.
    #[must_use]
    pub fn corners_of(&self, x: i32, y: i32) -> [u8; 4] {
        [
            self.vertex(x, y),
            self.vertex(x + 1, y),
            self.vertex(x, y + 1),
            self.vertex(x + 1, y + 1),
        ]
        .map(|v| v.unwrap_or(0))
    }

    /// Every cell, row-major (composed sheet order).
    pub fn coords(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (w, h) = (self.cells.0.cast_signed(), self.cells.1.cast_signed());

        (0..h).flat_map(move |y| (0..w).map(move |x| (x, y)))
    }

    /// Every vertex, row-major.
    pub fn vertex_coords(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let (w, h) = (self.cells.0.cast_signed(), self.cells.1.cast_signed());

        (0..=h).flat_map(move |y| (0..=w).map(move |x| (x, y)))
    }

    /// Lay a scene out, `far` inside the shape and `near` outside it.
    pub fn lay_out(&mut self, scene: Scene, near: u8, far: u8) {
        let cells = self.cells;
        let want: Vec<(i32, i32, u8)> = self
            .vertex_coords()
            .map(|(x, y)| (x, y, if scene.covers(x, y, cells) { far } else { near }))
            .collect();

        for (x, y, class) in want {
            self.set(x, y, class);
        }
    }
}

/// What a cell needs drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// All corners one class: its ground.
    Ground(u8),
    /// Two classes meet: which blend and case.
    Blend { near: u8, far: u8, case: usize },
    /// Two classes with no blend, or three or more.
    ///
    /// Not an error: blends join a class only to its tree neighbour, so the
    /// board asks for an adjacency the tileset cannot express.
    Impossible,
}

/// What a cell's corners ask for.
///
/// `blend(a, b)` returns the far class if the pair has blend art, else `None`.
pub fn cell(
    corners: [u8; 4],
    schema: Schema,
    turn: Turn,
    join: Join,
    blend: impl Fn(u8, u8) -> Option<u8>,
) -> Cell {
    let mut distinct = corners;
    distinct.sort_unstable();

    let [a, _, _, d] = distinct;

    if a == d {
        return Cell::Ground(a);
    }

    let two: Vec<u8> = {
        let mut seen = distinct.to_vec();
        seen.dedup();
        seen
    };

    match two.as_slice() {
        [near, other] => match blend(*near, *other) {
            Some(far) => {
                let near = if far == *near { *other } else { *near };
                let mask = turn.apply(mask_of(corners, far));

                schema
                    .slot(mask, join)
                    .map_or(Cell::Impossible, |case| Cell::Blend { near, far, case })
            }
            None => Cell::Impossible,
        },
        _ => Cell::Impossible,
    }
}

/// Which corners hold the far class, in canvas space.
fn mask_of(corners: [u8; 4], far: u8) -> Corners {
    [
        Corners::TOP_LEFT,
        Corners::TOP_RIGHT,
        Corners::BOTTOM_LEFT,
        Corners::BOTTOM_RIGHT,
    ]
    .iter()
    .zip(corners)
    .filter(|(_, c)| *c == far)
    .fold(Corners::NONE, |acc, (bit, _)| acc.with(*bit))
}

/// Test shapes, each exercising one family of cases. Here, not in the viewer,
/// so each prediction is a test; drawings are checked by eye.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// One far vertex: the four single-corner cases only.
    Speck,
    /// Horizontal boundary: one edge case along the straddling row.
    Border,
    /// Alternating vertices: the two diagonal cases only.
    Checker,
    /// Staircase: produces three-corner cases.
    Step,
    /// Filled disc: every edge, single-corner and three-corner case.
    Blob,
}

impl Scene {
    pub const ALL: [(Scene, &'static str); 5] = [
        (Scene::Speck, "speck"),
        (Scene::Border, "border"),
        (Scene::Checker, "checker"),
        (Scene::Step, "step"),
        (Scene::Blob, "blob"),
    ];

    /// Whether vertex `(x, y)` is inside the shape.
    #[must_use]
    pub const fn covers(self, x: i32, y: i32, cells: (u32, u32)) -> bool {
        let middle = (cells.0.cast_signed() / 2, cells.1.cast_signed() / 2);

        match self {
            Scene::Speck => x == middle.0 && y == middle.1,
            Scene::Border => y > middle.1,
            Scene::Checker => (x + y) % 2 == 1,
            Scene::Step => x >= y,
            Scene::Blob => {
                let (dx, dy) = (x - middle.0, y - middle.1);
                let radius = if middle.0 < middle.1 {
                    middle.0
                } else {
                    middle.1
                } / 2;

                dx * dx + dy * dy <= radius * radius
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const NEAR: u8 = 0;
    const FAR: u8 = 1;

    /// Blend used by the scenes: `NEAR` to `FAR`.
    fn pair(a: u8, b: u8) -> Option<u8> {
        ((a == NEAR && b == FAR) || (a == FAR && b == NEAR)).then_some(FAR)
    }

    /// Each case a scene uses, with its cell count.
    fn cases(scene: Scene, join: Join) -> Vec<(usize, usize)> {
        let mut board = Board::new(16, 16, NEAR);
        board.lay_out(scene, NEAR, FAR);

        let mut counts = vec![0usize; Schema::Corner.slots()];

        for (x, y) in board.coords() {
            if let Cell::Blend { case, .. } = cell(
                board.corners_of(x, y),
                Schema::Corner,
                Turn::NONE,
                join,
                pair,
            ) {
                counts[case] += 1;
            }
        }

        counts
            .into_iter()
            .enumerate()
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    fn used(scene: Scene, join: Join) -> BTreeSet<usize> {
        cases(scene, join).into_iter().map(|(c, _)| c).collect()
    }

    /// **Viewer**: the four cells close into one patch.
    #[test]
    fn a_speck_uses_the_four_single_corner_cases_and_no_others() {
        let counts = cases(Scene::Speck, Join::Apart);

        assert_eq!(counts.len(), 4, "four cells, four cases: {counts:?}");
        assert!(
            counts.iter().all(|(_, n)| *n == 1),
            "each case exactly once: {counts:?}"
        );

        // One corner each, all four covered.
        let corners: BTreeSet<Corners> = counts
            .iter()
            .filter_map(|(case, _)| Schema::Corner.corners(*case))
            .map(|(c, _)| c)
            .collect();

        assert_eq!(corners.len(), 4);
        assert!(corners.iter().all(|c| c.count() == 1), "{corners:?}");
    }

    /// **Viewer**: one unbroken line.
    #[test]
    fn a_border_uses_one_edge_case_along_its_whole_run() {
        let counts = cases(Scene::Border, Join::Apart);

        assert_eq!(counts.len(), 1, "one case only: {counts:?}");

        let (case, cells) = counts[0];
        let (corners, _) = Schema::Corner.corners(case).expect("a corner mask");

        assert_eq!(cells, 16, "every cell of the row");
        assert_eq!(corners.count(), 2, "an edge is two corners");
        assert!(
            corners.has(Corners::BOTTOM_LEFT) && corners.has(Corners::BOTTOM_RIGHT),
            "the far class is below, so the bottom edge: {corners:?}"
        );
    }

    /// **Viewer**: the two settings draw islands and bands.
    #[test]
    fn a_checker_uses_the_diagonals_and_the_join_picks_which_pair() {
        let apart = used(Scene::Checker, Join::Apart);
        let through = used(Scene::Checker, Join::Through);

        assert_eq!(apart.len(), 2, "two diagonals: {apart:?}");
        assert_eq!(through.len(), 2, "two diagonals: {through:?}");
        assert!(
            apart.is_disjoint(&through),
            "the join has to reach different slots or it means nothing"
        );

        for case in apart.union(&through) {
            let (corners, _) = Schema::Corner.corners(*case).expect("a corner mask");

            assert_eq!(corners.count(), 2, "case {case} is not a pair");
            assert!(
                !corners.has(Corners::TOP_LEFT) || !corners.has(Corners::TOP_RIGHT),
                "case {case} is an edge, not a diagonal"
            );
        }
    }

    #[test]
    fn a_step_reaches_a_three_corner_case() {
        let counts = used(Scene::Step, Join::Apart);
        let triples = counts
            .iter()
            .filter_map(|c| Schema::Corner.corners(*c))
            .filter(|(corners, _)| corners.count() == 3)
            .count();

        assert!(triples > 0, "no three-corner case in {counts:?}");
    }

    /// **Viewer**: an unbroken outline, so the arrangements agree with each
    /// other.
    #[test]
    fn a_blob_reaches_every_case_family() {
        let counts = used(Scene::Blob, Join::Apart);

        let family = |n: u32| {
            counts
                .iter()
                .filter_map(|c| Schema::Corner.corners(*c))
                .filter(|(corners, _)| corners.count() == n)
                .count()
        };

        assert_eq!(family(1), 4, "four single corners: {counts:?}");
        assert_eq!(family(2), 4, "four edges, no diagonals: {counts:?}");
        assert_eq!(family(3), 4, "four three-corner cases: {counts:?}");
    }

    #[test]
    fn a_uniform_board_needs_no_blend_at_all() {
        let board = Board::new(4, 4, NEAR);

        for (x, y) in board.coords() {
            let at = cell(
                board.corners_of(x, y),
                Schema::Corner,
                Turn::NONE,
                Join::Apart,
                pair,
            );

            assert_eq!(at, Cell::Ground(NEAR));
        }
    }

    #[test]
    fn three_classes_at_one_cell_have_no_picture() {
        let mut board = Board::new(2, 2, 0);
        board.set(0, 0, 1);
        board.set(1, 0, 2);

        assert_eq!(
            cell(
                board.corners_of(0, 0),
                Schema::Corner,
                Turn::NONE,
                Join::Apart,
                pair
            ),
            Cell::Impossible,
            "three classes meet, and no tile draws that"
        );
    }

    #[test]
    fn two_classes_with_no_blend_between_them_have_no_picture() {
        let mut board = Board::new(2, 2, 7);
        board.set(0, 0, 9);

        assert_eq!(
            cell(
                board.corners_of(0, 0),
                Schema::Corner,
                Turn::NONE,
                Join::Apart,
                pair
            ),
            Cell::Impossible,
        );
    }

    #[test]
    fn a_brush_paints_a_disc_and_says_whether_it_changed_anything() {
        let mut board = Board::new(8, 8, NEAR);

        assert!(board.brush((4, 4), 1, FAR), "the first stroke changes it");
        assert!(!board.brush((4, 4), 1, FAR), "the second does not");

        // Radius 1: centre and four neighbours, not a square.
        assert_eq!(board.vertex(4, 4), Some(FAR));
        assert_eq!(board.vertex(3, 4), Some(FAR));
        assert_eq!(board.vertex(3, 3), Some(NEAR), "the corners stay out");
    }
}
