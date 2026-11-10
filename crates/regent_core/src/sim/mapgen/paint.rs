//! Lay a chart's classes onto a map, as the engine does.
//!
//! **Binary** — landscape place walks every cell, x
//! outer, y inner, painting the surface chart's class through
//! Map paint point. The paint is `crate::sim::paint`, shared with
//! building pads (`crate::sim::world`'s placement).
//!
//! # Dirt between two biomes
//!
//! No band. A cell whose pair has no blend tile takes a third class that
//! blends with both, and map blend neighbours spreads it. This matches
//! 96.4% of saved classes (ceiling near 97); an **inferred** radius-two band
//! matched 93.4%.

use crate::data::rgs::{class_of, code_of};
use crate::sim::paint::{self, Cell, Paintable};

use crate::sim::mapgen::surface::Chart;

/// A map's cells, painted.
struct Grid {
    side: i32,
    cells: Vec<Cell>,
}

impl Paintable for Grid {
    fn cell(&self, at: (i32, i32)) -> Option<Cell> {
        self.index(at).map(|slot| self.cells[slot])
    }

    fn set(&mut self, at: (i32, i32), cell: Cell) {
        if let Some(slot) = self.index(at) {
            self.cells[slot] = cell;
        }
    }
}

impl Grid {
    fn index(&self, at: (i32, i32)) -> Option<usize> {
        ((0..self.side).contains(&at.0) && (0..self.side).contains(&at.1))
            .then(|| (at.1 * self.side + at.0).cast_unsigned() as usize)
    }
}

/// Paint every cell of a surface chart, and hand back the cells.
///
/// x outer, y inner, as the engine. The cascade writes neighbours a later
/// paint may cover, so order decides which blends survive.
#[must_use]
pub fn lay(chart: &Chart) -> Vec<Cell> {
    let side = chart.side();
    let mut grid = Grid {
        side,
        cells: vec![Cell::uniform(0); (side * side).max(0).cast_unsigned() as usize],
    };

    for x in 0..side {
        for y in 0..side {
            let Some(class) = chart.at((x, y)).and_then(class_of) else {
                continue;
            };

            paint::point(&mut grid, (x, y), class, 0);
        }
    }

    grid.cells
}

/// The painted cells written back over a chart, one class each.
///
/// Lossy: a cell names two classes, a chart one. For chart-to-chart
/// comparisons; prefer carrying the cells.
#[must_use]
pub fn classes(cells: &[Cell], over: &Chart) -> Chart {
    let side = over.side();
    let mut chart = over.clone();

    for (at, cell) in cells.iter().enumerate() {
        let at = i32::try_from(at).unwrap_or(0);

        chart.set((at % side, at / side), code_of(cell.near()));
    }

    chart
}
