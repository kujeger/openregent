//! The simulation's number units. **Binary** unless marked.
//!
//! ```text
//!  16.16 fixed    base::fixed: heights, positions, camera scales
//!  32nds          GPL coordinates: 16.16 >> POS32_SHIFT; PER_CELL to a
//!                 cell, PER_HALF to a half cell
//!  half cells     footprints and saved positions
//!  ground pixels  footprint art: PIXELS_PER_CELL to a cell, the same
//!                 count as 32nds, but another unit
//! ```
//!
//! The 16.16 type and its arithmetic are [`crate::base::fixed`]. GPL's own
//! `float` is 22.10 ([`crate::base::gfix`]); the two never mix.

use crate::base::fixed::{self, Fixed};

/// Half cells in a cell. **Inferred**: the grid place search
/// bounds, map `0x50`, is the cells twice over.
pub const HALVES: i32 = 2;

/// A point in thirty-seconds of a cell as cells.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "a map is at most 16,384 32nds")]
pub fn cells_f32((x, y): (i32, i32)) -> [f32; 2] {
    [x as f32 / PER_CELL as f32, y as f32 / PER_CELL as f32]
}

/// 16.16 to thirty-seconds of a cell: unit pos32
/// shifts right, arithmetic.
pub const POS32_SHIFT: u32 = 11;

/// Thirty-seconds of a cell to 16.16: `<< 11`.
///
/// ```
/// use regent_core::base::fixed::ONE;
/// use regent_core::sim::units::{PER_CELL, from_pos32};
///
/// assert_eq!(from_pos32(PER_CELL), ONE);
/// ```
#[must_use]
pub const fn from_pos32(n: i32) -> Fixed {
    Fixed::from_bits(n << POS32_SHIFT)
}

/// 16.16 to thirty-seconds of a cell, flooring: `>> 11`.
#[must_use]
pub const fn to_pos32(f: Fixed) -> i32 {
    f.to_bits() >> POS32_SHIFT
}

/// Thirty-seconds of a cell in a cell: `IsoGameView` turns a vertex
/// into GPL coordinates by `<< 16 >> 11`.
pub const PER_CELL: i32 = 1 << (16 - POS32_SHIFT);

/// Thirty-seconds of a cell in a half cell.
pub const PER_HALF: i32 = PER_CELL / 2;

/// A cell in 16.16 thirty-seconds, 32.0: cell pos of pos32
/// divides by `0x200000`.
pub const PER_CELL_FIXED: Fixed = fixed::from_whole(PER_CELL);

/// Ground pixels to a half cell: the shift in footprint rect build,
/// `<< 4` in `GSUnit`. Footprints are natively
/// half-cell, hence halves in saved positions.
pub const PIXELS_PER_HALF: i32 = 16;

/// Ground pixels to a whole cell.
pub const PIXELS_PER_CELL: i32 = 2 * PIXELS_PER_HALF;

/// Camera, `pixel_scale_cos_fixed`, at regular zoom (scale 1.0):
/// 32 times cos entry 39, `0x9368`, `IsoCameraAngle` 55° as
/// the table indexes it. Art heights pass through it.
pub const CAMERA_COS: Fixed = Fixed::from_bits(32 * 0x9368);

/// Camera, `pixel_scale_sin_fixed`, at regular zoom: 32 times
/// sin entry 39, `0xd14d`. A layer's art y
/// over it is board y.
pub const CAMERA_SIN: Fixed = Fixed::from_bits(32 * 0xd14d);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::fixed::ONE;

    #[test]
    fn a_cell_is_thirty_two_of_each_small_unit() {
        assert_eq!(PER_CELL, 32);
        assert_eq!(PIXELS_PER_CELL, 32);
        assert_eq!(PER_CELL_FIXED.to_bits(), 0x0020_0000);
        assert_eq!(to_pos32(ONE), PER_CELL);
    }
}
