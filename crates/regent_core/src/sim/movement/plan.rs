//! Where a walker goes next: who stands on which half cell, the flood
//! that picks a free cell, and the planner. **Binary** unless marked.
//!
//! ```text
//!  OrderMovePath, when the path is spent
//!    start   nearest free cell to its own        the nearest free cell  ─┐
//!    goal    beside the target: nearest free     the cell beside it   ├ flood
//!            else the nearest free to a point    the nearest free cell  ─┘
//!    plan    A*, 8 neighbours, g + h
//!            g: 1 a side step, 2 a diagonal; h: squared distance
//!            at most 1024 pops; the node nearest the goal wins
//!    keep    the first 10 nodes
//! ```

use crate::base::digest::Set;

use super::super::space::HalfRect;
use crate::sim::chunks::Chunks;
use crate::sim::placement::Edge;

/// A half cell.
pub type Cell = (i32, i32);

/// Occupants a half cell lists at most: a ninth blocks.
pub const OCCUPANTS: usize = 8;

/// The two collision layers of a descriptor's `Info` word:
/// `BlockGround` and `BlockFlying`. Units on no shared layer ignore each
/// other.
pub const LAYERS: u32 = 0x30;

///  bit a ruin carries: it blocks nothing.
pub const RUINED: u32 = 0x10;

/// Who stands on a half cell, as blocker default weighs it.
#[allow(
    clippy::struct_excessive_bools,
    reason = "each a test the engine makes"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Occupant {
    /// Its object handle.
    pub handle: u32,
    /// The descriptor's `Info` word.
    pub info: u32,
    /// Its state word.
    pub state: u32,
    /// A vehicle.
    pub vehicle: bool,
    /// A building, not a `BBt` chest.
    pub building: bool,
    /// A `BBt` chest: a building to `GenUnit` (`GenUnit`, unit
    /// kind 0), which path mark avoid asks.
    pub chest: bool,
    /// `GSUnit` lets it be cleared: scatter, a ruin.
    pub clearable: bool,
    /// As a walker: its class has flag `0x10`, so its paths carry info
    /// `0x1000` and it walks through what is clearable.
    pub tramples: bool,
    /// Its descriptor's `Info` `DontBlock`: a place search leaves it out
    /// whatever its state (`World::occupied`); the planner does not
    /// read it.
    pub dont_block: bool,
}

/// What a test counts as in the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocking {
    /// Info bit `0x200`: what stands still; planning.
    Still,
    /// Info bit `0x400`: anyone; stepping and the goal.
    Any,
    /// Info bit `0x100`: buildings only; `PathCost`'s default.
    Buildings,
}

impl Occupant {
    /// Whether this occupant stops `walker`.
    #[must_use]
    pub const fn blocks(&self, walker: &Occupant, how: Blocking) -> bool {
        let shared = self.info & walker.info & LAYERS != 0;

        // Blocker default: info 0x1000 lets a clearable one through.
        self.handle != walker.handle
            && shared
            && !(walker.tramples && self.clearable)
            && self.state & RUINED == 0
            && match how {
                Blocking::Any => true,
                Blocking::Still => !self.vehicle,
                Blocking::Buildings => self.building,
            }
    }
}

/// The collision grid, `map`: each half cell's occupants.
///
/// A unit registers the covered half cells of its pad mask, else of its
/// pad rect, about its half cell; not while held or with
///  bit `0x20`, its descriptor's `DontBlock` (`crate::sim::world::piece_state::DONT_BLOCK`).
///
/// Only a cell's count and who is on it are read, never their order: a
/// cell is a multiset.
#[derive(Clone, Debug, Default)]
pub struct Grid {
    /// Half cells a side.
    pub side: i32,
    /// Each half cell's last occupant, row by row: an index into `lists`,
    /// or [`END`]. Dense: a lookup is an index, not a hash. Both lists in
    /// shared chunks: a copy of the world copies only what changes after.
    heads: Chunks<u32>,
    /// Occupants, each with the index of the one before it on its cell.
    lists: Chunks<(Occupant, u32)>,
    /// Slots of `lists` [`Grid::leave`] freed, for the next to stand.
    free: Vec<u32>,
}

/// No occupant: the end of a half cell's list.
const END: u32 = u32::MAX;

impl Grid {
    /// An empty grid `side` half cells a side.
    #[must_use]
    pub fn new(side: i32) -> Grid {
        let side_len = usize::try_from(side).unwrap_or(0);

        Grid {
            side,
            heads: Chunks::filled(END, side_len * side_len),
            lists: Chunks::default(),
            free: Vec::new(),
        }
    }

    /// Register `who` on `cells`. A cell off the map is dropped: only
    /// cells on it are ever asked about.
    ///
    /// # Panics
    ///
    /// Past `u32::MAX` registrations.
    pub fn stand(&mut self, who: Occupant, cells: impl IntoIterator<Item = Cell>) {
        for c in cells {
            let Some(i) = self.index(c) else { continue };
            let entry = (who, *self.heads.get(i));

            *self.heads.get_mut(i) = if let Some(slot) = self.free.pop() {
                *self.lists.get_mut(slot as usize) = entry;
                slot
            } else {
                self.lists.push(entry);
                u32::try_from(self.lists.len() - 1).expect("under 4G occupants")
            };
        }
    }

    /// Take one occupant equal to `who` off each of `cells`, as
    /// [`Grid::stand`] put it there. Equal occupants are alike to every
    /// test, so which of them goes does not matter.
    pub fn leave(&mut self, who: &Occupant, cells: impl IntoIterator<Item = Cell>) {
        for c in cells {
            let Some(i) = self.index(c) else { continue };

            // Walk the cell's list to `who`, keeping the slot before it,
            // and unlink it.
            let mut slot = *self.heads.get(i);
            let mut before = None;

            while slot != END && self.lists.get(slot as usize).0 != *who {
                before = Some(slot);
                slot = self.lists.get(slot as usize).1;
            }

            if slot != END {
                let after = self.lists.get(slot as usize).1;

                match before {
                    Some(b) => self.lists.get_mut(b as usize).1 = after,
                    None => *self.heads.get_mut(i) = after,
                }
                self.free.push(slot);
            }
        }
    }

    /// The first half cell, row by row, whose occupants differ from
    /// `other`'s, with each one's, sorted. `None` where all are alike.
    #[must_use]
    pub fn differs(&self, other: &Grid) -> Option<(Cell, Vec<Occupant>, Vec<Occupant>)> {
        // One pair of buffers for the whole map, not a pair per cell.
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        let sorted = |grid: &Grid, c: Cell, out: &mut Vec<Occupant>| {
            out.clear();
            out.extend(
                grid.index(c)
                    .into_iter()
                    .flat_map(|i| grid.occupants(i).copied()),
            );
            out.sort_unstable();
        };
        let side = self.side.max(other.side);

        (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .find(|&c| {
                sorted(self, c, &mut mine);
                sorted(other, c, &mut theirs);
                mine != theirs
            })
            .map(|c| (c, mine, theirs))
    }

    /// The occupants of half cell `c`; none off the map.
    pub fn at(&self, c: Cell) -> impl Iterator<Item = &Occupant> {
        self.index(c).into_iter().flat_map(|i| self.occupants(i))
    }

    /// A half cell's slot in `heads`, if on the map.
    fn index(&self, c: Cell) -> Option<usize> {
        self.on_map(c)
            .then(|| usize::try_from(c.1 * self.side + c.0).ok())
            .flatten()
    }

    /// The occupants of slot `i`, last registered first.
    fn occupants(&self, i: usize) -> impl Iterator<Item = &Occupant> {
        let live = |n: u32| (n != END).then_some(n);

        std::iter::successors(live(*self.heads.get(i)), move |&n| {
            live(self.lists.get(n as usize).1)
        })
        .map(|n| &self.lists.get(n as usize).0)
    }

    /// Whether a half cell lies on the map.
    #[must_use]
    pub const fn on_map(&self, c: Cell) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.0 < self.side && c.1 < self.side
    }

    /// Half cell blocked: someone there blocks, or the list is full.
    #[must_use]
    pub fn blocked(&self, c: Cell, walker: &Occupant, how: Blocking) -> bool {
        self.index(c).is_some_and(|i| {
            // One walk of the list: whether anyone blocks, else its count.
            let mut count = 0;

            self.occupants(i).any(|o| {
                count += 1;
                o.blocks(walker, how)
            }) || count >= OCCUPANTS
        })
    }

    /// Footprint blocked: off the map, `shape`'s
    /// half cells about `at` off it as `edge` counts, or any of them
    /// blocked.
    #[must_use]
    pub fn fits(
        &self,
        shape: &[Cell],
        edge: Edge,
        at: Cell,
        walker: &Occupant,
        how: Blocking,
    ) -> bool {
        let past = match edge {
            Edge::Mask => 0,
            Edge::Rect => 1,
        };

        self.on_map(at)
            && shape.iter().all(|d| {
                let c = (at.0 + d.0, at.1 + d.1);

                self.on_map(c)
                    && self.on_map((c.0 + past, c.1 + past))
                    && !self.blocked(c, walker, how)
            })
    }
}

// ---------------------------------------------------------------------------
// Avoid modes

/// `Move`'s avoid modes in a `MovePath`'s info, and its radius:
/// the `Move`'s distance over 16, then over 16 again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Avoid {
    /// Info bits [`Avoid::VEHICLES`] and [`Avoid::BUILDINGS`].
    pub modes: u32,
    pub radius: i32,
}

impl Avoid {
    /// `avoid_vehicles`: squares round mobiles.
    pub const VEHICLES: u32 = 0x4;
    /// `avoid_buildings`: squares round buildings.
    pub const BUILDINGS: u32 = 0x8;
    /// Half cells about the start whose occupants are weighed: `-0x14`
    /// to `0x15`, half open.
    pub const REACH: i32 = 20;

    /// The avoid modes of `info`, with `radius`.
    #[must_use]
    pub const fn of(info: u32, radius: i32) -> Avoid {
        Avoid {
            modes: info & (Avoid::VEHICLES | Avoid::BUILDINGS),
            radius,
        }
    }

    /// Whether path mark avoid marks anything: a mode and a
    /// nonzero radius.
    #[must_use]
    pub const fn marks(&self) -> bool {
        self.modes != 0 && self.radius != 0
    }

    /// Whether path plan run plans again with no
    /// radius when a plan keeps under two nodes: a radius above 0 (the
    /// size, its other test, is 0 in HD).
    #[must_use]
    pub const fn replans(&self) -> bool {
        self.radius > 0
    }

    /// The half cells path mark avoid marks `-16` from `start`:
    /// each occupant within [`Avoid::REACH`] that blocks `walker` counting
    /// `how` and is a building (under [`Avoid::BUILDINGS`]) or a mobile
    /// (under [`Avoid::VEHICLES`]) marks a square about its half cell,
    /// half side the radius over 16 toward zero, clipped to the map.
    /// Counting what stands
    /// still, no mobile blocks, so `avoid_vehicles` marks only once a
    /// plan counts anyone ([`Blocking::Any`]).
    #[must_use]
    pub fn marked(&self, grid: &Grid, start: Cell, walker: &Occupant, how: Blocking) -> Set<Cell> {
        let mut marked = Set::default();

        if self.marks() {
            let half = self.radius / 16;
            let span = |c: i32, d: i32| (c - d).max(0)..(c + d + 1).min(grid.side);
            let wanted = |o: &Occupant| {
                (self.modes & Avoid::BUILDINGS != 0 && (o.building || o.chest))
                    || (self.modes & Avoid::VEHICLES != 0 && o.vehicle)
            };

            for x in span(start.0, Avoid::REACH) {
                for y in span(start.1, Avoid::REACH) {
                    // The engine reads at most eight occupants a half cell.
                    let hit = grid
                        .at((x, y))
                        .take(OCCUPANTS)
                        .any(|o| o.blocks(walker, how) && wanted(o));

                    if hit {
                        marked.extend(
                            span(x, half).flat_map(|mx| span(y, half).map(move |my| (mx, my))),
                        );
                    }
                }
            }
        }

        marked
    }
}

/// The half cells of a rect, row by row.
#[must_use]
pub fn rect_cells(r: HalfRect) -> Vec<Cell> {
    (r[1]..r[3])
        .flat_map(|y| (r[0]..r[2]).map(move |x| (x, y)))
        .collect()
}

// ---------------------------------------------------------------------------
// MSVC 9's heap, over node indices

/// `std::_Push_heap`: heap push hole.
fn push_hole(
    heap: &mut [usize],
    mut hole: usize,
    top: usize,
    val: usize,
    greater: &dyn Fn(usize, usize) -> bool,
) {
    while top < hole {
        let parent = (hole - 1) / 2;

        if !greater(heap[parent], val) {
            break;
        }

        heap[hole] = heap[parent];
        hole = parent;
    }

    heap[hole] = val;
}

/// `std::_Adjust_heap`: heap adjust.
fn adjust(
    heap: &mut [usize],
    mut hole: usize,
    bottom: usize,
    val: usize,
    greater: &dyn Fn(usize, usize) -> bool,
) {
    let top = hole;
    let mut idx = 2 * hole + 2;

    while idx < bottom {
        if greater(heap[idx], heap[idx - 1]) {
            idx -= 1;
        }

        heap[hole] = heap[idx];
        hole = idx;
        idx = 2 * idx + 2;
    }

    if idx == bottom {
        heap[hole] = heap[bottom - 1];
        hole = bottom - 1;
    }

    push_hole(heap, hole, top, val, greater);
}

/// `push_back` then `std::push_heap`.
fn push(heap: &mut Vec<usize>, val: usize, greater: &dyn Fn(usize, usize) -> bool) {
    heap.push(val);

    let hole = heap.len() - 1;

    push_hole(heap, hole, 0, val, greater);
}

/// `std::pop_heap` then `pop_back`.
fn pop(heap: &mut Vec<usize>, greater: &dyn Fn(usize, usize) -> bool) -> Option<usize> {
    let first = *heap.first()?;
    let last = heap.len() - 1;

    if last > 0 {
        let val = heap[last];

        heap[last] = first;
        adjust(heap, 0, last, val, greater);
    }

    heap.pop()
}

/// `std::make_heap`, after an erase: path open erase.
fn make(heap: &mut [usize], greater: &dyn Fn(usize, usize) -> bool) {
    let n = heap.len();

    for hole in (0..n / 2).rev() {
        let val = heap[hole];

        adjust(heap, hole, n, val, greater);
    }
}

// ---------------------------------------------------------------------------
// The flood

/// What the flood does with a cell it pops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visit {
    /// The answer.
    Take,
    /// Not it; push its neighbours.
    Expand,
    /// Not it, and not through it.
    Skip,
}

/// The neighbours a flood pushes, in order:
/// x outer, y inner.
pub const FLOOD_ORDER: [Cell; 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

/// Best-first flood from `seed`: pops the least `rank` (first key), then
/// the least squared distance from `ask`; the seed first. Each cell is
/// pushed once. `None` when it runs dry.
///
/// **Binary**: place search run, comparator
/// place search less; beside free cell for the cell beside a target.
pub fn flood(
    grid: (i32, i32),
    seed: Cell,
    ask: Cell,
    rank: &dyn Fn(Cell) -> i32,
    visit: &mut dyn FnMut(Cell) -> Visit,
) -> Option<Cell> {
    let d2 = |a: Cell, b: Cell| (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2);
    let mut keys: Vec<(i32, i32, Cell)> = vec![(0, d2(ask, seed), seed)];
    // Each cell pushed, by index (a node of 0 marks it).
    let mut pushed = Marks::new(seed);

    pushed.insert(seed, 0);

    let mut heap: Vec<usize> = vec![0];

    loop {
        let greater = {
            let keys = &keys;

            move |a: usize, b: usize| (keys[a].0, keys[a].1) > (keys[b].0, keys[b].1)
        };
        let at = keys[pop(&mut heap, &greater)?].2;

        match visit(at) {
            Visit::Take => return Some(at),
            Visit::Skip => {}
            Visit::Expand => {
                for d in FLOOD_ORDER {
                    let n = (at.0 + d.0, at.1 + d.1);

                    if n.0 < 0
                        || n.1 < 0
                        || n.0 >= grid.0
                        || n.1 >= grid.1
                        || pushed.get(n).is_some()
                    {
                        continue;
                    }

                    pushed.insert(n, 0);

                    keys.push((rank(n), d2(ask, n), n));

                    let greater = {
                        let keys = &keys;

                        move |a: usize, b: usize| (keys[a].0, keys[a].1) > (keys[b].0, keys[b].1)
                    };

                    push(&mut heap, keys.len() - 1, &greater);
                }
            }
        }
    }
}

/// Place search run's answer when its heap runs dry: `-1, -1`.
pub const NOWHERE: Cell = (-1, -1);

/// What the aim ranks a cell by first.
pub enum Rank<'r> {
    /// Its squared distance from the goal: a path to a point.
    Goal,
    /// Its pad-rect gap to the followed unit: a path
    /// `OrderFollowUnit` pushed, the unit found.
    Gap(&'r dyn Fn(Cell) -> i32),
}

/// Where path search aims: a flood seeded at the
/// order's goal, asking from `start` and counting anyone (flags `0x400`,
/// `0x400` in path search). Place search push ranks a cell
/// by `rank`, then by its squared distance from `start`: a walker on the
/// goal moves the aim to the free cell nearest the goal, the nearer the
/// walker on a tie. `None` when the flood runs dry.
pub fn aim(
    grid: (i32, i32),
    goal: Cell,
    start: Cell,
    rank: &Rank<'_>,
    free: &dyn Fn(Cell) -> bool,
) -> Option<Cell> {
    let d2 = |c: Cell| (goal.0 - c.0).pow(2) + (goal.1 - c.1).pow(2);
    let first: &dyn Fn(Cell) -> i32 = match rank {
        Rank::Goal => &d2,
        Rank::Gap(gap) => gap,
    };

    flood(grid, goal, start, first, &mut |c| {
        if free(c) { Visit::Take } else { Visit::Expand }
    })
}

// ---------------------------------------------------------------------------
// The planner

/// Where a planner node stands in its search: `node`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Open,
    Closed,
}

/// A planner node, `0x1c` bytes: f, g, h, x, y, parent, state.
#[derive(Clone, Copy, Debug)]
struct Node {
    f: i32,
    g: i32,
    h: i32,
    at: Cell,
    parent: Option<usize>,
    state: State,
}

/// No node on a cell of [`Marks`].
const UNMARKED: u32 = u32::MAX;

/// Half cells a side of [`Marks`]' first window: most plans stay within.
const MARKS_SIDE: i32 = 16;

/// Each cell's planner node (or a flood's pushed cells), in a window of cells about the start that
/// widens as the search leaves it: a lookup is an index, not a hash.
///
/// ```text
///  cell beyond the right edge: the width doubles to the right
///  +--------+        +--------+--------+
///  |  start |   ->   |  start |        |
///  +--------+        +--------+--------+
/// ```
struct Marks {
    /// The window's first cell.
    origin: Cell,
    /// Its width and height.
    size: (i32, i32),
    /// Node indices row by row, [`UNMARKED`] where none.
    slots: Vec<u32>,
}

impl Marks {
    /// An empty window about `start`.
    fn new(start: Cell) -> Marks {
        let half = MARKS_SIDE / 2;

        Marks {
            origin: (start.0 - half, start.1 - half),
            size: (MARKS_SIDE, MARKS_SIDE),
            slots: vec![UNMARKED; (MARKS_SIDE * MARKS_SIDE) as usize],
        }
    }

    /// Cell `c`'s slot, if in the window.
    fn slot(&self, c: Cell) -> Option<usize> {
        let (x, y) = (c.0 - self.origin.0, c.1 - self.origin.1);
        let inside = (0..self.size.0).contains(&x) && (0..self.size.1).contains(&y);

        inside
            .then(|| usize::try_from(y * self.size.0 + x).ok())
            .flatten()
    }

    /// The node on `c`.
    fn get(&self, c: Cell) -> Option<usize> {
        self.slot(c)
            .map(|i| self.slots[i])
            .filter(|&m| m != UNMARKED)
            .map(|m| m as usize)
    }

    /// Node `m` on `c`, the window widened to take it.
    fn insert(&mut self, c: Cell, m: usize) {
        while self.slot(c).is_none() {
            self.widen(c);
        }

        let i = self.slot(c).expect("widened to take it");

        self.slots[i] = u32::try_from(m).expect("under 4G nodes");
    }

    /// The window doubled across each side `c` lies beyond.
    fn widen(&mut self, c: Cell) {
        let grow = |at: i32, origin: i32, size: i32| {
            if at < origin {
                (origin - size, size * 2)
            } else if at >= origin + size {
                (origin, size * 2)
            } else {
                (origin, size)
            }
        };
        let (x0, w) = grow(c.0, self.origin.0, self.size.0);
        let (y0, h) = grow(c.1, self.origin.1, self.size.1);
        let mut wide = Marks {
            origin: (x0, y0),
            size: (w, h),
            slots: vec![UNMARKED; usize::try_from(w * h).unwrap_or(0)],
        };

        // Each old row into its place in the new window.
        let width = usize::try_from(self.size.0).unwrap_or(0);

        for y in 0..self.size.1 {
            let row = (self.origin.0, self.origin.1 + y);

            if let (Some(from), Some(to)) = (self.slot(row), wide.slot(row)) {
                wide.slots[to..to + width].copy_from_slice(&self.slots[from..from + width]);
            }
        }

        *self = wide;
    }
}

/// A plan: the kept nodes from the start, and how many the whole path
/// had.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub nodes: Vec<Cell>,
    pub whole: usize,
}

/// A* from `start` toward `goal` over half cells `passable` lets
/// through; keeps the first `keep` nodes of the path to the node nearest
/// the goal. `horizon` bounds the pops.
///
/// **Binary**: path search. Neighbours `dx` outer, `dy` inner, each
/// tested before it is weighed; a node reached cheaper is
/// taken out of the open heap, which is rebuilt, or out of the closed
/// list; the heap orders by f alone, so ties
/// fall as MSVC's heap leaves them. The closed list is not kept: nothing
/// here reads it, a node's state says it is closed.
///
/// `passable` answers by the cell alone: a cell with a node other than
/// the start's passed when the node was made, so it is not asked again.
pub fn plan(
    start: Cell,
    goal: Cell,
    keep: usize,
    horizon: usize,
    passable: &mut dyn FnMut(Cell) -> bool,
) -> Plan {
    let d2 = |a: Cell| (goal.0 - a.0).pow(2) + (goal.1 - a.1).pow(2);
    let h0 = d2(start);
    let mut nodes = vec![Node {
        f: h0,
        g: 0,
        h: h0,
        at: start,
        parent: None,
        state: State::Open,
    }];
    let mut mark = Marks::new(start);

    mark.insert(start, 0);

    let mut open: Vec<usize> = vec![0];
    let mut best = 0;

    for _ in 0..horizon {
        let greater = {
            let nodes = &nodes;

            move |a: usize, b: usize| nodes[a].f > nodes[b].f
        };
        let Some(n) = pop(&mut open, &greater) else {
            break;
        };

        nodes[n].state = State::Closed;

        if nodes[n].h < nodes[best].h {
            best = n;
        }

        if nodes[n].at == goal {
            break;
        }

        let at = nodes[n].at;

        for dx in -1..=1 {
            for dy in -1..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }

                let c = (at.0 + dx, at.1 + dy);
                let marked = mark.get(c);

                if marked.is_none_or(|m| m == 0) && !passable(c) {
                    continue;
                }

                let g = nodes[n].g + dx * dx + dy * dy;
                let m = match marked {
                    Some(m) if nodes[m].g <= g => continue,
                    Some(m) => {
                        if nodes[m].state == State::Open {
                            open.retain(|&o| o != m);

                            let greater = {
                                let nodes = &nodes;

                                move |a: usize, b: usize| nodes[a].f > nodes[b].f
                            };

                            make(&mut open, &greater);
                        }

                        m
                    }
                    None => {
                        nodes.push(Node {
                            f: 0,
                            g: 0,
                            h: d2(c),
                            at: c,
                            parent: None,
                            state: State::Open,
                        });
                        mark.insert(c, nodes.len() - 1);
                        nodes.len() - 1
                    }
                };

                nodes[m].parent = Some(n);
                nodes[m].g = g;
                nodes[m].f = nodes[m].h + g;
                nodes[m].state = State::Open;

                let greater = {
                    let nodes = &nodes;

                    move |a: usize, b: usize| nodes[a].f > nodes[b].f
                };

                push(&mut open, m, &greater);
            }
        }
    }

    let mut path = vec![];
    let mut at = Some(best);

    while let Some(i) = at {
        path.push(nodes[i].at);
        at = nodes[i].parent;
    }

    path.reverse();

    let whole = path.len();

    path.truncate(keep);

    Plan { nodes: path, whole }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::space;

    #[test]
    fn an_open_field_is_crossed_in_one_step_a_column() {
        let p = plan((0, 0), (3, 1), 10, 1024, &mut |_| true);

        assert_eq!(p.whole, 4);
        assert_eq!(p.nodes.last(), Some(&(3, 1)));
    }

    /// A plan past [`Marks`]' first window, left and down, widens it.
    #[test]
    fn a_plan_beyond_the_first_window_widens_it() {
        let far = (-3 * MARKS_SIDE, 2 * MARKS_SIDE);
        let p = plan((0, 0), far, usize::MAX, 1024, &mut |_| true);

        assert_eq!(p.whole, 3 * MARKS_SIDE as usize + 1);
        assert_eq!(p.nodes.last(), Some(&far));
    }

    #[test]
    fn a_wall_is_walked_round() {
        let wall = |c: Cell| !(c.0 == 1 && (-2..=2).contains(&c.1));
        let p = plan((0, 0), (2, 0), 10, 1024, &mut { wall });

        assert_eq!(p.nodes.first(), Some(&(0, 0)));
        assert_eq!(p.nodes.last(), Some(&(2, 0)));
        assert!(p.nodes.iter().all(|c| wall(*c)));
    }

    /// A walker whose class has flag `0x10` plans and steps with info
    /// `0x1000`, and blocker default lets it through
    /// an occupant `GSUnit` calls clearable, scatter and ruins: a
    /// yeti walks through trees. Anyone else on its layer still blocks.
    #[test]
    fn a_trampler_walks_through_scatter() {
        let mut grid = Grid::new(16);
        let tree = Occupant {
            handle: 0,
            info: 0x10,
            state: 0,
            vehicle: false,
            building: false,
            chest: false,
            clearable: true,
            tramples: false,
            dont_block: false,
        };
        let rock = Occupant {
            clearable: false,
            handle: 9,
            ..tree
        };

        grid.stand(tree, [(4, 4)]);
        grid.stand(rock, [(8, 8)]);

        let walker = |tramples| Occupant {
            handle: 1,
            info: 0x10,
            state: 0,
            vehicle: true,
            building: false,
            chest: false,
            clearable: false,
            tramples,
            dont_block: false,
        };
        let fits = |tramples, at| {
            grid.fits(
                &[(0, 0)],
                Edge::Mask,
                at,
                &walker(tramples),
                Blocking::Still,
            )
        };

        assert_eq!(
            [fits(true, (4, 4)), fits(false, (4, 4)), fits(true, (8, 8))],
            [true, false, false]
        );
    }

    /// **Binary**, path mark avoid: a radius of 32
    /// marks a square of half side 2 about each building or mobile within
    /// 20 half cells of the start, clipped to the map. A mobile blocks only
    /// a plan counting anyone (blocker default under `0x200`); no
    /// radius, or a building out of reach, marks nothing.
    #[test]
    fn an_avoid_mode_marks_a_square_about_each_unit_in_reach() {
        let mut grid = Grid::new(64);
        let hut = Occupant {
            handle: 7,
            info: LAYERS,
            state: 0,
            vehicle: false,
            building: true,
            chest: false,
            clearable: false,
            tramples: false,
            dont_block: false,
        };
        let cart = Occupant {
            handle: 8,
            vehicle: true,
            building: false,
            ..hut
        };
        let me = Occupant { handle: 1, ..cart };
        let on_map = |c: &Cell| (0..64).contains(&c.0) && (0..64).contains(&c.1);
        let square = |at: Cell, half: i32| -> Set<Cell> {
            (at.0 - half..=at.0 + half)
                .flat_map(|x| (at.1 - half..=at.1 + half).map(move |y| (x, y)))
                .filter(on_map)
                .collect()
        };

        grid.stand(hut, [(1, 10)]);
        grid.stand(cart, [(30, 10)]);
        grid.stand(hut, [(50, 10)]);

        let marked = |modes, radius, how| Avoid { modes, radius }.marked(&grid, (21, 10), &me, how);

        assert_eq!(
            marked(Avoid::BUILDINGS, 32, Blocking::Still),
            square((1, 10), 2)
        );
        assert!(marked(Avoid::VEHICLES, 32, Blocking::Still).is_empty());
        assert_eq!(
            marked(Avoid::VEHICLES, 32, Blocking::Any),
            square((30, 10), 2)
        );
        assert_eq!(
            marked(Avoid::BUILDINGS, 15, Blocking::Still),
            square((1, 10), 0)
        );
        assert!(marked(Avoid::BUILDINGS, 0, Blocking::Still).is_empty());
        assert!(marked(0, 32, Blocking::Any).is_empty());
    }

    /// A unit that leaves the grid leaves it as if it never stood there,
    /// whatever stood after it; a twin's entry stays.
    #[test]
    fn a_unit_left_is_a_unit_never_stood() {
        let tree = Occupant {
            handle: 0,
            info: 0x10,
            state: 0,
            vehicle: false,
            building: false,
            chest: false,
            clearable: true,
            tramples: false,
            dont_block: false,
        };
        let hut = Occupant {
            handle: 7,
            clearable: false,
            ..tree
        };
        let mut kept = Grid::new(16);
        let mut whole = Grid::new(16);

        kept.stand(tree, [(1, 1), (2, 1)]);
        kept.stand(hut, [(2, 1), (3, 1)]);
        kept.stand(tree, [(2, 1)]);
        kept.leave(&hut, [(2, 1), (3, 1)]);
        kept.leave(&tree, [(2, 1)]);
        kept.stand(hut, [(5, 5)]);
        whole.stand(tree, [(1, 1), (2, 1)]);
        whole.stand(hut, [(5, 5)]);

        assert_eq!(kept.differs(&whole), None);
        assert_eq!(
            kept.differs(&Grid::new(16)),
            Some(((1, 1), vec![tree], vec![]))
        );
    }

    /// A champion's pad rect, `x` -2 to 1, `y` -3 to 1, on a 16-cell map:
    /// it fits nowhere its rect reaches off the map, nor where its
    /// half-open end reaches the map's size; a mask may reach the last
    /// column.
    #[test]
    fn a_pad_rect_off_the_map_does_not_fit() {
        let grid = Grid::new(16);
        let me = Occupant {
            handle: 1,
            info: LAYERS,
            state: 0,
            vehicle: true,
            building: false,
            chest: false,
            clearable: false,
            tramples: false,
            dont_block: false,
        };
        let shape: Vec<Cell> = (-3..=1)
            .flat_map(|y| (-2..=1).map(move |x| (x, y)))
            .collect();
        let fits = |edge, at: Cell| grid.fits(&shape, edge, at, &me, Blocking::Still);

        assert_eq!(
            [
                fits(Edge::Rect, (8, 3)),
                fits(Edge::Rect, (8, 2)),
                fits(Edge::Rect, (13, 8)),
                fits(Edge::Rect, (14, 8)),
                fits(Edge::Mask, (14, 8)),
            ],
            [true, false, true, false, true]
        );
    }

    /// The goal and the cells about it hold a walker: the aim is the
    /// free cell nearest the goal, of those the one nearest the start.
    #[test]
    fn a_walker_on_the_goal_moves_the_aim_to_the_nearest_free_cell() {
        let taken = |c: Cell| (1..=3).contains(&c.0) && (1..=3).contains(&c.1);

        let at = |goal, start| aim((16, 16), goal, start, &Rank::Goal, &|c| !taken(c));

        assert_eq!(at((2, 2), (5, 2)), Some((4, 2)));
        assert_eq!(at((2, 2), (2, 5)), Some((2, 4)));
        assert_eq!(at((6, 6), (4, 4)), Some((6, 6)));
    }

    /// A path following a unit ranks by the gap to it, then by the start:
    /// the dwarves game's skeleton 53 at (5, 231), its hemmed-in peasant on
    /// (11, 236), aims at (7, 233), not (7, 236) nearer her, at 391,150 ms.
    #[test]
    fn a_followed_units_aim_ranks_by_the_gap_to_it() {
        let skeleton: HalfRect = [-1, -1, 1, 2];
        let peasant: HalfRect = [10, 235, 12, 237];
        let gap = |c: Cell| space::gap2(space::placed(skeleton, c), peasant);
        let free = |c: Cell| c == (7, 233) || c == (7, 236);
        let at = |rank| aim((64, 256), (11, 236), (5, 231), &rank, &free);

        assert_eq!(at(Rank::Goal), Some((7, 236)));
        assert_eq!(at(Rank::Gap(&gap)), Some((7, 233)));
    }

    #[test]
    fn the_heap_pops_in_order() {
        let keys = [5, 3, 9, 1, 7, 3];
        let greater = |a: usize, b: usize| keys[a] > keys[b];
        let mut heap = vec![];

        for i in 0..keys.len() {
            push(&mut heap, i, &greater);
        }

        let order: Vec<i32> = std::iter::from_fn(|| pop(&mut heap, &greater))
            .map(|i| keys[i])
            .collect();

        assert_eq!(order, [1, 3, 3, 5, 7, 9]);
    }
}
