//! The region chart: which zone owns which cell.
//!
//! **Binary** — region chart build, the third chart.
//! Gives the heightfield its zone ownership.
//!
//! ```text
//! fill every cell NONE
//! plant the seeds                        -- a shuffle, then a plus per seed
//! shuffle every point of the map         -- the order a stuck region rehomes in
//! while some cell is still NONE:
//!     if no region is left:
//!         give every NONE cell its first non-NONE neighbour
//!     else:
//!         code = distribution.pick()     -- one draw
//!         pick one of that zone's records at random   -- one draw
//!         grow it by a cell
//!         if it had nowhere left to go, rehome it or drop it
//! ```
//!
//! Two draws an iteration. Ring geometry and existing neighbours decide the
//! rest.
//!
//! # A blob grows on a ring, not on its own border
//!
//! A [`Record`] keeps its seed's centre and a pass count; its frontier is the
//! circle of that radius about the centre. Each pass walks the circle once,
//! then the radius grows by one. The pass count is radius and age.
//!
//! Below pass 2 a cell needs one of its eight neighbours to carry the code;
//! from 2 on, two adjacent ones, except on the map's edge.

use crate::data::fourcc::FourCc;

use crate::base::fixed::{self, ONE};
use crate::sim::mapgen::pick::Distribution;
use crate::sim::mapgen::rng::Rng;
use crate::sim::mapgen::shuffle::shuffled_points;
use crate::sim::trig;

/// The sentinel an unclaimed cell holds.
pub const NONE: FourCc = FourCc::new(b"NONE");

/// The eight neighbours, in the order both walks take them.
///
/// **Binary** — a switch, read three times. From top left, clockwise with `+y` down.
pub const RING: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
];

/// Ring samples per `r + 1`.
///
/// **Binary** — times 7 in region frontier rebuild.
const SAMPLES_PER_RADIUS: i32 = 7;

/// Seed lattice spacing in cells.
///
/// **Binary** — divisions by `0x66666667` in region seed place.
pub const LATTICE: u32 = 10;

/// Placement grid width in blocks.
pub const BLOCKS: i32 = 5;

/// The share a block's own ground gets in the zone table.
///
/// **Binary** — `0x14` in region seed place, used when the land does not
/// already name the code.
pub const BLOCK_WEIGHT: u32 = 20;

/// A growing region: one seed, and the ring it is working outwards along.
#[derive(Debug, Clone)]
pub struct Record {
    /// The seed's cell; the ring's centre.
    pub centre: (i32, i32),
    pub code: FourCc,
    /// The byte. Set on block-phase records; `Chart::rehome`
    /// will not move them.
    pub fixed: bool,
    /// The ring, in the order it is walked.
    frontier: Vec<(i32, i32)>,
    /// Walk position on the ring,.
    cursor: usize,
    /// Rebuild count and radius,.
    pass: i32,
}

impl Record {
    const fn new(centre: (i32, i32), code: FourCc, seeded: Seeded) -> Self {
        Record {
            centre,
            code,
            fixed: matches!(seeded, Seeded::ByBlock),
            frontier: Vec::new(),
            cursor: 0,
            pass: 0,
        }
    }

    /// Refill the ring, and say whether the region is spent.
    ///
    /// **Binary** — region frontier rebuild. Spent once
    /// the pass count exceeds a third of the width, or if the whole ring is
    /// off the map.
    ///
    /// `7 * (r + 1)` even angles from straight down. A sample equal to the one
    /// before is dropped; equal samples further apart are both kept, so a cell
    /// can appear twice.
    fn rebuild(&mut self, side: i32) -> bool {
        if side / 3 < self.pass {
            return true;
        }

        self.cursor = 0;
        self.frontier.clear();

        let (cx, cy) = self.centre;
        let r = self.pass;

        // Angle zero; the loop's first sample repeats it and is dropped.
        self.offer((cx, cy + r), side);

        let samples = (r + 1) * SAMPLES_PER_RADIUS;
        let step = fixed::div(2 * trig::PI, samples * ONE);
        let mut previous = (0, r);

        for i in 0..samples {
            let angle = fixed::mul(i * ONE, step);
            let at = (
                fixed::whole(fixed::mul(r * ONE, trig::sin(angle))),
                fixed::whole(fixed::mul(r * ONE, trig::cos(angle))),
            );

            if at != previous {
                self.offer((cx + at.0, cy + at.1), side);
            }

            // Updated even when the sample is dropped.
            previous = at;
        }

        self.frontier.is_empty()
    }

    /// Take a ring sample, if it is on the map.
    fn offer(&mut self, at: (i32, i32), side: i32) {
        if (0..side).contains(&at.0) && (0..side).contains(&at.1) {
            self.frontier.push(at);
        }
    }
}

/// The chart: a code per cell, and the regions still growing into it.
#[derive(Debug, Clone)]
pub struct Chart {
    side: i32,
    cells: Vec<FourCc>,
    /// Cells not [`NONE`],.
    filled: i32,
    records: Vec<Record>,
}

impl Chart {
    /// An empty square chart.
    ///
    /// Square: region neighbour count and region seed stamp test
    /// `y` against the width.
    ///
    /// # Panics
    ///
    /// On a side beyond the engine's `i32`.
    #[must_use]
    pub fn new(side: u32) -> Self {
        let cells = (side as usize) * (side as usize);
        let side = i32::try_from(side).expect("a map side fits in the engine's i32");

        Chart {
            side,
            cells: vec![NONE; cells],
            filled: 0,
            records: Vec::new(),
        }
    }

    #[must_use]
    pub const fn side(&self) -> i32 {
        self.side
    }

    /// What a cell holds, or `None` off the map.
    #[must_use]
    pub fn at(&self, at: (i32, i32)) -> Option<FourCc> {
        let (x, y) = at;

        if (0..self.side).contains(&x) && (0..self.side).contains(&y) {
            self.cells.get(self.slot((x, y))).copied()
        } else {
            None
        }
    }

    /// Write a cell, keeping the filled count.
    ///
    /// **Binary** — region set cell, inlined at five
    /// other sites. Writing [`NONE`] back decrements the count.
    ///
    /// Public so a caller can build a zone map from elsewhere, e.g. a save.
    pub fn set(&mut self, at: (i32, i32), code: FourCc) {
        let slot = self.slot(at);

        self.filled += i32::from(self.cells[slot] == NONE);
        self.filled -= i32::from(code == NONE);
        self.cells[slot] = code;
    }

    /// Row-major index of an on-map cell.
    #[allow(
        clippy::cast_sign_loss,
        reason = "every caller has bounded the point against the side first"
    )]
    const fn slot(&self, at: (i32, i32)) -> usize {
        (at.1 * self.side + at.0) as usize
    }

    /// Plant one seed: the centre, and its free orthogonal neighbours.
    ///
    /// **Binary** — region seed stamp. Always succeeds.
    pub fn plant(&mut self, centre: (i32, i32), code: FourCc, fixed: Seeded) {
        let (x, y) = centre;

        self.set(centre, code);

        for side in [(x, y - 1), (x, y + 1), (x - 1, y), (x + 1, y)] {
            if self.at(side) == Some(NONE) {
                self.set(side, code);
            }
        }

        self.records.push(Record::new(centre, code, fixed));
    }

    /// A region record with no plus stamped.
    fn add_record(&mut self, centre: (i32, i32), code: FourCc, pass: i32) {
        let mut record = Record::new(centre, code, Seeded::ByBlock);

        record.pass = pass;
        self.records.push(record);
    }

    /// Plant the lattice seeds, in shuffled order.
    ///
    /// **Binary** — region seed place. Shuffles a
    /// `side/10` square lattice (its only draws), then walks zones in table
    /// order, one lattice point times ten per seed.
    ///
    /// `seeds` is the region item's `words[0]` plus one. The engine retries a
    /// taken point; it never needs to, points being ten cells apart.
    ///
    /// # Panics
    ///
    /// If the side does not fit in a `u32`.
    pub fn seed(&mut self, zones: &[(FourCc, u32)], rng: &mut Rng) {
        let across = u32::try_from(self.side).expect("a side this chart was built with") / LATTICE;
        let mut points = shuffled_points(rng, across, across).into_iter();

        for (code, seeds) in zones {
            for _ in 0..*seeds {
                let Some((x, y)) = points.next() else {
                    return;
                };
                let at = (x * LATTICE, y * LATTICE);
                let at = (
                    i32::try_from(at.0).expect("a lattice point"),
                    i32::try_from(at.1).expect("a lattice point"),
                );

                self.plant(at, *code, Seeded::ByLattice);
            }
        }
    }

    /// Fill a placed block with its own ground, and seed a region there.
    ///
    /// **Binary** — second phase of region seed place,.
    /// For a placed block whose unit pattern carries `alternate`:
    ///
    /// * writes `code` (the pattern's tag, see [`crate::sim::mapgen::setup::stamps`]) over
    ///   every free cell of the block;
    /// * adds `code` to the zone table at [`BLOCK_WEIGHT`] if absent;
    /// * adds a record at the block centre with pass count half a block, so
    ///   its first ring is the block's edge.
    ///
    /// The record is flagged: dropped, not rehomed, once spent. No
    /// draws.
    pub fn stamp_block(&mut self, block: (i32, i32), code: FourCc, zones: &mut Distribution) {
        let step = self.side / BLOCKS;
        let origin = (block.0 * step, block.1 * step);

        for x in origin.0..origin.0 + step {
            for y in origin.1..origin.1 + step {
                if self.at((x, y)) == Some(NONE) {
                    self.set((x, y), code);
                }
            }
        }

        if !zones.codes().any(|held| held == code) {
            zones.add(code, BLOCK_WEIGHT);
        }

        // The fill wrote the centre; no plus. Region record set pass sets the pass.
        self.add_record((origin.0 + step / 2, origin.1 + step / 2), code, step / 2);
    }

    /// Whether a cell may be claimed for `code`.
    ///
    /// **Binary** — region may claim: needs one
    /// neighbour below pass 2 or on the border, else two.
    fn may_claim(&self, at: (i32, i32), code: FourCc, pass: i32) -> bool {
        let edge = at.0 == 0 || at.1 == 0 || at.0 == self.side - 1 || at.1 == self.side - 1;
        let want = if pass < 2 || edge { 1 } else { 2 };

        self.neighbour_run(at, code, want)
    }

    /// Whether `want` of a cell's eight neighbours carry `code` in a row.
    ///
    /// **Binary** — region neighbour count: a run, not
    /// a total. Find the first match, then walk the ring once from past it; a
    /// miss or map edge resets the run; stop when it equals `want`.
    ///
    /// The run starts at 1 and is tested only after a step, so `want` of 1
    /// needs a miss: a cell whose whole ring carries the code fails.
    fn neighbour_run(&self, at: (i32, i32), code: FourCc, want: i32) -> bool {
        let neighbour = |k: usize| self.at((at.0 + RING[k].0, at.1 + RING[k].1));
        let Some(first) = (0..RING.len()).find(|k| neighbour(*k) == Some(code)) else {
            return false;
        };

        let mut run = 1;

        for step in 1..=RING.len() {
            let Some(held) = neighbour((first + step) % RING.len()) else {
                // Off the map resets the run, untested.
                run = 0;
                continue;
            };

            run = if held == code { run + 1 } else { 0 };

            if run == want {
                return true;
            }
        }

        false
    }

    /// The next cell a region will try, advancing its ring as needed.
    ///
    /// **Binary** — region record point over
    /// region frontier next. `None`: spent (rebuild gave
    /// up, or the rest of the ring is other regions').
    fn next_point(&mut self, which: usize) -> Option<(i32, i32)> {
        loop {
            let record = &self.records[which];

            if record.cursor == record.frontier.len() {
                let side = self.side;
                let record = &mut self.records[which];

                record.pass += 1;

                if record.rebuild(side) {
                    return None;
                }
            }

            // Walk to a free cell; dead if none of the rest is ours.
            let (mut found, mut alive) = (None, false);
            let record = &self.records[which];
            let (code, mut cursor) = (record.code, record.cursor);

            while cursor < record.frontier.len() && found.is_none() {
                let at = record.frontier[cursor];

                cursor += 1;

                match self.at(at) {
                    Some(NONE) => (found, alive) = (Some(at), true),
                    held if held == Some(code) => alive = true,
                    _ => (),
                }
            }

            self.records[which].cursor = cursor;

            if !alive {
                return None;
            }

            if let Some(at) = found {
                return Some(at);
            }
        }
    }

    /// Grow one region by a single cell. Says whether it had nowhere to go.
    ///
    /// **Binary** — region grow one cell. If the drawn
    /// code may not claim the cell, the whole distribution is walked with no
    /// early exit: the last zone that may claim it gets it.
    fn grow(&mut self, which: usize, zones: &Distribution) -> Spent {
        let Some(at) = self.next_point(which) else {
            return Spent::Yes;
        };

        let record = &self.records[which];
        let (drawn, pass) = (record.code, record.pass);

        let code = if self.may_claim(at, drawn, pass) {
            Some(drawn)
        } else {
            zones.codes().rfind(|zone| self.may_claim(at, *zone, pass))
        };

        if let Some(code) = code {
            self.set(at, code);
        }

        Spent::No
    }

    /// Move a spent region onto an isolated cell, in the shuffled order.
    ///
    /// **Binary** — region plant seed cell. Takes the
    /// next cell of `order` with all eight neighbours free, moves the record
    /// there and clears its ring. Block-phase records never move.
    fn rehome(&mut self, which: usize, cursor: &mut usize, order: &[(i32, i32)]) -> bool {
        if self.records[which].fixed {
            return false;
        }

        let code = self.records[which].code;

        while *cursor < order.len() {
            let at = order[*cursor];

            *cursor += 1;

            // Eight: the whole ring, so no edge cell qualifies.
            if self.neighbour_run(at, NONE, 8) {
                self.set(at, code);

                let record = &mut self.records[which];

                record.centre = at;
                record.frontier.clear();
                record.cursor = 0;
                record.pass = 0;

                return true;
            }
        }

        false
    }

    /// Give every cell still free the first neighbour that is not.
    ///
    /// **Binary** — inline sweep in region chart build, once no region is left. The
    /// outer loop repeats it until the map is full.
    fn close_holes(&mut self) {
        for x in 0..self.side {
            for y in 0..self.side {
                if self.at((x, y)) != Some(NONE) {
                    continue;
                }

                let taken = RING
                    .iter()
                    .filter_map(|(dx, dy)| self.at((x + dx, y + dy)))
                    .find(|held| *held != NONE);

                if let Some(code) = taken {
                    self.set((x, y), code);
                }
            }
        }
    }

    /// Grow the planted seeds until the map is full.
    ///
    /// `order` is the shuffled point list built right after seeding
    /// (`width * height` draws, spent even if nothing rehomes).
    ///
    /// # Panics
    ///
    /// If the candidate count exceeds `i32`. It cannot.
    pub fn grow_all(&mut self, zones: &mut Distribution, order: &[(i32, i32)], rng: &mut Rng) {
        let total = self.side * self.side;
        let mut cursor = 0;

        while self.filled < total {
            // A sweep that fills nothing would repeat forever: no cell is
            // claimed to spread from. Only on a chart nothing was planted in.
            if self.records.is_empty() {
                let before = self.filled;

                self.close_holes();

                if self.filled == before {
                    break;
                }

                continue;
            }

            let Some(code) = zones.pick(rng) else {
                break;
            };

            let candidates: Vec<usize> = (0..self.records.len())
                .filter(|i| self.records[*i].code == code)
                .collect();

            let Some(count) = i32::try_from(candidates.len()).ok().filter(|n| *n > 0) else {
                zones.drop(code);
                continue;
            };

            let draw = usize::try_from(rng.below(count)).expect("a draw under a positive bound");
            let which = candidates[draw];

            if self.grow(which, zones) == Spent::No || self.filled >= total {
                continue;
            }

            // Spent on its first ring: release the seed cell (centre only; the
            // plus's arms stay) before rehoming or dropping.
            if self.records[which].pass == 1 {
                self.set(self.records[which].centre, NONE);
            }

            if !self.rehome(which, &mut cursor, order) {
                self.records.remove(which);
            }
        }
    }

    /// The chart as one code per cell, row major.
    #[must_use]
    pub fn cells(&self) -> &[FourCc] {
        &self.cells
    }
}

/// Which phase of region seed place planted a seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seeded {
    /// Lattice phase: may be rehomed.
    ByLattice,
    /// Block phase: flag, dropped rather than moved.
    ByBlock,
}

/// Whether a region had anywhere left to grow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spent {
    No,
    Yes,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZONE: FourCc = FourCc::new(b"$Aaa");
    const OTHER: FourCc = FourCc::new(b"$Bbb");

    /// The frontier is a circle at the pass count's radius.
    #[test]
    fn a_frontier_is_a_ring_at_the_pass_counts_radius() {
        let side = 400;

        for radius in 1..64 {
            let mut record = Record::new((200, 200), ZONE, Seeded::ByLattice);

            record.pass = radius;
            assert!(!record.rebuild(side), "radius {radius} came out empty");

            for (x, y) in &record.frontier {
                let (dx, dy) = (x - 200, y - 200);
                let reach = f64::from(dx * dx + dy * dy).sqrt();

                assert!(
                    (reach - f64::from(radius)).abs() < 1.5,
                    "radius {radius}: ({dx}, {dy}) is {reach:.2} out"
                );
            }

            // Gaps: spacing is near one cell, but table quantisation pushes a
            // few steps to two, so a blob can leak through its frontier.
            for pair in record.frontier.windows(2) {
                let step = (pair[1].0 - pair[0].0)
                    .abs()
                    .max((pair[1].1 - pair[0].1).abs());

                assert!(step <= 2, "radius {radius}: the ring jumps {step} cells");
            }
        }
    }

    /// A pass count past a third of the width reports the region spent.
    #[test]
    fn a_ring_that_misses_the_map_reports_the_region_spent() {
        let mut record = Record::new((0, 0), ZONE, Seeded::ByLattice);

        // 20 > 30 / 3.
        record.pass = 20;
        assert!(record.rebuild(30));
    }

    /// Two must be adjacent; a full ring fails a threshold of one.
    #[test]
    fn a_neighbour_run_counts_neighbours_in_a_row() {
        let mut chart = Chart::new(5);
        let middle = (2, 2);

        // One neighbour.
        chart.set((1, 1), ZONE);
        assert!(chart.neighbour_run(middle, ZONE, 1));
        assert!(!chart.neighbour_run(middle, ZONE, 2));

        // Two, opposite.
        chart.set((3, 3), ZONE);
        assert!(!chart.neighbour_run(middle, ZONE, 2));

        // Two, adjacent.
        chart.set((2, 1), ZONE);
        assert!(chart.neighbour_run(middle, ZONE, 2));

        // No gap: fails a threshold of one (run starts at 1, tested after a
        // step).
        let mut full = Chart::new(5);

        for (dx, dy) in RING {
            full.set((2 + dx, 2 + dy), ZONE);
        }

        assert!(!full.neighbour_run(middle, ZONE, 1));
        assert!(full.neighbour_run(middle, ZONE, 2));
        assert!(!full.neighbour_run(middle, OTHER, 1));
    }

    /// A seed is a plus of five; the filled count tracks non-sentinel cells.
    #[test]
    fn a_seed_is_a_plus_of_five_cells() {
        let mut chart = Chart::new(9);

        chart.plant((4, 4), ZONE, Seeded::ByLattice);

        assert_eq!(chart.filled, 5);
        assert_eq!(chart.at((4, 4)), Some(ZONE));
        assert_eq!(chart.at((4, 3)), Some(ZONE));
        assert_eq!(chart.at((3, 4)), Some(ZONE));
        assert_eq!(chart.at((3, 3)), Some(NONE));

        // Writing the sentinel back decrements.
        chart.set((4, 4), NONE);
        assert_eq!(chart.filled, 4);
    }

    /// The whole loop leaves no cell unclaimed.
    #[test]
    fn growing_fills_every_cell() {
        let side = 60;
        let mut chart = Chart::new(side);
        let mut zones = Distribution::new(vec![(ZONE, 60), (OTHER, 40)]);
        let mut rng = Rng::new(1);

        chart.seed(&[(ZONE, 1), (OTHER, 1)], &mut rng);

        let order: Vec<(i32, i32)> = shuffled_points(&mut rng, side, side)
            .into_iter()
            .map(|(x, y)| (x.cast_signed(), y.cast_signed()))
            .collect();

        chart.grow_all(&mut zones, &order, &mut rng);

        assert_eq!(chart.filled, (side * side).cast_signed());
        assert!(chart.cells().iter().all(|c| *c != NONE));
        assert!(chart.cells().contains(&ZONE));
        assert!(chart.cells().contains(&OTHER));
    }

    /// A chart with no seed and no record has nothing to grow from; the loop
    /// still ends.
    #[test]
    fn growing_an_unseeded_chart_ends() {
        let (done, ended) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let mut chart = Chart::new(4);
            let mut zones = Distribution::new(vec![(ZONE, 1)]);

            chart.grow_all(&mut zones, &[], &mut Rng::new(1));
            done.send(chart.filled).ok();
        });

        assert_eq!(
            ended.recv_timeout(std::time::Duration::from_secs(5)),
            Ok(0),
            "growing a chart with nothing planted never ends"
        );
    }
}
