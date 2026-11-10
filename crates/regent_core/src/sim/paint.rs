//! Painting a terrain class onto a map, as the engine does.
//!
//! **Binary** — map paint cell,
//! Map blend neighbours, class resolution in
//! Map resolve pair.
//!
//! Runs in play. All ground changes go through it: the generator,
//! building placement, the
//! editor brush.
//!
//! # Why a class is not simply written
//!
//! A cell holds two classes and a blend case (which corners take the far one),
//! so three classes cannot meet in it. The tileset draws only Dirt against a
//! parent and a parent against its child; other pairs have no art.
//!
//! [`resolve`] substitutes a third class blending with both sides — for a child
//! against Dirt, the child's parent. Dirt between biomes comes from this plus
//! the cascade, not from a band of fixed radius.
//!
//! # The cascade
//!
//! Corners are shared with neighbours. After a paint that moved a cell's pair
//! or case, map blend neighbours pushes the class outward — eight ways
//! from a uniform cell, two from an edge case, three from a corner case, none
//! from the four diagonals — recursing where a neighbour moved.
//!
//! Terminates because the corner mask is a union: for a fixed pair it only
//! grows, at most four times. A paint reaches about two rings.
//!
//! # The corner bookkeeping
//!
//! Map paint cell takes the present mask on the side the class landed
//! (forced far when the pair moved), ors in the incoming corners on the near
//! side, and keeps the present mask only where the present case is zero.
//! Inverting that last rule moves 187,443 of 262,144 cells and scores 28.9% of
//! a save's classes; correct, 96.4% against a ceiling near 97.
//!
//! `crate::sim::mapgen::paint` lays a whole map with it; `regent_core::sim::placement`
//! names the cells a building's pad paints.

/// Terrain class count; bounds the substitution scan.
///
/// **Binary** — the loop bound in map resolve pair.
const CLASSES: u8 = 16;

/// One map cell: two classes, and which corners take the far one.
///
/// No `variant` or `rotation`: the engine draws them from a clock-seeded
/// generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cell {
    /// Near class in the high nibble, far in the low.
    pub pair: u8,
    /// Corners taking the far class: index into
    /// [`Schema`](crate::content::terrain::schema::Schema), 0 for uniform.
    pub case: u8,
}

impl Cell {
    /// The main class.
    #[must_use]
    pub const fn near(self) -> u8 {
        self.pair >> 4
    }

    /// The class its `case` corners take.
    #[must_use]
    pub const fn far(self) -> u8 {
        self.pair & 0x0f
    }

    /// A cell wholly of one class.
    #[must_use]
    pub const fn uniform(class: u8) -> Self {
        Cell {
            pair: class << 4 | class,
            case: 0,
        }
    }
}

/// Which drawing of its pair and case a cell shows: the variant, and
/// the quarter of a 2 × 2 picture ([`crate::content::terrain::grid::Cell`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Picture {
    pub variant: u16,
    pub rotation: u16,
}

/// Whether the tileset can draw these two classes meeting.
///
/// Classes blendable: five three-step ladders sharing
/// Dirt; art exists for Dirt-to-parent and parent-to-child only.
const fn blendable(a: u8, b: u8) -> bool {
    crate::data::rgs::blendable(a, b)
}

/// What [`resolve`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    /// The pair to write.
    pub pair: u8,
    /// Side the painted class landed on: 0 near, 1 far.
    pub side: u8,
    /// Whether the cell may keep the corners it already had.
    ///
    /// **Binary** — the cell's byte compared with 0: true where the
    /// present case is zero. A blended cell drops its corners, which name
    /// classes the new pair lacks. True regardless where the pair stands.
    pub keep: bool,
}

/// The pair a cell holds once `class` is painted into it.
///
/// **Binary** — map resolve pair, branch for branch.
/// Step 3 makes the Dirt between biomes.
#[must_use]
pub fn resolve(cell: Cell, class: u8, corners: u8) -> Resolved {
    let (near, far) = (cell.near(), cell.far());

    // Every derived branch returns this. Inverted, a whole-map paint moves 71%
    // of cells.
    let keeps = cell.case == 0;

    // 1. Class already present.
    if near == class {
        return Resolved {
            pair: cell.pair,
            side: 0,
            keep: true,
        };
    }

    if far == class {
        return Resolved {
            pair: cell.pair,
            side: 1,
            keep: true,
        };
    }

    // 2. Ordered pair, low class in the high nibble, if art exists.
    if blendable(far, class) {
        let (low, high) = if class < far {
            (class, far)
        } else {
            (far, class)
        };

        return Resolved {
            pair: low << 4 | high,
            side: u8::from(class != low),
            keep: false,
        };
    }

    // 3. Dirt rule: replace the far class with the lowest class blending with
    //    both. Child against Dirt gives the parent: Dirt, parent, child.
    if let Some(third) = (0..CLASSES).find(|j| blendable(far, *j) && blendable(class, *j)) {
        return with(class, third, keeps);
    }

    // 4. Nothing blends with both: take one blending with `class`, only if
    //    corners were asked for (never in the generator).
    if corners != 0
        && let Some(third) = (0..CLASSES).find(|j| blendable(class, *j))
    {
        return with(class, third, keeps);
    }

    // 5. Uniform `class`.
    Resolved {
        pair: class << 4 | class,
        side: 1,
        keep: keeps,
    }
}

/// Pair of `class` and a substitute: lower class in the high nibble,
/// `third == class` counting as lower.
const fn with(class: u8, third: u8, keep: bool) -> Resolved {
    if third <= class {
        Resolved {
            pair: third << 4 | class,
            side: 1,
            keep,
        }
    } else {
        Resolved {
            pair: class << 4 | third,
            side: 0,
            keep,
        }
    }
}

/// Which corners each case gives the near class, and which the far.
///
/// **Binary** — the 18-entry tables at blend case masks and blend case masks,
/// transcribed, indexed `[case + 18 * side]`. Complements:
/// `row0 ^ row1 == 0xf`.
///
/// Engine bit order, not the viewer's: clockwise from top left, bit 3 down to
/// bit 0.
///
/// ```text
///     b3 --- b2      0x0a = b3|b1, the main diagonal
///     |       |      0x0c = the top edge, 0x03 the bottom
///     b0 --- b1      0x09 = the left edge, 0x06 the right
/// ```
///
/// Index 0 is the empty far set, 17 the full one. Neither is stored; both
/// collapse to uniform.
const MASKS: [[u8; 18]; 2] = [
    [
        0x0f, 0x0a, 0x05, 0x09, 0x03, 0x0c, 0x06, 0x05, 0x0a, 0x08, 0x04, 0x02, 0x01, 0x07, 0x0b,
        0x0d, 0x0e, 0x00,
    ],
    [
        0x00, 0x05, 0x0a, 0x06, 0x0c, 0x03, 0x09, 0x0a, 0x05, 0x07, 0x0b, 0x0d, 0x0e, 0x08, 0x04,
        0x02, 0x01, 0x0f,
    ],
];

/// The first case whose mask on this side is exactly `mask`.
///
/// **Binary** — linear scan from 0, so diagonals resolve to the lower slot
/// (1 before 8, 2 before 7).
fn case_of(mask: u8, side: usize) -> Option<u8> {
    MASKS[side]
        .iter()
        .position(|m| *m == mask)
        .and_then(|at| u8::try_from(at).ok())
}

/// Paints a class into one cell; true if pair or case moved (gates the
/// cascade).
///
/// **Binary** — map paint cell. `corners` is the case
/// whose near mask the class claims: 0 for the whole cell, or an edge or
/// corner case from the cascade.
pub fn paint(cell: &mut Cell, class: u8, corners: u8) -> bool {
    let was = *cell;
    let resolved = resolve(*cell, class, corners);

    // Present mask on the landed side (far if the pair moved), union incoming
    // near-side corners. `keep` false drops the present mask.
    let side = usize::from(resolved.side == 1 || resolved.pair != was.pair);
    let held = if resolved.keep {
        MASKS[side].get(was.case as usize).copied().unwrap_or(0)
    } else {
        0
    };
    let mask = held | MASKS[0].get(corners as usize).copied().unwrap_or(0);

    let mut now = Cell {
        pair: resolved.pair,
        case: case_of(mask, usize::from(resolved.side == 1)).unwrap_or(corners),
    };

    // All or no corners: collapse to uniform.
    if now.case == 0 {
        now.pair = now.near() << 4 | now.near();
    } else if now.case == 17 {
        let far = now.far();

        now = Cell {
            pair: far << 4 | far,
            case: 0,
        };
    }

    *cell = now;
    now != was
}

/// Which neighbours a case pushes to, and with what.
///
/// **Binary** — blocks selected by blend case dispatch.
/// Entry: `(dx, dy, class, corner case to claim)`. Diagonals (1, 2, 7, 8)
/// push nothing.
const fn spread(case: u8) -> &'static [(i32, i32, Side, u8)] {
    use Side::{Far, Major, Near};

    match case {
        // Uniform: all eight.
        0 => &[
            (0, -1, Near, 4),
            (-1, 0, Near, 6),
            (1, 0, Near, 3),
            (0, 1, Near, 5),
            (-1, -1, Near, 11),
            (1, -1, Near, 12),
            (1, 1, Near, 9),
            (-1, 1, Near, 10),
        ],
        // Edges: each side pushes its own class outward.
        3 => &[(-1, 0, Near, 6), (1, 0, Far, 3)],
        4 => &[(0, -1, Far, 4), (0, 1, Near, 5)],
        5 => &[(0, -1, Near, 4), (0, 1, Far, 5)],
        6 => &[(-1, 0, Far, 6), (1, 0, Near, 3)],
        // Odd corner: the three neighbours away from it take the majority.
        9 | 13 => &[(1, 0, Major, 3), (0, 1, Major, 5), (1, 1, Major, 9)],
        10 | 14 => &[(-1, 0, Major, 6), (0, 1, Major, 5), (-1, 1, Major, 10)],
        11 | 15 => &[(-1, 0, Major, 6), (0, -1, Major, 4), (-1, -1, Major, 11)],
        12 | 16 => &[(1, 0, Major, 3), (0, -1, Major, 4), (1, -1, Major, 12)],
        _ => &[],
    }
}

/// Which of a cell's classes a spread entry carries.
#[derive(Debug, Clone, Copy)]
enum Side {
    Near,
    Far,
    /// Class of the three non-odd corners: near on 13–16, far on 9–12.
    Major,
}

/// A grid of cells a class can be painted onto.
pub trait Paintable {
    /// The cell at these coordinates, or `None` off the map.
    fn cell(&self, at: (i32, i32)) -> Option<Cell>;
    /// Write one back.
    fn set(&mut self, at: (i32, i32), cell: Cell);
}

/// Paints a class into a cell, then cascades if it moved.
///
/// **Binary** — map paint point. `corners` 0 claims the
/// whole cell.
pub fn point(ground: &mut impl Paintable, at: (i32, i32), class: u8, corners: u8) {
    let Some(mut cell) = ground.cell(at) else {
        return;
    };

    if paint(&mut cell, class, corners) {
        ground.set(at, cell);
        cascade(ground, at, class, cell.case, 0);
    }
}

/// Cascade depth cap.
///
/// Engine has none and needs none. Guards against a table misreading; far above
/// the two rings a paint reaches.
const DEPTH: u32 = 16;

/// Pushes a class to the neighbours a case obliges, recursing where one moved.
fn cascade(ground: &mut impl Paintable, at: (i32, i32), class: u8, case: u8, depth: u32) {
    if depth >= DEPTH {
        return;
    }

    let Some(here) = ground.cell(at) else {
        return;
    };

    for (dx, dy, side, corners) in spread(case) {
        let to = (at.0 + dx, at.1 + dy);
        let Some(mut neighbour) = ground.cell(to) else {
            continue;
        };
        let carried = match side {
            Side::Near => here.near(),
            Side::Far => here.far(),
            Side::Major => {
                if (13..=16).contains(&case) {
                    here.near()
                } else {
                    here.far()
                }
            }
        };

        let _ = class;

        if paint(&mut neighbour, carried, *corners) {
            ground.set(to, neighbour);
            cascade(ground, to, carried, neighbour.case, depth + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Cell, resolve};

    /// Dirt against a child resolves to Dirt against the child's parent.
    #[test]
    fn dirt_appears_where_a_child_meets_bare_ground() {
        // Class 8 is a child, 7 its parent, 0 Dirt.
        let child = Cell::uniform(8);
        let got = resolve(child, 0, 0);

        assert_eq!(got.pair, 0x07, "Dirt against the child's parent");

        // A uniform cell keeps its mask even when the pair is derived.
        assert!(got.keep, "a uniform cell should keep its corners");
        assert!(
            !resolve(
                Cell {
                    pair: 0x07,
                    case: 4
                },
                9,
                0
            )
            .keep,
            "a cell already blended should drop them"
        );

        // Parent blends with Dirt directly.
        let parent = Cell::uniform(7);

        assert_eq!(resolve(parent, 0, 0).pair, 0x07, "Dirt against a parent");

        // Class already present: no change.
        assert_eq!(resolve(child, 8, 0).pair, child.pair);
        assert!(resolve(child, 8, 0).keep);
    }

    /// The engine's far masks and the viewer's slot table
    /// (`content::terrain::schema`) name the same corners for cases 1 to 16.
    /// Kept apart: one is the binary's table, the other the slots as the
    /// browser shows them.
    #[test]
    fn far_masks_match_the_corner_slots() {
        use crate::content::terrain::schema::{Corners, Schema};

        // Engine bit order (b3 TL, b2 TR, b1 BR, b0 BL) to the viewer's.
        let viewer = |mask: u8| {
            [
                (0b1000, Corners::TOP_LEFT),
                (0b0100, Corners::TOP_RIGHT),
                (0b0010, Corners::BOTTOM_RIGHT),
                (0b0001, Corners::BOTTOM_LEFT),
            ]
            .into_iter()
            .filter(|(bit, _)| mask & bit != 0)
            .fold(Corners::NONE, |all, (_, c)| all.with(c))
        };

        for case in 1..=16 {
            let slot = Schema::Corner.corners(case).map(|(c, _)| c);

            assert_eq!(slot, Some(viewer(super::MASKS[1][case])), "case {case}");
        }
    }
}
