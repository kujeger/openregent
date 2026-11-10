//! A weighted pick over a table of codes.
//!
//! **Binary** — distribution build: entries of stride
//! `0x1c`, code at `+4`, weight at `+8`.
//!
//! Distribution pick sums the weights, draws one
//! `rng_next(total)` in distribution pick, and walks to the bucket. One draw
//! per pick, always.

use crate::data::fourcc::FourCc;

use crate::sim::mapgen::rng::Rng;

/// A weighted table and its pick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Distribution {
    entries: Vec<(FourCc, u32)>,
}

impl Distribution {
    #[must_use]
    pub const fn new(entries: Vec<(FourCc, u32)>) -> Self {
        Distribution { entries }
    }

    /// The draw's bound.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.entries.iter().map(|(_, w)| w).sum()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The codes, in table order.
    pub fn codes(&self) -> impl DoubleEndedIterator<Item = FourCc> + '_ {
        self.entries.iter().map(|(id, _)| *id)
    }

    /// Put a code in the table.
    ///
    /// **Binary** — distribution entry new and distribution push, used by the region
    /// chart's block phase.
    pub fn add(&mut self, code: FourCc, weight: u32) {
        self.entries.push((code, weight));
    }

    /// Entry count.
    ///
    /// **Binary** — distribution count.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// The code of entry `n`, without a draw.
    ///
    /// **Binary** — distribution at; out of range returns a static empty entry.
    /// The surface chart reads entries 0 (base) and 1 (minority).
    #[must_use]
    pub fn at(&self, n: usize) -> Option<FourCc> {
        self.entries.get(n).map(|(id, _)| *id)
    }

    /// Take a code out of the table.
    ///
    /// **Binary** — distribution drop, called by the
    /// region chart's grow loop when a drawn zone has no region left. The
    /// erase keeps order.
    ///
    /// The scan advances both cursors on an erase,
    /// so after removing entry *i* it next checks old entry *i+2*: one of two
    /// adjacent duplicates survives. Inert on shipped data, which has no
    /// duplicates (the block phase checks distribution holds).
    pub fn drop(&mut self, code: FourCc) {
        let mut at = 0;

        while at < self.entries.len() {
            if self.entries[at].0 == code {
                self.entries.remove(at);
            }

            at += 1;
        }
    }

    /// One pick, costing exactly one draw.
    ///
    /// A bucket is taken when running sum `>=` draw (a signed compare
    /// in distribution pick). Weights 76 and 25 give the first entry draws 0 to 76
    /// of 0 to 100: 77 of 101. Distribution pick index does the same.
    /// With `>` the terrain chart matched 65.2% of cells; `>=` matches 92.2%.
    ///
    /// No bucket found (total weight zero, e.g. `#Swd:0`) falls back to the
    /// first entry. The draw is spent either way.
    ///
    /// `None` only on an empty table.
    ///
    /// # Panics
    ///
    /// If the weights exceed the engine's `i32`.
    pub fn pick(&self, rng: &mut Rng) -> Option<FourCc> {
        self.pick_entry(rng).map(|n| self.entries[n].0)
    }

    /// The same pick, as the entry's position in the table.
    ///
    /// **Binary** — distribution pick returns the entry; mobject kind
    /// reads its code and byte. Entries can share a code and
    /// differ in that byte (`%sno_g-tree_02` names `NGa1` twice), so the
    /// position is needed.
    ///
    /// # Panics
    ///
    /// If the weights exceed the engine's `i32`.
    pub fn pick_entry(&self, rng: &mut Rng) -> Option<usize> {
        let landed = self.walk(rng);

        landed.or_else(|| (!self.entries.is_empty()).then_some(0))
    }

    /// The same pick, as an index rather than a code.
    ///
    /// **Binary** — distribution pick index, used by the
    /// landscape chart; same `>=` test in distribution pick index. Returns 0 with no
    /// bucket, even on an empty table, so no `Option`.
    ///
    /// # Panics
    ///
    /// If the weights exceed the engine's `i32`.
    pub fn pick_index(&self, rng: &mut Rng) -> usize {
        self.walk(rng).unwrap_or(0)
    }

    /// One draw and its bucket; `None` if the walk runs off the end. An empty
    /// table still costs the draw.
    fn walk(&self, rng: &mut Rng) -> Option<usize> {
        // Signed, as in the engine.
        let total = i32::try_from(self.total()).expect("a table's weights fit in an i32");
        let draw = rng.below(total);
        let mut seen = 0;

        self.entries.iter().position(|(_, weight)| {
            seen += i32::try_from(*weight).expect("a weight fits in an i32");

            draw <= seen
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A weighted pick costs one draw and lands in proportion.
    #[test]
    fn a_weighted_pick_costs_one_draw_and_lands_in_proportion() {
        /// `#Swamp_Mostly_Child` out of `mx_constants.rgs`.
        const MIX: [(&[u8; 4], u32); 2] = [(b"GR08", 76), (b"GR07", 25)];

        let table = Distribution::new(
            MIX.iter()
                .map(|(id, w)| (FourCc::new(id), *w))
                .collect::<Vec<_>>(),
        );

        let mut rng = Rng::new(1);
        let mut counter = Rng::new(1);
        let mut first = 0;

        for _ in 0..100_000 {
            first += usize::from(table.pick(&mut rng) == Some(FourCc::new(MIX[0].0)));
            counter.step();
        }

        assert_eq!(rng.state, counter.state, "a pick is not one draw");

        // 77 of 101, within a point (the `>=` bucket test).
        let want = 77 * 100_000 / 101;
        assert!(
            first.abs_diff(want) < 1_000,
            "{first} of 100,000 took the first bucket, against {want}"
        );
    }

    /// A table of zero weights still costs a draw, and takes the first entry.
    ///
    /// Shipped: `#Swd:0` has one entry at weight zero.
    #[test]
    fn a_table_of_no_weight_spends_a_draw_and_takes_the_first() {
        let table = Distribution::new(vec![(FourCc::new(b"GR08"), 0), (FourCc::new(b"GR07"), 0)]);

        let mut rng = Rng::new(1);
        let mut counter = Rng::new(1);

        assert_eq!(table.pick(&mut rng), Some(FourCc::new(b"GR08")));
        assert_eq!(rng.state, counter.step(), "the draw was not spent");
    }

    /// A drop removes one of two adjacent duplicates, and both of two apart.
    ///
    /// See [`Distribution::drop`].
    #[test]
    fn a_drop_steps_over_the_entry_that_shifted_down() {
        let (a, b) = (FourCc::new(b"GR08"), FourCc::new(b"GR07"));

        let mut adjacent = Distribution::new(vec![(a, 1), (a, 2), (b, 3)]);

        adjacent.drop(a);
        assert_eq!(adjacent.codes().collect::<Vec<_>>(), vec![a, b]);

        let mut apart = Distribution::new(vec![(a, 1), (b, 2), (a, 3)]);

        apart.drop(a);
        assert_eq!(apart.codes().collect::<Vec<_>>(), vec![b]);
    }

    /// An empty table picks nothing and still spends a draw.
    #[test]
    fn an_empty_table_picks_nothing() {
        let table = Distribution::default();
        let mut rng = Rng::new(1);

        assert!(table.is_empty());
        assert_eq!(table.pick(&mut rng), None);
        assert_eq!(rng.state, Rng::new(1).step());
    }
}
