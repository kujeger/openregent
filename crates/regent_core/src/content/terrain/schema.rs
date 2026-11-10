//! What a blend set's seventeen slots mean.
//!
//! **Viewer** (painted boards, seams inspected) and **Corpus** (saves store a
//! cell's slot number). Fifteen non-empty corner arrangements, plus a second
//! slot per diagonal.
//!
//! ```text
//!   TL ── TR      slot 13 = {TL}          slot 4 = {TL,TR}
//!    │     │      slot  1 = {TR,BL}       slot 9 = all but TL
//!   BL ── BR      slot  0 = all four
//! ```
//!
//! So terrain lives on vertices; see [`super::board`].
//!
//! # The evidence
//!
//! Four boards, each predicting one case family. Predictions are tests in
//! [`super::board`]; that pictures join is checked by eye.
//!
//! | Board | Predicts | Found |
//! |---|---|---|
//! | one far vertex alone | the four single-corner cases, closing into one patch | a closed patch |
//! | a straight boundary | one edge case, unbroken along its run | one unbroken line |
//! | alternating vertices | the two diagonals, and nothing else | the two diagonals |
//! | a filled disc | every edge, corner and three-corner case, outline unbroken | unbroken |
//!
//! The first board fixes orientation: the other seven [`Turn`]s leave a hole,
//! so canvas corners are board corners ([`Turn::NONE`]).
//!
//! [`Join`] is real: a diagonal draws as two islands or a band, and switching
//! changes a checkerboard. Hence seventeen slots, not fifteen.
//!
//! # Open
//!
//! - Only `MapTextureAtoB` was checked by eye. Every blend set in both
//!   editions has seventeen slots, so the numbering is shared — **Inferred**,
//!   not seen.
//! - Slot 0 is unreachable in practice: four agreeing corners draw the body
//!   set. On eight of ten shipped sets slot 0 shares a tile with the
//!   destination body's last variant.
//! - What picks a join at run time is unknown.
//!
//! # Canvas, not board
//!
//! [`Corners`] is canvas space: `TOP_LEFT` is the 32x32 picture's top left.
//! Checked against this crate's board (x right, y down). Board to screen is
//! also [`Turn::NONE`] ([`super::compass`]).

/// Which corners of a tile take the far terrain, in canvas space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Corners(pub u8);

impl Corners {
    pub const TOP_LEFT: Corners = Corners(0b0001);
    pub const TOP_RIGHT: Corners = Corners(0b0010);
    pub const BOTTOM_LEFT: Corners = Corners(0b0100);
    pub const BOTTOM_RIGHT: Corners = Corners(0b1000);

    /// No corner: the near terrain's body set, not a blend.
    pub const NONE: Corners = Corners(0);

    #[must_use]
    pub const fn with(self, other: Corners) -> Self {
        Corners(self.0 | other.0)
    }

    #[must_use]
    pub const fn has(self, other: Corners) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub const fn count(self) -> u32 {
        self.0.count_ones()
    }

    /// All sixteen arrangements, empty first.
    pub fn all() -> impl Iterator<Item = Corners> {
        (0..=0b1111).map(Corners)
    }
}

/// A rotation/mirror between corner frames: four quarter turns, each with or
/// without a mirror.
///
/// Canvas to board and board to screen
/// ([`super::compass`]) are both [`Turn::NONE`]. Kept so
/// frames are stated, and the other seven can test a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Turn {
    /// Quarter turns clockwise, 0 to 3.
    pub quarters: u8,
    pub mirror: Mirror,
}

/// Whether a turn also flips left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirror {
    Keep,
    Flip,
}

impl Turn {
    /// Identity.
    pub const NONE: Turn = Turn {
        quarters: 0,
        mirror: Mirror::Keep,
    };

    /// All eight, in fixed order.
    pub fn all() -> impl Iterator<Item = Turn> {
        [Mirror::Keep, Mirror::Flip]
            .into_iter()
            .flat_map(|mirror| (0..4).map(move |quarters| Turn { quarters, mirror }))
    }

    /// Apply this turn to an arrangement.
    #[must_use]
    pub fn apply(self, corners: Corners) -> Corners {
        let mirrored = match self.mirror {
            Mirror::Keep => corners,
            Mirror::Flip => swap(corners, Corners::TOP_LEFT, Corners::TOP_RIGHT)
                .pipe(|c| swap(c, Corners::BOTTOM_LEFT, Corners::BOTTOM_RIGHT)),
        };

        (0..self.quarters).fold(mirrored, |c, _| quarter(c))
    }

    pub fn name(self) -> String {
        let turn = u16::from(self.quarters) * 90;

        match self.mirror {
            Mirror::Keep => format!("{turn}"),
            Mirror::Flip => format!("{turn} flipped"),
        }
    }
}

/// One quarter turn clockwise: TL to TR to BR to BL.
const fn quarter(c: Corners) -> Corners {
    let mut out = 0;

    if c.has(Corners::TOP_LEFT) {
        out |= Corners::TOP_RIGHT.0;
    }
    if c.has(Corners::TOP_RIGHT) {
        out |= Corners::BOTTOM_RIGHT.0;
    }
    if c.has(Corners::BOTTOM_RIGHT) {
        out |= Corners::BOTTOM_LEFT.0;
    }
    if c.has(Corners::BOTTOM_LEFT) {
        out |= Corners::TOP_LEFT.0;
    }

    Corners(out)
}

/// Exchange two corners of an arrangement.
const fn swap(c: Corners, a: Corners, b: Corners) -> Corners {
    let mut out = c.0 & !(a.0 | b.0);

    if c.has(a) {
        out |= b.0;
    }
    if c.has(b) {
        out |= a.0;
    }

    Corners(out)
}

/// Lets [`Turn::apply`] chain instead of nest.
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl Pipe for Corners {}

/// How the two far corners of a diagonal meet.
///
/// **Viewer**: on a checkerboard `Apart` draws islands, `Through` bands; the
/// two spare slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Join {
    /// The two corners stay separate.
    Apart,
    /// They meet through the middle.
    Through,
}

impl Join {
    pub const ALL: [(Join, &'static str); 2] = [(Join::Apart, "apart"), (Join::Through, "through")];
}

/// How a blend set's slots are numbered.
///
/// An enum so an alternative numbering (a mod, another tileset) can sit
/// beside this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schema {
    /// Four corners, fifteen arrangements, two spare slots for the diagonals.
    #[default]
    Corner,
}

/// Slot table, indexed by slot. Written out so it reads against the browser's
/// pictures.
const CORNER_SLOTS: [(u8, Join); 17] = [
    (0b1111, Join::Apart),   //  0  all four
    (0b0110, Join::Apart),   //  1  TR BL
    (0b1001, Join::Apart),   //  2  TL BR
    (0b1010, Join::Apart),   //  3  TR BR   right edge
    (0b0011, Join::Apart),   //  4  TL TR   top edge
    (0b1100, Join::Apart),   //  5  BL BR   bottom edge
    (0b0101, Join::Apart),   //  6  TL BL   left edge
    (0b1001, Join::Through), //  7  TL BR  again
    (0b0110, Join::Through), //  8  TR BL  again
    (0b1110, Join::Apart),   //  9  all but TL
    (0b1101, Join::Apart),   // 10  all but TR
    (0b0111, Join::Apart),   // 11  all but BR
    (0b1011, Join::Apart),   // 12  all but BL
    (0b0001, Join::Apart),   // 13  TL
    (0b0010, Join::Apart),   // 14  TR
    (0b1000, Join::Apart),   // 15  BR
    (0b0100, Join::Apart),   // 16  BL
];

impl Schema {
    /// Slot drawing this arrangement. `None` for empty (body set).
    #[must_use]
    pub fn slot(self, want: Corners, join: Join) -> Option<usize> {
        let Schema::Corner = self;

        // Exact join first, else the other: `Through` on a non-diagonal still
        // gets a slot.
        let matching = |j: Join| {
            CORNER_SLOTS
                .iter()
                .position(|(mask, slot_join)| *mask == want.0 && *slot_join == j)
        };

        matching(join).or_else(|| matching(other(join)))
    }

    /// The arrangement a slot claims to draw.
    #[must_use]
    pub fn corners(self, slot: usize) -> Option<(Corners, Join)> {
        let Schema::Corner = self;

        CORNER_SLOTS
            .get(slot)
            .map(|(mask, join)| (Corners(*mask), *join))
    }

    /// Slot count.
    #[must_use]
    pub const fn slots(self) -> usize {
        let Schema::Corner = self;

        CORNER_SLOTS.len()
    }
}

const fn other(join: Join) -> Join {
    match join {
        Join::Apart => Join::Through,
        Join::Through => Join::Apart,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// An unreachable arrangement would leave the generator without a tile.
    #[test]
    fn every_arrangement_but_the_empty_one_has_a_slot() {
        let reachable: BTreeSet<Corners> = Corners::all()
            .filter(|c| Schema::Corner.slot(*c, Join::Apart).is_some())
            .collect();

        assert_eq!(reachable.len(), 15, "fifteen non-empty arrangements");
        assert!(
            !reachable.contains(&Corners::NONE),
            "the empty arrangement is the body set, not a case"
        );
    }

    #[test]
    fn every_slot_is_reachable() {
        let used: BTreeSet<usize> = Corners::all()
            .flat_map(|c| {
                [Join::Apart, Join::Through]
                    .into_iter()
                    .filter_map(move |j| Schema::Corner.slot(c, j))
            })
            .collect();

        assert_eq!(used.len(), 17, "no slot is dead: {used:?}");
    }

    #[test]
    fn each_diagonal_has_two_slots_differing_only_in_their_join() {
        for diagonal in [
            Corners::TOP_LEFT.with(Corners::BOTTOM_RIGHT),
            Corners::TOP_RIGHT.with(Corners::BOTTOM_LEFT),
        ] {
            let apart = Schema::Corner.slot(diagonal, Join::Apart).unwrap();
            let through = Schema::Corner.slot(diagonal, Join::Through).unwrap();

            assert_ne!(apart, through, "{diagonal:?} needs two slots");
            assert_eq!(Schema::Corner.corners(apart).unwrap().0, diagonal);
            assert_eq!(Schema::Corner.corners(through).unwrap().0, diagonal);
        }
    }

    #[test]
    fn a_single_corner_case_exists_which_no_edge_scheme_can_produce() {
        // A per-edge scheme cannot draw a lone far corner. Four slots do,
        // refuting the edge reading.
        let singles = (0..Schema::Corner.slots())
            .filter_map(|s| Schema::Corner.corners(s))
            .filter(|(c, _)| c.count() == 1)
            .count();

        assert_eq!(singles, 4, "one slot per corner alone");
    }

    #[test]
    fn a_quarter_turn_four_times_is_no_turn_at_all() {
        for c in Corners::all() {
            let turned = (0..4).fold(c, |c, _| quarter(c));

            assert_eq!(turned, c, "{c:?} did not come back");
        }
    }

    #[test]
    fn a_turn_permutes_the_corners_rather_than_losing_them() {
        // A dropped or duplicated bit would make arrangements unreachable.
        for turn in Turn::all() {
            for c in Corners::all() {
                assert_eq!(
                    turn.apply(c).count(),
                    c.count(),
                    "{} changed the corner count of {c:?}",
                    turn.name()
                );
            }

            let images: BTreeSet<Corners> = Corners::all().map(|c| turn.apply(c)).collect();

            assert_eq!(images.len(), 16, "{} is not a bijection", turn.name());
        }
    }

    #[test]
    fn there_are_eight_turns_and_they_are_distinct() {
        let turns: Vec<Turn> = Turn::all().collect();

        assert_eq!(turns.len(), 8);

        // Distinct as permutations, not just names.
        let behaviours: BTreeSet<Vec<Corners>> = turns
            .iter()
            .map(|t| Corners::all().map(|c| t.apply(c)).collect())
            .collect();

        assert_eq!(behaviours.len(), 8, "two turns do the same thing");
    }

    #[test]
    fn a_quarter_turn_moves_the_top_left_to_the_top_right() {
        // Pins "clockwise".
        assert_eq!(quarter(Corners::TOP_LEFT), Corners::TOP_RIGHT);
        assert_eq!(quarter(Corners::TOP_RIGHT), Corners::BOTTOM_RIGHT);
        assert_eq!(quarter(Corners::BOTTOM_RIGHT), Corners::BOTTOM_LEFT);
        assert_eq!(quarter(Corners::BOTTOM_LEFT), Corners::TOP_LEFT);
    }
}
