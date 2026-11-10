//! Terrain classes as the chart and a save name them: a `DTPT` id, `GR08`,
//! is class 8 in a save's cell.

use crate::data::fourcc::FourCc;

/// The class number a `DTPT` id carries, or `None` for `NONE` and the like.
///
/// `GR08` is 8, as in a save's cell.
#[must_use]
pub fn class_of(id: FourCc) -> Option<u8> {
    let text = id.to_string();
    let (letters, digits) = text.split_at_checked(2)?;

    (letters == "GR").then(|| digits.parse().ok())?
}

/// The `DTPT` id a class number is written as: 8 is `GR08`.
///
/// The inverse of [`class_of`].
#[must_use]
pub const fn code_of(class: u8) -> FourCc {
    let tens = b'0' + (class / 10) % 10;

    FourCc([b'G', b'R', tens, b'0' + class % 10])
}

/// Whether the tileset can draw these two classes meeting.
///
/// **Corpus** and art: five three-step ladders sharing Dirt. The tileset ships
/// `MapTexture`s only for Dirt-to-parent and parent-to-child. A parent is an
/// odd class, its child the even one above. See
/// [terrain.md](docs/formats/terrain.md#the-classes-form-a-tree-and-the-art-agrees)
/// (**Measured**).
#[must_use]
pub const fn blendable(a: u8, b: u8) -> bool {
    let (low, high) = if a < b { (a, b) } else { (b, a) };

    low == high || (low == DIRT && high % 2 == 1) || (low % 2 == 1 && high == low + 1)
}

/// The class a zeroed cell holds, and the base every ladder shares.
pub const DIRT: u8 = 0;
