//! The sepia faces behind the front end's panels ([`BACKDROP`]): unit
//! portraits on a grid, toned.
//!
//! ```text
//!   ├──── 110 ────┼──── 110 ────┼ … 7 cells, 770
//!   │ 5 ┌─────┐   │   ┌─────┐   │
//!   │   │ 100 │   │   │     │   │   black round each portrait
//!   │   └─────┘   │   └─────┘   │
//!   ├─────────────┼─────────────┼ … 5 rows, 550
//! ```
//!
//! **Corpus**: each of the 35 cells is [`ORIGINAL`]'s unit's portrait,
//! set 1000, at (5, 5) in its cell, its colour [`sepia`] of the
//! portrait's light. The engine draws the picture as shipped; the toning
//! is the artist's.
//!
//! [`BACKDROP`]: super::BACKDROP

use std::collections::BTreeSet;

use crate::data::catalogue::Catalogue;
use crate::data::fourcc::FourCc;
use crate::data::payload::Rgb;

/// A cell's side, pixels.
pub const CELL: usize = 110;

/// A portrait's corner from its cell's.
pub const INSET: usize = 5;

/// Cells a row.
pub const COLUMNS: usize = 7;

/// The portraits' set in their unit's `IMAG`.
pub const PORTRAIT_SET: i32 = 1000;

/// The units whose portraits [`BACKDROP`](super::BACKDROP) shows, row by
/// row from the top left.
pub const ORIGINAL: [FourCc; 35] = [
    FourCc(*b"BVd1"),
    FourCc(*b"BVL1"),
    FourCc(*b"AVN1"),
    FourCc(*b"BVE1"),
    FourCc(*b"AVg1"),
    FourCc(*b"AVL1"),
    FourCc(*b"BVM1"),
    FourCc(*b"AVB1"),
    FourCc(*b"AVf1"),
    FourCc(*b"BVg1"),
    FourCc(*b"AVF1"),
    FourCc(*b"BVL3"),
    FourCc(*b"AVG1"),
    FourCc(*b"BVD1"),
    FourCc(*b"BVN1"),
    FourCc(*b"AVM1"),
    FourCc(*b"BVV1"),
    FourCc(*b"BVZ1"),
    FourCc(*b"BVe1"),
    FourCc(*b"AVa1"),
    FourCc(*b"BVX1"),
    FourCc(*b"BVK1"),
    FourCc(*b"AVd1"),
    FourCc(*b"BVA1"),
    FourCc(*b"AVc1"),
    FourCc(*b"BVR1"),
    FourCc(*b"AVJ1"),
    FourCc(*b"BVc1"),
    FourCc(*b"AVe1"),
    FourCc(*b"BVJ1"),
    FourCc(*b"AVC1"),
    FourCc(*b"BVk1"),
    FourCc(*b"AVK1"),
    FourCc(*b"AVj1"),
    FourCc(*b"BVL4"),
];

/// [`sepia`]'s colour at light 0, 16, 32 … 240 and 255: black, through
/// brown, to a dim grey. **Inferred**: a least-squares fit of
/// [`ORIGINAL`]'s portraits to the picture.
pub const RAMP: [[u8; 3]; 17] = [
    [1, 1, 0],
    [5, 3, 1],
    [9, 7, 0],
    [17, 13, 6],
    [25, 19, 8],
    [32, 25, 15],
    [39, 30, 16],
    [45, 34, 20],
    [53, 40, 25],
    [56, 45, 31],
    [60, 51, 38],
    [63, 56, 45],
    [65, 61, 51],
    [70, 66, 58],
    [72, 71, 64],
    [77, 76, 71],
    [78, 78, 74],
];

/// [`RAMP`]'s step in light.
const STEP: f32 = 16.0;

/// The unit codes whose prefix the faces take: heroes and townsfolk
/// (`AV`), monsters (`BV`).
const PREFIXES: [&[u8; 2]; 2] = [b"AV", b"BV"];

/// The `AV` family left out: the resources (`AVk1`, a heal plant,
/// …), whose portraits show items, not faces.
const RESOURCES: &[u8; 3] = b"AVk";

/// `colour` toned: its light, `0.30 r + 0.59 g + 0.11 b`, along [`RAMP`].
#[must_use]
pub fn sepia(colour: Rgb) -> Rgb {
    let [r, g, b] = colour.channels().map(f32::from);
    let light = 0.30 * r + 0.59 * g + 0.11 * b;

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a step of 0 to 16"
    )]
    let at = ((light / STEP) as usize).min(RAMP.len() - 2);

    // The last step runs 240 to 255, not 256.
    #[allow(clippy::cast_precision_loss, reason = "a step of 0 to 16")]
    let (from, to) = (at as f32 * STEP, ((at + 1) as f32 * STEP).min(255.0));
    let share = ((light - from) / (to - from)).clamp(0.0, 1.0);

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel"
    )]
    let channel = |c: usize| {
        let (low, high) = (f32::from(RAMP[at][c]), f32::from(RAMP[at + 1][c]));

        (low + (high - low) * share).round().clamp(0.0, 255.0) as u8
    };

    Rgb::from([channel(0), channel(1), channel(2)])
}

/// Whether unit `code`'s portrait may stand in the faces: an `AV` or
/// `BV` unit, not a resource. **Inferred**: [`ORIGINAL`] is all such;
/// openregent's pool for the faces it makes (`docs/openregent/enhancements.md`).
#[must_use]
pub fn face(code: FourCc) -> bool {
    let FourCc(c) = code;

    PREFIXES.iter().any(|p| c.starts_with(*p)) && !c.starts_with(RESOURCES)
}

/// Every `IMAG` record of `catalogue` that [`face`] takes, once each, in
/// code order. Whether it has a portrait is the reader's to find.
#[must_use]
pub fn pool(catalogue: &Catalogue<'_>) -> Vec<FourCc> {
    let codes: BTreeSet<FourCc> = catalogue
        .tagged(b"IMAG")
        .map(|(_, r)| r.id)
        .filter(|&c| face(c))
        .collect();

    codes.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sepia_runs_the_ramp_end_to_end() {
        let at = |level: u8| sepia(Rgb::from([level; 3])).channels();

        assert_eq!(at(0), RAMP[0]);
        assert_eq!(at(128), RAMP[8]);
        assert_eq!(at(255), RAMP[16]);
    }

    #[test]
    fn the_original_faces_are_all_in_the_pool() {
        assert!(ORIGINAL.into_iter().all(face));
    }

    #[test]
    fn resources_and_others_are_no_faces() {
        let no = [*b"AVk1", *b"AVk4", *b"INse", *b"ABa1"];

        assert!(no.into_iter().all(|c| !face(FourCc(c))));
    }
}
