//! The sepia faces behind the menus, made from the units' portraits in
//! place of `INse` 1023 enlarged (`docs/openregent/enhancements.md`): the original's
//! grid and tone (`regent_core::view::menu::faces`), one portrait a cell,
//! at the interface's scale: the size the game's selection panel draws
//! a portrait.
//!
//! ```text
//!  pool ──shuffle(seed)──> [f0 f1 f2 … fn-1]
//!  row 0:  f0      f1      f2  …
//!  row 1:  fS      fS+1    fS+2 …     S = ROW_STEP, wrapping
//!  row 2:  f2S     …
//! ```
//!
//! A cell's face rests on the seed and its place alone: a resize keeps
//! every face where it was, and neighbours in a row never repeat.

use regent_core::data::fourcc::FourCc;

use super::art::Pic;
use crate::draw::shot::Capture;

/// How far on in the shuffled pool each row starts: well past a row's
/// width, so the row under shows other faces.
const ROW_STEP: usize = 37;

/// The portraits the faces draw on, shuffled by the seed.
#[derive(Default)]
pub struct Faces {
    order: Vec<Pic>,
}

impl Faces {
    /// `pool` in the order `seed` gives.
    #[must_use]
    pub fn new(mut pool: Vec<Pic>, seed: u32) -> Faces {
        shuffle(&mut pool, seed);

        Faces { order: pool }
    }

    /// The portrait of the cell `column` across and `row` down from the
    /// window's top left; `None` with no portraits read.
    #[must_use]
    pub fn at(&self, column: usize, row: usize) -> Option<Pic> {
        let n = self.order.len();

        (n > 0).then(|| self.order[(column + row * ROW_STEP) % n])
    }
}

/// `items` shuffled, Fisher-Yates on a splitmix64 stream from `seed`.
fn shuffle<T>(items: &mut [T], seed: u32) {
    let mut state = u64::from(seed);
    let mut next = || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);

        let z = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);

        z ^ (z >> 31)
    };

    for i in (1..items.len()).rev() {
        #[allow(clippy::cast_possible_truncation, reason = "below items.len()")]
        let j = (next() % (i as u64 + 1)) as usize;

        items.swap(i, j);
    }
}

/// The seed a run without `--faces-seed` takes: a screenshot's fixed,
/// else the clock's.
#[must_use]
pub fn seed(given: Option<u32>, run: Capture) -> u32 {
    match (given, run) {
        (Some(seed), _) => seed,
        (None, Capture::Shot) => 0,
        #[allow(clippy::cast_sign_loss, reason = "any bits serve")]
        (None, Capture::Played) => super::freestyle::clock() as u32,
    }
}

/// `code`'s portrait as the menus' art names it.
#[must_use]
pub const fn portrait(code: FourCc) -> Pic {
    Pic::of(code, regent_core::view::menu::faces::PORTRAIT_SET)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faces(n: u8, seed: u32) -> Faces {
        let pool = (0..n)
            .map(|i| portrait(FourCc([b'A', b'V', b'a', i])))
            .collect();

        Faces::new(pool, seed)
    }

    #[test]
    fn a_seed_gives_the_same_faces_every_time() {
        let (a, b) = (faces(90, 7), faces(90, 7));

        assert!((0..20).all(|c| a.at(c, 3) == b.at(c, 3)));
    }

    #[test]
    fn another_seed_gives_other_faces() {
        let (a, b) = (faces(90, 7), faces(90, 8));

        assert!((0..20).any(|c| a.at(c, 0) != b.at(c, 0)));
    }

    #[test]
    fn neighbours_differ_across_and_down() {
        let f = faces(90, 1);

        for row in 0..12 {
            for column in 0..30 {
                assert_ne!(f.at(column, row), f.at(column + 1, row));
                assert_ne!(f.at(column, row), f.at(column, row + 1));
            }
        }
    }

    #[test]
    fn no_portraits_no_faces() {
        assert_eq!(Faces::default().at(0, 0), None);
    }
}
