//! A map's ground as arrays: heights and shade on the vertices, pictures on
//! the cells.

use crate::data::error::{Error, Result};

/// Map sizes: the scenario name's trailing letter, and cells per side.
///
/// Confirmed by saves at each. The Enemy Forces choice carries the size
/// ("Goblin Caves (Beginner) - L"); freestyle has no separate size control.
pub const MAP_SIZES: [(char, u32); 3] = [('S', 128), ('L', 256), ('H', 512)];

/// One cell: which picture to draw, and from which two terrain classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// The two terrain classes, packed one per nibble.
    pub pair: u8,
    /// Which of the seventeen blend cases this cell takes.
    ///
    /// A slot in [`crate::content::terrain::schema`]'s table; solving corners from a real
    /// map reproduces all seventeen.
    pub case: u8,
    /// Which drawing of that case, where the tileset offers several.
    pub variant: u16,
    /// Which quarter of a two-by-two picture this cell draws.
    ///
    /// Not a rotation. The stream index into a four-cell group picture: 0 top
    /// left, 1 top right, 2 bottom left, 3 bottom right. A uniform cell's art
    /// is column [`Cell::variant`], stream `rotation`.
    ///
    /// Non-zero only where the column has four streams — see
    /// **Measured**.
    pub rotation: u16,
}

impl Cell {
    /// The class the cell is mostly made of.
    #[must_use]
    pub const fn near(self) -> u8 {
        self.pair >> 4
    }

    /// The class that intrudes on the corners the case names.
    #[must_use]
    pub const fn far(self) -> u8 {
        self.pair & 0xF
    }

    /// Whether the cell is a single terrain rather than a blend.
    ///
    /// True exactly when `case` is zero, on every corpus cell.
    #[must_use]
    pub const fn uniform(self) -> bool {
        self.near() == self.far()
    }
}

/// One vertex: how high the ground is, how lit it is, and a flag byte.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    pub height: f32,
    /// How brightly this corner of the ground is lit. See [`Vertex::FLAT`].
    pub shade: u8,
    /// Bit `p`: ground player `p` has explored. The crown's is
    /// [`Vertex::EXPLORED`]; slot 7's, `0x80`, is on every vertex.
    pub flags: u8,
}

impl Vertex {
    /// What [`Vertex::shade`] reads where the ground around it is level.
    ///
    /// Shade is diffuse light baked from the heightfield; 233 distinct values
    /// per map, `0xff` most common.
    ///
    /// **Binary**: `Map` splits each cell into two
    /// triangles by `(x^y)` parity, averages the unit normals of the triangles
    /// touching a vertex (eight or four) without renormalising, dots with
    /// `normalise(LightX, LightY, LightZ) * Contrast`, and writes
    /// `trunc(dot * 256)`, clamped. Constants from `miscdata.cam` `GOPT`; see
    /// [`crate::data::settings`]. Level ground: `trunc(0.9267 * 256)` = 237.
    /// `Ambient` is applied at draw time. The last vertex row is
    /// never lit. 99.97% of the corpus matches; see `sim::light`.
    ///
    /// Corpus checks:
    ///
    /// - All 18,310 vertices with level neighbours read 237:
    ///   **Measured**.
    /// - Flattened ground reshades its ring:
    ///   `flattening_the_ground_reshades_the_ring_around_it`.
    ///
    /// `+x` is right, `+y` down; the sun is lower right, shadows fall upper
    /// left.
    ///
    /// `MaxSlope` is a slope clamp, not lighting. See [`Terrain::RIM_STEP`].
    pub const FLAT: u8 = 237;

    /// The [`Vertex::flags`] bit set where the player has seen the ground.
    ///
    /// **Game**: 726 of 16,641 vertices at a freestyle start; all 16,641 with
    /// the map revealed.
    pub const EXPLORED: u8 = 0x01;

    /// [`Vertex::flags`] where no one has seen the ground: slot 7's bit
    /// alone, which is on every vertex of every save.
    pub const UNSEEN: u8 = 0x80;

    /// Whether the player has seen the ground here.
    #[must_use]
    pub const fn explored(&self) -> bool {
        self.flags & Self::EXPLORED != 0
    }
}

/// A map: heights on the vertices, pictures on the cells.
#[derive(Debug, Clone, Copy)]
pub struct Terrain<'a> {
    side: u32,
    vertices: &'a [Vertex],
    cells: &'a [Cell],
}

impl<'a> Terrain<'a> {
    /// A map from its vertex and cell arrays.
    ///
    /// # Errors
    ///
    /// If the cells are no square grid of at least one cell, with vertices
    /// one larger each way.
    pub fn typed(vertices: &'a [Vertex], cells: &'a [Cell]) -> Result<Self> {
        let side = cells.len().isqrt();
        let across = side + 1;

        match u32::try_from(side) {
            Ok(side)
                if side > 0
                    && cells.len() == side as usize * side as usize
                    && vertices.len() == across * across =>
            {
                Ok(Terrain {
                    side,
                    vertices,
                    cells,
                })
            }
            _ => Err(Error::BrokenInvariant {
                what: "terrain is no square grid",
                expected: 0,
                found: cells.len() as u64,
            }),
        }
    }

    /// In-memory height scale: 16.16 fixed point. The file stores `f32`; the
    /// generator's constants are whole multiples.
    pub const FIXED: f32 = crate::base::fixed::ONE_F32;

    /// `tan(MaxSlope)`: the most neighbouring vertices may differ by.
    ///
    /// **Binary**: the generator's last pass floods the slope clamp
    /// from `(0, 0)`, then pins rows 0 and `side`.
    ///
    /// **Corpus**: never exceeded, and reached exactly. See
    /// **Measured**.
    ///
    /// Not `tan(45°)`: set max slope truncates degrees to an index into
    /// a 256-entry trig table, so 45° gives index 31, `tan(43.59°)`. See
    /// `crate::sim::mapgen::rim::tan_of`.
    pub const RIM_STEP: f32 = 62394.0 / Self::FIXED;

    /// The height of vertices `(0, 1)` and `(1, 1)` only.
    ///
    /// **Binary**: the unset sentinel `-1.0` at `(0, 0)` during the slope
    /// clamp pulls its neighbours to `-1.0 + ` [`Terrain::RIM_STEP`]; row 0 is
    /// then zeroed, row 1 is not. So mark + step = 1.0 unit. See
    /// **Measured**.
    pub const RIM_MARK: f32 = -3142.0 / Self::FIXED;

    /// Cells across, which is also cells down.
    pub const fn side(&self) -> u32 {
        self.side
    }

    /// Vertices across: one more than the cells, since they are corners.
    pub const fn across(&self) -> u32 {
        self.side + 1
    }

    /// The cell at `(x, y)`, or `None` past the edge.
    #[must_use]
    pub fn cell(&self, x: u32, y: u32) -> Option<Cell> {
        let i = index(x, y, self.side)?;

        self.cells.get(i).copied()
    }

    /// The vertex at `(x, y)`, or `None` past the edge.
    #[must_use]
    pub fn vertex(&self, x: u32, y: u32) -> Option<Vertex> {
        let i = index(x, y, self.across())?;

        self.vertices.get(i).copied()
    }

    /// Every cell, in storage order: x fastest, then y.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.side).flat_map(move |y| (0..self.side).filter_map(move |x| self.cell(x, y)))
    }

    /// Every vertex, in storage order.
    pub fn vertices(&self) -> impl Iterator<Item = Vertex> + '_ {
        (0..self.across())
            .flat_map(move |y| (0..self.across()).filter_map(move |x| self.vertex(x, y)))
    }
}

/// Where `(x, y)` sits in a `span` by `span` table, or `None` past the edge.
fn index(x: u32, y: u32, span: u32) -> Option<usize> {
    (x < span && y < span).then(|| (y * span + x) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A map of no cells is refused: its one vertex makes no grid.
    #[test]
    fn a_map_of_no_cells_is_refused() {
        let vertex = Vertex {
            height: 0.0,
            shade: 0,
            flags: 0,
        };

        assert!(Terrain::typed(&[vertex], &[]).is_err());
    }

    #[test]
    fn a_typed_map_is_square() {
        let cell = Cell {
            pair: 0,
            case: 0,
            variant: 0,
            rotation: 0,
        };
        let vertex = Vertex {
            height: 0.0,
            shade: 0,
            flags: 0,
        };

        assert!(Terrain::typed(&[vertex; 9], &[cell; 3]).is_err());
        assert!(Terrain::typed(&[vertex; 8], &[cell; 4]).is_err());
    }
}
