//! Which drawing of a cell a generated map asks for.
//!
//! A cell carries a `variant` and a `rotation`: which picture the engine
//! drew. A scatter object's slot is [`crate::sim::mapgen::animate`]'s.
//!
//! Two fresh games on one setup agree on every scatter slot, about one cell
//! variant in seven, and no rotation where either turned a tile:
//! **Measured**.
//!
//! So variant and rotation are invented. Chosen here, since
//! zero everywhere would draw one picture per class.

use crate::content::terrain::grid::Cell;

/// Knuth's multiplicative hash constant.
const MIX: u32 = 2_654_435_761;

/// A cell's variant: one of `count`, by where it stands.
/// The engine's is paint rng's, seeded by the clock, so any is as
/// good. Rotation stays 0.
#[must_use]
pub fn spread(cell: Cell, count: u16, at: (u32, u32)) -> Cell {
    let mixed = at.0.wrapping_mul(MIX) ^ at.1.wrapping_mul(MIX).rotate_left(16);
    let variant = u16::try_from(mixed % u32::from(count.max(1))).unwrap_or(0);

    Cell { variant, ..cell }
}
